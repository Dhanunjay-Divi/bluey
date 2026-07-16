#!/usr/bin/env bash
set -euo pipefail

BLUEY_UV_VERSION="0.11.29"
BLUEY_MARKITDOWN_VERSION="0.1.6"
BLUEY_MARKITDOWN_EXCLUDE_NEWER="2026-07-16T00:00:00Z"
BLUEY_AZURE_CONTENT_UNDERSTANDING_VERSION="1.2.0b2"

INSTALL_HELPER_NAMES=(
  termb Terminal hostovb host-overlay adriverb audio-driver screen-driver
  bluey-overlay-macos cue-overlay-macos
  bluey-audio-macos cue-audio-macos
  bluey-whisper-macos cue-whisper
  bluey-file-picker-macos cue-file-picker-macos
)

PUBLIC_HELPER_NAMES=(
  hostovb host-overlay adriverb audio-driver screen-driver
  bluey-overlay-macos cue-overlay-macos
  bluey-audio-macos cue-audio-macos
  bluey-whisper-macos cue-whisper
  bluey-file-picker-macos cue-file-picker-macos
)

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

replace_first_binary_alias() {
  local source_dir="$1"
  local alias_name="$2"
  shift 2
  local candidate

  rm -f "$source_dir/$alias_name"
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

  replace_first_binary_alias "$bin_path" termb bluey-daemon cue-daemon
  copy_first_binary_alias "$bin_path" Terminal bluey-daemon cue-daemon
  copy_first_binary_alias "$bin_path" hostovb bluey-overlay-macos cue-overlay-macos
  copy_first_binary_alias "$bin_path" host-overlay bluey-overlay-macos cue-overlay-macos
  copy_first_binary_alias "$bin_path" adriverb bluey-audio-macos cue-audio-macos
  copy_first_binary_alias "$bin_path" audio-driver bluey-audio-macos cue-audio-macos
  copy_first_binary_alias "$bin_path" screen-driver bluey-capture cue-capture
}

bluey_uv_url() {
  if [[ -n "${BLUEY_UV_URL:-}" ]]; then
    [[ -n "${BLUEY_UV_SHA256:-}" ]] || return 1
    printf '%s\n' "$BLUEY_UV_URL"
    return 0
  fi

  case "$(uname -m)" in
    arm64|aarch64)
      printf '%s\n' "https://github.com/astral-sh/uv/releases/download/${BLUEY_UV_VERSION}/uv-aarch64-apple-darwin.tar.gz"
      ;;
    x86_64|amd64)
      printf '%s\n' "https://github.com/astral-sh/uv/releases/download/${BLUEY_UV_VERSION}/uv-x86_64-apple-darwin.tar.gz"
      ;;
    *)
      return 1
      ;;
  esac
}

bluey_uv_sha256() {
  if [[ -n "${BLUEY_UV_URL:-}" ]]; then
    [[ -n "${BLUEY_UV_SHA256:-}" ]] || return 1
    printf '%s\n' "$BLUEY_UV_SHA256"
    return 0
  fi

  case "$(uname -m)" in
    arm64|aarch64)
      printf '%s\n' "61c04acc52a33ef0f331e494bdfbedcdb6c26c6970c022ed3699e5860f8930e3"
      ;;
    x86_64|amd64)
      printf '%s\n' "c4c4de482da9ccdd076dc4fb5cfe7b740609029385c72f58606be3153602387d"
      ;;
    *)
      return 1
      ;;
  esac
}

ensure_bluey_uv() {
  local root="$1"
  local uv_dir="$root/tools/uv"
  local uv_bin="$uv_dir/uv"

  if [[ -n "${BLUEY_UV_URL:-}" && -z "${BLUEY_UV_SHA256:-}" ]]; then
    warn "BLUEY_UV_URL overrides require BLUEY_UV_SHA256"
    return 1
  fi

  if [[ -x "$uv_bin" ]]; then
    local installed_version
    installed_version="$("$uv_bin" --version 2>/dev/null | awk '{print $2}' || true)"
    if [[ "$installed_version" == "$BLUEY_UV_VERSION" ]]; then
      printf '%s\n' "$uv_bin"
      return 0
    fi
    rm -f "$uv_bin"
  fi

  local url expected_sha tmp archive extract found
  url="$(bluey_uv_url)" || return 1
  expected_sha="$(bluey_uv_sha256)" || return 1
  expected_sha="$(printf '%s' "$expected_sha" | tr '[:upper:]' '[:lower:]')"
  if [[ "${#expected_sha}" -ne 64 || "$expected_sha" == *[!0-9a-f]* ]]; then
    warn "Bluey local Python runtime helper SHA256 must contain exactly 64 hexadecimal characters"
    return 1
  fi
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/bluey-uv.XXXXXX")"
  archive="$tmp/uv.tar.gz"
  extract="$tmp/extract"
  mkdir -p "$uv_dir" "$extract"

  printf 'Installing Bluey local Python runtime helper...\n' >&2
  if ! curl -fsSL "$url" -o "$archive"; then
    rm -rf "$tmp"
    return 1
  fi
  if ! printf '%s  %s\n' "$expected_sha" "$archive" | shasum -a 256 -c - >/dev/null 2>&1; then
    warn "Bluey local Python runtime helper checksum verification failed"
    rm -rf "$tmp"
    return 1
  fi
  if ! tar -xzf "$archive" -C "$extract"; then
    rm -rf "$tmp"
    return 1
  fi
  found="$(find "$extract" -type f -name uv 2>/dev/null | head -n 1 || true)"
  if [[ -z "$found" ]]; then
    rm -rf "$tmp"
    return 1
  fi
  cp "$found" "$uv_bin"
  chmod +x "$uv_bin"
  rm -rf "$tmp"
  printf 'Bluey local Python runtime helper installed.\n' >&2
  printf '%s\n' "$uv_bin"
}

