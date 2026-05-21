+++
title = "Validation, Inspection, and Benchmarking"
description = "Check outputs, inspect ROOT metadata, and benchmark candidate job counts."
weight = 50
template = "page"
+++

`radd` includes a few commands that help you understand inputs and outputs without changing merge behavior.

## Validate Outputs

Run:

```bash
radd validate out.root
```

The v1 validation level is basic:

- the output path exists
- the output is a regular file
- the output is nonempty

`radd merge` runs this basic validation by default after successful `hadd` execution and before scratch cleanup.

Skip validation only when it is not useful for your workflow:

```bash
radd merge out.root @inputs.txt --no-validate
```

## Inspect Inputs

Basic inspection:

```bash
radd inspect @inputs.txt
```

ROOT-backed metadata inspection:

```bash
radd inspect --root-metadata @inputs.txt
```

Use `--root` to select a specific external ROOT executable:

```bash
radd inspect --root-metadata --root /opt/root/bin/root @inputs.txt
```

ROOT-backed inspection runs external ROOT commands. It does not link `radd` to ROOT libraries.

## Benchmark Candidate Settings

Run:

```bash
radd bench @inputs.txt --jobs-candidates 1,2,4,8 --sample-size 8
```

Benchmark mode samples inputs deterministically, writes outputs only below scratch, measures elapsed time and throughput, and recommends a job count.

Use JSON output when feeding results into other tools:

```bash
radd bench @inputs.txt --jobs-candidates 1,2,4,8 --json
```

Keep benchmark scratch files for inspection:

```bash
radd bench @inputs.txt --keep-bench-files
```

Benchmark results are approximate. Machine load and filesystem behavior matter.

## Testing With Real ROOT Files

Developers can run the opt-in live suite before release or performance work:

```bash
make root-test
```

It requires `root` and `hadd` on `PATH`. The suite creates temporary ROOT files,
runs `radd inspect --root-metadata`, performs a real `hadd` merge through
`radd merge`, checks the merged `Events` tree with ROOT, and exercises
`radd bench` with JSON output.
