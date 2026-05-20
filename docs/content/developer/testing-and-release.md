+++
title = "Testing and Release"
description = "Run the checks and understand how fake ROOT commands keep CI lightweight."
weight = 40
template = "page"
+++

The release checks are:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Run them before publishing changes.

The repo-level Makefile wraps the same checks and the documentation build:

```bash
make check
```

Useful local targets include:

```bash
make test
make docs
make docs-serve
make release
make run ARGS="--help"
```

## Test Shape

The suite has two layers:

- unit tests inside modules for planners, command builders, cache keys, validation, metadata parsing, and benchmark logic
- integration tests in `tests/cli_smoke.rs` for CLI behavior

The integration suite uses fake `hadd` and fake `root` commands. CI does not require ROOT.

## Fake hadd

The fake `hadd` script can:

- print help text with selected capabilities
- print a fake version
- record argv
- create output files
- create empty output files for validation tests
- fail selected outputs
- sleep for benchmark tests

This lets the test suite cover subprocess orchestration without needing ROOT fixtures.

## Fake root

The fake `root` command prints marker-delimited JSON for metadata-inspection tests. The parser is tested separately from ROOT startup behavior.

## CI

GitHub Actions runs formatting, clippy, and tests on Linux and macOS with stable Rust.

## Release Build

Build:

```bash
cargo build --release
```

The release profile uses thin LTO, one codegen unit, and symbol stripping.

Release packages should ship the single `radd` binary and document that ROOT remains an external runtime dependency.
