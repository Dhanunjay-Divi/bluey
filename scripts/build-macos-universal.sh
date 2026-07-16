#!/usr/bin/env bash
# Combine complete arm64 and x86_64 terminal releases into one universal tree.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CARGO_TARGET_ROOT="${BLUEY_CARGO_TARGET_DIR:-$ROOT/target}"
OUT="${BLUEY_UNIVERSAL_OUT:-$ROOT/dist/bluey-macos-universal}"

for command_name in chmod install mkdir python3 rm sed sort swift tr xcrun; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "[macos-universal] required command not found: $command_name" >&2
        exit 1
    fi
done
if ! xcrun --find lipo >/dev/null 2>&1; then
    echo "[macos-universal] required Apple tool not found: lipo" >&2
    exit 1
fi

require_executable() {
    local path="$1"
    if [[ ! -f "$path" || ! -x "$path" || ! -s "$path" ]]; then
        echo "[macos-universal] required executable is missing or empty: $path" >&2
        exit 1
    fi
}

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
        echo "[macos-universal] architecture mismatch for $path: expected '$expected', got '$actual'" >&2
        exit 1
    fi
}

combine() {
    local arm_path="$1"
    local x86_path="$2"
    local destination="$3"
    require_executable "$arm_path"
    require_executable "$x86_path"
    verify_arches "$arm_path" arm64
    verify_arches "$x86_path" x86_64
    xcrun lipo -create "$arm_path" "$x86_path" -output "$destination"
    chmod 0755 "$destination"
    verify_arches "$destination" arm64 x86_64
}

rm -rf "$OUT"
mkdir -p "$OUT"

combine \
    "$CARGO_TARGET_ROOT/aarch64-apple-darwin/release/bluey" \
    "$CARGO_TARGET_ROOT/x86_64-apple-darwin/release/bluey" \
    "$OUT/bluey"
combine \
    "$CARGO_TARGET_ROOT/aarch64-apple-darwin/release/bluey-daemon" \
    "$CARGO_TARGET_ROOT/x86_64-apple-darwin/release/bluey-daemon" \
    "$OUT/bluey-daemon"

ARM_OVERLAY="$(swift_binary native/macos/cue-overlay arm64 cue-overlay)"
X86_OVERLAY="$(swift_binary native/macos/cue-overlay x86_64 cue-overlay)"
combine "$ARM_OVERLAY" "$X86_OVERLAY" "$OUT/bluey-overlay-macos"

ARM_AUDIO="$(swift_binary native/macos/cue-audio arm64 cue-audio)"
X86_AUDIO="$(swift_binary native/macos/cue-audio x86_64 cue-audio)"
combine "$ARM_AUDIO" "$X86_AUDIO" "$OUT/bluey-audio-macos"

ARM_WHISPER="$(swift_binary native/macos/cue-whisper arm64 CueWhisper)"
X86_WHISPER="$(swift_binary native/macos/cue-whisper x86_64 CueWhisper)"
combine "$ARM_WHISPER" "$X86_WHISPER" "$OUT/bluey-whisper-macos"

ARM_PICKER="$(swift_binary native/macos/cue-picker arm64 cue-picker)"
X86_PICKER="$(swift_binary native/macos/cue-picker x86_64 cue-picker)"
combine "$ARM_PICKER" "$X86_PICKER" "$OUT/bluey-file-picker-macos"

install -m 0755 "$OUT/bluey-daemon" "$OUT/termb"
install -m 0755 "$OUT/bluey-daemon" "$OUT/Terminal"
install -m 0755 "$OUT/bluey-overlay-macos" "$OUT/cue-overlay-macos"
install -m 0755 "$OUT/bluey-overlay-macos" "$OUT/hostovb"
install -m 0755 "$OUT/bluey-overlay-macos" "$OUT/host-overlay"
install -m 0755 "$OUT/bluey-audio-macos" "$OUT/cue-audio-macos"
install -m 0755 "$OUT/bluey-audio-macos" "$OUT/adriverb"
install -m 0755 "$OUT/bluey-audio-macos" "$OUT/audio-driver"
install -m 0755 "$OUT/bluey-whisper-macos" "$OUT/cue-whisper"
install -m 0755 "$OUT/bluey-file-picker-macos" "$OUT/cue-file-picker-macos"

PICKER_PLIST="native/macos/cue-picker/.build/BlueyFilePicker.app/Contents/Info.plist"
if [[ ! -f "$PICKER_PLIST" || ! -s "$PICKER_PLIST" ]]; then
    echo "[macos-universal] required file picker Info.plist is missing: $PICKER_PLIST" >&2
    exit 1
fi
mkdir -p "$OUT/BlueyFilePicker.app/Contents/MacOS"
install -m 0644 "$PICKER_PLIST" "$OUT/BlueyFilePicker.app/Contents/Info.plist"
install -m 0755 \
    "$OUT/bluey-file-picker-macos" \
    "$OUT/BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos"

python3 - "$OUT" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
expected = {
    "BlueyFilePicker.app/Contents/Info.plist",
    "BlueyFilePicker.app/Contents/MacOS/bluey-file-picker-macos",
    "Terminal",
    "adriverb",
    "audio-driver",
    "bluey-audio-macos",
    "bluey-daemon",
    "bluey-file-picker-macos",
    "bluey-overlay-macos",
    "bluey-whisper-macos",
    "bluey",
    "cue-audio-macos",
    "cue-file-picker-macos",
    "cue-overlay-macos",
    "cue-whisper",
    "host-overlay",
    "hostovb",
    "termb",
}
actual = {
    path.relative_to(root).as_posix()
    for path in root.rglob("*")
    if path.is_file()
}
if actual != expected:
    raise SystemExit(
        "[macos-universal] output member mismatch; "
        f"missing={sorted(expected - actual)}, extra={sorted(actual - expected)}"
    )
PY

echo "[macos-universal] output=$OUT"
