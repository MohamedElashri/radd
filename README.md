# radd

`radd` is a safe Rust command-line frontend for merging ROOT files with the
installed ROOT `hadd` executable. It resolves input manifests, builds a
size-balanced staged merge plan, runs `hadd` with bounded concurrency, and can
write reproducible logs for later auditing.

`radd` does not replace ROOT. It does not parse ROOT files, merge ROOT objects
itself, link against ROOT C++ libraries, or bundle ROOT. A working ROOT
installation with `hadd` on `PATH` is required for real merges.

## Install

Build from a checkout:

```bash
cargo build --release
target/release/radd --version
```

Put `target/release/radd` somewhere on your `PATH`.

For source builds, install a recent stable Rust toolchain and make sure ROOT is
available in your shell:

```bash
hadd -h
radd doctor
```

Release packages can ship the single `radd` binary. They should document that
ROOT remains an external runtime dependency.

## Documentation

Full user and developer documentation lives in `docs/`. It is a Nida site:

```bash
nida build --site ./docs
```

The same workflow is available through Make:

```bash
make docs
make check
```

## Quick Start

Merge direct inputs:

```bash
radd merge out.root input1.root input2.root --jobs 4
```

Merge from a manifest:

```bash
radd merge out.root @inputs.txt --jobs 8 --scratch /local/scratch/radd
```

Preview the staged topology and exact `hadd` commands:

```bash
radd plan out.root @inputs.txt --jobs 8 --chunk-count 8 --commands
```

Run a dry run without creating scratch files or executing `hadd`:

```bash
radd merge out.root @inputs.txt --jobs 8 --dry-run
```

## Input Manifests

Manifest files are referenced with `@`:

```text
# comments and blank lines are ignored
/data/a.root
/data/b.root
relative/c.root
```

Relative manifest entries are resolved from the current working directory.
Duplicate resolved inputs are rejected so repeated files are never merged by
accident.

## Common Workflows

Inspect basic input metadata:

```bash
radd inspect @inputs.txt
```

Optionally ask ROOT for top-level keys, trees, UUIDs, and compression metadata:

```bash
radd inspect --root-metadata @inputs.txt
```

Validate an output with the built-in basic check:

```bash
radd validate out.root
```

Write audit artifacts during a merge:

```bash
radd merge out.root @inputs.txt \
  --jobs 8 \
  --manifest radd-manifest.json \
  --command-log radd-commands.jsonl \
  --json
```

The manifest records resolved inputs, options, plan stages, and exact `hadd`
argv vectors. The command log is JSON Lines.

## Scratch Space

`radd` writes staged partial outputs below the selected scratch directory and
removes them after a successful validated merge. On `hadd` or validation
failure, temporary files are preserved for debugging.

Use fast local storage when possible:

```bash
radd merge out.root @inputs.txt --jobs 8 --scratch /local_nvme/radd
```

Input staging is opt-in. It hardlinks inputs into scratch when possible and
falls back to copying:

```bash
radd merge out.root @inputs.txt \
  --jobs 8 \
  --scratch /local_nvme/radd \
  --stage-inputs
```

Use `--keep-staged-inputs` to retain staged inputs after success. If a previous
run kept staged files, remove that staged-input directory or choose a different
scratch directory before rerunning with the same inputs.

## Cache

The cache is disabled by default. Enable it when repeated first-stage partial
merges are useful:

```bash
radd merge out.root @inputs.txt --jobs 8 --chunk-count 8 --cache
```

Cached chunks live under `~/.cache/radd` by default, or under `RADD_CACHE_DIR`
when that environment variable is set. Inspect or clear the managed cache:

```bash
radd cache list
radd cache clean
```

Cache keys include input paths, input sizes, modification times, merge policy,
`hadd`-relevant flags, object-selection options, the `radd` version, and the
detected ROOT or `hadd` version when available.

## Object Selection

Skip TTrees:

```bash
radd merge out.root @inputs.txt --no-trees
```

Merge or skip selected top-level objects when the installed `hadd` supports
object lists:

```bash
radd merge out.root @inputs.txt --only DecayTree --only Events
radd merge out.root @inputs.txt --skip DebugTree
```

`--only` and `--skip` are mutually exclusive. The same options are accepted by
`plan`, `merge`, and `bench`.

## Benchmarking

Benchmark candidate radd job counts with scratch-only outputs:

```bash
radd bench @inputs.txt --jobs-candidates 1,2,4,8 --sample-size 8
```

Benchmark results are approximate and depend on current machine and filesystem
load. The recommendation can be fed back into a merge:

```bash
radd merge out.root @inputs.txt --jobs 4 --chunk-count 4
```

Use `--json` for machine-readable benchmark output and `--keep-bench-files` to
retain benchmark scratch files.

## Safety Notes

`radd` constructs subprocesses with explicit argv vectors and does not execute
through a shell. Successful merges run basic validation by default: the output
must exist, be a regular file, and be nonempty. Use `--no-validate` only when
that check is not useful for your workflow.

Existing output files are not overwritten unless you pass `--force`:

```bash
radd merge out.root @inputs.txt --force
```

`radd` refuses to use an input file as the output path even with `--force`.

`--keep-going` is passed to `hadd` as `-k`; it lets `hadd` continue past
recoverable input problems. `radd` still stops the staged merge tree if a
required stage output is missing.

`--jobs` controls radd-level stage concurrency. `--hadd-jobs` controls ROOT
`hadd -j` inside each subprocess. Using both can oversubscribe a machine, so
start conservatively.

## Development

Run the release checks:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The automated tests use fake `hadd` and fake `root` commands, so they do not
require ROOT. `radd doctor` is the quick local check for a real ROOT setup.
