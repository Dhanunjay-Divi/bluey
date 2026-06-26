# Round 160 - Canvas Dedup And Code-Only Artifacts - 2026-06-24

## Why

Screen and document answers were being duplicated into the canvas. That made the left chat and right canvas show the same text, which is confusing. The canvas should be a workbench, not a second chat feed.

## Change

- Stopped daemon-side fallback canvas creation for screen and document answers.
- Ignored managed `screen`, `vision`, `screenshot`, `document`, `docs`, `file`, and generic structured artifacts for overlay canvas rendering.
- Kept managed code, patch, diff, and system-design artifacts.
- Cleaned code canvas formatting so fenced code goes into `CODE`, and only time/space complexity lines go into `COMPLEXITY`.
- Added an overlay guard so older non-code/non-design artifacts already in history do not reopen as duplicated canvas content.
- Removed automatic chat suffixes like `Code is in the canvas.` so the left side stays as the actual answer, not a UI explanation.

## Intended UX

- Normal screen answers stay in chat.
- Document-grounded answers stay in chat.
- Coding answers put code or changed blocks in the canvas.
- Coding follow-ups explain the delta in chat and keep or update the existing canvas only when code actually changes.
- System design answers use the canvas only for deeper architecture/workbench material.

## Verification

- `cargo test -p cue-daemon answer_overlay_artifact --lib`
- `cargo test -p cue-daemon llm_overlay_artifact --lib`
- `cargo check -p cue-daemon`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo build --release -p cue-cli -p cue-daemon`
- `./native/macos/cue-overlay/build.sh`
- Refreshed local install binaries and restarted `scripts/bluey-visible-local.sh`.
