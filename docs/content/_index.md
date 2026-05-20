+++
title = "radd Docs"
description = "User and developer documentation for the safe ROOT hadd frontend."
sort_by = "weight"
+++

`radd` is a safe Rust command-line frontend for merging ROOT files with the installed ROOT `hadd` executable. It resolves direct inputs and manifests, builds size-balanced staged merge plans, runs explicit `hadd` subprocess commands, and can write audit artifacts for reproducible workflows.

Use the guide when you want to run merges. Use the reference when you need exact command behavior. Use the developer section when you want to understand or change the implementation.

## Documentation Map

- [Guide](guide/): installation, first merge, scratch space, cache, inspection, validation, and benchmarking.
- [Reference](reference/): command-line flags, manifests, telemetry, cache behavior, and safety semantics.
- [Developer](developer/): architecture, planning and execution internals, testing, and release checks.

`radd` does not replace ROOT. It does not parse ROOT files, merge ROOT objects itself, link against ROOT C++ libraries, or bundle ROOT.
