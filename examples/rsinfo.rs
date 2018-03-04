// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Print out information about a job.
*/

#[macro_use] extern crate clap;
extern crate failure;
extern crate slurm;

use clap::{Arg, App};
use failure::Error;
use std::process;

fn main() {
    let matches = App::new("rsinfo")
        .version(crate_version!())
        .about("Print information about one job.")
        .arg(Arg::with_name("JOBID")
             .help("The jobid of the job in question")
             .required(true)
             .index(1))
        .get_matches();

    let jobid = matches.value_of("JOBID").unwrap();

    process::exit(match inner(jobid) {
        Ok(code) => code,

        Err(e) => {
            eprintln!("fatal error in rsinfo");
            for cause in e.causes() {
                eprintln!("  caused by: {}", cause);
            }
            1
        },
    });
}


fn inner(jobid: &str) -> Result<i32, Error> {
   let jobid = jobid.parse::<u32>()?;
   let info = slurm::get_job_info(jobid)?;
   println!("Job ID: {}", info.job_id());
   println!("Partition: {}", info.partition());
   Ok(0)
}
