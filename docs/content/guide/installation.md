+++
title = "Installation"
description = "Install radd from a GitHub release archive or build it from source."
weight = 5
template = "page"
+++

`radd` is distributed as a single binary. ROOT is not bundled. Install ROOT separately and make sure `hadd` is available on `PATH`, or pass a custom executable with `--hadd`.

## Install from a Release

The fastest path is the install script:

```bash
curl -fsSL https://melashri.net/radd/install.sh | sh
radd doctor
```

By default, the script installs the latest release for your OS and CPU to `$HOME/.local/bin`.

Override the release or install directory with environment variables:

```bash
curl -fsSL https://melashri.net/radd/install.sh | \
  RADD_VERSION=v0.1.0 RADD_INSTALL_DIR=/usr/local/bin sh
```

If you are installing from a fork or mirror, set `RADD_REPO`:

```bash
curl -fsSL https://melashri.net/radd/install.sh | \
  RADD_REPO=owner/repo sh
```

## Manual Release Install

Download the archive and checksum for your platform from the project's GitHub Releases page.

Release archives are named by version and platform:

- `radd-vX.Y.Z-linux-amd64.tar.gz`
- `radd-vX.Y.Z-linux-arm64.tar.gz`
- `radd-vX.Y.Z-macos-amd64.tar.gz`
- `radd-vX.Y.Z-macos-arm64.tar.gz`

Verify and install:

```bash
shasum -a 256 -c radd-v0.1.0-linux-amd64.tar.gz.sha256
tar -xzf radd-v0.1.0-linux-amd64.tar.gz
install -d "$HOME/.local/bin"
install -m 0755 radd-v0.1.0-linux-amd64/radd "$HOME/.local/bin/radd"
```

Make sure `~/.local/bin` is on your `PATH`, then check the binary:

```bash
radd --version
radd doctor
```

On macOS or arm64 Linux, use the matching archive name in the same commands. Release binaries are not a substitute for ROOT; `radd doctor` should still find a working `hadd`.

## Build from Source

Install a recent stable Rust toolchain, then build from a checkout:

```bash
cargo build --release
target/release/radd --version
```

Install the built binary wherever your shell can find it:

```bash
install -d "$HOME/.local/bin"
install -m 0755 target/release/radd "$HOME/.local/bin/radd"
```

You can also let Cargo build and install from the checkout:

```bash
cargo install --path .
```

## Check ROOT

Before a real merge, check the ROOT side:

```bash
hadd -h
radd doctor
```

Use a custom `hadd` path when needed:

```bash
radd doctor --hadd /opt/root/bin/hadd
```
