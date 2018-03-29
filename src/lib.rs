// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Interface to the [Slurm](https://slurm.schedmd.com/) workload manager.

Slurm is a system for scheduling and running jobs on large computing clusters.
It is often used in scientific HPC (high-performance computing) contexts.

This crate provides hooks for submitting new jobs and interrogating their
status. Support for other kinds of operations, such as canceling jobs or
altering their runtime parameters, would be entirely appropriate but has not
yet been implemented.

# Example: querying a running job

```no_run
extern crate failure;
extern crate slurm;

fn print_random_job_information(jobid: slurm::JobId) -> Result<(), failure::Error> {
    let info = slurm::get_job_info(jobid)?;
    println!("Job ID: {}", info.job_id()); // same as what we put in
    println!("Job's partition: {}", info.partition());
    Ok(())
}
```

# Example: querying a completed job

To gather information about jobs that have completed, you must connect to the
Slurm accounting database and query it.

```no_run
extern crate chrono;
extern crate failure;
extern crate slurm;

fn print_other_job_information(jobid: slurm::JobId) -> Result<(), failure::Error> {
    let mut filter = slurm::JobFiltersOwned::default();
    filter.step_list_mut().append(slurm::JobStepFilterOwned::new(jobid));

    let db = slurm::DatabaseConnectionOwned::new()?;
    let jobs = db.get_jobs(&filter)?;
    let now = chrono::Utc::now();

    for job in jobs.iter() {
        println!("Job ID {}, name {}", job.job_id(), job.job_name());

        if let Some(d) = job.wait_duration() {
            println!("  job started; wait time: {} s", d.num_seconds());
        } else if let Some(t_el) = job.eligible_time() {
            let wait = now.signed_duration_since(t_el).num_seconds();
            println!("  job not yet started; time since eligibility: {} s", wait);
        } else {
            println!("  job not yet eligible to run");
        }
    }

    Ok(())
}
```

# Submitting a “Hello World” job

```no_run
extern crate failure;
extern crate slurm;

fn submit_hello_world() -> Result<slurm::JobId, failure::Error> {
    let cwd = std::env::current_dir()?;

    let log = {
        let mut p = cwd.clone();
        p.push("%j.log");
        p.to_str().ok_or(failure::err_msg("cannot stringify log path"))?.to_owned()
    };

    let mut desc = slurm::JobDescriptorOwned::new();

    desc.set_name("helloworld")
        .set_argv(&["helloworld"])
        .inherit_environment()
        .set_stderr_path(&log)
        .set_stdin_path("/dev/null")
        .set_stdout_path(&log)
        .set_work_dir_cwd()?
        .set_script("#! /bin/bash \
                     set -e -x \
                     echo hello world \"$@\"")
        .set_gid_current() // JobDescriptor args must come after due to the return type
        .set_num_tasks(1)
        .set_time_limit(5)
        .set_uid_current();

    let msg = desc.submit_batch()?;
    println!("new job id: {}", msg.job_id());
    Ok(msg.job_id())
}
```

# A note on memory management

The Slurm C library uses a (primitive) custom memory allocator for its data
structures. Because we must maintain compatibility with this allocator, we
have to allocate all of our data structures from the heap rather than the
stack. Almost all of the structures exposed here come in both “borrowed” and
“owned” flavors; they are largely equivalent, but only the owned versions free
their data when they go out of scope. Borrowed structures need not be
immutable, but it is not possible to modify them in ways that require freeing
or allocating memory associated with their sub-structures.

*/

extern crate chrono;
#[macro_use] extern crate failure;
#[macro_use] extern crate failure_derive;
extern crate libc;
extern crate slurm_sys;

use chrono::{DateTime, Duration, TimeZone, Utc};
use failure::Error;
use std::borrow::Cow;
use std::default::Default;
use std::ffi::CStr;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::os::raw::{c_char, c_int, c_void};


/// A job identifier number; this will always be `u32`.
pub type JobId = u32;

/// A job-step identifier number; this will always be `u32`.
pub type StepId = u32;


/// A quick macro framework to map low-level slurm API errors to a Rust interface.
macro_rules! declare_slurm_errors {
    ($(<$rustname:ident, $sysname:path, $doc:expr;>),*) => {
        /// Specifically-enumerated errors that we can get from the Slurm API.
        ///
        /// This is not exhaustive; the only specifically implemented options are ones
        /// that are expected to be of interest to general developers.
        #[derive(Copy, Clone, Debug, Eq, Fail, Hash, PartialEq)]
        pub enum SlurmError {
            $(
                #[doc=$doc] $rustname,
            )*

            /// Some other Slurm error.
            Other(c_int),
        }

        impl SlurmError {
            fn from_slurm(errno: c_int) -> SlurmError {
                match errno as u32 {
                    $(
                        $sysname => SlurmError::$rustname,
                    )*
                    _ => SlurmError::Other(errno),
                }
            }

            fn to_slurm(&self) -> c_int {
                match self {
                    $(
                        &SlurmError::$rustname => $sysname as c_int,
                    )*
                    &SlurmError::Other(errno) => errno,
                }
            }
        }
    }
}

declare_slurm_errors!(
    <InvalidJobId, slurm_sys::ESLURM_INVALID_JOB_ID,
     "The job ID did not correspond to a valid job.";>,
    <InvalidPartitionName, slurm_sys::ESLURM_INVALID_PARTITION_NAME,
     "The specified partition name was not recognized.";>
);

impl Display for SlurmError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        let e = self.to_slurm();
        let m = unsafe { CStr::from_ptr(slurm_sys::slurm_strerror(e)) };
        write!(f, "{} (Slurm errno {})", m.to_string_lossy(), e)
    }
}


/// Most Slurm API calls return an zero on success. The library API docs state
/// that the return code on error is -1, and this macro encapsulates the task
/// of obtaining an errno and converting it to a result. However, in at least
/// one case the return code is an errno, which would be a nicer pattern from
/// a thread-safety standpoint.
macro_rules! stry {
    ($op:expr) => {{
        if $op != 0 {
            let e = unsafe { slurm_sys::slurm_get_errno() };
            Err(SlurmError::from_slurm(e))
        } else {
            Ok(())
        }?
    }}
}

