#!/usr/bin/env bash
set -euo pipefail

die() {
  printf 'bluey install: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

warn() {
  printf 'bluey install warning: %s\n' "$*" >&2
}

copy_first_binary_alias() {
  local source_dir="$1"
  local alias_name="$2"
  shift 2
  local candidate

  [[ -e "$source_dir/$alias_name" ]] && return 0
  for candidate in "$@"; do
    if [[ -x "$source_dir/$candidate" ]]; then
      cp "$source_dir/$candidate" "$source_dir/$alias_name"
      chmod +x "$source_dir/$alias_name"
      return 0
    fi
  done
}

ensure_process_identity_aliases() {
  local bin_path="$1"

  copy_first_binary_alias "$bin_path" Terminal bluey-daemon cue-daemon
  copy_first_binary_alias "$bin_path" host-overlay bluey-overlay-macos cue-overlay-macos
  copy_first_binary_alias "$bin_path" audio-driver bluey-audio-macos cue-audio-macos
}

install_local_doc_tools() {
  local root="$1"
  local tools_dir="$root/tools/doc-converter"
  local wrapper="$root/bin/bluey-doc-converter"

  if [[ "${BLUEY_SKIP_LOCAL_TOOLS:-0}" == "1" ]]; then
    warn "Skipping Bluey-local document tools because BLUEY_SKIP_LOCAL_TOOLS=1"
    return 0
  fi
  if ! command -v python3 >/dev/null 2>&1; then
    warn "python3 was not found; document conversion will use built-in fallbacks only"
    return 0
  fi

  mkdir -p "$tools_dir" "$root/bin"
  if ! python3 -m venv "$tools_dir/.venv" >/dev/null 2>&1; then
    warn "could not create Bluey-local Python venv; document conversion will use built-in fallbacks only"
    return 0
  fi

  local py="$tools_dir/.venv/bin/python"
  "$py" -m pip install --disable-pip-version-check --upgrade pip >/dev/null 2>&1 || true
  if ! "$py" -m pip install --disable-pip-version-check "markitdown[all]" >/dev/null 2>&1; then
    if ! "$py" -m pip install --disable-pip-version-check markitdown >/dev/null 2>&1; then
      warn "could not install MarkItDown into Bluey's local tools venv; document conversion will use built-in fallbacks only"
      return 0
    fi
  fi

  cat > "$wrapper" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec "$ROOT/tools/doc-converter/.venv/bin/markitdown" "$@"
SH
  chmod +x "$wrapper"
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
  Darwin-x86_64|Darwin-amd64)
    platform="darwin-x86_64"
    ;;
  *)
    die "v${version} installer supports macOS arm64 and x86_64 only; current platform is $(uname -s)-$(uname -m)"
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
ensure_process_identity_aliases "$target_tmp/bin"
chmod +x "$target_tmp/bin/bluey" "$target_tmp/bin/bluey-daemon"
if [[ -f "$target_tmp/bin/Terminal" ]]; then chmod +x "$target_tmp/bin/Terminal"; fi
if [[ -f "$target_tmp/bin/host-overlay" ]]; then chmod +x "$target_tmp/bin/host-overlay"; fi
if [[ -f "$target_tmp/bin/audio-driver" ]]; then chmod +x "$target_tmp/bin/audio-driver"; fi
if [[ -f "$target_tmp/bin/screen-driver" ]]; then chmod +x "$target_tmp/bin/screen-driver"; fi
if [[ -f "$target_tmp/bin/bluey-overlay-macos" ]]; then chmod +x "$target_tmp/bin/bluey-overlay-macos"; fi
if [[ -f "$target_tmp/bin/cue-overlay-macos" ]]; then chmod +x "$target_tmp/bin/cue-overlay-macos"; fi
if [[ -f "$target_tmp/bin/bluey-audio-macos" ]]; then chmod +x "$target_tmp/bin/bluey-audio-macos"; fi
if [[ -f "$target_tmp/bin/cue-audio-macos" ]]; then chmod +x "$target_tmp/bin/cue-audio-macos"; fi
if [[ -f "$target_tmp/bin/cue-whisper" ]]; then chmod +x "$target_tmp/bin/cue-whisper"; fi
if [[ -f "$target_tmp/bin/bluey-whisper-macos" ]]; then chmod +x "$target_tmp/bin/bluey-whisper-macos"; fi
if [[ -f "$target_tmp/bin/bluey-file-picker-macos" ]]; then chmod +x "$target_tmp/bin/bluey-file-picker-macos"; fi
if [[ -f "$target_tmp/bin/cue-file-picker-macos" ]]; then chmod +x "$target_tmp/bin/cue-file-picker-macos"; fi
if [[ -f "$target_tmp/bin/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos" ]]; then
  chmod +x "$target_tmp/bin/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos"
fi
install_local_doc_tools "$target_tmp"

rm -rf "$target"
mv "$target_tmp" "$target"

ln -sfn "$target/bin/bluey" "$bin_dir/bluey"
daemon_link_target="$target/bin/Terminal"
if [[ ! -x "$daemon_link_target" ]]; then
  daemon_link_target="$target/bin/bluey-daemon"
fi
ln -sfn "$daemon_link_target" "$bin_dir/bluey-daemon"
for helper in \
  Terminal host-overlay audio-driver screen-driver \
  bluey-overlay-macos cue-overlay-macos \
  bluey-audio-macos cue-audio-macos \
  bluey-whisper-macos cue-whisper \
  bluey-file-picker-macos cue-file-picker-macos
do
  if [[ -x "$target/bin/$helper" ]]; then
    ln -sfn "$target/bin/$helper" "$bin_dir/$helper"
  fi
done

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
