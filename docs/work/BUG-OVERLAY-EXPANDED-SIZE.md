# BUG: Mac smoke step 2 fail — expanded overlay is ~720x921, not 720x520

**Branch:** `feat/phase-3-round-12`
**Tip:** `86ca326`
**Reporter:** Kiro (filed during Mac smoke run)
**Owner:** Codex (Swift overlay)

---

## Symptom

`docs/deploy/PHASE2-MAC-SMOKE.md` step 2 says:

> **Expect:** Overlay expands to ~590x510 panel

Codex's smoke observed: pill expands, but the expanded overlay is
~720x921 — too tall, visually overwhelms the screen.

## What the source says

`native/macos/cue-overlay/Sources/cue-overlay/main.swift::ensureExpandedWindow`:

```swift
let expandedSize = NSSize(width: 720, height: 520)
let window = OverlayWindow(
    contentRect: NSRect(origin: expandedOrigin, size: expandedSize),
    draggable: false)
```

Width 720 matches observed. Height 520 declared, observed 921.

Source change history (git blame):
- `9babb20` (Stage 25 batch) bumped from `590x510` → `720x520`
- The smoke doc still says expected `~590x510` (it's stale; needs an
  update to `~720x520`)

## What I checked + ruled out

- No `setContentSize` calls in main.swift
- All `setFrame` calls preserve width/height (just reposition)
- `OverlayWindow` styleMask is `.borderless` so contentRect = window frame
- `ensureRoomForCanvas` only widens to 820, doesn't change height (and
  it only fires when the canvas opens, which the screenshot shows
  isn't open here)