/// This is like `stry!` but also wraps the Slurm call in an `unsafe{}` block,
/// since most (all?) of the times we're doing this, we're using the C API.
macro_rules! ustry {
    ($op:expr) => {
        stry!(unsafe { $op })
    }
}

/// This is like `stry!` but for unsafe Slurm calls that return pointers.
macro_rules! pstry {
    ($op:expr) => {{
        let ptr = unsafe { $op };

        if ptr.is_null() {
            let e = unsafe { slurm_sys::slurm_get_errno() };
            Err(SlurmError::from_slurm(e))
        } else {
            Ok(ptr)
        }?
    }}
}


/// Allocate memory using Slurm's allocator.
fn slurm_alloc_array<T>(count: usize) -> *mut T {
    const TEXT: &[u8] = b"slurm-rs\0";
    let ptr = unsafe {
        slurm_sys::slurm_try_xmalloc(std::mem::size_of::<T>() * count, TEXT.as_ptr() as _, 1, TEXT.as_ptr() as _)
    };

    if ptr.is_null() {
        panic!("Slurm memory allocation failed");
    }

    ptr as _
}


/// Allocate a structure using Slurm's allocator.
fn slurm_alloc<T>() -> *mut T {
    slurm_alloc_array(1)
}


/// Allocate a C-style string using Slurm's allocator, encoding it as UTF-8.
fn slurm_alloc_utf8_string<S: AsRef<str>>(s: S) -> *mut c_char {
    let bytes = s.as_ref().as_bytes();
    let n = bytes.len() + 1;
    let ptr = slurm_alloc_array(n);
    let dest = unsafe { std::slice::from_raw_parts_mut(ptr, n) };
    dest[..n-1].copy_from_slice(bytes);
    dest[n-1] = b'\0';
    ptr as _
}


/// Allocate an array of C-style strings using Slurm's allocator.
///
/// The strings are encoded as UTF8. Returns the pointer to the string array
/// and the number of strings allocated, which may not be known by the caller
/// if the argument is an iterator of indeterminate size.
fn slurm_alloc_utf8_string_array<I: IntoIterator<Item = S>, S: AsRef<str>>(strings: I) -> (*mut *mut c_char, usize) {
    let buf: Vec<_> = strings.into_iter().collect();
    let ptr = slurm_alloc_array(buf.len());
    let sl = unsafe { std::slice::from_raw_parts_mut(ptr, buf.len()) };

    for (i, s) in buf.iter().enumerate() {
        sl[i] = slurm_alloc_utf8_string(s.as_ref()) as _;
    }

    (ptr, buf.len())
}


/// Free a structure using Slurm's allocator.
///
/// A mutable reference to the pointer is required; after freeing, the pointer
/// is nullified. This call is a no-op if the input pointer is already null.
fn slurm_free<T>(thing: &mut *mut T) {
    const TEXT: &[u8] = b"slurm-rs\0";
    let p = &mut (*thing as *mut c_void);
    unsafe { slurm_sys::slurm_xfree(p, TEXT.as_ptr() as _, 1, TEXT.as_ptr() as _) };
}


/// Free an array of strings allocated through Slurm's allocator.
///
/// A mutable reference to the pointer is required; after freeing, the pointer
/// is nullified. This call is a no-op if the input pointer is already null.
fn slurm_free_string_array(ptr_ref: &mut *mut *mut c_char, count: usize) {
    if ptr_ref.is_null() {
        return;
    }

    let sl = unsafe { std::slice::from_raw_parts_mut(*ptr_ref, count) };

    for mut sub_ptr in sl {
        slurm_free(&mut sub_ptr);
    }

    slurm_free(ptr_ref);
}


/// A helper trait that lets us generically iterate over lists. It must be
/// public so that we can expose `Iterator` for `SlurmListIteratorOwned`.
pub trait UnownedFromSlurmPointer {
    /// Create an unowned wrapper object from a Slurm pointer.
    fn unowned_from_slurm_pointer(ptr: *mut c_void) -> Self;
}


/// Helper for creating public structs that directly wrap Slurm API
/// structures. Because we must use Slurm's internal allocator, these all wrap
/// native pointers. It's a bit annoying but as far as I can tell it's what we
/// have to do. All of these types are "borrowed" items; they should not
/// implement Drop methods.
macro_rules! make_slurm_wrap_struct {
    ($rust_name:ident, $slurm_name:path, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug)]
        pub struct $rust_name(*mut $slurm_name);

        impl $rust_name {
            /// Access the underlying slurm_sys struct immutably.
            #[allow(unused)]
            #[inline(always)]
            fn sys_data(&self) -> &$slurm_name {
                unsafe { &(*self.0) }
            }

            /// Access the underlying slurm_sys struct mutably.
            #[allow(unused)]
            #[inline(always)]
            fn sys_data_mut(&mut self) -> &mut $slurm_name {
                unsafe { &mut (*self.0) }
            }

            /// Transmute a reference to a pointer to the underlying datatype
            /// into a reference to this wrapper struct. This leverages the
            /// fact that the wrapper type is a unit struct that is basically
            /// just a pointer itself. This function allows us to return
            /// references to fields of various `slurm_sys` structs as if
            /// they were our Rust wrapper types.
            #[allow(unused)]
            #[inline(always)]
            unsafe fn transmute_ptr<'a>(ptr: &'a *mut $slurm_name) -> &'a Self {
                std::mem::transmute(ptr)
            }

            /// Like `transmute_ptr`, but mutable.
            #[allow(unused)]
            #[inline(always)]
            unsafe fn transmute_ptr_mut<'a>(ptr: &'a mut *mut $slurm_name) -> &'a mut Self {
                std::mem::transmute(ptr)
            }
        }

        impl UnownedFromSlurmPointer for $rust_name {
            #[inline(always)]
            fn unowned_from_slurm_pointer(ptr: *mut c_void) -> Self {
                $rust_name(ptr as _)
            }
        }
    }
}

