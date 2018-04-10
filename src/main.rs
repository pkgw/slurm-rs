// Copyright 2018 Peter Williams <peter@newton.cx>
// Licensed under the MIT License.

//! The main CLI driver logic.

extern crate failure;
extern crate slurm;
#[macro_use] extern crate structopt;

use failure::Error;
use std::process;
use structopt::StructOpt;


#[derive(Debug, StructOpt)]
pub struct RecentCommand {
}

impl RecentCommand {
    fn cli(self) -> Result<i32, Error> {
        Ok(0)
    }
}


#[derive(Debug, StructOpt)]
#[structopt(name = "slurmplus", about = "Better commands for interacting with Slurm.")]
pub enum SlurmPlusCli {
    #[structopt(name = "recent")]
    /// Summarize recently-run jobs
    Recent(RecentCommand),
}

impl SlurmPlusCli {
    fn cli(self) -> Result<i32, Error> {
        match self {
            SlurmPlusCli::Recent(cmd) => cmd.cli(),
        }
    }
}


fn main() {
    let program = SlurmPlusCli::from_args();

    process::exit(match program.cli() {
        Ok(code) => code,

        Err(e) => {
            eprintln!("slurmplus: the command failed");
            for cause in e.causes() {
                eprintln!("  caused by: {}", cause);
            }
            1
        },
    });
}
