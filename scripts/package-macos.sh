#!/usr/bin/env bash
# Build and package a complete terminal-only Bluey release for macOS.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
    echo "usage: $0 [--dry-run] <arm64|x86_64|universal>" >&2
}

DRY_RUN=0
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN=1
    shift
fi
if [[ "$#" -ne 1 ]]; then
    usage
    exit 2
fi
MODE="$1"

case "$MODE" in
    arm64)
        PLATFORM="darwin-arm64"
        RUST_TARGETS=(aarch64-apple-darwin)
        SWIFT_ARCHES=(arm64)
        ;;
    x86_64)
        PLATFORM="darwin-x86_64"
        RUST_TARGETS=(x86_64-apple-darwin)
        SWIFT_ARCHES=(x86_64)
        ;;
    universal)
        PLATFORM="darwin-universal"
        RUST_TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
        SWIFT_ARCHES=(arm64 x86_64)
        ;;
    *)
        usage
        exit 2
        ;;
esac

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "[macos-package] macOS packaging requires a Darwin host" >&2
    exit 1
fi

VERSION="$(
    awk -F'"' '
        /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
        /^\[/ { in_workspace_package = 0 }
        in_workspace_package && /^version[[:space:]]*=/ { print $2; exit }
    ' Cargo.toml
)"
if [[ -z "$VERSION" ]]; then
    echo "[macos-package] could not determine workspace version from Cargo.toml" >&2
    exit 1
fi
if [[ -n "${BLUEY_VERSION:-}" && "${BLUEY_VERSION#v}" != "${VERSION#v}" ]]; then
    echo "[macos-package] BLUEY_VERSION=${BLUEY_VERSION} does not match Cargo.toml version $VERSION" >&2
    exit 1
fi
if [[ -z "${BLUEY_UPDATE_PUBKEY:-}" ]]; then
    echo "[macos-package] BLUEY_UPDATE_PUBKEY is required for release builds" >&2
    exit 1
fi

required_commands=(
    awk
    cargo
    cmp
    cp
    git
    install
    mkdir
    python3
    rm
    sed
    shasum
    sort
    swift
    tr
    uname
    xcrun
)
for command_name in "${required_commands[@]}"; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "[macos-package] required command not found: $command_name" >&2
        exit 1
    fi
done
if ! xcrun --find lipo >/dev/null 2>&1; then
    echo "[macos-package] required Apple tool not found: lipo" >&2
    exit 1
fi

python3 - "$BLUEY_UPDATE_PUBKEY" <<'PY'
import base64
import binascii
import sys

value = sys.argv[1]
try:
    decoded = base64.b64decode(value, validate=True)
except (binascii.Error, ValueError) as error:
    raise SystemExit(f"[macos-package] BLUEY_UPDATE_PUBKEY is not valid base64: {error}")
if len(decoded) != 32:
    raise SystemExit(
        "[macos-package] BLUEY_UPDATE_PUBKEY must encode exactly 32 Ed25519 bytes"
    )
PY

SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
case "$SOURCE_DATE_EPOCH" in
    *[!0-9]* | "")
        echo "[macos-package] SOURCE_DATE_EPOCH must be an integer Unix timestamp" >&2
        exit 1
        ;;