/// Helper for creating "owned" versions of unowned structs. This is super
/// tedious but I think it's what we need to do to correctly interface with
/// Slurm's allocator.
macro_rules! make_owned_version {
    (@customdrop $unowned_type:ident, $owned_name:ident, $doc:expr) => {
        #[doc=$doc]
        #[derive(Debug)]
        pub struct $owned_name($unowned_type);

        impl Deref for $owned_name {
            type Target = $unowned_type;

            fn deref(&self) -> &$unowned_type {
                &self.0
            }
        }

        impl DerefMut for $owned_name {
            fn deref_mut(&mut self) -> &mut $unowned_type {
                &mut self.0
            }
        }

        impl $owned_name {
            /// This function is unsafe because it may not be valid for the
            /// returned value to be filled with zeros. (Slurm is generally
            /// pretty good about all-zeros being OK, though.)
            #[allow(unused)]
            unsafe fn alloc_zeroed() -> Self {
                $owned_name($unowned_type(slurm_alloc()))
            }

            /// This function is unsafe because it can potentially leak memory
            /// if not used correctly.
            #[allow(unused)]
            unsafe fn give_up_ownership(mut self) -> $unowned_type {
                let ptr = (self.0).0;
                (self.0).0 = 0 as _; // ensures that slurm_free() doesn't free the memory
                $unowned_type(ptr)
            }

            /// This function is unsafe because we commit ourselves to freeing
            /// the passed-in pointer, which could potentially be bad if we
            /// don't in fact own it.
            #[allow(unused)]
            unsafe fn assume_ownership(ptr: *mut c_void) -> Self {
                $owned_name($unowned_type(ptr as _))
            }
        }
    };

    ($unowned_type:ident, $owned_name:ident, $doc:expr) => {
        make_owned_version!(@customdrop $unowned_type, $owned_name, $doc);

        impl Drop for $owned_name {
            fn drop(&mut self) {
                slurm_free(&mut (self.0).0);
            }
        }
    };
}


// The slurm list type gets custom implementations because we give it a type
// parameter to allow typed access.

/// A list of some kind of object known to Slurm.
///
/// These lists show up in a variety of places in the Slurm API. As with the
/// other core structures exposed by this crate, this type represents a
/// *borrowed* reference to a list.
#[derive(Debug)]
pub struct SlurmList<T>(slurm_sys::List, PhantomData<T>);

impl<T> SlurmList<T> {
    unsafe fn transmute_ptr<'a>(ptr: &'a slurm_sys::List) -> &'a Self {
        std::mem::transmute(ptr)
    }

    unsafe fn transmute_ptr_mut<'a>(ptr: &'a mut slurm_sys::List) -> &'a mut Self {
        std::mem::transmute(ptr)
    }
}

impl<T: UnownedFromSlurmPointer> SlurmList<T> {
    pub fn iter<'a>(&'a self) -> SlurmListIteratorOwned<'a, T> {
        let ptr = unsafe { slurm_sys::slurm_list_iterator_create(self.0) };

        if ptr.is_null() {
            panic!("failed to create list iterator");
        }

        SlurmListIteratorOwned(ptr as _, PhantomData)
    }
}

/// An owned version of `SlurmList`.
#[derive(Debug)]
pub struct SlurmListOwned<T>(SlurmList<T>);

impl<T> Deref for SlurmListOwned<T> {
    type Target = SlurmList<T>;

    fn deref(&self) -> &SlurmList<T> {
        &self.0
    }
}

impl<T> DerefMut for SlurmListOwned<T> {
    fn deref_mut(&mut self) -> &mut SlurmList<T> {
        &mut self.0
    }
}

impl<T> SlurmListOwned<T> {
    #[allow(unused)]
    unsafe fn give_up_ownership(mut self) -> SlurmList<T> {
        let ptr = (self.0).0;
        (self.0).0 = 0 as _; // ensures that slurm_free() doesn't free the memory
        SlurmList(ptr, PhantomData)
    }

    #[allow(unused)]
    unsafe fn assume_ownership(ptr: *mut c_void) -> Self {
        SlurmListOwned(SlurmList(ptr as _, PhantomData))
    }
}

impl<T> Drop for SlurmListOwned<T> {
    fn drop(&mut self) {
        unsafe { slurm_sys::slurm_list_destroy((self.0).0) };
    }
}


// Likewise for iterating through lists, except the iterators are always owned
#[derive(Debug)]
pub struct SlurmListIteratorOwned<'a, T: 'a + UnownedFromSlurmPointer>(*mut slurm_sys::listIterator, PhantomData<&'a T>);

impl<'a, T: 'a + UnownedFromSlurmPointer> Drop for SlurmListIteratorOwned<'a, T> {
    fn drop(&mut self) {
        unsafe { slurm_sys::slurm_list_iterator_destroy(self.0) };
    }
}

impl<'a, T: 'a + UnownedFromSlurmPointer> Iterator for SlurmListIteratorOwned<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        let ptr = unsafe { slurm_sys::slurm_list_next(self.0) };

        if ptr.is_null() {
            None
        } else {
            Some(T::unowned_from_slurm_pointer(ptr))
        }
    }
}


// Now we can finally start wrapping types that we care about.

