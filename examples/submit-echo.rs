// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT License

/*! Submit a hello-world echo job
 */

#[macro_use] extern crate clap;
#[macro_use] extern crate failure;
extern crate slurm;

use clap::App;
use failure::Error;
use std::env;
use std::process;

fn main() {
    let _matches = App::new("submit-echo")
        .version(crate_version!())
        .about("Submit a hello-world echo job")
        .get_matches();

    process::exit(match inner() {
        Ok(code) => code,

        Err(e) => {
            eprintln!("failed to submit job");
            for cause in e.causes() {
                eprintln!("  caused by: {}", cause);
            }
            1
        },
    });
}


fn inner() -> Result<i32, Error> {
    let cwd = env::current_dir()?;

    let log = {
        let mut p = cwd.clone();
        p.push("%j.log");
        p.to_str().ok_or(format_err!("cannot stringify log path"))?.to_owned()
    };

    let mut desc = slurm::JobDescriptorOwned::new();

    desc.set_name("helloworld")
        .set_argv(&["helloworld"])
        .inherit_environment()
        .set_stderr_path(&log)
        .set_stdin_path("/dev/null")
        .set_stdout_path(&log)
        .set_work_dir_cwd()?
        .set_script("#! /bin/bash
set -e -x
echo hello world
")
        .set_num_tasks(1) // JobDescriptor args must come after due to the return type
        .set_uid_current();

    let msg = desc.submit_batch()?;
    println!("new job id: {}", msg.job_id());
    Ok(0)
}
