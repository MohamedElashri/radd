+++
title = "Planning and Merging"
description = "Understand staged merge plans and the normal merge command."
weight = 30
template = "page"
+++

`radd` builds a merge plan before executing anything. The plan is deterministic for the same resolved inputs and options.

## Plan a Merge

```bash
radd plan out.root @inputs.txt --jobs 8 --chunk-count 8 --fan-in 8
```

The first stage uses size-balanced chunks. Later stages merge partial outputs until one final output remains.

Add `--commands` to see the exact `hadd` argv vectors:

```bash
radd plan out.root @inputs.txt --jobs 8 --commands
```

Use JSON output for tooling:

```bash
radd plan out.root @inputs.txt --json
```

## Run a Merge

```bash
radd merge out.root @inputs.txt \
  --jobs 8 \
  --chunk-count 8 \
  --scratch /local/scratch/radd
```

`--jobs` controls how many `hadd` subprocesses `radd` runs concurrently within one stage. A later stage starts only after all required outputs from the previous stage exist.

`--hadd-jobs` controls ROOT's internal `hadd -j` worker count for each subprocess. Using both `--jobs` and `--hadd-jobs` can oversubscribe a machine, so start conservatively.

## Merge Policies

The policy is recorded in plans, manifests, telemetry, and cache keys:

```bash
radd merge out.root @inputs.txt --policy fastest
```

Supported values are:

- `fastest`
- `balanced`
- `smallest`
- `reproducible`

Current v1 command construction preserves input compression with `hadd -fk` under all policies. The policy still matters for audit data and future-compatible workflows.

## Keep Going

```bash
radd merge out.root @inputs.txt --keep-going
```

`--keep-going` passes `-k` to `hadd`. It does not make `radd` ignore missing stage outputs. If a required partial output is not produced, the staged merge stops.
