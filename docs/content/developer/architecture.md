+++
title = "Architecture"
description = "How radd is organized as a safe frontend around hadd."
weight = 10
template = "page"
+++

`radd` is one Rust binary. The v1 backend is stock ROOT `hadd` invoked as a subprocess.

The implementation avoids:

- ROOT C++ linking
- ROOT binary parsing
- direct ROOT object merging
- `unsafe` Rust
- shell command strings

## Module Map

`src/main.rs`
: process entry point and exit-code handling

`src/cli.rs`
: clap command definitions, command dispatch, and workflow orchestration

`src/input.rs`
: direct input and `@manifest` resolution

`src/planner.rs`
: size-balanced chunking and staged merge-tree construction

`src/hadd.rs`
: `hadd` argv construction, object-selection support, capability detection, and version probing

`src/executor.rs`
: bounded per-stage subprocess execution and temporary output cleanup

`src/cache.rs`
: first-stage partial cache keying, validation, reuse, publishing, listing, and cleaning

`src/staging.rs`
: scratch preparation and optional input staging

`src/telemetry.rs`
: merge telemetry, reproducibility manifests, and command logs

`src/validate.rs`
: output validation

`src/inspect.rs`
: input summaries and optional ROOT-backed metadata inspection

`src/bench.rs`
: benchmark sampling, candidate execution, throughput, and recommendation logic

## Command Flow

Most workflows follow this shape:

1. Parse CLI arguments.
2. Resolve input files.
3. Build a merge plan.
4. Build explicit `hadd` commands.
5. Prepare scratch and optional input staging.
6. Prepare cache hits and misses when enabled.
7. Execute stages with bounded concurrency.
8. Validate the final output unless skipped.
9. Write requested artifacts.
10. Clean temporary files after success.

`plan` stops after planning and optional command construction. `merge --dry-run` goes through merge setup far enough to produce telemetry and artifacts without creating scratch or running `hadd`.