esac
if (( 10#$SOURCE_DATE_EPOCH > 4294967295 )); then
    echo "[macos-package] SOURCE_DATE_EPOCH exceeds the gzip timestamp range" >&2
    exit 1
fi
export SOURCE_DATE_EPOCH

for helper in cue-audio cue-overlay cue-picker cue-whisper; do
    if [[ ! -x "$ROOT/native/macos/$helper/build.sh" ]]; then
        echo "[macos-package] required helper builder is missing: native/macos/$helper/build.sh" >&2
        exit 1
    fi
done

BUILD_ROOT="$ROOT/target/macos-package/$MODE"
CARGO_BUILD_ROOT="$BUILD_ROOT/cargo"
STAGE_DIR="$BUILD_ROOT/staging"
STAGE_BIN="$STAGE_DIR/bin"
UNIVERSAL_OUT="$BUILD_ROOT/universal"
DIST_DIR="$ROOT/dist"
ARCHIVE="$DIST_DIR/bluey-${VERSION#v}-${PLATFORM}.tar.gz"
ARCHIVE_SHA="$ARCHIVE.sha256"

if (( DRY_RUN )); then
    echo "[macos-package] dry-run mode=$MODE platform=$PLATFORM version=${VERSION#v}"
    echo "[macos-package] rust_targets=${RUST_TARGETS[*]}"
    echo "[macos-package] swift_arches=${SWIFT_ARCHES[*]}"
    echo "[macos-package] archive=$ARCHIVE"
    echo "[macos-package] expected_files=18 terminal_only=true"
    exit 0
fi

cleanup() {
    rm -rf "$STAGE_DIR" "$UNIVERSAL_OUT"
}
trap cleanup EXIT
rm -rf "$STAGE_DIR" "$UNIVERSAL_OUT"
mkdir -p "$STAGE_BIN" "$DIST_DIR"
rm -f "$ARCHIVE" "$ARCHIVE_SHA"

for rust_target in "${RUST_TARGETS[@]}"; do
    echo "[macos-package] building Rust target $rust_target"
    BLUEY_UPDATE_PUBKEY="$BLUEY_UPDATE_PUBKEY" \
    CARGO_TARGET_DIR="$CARGO_BUILD_ROOT" \
        cargo build \
            --locked \
            --release \
            --target "$rust_target" \
            -p cue-daemon \
            --bin bluey-daemon \
            -p cue-cli \
            --bin bluey
done

for swift_arch in "${SWIFT_ARCHES[@]}"; do
    echo "[macos-package] building Swift helpers for $swift_arch"
    for helper in cue-overlay cue-audio cue-whisper cue-picker; do
        BLUEY_OVERLAY_SWIFT_CONFIGURATION=release \
        BLUEY_SWIFT_ARCH="$swift_arch" \
            bash "$ROOT/native/macos/$helper/build.sh"
    done
done

swift_binary() {
    local package_dir="$1"
    local arch="$2"
    local product="$3"
    local bin_dir
    bin_dir="$(
        cd "$package_dir"
        swift build \
            -c release \
            --arch "$arch" \
            --disable-automatic-resolution \
            --show-bin-path
    )"
    printf '%s/%s\n' "$bin_dir" "$product"
}

require_executable() {
    local path="$1"
    if [[ ! -f "$path" || ! -x "$path" || ! -s "$path" ]]; then
        echo "[macos-package] required executable is missing or empty: $path" >&2
        exit 1
    fi
}

verify_arches() {
    local path="$1"
    shift
    local expected actual
    expected="$(printf '%s\n' "$@" | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//')"
    actual="$(
        xcrun lipo -archs "$path" \
            | tr ' ' '\n' \
            | sed '/^$/d' \
            | LC_ALL=C sort \
            | tr '\n' ' ' \
            | sed 's/ $//'
    )"
    if [[ "$actual" != "$expected" ]]; then
        echo "[macos-package] architecture mismatch for $path: expected '$expected', got '$actual'" >&2
        exit 1
    fi
}

copy_executable() {
    local source="$1"
    local destination="$2"
    require_executable "$source"
    install -m 0755 "$source" "$STAGE_BIN/$destination"
}

if [[ "$MODE" == "universal" ]]; then
    BLUEY_CARGO_TARGET_DIR="$CARGO_BUILD_ROOT" \
    BLUEY_UNIVERSAL_OUT="$UNIVERSAL_OUT" \
        bash "$ROOT/scripts/build-macos-universal.sh"
    cp -R "$UNIVERSAL_OUT"/. "$STAGE_BIN"/
