// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT license.

/// The version requirement here is totally made up.

extern crate bindgen;
extern crate pkg_config;

use std::env;
use std::path::PathBuf;

fn main() {
    let slurm = pkg_config::Config::new().atleast_version("15.0").probe("slurm").unwrap();

    let mut builder = bindgen::Builder::default()
        .header("src/wrapper.h");

    for ref path in &slurm.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
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
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
