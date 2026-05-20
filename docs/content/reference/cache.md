+++
title = "Cache Reference"
description = "Cache root selection, key material, and invalidation behavior."
weight = 30
template = "page"
+++

The cache stores reusable first-stage partial merge outputs. It is opt-in.

```bash
radd merge out.root @inputs.txt --cache
```

## Cache Root

`radd` chooses the cache root in this order:

1. `RADD_CACHE_DIR`
2. `$XDG_CACHE_HOME/radd`
3. `$HOME/.cache/radd`
4. a temporary-directory fallback

The cache layout is:

```text
cache-root/
  chunks/
    <key>.root
  manifests/
    <key>.json
```

## Cacheable Jobs

Only first-stage jobs that write scratch partial outputs are cacheable. Final output jobs are never cached.

If a valid cache entry exists, `radd` copies the cached chunk to the planned scratch path and skips that first-stage `hadd` job.

## Key Material

Cache keys include:

- input paths
- input sizes
- input modification times
- merge policy
- `hadd`-relevant flags
- object-selection mode and names
- `radd` version
- detected `hadd` or ROOT version when available

If no version can be detected, version-aware invalidation cannot distinguish two ROOT builds at the same path.

## Validation

A cache entry is reused only when:

- the manifest exists and can be parsed
- the chunk file exists
- the chunk file is nonempty
- the chunk size matches the manifest

Invalid entries are rebuilt.

## Maintenance

Inspect entries:

```bash
radd cache list
```

Clean managed cache files:

```bash
radd cache clean
```

`cache clean` removes files from the managed `chunks/` and `manifests/` directories.
