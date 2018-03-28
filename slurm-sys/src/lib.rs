// Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT license.

//! Low-level bindings to the `libslurm` and `libslurmdb` libraries.
//!
//! The [Slurm](https://slurm.schedmd.com/) workload manager a system for
//! scheduling and running jobs on large computing clusters. It is often used
//! in scientific HPC (high-performance computing) contexts.
//!
//! These bindings provide nothing beyond the barest minimum needed to
//! interface to the C code unsafely. As such, this crate has no documentation
//! beyond the text you see here. Use a higher-level Rust crate in application
//! code.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
