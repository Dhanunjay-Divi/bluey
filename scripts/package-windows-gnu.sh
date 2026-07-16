#!/usr/bin/env bash
# Build and package the canonical Windows x86_64 release from a POSIX host.
#
# This is a release fallback for macOS/Linux operators with MinGW-w64. It does
# not replace scripts/build-windows.ps1 or the Windows/MSVC release workflow.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${BLUEY_WINDOWS_TARGET:-x86_64-pc-windows-gnu}"
MINGW_PREFIX="${BLUEY_MINGW_PREFIX:-x86_64-w64-mingw32}"
MINGW_CC="${BLUEY_MINGW_CC:-${MINGW_PREFIX}-gcc}"
MINGW_CXX="${BLUEY_MINGW_CXX:-${MINGW_PREFIX}-g++}"
MINGW_AR="${BLUEY_MINGW_AR:-${MINGW_PREFIX}-ar}"
MINGW_RANLIB="${BLUEY_MINGW_RANLIB:-${MINGW_PREFIX}-ranlib}"
MINGW_OBJDUMP="${BLUEY_MINGW_OBJDUMP:-${MINGW_PREFIX}-objdump}"
MINGW_STRIP="${BLUEY_MINGW_STRIP:-${MINGW_PREFIX}-strip}"
HOST_CC="${BLUEY_HOST_CC:-cc}"
HOST_CXX="${BLUEY_HOST_CXX:-c++}"

VERSION="$(
    awk -F'"' '
        /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
        /^\[/ { in_workspace_package = 0 }
        in_workspace_package && /^version[[:space:]]*=/ { print $2; exit }
    ' Cargo.toml
)"
if [ -z "$VERSION" ]; then
    echo "[windows-gnu] could not determine workspace version from Cargo.toml" >&2
    exit 1
fi
if [ -n "${BLUEY_VERSION:-}" ] && [ "${BLUEY_VERSION#v}" != "${VERSION#v}" ]; then
    echo "[windows-gnu] BLUEY_VERSION=${BLUEY_VERSION} does not match Cargo.toml version $VERSION" >&2
    exit 1
fi

if [ -z "${BLUEY_UPDATE_PUBKEY:-}" ]; then
    echo "[windows-gnu] BLUEY_UPDATE_PUBKEY is required for release builds" >&2
    exit 1
fi

required_commands=(
    cargo
    python3
    rustup
    shasum
    "$HOST_CC"
    "$HOST_CXX"
    "$MINGW_CC"
    "$MINGW_CXX"
    "$MINGW_AR"
    "$MINGW_RANLIB"
    "$MINGW_OBJDUMP"
    "$MINGW_STRIP"
)
for command_name in "${required_commands[@]}"; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "[windows-gnu] required command not found: $command_name" >&2
        exit 1
    fi
done

if ! rustup target list --installed | grep -Fxq "$TARGET"; then
    echo "[windows-gnu] Rust target is not installed: $TARGET" >&2
    echo "[windows-gnu] install it with: rustup target add $TARGET" >&2
    exit 1
fi

python3 - "$BLUEY_UPDATE_PUBKEY" <<'PY'
import base64
import binascii
import sys

value = sys.argv[1].strip()
try:
    decoded = base64.b64decode(value, validate=True)
except (binascii.Error, ValueError) as error:
    raise SystemExit(f"[windows-gnu] BLUEY_UPDATE_PUBKEY is not valid base64: {error}")
if len(decoded) != 32:
    raise SystemExit(
        "[windows-gnu] BLUEY_UPDATE_PUBKEY must encode exactly 32 Ed25519 bytes"
    )
PY

SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}"
case "$SOURCE_DATE_EPOCH" in
    *[!0-9]* | "")
        echo "[windows-gnu] SOURCE_DATE_EPOCH must be an integer Unix timestamp" >&2
        exit 1
        ;;
esac
export SOURCE_DATE_EPOCH

