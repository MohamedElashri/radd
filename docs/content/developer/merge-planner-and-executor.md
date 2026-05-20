+++
title = "Planner and Executor"
description = "How merge plans become staged hadd subprocesses."
weight = 20
template = "page"
+++

Planning and execution are deliberately separated. The planner creates deterministic data. The executor consumes explicit commands.

## Size-Balanced Chunks

The first stage uses the Longest Processing Time heuristic:

1. Sort inputs by descending size.
2. Create the requested number of buckets.
3. Place each input in the currently lightest bucket.
4. Drop empty buckets.
5. Create one first-stage job per nonempty bucket.

The planner breaks ties by stable paths and bucket indexes so results are reproducible.

## Merge Tree

After first-stage chunking, later stages group partial outputs by `fan_in`.

If a group is the final group, its job writes to the requested output. Otherwise it writes a scratch partial named like:

```text
radd-stage-<level>-job-<job>.root
```

## Command Construction

`hadd.rs` converts a `MergeJob` into an argv vector:

```text
hadd -f -fk out.root input-a.root input-b.root
```

Optional flags are inserted before the output path:

- `-j` for `--hadd-jobs`
- `-d` for temporary directory when `--hadd-jobs` is used
- `-k` for `--keep-going`
- `-n` for `--max-open-files`
- `-T` for `--no-trees`
- `-L` and `-Ltype` for object lists

The display formatter quotes paths for readability only. Execution uses argv directly.

## Stage Execution

The executor runs one stage at a time. Within a stage, it uses a bounded number of worker threads controlled by `--jobs`.

If any job fails, the stage stops and later stages do not start.

After a stage succeeds, the executor verifies that every expected output exists and is a file.

## Cleanup

Temporary partial outputs are derived from the plan by selecting jobs whose output is not the requested final output.

Cleanup happens after successful execution and validation. Failure paths preserve temporary files.
