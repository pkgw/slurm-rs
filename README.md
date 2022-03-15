# NOTE: Unmaintained

This repo is unmaintained and hasn't been updated in a long time. It worked pretty
decently, but I haven't needed it for a while.

Architecturally, the API/ABI to the Slurm libraries was not very stable, and was
pretty hard to use. It would probably be better to develop a crate that invokes
the Slurm CLI programs under the hood, rather than trying to link against the
shared libraries.


# slurm-rs: slurm and slurmplus

Rust bindings for the [Slurm workload manager](https://slurm.schedmd.com/),
and a command-line program (`slurmplus`) that provides some useful
functionality.

[![](http://meritbadge.herokuapp.com/slurm)](https://crates.io/crates/slurm)
[![](https://docs.rs/slurm/badge.svg)](https://docs.rs/slurm)

- [slurm-sys on crates.io](https://crates.io/crates/slurm-sys)
- [slurm on crates.io](https://crates.io/crates/slurm)
- [slurmplus on crates.io](https://crates.io/crates/slurmplus)
- [Rust API documentation for the slurm crate](https://docs.rs/slurm)

The coverage of the underlying Slurm feature set is far from complete, but the
basic framework is in place.

For a summary of recent changes to the code, see
[CHANGELOG.md](./CHANGELOG.md) for the command-line tool,
[slurm/CHANGELOG.md](slurm/CHANGELOG.md) for the developer-facing library, and
[slurm-sys/CHANGELOG.md](slurm-sys/CHANGELOG.md) for the low-level FFI
bindings.


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