else
    rust_target="${RUST_TARGETS[0]}"
    swift_arch="${SWIFT_ARCHES[0]}"
    copy_executable "$CARGO_BUILD_ROOT/$rust_target/release/bluey" bluey
    copy_executable "$CARGO_BUILD_ROOT/$rust_target/release/bluey-daemon" bluey-daemon
    copy_executable "$STAGE_BIN/bluey-daemon" termb
    copy_executable "$STAGE_BIN/bluey-daemon" Terminal

    overlay="$(swift_binary native/macos/cue-overlay "$swift_arch" cue-overlay)"
    copy_executable "$overlay" bluey-overlay-macos
    copy_executable "$overlay" cue-overlay-macos
    copy_executable "$overlay" hostovb
    copy_executable "$overlay" host-overlay

    audio="$(swift_binary native/macos/cue-audio "$swift_arch" cue-audio)"
    copy_executable "$audio" bluey-audio-macos
    copy_executable "$audio" cue-audio-macos
    copy_executable "$audio" adriverb
    copy_executable "$audio" audio-driver

    whisper="$(swift_binary native/macos/cue-whisper "$swift_arch" CueWhisper)"
    copy_executable "$whisper" bluey-whisper-macos
    copy_executable "$whisper" cue-whisper

    picker="$(swift_binary native/macos/cue-picker "$swift_arch" cue-picker)"
    copy_executable "$picker" bluey-file-picker-macos
    copy_executable "$picker" cue-file-picker-macos

    picker_plist="native/macos/cue-picker/.build/BlueyFilePicker.app/Contents/Info.plist"
    if [[ ! -f "$picker_plist" || ! -s "$picker_plist" ]]; then
        echo "[macos-package] required file picker Info.plist is missing: $picker_plist" >&2
        exit 1
    fi
    mkdir -p "$STAGE_BIN/BlueyFilePicker.app/Contents/MacOS"
    install -m 0644 \
        "$picker_plist" \
        "$STAGE_BIN/BlueyFilePicker.app/Contents/Info.plist"
    install -m 0755 \
        "$picker" \
        "$STAGE_BIN/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos"
fi

if [[ "$MODE" == "universal" ]]; then
    expected_arches=(arm64 x86_64)
else
    expected_arches=("${SWIFT_ARCHES[0]}")
fi
for binary in \
    "$STAGE_BIN/bluey" \
    "$STAGE_BIN/bluey-daemon" \
    "$STAGE_BIN/bluey-overlay-macos" \
    "$STAGE_BIN/bluey-audio-macos" \
    "$STAGE_BIN/bluey-whisper-macos" \
    "$STAGE_BIN/bluey-file-picker-macos" \
    "$STAGE_BIN/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos"; do
    require_executable "$binary"
    verify_arches "$binary" "${expected_arches[@]}"
done

python3 - "$STAGE_BIN/bluey" "$BLUEY_UPDATE_PUBKEY" <<'PY'
from pathlib import Path
import sys

binary = Path(sys.argv[1]).read_bytes()
public_key = sys.argv[2].encode()
if public_key not in binary:
    raise SystemExit(
        "[macos-package] bluey does not contain the required embedded update public key"
    )
PY

cmp "$STAGE_BIN/bluey-daemon" "$STAGE_BIN/termb"
cmp "$STAGE_BIN/bluey-daemon" "$STAGE_BIN/Terminal"
cmp "$STAGE_BIN/bluey-overlay-macos" "$STAGE_BIN/cue-overlay-macos"
cmp "$STAGE_BIN/bluey-overlay-macos" "$STAGE_BIN/hostovb"
cmp "$STAGE_BIN/bluey-overlay-macos" "$STAGE_BIN/host-overlay"
cmp "$STAGE_BIN/bluey-audio-macos" "$STAGE_BIN/cue-audio-macos"
cmp "$STAGE_BIN/bluey-audio-macos" "$STAGE_BIN/adriverb"
cmp "$STAGE_BIN/bluey-audio-macos" "$STAGE_BIN/audio-driver"
cmp "$STAGE_BIN/bluey-whisper-macos" "$STAGE_BIN/cue-whisper"
cmp "$STAGE_BIN/bluey-file-picker-macos" "$STAGE_BIN/cue-file-picker-macos"
cmp \
    "$STAGE_BIN/bluey-file-picker-macos" \
    "$STAGE_BIN/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos"

echo "[macos-package] writing deterministic release archive"
python3 - "$STAGE_DIR" "$ARCHIVE" "$SOURCE_DATE_EPOCH" <<'PY'
from __future__ import annotations

import gzip
import io
from pathlib import Path
import sys
import tarfile

