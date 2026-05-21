# Changelog

All notable changes to `radd` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses semantic versioning.

## [0.1.0] - 2026-05-21

### Added

- Initial `radd` command-line frontend for orchestrating ROOT `hadd` merges.
- Merge planning with size-balanced chunks, configurable job counts, fan-in, scratch directories, and merge policies.
- Merge execution with bounded stage concurrency, dry-run mode, overwrite protection, and output/input collision checks.
- Manifest input support with comments, blank lines, relative paths, and duplicate input detection.
- `doctor`, `plan`, `merge`, `validate`, `inspect`, `bench`, and `cache` subcommands.
- Basic output validation and optional ROOT-backed input metadata inspection.
- Benchmark mode for sampling real inputs, timing candidate job counts, and reporting recommended merge flags.
- Optional object selection through `hadd` object-list flags with `--only` and `--skip`.
- Optional input staging into scratch via hardlinks or copies.
- Optional managed partial-merge cache with cache listing and cleanup commands.
- Reproducibility artifacts: merge manifest JSON and command log JSON Lines.
- Release packaging for Linux and macOS on amd64 and arm64.
- Nida-powered documentation site and GitHub Pages deployment workflow.
- Lightweight default test suite with fake `hadd`/`root` commands plus opt-in real ROOT integration tests through `make root-test`.

### Notes

- `radd` requires an external ROOT installation for real merges; it does not parse ROOT files, merge ROOT objects itself, link against ROOT C++ libraries, or bundle ROOT.

[0.1.0]: https://github.com/MohamedElashri/radd/releases/tag/v0.1.0
