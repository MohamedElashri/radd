+++
title = "Scratch, Cache, and Staging"
description = "Choose scratch space, reuse cacheable chunks, and stage inputs locally."
weight = 40
template = "page"
+++

`radd` uses scratch space for intermediate partial outputs. Choose a fast local filesystem when merging many files.

```bash
radd merge out.root @inputs.txt --jobs 8 --scratch /local_nvme/radd
```

## Temporary Outputs

Successful validated merges remove temporary partial outputs. On `hadd` failure or validation failure, temporary files are preserved for debugging.

If a plan only needs one `hadd` job, the job writes directly to the requested output and no partial output is needed.

## Input Staging

Input staging is opt-in:

```bash
radd merge out.root @inputs.txt \
  --jobs 8 \
  --scratch /local_nvme/radd \
  --stage-inputs
```

Staging hardlinks inputs into scratch when possible and falls back to copying. Staged file sizes are verified before `hadd` runs.

By default, staged inputs are removed after a successful validated merge:

```bash
radd merge out.root @inputs.txt --stage-inputs
```

Keep staged inputs for reuse or inspection:

```bash
radd merge out.root @inputs.txt --stage-inputs --keep-staged-inputs
```

Staging refuses to overwrite existing staged paths. If a previous run kept staged inputs, remove that directory or choose a different scratch directory.

## Cache

The cache is disabled by default. Enable it when repeated first-stage chunk merges are useful:

```bash
radd merge out.root @inputs.txt --jobs 8 --chunk-count 8 --cache
```

Cache entries live under:

1. `RADD_CACHE_DIR`, when set
2. `$XDG_CACHE_HOME/radd`, when set
3. `$HOME/.cache/radd`
4. a temporary-directory fallback

Inspect and clean the managed cache:

```bash
radd cache list
radd cache clean
```

Cache keys include input paths, sizes, modification times, `radd` version, detected ROOT or `hadd` version when available, merge policy, and `hadd`-relevant flags.

Only first-stage scratch partials are cached. Final outputs are never cached.
