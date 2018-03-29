// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Make a list of recent jobs belonging to this user.
 */

extern crate chrono;
#[macro_use] extern crate clap;
extern crate failure;
extern crate slurm;

use chrono::{Duration, Utc};
use clap::App;
use failure::Error;
use slurm::JobStepRecordSharedFields;
use std::process;

fn main() {
    let _matches = App::new("rsinfo")
        .version(crate_version!())
        .about("Make a list of recent jobs")
        .get_matches();

    process::exit(match inner() {
        Ok(code) => code,

        Err(e) => {
            eprintln!("fatal error in account");
            for cause in e.causes() {
                eprintln!("  caused by: {}", cause);
            }
            1
        },
    });
}


fn inner() -> Result<i32, Error> {
    let now = Utc::now();
    let min_start = now - Duration::days(7);

    let mut filter = slurm::JobFiltersOwned::default();
    filter.userid_list_mut().append("555409");
    filter.usage_start(min_start);

    let db = slurm::DatabaseConnectionOwned::new()?;
    let jobs = db.get_jobs(&filter)?;

    for job in jobs.iter() {
        println!("{} {} {}", job.job_id(), job.job_name(), job.state().shortcode());
    }

    Ok(0)
}
