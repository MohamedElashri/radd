#!/bin/sh
set -eu

repo="${RADD_REPO:-MohamedElashri/radd}"
version="${RADD_VERSION:-latest}"
install_dir="${RADD_INSTALL_DIR:-$HOME/.local/bin}"
hadd_name="${RADD_HADD:-hadd}"
color_mode="${RADD_COLOR:-auto}"

reset=""
red=""
green=""
yellow=""
cyan=""

case "${NO_COLOR:-}:${color_mode}" in
    *:never | ?*:*) ;;
    *:always)
        reset="$(printf '\033[0m')"
        red="$(printf '\033[31m')"
        green="$(printf '\033[32m')"
        yellow="$(printf '\033[33m')"
        cyan="$(printf '\033[36m')"
        ;;
    *:auto | *:*)
        if [ -t 1 ] && [ "${TERM:-}" != "dumb" ]; then
            reset="$(printf '\033[0m')"
            red="$(printf '\033[31m')"
            green="$(printf '\033[32m')"
            yellow="$(printf '\033[33m')"
            cyan="$(printf '\033[36m')"
        fi
        ;;
esac

timestamp() {
    date '+%H:%M:%S'
}

log() {
    level="$1"
    color="$2"
    shift 2
    printf '[%s] %b%-5s%b %s\n' "$(timestamp)" "$color" "$level" "$reset" "$*"
}

info() {
    log "INFO" "$cyan" "$@"
}

ok() {
    log "OK" "$green" "$@"
}

warn() {
    log "WARN" "$yellow" "$@"
}

error() {
    log "ERROR" "$red" "$@" >&2
}

print_output() {
    printf '%s\n' "$1" | while IFS= read -r line; do
        printf '        %s\n' "$line"
    done
}

find_executable() {
    case "$1" in
        */*)
            if [ -x "$1" ]; then
                printf '%s\n' "$1"
                return 0
            fi
            ;;
        *)
            command -v "$1" 2>/dev/null && return 0
            ;;
    esac

    return 1
}

check_root_tools() {
    info "Checking ROOT runtime tools"

    if hadd_path="$(find_executable "$hadd_name")"; then
        ok "hadd: found at ${hadd_path}"
        if "$hadd_path" -h >/dev/null 2>&1; then
            ok "hadd help: available"
        else
            warn "hadd was found, but '${hadd_name} -h' did not complete successfully"
        fi
    else
        case "$hadd_name" in
            */*) warn "hadd: not executable at ${hadd_name}" ;;
            *) warn "hadd: not found on PATH" ;;
        esac
        warn "Set RADD_HADD=/path/to/hadd for setup checks, or pass --hadd when running radd"
    fi

    if root_config_path="$(find_executable root-config)"; then
        ok "root-config: found at ${root_config_path}"
        root_version="$("$root_config_path" --version 2>/dev/null || true)"
        if [ -n "$root_version" ]; then
            ok "ROOT version: ${root_version}"
        fi
    else
        warn "root-config: not found on PATH"
    fi

    if root_path="$(find_executable root)"; then
        ok "root: found at ${root_path}"
    else
        warn "root: not found on PATH"
    fi
}

run_doctor() {
    info "Running radd doctor"

    if doctor_output="$("${install_dir}/radd" doctor --hadd "$hadd_name" 2>&1)"; then
        print_output "$doctor_output"
        ok "ROOT and hadd checks passed"
    else
        print_output "$doctor_output"
        warn "radd installed, but ROOT setup needs attention before real merges"
    fi
}

usage() {
    cat <<'EOF'
Install radd from GitHub Releases.

Environment variables:
  RADD_REPO         GitHub repository, default: MohamedElashri/radd
  RADD_VERSION      Release tag such as v0.1.0, default: latest
  RADD_INSTALL_DIR  Install directory, default: $HOME/.local/bin
  RADD_HADD         hadd executable for setup checks, default: hadd
  RADD_COLOR        Color output: auto, always, or never

Examples:
  curl -fsSL https://melashri.net/radd/install.sh | sh
  RADD_VERSION=v0.1.0 sh install.sh
  RADD_INSTALL_DIR=/usr/local/bin sh install.sh
  RADD_HADD=/opt/root/bin/hadd sh install.sh
EOF
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    usage
    exit 0
fi

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        error "required command not found: $1"
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
    error "required command not found: shasum or sha256sum"
    exit 1
fi

case "$(uname -s)" in
    Linux) os="linux" ;;
    Darwin) os="macos" ;;
    *)
        error "unsupported operating system: $(uname -s)"
        exit 1
        ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) arch="amd64" ;;
    arm64 | aarch64) arch="arm64" ;;
    *)
        error "unsupported CPU architecture: $(uname -m)"
        exit 1
        ;;
esac

info "Detected platform: ${os}-${arch}"
check_root_tools

if [ "$version" = "latest" ]; then
    info "Resolving latest release for ${repo}"
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

info "Downloading ${archive} from ${repo} ${tag}"
curl -fsSLo "${tmp_dir}/${archive}" "${base_url}/${archive}"
curl -fsSLo "${tmp_dir}/${checksum}" "${base_url}/${checksum}"

(
    cd "$tmp_dir"
    $checksum_cmd "$checksum"
)
ok "Checksum verified"

info "Extracting ${archive}"
tar -xzf "${tmp_dir}/${archive}" -C "$tmp_dir"
info "Installing radd to ${install_dir}/radd"
install -d "$install_dir"
install -m 0755 "${tmp_dir}/${package}/radd" "${install_dir}/radd"

installed_version="$("${install_dir}/radd" --version)"
ok "Installed ${installed_version} to ${install_dir}/radd"

case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
        warn "${install_dir} is not on PATH"
        warn "Add it to PATH before running radd from a new shell"
        ;;
esac

run_doctor
info "ROOT is not bundled; install ROOT separately when needed for real merges"
