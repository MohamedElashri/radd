+++
title = "Getting Started"
description = "Build radd, check ROOT hadd, and run a first merge."
weight = 10
template = "page"
+++

`radd` keeps the first workflow close to familiar `hadd`: choose an output, pass ROOT inputs, and let the tool plan a staged merge around stock ROOT.

## Requirements

You need:

- a recent stable Rust toolchain for source builds
- ROOT installed separately
- `hadd` available on `PATH`, or a path supplied with `--hadd`

Check the ROOT side first:

```bash
hadd -h
```

## Build from Source

From a checkout:

```bash
cargo build --release
target/release/radd --version
```

Put `target/release/radd` somewhere on your `PATH`.

## Check the Environment

Run:

```bash
radd doctor
```

`doctor` checks whether `hadd` can be found and invoked, reports `root-config` when available, and checks whether the current directory and temporary directory are writable.

Use a custom `hadd` path when needed:

```bash
radd doctor --hadd /opt/root/bin/hadd
```

## First Merge

Merge direct input files:

```bash
radd merge out.root input1.root input2.root --jobs 4
```

Merge from a manifest:

```bash
radd merge out.root @inputs.txt --jobs 8 --scratch /local/scratch/radd
```

Preview the plan before running:

```bash
radd plan out.root @inputs.txt --jobs 8 --chunk-count 8 --commands
```

Run a dry run through the merge command:

```bash
radd merge out.root @inputs.txt --jobs 8 --dry-run
```

Dry runs do not create scratch directories and do not execute `hadd`.

## Existing Outputs

`radd merge` refuses to overwrite an existing output unless you pass `--force`:

```bash
radd merge out.root @inputs.txt --force
```

An output path that is also one of the input files is always rejected.
