// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT license.

//! The version requirement here is totally made up.

extern crate bindgen;
extern crate pkg_config;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;

fn main() {
    let mut builder = bindgen::Builder::default()
        .header("src/wrapper.h");

    // Some Slurm installs don't have a pkg-config file.
    if let Ok(libdir) = env::var("SLURM_LIBDIR") {
        println!("cargo:rustc-link-search=native={}", libdir);
        println!("cargo:rustc-link-lib=dylib=slurm");
        println!("cargo:rustc-link-lib=dylib=slurmdb");

        if let Ok(incdir) = env::var("SLURM_INCDIR") {
            builder = builder.clang_arg(format!("-I{}", incdir));
        }
    } else {
        let slurm = pkg_config::Config::new().atleast_version("15.0").probe("slurm").unwrap();

        println!("cargo:rustc-link-lib=dylib=slurmdb");

        for ref path in &slurm.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
    }

    let bindings = builder
        .whitelist_type("slurm_.*")
        .whitelist_type("slurmdb_.*")
        .whitelist_function("slurm_.*")
        .whitelist_function("slurmdb_.*")
        .whitelist_var("SLURM.*")
        .whitelist_var("ESCRIPT.*")
        .whitelist_var("ESLURM.*")
        .whitelist_var("SLURMDB.*")
        .whitelist_var("SLURMRS.*")
        .generate()
        .expect("Unable to generate bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings_path = out_dir.join("bindings.rs");

    bindings
        .write_to_file(&bindings_path)
        .expect("Couldn't write bindings!");

    // Now, we (grossly) parse the bindings file and emit a second file. This
    // contains information that the main `slurm` crate can use in *its*
    // build.rs to auto-enable Cargo features that will then allow the main
    // codebase to conditionally compile Rust interfaces that depend on what
    // the C code supports. This all is the least-bad approach I can devise
    // that deals with the fact that the C API is not super stable.

    let bindings_file = File::open(&bindings_path)
        .expect(&format!("couldn't open bindgen output file {}", bindings_path.display()));
    let bindings_buf = BufReader::new(bindings_file);

    let features_path = out_dir.join("features.rs");
    let mut features_file = File::create(&features_path)
        .expect(&format!("couldn't create features output file {}", features_path.display()));

    writeln!(features_file, "pub const C_API_FEATURES: &[&str] = &[")
        .expect(&format!("couldn't write to features output file {}", features_path.display()));

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum State {
        Scanning,
        CheckingSelectedStepT,
        CheckingSubmitResponseMsg,
    }

    let mut state = State::Scanning;

    for maybe_line in bindings_buf.lines() {
        let line = maybe_line.expect(&format!("couldn't read bindgen output file {}", bindings_path.display()));

        match state {
            State::Scanning => {
                if line.starts_with("pub struct slurmdb_selected_step_t {") {
                    state = State::CheckingSelectedStepT;
                } else if line.starts_with("pub struct submit_response_msg {") {
                    state = State::CheckingSubmitResponseMsg;
                }
            },

            State::CheckingSelectedStepT => {
                if line == "}" {
                    state = State::Scanning;
                } else if line.contains("pack_job_offset") {
                    writeln!(features_file, "\"selected_step_t_pack_job_offset\",")
                        .expect(&format!("couldn't write to features output file {}", features_path.display()));
                }
            }

            State::CheckingSubmitResponseMsg => {
                if line == "}" {
                    state = State::Scanning;
                } else if line.contains("job_submit_user_msg") {
                    writeln!(features_file, "\"submit_response_user_message\",")
                        .expect(&format!("couldn't write to features output file {}", features_path.display()));
                }
            }
        }
    }

    writeln!(features_file, "];")
        .expect(&format!("couldn't write to features output file {}", features_path.display()));
}
