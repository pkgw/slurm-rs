// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Interface to the SLURM cluster job manager.

*/

#[macro_use] extern crate failure;
#[macro_use] extern crate failure_derive;
extern crate slurm_sys;

use failure::Error;
use std::borrow::Cow;
use std::ffi::CStr;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::ops::Deref;
use std::os::raw::c_int;


/// A job identifier number; this will always be `u32`.
pub type JobId = u32;


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
        /// Specifically-enumerated errors that we can get from the SLURM API.
        ///
        /// This is not exhaustive; the only specifically implemented options are ones
        /// that are expected to be of interest to general developers.
        #[derive(Copy, Clone, Debug, Eq, Fail, Hash, PartialEq)]
        pub enum SlurmError {
            $(
                #[doc=$doc] $rustname,
            )*

            /// Some other SLURM error.
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
        write!(f, "{} (SLURM errno {})", m.to_string_lossy(), e)
    }
}


/// Most SLURM API calls return an zero on success. The library API docs state
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

/// This is like `stry!` but also wraps the SLURM call in an `unsafe{}` block,
/// since most (all?) of the times we're doing this, we're using the C API.
macro_rules! ustry {
    ($op:expr) => {
        stry!(unsafe { $op })
    }
}


/// Information about a running job.
#[derive(Debug)]
pub struct JobInfo(*mut slurm_sys::job_info);

impl JobInfo {
     /// Get this job's ID.
     pub fn job_id(&self) -> JobId {
         unsafe { *self.0 }.job_id
     }

     /// Get the cluster partition on which this job resides.
     pub fn partition(&self) -> Cow<str> {
         unsafe { CStr::from_ptr((*self.0).partition) }.to_string_lossy()
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
pub fn get_job_info(jid: JobId) -> Result<SingleJobInfoResponse, Error> {
    let mut resp: *mut slurm_sys::job_info_msg_t = 0 as _;

    ustry!(slurm_sys::slurm_load_job(&mut resp, jid, 0));

    let rc = unsafe { (*resp).record_count };
    if rc != 1 {
        return Err(format_err!("expected exactly one info record for job {}; got {} items", jid, rc));
    }

    Ok(SingleJobInfoResponse {
        message: resp,
        as_info: JobInfo(unsafe { (*resp).job_array }),
    })
}


/// Information about a single job.
///
/// This type implements `Deref` to `JobInfo` and so can be essentially be
/// treated as a `JobInfo`. Due to how the SLURM library manages memory, this
/// separate type is necessary in some cases.
#[derive(Debug)]
pub struct SingleJobInfoResponse {
    message: *mut slurm_sys::job_info_msg_t,
    as_info: JobInfo,
}

impl Drop for SingleJobInfoResponse {
    fn drop(&mut self) {
        unsafe { slurm_sys::slurm_free_job_info_msg(self.message) };
    }
}

impl Deref for SingleJobInfoResponse {
    type Target = JobInfo;

    fn deref(&self) -> &JobInfo {
        &self.as_info
    }
}
