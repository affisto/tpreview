#!/bin/sh
# Release installer: POSIX sh + curl or wget + tar + sha256sum or shasum.
# The default is pinned to this script's release, even through /releases/latest/.
TPREVIEW_DEFAULT_VERSION='v0.1.1'

main() {
    set -eu
    version=${TPREVIEW_VERSION:-$TPREVIEW_DEFAULT_VERSION}
    install_dir=${TPREVIEW_INSTALL_DIR:-}
    downloader=${TPREVIEW_DOWNLOADER:-auto}
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version|--bin-dir|--downloader)
                [ "$#" -ge 2 ] || die "Missing value for $1"
                case "$1" in
                    --version) version=$2 ;;
                    --bin-dir) install_dir=$2 ;;
                    --downloader) downloader=$2 ;;
                esac
                shift 2 ;;
            -h|--help)
                printf '%s\n' 'Install tpreview from GitHub Releases.' \
                    'Usage: sh install.sh [--version vX.Y.Z] [--bin-dir DIR] [--downloader auto|curl|wget]' \
                    'Default directory: $HOME/.local/bin. No sudo or shell-profile changes.' \
                    'Environment: TPREVIEW_VERSION, TPREVIEW_INSTALL_DIR, TPREVIEW_DOWNLOADER.'
                return ;;
            *) die "Unknown argument: $1 (use --help)" ;;
        esac
    done
    case "$version" in *[!0-9A-Za-z.-]*) die 'Invalid release tag' ;; esac
    printf '%s\n' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$' || die 'Expected a release tag such as v0.1.1'
    if [ -z "$install_dir" ]; then
        [ -n "${HOME:-}" ] || die 'Set --bin-dir when HOME is unset'
        install_dir=$HOME/.local/bin
    fi
    case "$install_dir" in /*) ;; *) install_dir=$PWD/$install_dir ;; esac
    case "$(uname -s)" in
        Darwin) system=apple-darwin ;;
        Linux)
            system=unknown-linux-gnu
            if command -v ldd >/dev/null 2>&1 && ldd --version 2>&1 | grep -qi musl; then
                die 'The Linux bundles require glibc; musl/Alpine needs a source build'
            fi ;;
        *) die 'Supported operating systems: macOS and Linux (glibc)' ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64) arch=x86_64 ;;
        arm64|aarch64) arch=aarch64 ;;
        *) die 'Supported architectures: x86_64 and ARM64' ;;
    esac
    case "$downloader" in
        auto)
            if command -v curl >/dev/null 2>&1; then downloader=curl
            elif command -v wget >/dev/null 2>&1; then downloader=wget
            else die 'Install curl or wget first'; fi ;;
        curl|wget) command -v "$downloader" >/dev/null 2>&1 || die "$downloader is not installed" ;;
        *) die '--downloader must be auto, curl or wget' ;;
    esac
    for tool in tar mktemp chmod mv mkdir awk grep; do
        command -v "$tool" >/dev/null 2>&1 || die "Required utility is missing: $tool"
    done
    if command -v sha256sum >/dev/null 2>&1; then hash_tool=sha256sum
    elif command -v shasum >/dev/null 2>&1; then hash_tool=shasum
    else die 'SHA-256 verification requires sha256sum or shasum'; fi

    temp_dir=''
    staged=''
    trap cleanup 0
    trap 'exit 130' 2
    trap 'exit 143' 15
    temp_dir=$(mktemp -d "${TMPDIR:-/tmp}/tpreview-install.XXXXXXXX")
    package=tpreview-$arch-$system
    archive=$package.tar.gz
    base=https://github.com/affisto/tpreview/releases/download/$version
    printf 'Downloading %s (%s) using %s…\n' "$version" "$arch-$system" "$downloader"
    download "$base/$archive" "$temp_dir/$archive" || die 'Bundle download failed; check the release tag and connection'
    download "$base/$archive.sha256" "$temp_dir/$archive.sha256" || die 'Checksum download failed'
    # Accept only this archive's one-line manifest; never let it name arbitrary local files.
    awk -v name="$archive" 'NR != 1 || NF != 2 || $2 != name || length($1) != 64 || $1 ~ /[^0-9a-f]/ {bad=1} END {exit (bad || NR != 1)}' "$temp_dir/$archive.sha256" || die 'Invalid checksum manifest'
    expected=$(awk '{print $1}' "$temp_dir/$archive.sha256")
    if [ "$hash_tool" = sha256sum ]; then actual=$(sha256sum "$temp_dir/$archive" | awk '{print $1}')
    else actual=$(shasum -a 256 "$temp_dir/$archive" | awk '{print $1}'); fi
    [ "$actual" = "$expected" ] || die 'SHA-256 mismatch; existing installation was not changed'

    mkdir -p "$install_dir" || die 'Cannot create install directory; choose a writable --bin-dir'
    [ ! -d "$install_dir/tpreview" ] || die 'The destination tpreview is a directory'
    staged=$(mktemp "$install_dir/.tpreview-install.XXXXXXXX")
    # Extract only the executable to stdout: archive paths cannot write into the filesystem.
    tar -xzOf "$temp_dir/$archive" "$package/tpreview" > "$staged" || die 'Bundle is missing its executable'
    [ -s "$staged" ] || die 'The bundled executable is empty'
    chmod 755 "$staged"
    installed_version=$("$staged" --version) || die 'This executable cannot run on this OS; the previous installation is intact'
    [ "$installed_version" = "tpreview ${version#v}" ] || die "Unexpected executable version: $installed_version"
    # Same-directory rename keeps an existing installation intact until verification succeeds.
    mv -f "$staged" "$install_dir/tpreview"
    staged=''
    printf 'Installed %s to %s/tpreview\n' "$installed_version" "$install_dir"
    case :${PATH:-}: in
        *:"$install_dir":*) printf '%s\n' 'Run: tpreview --doctor' ;;
        *) printf 'Add %s to your PATH, or run %s/tpreview --doctor\n' "$install_dir" "$install_dir" ;;
    esac
}

die() { printf 'tpreview installer: %s\n' "$*" >&2; exit 1; }
cleanup() {
    if [ -n "$staged" ]; then rm -f "$staged"; fi
    if [ -n "$temp_dir" ]; then rm -rf "$temp_dir"; fi
}
download() {
    if [ "$downloader" = curl ]; then
        curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
            --connect-timeout 15 --max-time 180 --retry 2 --output "$2" "$1"
    else
        wget --quiet --https-only --timeout=30 --tries=3 --output-document="$2" "$1"
    fi
}

# Keep all actions inside main so a truncated download cannot run a partial body.
main "$@"