BUILD_ROOT="$ROOT/target/windows-gnu-package"
CARGO_BUILD_ROOT="$BUILD_ROOT/cargo"
NATIVE_BUILD_DIR="$BUILD_ROOT/native"
TEST_BUILD_DIR="$BUILD_ROOT/tests"
STAGE_DIR="$BUILD_ROOT/staging"
STAGE_BIN="$STAGE_DIR/bin"
DIST_DIR="$ROOT/dist"
ARCHIVE="$DIST_DIR/bluey-${VERSION#v}-windows-x86_64.zip"
ARCHIVE_SHA="$ARCHIVE.sha256"

rm -rf "$NATIVE_BUILD_DIR" "$TEST_BUILD_DIR" "$STAGE_DIR"
mkdir -p "$NATIVE_BUILD_DIR" "$TEST_BUILD_DIR" "$STAGE_BIN" "$DIST_DIR"
rm -f "$ARCHIVE" "$ARCHIVE_SHA"

echo "[windows-gnu] running portable native-helper contract tests"
"$HOST_CC" \
    -std=c11 -O2 -Wall -Wextra -Wpedantic -Werror \
    native/windows/cue-overlay/tests/overlay_protocol_tests.c \
    -o "$TEST_BUILD_DIR/overlay-protocol-tests"
"$TEST_BUILD_DIR/overlay-protocol-tests"

"$HOST_CXX" \
    -std=c++17 -O2 -Wall -Wextra -Wpedantic -Werror \
    native/windows/cue-capture/tests/capture_contract_tests.cpp \
    -o "$TEST_BUILD_DIR/capture-contract-tests"
"$TEST_BUILD_DIR/capture-contract-tests"

"$HOST_CC" \
    -std=c11 -O2 -Wall -Wextra -Wpedantic -Werror -Wformat=2 \
    -Wstrict-prototypes \
    native/windows/cue-audio/audio_args.c \
    native/windows/cue-audio/audio_args_test.c \
    -o "$TEST_BUILD_DIR/audio-args-test"
"$TEST_BUILD_DIR/audio-args-test"

"$HOST_CC" \
    -std=c11 -O2 -Wall -Wextra -Wpedantic -Werror -Wformat=2 \
    -Wstrict-prototypes \
    native/windows/cue-audio/resampler.c \
    native/windows/cue-audio/resampler_test.c \
    -lm \
    -o "$TEST_BUILD_DIR/resampler-test"
"$TEST_BUILD_DIR/resampler-test"

echo "[windows-gnu] building native Windows helpers"
native_c_common=(
    -std=c11
    -O2
    -Wall
    -Wextra
    -Wpedantic
    -Werror
    -Wformat=2
    -Wstrict-prototypes
    -fno-ident
    -static
    -Wl,--no-insert-timestamp
)
native_cxx_common=(
    -std=c++17
    -O2
    -Wall
    -Wextra
    -Wpedantic
    -Werror
    -fno-ident
    -static
    -Wl,--no-insert-timestamp
)

"$MINGW_CXX" \
    -x c++ \
    "${native_cxx_common[@]}" \
    -Wno-missing-field-initializers \
    -Wno-unused-function \
    -DUNICODE \
    -D_UNICODE \
    -D_WINVER=0x0601 \
    -D_WIN32_WINNT=0x0601 \
    -municode \
    native/windows/cue-overlay/main.c \
    -Wl,--subsystem,windows:6.01 \
    -luser32 \
    -lgdi32 \
    -ld2d1 \
    -ldwrite \
    -luuid \
    -lshell32 \
    -lcomctl32 \
    -ladvapi32 \
    -lole32 \
    -loleaut32 \
    -o "$NATIVE_BUILD_DIR/bluey-overlay.exe"

"$MINGW_CXX" \
    "${native_cxx_common[@]}" \
    -D_WIN32_WINNT=0x0601 \
    -municode \
    native/windows/cue-capture/main.cpp \
    -Wl,--subsystem,console:6.01 \
    -lgdiplus \
    -luser32 \
    -lgdi32 \
    -lole32 \
    -o "$NATIVE_BUILD_DIR/bluey-capture.exe"

