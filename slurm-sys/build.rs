// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT license.

//! The Slurm version requirement here is totally made up.
//!
//! I want the docs of slurm-rs to be available on docs.rs, which requires us
//! to be able to compile this bindings module on that platform. The code
//! doesn't need to be *runnable*, but we need to be able to build it.
//! Unsurprisingly, the docs.rs VM does not happen to have libslurm installed,
//! and it currently looks unlikely that there will ever be a mechanism to
//! install it ourselves. Therefore we undertake a massive hack: we download a
//! pre-generated version of the binding file rather than creating it with
//! bindgen, and we don't have this module link with any libraries. This would
//! be a disaster if we actually wanted to run the resulting code, but we
//! don't.
//!
//! In this case we download the pre-generated file using `wget` since it is
//! available on docs.rs and we avoid having to link this file with all sorts
//! of network libraries. We could store it in Git, but the file is big and I
//! want to avoid the possibility of confusion.
//!
//! The other thing is that this file needs to be able to know that it's being
//! built on docs.rs in order to activate the hack! That's done by (ab)using
//! the `rustc_args` and `rustdoc_args` properties of the
//! `package.metadata.docs.rs` section of Cargo.toml.

extern crate bindgen;
extern crate pkg_config;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;

const PREBUILT_BINDINGS_URL: &str = "https://gist.github.com/pkgw/40e36f9dc0d771323205fc0617ac7141/\
                                     raw/6405dba98cd0eec7fab483b3d090b919e1383094/bindings.rs";

fn main() {
    let mut do_the_bindgen = true;
    let mut builder = bindgen::Builder::default()
        .header("src/wrapper.h");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings_path = out_dir.join("bindings.rs");

    if cfg!(slurmrs_on_docs_rs) {
        // Activate the hack!
        do_the_bindgen = false;
        Command::new("curl")
            .arg("-sSL")
            .arg("-o")
            .arg(&bindings_path)
            .arg(PREBUILT_BINDINGS_URL)
            .status()
            .expect("failed to execute process");
    } else if let Ok(libdir) = env::var("SLURM_LIBDIR") {
        // Some Slurm installs don't have a pkg-config file.
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

    if do_the_bindgen {
        let bindings = builder
            .whitelist_type("job_.*")
            .whitelist_type("slurm_.*")
            .whitelist_type("slurmdb_.*")
            .whitelist_function("slurm_.*")
            .whitelist_function("slurmdb_.*")
            .whitelist_var("ESCRIPT.*")
            .whitelist_var("ESLURM.*")
            .whitelist_var("SLURM.*")
            .whitelist_var("SLURMDB.*")
            .whitelist_var("SLURMRS.*")
            .rustfmt_bindings(true)
            .generate()
            .expect("Unable to generate bindings");

        bindings
            .write_to_file(&bindings_path)
            .expect("Couldn't write bindings!");
    }

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
    let mut n_lines = 0;

    for maybe_line in bindings_buf.lines() {
        let line = maybe_line.expect(&format!("couldn't read bindgen output file {}", bindings_path.display()));
        n_lines += 1;

        match state {
            State::Scanning => {
                if line.starts_with("pub struct slurmdb_selected_step_t {") {
                    state = State::CheckingSelectedStepT;
                } else if line.starts_with("pub struct submit_response_msg {") {
                    state = State::CheckingSubmitResponseMsg;
                } else if line.starts_with("pub const job_states_JOB_DEADLINE") {
                    writeln!(features_file, "\"job_state_deadline\",")
                        .expect(&format!("couldn't write to features output file {}", features_path.display()));
                } else if line.starts_with("pub const job_states_JOB_OOM") {
                    writeln!(features_file, "\"job_state_oom\",")
                        .expect(&format!("couldn't write to features output file {}", features_path.display()));
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

    // If rustfmt is unavailable, the output is all on two (very long) lines. Can't parse that.
    assert!(n_lines > 100, "to build this crate you must install a functional \"rustfmt\" (see README.md)");

    writeln!(features_file, "];")
        .expect(&format!("couldn't write to features output file {}", features_path.display()));
}
