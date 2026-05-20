+++
title = "Contributing"
description = "Practical guidance for changing radd safely."
weight = 50
template = "page"
+++

Keep changes small, explicit, and testable.

## Safety Rules

Do not introduce:

- `unsafe` Rust
- shell command execution through `sh -c`
- ROOT C++ linking in the default build
- ROOT binary parsing
- implicit destructive behavior

Build subprocess calls through explicit argv vectors.

## Where to Change Things

Add or change public CLI flags in `src/cli.rs`, then update:

- CLI parsing tests
- smoke tests for behavior
- docs in `docs/content/reference/cli.md`
- user-facing examples when the workflow changes

Change planning behavior in `src/planner.rs`, then add or update deterministic planner tests.

Change `hadd` flags in `src/hadd.rs`, then update command-construction tests and command-log expectations.

Change cache key material in `src/cache.rs`, then add tests showing when keys stay stable and when they change.

## Documentation Rules

The root `README.md` should stay short and user-facing.

The `docs/` tree is for fuller user and developer documentation. Avoid private notes or implementation planning in public docs.

## Before Opening a PR

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Also run a small manual smoke when possible:

```bash
radd doctor
radd plan out.root @inputs.txt --jobs 2 --commands
```

If ROOT is available locally, run one small real merge with disposable inputs.