"$MINGW_CC" \
    "${native_c_common[@]}" \
    -D_WIN32_WINNT=0x0A00 \
    native/windows/cue-audio/main.c \
    native/windows/cue-audio/audio_args.c \
    native/windows/cue-audio/resampler.c \
    -Wl,--subsystem,console:10.0 \
    -lole32 \
    -luuid \
    -lm \
    -o "$NATIVE_BUILD_DIR/bluey-audio.exe"

echo "[windows-gnu] building Rust CLI and daemon"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$MINGW_CC"
export CC_x86_64_pc_windows_gnu="$MINGW_CC"
export CXX_x86_64_pc_windows_gnu="$MINGW_CXX"
export AR_x86_64_pc_windows_gnu="$MINGW_AR"
export RANLIB_x86_64_pc_windows_gnu="$MINGW_RANLIB"
BLUEY_UPDATE_PUBKEY="$BLUEY_UPDATE_PUBKEY" \
CARGO_TARGET_DIR="$CARGO_BUILD_ROOT" \
    cargo build \
        --locked \
        --release \
        --target "$TARGET" \
        -p cue-daemon \
        --bin bluey-daemon \
        -p cue-cli \
        --bin bluey

RUST_RELEASE_DIR="$CARGO_BUILD_ROOT/$TARGET/release"
for required_rust_binary in bluey.exe bluey-daemon.exe; do
    if [ ! -s "$RUST_RELEASE_DIR/$required_rust_binary" ]; then
        echo "[windows-gnu] missing Rust release binary: $RUST_RELEASE_DIR/$required_rust_binary" >&2
        exit 1
    fi
done

cp "$RUST_RELEASE_DIR/bluey.exe" "$STAGE_BIN/bluey.exe"
cp "$RUST_RELEASE_DIR/bluey-daemon.exe" "$STAGE_BIN/bluey-daemon.exe"
cp "$STAGE_BIN/bluey-daemon.exe" "$STAGE_BIN/termb.exe"
cp "$STAGE_BIN/bluey-daemon.exe" "$STAGE_BIN/Terminal.exe"

cp "$NATIVE_BUILD_DIR/bluey-overlay.exe" "$STAGE_BIN/bluey-overlay.exe"
cp "$STAGE_BIN/bluey-overlay.exe" "$STAGE_BIN/cue-overlay.exe"
cp "$STAGE_BIN/bluey-overlay.exe" "$STAGE_BIN/hostovb.exe"
cp "$STAGE_BIN/bluey-overlay.exe" "$STAGE_BIN/host-overlay.exe"

cp "$NATIVE_BUILD_DIR/bluey-capture.exe" "$STAGE_BIN/bluey-capture.exe"
cp "$STAGE_BIN/bluey-capture.exe" "$STAGE_BIN/cue-capture.exe"
cp "$STAGE_BIN/bluey-capture.exe" "$STAGE_BIN/screen-driver.exe"

cp "$NATIVE_BUILD_DIR/bluey-audio.exe" "$STAGE_BIN/bluey-audio.exe"
cp "$STAGE_BIN/bluey-audio.exe" "$STAGE_BIN/cue-audio.exe"
cp "$STAGE_BIN/bluey-audio.exe" "$STAGE_BIN/adriverb.exe"
cp "$STAGE_BIN/bluey-audio.exe" "$STAGE_BIN/audio-driver.exe"