make_slurm_wrap_struct!(JobInfo, slurm_sys::job_info, "\
Information about a running job.

The following items in the Slurm API are *not* exposed in these Rust bindings:

```ignore
pub struct job_info {
    pub account: *mut c_char,
    pub admin_comment: *mut c_char,
    pub alloc_node: *mut c_char,
    pub alloc_sid: u32,
    pub array_bitmap: *mut c_void,
    pub array_job_id: u32,
    pub array_task_id: u32,
    pub array_max_tasks: u32,
    pub array_task_str: *mut c_char,
    pub assoc_id: u32,
    pub batch_flag: u16,
    pub batch_host: *mut c_char,
    pub bitflags: u32,
    pub boards_per_node: u16,
    pub burst_buffer: *mut c_char,
    pub burst_buffer_state: *mut c_char,
    pub cluster: *mut c_char,
    pub cluster_features: *mut c_char,
    pub command: *mut c_char,
    pub comment: *mut c_char,
    pub contiguous: u16,
    pub core_spec: u16,
    pub cores_per_socket: u16,
    pub billable_tres: f64,
    pub cpus_per_task: u16,
    pub cpu_freq_min: u32,
    pub cpu_freq_max: u32,
    pub cpu_freq_gov: u32,
    pub deadline: time_t,
    pub delay_boot: u32,
    pub dependency: *mut c_char,
    pub derived_ec: u32,
    pub eligible_time: time_t,
    pub end_time: time_t,
    pub exc_nodes: *mut c_char,
    pub exc_node_inx: *mut i32,
    pub exit_code: u32,
    pub features: *mut c_char,
    pub fed_origin_str: *mut c_char,
    pub fed_siblings_active: u64,
    pub fed_siblings_active_str: *mut c_char,
    pub fed_siblings_viable: u64,
    pub fed_siblings_viable_str: *mut c_char,
    pub gres: *mut c_char,
    pub gres_detail_cnt: u32,
    pub gres_detail_str: *mut *mut c_char,
    pub group_id: u32,
    pub job_resrcs: *mut job_resources_t,
    pub job_state: u32,
    pub last_sched_eval: time_t,
    pub licenses: *mut c_char,
    pub max_cpus: u32,
    pub max_nodes: u32,
    pub mcs_label: *mut c_char,
    pub name: *mut c_char,
    pub network: *mut c_char,
    pub nodes: *mut c_char,
    pub nice: u32,
    pub node_inx: *mut i32,
    pub ntasks_per_core: u16,
    pub ntasks_per_node: u16,
    pub ntasks_per_socket: u16,
    pub ntasks_per_board: u16,
    pub num_cpus: u32,
    pub num_nodes: u32,
    pub num_tasks: u32,
    pub pack_job_id: u32,
    pub pack_job_id_set: *mut c_char,
    pub pack_job_offset: u32,
    pub pn_min_memory: u64,
    pub pn_min_cpus: u16,
    pub pn_min_tmp_disk: u32,
    pub power_flags: u8,
    pub preempt_time: time_t,
    pub pre_sus_time: time_t,
    pub priority: u32,
    pub profile: u32,
    pub qos: *mut c_char,
    pub reboot: u8,
    pub req_nodes: *mut c_char,
    pub req_node_inx: *mut i32,
    pub req_switch: u32,
    pub requeue: u16,
    pub resize_time: time_t,
    pub restart_cnt: u16,
    pub resv_name: *mut c_char,
    pub sched_nodes: *mut c_char,
    pub select_jobinfo: *mut dynamic_plugin_data_t,
    pub shared: u16,
    pub show_flags: u16,
    pub sockets_per_board: u16,
    pub sockets_per_node: u16,
    pub start_time: time_t,
    pub start_protocol_ver: u16,
    pub state_desc: *mut c_char,
    pub state_reason: u16,
    pub std_err: *mut c_char,
    pub std_in: *mut c_char,
    pub std_out: *mut c_char,
    pub submit_time: time_t,
    pub suspend_time: time_t,
    pub time_limit: u32,
    pub time_min: u32,
    pub threads_per_core: u16,
    pub tres_req_str: *mut c_char,
    pub tres_alloc_str: *mut c_char,
    pub user_id: u32,
    pub user_name: *mut c_char,
    pub wait4switch: u32,
    pub wckey: *mut c_char,
    pub work_dir: *mut c_char,
}
```

");

impl JobInfo {
     /// Get this job's ID.
     pub fn job_id(&self) -> JobId {
         self.sys_data().job_id
     }

     /// Get the cluster partition on which this job resides.
     pub fn partition(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().partition) }.to_string_lossy()
     }
}


/// Get information about a single job.
///
/// The job must still be running. If it existed but is no longer running,
/// the result is an error (errno 2017, "invalid job id").
///
/// While the (successful) return value of this function is not a `JobInfo`
/// struct, it is a type that derefs to `JobInfo`, and so can be used like
/// one.
pub fn get_job_info(jid: JobId) -> Result<SingleJobInfoMessageOwned, Error> {
    let mut msg: *mut slurm_sys::job_info_msg_t = 0 as _;

    ustry!(slurm_sys::slurm_load_job(&mut msg, jid, 0));

    let rc = unsafe { (*msg).record_count };
    if rc != 1 {
        return Err(format_err!("expected exactly one info record for job {}; got {} items", jid, rc));
    }

    Ok(unsafe { SingleJobInfoMessageOwned::assume_ownership(msg as _) })
}


