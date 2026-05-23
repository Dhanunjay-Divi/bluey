# Mac Overlay Production Smoke Close-Out

Date: 2026-05-23

## Scope

This close-out covers the user-visible macOS overlay regressions found during
Phase 2 Mac smoke:

- Expanded overlay chrome must fit the visible screen and never crop header,
  balance, model selection, or composer controls.
- Live transcript must stay in the fixed bottom caption strip.
- Transcript updates must not create repeated chat bubbles or resize the
  expanded overlay.
- Customer launch paths must keep the overlay capture-excluded; capture-visible
  mode is dev-only and test-only.

## Fixes Landed

- `3ceb2ee fix(macos): bound live transcript strip`
  - `transcript` cards are intercepted before they enter the main feed.
  - Transcript text now updates only the fixed bottom caption strip.
  - Caption strip uses a horizontal `NSScrollView` and tails to the newest text.
  - Smoke doc now flags transcript bubbles/window growth as Step 3 failures.

- Current follow-up commit
  - Added `scripts/macos-overlay-visual-smoke.sh`.
  - The script builds debug Bluey + native overlay, launches with dev-only
    capture-visible flags, clicks the pill, starts simulated audio, asserts the
    expanded overlay remains fixed-size, captures a screenshot, prints status,
    and shuts Bluey down.
  - Updated `docs/deploy/PHASE2-MAC-SMOKE.md` to make this the optional
    repeatable guard for Steps 1-3.

## Verification

Commands run:

```bash
swift build -c release --package-path native/macos/cue-overlay
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
(cd crates/cue-dashboard/ui && npm test -- --run && npm run build)
(cd server && cargo clippy --all-targets -- -D warnings && cargo test)
bash -n scripts/macos-overlay-visual-smoke.sh
scripts/macos-overlay-visual-smoke.sh
git diff --check
```

Results:

- Visual smoke: PASS.
- Expanded overlay bounds stayed `820x520` before and after simulated transcript
  updates.
- Normal customer-mode `bluey status` reports `overlay_capture_excluded: true`.
- Quartz window list in normal mode reports Bluey Overlay windows with
  `kCGWindowSharingState = 0`.
- Dev visual-smoke screenshot:
  `/tmp/bluey-smoke-shots/macos-overlay-visual-smoke.png`.

## Notes For Review

- The visual smoke intentionally uses `BLUEY_DEV_OVERLAY=1` and
  `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1`; that is required for screenshot-based
  QA and must not be used in customer launch paths.
- `BLUEY_AUDIO_SIMULATED_ONLY=1` is used so the smoke does not depend on real
  microphone/system-audio permissions.
- Steps 4-10 of `docs/deploy/PHASE2-MAC-SMOKE.md` still require a managed
  account/server path for answer streaming, cost labels, cloud sync, and RAG.
