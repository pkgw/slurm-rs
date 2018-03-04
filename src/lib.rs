// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Interface to the Slurm workload manager.

The Slurm C library uses a (primitive) custom memory allocator for its data
structures. Because we must maintain compatibility with this allocator, we
have to allocate all of our data structures from the heap rather than the
stack. Almost all of the structures exposed here come in both “borrowed” and
“owned” flavors; they are largely equivalent, but only the owned versions free
their data when they go out of scope.

*/

#[macro_use] extern crate failure;
#[macro_use] extern crate failure_derive;
extern crate slurm_sys;

use failure::Error;
use std::borrow::Cow;
use std::default::Default;
use std::ffi::CStr;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::os::raw::{c_int, c_void};


/// A job identifier number; this will always be `u32`.
pub type JobId = u32;

/// A job-step identifier number; this will always be `u32`.
pub type StepId = u32;


// (Ab)use macros a bit to map low-level slurm API errors to a Rust interface.

macro_rules! each_mapped_slurm_error {
    ($mac:ident) => {
        $mac!(
            <InvalidJobId, slurm_sys::ESLURM_INVALID_JOB_ID,
             "The job ID did not correspond to a valid job.";>,
            <InvalidPartitionName, slurm_sys::ESLURM_INVALID_PARTITION_NAME,
             "The specified partition name was not recognized.";>
        );
    }
}

macro_rules! declare_slurm_error {
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

each_mapped_slurm_error!(declare_slurm_error);

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

        if ptr == 0 as _ {
            let e = unsafe { slurm_sys::slurm_get_errno() };
            Err(SlurmError::from_slurm(e))
        } else {
            Ok(ptr)
        }?
    }}
}


/// Allocate a structure using Slurm's allocator.
fn slurm_alloc<T>() -> *mut T {
    const TEXT: &[u8] = b"slurm-rs\0";
    let ptr = unsafe { slurm_sys::slurm_try_xmalloc(std::mem::size_of::<T>(), TEXT.as_ptr() as _, 1, TEXT.as_ptr() as _) };

    if ptr == 0 as _ {
        panic!("Slurm memory allocation failed");
    }

    ptr as _
}


/// Free a structure using Slurm's allocator.
fn slurm_free<T>(thing: &mut *mut T) {
    const TEXT: &[u8] = b"slurm-rs\0";
    let p = &mut (*thing as *mut c_void);
    unsafe { slurm_sys::slurm_xfree(p, TEXT.as_ptr() as _, 1, TEXT.as_ptr() as _) };
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
pub struct SlurmList<T>(*mut slurm_sys::xlist, PhantomData<T>);

impl<T> SlurmList<T> {
    unsafe fn transmute_ptr<'a>(ptr: &'a *mut slurm_sys::xlist) -> &'a Self {
        std::mem::transmute(ptr)
    }

    unsafe fn transmute_ptr_mut<'a>(ptr: &'a mut *mut slurm_sys::xlist) -> &'a mut Self {
        std::mem::transmute(ptr)
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


// Now we can finally start wrapping types that we care about.

make_slurm_wrap_struct!(JobInfo, slurm_sys::job_info, "Information about a running job.");

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


make_slurm_wrap_struct!(JobFilters, slurm_sys::slurmdb_job_cond_t, "A set of
                        filters for identifying jobs of interest when querying
                        the Slurm accounting database.");

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

        if self.0 == 0 as _ {
            // XXX if malloc fails, I think this function will abort under us.
            self.0 = unsafe { slurm_sys::slurm_list_create(Some(slurm_sys::slurmdb_destroy_selected_step)) };
        }

        unsafe { slurm_sys::slurm_list_append(self.0, item.0 as _); }
    }
}


make_slurm_wrap_struct!(JobRecord, slurm_sys::slurmdb_job_rec_t, "Accounting information about a job.");

impl JobRecord {
    /// Get the job's exit code.
    pub fn exit_code(&self) -> u32 {
        self.sys_data().exitcode
    }
}
