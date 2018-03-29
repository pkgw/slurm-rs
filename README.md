# slurm-rs

Rust bindings for the [Slurm workload manager](https://slurm.schedmd.com/).

The API coverage is far from complete, but the basic framework is in place.


## Building and Compatibility

See the README for the `slurm-sys` subdirectory for some notes on how to build
against your Slurm library correctly. You must have a functional `rustfmt`
installed. You may also need to set some environment variables to allow the
build script to locate your Slurm libraries and include files.

At the moment, this crate is being developed against Slurm 17.11. The Slurm C
API is not especially stable, so it is possible that this crate will fail to
compile against other versions of Slurm, or even exhibit wrong runtime
behavior. The goal is for the crate to work with a wide range of Slurm
versions, and there is code infrastructure to adapt to the evolving C API. If
the crate fails to build for a reason that appears to be related to the
version of Slurm that you're using, please file an issue with the details.

This crate also requires that the Slurm accounting database library
`libslurmdb` is available. Contributions to relax this requirement would be
welcome.


## Licensing

Licensed under the MIT License.
