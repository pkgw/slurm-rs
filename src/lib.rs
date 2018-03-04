// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Interface to the Slurm workload manager.

The Slurm C library uses a (primitive) custom memory allocator for its data
structures. Because we must maintain compatibility with this allocator, it is
not helpful to stack-allocate the various Slurm data structures.

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
    ($rust_name:ident, $slurm_name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug)]
        pub struct $rust_name(*mut slurm_sys::$slurm_name);

        impl $rust_name {
            /// Access the underlying slurm_sys struct immutably.
            #[allow(unused)]
            fn sys_data(&self) -> &slurm_sys::$slurm_name {
                unsafe { &(*self.0) }
            }

            /// Access the underlying slurm_sys struct mutably.
            #[allow(unused)]
            fn sys_data_mut(&mut self) -> &mut slurm_sys::$slurm_name {
                unsafe { &mut (*self.0) }
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


make_slurm_wrap_struct!(JobInfo, job_info, "Information about a running job.");

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
pub fn get_job_info(jid: JobId) -> Result<SingleJobInfoMessage, Error> {
    let mut msg: *mut slurm_sys::job_info_msg_t = 0 as _;

    ustry!(slurm_sys::slurm_load_job(&mut msg, jid, 0));

    let rc = unsafe { (*msg).record_count };
    if rc != 1 {
        return Err(format_err!("expected exactly one info record for job {}; got {} items", jid, rc));
    }

    Ok(SingleJobInfoMessage {
        message: msg,
        as_info: JobInfo(unsafe { (*msg).job_array }),
    })
}


/// Information about a single job.
///
/// This type implements `Deref` to `JobInfo` and so can be essentially be
/// treated as a `JobInfo`. Due to how the Slurm library manages memory, this
/// separate type is necessary in some cases.
#[derive(Debug)]
pub struct SingleJobInfoMessage {
    message: *mut slurm_sys::job_info_msg_t,
    as_info: JobInfo,
}

impl Drop for SingleJobInfoMessage {
    fn drop(&mut self) {
        unsafe { slurm_sys::slurm_free_job_info_msg(self.message) };
    }
}

impl Deref for SingleJobInfoMessage {
    type Target = JobInfo;

    fn deref(&self) -> &JobInfo {
        &self.as_info
    }
}


/// A connection to the Slurm accounting database.
#[derive(Debug)]
pub struct DatabaseConnection(*mut c_void);

impl DatabaseConnection {
    /// Connect to the Slurm database.
    pub fn new() -> Result<Self, SlurmError> {
        let ptr = unsafe { slurm_sys::slurmdb_connection_get() };

        if ptr == 0 as _ {
            let e = unsafe { slurm_sys::slurm_get_errno() };
            Err(SlurmError::from_slurm(e))
        } else {
            Ok(DatabaseConnection(ptr))
        }
    }
}


impl Drop for DatabaseConnection {
    fn drop(&mut self) {
        let _ignored = unsafe { slurm_sys::slurmdb_connection_close(&mut self.0) };
    }
}


/// A set of filters for identifying jobs of interest when querying the Slurm
/// accounting database.
#[derive(Debug)]
pub struct JobFilters(*mut slurm_sys::slurmdb_job_cond_t);

impl JobFilters {
    fn sys_data(&self) -> &slurm_sys::slurmdb_job_cond_t {
        unsafe { &(*self.0) }
    }

    fn sys_data_mut(&mut self) -> &mut slurm_sys::slurmdb_job_cond_t {
        unsafe { &mut (*self.0) }
    }

    pub fn step_list(&self) -> &SlurmList<JobStepFilter> {
        unsafe { SlurmList::transmute(&(*self.0).step_list) }
    }

    pub fn step_list_mut(&mut self) -> &mut SlurmList<JobStepFilter> {
        unsafe { SlurmList::transmute_mut(&mut (*self.0).step_list) }
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


/// A filter for selecting jobs and job steps.
#[derive(Copy, Clone, Debug)]
pub struct JobStepFilter(*mut slurm_sys::slurmdb_selected_step_t);

//impl JobStepFilter {
//    /// Create a new job step filter
//    pub fn new(jid: JobId) -> Self {
//        JobStepFilter(slurm_sys::slurmdb_selected_step_t {
//            array_task_id: slurm_sys::SLURMRS_NO_VAL,
//            jobid: jid,
//            pack_job_offset: slurm_sys::SLURMRS_NO_VAL,
//            stepid: slurm_sys::SLURMRS_NO_VAL,
//        })
//    }
//}


/// A list of some kind of object known to Slurm.
///
/// These lists show up in a variety of places in the Slurm API.
#[derive(Copy, Clone, Debug)]
pub struct SlurmList<'a, T: 'a>(*mut slurm_sys::xlist, PhantomData<&'a T>);

impl<'a, T: 'a> SlurmList<'a, T> {
    unsafe fn destroy(&mut self) {
        if self.0 != 0 as _ {
            slurm_sys::slurm_list_destroy(self.0);
            self.0 = 0 as _;
        }
    }

    unsafe fn transmute(list: &*mut slurm_sys::xlist) -> &Self {
        std::mem::transmute(list)
    }

    unsafe fn transmute_mut(list: &mut *mut slurm_sys::xlist) -> &mut Self {
        std::mem::transmute(list)
    }
}

impl<'a> SlurmList<'a, JobStepFilter> {
    pub fn add(&mut self, value: JobStepFilter) {
    }
}