stage = Path(sys.argv[1])
archive_path = Path(sys.argv[2])
epoch = int(sys.argv[3])
expected = {
    "bin/BlueyFilePicker.app/Contents/Info.plist": 0o644,
    "bin/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos": 0o755,
    "bin/Terminal": 0o755,
    "bin/adriverb": 0o755,
    "bin/audio-driver": 0o755,
    "bin/bluey-audio-macos": 0o755,
    "bin/bluey-daemon": 0o755,
    "bin/bluey-file-picker-macos": 0o755,
    "bin/bluey-overlay-macos": 0o755,
    "bin/bluey-whisper-macos": 0o755,
    "bin/bluey": 0o755,
    "bin/cue-audio-macos": 0o755,
    "bin/cue-file-picker-macos": 0o755,
    "bin/cue-overlay-macos": 0o755,
    "bin/cue-whisper": 0o755,
    "bin/host-overlay": 0o755,
    "bin/hostovb": 0o755,
    "bin/termb": 0o755,
}
actual = {
    path.relative_to(stage).as_posix()
    for path in stage.rglob("*")
    if path.is_file()
}
if actual != set(expected):
    raise SystemExit(
        "[macos-package] staging member mismatch; "
        f"missing={sorted(set(expected) - actual)}, extra={sorted(actual - set(expected))}"
    )

with archive_path.open("wb") as raw:
    with gzip.GzipFile(
        fileobj=raw,
        mode="wb",
        filename="",
        compresslevel=9,
        mtime=epoch,
    ) as compressed:
        with tarfile.open(
            fileobj=compressed,
            mode="w",
            format=tarfile.USTAR_FORMAT,
        ) as archive:
            for relative, mode in sorted(expected.items()):
                path = stage / relative
                payload = path.read_bytes()
                member = tarfile.TarInfo(relative)
                member.size = len(payload)
                member.mode = mode
                member.mtime = epoch
                member.uid = 0
                member.gid = 0
                member.uname = ""
                member.gname = ""
                archive.addfile(member, fileobj=io.BytesIO(payload))
PY

python3 - "$ARCHIVE" <<'PY'
from pathlib import Path
import stat
import sys
import tarfile

archive_path = Path(sys.argv[1])
expected = {
    "bin/BlueyFilePicker.app/Contents/Info.plist": 0o644,
    "bin/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos": 0o755,
    "bin/Terminal": 0o755,
    "bin/adriverb": 0o755,
    "bin/audio-driver": 0o755,
    "bin/bluey-audio-macos": 0o755,
    "bin/bluey-daemon": 0o755,
    "bin/bluey-file-picker-macos": 0o755,
    "bin/bluey-overlay-macos": 0o755,
    "bin/bluey-whisper-macos": 0o755,
    "bin/bluey": 0o755,
    "bin/cue-audio-macos": 0o755,
    "bin/cue-file-picker-macos": 0o755,
    "bin/cue-overlay-macos": 0o755,
    "bin/cue-whisper": 0o755,
    "bin/host-overlay": 0o755,
    "bin/hostovb": 0o755,
    "bin/termb": 0o755,
}
with tarfile.open(archive_path, "r:gz") as archive:
    members = archive.getmembers()
    names = [member.name for member in members]
    if len(names) != len(set(names)):
        raise SystemExit("[macos-package] archive contains duplicate members")
    actual = set(names)
    if actual != set(expected):
        raise SystemExit(
            "[macos-package] archive member mismatch; "
            f"missing={sorted(set(expected) - actual)}, extra={sorted(actual - set(expected))}"
        )
    for member in members:
        if not member.isfile():
            raise SystemExit(f"[macos-package] non-file archive member: {member.name}")
        actual_mode = stat.S_IMODE(member.mode)
        if actual_mode != expected[member.name]:
            raise SystemExit(
                f"[macos-package] mode mismatch for {member.name}: "
                f"expected {oct(expected[member.name])}, got {oct(actual_mode)}"
            )
PY

python3 scripts/check-release-artifact-contents.py "$ARCHIVE"
(
    cd "$DIST_DIR"
    shasum -a 256 "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE_SHA")"
)

archive_digest="$(shasum -a 256 "$ARCHIVE" | awk '{ print $1 }')"
echo "[macos-package] archive=$ARCHIVE"
echo "[macos-package] sha256=$archive_digest"
echo "[macos-package] source_date_epoch=$SOURCE_DATE_EPOCH"
