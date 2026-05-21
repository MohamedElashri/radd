+++
title = "Getting Started"
description = "Check ROOT hadd and run a first merge."
weight = 10
template = "page"
+++

`radd` keeps the first workflow close to familiar `hadd`: choose an output, pass ROOT inputs, and let the tool plan a staged merge around stock ROOT.

## Requirements

You need:

- `radd` installed from a release archive or source checkout
- ROOT installed separately
- `hadd` available on `PATH`, or a path supplied with `--hadd`

See [Installation](../installation/) for release archives and source builds.

Check the installed `radd` binary:

```bash
radd --version
```

Check the ROOT side:

```bash
hadd -h
```

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