for binary in "$STAGE_BIN"/*.exe; do
    "$MINGW_STRIP" --strip-unneeded "$binary"
    if ! "$MINGW_OBJDUMP" -f "$binary" | grep -Fq "file format pei-x86-64"; then
        echo "[windows-gnu] non-x86_64 PE binary in release staging: $binary" >&2
        exit 1
    fi
    if "$MINGW_OBJDUMP" -p "$binary" \
        | awk '/DLL Name:/ { print tolower($3) }' \
        | grep -Eq '^(libgcc_s_.*|libstdc\+\+-6|libwinpthread-1)\.dll$'; then
        echo "[windows-gnu] unbundled MinGW runtime dependency in: $binary" >&2
        exit 1
    fi
done

if ! LC_ALL=C grep -aFq "$BLUEY_UPDATE_PUBKEY" "$STAGE_BIN/bluey.exe"; then
    echo "[windows-gnu] bluey.exe does not contain the required embedded update public key" >&2
    exit 1
fi

cmp "$STAGE_BIN/bluey-daemon.exe" "$STAGE_BIN/termb.exe"
cmp "$STAGE_BIN/bluey-daemon.exe" "$STAGE_BIN/Terminal.exe"
cmp "$STAGE_BIN/bluey-overlay.exe" "$STAGE_BIN/cue-overlay.exe"
cmp "$STAGE_BIN/bluey-overlay.exe" "$STAGE_BIN/hostovb.exe"
cmp "$STAGE_BIN/bluey-overlay.exe" "$STAGE_BIN/host-overlay.exe"
cmp "$STAGE_BIN/bluey-capture.exe" "$STAGE_BIN/cue-capture.exe"
cmp "$STAGE_BIN/bluey-capture.exe" "$STAGE_BIN/screen-driver.exe"
cmp "$STAGE_BIN/bluey-audio.exe" "$STAGE_BIN/cue-audio.exe"
cmp "$STAGE_BIN/bluey-audio.exe" "$STAGE_BIN/adriverb.exe"
cmp "$STAGE_BIN/bluey-audio.exe" "$STAGE_BIN/audio-driver.exe"

echo "[windows-gnu] writing deterministic release archive"
python3 - "$STAGE_DIR" "$ARCHIVE" "$SOURCE_DATE_EPOCH" <<'PY'
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
import stat
import sys
import zipfile

stage = Path(sys.argv[1])
archive_path = Path(sys.argv[2])
epoch = int(sys.argv[3])
minimum = int(datetime(1980, 1, 1, tzinfo=timezone.utc).timestamp())
maximum = int(datetime(2107, 12, 31, 23, 59, 58, tzinfo=timezone.utc).timestamp())
epoch = max(minimum, min(epoch, maximum))
timestamp = datetime.fromtimestamp(epoch, timezone.utc)
zip_time = (
    timestamp.year,
    timestamp.month,
    timestamp.day,
    timestamp.hour,
    timestamp.minute,
    timestamp.second - (timestamp.second % 2),
)

files = sorted(path for path in stage.rglob("*") if path.is_file())
with zipfile.ZipFile(
    archive_path,
    "w",
    compression=zipfile.ZIP_DEFLATED,
    compresslevel=9,
) as archive:
    for path in files:
        relative = path.relative_to(stage).as_posix()
        info = zipfile.ZipInfo(relative, zip_time)
        info.create_system = 3
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = (stat.S_IFREG | 0o755) << 16
        info.flag_bits |= 0x800
        archive.writestr(info, path.read_bytes(), compresslevel=9)
PY

python3 - "$ARCHIVE" <<'PY'
from pathlib import Path
import sys
import zipfile

archive_path = Path(sys.argv[1])
expected = {
    "bin/Terminal.exe",
    "bin/adriverb.exe",
    "bin/audio-driver.exe",
    "bin/bluey-audio.exe",
    "bin/bluey-capture.exe",
    "bin/bluey-daemon.exe",
    "bin/bluey-overlay.exe",
    "bin/bluey.exe",
    "bin/cue-audio.exe",
    "bin/cue-capture.exe",
    "bin/cue-overlay.exe",
    "bin/host-overlay.exe",
    "bin/hostovb.exe",
    "bin/screen-driver.exe",
    "bin/termb.exe",
}
with zipfile.ZipFile(archive_path) as archive:
    names = archive.namelist()
    if len(names) != len(set(names)):
        raise SystemExit("[windows-gnu] archive contains duplicate members")
    actual = set(names)
if actual != expected:
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    raise SystemExit(
        f"[windows-gnu] archive member mismatch; missing={missing}, extra={extra}"
    )
PY

python3 scripts/check-release-artifact-contents.py "$ARCHIVE"
(
    cd "$DIST_DIR"
    shasum -a 256 "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE_SHA")"
)

archive_digest="$(shasum -a 256 "$ARCHIVE" | awk '{ print $1 }')"
echo "[windows-gnu] archive=$ARCHIVE"
echo "[windows-gnu] sha256=$archive_digest"
echo "[windows-gnu] source_date_epoch=$SOURCE_DATE_EPOCH"
