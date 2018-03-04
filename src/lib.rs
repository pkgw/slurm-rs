// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Interface to the SLURM cluster job manager.

*/

#[macro_use] extern crate failure;
extern crate slurm_sys;

use failure::Error;
use std::borrow::Cow;
use std::ffi::CStr;
use std::ops::Deref;


/// A job identifier number; this will always be `u32`.
pub type JobId = u32;


/// Most SLURM API calls return an zero on success. The library API docs state
/// that the return code on error is -1, and this macro encapsulates the task
/// of obtaining an errno and converting it to a result. However, in at least
/// one case the return code is an errno, which would be a nicer pattern from
/// a thread-safety standpoint.
macro_rules! stry {
    ($op:expr) => {{
        if $op != 0 {
            let e = unsafe { slurm_sys::slurm_get_errno() };
            let m = unsafe { CStr::from_ptr(slurm_sys::slurm_strerror(e)) };
            Err(format_err!("SLURM error {}: {}", e, m.to_string_lossy()))
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