make_slurm_wrap_struct!(SingleJobInfoMessage, slurm_sys::job_info_msg_t, "Information about a single job.

This type implements `Deref` to `JobInfo` and so can be essentially be
treated as a `JobInfo`. Due to how the Slurm library manages memory, this
separate type is necessary in some cases.");

impl Deref for SingleJobInfoMessage {
    type Target = JobInfo;

    fn deref(&self) -> &JobInfo {
        unsafe { JobInfo::transmute_ptr(&self.sys_data().job_array) }
    }
}

impl DerefMut for SingleJobInfoMessage {
    fn deref_mut(&mut self) -> &mut JobInfo {
        unsafe { JobInfo::transmute_ptr_mut(&mut self.sys_data_mut().job_array) }
    }
}

make_owned_version!(@customdrop SingleJobInfoMessage, SingleJobInfoMessageOwned,
                    "An owned version of `SingleJobInfoMessage`.");

impl Drop for SingleJobInfoMessageOwned {
    fn drop(&mut self) {
        unsafe { slurm_sys::slurm_free_job_info_msg((self.0).0) };
    }
}


make_slurm_wrap_struct!(DatabaseConnection, c_void, "A connection to the Slurm accounting database.");

impl DatabaseConnection {
    /// Query for information about jobs.
    pub fn get_jobs(&self, filters: &JobFilters) -> Result<SlurmListOwned<JobRecord>, SlurmError> {
        let ptr = pstry!(slurm_sys::slurmdb_jobs_get(self.0, filters.0));
        Ok(unsafe { SlurmListOwned::assume_ownership(ptr as _) })
    }
}


make_owned_version!(@customdrop DatabaseConnection, DatabaseConnectionOwned,
                    "An owned version of `DatabaseConnection`.");

impl DatabaseConnectionOwned {
    /// Connect to the Slurm database.
    pub fn new() -> Result<Self, SlurmError> {
        let ptr = pstry!(slurm_sys::slurmdb_connection_get());
        Ok(unsafe { DatabaseConnectionOwned::assume_ownership(ptr) })
    }
}

impl Drop for DatabaseConnectionOwned {
    fn drop(&mut self) {
        // This function can return error codes, but we're not in a position
        // to do anything about it in the Drop call.
        let _ignored = unsafe { slurm_sys::slurmdb_connection_close(&mut (self.0).0) };
    }
}


make_slurm_wrap_struct!(JobFilters, slurm_sys::slurmdb_job_cond_t, "\
A set of filters for identifying jobs of interest when querying the Slurm
accounting database.

The following items in the Slurm API are *not* exposed in these Rust bindings:

```ignore
pub struct slurmdb_job_cond_t {
    pub acct_list: List,
    pub associd_list: List,
    pub cluster_list: List,
    pub cpus_max: u32,
    pub cpus_min: u32,
    pub duplicates: u16,
    pub exitcode: i32,
    pub format_list: List,
    pub groupid_list: List,
    pub jobname_list: List,
    pub nodes_max: u32,
    pub nodes_min: u32,
    pub partition_list: List,
    pub qos_list: List,
    pub resv_list: List,
    pub resvid_list: List,
    pub state_list: List,
    pub timelimit_max: u32,
    pub timelimit_min: u32,
    pub usage_end: time_t,
    pub usage_start: time_t,
    pub used_nodes: *mut c_char,
    pub userid_list: List,
    pub wckey_list: List,
    pub without_steps: u16,
    pub without_usage_truncation: u16,
}
```

");

impl JobFilters {
    pub fn step_list(&self) -> &SlurmList<JobStepFilter> {
        unsafe { SlurmList::transmute_ptr(&self.sys_data().step_list) }
    }

    pub fn step_list_mut(&mut self) -> &mut SlurmList<JobStepFilter> {
        unsafe { SlurmList::transmute_ptr_mut(&mut self.sys_data_mut().step_list) }
    }
}

make_owned_version!(JobFilters, JobFiltersOwned, "An owned version of `JobFilters`");

impl Default for JobFiltersOwned {
    fn default() -> Self {
        let mut inst = unsafe { Self::alloc_zeroed() };
        {
            let sdm = inst.sys_data_mut();
            sdm.without_usage_truncation = 1;
        }
        inst
    }
}


make_slurm_wrap_struct!(JobStepFilter, slurm_sys::slurmdb_selected_step_t,
                        "A filter for selecting jobs and job steps.");

make_owned_version!(@customdrop JobStepFilter, JobStepFilterOwned, "An owned version of `JobStepFilter`.");

impl Drop for JobStepFilterOwned {
    fn drop(&mut self) {
        unsafe { slurm_sys::slurmdb_destroy_selected_step((self.0).0 as _) };
    }
}

impl JobStepFilterOwned {
    /// Create a new job step filter.
    pub fn new(jid: JobId) -> Self {
        let mut inst = unsafe { Self::alloc_zeroed() };
        {
            let sdm = inst.sys_data_mut();
            sdm.array_task_id = slurm_sys::SLURMRS_NO_VAL;
            sdm.jobid = jid;
            sdm.pack_job_offset = slurm_sys::SLURMRS_NO_VAL;
            sdm.stepid = slurm_sys::SLURMRS_NO_VAL;
        }
        inst
    }
}

impl SlurmList<JobStepFilter> {
    pub fn append(&mut self, item: JobStepFilterOwned) {
        let item = unsafe { item.give_up_ownership() };

        if self.0.is_null() {
            // XXX if malloc fails, I think this function will abort under us.
            self.0 = unsafe { slurm_sys::slurm_list_create(Some(slurm_sys::slurmdb_destroy_selected_step)) };
        }

        unsafe { slurm_sys::slurm_list_append(self.0, item.0 as _); }
    }
}


make_slurm_wrap_struct!(JobRecord, slurm_sys::slurmdb_job_rec_t, "\
Accounting information about a job.

The following items in the Slurm API are *not* exposed in these Rust bindings:

```ignore
pub struct slurmdb_job_rec_t {
    pub account: *mut c_char,
    pub admin_comment: *mut c_char,
    pub alloc_gres: *mut c_char,
    pub alloc_nodes: u32,
    pub array_job_id: u32,
    pub array_max_tasks: u32,
    pub array_task_id: u32,
    pub array_task_str: *mut c_char,
    pub associd: u32,
    pub blockid: *mut c_char,
    pub cluster: *mut c_char,
    pub derived_ec: u32,
    pub derived_es: *mut c_char,
    pub first_step_ptr: *mut c_void,
    pub gid: u32,
    pub lft: u32,
    pub mcs_label: *mut c_char,
    pub partition: *mut c_char,
    pub pack_job_id: u32,
    pub pack_job_offset: u32,
    pub priority: u32,
    pub qosid: u32,
    pub req_cpus: u32,
    pub req_gres: *mut c_char,
    pub req_mem: u64,
    pub resvid: u32,
    pub resv_name: *mut c_char,
    pub show_full: u32,
    pub steps: List,
    pub timelimit: u32,
    pub track_steps: u16,
    pub tres_req_str: *mut c_char,
    pub uid: u32,
    pub used_gres: *mut c_char,
    pub user: *mut c_char,
    pub wckey: *mut c_char,
    pub wckeyid: u32,
    pub work_dir: *mut c_char,
}
```

(The above listing omits fields that would be handled by the
`JobStepRecordSharedFields` trait.)
");

/// A trait for accessing fields common to SlurmDB job records and step
/// records.
pub trait JobStepRecordSharedFields {
    /// Get the job/step's end time, or None if it has not yet ended.
    fn end_time(&self) -> Option<DateTime<Utc>>;

    /// Get the job/step's exit code, or None if it has not yet ended.
    fn exit_code(&self) -> Option<i32>;

    /// Get the maximum "virtual memory size" of the job/step, in kibibytes.
    ///
    /// This quantity is not available (i.e., the function returns `None`)
    /// until the job has finished running.
    fn max_vm_size(&self) -> Option<u64>;

    /// Get the job/step's start time, or None if it has not yet started.
    fn start_time(&self) -> Option<DateTime<Utc>>;

    /// Get the wallclock time taken by the job/step: end time minus start time.
    ///
    /// Returns None if the job/step has not yet completed (or even started).
    fn wallclock_duration(&self) -> Option<Duration>;
}

/// We implement the JobStepRecordSharedFields trait with a macro; that seems
/// like the easiest thing to do.
///
/// The complete list of overlapping fields is:
///
/// ```ignore
/// {
///    elapsed: u32,
///    end: i64,
///    exitcode: i32,
///    nodes: *mut c_char,
///    requid: u32,
///    start: i64,
///    state: u32,
///    stats: &slurm_sys::slurmdb_stats_t,
///    suspended: u32,
///    sys_cpu_sec: u32,
///    sys_cpu_usec: u32,
///    tot_cpu_sec: u32,
///    tot_cpu_usec: u32,
///    tres_alloc_str: *mut c_char,
///    user_cpu_sec: u32,
///    user_cpu_usec: u32,
/// }
/// ```
macro_rules! impl_job_step_record_shared_fields {
    ($type:path) => {
        impl JobStepRecordSharedFields for $type {
            fn end_time(&self) -> Option<DateTime<Utc>> {
                match self.sys_data().end as i64 {
                    0 => None,
                    t => Some(Utc.timestamp(t, 0)),
                }
            }

            fn exit_code(&self) -> Option<i32> {
                match self.sys_data().end as i64  {
                    0 => None,
                    _ => Some(self.sys_data().exitcode as i32),
                }
            }

            fn max_vm_size(&self) -> Option<u64> {
                match self.sys_data().stats.vsize_max {
                    slurm_sys::SLURMRS_NO_VAL64 => None,
                    other => Some(other),
                }
            }

            fn start_time(&self) -> Option<DateTime<Utc>> {
                match self.sys_data().start as i64 {
                    0 => None,
                    t => Some(Utc.timestamp(t, 0)),
                }
            }

            fn wallclock_duration(&self) -> Option<Duration> {
                match (self.start_time(), self.end_time()) {
                    (Some(start), Some(end)) => Some(end.signed_duration_since(start)),
                    _ => None,
                }
            }
        }
    }
}

impl_job_step_record_shared_fields!(JobRecord);

impl JobRecord {
    /// Get the job's "eligible" time, or None if the job is not yet eligible to run.
    pub fn eligible_time(&self) -> Option<DateTime<Utc>> {
        match self.sys_data().eligible as i64 {
            0 => None,
            t => Some(Utc.timestamp(t, 0)),
        }
    }

    /// Get the job's ID number.
    pub fn job_id(&self) -> JobId {
        self.sys_data().jobid
    }

    /// Get the job's name.
    pub fn job_name(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().jobname) }.to_string_lossy()
    }

    /// Get the job's submission time.
    pub fn submit_time(&self) -> DateTime<Utc> {
        Utc.timestamp(self.sys_data().submit as i64, 0)
    }

    /// Get the wallclock time spent waiting for the job to become eligible,
    /// or None if the job has not yet become eligible to run.
    pub fn eligible_wait_duration(&self) -> Option<Duration> {
        self.eligible_time().map(|t| t.signed_duration_since(self.submit_time()))
    }

    /// Get the wallclock time spent waiting for the job to start, or None
    /// if the job has not yet started.
    ///
    /// This includes time that the job spent waiting to become eligible to run.
    pub fn wait_duration(&self) -> Option<Duration> {
        self.start_time().map(|t| t.signed_duration_since(self.submit_time()))
    }

    /// Steps.
    pub fn steps(&self) -> &SlurmList<StepRecord> {
        unsafe { SlurmList::transmute_ptr(&self.sys_data().steps) }
    }
}


make_slurm_wrap_struct!(StepRecord, slurm_sys::slurmdb_step_rec_t, "\
Accounting information about a step within a job.

The following items in the Slurm API are *not* exposed in these Rust bindings:

```ignore
pub struct slurmdb_step_rec_t {
    pub job_ptr: *mut slurmdb_job_rec_t,
    pub nnodes: u32,
    pub ntasks: u32,
    pub pid_str: *mut c_char,
    pub req_cpufreq_min: u32,
    pub req_cpufreq_max: u32,
    pub req_cpufreq_gov: u32,
    pub task_dist: u32,
}
```

(The above listing omits fields that would be handled by the
`JobStepRecordSharedFields` trait.)
");

impl_job_step_record_shared_fields!(StepRecord);

impl StepRecord {
    /// Get the step's ID.
    pub fn step_id(&self) -> StepId {
         self.sys_data().stepid
    }

    /// Get the step's name.
    pub fn step_name(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().stepname) }.to_string_lossy()
    }
}


