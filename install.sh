#!/bin/sh
set -eu

repo="${RADD_REPO:-MohamedElashri/radd}"
version="${RADD_VERSION:-latest}"
install_dir="${RADD_INSTALL_DIR:-$HOME/.local/bin}"

usage() {
    cat <<'EOF'
Install radd from GitHub Releases.

Environment variables:
  RADD_REPO         GitHub repository, default: MohamedElashri/radd
  RADD_VERSION      Release tag such as v0.1.0, default: latest
  RADD_INSTALL_DIR  Install directory, default: $HOME/.local/bin

Examples:
  curl -fsSL https://raw.githubusercontent.com/MohamedElashri/radd/main/install.sh | sh
  RADD_VERSION=v0.1.0 sh install.sh
  RADD_INSTALL_DIR=/usr/local/bin sh install.sh
EOF
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    usage
    exit 0
fi

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: required command not found: $1" >&2
        exit 1
    fi
}

need curl
need tar

if command -v shasum >/dev/null 2>&1; then
    checksum_cmd="shasum -a 256 -c"
elif command -v sha256sum >/dev/null 2>&1; then
    checksum_cmd="sha256sum -c"
else
    echo "error: required command not found: shasum or sha256sum" >&2
    exit 1
fi

case "$(uname -s)" in
    Linux) os="linux" ;;
    Darwin) os="macos" ;;
    *)
        echo "error: unsupported operating system: $(uname -s)" >&2
        exit 1
        ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) arch="amd64" ;;
    arm64 | aarch64) arch="arm64" ;;
    *)
        echo "error: unsupported CPU architecture: $(uname -m)" >&2
        exit 1
        ;;
esac

if [ "$version" = "latest" ]; then
    latest_url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${repo}/releases/latest")"
    tag="${latest_url##*/}"
else
    tag="$version"
fi

case "$tag" in
    v*) ;;
    [0-9]*) tag="v${tag}" ;;
esac

release_version="${tag#v}"
package="radd-v${release_version}-${os}-${arch}"
archive="${package}.tar.gz"
checksum="${archive}.sha256"
base_url="https://github.com/${repo}/releases/download/${tag}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/radd-install.XXXXXX")"
cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

echo "Downloading ${archive} from ${repo} ${tag}"
curl -fsSLo "${tmp_dir}/${archive}" "${base_url}/${archive}"
curl -fsSLo "${tmp_dir}/${checksum}" "${base_url}/${checksum}"

(
    cd "$tmp_dir"
    $checksum_cmd "$checksum"
)

tar -xzf "${tmp_dir}/${archive}" -C "$tmp_dir"
install -d "$install_dir"
install -m 0755 "${tmp_dir}/${package}/radd" "${install_dir}/radd"

echo "Installed radd to ${install_dir}/radd"
"${install_dir}/radd" --version

case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
        echo "Note: ${install_dir} is not on PATH."
        echo "Add it to PATH before running radd from a new shell."
        ;;
esac

echo "ROOT is not bundled. Run 'radd doctor' after installing ROOT or setting --hadd."
