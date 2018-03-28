# slurm-rs

Rust bindings for the [Slurm workload manager](https://slurm.schedmd.com/).

The API coverage is far from complete, but the basic framework is in place.


## Compatibility

At the moment, this crate will work if compiled against Slurm 17.11. It
*should* work with newer versions and *might* work with somewhat older
versions; however it is known not to work with version 17.02. This crate also
requires that the Slurm accounting database and `libslurmdb` library are
available. It would be great to relax these requirements, but the necessary
technical groundwork has not yet been laid. Contributions are welcome!


## Licensing

Licensed under the MIT License.