make_slurm_wrap_struct!(JobDescriptor, slurm_sys::job_descriptor, "\
A description of a batch job to submit.

The following items in the Slurm API are *not* exposed in these Rust bindings:

```ignore
pub struct job_descriptor {
    pub account: *mut c_char,
    pub acctg_freq: *mut c_char,
    pub admin_comment: *mut c_char,
    pub alloc_node: *mut c_char,
    pub alloc_resp_port: u16,
    pub alloc_sid: u32,
    pub array_inx: *mut c_char,
    pub array_bitmap: *mut c_void,
    pub begin_time: time_t,
    pub bitflags: u32,
    pub burst_buffer: *mut c_char,
    pub ckpt_interval: u16,
    pub ckpt_dir: *mut c_char,
    pub clusters: *mut c_char,
    pub cluster_features: *mut c_char,
    pub comment: *mut c_char,
    pub contiguous: u16,
    pub core_spec: u16,
    pub cpu_bind: *mut c_char,
    pub cpu_bind_type: u16,
    pub cpu_freq_min: u32,
    pub cpu_freq_max: u32,
    pub cpu_freq_gov: u32,
    pub deadline: time_t,
    pub delay_boot: u32,
    pub dependency: *mut c_char,
    pub end_time: time_t,
    pub environment: *mut *mut c_char,
    pub env_size: u32,
    pub extra: *mut c_char,
    pub exc_nodes: *mut c_char,
    pub features: *mut c_char,
    pub fed_siblings_active: u64,
    pub fed_siblings_viable: u64,
    pub gres: *mut c_char,
    pub immediate: u16,
    pub job_id: u32,
    pub job_id_str: *mut c_char,
    pub kill_on_node_fail: u16,
    pub licenses: *mut c_char,
    pub mail_type: u16,
    pub mail_user: *mut c_char,
    pub mcs_label: *mut c_char,
    pub mem_bind: *mut c_char,
    pub mem_bind_type: u16,
    pub network: *mut c_char,
    pub nice: u32,
    pub num_tasks: u32,
    pub open_mode: u8,
    pub origin_cluster: *mut c_char,
    pub other_port: u16,
    pub overcommit: u8,
    pub pack_job_offset: u32,
    pub plane_size: u16,
    pub power_flags: u8,
    pub priority: u32,
    pub profile: u32,
    pub qos: *mut c_char,
    pub reboot: u16,
    pub resp_host: *mut c_char,
    pub restart_cnt: u16,
    pub req_nodes: *mut c_char,
    pub requeue: u16,
    pub reservation: *mut c_char,
    pub shared: u16,
    pub spank_job_env: *mut *mut c_char,
    pub spank_job_env_size: u32,
    pub task_dist: u32,
    pub time_min: u32,
    pub wait_all_nodes: u16,
    pub warn_flags: u16,
    pub warn_signal: u16,
    pub warn_time: u16,
    pub cpus_per_task: u16,
    pub min_cpus: u32,
    pub max_cpus: u32,
    pub min_nodes: u32,
    pub max_nodes: u32,
    pub boards_per_node: u16,
    pub sockets_per_board: u16,
    pub sockets_per_node: u16,
    pub cores_per_socket: u16,
    pub threads_per_core: u16,
    pub ntasks_per_node: u16,
    pub ntasks_per_socket: u16,
    pub ntasks_per_core: u16,
    pub ntasks_per_board: u16,
    pub pn_min_cpus: u16,
    pub pn_min_memory: u64,
    pub pn_min_tmp_disk: u32,
    pub geometry: [u16; 5],
    pub conn_type: [u16; 5],
    pub rotate: u16,
    pub blrtsimage: *mut c_char,
    pub linuximage: *mut c_char,
    pub mloaderimage: *mut c_char,
    pub ramdiskimage: *mut c_char,
    pub req_switch: u32,
    pub select_jobinfo: *mut dynamic_plugin_data_t,
    pub tres_req_cnt: *mut u64,
    pub wait4switch: u32,
    pub wckey: *mut c_char,
    pub x11: u16,
    pub x11_magic_cookie: *mut c_char,
    pub x11_target_port: u16,
}
```

");

impl JobDescriptor {
    /// Get the group ID associated with this job.
    pub fn gid(&self) -> u32 {
        self.sys_data().group_id
    }

    /// Set the group ID associated with this job.
    pub fn set_gid(&mut self, value: u32) -> &mut Self {
        self.sys_data_mut().group_id = value;
        self
    }

    /// Set the group ID associated with this job to that of the current process.
    pub fn set_gid_current(&mut self) -> &mut Self {
        self.set_gid(unsafe { libc::getgid() })
    }

    /// Get this job's name.
    pub fn name(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().name) }.to_string_lossy()
    }

    /// Get the number of tasks within this job.
    pub fn num_tasks(&self) -> u32 {
        self.sys_data().num_tasks
    }

    /// Set the number of tasks within this job.
    pub fn set_num_tasks(&mut self, value: u32) -> &mut Self {
        self.sys_data_mut().num_tasks = value;
        self
    }

    /// Get this job's assigned partition.
    pub fn partition(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().partition) }.to_string_lossy()
    }

    /// Get the contents of this job's batch wrapper script.
    pub fn script(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().script) }.to_string_lossy()
    }

    /// Get the path for this job's standard error stream.
    pub fn stderr_path(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().std_err) }.to_string_lossy()
    }

    /// Get the path for this job's standard input stream.
    pub fn stdin_path(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().std_in) }.to_string_lossy()
    }

    /// Get the path for this job's standard output stream.
    pub fn stdout_path(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().std_out) }.to_string_lossy()
    }

    /// Get the time limit associated with this job, measured in minutes.
    pub fn time_limit(&self) -> u32 {
        self.sys_data().time_limit
    }

    /// Set the time limit associated with this job.
    ///
    /// The value is measured in minutes.
    pub fn set_time_limit(&mut self, minutes: u32) -> &mut Self {
        self.sys_data_mut().time_limit = minutes;
        self
    }

    /// Get the user ID associated with this job.
    pub fn uid(&self) -> u32 {
        self.sys_data().user_id
    }

    /// Set the user ID associated with this job.
    pub fn set_uid(&mut self, value: u32) -> &mut Self {
        self.sys_data_mut().user_id = value;
        self
    }

    /// Set the user ID associated with this job to that of the current process.
    pub fn set_uid_current(&mut self) -> &mut Self {
        self.set_uid(unsafe { libc::getuid() })
    }

    /// Get the contents of this job's assigned working directory.
    pub fn work_dir(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().work_dir) }.to_string_lossy()
    }

    /// Submit this job to the batch processor.
    ///
    /// TODO? Handle server-side errors reported in the response.
    pub fn submit_batch(&self) -> Result<SubmitResponseMessageOwned, SlurmError> {
        let mut msg = std::ptr::null_mut();
        ustry!(slurm_sys::slurm_submit_batch_job(self.0, &mut msg as _));
        Ok(unsafe { SubmitResponseMessageOwned::assume_ownership(msg as _) })
    }
}

