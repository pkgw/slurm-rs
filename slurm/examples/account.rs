// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Demonstration of querying the Slurmdb job accounting database.
 */

extern crate chrono;
#[macro_use] extern crate clap;
extern crate failure;
extern crate slurm;

use chrono::Utc;
use clap::{Arg, App};
use failure::Error;
use slurm::JobStepRecordSharedFields;
use std::process;

fn main() {
    let matches = App::new("rsinfo")
        .version(crate_version!())
        .about("Print accounting information about one job.")
        .arg(Arg::with_name("JOBID")
             .help("The jobid of the job in question")
             .required(true)
             .index(1))
        .get_matches();

    let jobid = matches.value_of("JOBID").unwrap();

    process::exit(match inner(jobid) {
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


fn inner(jobid: &str) -> Result<i32, Error> {
    let jobid = jobid.parse::<slurm::JobId>()?;

    let mut filter = slurm::JobFiltersOwned::default();
    filter.step_list_mut().append(slurm::JobStepFilterOwned::new(jobid));

    let db = slurm::DatabaseConnectionOwned::new()?;
    let jobs = db.get_jobs(&filter)?;
    let now = Utc::now();

    for job in jobs.iter() {
        println!("{} {}", job.job_id(), job.job_name());

        if let Some(d) = job.eligible_wait_duration() {
            println!("  time for job to become eligible to run: {} s", d.num_seconds());
        } else {
            let wait = now.signed_duration_since(job.submit_time());
            println!("  job not yet eligible to run; time since submission: {} s", wait.num_seconds());
        }

        if let Some(d) = job.wait_duration() {
            println!("  wait time: {} s", d.num_seconds());
        } else if let Some(t_el) = job.eligible_time() {
            let wait = now.signed_duration_since(t_el);
            println!("  job not yet started; time since eligibility: {} s", wait.num_seconds());
        } else {
            println!("  still waiting to start");
        }

        if let Some(t_st) = job.start_time() {
            let t_limit = t_st + chrono::Duration::minutes(job.time_limit() as i64);
            let remaining = t_limit.signed_duration_since(now).num_minutes();
            if remaining > 0 {
                println!("  time left until job hits time limit: {} min", remaining);
            }
        }

        for step in job.steps().iter() {
            println!("  step {} {}", step.step_id(), step.step_name());

            if let Some(d) = step.wallclock_duration() {
                println!("    wallclock runtime: {} s", d.num_seconds());
                println!("    exit code: {}", step.exit_code().unwrap());
            } else if let Some(t_st) = step.start_time() {
                let wait = now.signed_duration_since(t_st);
                println!("    step not yet finished; time since start: {} s", wait.num_seconds());
            } else {
                println!("    step not yet finished");
            }

            if let Some(b) = step.max_vm_size() {
                println!("    max VM size: {:.2} MiB", (b as f64) / 1024.);
            } else {
                println!("    max VM size not available (probably because step not finished)");
            }
        }
    }

    Ok(0)
}
