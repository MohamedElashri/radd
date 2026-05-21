+++
title = "CLI Reference"
description = "Commands and flags exposed by the radd command line."
weight = 10
template = "page"
+++

`radd` exposes one binary with subcommands for checking the environment, updating the installed binary, planning merges, executing merges, validating outputs, inspecting inputs, benchmarking settings, and managing cache entries.

## Global Flags

- `--verbose`: increase diagnostic output
- `-q`, `--quiet`: reduce diagnostic output
- `-h`, `--help`: print help
- `-v`, `-V`, `--version`: print version

## version

```bash
radd version
```

Prints the `radd` package version. The same version is available through `radd --version`, `radd -v`, and `radd -V`.

## doctor

```bash
radd doctor [--hadd hadd]
```

Checks whether `hadd` can be found and invoked, whether `root-config` is available, and whether important directories are writable.

Flags:

- `--hadd PATH`: executable name or path to check

## update

```bash
radd update
radd update --check-only
radd update --yes
```

Checks GitHub Releases for a newer `radd` release matching the current OS and CPU. When an update is available, `radd update` asks for confirmation before downloading the release archive, verifying its SHA-256 checksum, extracting it, and replacing the current executable.

Flags:

- `--check-only`: report whether an update is available without downloading it
- `--yes`: accept the update prompt
- `--target TAG`: install a specific release tag instead of resolving the latest release
- `--repo owner/repo`: check a fork or mirror; defaults to `RADD_REPO` or `MohamedElashri/radd`
- `--install-path PATH`: replace a specific binary path instead of the running executable

## plan

```bash
radd plan [OPTIONS] out.root input1.root input2.root
radd plan [OPTIONS] out.root @inputs.txt
```

Builds and prints a merge plan without executing `hadd`.

Common flags:

- `--jobs N`: number of merge jobs to plan for
- `--chunk-count N`: number of first-level chunks
- `--fan-in N`: number of partial outputs to merge in one tree job
- `--scratch DIR`: scratch directory for intermediate partial outputs
- `--policy fastest|balanced|smallest|reproducible`
- `--json`: emit the plan as JSON
- `--commands`: include exact planned `hadd` argv vectors

`hadd` command flags:

- `--hadd PATH`
- `--keep-going`
- `--hadd-jobs N`
- `--max-open-files N`
- `--no-trees`
- `--only OBJECT`
- `--skip OBJECT`

## merge

```bash
radd merge [OPTIONS] out.root input1.root input2.root
radd merge [OPTIONS] out.root @inputs.txt
```

Builds a plan, runs `hadd` subprocesses stage by stage, validates the output by default, and cleans successful temporary files.

Planning and execution flags:

- `--jobs N`
- `--chunk-count N`
- `--fan-in N`
- `--scratch DIR`
- `--policy fastest|balanced|smallest|reproducible`
- `--dry-run`
- `--force`
- `--cache`
- `--stage-inputs`
- `--keep-staged-inputs`

Audit and output flags:

- `--json`
- `--manifest PATH`
- `--command-log PATH`
- `--no-validate`

`hadd` command flags:

- `--hadd PATH`
- `--keep-going`
- `--hadd-jobs N`
- `--max-open-files N`
- `--no-trees`
- `--only OBJECT`
- `--skip OBJECT`

## validate

```bash
radd validate out.root
```

Runs basic output validation.

## inspect

```bash
radd inspect input1.root input2.root
radd inspect @inputs.txt
radd inspect --root-metadata @inputs.txt
```

Flags:

- `--root-metadata`: attempt optional ROOT-backed metadata inspection
- `--root PATH`: ROOT executable name or path

## bench

```bash
radd bench [OPTIONS] @inputs.txt
```

Benchmarks candidate `radd` job counts with scratch-only outputs.

Flags:

- `--jobs-candidates LIST`
- `--sample-size N`
- `--scratch DIR`
- `--fan-in N`
- `--policy fastest|balanced|smallest|reproducible`
- `--json`
- `--keep-bench-files`
- `--hadd PATH`
- `--keep-going`
- `--hadd-jobs N`
- `--max-open-files N`
- `--no-trees`
- `--only OBJECT`
- `--skip OBJECT`

## cache

```bash
radd cache list
radd cache clean
```

Lists or clears managed partial-merge cache files.
