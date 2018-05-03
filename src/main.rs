// Copyright 2018 Peter Williams <peter@newton.cx>
// Licensed under the MIT License.

//! The main CLI driver logic.

extern crate chrono;
extern crate failure;
extern crate itertools;
extern crate slurm;
#[macro_use] extern crate structopt;
extern crate termcolor;
extern crate users;

use failure::Error;
use std::process;
use structopt::StructOpt;

#[macro_use] mod colorio; // keep first to get macros
mod recent;
mod status;

use colorio::ColorIo;


#[derive(Debug, StructOpt)]
#[structopt(name = "slurmplus", about = "Better commands for interacting with Slurm.")]
enum SlurmPlusCli {
    #[structopt(name = "recent")]
    /// Summarize recently-run jobs
    Recent(recent::RecentCommand),

    #[structopt(name = "status")]
    /// Get the status of a job
    Status(status::StatusCommand),
}

impl SlurmPlusCli {
    fn cli(self, cio: &mut ColorIo) -> Result<i32, Error> {
        match self {
            SlurmPlusCli::Recent(cmd) => cmd.cli(cio),
            SlurmPlusCli::Status(cmd) => cmd.cli(cio),
        }
    }
}


fn main() {
    let program = SlurmPlusCli::from_args();
    let mut cio = ColorIo::new();

    process::exit(match program.cli(&mut cio) {
        Ok(code) => code,

        Err(e) => {
            cio.print_error(e);
            1
        },
    });
}
