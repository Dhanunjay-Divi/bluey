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
require_source "askButton = NSButton(title: \"Answer\""
require_source "styleIconButton(attachButton, symbol: \"plus\""
require_source "styleControlButton(instructionsButton, symbol: \"text.bubble\""
require_source "styleControlButton(askButton, symbol: \"arrow.up\", accent: true)"
require_source "modelMenu.trailingAnchor.constraint(equalTo: analyzeButton.leadingAnchor"
require_source "composerBarHeightConstraint?.constant = textHeight + 62"
require_source "opacityLabel.stringValue = \"Opacity\""
require_source "opacityControl.widthAnchor.constraint(equalToConstant: 154)"
require_source "knowledgeBadge = NSTextField(labelWithString: \"Docs empty\")"
require_source "routeBadge = NSTextField(labelWithString: \"Auto · ready\")"
require_source "composerSurface.addSubview(recordingButton)"
require_source "composerSurface.addSubview(askButton)"
require_source "sessionDrawer.widthAnchor.constraint(equalTo: widthAnchor, multiplier: 0.46)"
require_source "emitEvent([\"type\": \"session_delete_requested\""
require_source "NSButton(title: \"\", target: self, action: #selector(deleteSessionClicked(_:)))"
require_source "addSubview(headerBar)"
require_source "configureFixedChromeLayoutPriorities()"
require_source "keepFixedChromeInBounds()"
require_source "PillMetrics.centeredFrame"
require_source "startExpandedPassthroughTracking()"
require_source "pillWindow?.orderOut(nil)"
require_source "private struct ResizeEdges"
require_source "resizeEdges(at:"
require_source "private let fullWindowButton = NSButton(title: \"\", target: nil, action: nil)"
require_source "private let copyButton = NSButton(title: \"\", target: nil, action: nil)"
require_source "fullWindowButton.toolTip = \"Expand canvas\""
require_source "copyButton.toolTip = \"Copy canvas\""
require_source "private func hasInteractiveView(at localPoint: NSPoint) -> Bool"
require_source "setFrameTopLeftPoint"
require_source "resizable: false"
require_source "maximumFrameHeight"
require_source "fitExpandedFrameToVisibleScreen"
require_source "setKnowledgeBadge(\"Docs loading\""
require_source "setTranscriptState(\"LISTENING\""
require_source "updateRouteBadge(for: q"
reject_source "Full access"
reject_source "Start Bluey"
reject_source "opacityLabel.stringValue = \"%\""
reject_source "opacityValueLabel.stringValue = \"\\(Int((value * 100.0).rounded()))%\""
reject_source "headerBar.frame.contains(localPoint)"
reject_source "composerBar.frame.contains(localPoint)"

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

def bluey_windows():
    rows = []
    for win in windows():
        owner = (win.get("kCGWindowOwnerName") or "").lower()
        if owner not in ("bluey overlay", "bluey-overlay-macos"):
            continue
        bounds = win.get("kCGWindowBounds", {})
        rows.append(bounds)
    return rows

def expanded_window():
    candidates = []
    for bounds in bluey_windows():
        width = int(round(bounds.get("Width", 0)))
        height = int(round(bounds.get("Height", 0)))
        if width > 400 and height > 100:
            candidates.append(bounds)
    if not candidates:
        return None
    return max(candidates, key=lambda b: b.get("Width", 0) * b.get("Height", 0))

def pill_window():
    for bounds in bluey_windows():
        width = int(round(bounds.get("Width", 0)))
        height = int(round(bounds.get("Height", 0)))
        if 160 <= width <= 190 and 30 <= height <= 38:
            return bounds
    return None

for _ in range(10):
    if expanded_window() is not None:
        sys.exit(0)
    pill = pill_window()
    if pill is None:
        time.sleep(0.2)
        continue
    x = pill["X"] + pill["Width"] / 2
    y = pill["Y"] + pill["Height"] / 2
    for typ in (Quartz.kCGEventMouseMoved, Quartz.kCGEventLeftMouseDown, Quartz.kCGEventLeftMouseUp):
        event = Quartz.CGEventCreateMouseEvent(None, typ, (x, y), Quartz.kCGMouseButtonLeft)
        Quartz.CGEventPost(Quartz.kCGHIDEventTap, event)
        time.sleep(0.08)
    time.sleep(0.35)

print("Bluey pill did not expand after retrying", file=sys.stderr)
sys.exit(2)
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

echo "[visual-smoke] checking idle overlay bounds without mock audio"
"$BLUEY_BIN" audio status >/tmp/bluey-overlay-visual-smoke.audio.log 2>&1 || true
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
export SHOT="$shot"
python3 <<'PY'
import json
import os

import Quartz
from PIL import Image


def expanded(rows):
    candidates = [row for row in rows if row["height"] > 100]
    if not candidates:
        raise AssertionError("no expanded overlay window found")
    return max(candidates, key=lambda row: row["width"] * row["height"])


row = expanded(json.loads(os.environ["AFTER_JSON"]))
image = Image.open(os.environ["SHOT"]).convert("RGB")
display_bounds = Quartz.CGDisplayBounds(Quartz.CGMainDisplayID())
scale_x = image.width / max(1, display_bounds.size.width)
scale_y = image.height / max(1, display_bounds.size.height)

# CGWindow bounds are in display points while screencapture stores pixels on
# Retina hosts. Keep a tiny tolerance by clamping the crop to the image.
x = max(0, int(row["x"] * scale_x))
y = max(0, int(row["y"] * scale_y))
w = max(1, int(row["width"] * scale_x))
header_box = (
    min(image.width, x + 12),
    min(image.height, y + 8),
    min(image.width, x + w - 12),
    min(image.height, y + 58),
)
if header_box[2] <= header_box[0] or header_box[3] <= header_box[1]:
    raise AssertionError(f"invalid header crop: {header_box}, row={row}")

pixels = list(image.crop(header_box).getdata())
bright = sum(1 for r, g, b in pixels if r + g + b > 560)
blue_accent = sum(1 for r, g, b in pixels if b > 80 and g > 80 and r < 130)
yellow_accent = sum(1 for r, g, b in pixels if r > 140 and g > 105 and b < 90)

if bright < 120 or blue_accent + yellow_accent < 35:
    raise AssertionError(
        "expanded overlay header appears missing or clipped: "
        f"bright={bright} accent={blue_accent + yellow_accent} crop={header_box}"
    )

print(
    "[visual-smoke] header visible: "
    f"bright={bright} accent={blue_accent + yellow_accent}"
)
PY
"$BLUEY_BIN" status
echo "[visual-smoke] PASS"
