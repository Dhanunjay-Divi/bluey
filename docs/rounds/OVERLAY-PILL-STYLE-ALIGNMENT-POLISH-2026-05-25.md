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
- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-picker/*`
- `scripts/build-macos.sh`
- `scripts/build-macos-universal.sh`
- `scripts/install.sh`
- `scripts/smoke-test.sh`
- `Makefile`

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
- Fixed file-picker affordance mismatch:
  - macOS `choose file` now uses allowed UTIs plus explicit extensions for
    readable text/code/PDF/DOC/DOCX/RTF/Markdown so unsupported media such as
    `.mp4` is dimmed/unselectable by Finder.
  - Added a dedicated macOS `bluey-file-picker-macos` helper backed by
    `NSOpenPanel` + `NSOpenSavePanelDelegate`. The delegate explicitly disables
    unsupported files instead of relying on AppleScript's inconsistent visual
    filtering in Recents.
  - The daemon discovers this helper next to the installed `bluey` binary,
    under `target/{debug,release}`, or in `native/macos/cue-picker/.build`,
    and only falls back to AppleScript if the helper is missing.
  - Windows OpenFileDialog no longer exposes an `All files` fallback that made
    unsupported formats appear selectable.
- Clarified the conversation send model:
  - `Listen`/transcription continuously captures session context and updates
    the live transcript preview.
  - Silence does not auto-send to the LLM. The idle timer only stops recording
    after the configured no-transcript window to avoid STT spend.
  - `Answer`/send is the explicit LLM boundary. The sent prompt appears as the
    right-side user card, while the streamed Bluey response appears on the left.
  - Raw transcript chunks stay in the live preview strip instead of becoming an
    ever-growing stack of chat bubbles.

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
- Attach picker now highlights only readable Bluey context file types.

After testing:

```bash
./target/debug/bluey off
```

## Verification

```bash
swift build -c release --package-path native/macos/cue-overlay
bash native/macos/cue-overlay/build.sh
bash native/macos/cue-picker/build.sh
cargo fmt --all --check
cargo check -p cue-daemon
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
