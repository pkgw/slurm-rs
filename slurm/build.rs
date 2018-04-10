// Copyright 2018 Peter Williams <peter@newton.cx> and collaborators
// Licensed under the MIT license.

//! Fairly gross hack to adapt to the changing C API.
//!
//! The `slurm-sys` build script generates the Rust API from the Slurm C
//! headers, then scans the output file to check for various features of the C
//! API that have come and gone over various releases. It creates a special
//! constant in the Rust API that enumerates those features.
//!
//! Here, we use this constant to add feature flags to the `rustc` arguments
//! used to compile this module. That way we can use `#[cfg]` directives to
//! conditionally compile Rust code based on what's available in the C API,
//! without the user having to know or care about what's going on under the hood.
//!
//! (In principle some features *should* be exposed at higher levels: say that
//! a new version adds a new major feature and certain upstream programs need
//! to know that it is available. We don't have that situation yet, though.)

extern crate slurm_sys;

fn main() {
    for feat in slurm_sys::C_API_FEATURES {
        println!("cargo:rustc-cfg=slurm_api_{}", feat);
    }
}
