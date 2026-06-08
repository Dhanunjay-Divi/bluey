#!/usr/bin/env bash
set -euo pipefail

# Dev-only visual smoke for the native macOS overlay.
#
# This intentionally builds a debug overlay and enables
# BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE so the test can screenshot the overlay.
# Release overlay binaries ignore capture-visible QA flags at compile time.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${BLUEY_VISUAL_SMOKE_OUT:-/tmp/bluey-smoke-shots}"
BLUEY_BIN="${BLUEY_BIN:-$ROOT/target/debug/bluey}"
EXPECTED_HEIGHT="${BLUEY_EXPECTED_OVERLAY_HEIGHT:-520}"
MIN_WIDTH="${BLUEY_MIN_OVERLAY_WIDTH:-680}"
MAX_WIDTH="${BLUEY_MAX_OVERLAY_WIDTH:-960}"
OVERLAY_SOURCE="$ROOT/native/macos/cue-overlay/Sources/cue-overlay/main.swift"

require_source() {
  local pattern="$1"
  if ! grep -Fq -- "$pattern" "$OVERLAY_SOURCE"; then
    echo "[visual-smoke] missing required UI contract marker: $pattern" >&2
    exit 2
  fi
}

reject_source() {
  local pattern="$1"
  if grep -Fq -- "$pattern" "$OVERLAY_SOURCE"; then
    echo "[visual-smoke] rejected UI contract marker still present: $pattern" >&2
    exit 2
  fi
}

mkdir -p "$OUT_DIR"
cd "$ROOT"

echo "[visual-smoke] checking overlay UI contract"
require_source "private final class ComposerTextView"
require_source "recordingButton = NSButton(title: \"Listen\""
require_source "styleIconButton(attachButton, symbol: \"plus\""
require_source "styleControlButton(instructionsButton, symbol: \"text.bubble\""
require_source "modelMenu.trailingAnchor.constraint(equalTo: analyzeButton.leadingAnchor"
require_source "composerBarHeightConstraint?.constant = textHeight + 62"
require_source "knowledgeBadge = NSTextField(labelWithString: \"Docs empty\")"
require_source "routeBadge = NSTextField(labelWithString: \"Auto · ready\")"
require_source "addSubview(headerBar, positioned: .above, relativeTo: nil)"
require_source "configureFixedChromeLayoutPriorities()"
require_source "keepFixedChromeInBounds()"
require_source "PillMetrics.centeredFrame"
require_source "startExpandedPassthroughTracking()"
require_source "pillWindow?.orderOut(nil)"
require_source "resizable: true"
require_source "maximumFrameHeight"
require_source "fitExpandedFrameToVisibleScreen"
require_source "setKnowledgeBadge(\"Docs loading\""
require_source "setTranscriptState(\"LISTENING\""
require_source "updateRouteBadge(for: q"
reject_source "Full access"
reject_source "Start Bluey"

echo "[visual-smoke] building debug CLI/daemon + macOS overlay"
cargo build --bin bluey --bin bluey-daemon >/dev/null
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh >/dev/null

cleanup() {
  "$BLUEY_BIN" off >/dev/null 2>&1 || true
}
trap cleanup EXIT

cleanup

echo "[visual-smoke] launching Bluey in dev capture-visible mode"
BLUEY_DEV_OVERLAY=1 \
BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 \
BLUEY_AUDIO_SIMULATED_ONLY=1 \
"$BLUEY_BIN" on >/tmp/bluey-overlay-visual-smoke.on.log 2>&1
sleep 1.2

python3 <<'PY'
import Quartz
import sys
import time

def windows():
    return Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly,
        Quartz.kCGNullWindowID,
    ) or []

pill = None
for win in windows():
    owner = (win.get("kCGWindowOwnerName") or "").lower()
    if owner not in ("bluey overlay", "bluey-overlay-macos"):
        continue
    bounds = win.get("kCGWindowBounds", {})
    width = int(round(bounds.get("Width", 0)))
    height = int(round(bounds.get("Height", 0)))
    if 160 <= width <= 190 and 30 <= height <= 38:
        pill = bounds
        break

if pill is None:
    print("no Bluey pill window found", file=sys.stderr)
    sys.exit(2)

x = pill["X"] + pill["Width"] / 2
y = pill["Y"] + pill["Height"] / 2
for typ in (Quartz.kCGEventMouseMoved, Quartz.kCGEventLeftMouseDown, Quartz.kCGEventLeftMouseUp):
    event = Quartz.CGEventCreateMouseEvent(None, typ, (x, y), Quartz.kCGMouseButtonLeft)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, event)
    time.sleep(0.08)
PY

sleep 0.8

before_json="$(python3 <<'PY'
import json
import Quartz
rows = []
for win in Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID) or []:
    owner = (win.get("kCGWindowOwnerName") or "").lower()
    if owner in ("bluey overlay", "bluey-overlay-macos"):
        b = win.get("kCGWindowBounds", {})
        rows.append({"width": int(round(b.get("Width", 0))), "height": int(round(b.get("Height", 0))), "x": int(round(b.get("X", 0))), "y": int(round(b.get("Y", 0))), "sharing": win.get("kCGWindowSharingState")})
print(json.dumps(rows))
PY
)"

echo "[visual-smoke] starting simulated audio"
"$BLUEY_BIN" audio start >/tmp/bluey-overlay-visual-smoke.audio.log 2>&1 || true
sleep 4

after_json="$(python3 <<'PY'
import json
import Quartz
rows = []
for win in Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID) or []:
    owner = (win.get("kCGWindowOwnerName") or "").lower()
    if owner in ("bluey overlay", "bluey-overlay-macos"):
        b = win.get("kCGWindowBounds", {})
        rows.append({"width": int(round(b.get("Width", 0))), "height": int(round(b.get("Height", 0))), "x": int(round(b.get("X", 0))), "y": int(round(b.get("Y", 0))), "sharing": win.get("kCGWindowSharingState")})
print(json.dumps(rows))
PY
)"

export BEFORE_JSON="$before_json"
export AFTER_JSON="$after_json"
export EXPECTED_HEIGHT MIN_WIDTH MAX_WIDTH
python3 <<'PY'
import json
import os
import sys

def expanded(rows):
    candidates = [row for row in rows if row["height"] > 100]
    if not candidates:
        raise AssertionError("no expanded overlay window found")
    return max(candidates, key=lambda row: row["width"] * row["height"])

before = expanded(json.loads(os.environ["BEFORE_JSON"]))
after = expanded(json.loads(os.environ["AFTER_JSON"]))
expected_height = int(os.environ["EXPECTED_HEIGHT"])
min_width = int(os.environ["MIN_WIDTH"])
max_width = int(os.environ["MAX_WIDTH"])

for label, row in [("before", before), ("after", after)]:
    if abs(row["height"] - expected_height) > 1:
        raise AssertionError(f"{label}: expected height {expected_height}, got {row['height']}: {row}")
    if not (min_width <= row["width"] <= max_width):
        raise AssertionError(f"{label}: expected width {min_width}..{max_width}, got {row['width']}: {row}")

if before["width"] != after["width"] or before["height"] != after["height"]:
    raise AssertionError(f"overlay resized during transcript updates: before={before} after={after}")

print(f"[visual-smoke] bounds stable: {after['width']}x{after['height']}")
PY

shot="$OUT_DIR/macos-overlay-visual-smoke.png"
screencapture -x "$shot"
echo "[visual-smoke] screenshot: $shot"
"$BLUEY_BIN" status
echo "[visual-smoke] PASS"
