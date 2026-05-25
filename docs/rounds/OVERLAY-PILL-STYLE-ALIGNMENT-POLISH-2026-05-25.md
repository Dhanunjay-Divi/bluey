# Overlay Pill / Style / Alignment Polish

Date: 2026-05-25

## Why This Round Happened

The macOS overlay looked too uneven for product testing:

- The collapsed Bluey pill did not feel close enough to the Pinky-style compact
  launcher.
- The expanded header controls were technically present but visually noisy.
- `KB empty` appeared in two places: header badge and lower attachment strip.
- The Style flow reused the session drawer, which made it look like an
  off-center popup.
- Close confirmation text/buttons were not visually centered.
- Live transcript needed to stay inside a fixed strip and never grow the
  overlay.

## What Changed

File changed:

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

Changes:

- Reworked `PillMetrics` and `PillView` geometry:
  - 128 x 38 compact pill.
  - Larger centered logo tile.
  - Stronger but cleaner cyan border/glow.
  - Dot pulled closer to `Bluey`.
  - Title and logo vertically centered.
- Split Style out of the session drawer:
  - Added centered `answerStyleOverlay` + `answerStylePanel`.
  - Save button is centered with icon/text group.
  - Session drawer now stays focused on recordings only.
- Fixed KB duplication:
  - Empty state now lives only in the header badge.
  - Attachment strip height collapses to `0` when no docs are attached.
  - Attachment strip appears only for loading or loaded docs.
- Center-aligned popup content:
  - Close confirmation title/body centered.
  - Close-confirm buttons use centered icon+text grouping.
- Kept live transcript bounded:
  - Smoke checked that transcript updates remain inside the horizontal strip
    while simulated audio emits many chunks.

## Manual Smoke Notes

Run locally in capture-visible dev mode only:

```bash
./target/debug/bluey off
BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 ./target/debug/bluey on
```

Observed:

- Pill visible and compact.
- Pill click opens expanded overlay.
- Style button opens centered style panel.
- Save closes style panel and emits style update.
- Close button opens centered close-confirm panel.
- KB empty appears only in the header.
- Attach click changes header to `KB loading` and shows one loading chip.
- Listen creates simulated transcript chunks without growing the window.

After testing:

```bash
./target/debug/bluey off
```

## Verification

```bash
swift build -c release --package-path native/macos/cue-overlay
bash native/macos/cue-overlay/build.sh
git diff --check
```

All passed.

## Remaining UI Work

Next UI pass should focus on:

- Final ChatGPT-style composer polish once backend wiring is stable.
- Full session drawer visual QA with real session rows.
- Balance/cost labels against a managed account.
- Canvas auto-open and collapse/reopen with real code/system-design artifacts.
- Screen-capture exclusion smoke with normal production mode.
