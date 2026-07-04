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
  BLUEY_VERSION       Version to install. Default: 0.1.10
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

# Where this script lives — used for the local-dir (AirDrop) install mode below.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

version="${BLUEY_VERSION:-0.1.10}"
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

extract_dir="$tmp/extract"
target_tmp="${install_root}/${version}.tmp"
target="${install_root}/${version}"
rm -rf "$extract_dir" "$target_tmp"
mkdir -p "$target_tmp" "$bin_dir"

if [[ -x "$script_dir/bin/bluey" && -x "$script_dir/bin/bluey-daemon" ]]; then
  # LOCAL-DIR (AirDrop) mode: the binaries are already unpacked next to this
  # script (the receiver ran `tar -xzf … && cd … && ./install.sh`). Install
  # straight from that adjacent bin/ — no download, no archive, no checksum.
  printf 'Installing Bluey from %s...\n' "$script_dir/bin"
  extract_dir="$script_dir"
else
  # ARCHIVE / DOWNLOAD mode: unpack a tarball (local BLUEY_ARCHIVE or a release
  # download) whose top level is bin/.
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
  mkdir -p "$extract_dir"
  tar -xzf "$archive" -C "$extract_dir"
  # A tarball may wrap its payload in a single top-level folder
  # (bluey-<ver>-<platform>/bin/…) or place bin/ at the root. Normalize: if bin/
  # isn't at the root, descend into the sole wrapper dir that contains it.
  if [[ ! -x "$extract_dir/bin/bluey" ]]; then
    wrapper="$(find "$extract_dir" -maxdepth 2 -type f -name bluey -path '*/bin/bluey' -print -quit 2>/dev/null)"
    [[ -n "$wrapper" ]] && extract_dir="$(dirname "$(dirname "$wrapper")")"
  fi
fi

[[ -x "$extract_dir/bin/bluey" ]] || die "archive missing executable bin/bluey"
[[ -x "$extract_dir/bin/bluey-daemon" ]] || die "archive missing executable bin/bluey-daemon"

cp -R "$extract_dir"/. "$target_tmp"/
# Make the whole install user-writable upfront. Some staged files ship read-only
# (e.g. libopenblas.0.dylib comes from Homebrew as r--r--r--), and a read-only
# file cannot have its com.apple.quarantine xattr removed later — xattr -d fails
# with "Permission denied", silently leaving quarantine on exactly the dylib the
# diarize daemon loads at startup, which re-triggers the "Apple could not verify"
# popup. Doing this here (freshly copied, definitely owned) guarantees the later
# codesign + quarantine-strip can touch every file.
chmod -R u+w "$target_tmp" 2>/dev/null || true
chmod +x "$target_tmp/bin/bluey" "$target_tmp/bin/bluey-daemon"
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

rm -rf "$target"
mv "$target_tmp" "$target"

ln -sfn "$target/bin/bluey" "$bin_dir/bluey"
ln -sfn "$target/bin/bluey-daemon" "$bin_dir/bluey-daemon"

# ── Make macOS launch these binaries without an "Apple could not verify" popup ──
#
# The binaries are ad-hoc signed, NOT Apple-notarized (no Developer account). Two
# things make macOS block them, and each needs its own cure:
#   1. codesign --force --sign -  — re-seal each Mach-O. AirDrop/download plus the
#      diarize install_name_tool rewrite invalidate the shipped signature; an
#      unsigned/broken-signature binary is killed on launch. Ad-hoc needs no cert.
#   2. xattr -dr com.apple.quarantine — REMOVE the quarantine flag. This is what
#      actually silences the "cannot verify" popup: ad-hoc signing alone does NOT
#      satisfy Gatekeeper (spctl still rejects it), so quarantine MUST be gone or
#      the popup fires on first launch of a GUI .app (e.g. BlueyAudio.app).
#   3. spctl --add — whitelist the .app bundles in Gatekeeper's own database, so
#      even a GUI double-launch is approved. Best-effort (may need admin); the
#      quarantine strip alone is enough for the daemon-SPAWNED subprocesses.
#
# ORDER MATTERS: sign FIRST, then strip quarantine LAST — codesign does not touch
# the quarantine xattr, but doing the strip last guarantees nothing re-quarantines
# a file after we cleared it. Best-effort throughout: never fail the install.
if command -v codesign >/dev/null 2>&1; then
  # .app bundles first (deep, so their nested Mach-O + resources are sealed).
  while IFS= read -r app; do
    codesign --force --deep --sign - "$app" >/dev/null 2>&1 || true
  done < <(find "$target/bin" -maxdepth 1 -name '*.app' -type d 2>/dev/null)
  # Then every bare Mach-O in bin/ (a top-level dylib like OpenBLAS is signed too).
  while IFS= read -r f; do
    case "$f" in *.app/*) continue ;; esac
    if file "$f" 2>/dev/null | grep -q 'Mach-O'; then
      codesign --force --sign - "$f" >/dev/null 2>&1 || true
    fi
  done < <(find "$target/bin" -maxdepth 1 -type f 2>/dev/null)
fi

# Whitelist the GUI .app bundles in Gatekeeper's assessment DB so a launch is
# approved despite being non-notarized. Best-effort (needs admin; harmless if it
# can't). The quarantine strip below is the primary cure for spawned subprocesses.
if command -v spctl >/dev/null 2>&1; then
  while IFS= read -r app; do
    spctl --add --label "Bluey" "$app" >/dev/null 2>&1 || true
  done < <(find "$target/bin" -maxdepth 1 -name '*.app' -type d 2>/dev/null)
fi

# Strip quarantine LAST, recursively, over the WHOLE install — this is the cure
# that stops the "Apple could not verify" popup. Then VERIFY it actually took; if
# anything is still quarantined, tell the user the one command that fixes it.
if command -v xattr >/dev/null 2>&1; then
  # CRITICAL: some staged files are read-only (e.g. libopenblas.0.dylib comes from
  # Homebrew as r--r--r--). `xattr -d` on a non-writable file fails with
  # "Permission denied", silently leaving quarantine on exactly the dylib the
  # diarize daemon must load at startup — which re-triggers the popup. Make the
  # tree user-writable first so the strip can touch every file.
  chmod -R u+w "$target" >/dev/null 2>&1 || true
  xattr -dr com.apple.quarantine "$target" 2>/dev/null || true
  # Some macOS versions leave a stubborn flag on nested bundle contents; a second
  # pass over every file closes that gap.
  find "$target" -exec xattr -d com.apple.quarantine {} \; >/dev/null 2>&1 || true
  if xattr -r "$target" 2>/dev/null | grep -q com.apple.quarantine; then
    printf '\n  NOTE: macOS quarantine could not be fully cleared. If you see an\n' >&2
    printf '  "Apple could not verify" popup, click Done, then run:\n' >&2
    printf '    xattr -dr com.apple.quarantine %s\n\n' "$target" >&2
  fi
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
