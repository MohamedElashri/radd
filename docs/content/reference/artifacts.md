+++
title = "Artifacts"
description = "Reproducibility manifests, command logs, and JSON output."
weight = 20
template = "page"
+++

`radd` can write audit artifacts during `merge` and dry-run workflows.

## Command Log

```bash
radd merge out.root @inputs.txt --command-log radd-commands.jsonl
```

The command log is JSON Lines. Each record contains the planned stage level, job id, output path, and exact `hadd` argv vector.

Command logs describe the planned executable commands. With cache enabled, a cached first-stage job may be skipped at execution time even though its planned command remains present in the log.

## Reproducibility Manifest

```bash
radd merge out.root @inputs.txt --manifest radd-manifest.json
```

The manifest records:

- `radd` version
- resolved input paths, sizes, and modification times
- merge options
- detected `hadd` or ROOT version when available
- plan topology
- exact command records
- input-staging summary when relevant

Manifest and command-log paths are overwritten if they already exist.

## Merge Telemetry JSON

```bash
radd merge out.root @inputs.txt --json
```

Merge telemetry includes:

- `radd_version`
- `hadd_path`
- `hadd_version`
- start and end times
- elapsed seconds
- input counts and byte totals
- output path and output size
- scratch directory
- policy, jobs, fan-in, stage count, and command count
- cache hit and miss counters
- dry-run status
- input-staging summary when relevant

Failed executions currently return ordinary errors instead of writing structured failure telemetry.

## Benchmark JSON

```bash
radd bench @inputs.txt --jobs-candidates 1,2,4,8 --json
```

Benchmark JSON includes candidate job counts, elapsed seconds, throughput, output sizes, and recommended merge flags.
