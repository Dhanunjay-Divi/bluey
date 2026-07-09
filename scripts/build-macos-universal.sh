#!/usr/bin/env bash
# build-macos-universal.sh
#
# Combines arm64 + x86_64 builds via `lipo -create` into universal binaries.
# Inputs:
#   target/aarch64-apple-darwin/release/{bluey,bluey-daemon}
#   target/x86_64-apple-darwin/release/{bluey,bluey-daemon}
#   native/macos/cue-overlay/.build/{arm64,x86_64}-apple-macosx/release/cue-overlay
#   native/macos/cue-audio/.build/{arm64,x86_64}-apple-macosx/release/cue-audio
#   native/macos/cue-whisper/.build/{arm64,x86_64}-apple-macosx/release/CueWhisper
#   native/macos/cue-picker/.build/{arm64,x86_64}-apple-macosx/release/cue-picker
#
# Output:
#   dist/bluey-macos-universal/{bluey,bluey-daemon,Terminal,host-overlay,audio-driver,bluey-overlay-macos,bluey-audio-macos,bluey-whisper-macos,bluey-file-picker-macos}

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="dist/bluey-macos-universal"
rm -rf "$OUT"
mkdir -p "$OUT"

# Rust binaries.
lipo -create \
    "target/aarch64-apple-darwin/release/bluey" \
    "target/x86_64-apple-darwin/release/bluey" \
    -output "$OUT/bluey"

lipo -create \
    "target/aarch64-apple-darwin/release/bluey-daemon" \
    "target/x86_64-apple-darwin/release/bluey-daemon" \
    -output "$OUT/bluey-daemon"
cp "$OUT/bluey-daemon" "$OUT/Terminal"

# Swift overlay.
ARM_OVERLAY="native/macos/cue-overlay/.build/arm64-apple-macosx/release/cue-overlay"
X86_OVERLAY="native/macos/cue-overlay/.build/x86_64-apple-macosx/release/cue-overlay"
if [[ -f "$ARM_OVERLAY" && -f "$X86_OVERLAY" ]]; then
    lipo -create "$ARM_OVERLAY" "$X86_OVERLAY" -output "$OUT/bluey-overlay-macos"
    cp "$OUT/bluey-overlay-macos" "$OUT/host-overlay"
else
    echo "warn: overlay arch builds not both present; skipping overlay in universal" >&2
fi

# Swift audio.
ARM_AUDIO="native/macos/cue-audio/.build/arm64-apple-macosx/release/cue-audio"
X86_AUDIO="native/macos/cue-audio/.build/x86_64-apple-macosx/release/cue-audio"
if [[ -f "$ARM_AUDIO" && -f "$X86_AUDIO" ]]; then
    lipo -create "$ARM_AUDIO" "$X86_AUDIO" -output "$OUT/bluey-audio-macos"
    cp "$OUT/bluey-audio-macos" "$OUT/audio-driver"
else
    echo "warn: audio arch builds not both present; skipping audio in universal" >&2
fi

# Swift whisper.
ARM_WHISPER="native/macos/cue-whisper/.build/arm64-apple-macosx/release/CueWhisper"
X86_WHISPER="native/macos/cue-whisper/.build/x86_64-apple-macosx/release/CueWhisper"
if [[ -f "$ARM_WHISPER" && -f "$X86_WHISPER" ]]; then
    lipo -create "$ARM_WHISPER" "$X86_WHISPER" -output "$OUT/bluey-whisper-macos"
else
    echo "warn: whisper arch builds not both present; skipping whisper in universal" >&2
fi

# Swift context file picker.
ARM_PICKER="native/macos/cue-picker/.build/arm64-apple-macosx/release/cue-picker"
X86_PICKER="native/macos/cue-picker/.build/x86_64-apple-macosx/release/cue-picker"
if [[ -f "$ARM_PICKER" && -f "$X86_PICKER" ]]; then
    lipo -create "$ARM_PICKER" "$X86_PICKER" -output "$OUT/bluey-file-picker-macos"
    APP_DIR="$OUT/BlueyFilePicker.app"
    rm -rf "$APP_DIR"
    mkdir -p "$APP_DIR/Contents/MacOS"
    cp native/macos/cue-picker/.build/BlueyFilePicker.app/Contents/Info.plist "$APP_DIR/Contents/Info.plist"
    cp "$OUT/bluey-file-picker-macos" "$APP_DIR/Contents/MacOS/bluey-file-picker-macos"
else
    echo "warn: picker arch builds not both present; skipping picker in universal" >&2
fi

# Verify.
echo "=== universal binaries ==="
for f in "$OUT"/*; do
    if [[ -x "$f" && ! -d "$f" ]]; then
        printf "%-40s " "$(basename "$f"):"
        lipo -archs "$f" 2>/dev/null || file "$f" | head -1
    fi
done

echo "$OUT"