make_owned_version!(@customdrop JobDescriptor, JobDescriptorOwned, "An owned version of `JobDescriptor`.");

impl JobDescriptorOwned {
    /// Create a new, defaulted job descriptor.
    pub fn new() -> Self {
        let inst = unsafe { Self::alloc_zeroed() };
        unsafe { slurm_sys::slurm_init_job_desc_msg((inst.0).0); }
        inst
    }

    fn maybe_clear_argv(&mut self) {
        let d = self.sys_data_mut();
        slurm_free_string_array(&mut d.argv, d.argc as usize);
        d.argc = 0;
    }

    /// Specify the command-line arguments of the job.
    pub fn set_argv<I: IntoIterator<Item = S>, S: AsRef<str>>(&mut self, argv: I) -> &mut Self {
        self.maybe_clear_argv();
        let (ptr, size) = slurm_alloc_utf8_string_array(argv);
        {
            let d = self.sys_data_mut();
            d.argv = ptr;
            d.argc = size as u32;
        }
        self
    }

    fn maybe_clear_environment(&mut self) {
        let d = self.sys_data_mut();
        slurm_free_string_array(&mut d.environment, d.env_size as usize);
        d.env_size = 0;
    }

    /// Explicitly specify the UNIX environment of the job.
    pub fn set_environment<I: IntoIterator<Item = S>, S: AsRef<str>>(&mut self, env: I) -> &mut Self {
        self.maybe_clear_environment();
        let (ptr, size) = slurm_alloc_utf8_string_array(env);
        {
            let d = self.sys_data_mut();
            d.environment = ptr;
            d.env_size = size as u32;
        }
        self
    }

