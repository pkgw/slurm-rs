// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Interface to the SLURM cluster job manager.

*/

#[macro_use] extern crate failure;
extern crate slurm_sys;

use failure::Error;
use std::borrow::Cow;
use std::ffi::CStr;


/// A job identifier number; this will always be u32.
pub type JobId = u32;


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

macro_rules! ustry {
    ($op:expr) => {
        stry!(unsafe { $op })
    }
}


/// Information about a running job.
pub struct JobInfo(slurm_sys::job_info);


impl JobInfo {
     /// Get information about the specified job.
     pub fn get_one(jid: JobId) -> Result<JobInfo, Error> {
         let mut resp: *mut slurm_sys::job_info_msg_t = 0 as _;

         ustry!(slurm_sys::slurm_load_job(&mut resp, jid, 0));
         let (rc, infos) = unsafe { ((*resp).record_count, (*resp).job_array) };

         if rc != 1 {
             return Err(format_err!("expected exactly one info record for job {}; got {} items", jid, rc));
         }

         // XXX THIS IS BUSTED because the job info structures contain string
         // pointers and freeing the job info message frees those strings.
         let rv = JobInfo(unsafe { *infos });

         unsafe { slurm_sys::slurm_free_job_info_msg(resp) };

         Ok(rv)
     }

     /// Get this job's ID.
     pub fn job_id(&self) -> JobId {
         self.0.job_id
     }

     /// Get the cluster partition on which this job resides.
     /// XXX CURRENTLY BUSTED, SEE ABOVE.
     pub fn partition(&self) -> Cow<str> {
         unsafe { CStr::from_ptr(self.0.partition) }.to_string_lossy()
     }
}
