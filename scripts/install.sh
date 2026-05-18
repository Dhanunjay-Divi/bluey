#!/usr/bin/env bash
set -euo pipefail

die() {
  printf 'bluey install: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

usage() {
  cat <<'EOF'
Install Bluey from a release tarball.

Environment:
  BLUEY_VERSION       Version to install. Default: 0.1.0
  BLUEY_REPO          GitHub repo. Default: Dhanunjay-Divi/bluey
  BLUEY_ARCHIVE       Local archive path. If set, no download is attempted.
  BLUEY_INSTALL_DIR   Versioned install root. Default: ~/.local/bluey
  BLUEY_BIN_DIR       Symlink directory. Default: ~/.local/bin
  BLUEY_SKIP_CHECKSUM Set to 1 to skip SHA256SUMS verification.

Examples:
  scripts/install.sh
  BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-arm64.tar.gz scripts/install.sh
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

need tar
need shasum
need mktemp

version="${BLUEY_VERSION:-0.1.0}"
repo="${BLUEY_REPO:-Dhanunjay-Divi/bluey}"
install_root="${BLUEY_INSTALL_DIR:-$HOME/.local/bluey}"
bin_dir="${BLUEY_BIN_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64|Darwin-aarch64)
    platform="darwin-arm64"
    ;;
  *)
    die "v${version} installer supports macOS arm64 only; current platform is $(uname -s)-$(uname -m)"
    ;;
esac

archive_name="bluey-${version}-${platform}.tar.gz"
tmp="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT

if [[ -n "${BLUEY_ARCHIVE:-}" ]]; then
  [[ -f "$BLUEY_ARCHIVE" ]] || die "BLUEY_ARCHIVE does not exist: $BLUEY_ARCHIVE"
  archive="$BLUEY_ARCHIVE"
else
  need curl
  base_url="https://github.com/${repo}/releases/download/v${version}"
  archive="$tmp/$archive_name"
  printf 'Downloading %s...\n' "$archive_name"
  curl -fsSL "$base_url/$archive_name" -o "$archive"

  if [[ "${BLUEY_SKIP_CHECKSUM:-0}" != "1" ]]; then
    printf 'Verifying checksum...\n'
    curl -fsSL "$base_url/SHA256SUMS.txt" -o "$tmp/SHA256SUMS.txt"
    grep " ${archive_name}\$" "$tmp/SHA256SUMS.txt" > "$tmp/$archive_name.sha256" \
      || die "checksum for $archive_name not found in SHA256SUMS.txt"
    (cd "$tmp" && shasum -a 256 -c "$archive_name.sha256")
  fi
fi

extract_dir="$tmp/extract"
target_tmp="${install_root}/${version}.tmp"
target="${install_root}/${version}"

rm -rf "$extract_dir" "$target_tmp"
mkdir -p "$extract_dir" "$target_tmp" "$bin_dir"
tar -xzf "$archive" -C "$extract_dir"

[[ -x "$extract_dir/bin/bluey" ]] || die "archive missing executable bin/bluey"
[[ -x "$extract_dir/bin/bluey-daemon" ]] || die "archive missing executable bin/bluey-daemon"

cp -R "$extract_dir"/. "$target_tmp"/
chmod +x "$target_tmp/bin/bluey" "$target_tmp/bin/bluey-daemon"
if [[ -f "$target_tmp/bin/bluey-overlay-macos" ]]; then chmod +x "$target_tmp/bin/bluey-overlay-macos"; fi
if [[ -f "$target_tmp/bin/cue-overlay-macos" ]]; then chmod +x "$target_tmp/bin/cue-overlay-macos"; fi
if [[ -f "$target_tmp/bin/bluey-audio-macos" ]]; then chmod +x "$target_tmp/bin/bluey-audio-macos"; fi
if [[ -f "$target_tmp/bin/cue-audio-macos" ]]; then chmod +x "$target_tmp/bin/cue-audio-macos"; fi
if [[ -f "$target_tmp/bin/cue-whisper" ]]; then chmod +x "$target_tmp/bin/cue-whisper"; fi
if [[ -f "$target_tmp/bin/bluey-whisper-macos" ]]; then chmod +x "$target_tmp/bin/bluey-whisper-macos"; fi

rm -rf "$target"
mv "$target_tmp" "$target"

ln -sfn "$target/bin/bluey" "$bin_dir/bluey"
ln -sfn "$target/bin/bluey-daemon" "$bin_dir/bluey-daemon"

if command -v xattr >/dev/null 2>&1; then
  xattr -dr com.apple.quarantine "$target" 2>/dev/null || true
fi

"$target/bin/bluey" --version >/dev/null

cat <<EOF
Bluey ${version} installed.

Add this to PATH if needed:
  export PATH="$bin_dir:\$PATH"

Start Bluey:
  bluey on

Stop Bluey:
  bluey off
EOF