    /// Set the UNIX environment of the job to match that of the current process.
    ///
    /// This will panic if any environment variables are not decodable as
    /// Unicode. This limitation could be worked around with some developer
    /// effort.
    pub fn inherit_environment(&mut self) -> &mut Self {
        self.set_environment(std::env::vars().map(|(key, val)| format!("{}={}", key, val)))
    }

    /// Set this job's name.
    pub fn set_name<S: AsRef<str>>(&mut self, name: S) -> &mut Self {
        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.name);
            d.name = slurm_alloc_utf8_string(name);
        }
        self
    }

    /// Set this job's partition.
    pub fn set_partition<S: AsRef<str>>(&mut self, partition: S) -> &mut Self {
        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.partition);
            d.partition = slurm_alloc_utf8_string(partition);
        }
        self
    }

    /// Set the contents of this job's wrapper shell script.
    ///
    /// This is the textual content of a shell script that will be executed as
    /// the batch job wrapper. It should start with a shebang (`#!`) line.
    pub fn set_script<S: AsRef<str>>(&mut self, script: S) -> &mut Self {
        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.script);
            d.script = slurm_alloc_utf8_string(script);
        }
        self
    }

    /// Set the path that will be used as this job's standard error stream.
    pub fn set_stderr_path<S: AsRef<str>>(&mut self, path: S) -> &mut Self {
        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.std_err);
            d.std_err = slurm_alloc_utf8_string(path);
        }
        self
    }

    /// Set the path that will be used as this job's standard input stream.
    pub fn set_stdin_path<S: AsRef<str>>(&mut self, path: S) -> &mut Self {
        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.std_in);
            d.std_in = slurm_alloc_utf8_string(path);
        }
        self
    }

    /// Set the path that will be used as this job's standard output stream.
    pub fn set_stdout_path<S: AsRef<str>>(&mut self, path: S) -> &mut Self {
        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.std_out);
            d.std_out = slurm_alloc_utf8_string(path);
        }
        self
    }

    /// Set this job's working directory.
    ///
    /// The working directory should be one that exists on all worker nodes of
    /// the cluster.
    pub fn set_work_dir<S: AsRef<str>>(&mut self, work_dir: S) -> &mut Self {
        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.work_dir);
            d.work_dir = slurm_alloc_utf8_string(work_dir);
        }
        self
    }

    /// Set this job's working directory to the current process's current
    /// directory.
    ///
    /// See `std::env::current_dir` for an explanation of the cases in which
    /// this operation can fail.
    pub fn set_work_dir_cwd(&mut self) -> Result<&mut Self, Error> {
        Ok(self.set_work_dir(std::env::current_dir()?
                          .to_str()
                          .ok_or(format_err!("could not express CWD as UTF8"))?
        ))
    }
}

impl Drop for JobDescriptorOwned {
    fn drop(&mut self) {
        self.maybe_clear_argv();
        self.maybe_clear_environment();

        {
            let d = self.sys_data_mut();
            slurm_free(&mut d.name);
            slurm_free(&mut d.partition);
            slurm_free(&mut d.script);
            slurm_free(&mut d.std_err);
            slurm_free(&mut d.std_in);
            slurm_free(&mut d.std_out);
            slurm_free(&mut d.work_dir);
        }

        slurm_free(&mut (self.0).0);
    }
}


make_slurm_wrap_struct!(SubmitResponseMessage, slurm_sys::submit_response_msg, "\
Information returned by Slurm upon job submission.
");

impl SubmitResponseMessage {
    /// Get the job ID of the new job.
    ///
    /// XXX: It looks like it is possible to have a non-zero `error_code` with
    /// a non-zero job ID; I'm not sure in what cases that occurs.
    pub fn job_id(&self) -> JobId {
        self.sys_data().job_id
    }

    /// Get the job-step ID of the new job.
    ///
    /// XXX: It looks like it is possible to have a non-zero `error_code` with
    /// a non-zero job ID; I'm not sure in what cases that occurs.
    pub fn step_id(&self) -> StepId {
        self.sys_data().step_id
    }

    /// Get the error code returned by the server.
    pub fn error_code(&self) -> u32 {
        self.sys_data().error_code
    }

    /// Get the "user message" returned by the server.
    ///
    /// I think this is arbitrary text that should be shown to the user?
    pub fn user_message(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.sys_data().job_submit_user_msg) }.to_string_lossy()
    }
}

make_owned_version!(@customdrop SubmitResponseMessage, SubmitResponseMessageOwned, "An owned version of `SubmitResponseMessage`.");

impl Drop for SubmitResponseMessageOwned {
    fn drop(&mut self) {
        unsafe { slurm_sys::slurm_free_submit_response_response_msg((self.0).0 as _) };
    }
}