run_with_bluey_uv_env() {
  local root="$1"
  shift

  mkdir -p "$root/tools/uv-cache" "$root/tools/python"
  UV_CACHE_DIR="$root/tools/uv-cache" \
  UV_PYTHON_INSTALL_DIR="$root/tools/python" \
  UV_PYTHON_DOWNLOADS=automatic \
  UV_LINK_MODE=copy \
    "$@"
}

remove_bluey_legacy_terminal_link() {
  local bin_path="$1"
  local install_path="$2"
  local legacy_link="$bin_path/Terminal"
  local target

  [[ -L "$legacy_link" ]] || return 0
  target="$(readlink "$legacy_link" 2>/dev/null || true)"
  if [[ "$target" != /* ]]; then
    local link_dir target_dir target_name
    link_dir="$(cd "$(dirname "$legacy_link")" 2>/dev/null && pwd -P)" || return 0
    target_dir="$(cd "$link_dir/$(dirname "$target")" 2>/dev/null && pwd -P)" || return 0
    target_name="$(basename "$target")"
    target="$target_dir/$target_name"
  fi
  case "$target" in
    "$install_path"/*|"$HOME/.bluey"/*)
      rm -f "$legacy_link"
      ;;
  esac
}

install_local_doc_tools() {
  local root="$1"
  local tools_dir="$root/tools/doc-converter"
  local wrapper="$root/bin/bluey-doc-converter"
  local venv_dir="$tools_dir/.venv"
  local uv_bin=""

  rm -f "$wrapper"
  rm -rf "$venv_dir"
  if [[ "${BLUEY_SKIP_LOCAL_TOOLS:-0}" == "1" ]]; then
    warn "Skipping Bluey-local document tools because BLUEY_SKIP_LOCAL_TOOLS=1"
    return 0
  fi

  mkdir -p "$tools_dir" "$root/bin"
  uv_bin="$(ensure_bluey_uv "$root" || true)"
  if [[ -z "$uv_bin" ]]; then
    warn "could not prepare Bluey-local document tools; document conversion will use built-in fallbacks only"
    return 0
  fi

  if ! run_with_bluey_uv_env "$root" "$uv_bin" venv --python 3.12 "$venv_dir" >/dev/null 2>&1; then
    rm -rf "$venv_dir"
    warn "could not prepare Bluey-local document tools; document conversion will use built-in fallbacks only"
    return 0
  fi

  local py="$venv_dir/bin/python"
  if ! run_with_bluey_uv_env "$root" "$uv_bin" pip install \
      --prerelease explicit \
      --python "$py" \
      --exclude-newer "$BLUEY_MARKITDOWN_EXCLUDE_NEWER" \
      "markitdown[all]==$BLUEY_MARKITDOWN_VERSION" \
      "azure-ai-contentunderstanding==$BLUEY_AZURE_CONTENT_UNDERSTANDING_VERSION" >/dev/null 2>&1; then
    rm -rf "$venv_dir"
    if ! run_with_bluey_uv_env "$root" "$uv_bin" venv --python 3.12 "$venv_dir" >/dev/null 2>&1; then
      rm -rf "$venv_dir"
      warn "could not prepare Bluey-local document tools; document conversion will use built-in fallbacks only"
      return 0
    fi
    if ! run_with_bluey_uv_env "$root" "$uv_bin" pip install \
        --python "$py" \
        --exclude-newer "$BLUEY_MARKITDOWN_EXCLUDE_NEWER" \
        "markitdown==$BLUEY_MARKITDOWN_VERSION" >/dev/null 2>&1; then
      rm -rf "$venv_dir"
      warn "could not install pinned MarkItDown into Bluey's local tools venv; document conversion will use built-in fallbacks only"
      return 0
    fi
  fi

  if [[ ! -x "$venv_dir/bin/markitdown" ]] && [[ ! -x "$venv_dir/bin/markitdown.exe" ]]; then
    rm -rf "$venv_dir"
    warn "could not install pinned MarkItDown into Bluey's local tools venv; document conversion will use built-in fallbacks only"
    return 0
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
for executable in bluey bluey-daemon "${INSTALL_HELPER_NAMES[@]}"; do
  if [[ -f "$target_tmp/bin/$executable" ]]; then
    chmod +x "$target_tmp/bin/$executable"
  fi
done
if [[ -f "$target_tmp/bin/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos" ]]; then
  chmod +x "$target_tmp/bin/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos"
fi
install_local_doc_tools "$target_tmp"

rm -rf "$target"
mv "$target_tmp" "$target"

remove_bluey_legacy_terminal_link "$bin_dir" "$install_root"
ln -sfn "$target/bin/bluey" "$bin_dir/bluey"
daemon_link_target="$target/bin/termb"
if [[ ! -x "$daemon_link_target" ]]; then
  daemon_link_target="$target/bin/Terminal"
fi
if [[ ! -x "$daemon_link_target" ]]; then
  daemon_link_target="$target/bin/bluey-daemon"
fi
ln -sfn "$daemon_link_target" "$bin_dir/bluey-daemon"
for helper in "${PUBLIC_HELPER_NAMES[@]}"; do
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
