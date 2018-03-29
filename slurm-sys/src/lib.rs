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
include!(concat!(env!("OUT_DIR"), "/features.rs"));

/// This function can be passed as a callback to functions like
/// `slurm_list_create` that want a deallocator argument. `slurm_xfree`
/// doesn't work because (1) it takes a pointer *to a* pointer, so that it can
/// zero it out; and (2) it takes additional arguments populated from C
/// preprocessor `__FILE__` and `__LINE__` directives.
#[no_mangle]
pub extern fn slurmrs_free(ptr: *mut std::os::raw::c_void) {
    let mut copy = ptr;
    const TEXT: &[u8] = b"slurm-rs\0";
    unsafe { slurm_xfree(&mut copy, TEXT.as_ptr() as _, 1, TEXT.as_ptr() as _) };
}
