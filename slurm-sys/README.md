# slurm-sys

This crate provides low-level bindings to the `libslurm` and `libslurmdb`
libraries associated with the [Slurm](https://slurm.schedmd.com/) workload
manager.

## Building

You must have a working version of
[rustfmt](https://github.com/rust-lang-nursery/rustfmt) installed in order to
build this crate correctly! To handle the evolving Slurm C API, this crate's
build script parses the output of `bindgen` in a simplistic manner. Without
`rustfmt`, the code is not formatted in a way that the build script can
handle.

By default, this crate's build script will use a
[pkg-config](https://www.freedesktop.org/wiki/Software/pkg-config/) search for
`slurm` to determine the necessary library and include search paths. Not all
Slurm installs come with a `pkg-config` file, however. If that is the case for
you, set the environment variables `SLURM_LIBDIR` and, optionally,
`SLURM_INCDIR` to point to the directories containing the Slurm shared
libraries and include files, respectively. In particular, these variables
should be set such that the files `$SLURM_LIBDIR/libslurm.so` and
`$SLURM_INCDIR/slurm/slurm.h` exist.


## Licensing

Licensed under the MIT License.
