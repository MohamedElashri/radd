+++
title = "Manifests and Inputs"
description = "Use direct ROOT inputs or @manifest files safely."
weight = 20
template = "page"
+++

`radd` accepts direct file arguments and manifest arguments. A manifest argument starts with `@`.

```bash
radd inspect input1.root input2.root
radd inspect @inputs.txt
```

## Manifest Format

A manifest is a plain text file:

```text
# comments and blank lines are ignored
/data/run-a.root
/data/run-b.root
relative/run-c.root
```

Blank lines are ignored. Lines whose first non-whitespace character is `#` are ignored.

Relative paths are resolved from the current working directory, not from the manifest file's directory.

## Duplicate Inputs

Resolved input paths are canonicalized. If the same file appears twice, `radd` returns an error instead of silently deduplicating:

```text
duplicate input file: ./a.root was already listed as a.root
```

This is deliberate. Repeated ROOT inputs can be meaningful in some workflows, so v1 avoids guessing.

## Unsupported Nested Manifests

Manifest entries that begin with `@` are rejected. Keep generated input lists flat.

## Inspect Before Merging

Run:

```bash
radd inspect @inputs.txt
```

This prints the resolved input count and total bytes. Add ROOT-backed metadata inspection when ROOT is available and you want top-level keys, trees, compression metadata, and UUIDs:

```bash
radd inspect --root-metadata @inputs.txt
```

ROOT metadata inspection is best-effort. A ROOT startup or parsing failure is reported as a warning and does not prevent basic inspection.
