+++
title = "Safety Semantics"
description = "What radd refuses, preserves, overwrites, and cleans."
weight = 40
template = "page"
+++

`radd` is a frontend around stock ROOT `hadd`, but it adds explicit safety checks around planning and execution.

## No Shell Execution

`radd` builds explicit argv vectors and runs subprocesses with `std::process::Command`. It does not execute through `sh -c`.

## Output Overwrite

`radd merge` refuses to overwrite an existing output unless `--force` is passed.

```bash
radd merge out.root @inputs.txt --force
```

Even with `--force`, `radd` refuses:

- output paths that are also input files
- symlink output paths
- existing output paths that are not regular files

`plan` and `merge --dry-run` are non-destructive and do not require `--force`.

## Temporary Files

Successful validated merges remove temporary partial outputs and object-selection list files.

On `hadd` failure or validation failure, temporary files are preserved for debugging.

## Validation

`merge` runs basic validation by default. The output must exist, be a regular file, and be nonempty.

Skip validation explicitly:

```bash
radd merge out.root @inputs.txt --no-validate
```

## Keep Going

`--keep-going` passes `-k` to `hadd`. `radd` still stops the staged merge if required stage outputs are missing.

## Cache

Cache reuse copies cached chunks into scratch paths. Final outputs are not cached.

Corrupt or incomplete cache entries are rebuilt.
