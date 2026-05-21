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
make root-test
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

## Real ROOT Integration Tests

Run the live ROOT suite when you want end-to-end coverage against real ROOT files:

```bash
make root-test
```

This sets `RADD_REAL_ROOT_TESTS=1` and runs `tests/real_root.rs`. The tests require
both `root` and `hadd` on `PATH`. They generate small ROOT files in a temporary
directory, inspect them through ROOT-backed metadata, merge them through the real
`hadd`, validate the merged output, and run `radd bench` against the generated
fixtures.

The generated files contain a real `Events` `TTree` and `Counts` histogram, so the
suite verifies more than file existence. A ROOT macro reopens the merged output
and checks the expected tree and histogram entries.

The live suite is intentionally opt-in. Normal `cargo test` remains fast and does
not require a ROOT installation, while release candidates can be checked with:

```bash
cargo test
make root-test
```

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

GitHub Actions CI runs formatting, clippy, and tests on Linux with the pinned repo toolchain from `rust-toolchain.toml`. The release workflow uses the same toolchain and builds the Linux and macOS release artifacts.

## Release Workflow

The release workflow runs when a tag matching `v*` is pushed. It performs the release checks, builds optimized Linux and macOS binaries for amd64 and arm64, smoke-tests the version commands, packages archives with a short bundled README, writes SHA-256 checksum files, uploads the packages as workflow artifacts, and publishes them to the GitHub release for the tag.

It can also be started manually with `workflow_dispatch`; manual runs build and upload artifacts but only tagged runs publish a GitHub release.

Release artifacts are named:

- `radd-vX.Y.Z-linux-amd64.tar.gz`
- `radd-vX.Y.Z-linux-arm64.tar.gz`
- `radd-vX.Y.Z-macos-amd64.tar.gz`
- `radd-vX.Y.Z-macos-arm64.tar.gz`

Each archive also has a matching `.sha256` file.

Before tagging, verify the package version:

```bash
radd --version
radd -v
radd -V
radd version
```

## Release Build

Build locally:

```bash
cargo build --locked --release
```

The release profile uses thin LTO, one codegen unit, and symbol stripping.

Release packages ship the single `radd` binary and a small archive README. ROOT remains an external runtime dependency and is checked by `radd doctor`.
