# ROUND-444 Bluey On Hidden Overlay Restore

## Trigger

User ran `bluey on` and did not see Bluey on screen.

## Findings

- The installed daemon and native overlay were both running:
  - `/Users/uno/.bluey/bin/bluey-daemon`
  - `/Users/uno/.bluey/bin/bluey-overlay-macos`
- `bluey status` reported `overlay_visible: false`, so the expanded overlay was collapsed/hidden even though Bluey was still on.
- `bluey overlay show` restored the expanded overlay and `bluey status` flipped to `overlay_visible: true`.
- macOS window inspection showed Bluey windows present on the main display, including an expanded window at about `X=1017 Y=44 W=689 H=500`.
- `overlay_capture_excluded: true` is still enabled, so Bluey can be visible to the user but intentionally absent from screenshots or screen-share capture.

## Root Cause

`bluey on` booted the daemon and native overlay, but the CLI deliberately avoided sending `OverlayShow` because the intended launch mode is compact/pill-first. If the previous overlay state was collapsed or hidden, that left a running daemon with no obvious expanded Bluey window.

## Change

Updated `crates/cue-cli/src/app.rs` so `bluey on` checks daemon status before boot. If the existing overlay was hidden/collapsed, `bluey on` sends `OverlayShow` after a successful boot.

This keeps the normal pill-first startup behavior for fresh visible launches, while making repeated `bluey on` recover from a hidden overlay.

## Verification

- Ran `bluey overlay show`; status changed to `overlay_visible: true`.
- Verified Bluey overlay process remained alive.
- Verified actual macOS Bluey windows existed inside the main display bounds.
- Ran `cargo check -p cue-cli --quiet`.
- Ran `git diff --check`.

## Notes

- This was not a signed release deploy.
- If the user is checking through screenshots or screen sharing, Bluey may still appear absent because capture exclusion is working as designed.
