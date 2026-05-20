# radd

`radd` is a Rust command-line tool for organizing and accelerating ROOT file
merges by orchestrating the installed ROOT `hadd` executable.

The first release is intentionally a frontend around `hadd`. It does not parse
ROOT files, merge ROOT objects itself, link against ROOT C++ libraries, or
bundle ROOT. You will need a working ROOT installation with `hadd` available on
your system.

Current status: early CLI implementation. `radd doctor`, `radd inspect`, and
minimal input resolution for `radd plan` are available.
