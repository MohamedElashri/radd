+++
title = "Audit Artifacts"
description = "How telemetry, manifests, command logs, and cache data are produced."
weight = 30
template = "page"
+++

Audit artifacts are generated from the same plan and command records used for execution.

## Command Records

`telemetry::command_log_records` flattens executable stages into records. Each record stores:

- stage level
- job id
- output path
- exact argv vector

The command log writer serializes one record per line as JSON.

## Reproducibility Manifest

The manifest combines:

- resolved inputs
- options
- plan
- command records
- staging summary

It is designed for auditability rather than replaying automatically. Paths are stored as filesystem paths, and modification times are serialized as Unix seconds and nanoseconds when available.

## Telemetry

Telemetry is built after dry-run planning or after executed merge completion. It includes timing, output size, cache counters, and staging information.

Failure telemetry is not emitted yet. Execution errors currently return through `anyhow::Result`.

## Cache Manifests

Cache manifests are separate from reproducibility manifests. They exist to validate whether a cached first-stage chunk is safe to reuse.

Cache manifests include:

- cache schema version
- key
- output size
- input metadata
- cache-relevant options
- `radd` version
- detected `hadd` or ROOT version when available

The cache never trusts a chunk file without a matching readable manifest.
