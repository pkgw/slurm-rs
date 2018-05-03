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
use std::io::Write;
use std::process;
use structopt::StructOpt;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};


mod recent;
mod status;


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
    fn cli(self, stdout: StandardStream) -> Result<i32, Error> {
        match self {
            SlurmPlusCli::Recent(cmd) => cmd.cli(stdout),
            SlurmPlusCli::Status(cmd) => cmd.cli(stdout),
        }
    }
}


fn main() {
    let program = SlurmPlusCli::from_args();

    let stdout = StandardStream::stdout(ColorChoice::Auto);
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    process::exit(match program.cli(stdout) {
        Ok(code) => code,

        Err(e) => {
            let mut first = true;

            let mut red = ColorSpec::new();
            red.set_fg(Some(Color::Red)).set_bold(true);

            for cause in e.causes() {
                if first {
                    let _r = stderr.set_color(&red);
                    let _r = write!(stderr, "error:");
                    let _r = stderr.reset();
                    let _r = writeln!(stderr, " {}", cause);
                    first = false;
                } else {
                    let _r = write!(stderr, "  ");
                    let _r = stderr.set_color(&red);
                    let _r = write!(stderr, "caused by:");
                    let _r = stderr.reset();
                    let _r = writeln!(stderr, " {}", cause);
                }
            }
            1
        },
    });
}