- `captureVisibleForDebug` (env-set in codex's smoke) only flips
  `sharingType`, not size

## What I suspect (you'll know better)

The window is created at 720x520 but somewhere autolayout is growing
the height to fit content. Possible causes:

1. The chat scroll view has bottom-up constraints with no max height
2. The "How Bluey should answer" inline textbox is auto-growing
3. NSWindow's `contentResizesToFit*` or autoresizing mask is on
4. Some constraint between header → chat → captions strip → composer
   doesn't have a height anchor and the window expands to satisfy

The screenshot (`/tmp/bluey-smoke-shots/step2-expanded-after-click.png`)
shows ~700-800px of empty dark space between the header and the first
chat message. That's the giveaway: content is bottom-anchored in a
much taller container than 520.

## Suggested fix

Quick fix — enforce window min/max size at creation:

```swift
let expandedSize = NSSize(width: 720, height: 520)
let window = OverlayWindow(
    contentRect: NSRect(origin: expandedOrigin, size: expandedSize),
    draggable: false)
window.minSize = expandedSize
window.maxSize = expandedSize
window.contentMinSize = expandedSize
window.contentMaxSize = expandedSize
```

This locks the window. If the chat content overflows, the scroll view
should handle it (which is the design intent).

Real fix — find the autolayout constraint that's pushing height and
clamp it. The chat scroll view's bottom anchor probably needs to pin
to (captions strip top - margin) with `priority = .required` and the
content view's `intrinsicContentSize` height should be ignored.

## Side issue: smoke doc has stale expected size

`docs/deploy/PHASE2-MAC-SMOKE.md` step 2 says "expanded overlay
~590x510 panel". After codex's `9babb20` Stage 25 batch widened to
720x520 (with code canvas opening to 820 wide), the smoke doc was
not updated.

Once the runtime size is fixed, please update the smoke doc to
match the new design.

## Pre-flight artifact note

I also rebuilt `/tmp/bluey-internal-test/` from current HEAD because
the previous bundle was stale (didn't have `bluey doctor`). Codex
should now be able to use either the artifact OR the repo release
build directly — `target/release/bluey` and `target/release/bluey-daemon`
are both at `86ca326`-state. The artifact run script prints the HEAD
commit on launch so it's obvious if anyone uses a stale bundle.

## How to verify after fix

1. Build:
   ```bash
   swift build -c release --package-path native/macos/cue-overlay
   ```
2. Run pill + click:
   ```bash
   bash /tmp/bluey-internal-test/run-internal-test.sh
   bluey on
   # click pill in top-right
   ```
3. Use Quartz Window List to read the actual frame:
   ```bash
   python3 -c "
   import Quartz
   wins = Quartz.CGWindowListCopyWindowInfo(
       Quartz.kCGWindowListOptionOnScreenOnly,
       Quartz.kCGNullWindowID,
   )
   for w in wins:
       owner = w.get('kCGWindowOwnerName', '')
       if 'BlueyOverlay' in owner or 'cue-overlay' in owner:
           b = w.get('kCGWindowBounds', {})
           print(owner, b)
   "
   ```
4. Expected after fix: `Width: 720, Height: 520`

## Round-close ack

This bug is from the Mac smoke that's part of the closed-alpha gate,
not from the Observability Round itself. The Observability Round
remains 🟢 closed; this is a Stage 25 / UI canvas regression that
slipped through because we never visually-verified the new 720x520
size on a real Mac after `9babb20`.

## 2026-05-22 Fix

Codex fixed the runtime size regression by clamping the expanded
`NSWindow` at creation and by making `OverlayWindow` enforce that
fixed size for later `setFrame` / `setContentSize` calls:

```swift
window.minSize = expandedSize
window.maxSize = expandedSize
window.contentMinSize = expandedSize
window.contentMaxSize = expandedSize
```

The smoke doc expectation was also updated from `~590x510` to
`~720x520` to match the Stage 25 canvas-ready design.

Local verification after rebuilding `native/macos/cue-overlay`:

```text
pill:     110x30
expanded: 720x520
```

Screenshots:

- `/tmp/bluey-smoke-shots/step1-after-size-fix.png`
- `/tmp/bluey-smoke-shots/step2-after-size-fix.png`

The `/tmp/bluey-internal-test` overlay helper and
`Resources/BlueyOverlay.app` were refreshed from the rebuilt
`target/release` overlay artifacts.

## 2026-05-22 Follow-up Fixes During Step 3/4 Smoke

The initial lock fixed the Step 2 height, but Step 3 exposed two
additional AppKit/autolayout follow-ups:

1. Clicking **Start Bluey** could preserve an invalid later origin
   from AppKit sizing and move the expanded window off-screen
   (`Y = 10107`). `OverlayWindow` now clamps later `setFrame` /
   `setContentSize` calls back into the visible screen.
2. Dense composer/header constraints could ask the window for an oversized
   width (`Width = 1704`). The normal expanded window now chooses a
   screen-fitting adaptive width capped at 820px, with a lower minimum on
   smaller displays so the nav, model selector, balance, and composer
   controls fit instead of being cropped. The canvas path explicitly
   raises that cap to 960 only while the canvas is open, then restores the
   screen-fitting compact width when it closes.
3. Long composer text could push the Style / Docs / Screen / Answer
   controls out of the 720-wide panel. The composer now has low
   horizontal hugging and compression resistance, uses a scrollable
   non-wrapping text cell, and the action controls keep required
   horizontal compression resistance.

Latest local smoke evidence after rebuilding and refreshing
`/tmp/bluey-internal-test`:

```text
Step 3 Start: expanded window stayed compact at a valid on-screen origin.
Step 3 Stop: transcript segment count stopped advancing and the button
             returned to Start Bluey.
Step 4 UI:   header and composer controls remained visible with a long
             prompt typed into the surface.
Step 6:      normal mode reports overlay_capture_excluded=true and Quartz
             reports the expanded overlay as sharing=0. A standard
             macOS screenshot and 3-second screen recording omitted the
             expanded overlay while it was on screen.
```

Smoke artifacts:

- `/tmp/bluey-smoke-shots/step3-start-fixed-width.png`
- `/tmp/bluey-smoke-shots/step3-stop-fixed.png`
- `/tmp/bluey-smoke-shots/step4-composer-fit.png`
- `/tmp/bluey-smoke-shots/step4-after-send-visible-buttons.png`
- `/tmp/bluey-smoke-shots/step6-capture-excluded-normal.png`
- `/tmp/bluey-smoke-shots/step6-capture-excluded-normal.mov`

The full Step 4 managed-answer assertions remain account-gated on this
machine: `bluey doctor` reports the test install is logged out, so
real answer streaming, cost labels, and managed canvas artifacts need
either a staging server login or a production test account.
