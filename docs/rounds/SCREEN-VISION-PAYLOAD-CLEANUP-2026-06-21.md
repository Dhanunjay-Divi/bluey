# Screen Vision Payload Cleanup - 2026-06-21

## Why

Live Screen -> Answer testing showed two different failures:

- The desktop could send too much image data after screenshot fallback, which can hit request/proxy limits before the managed vision route runs.
- The fallback card and attachment note exposed noisy browser automation details instead of a simple user-facing "screen context ready" state.

The server logs also showed upstream 502s from managed provider routes. This round fixes the desktop-side payload/noise issues, but the provider route still needs a live smoke after the server config is verified.

## Changes

- macOS screen fallback now captures JPEG instead of PNG to keep normal full-screen screenshots smaller before base64 upload.
- Managed vision upload cap on the desktop is now 4 MB per image data URL, matching a safer client-side budget below server/proxy limits.
- Oversized image files are rejected before reading the whole file into memory.
- Screenshot fallback attachment notes no longer include raw browser/AppleScript failure text.
- The visible fallback card now says only that page text was unavailable and that Bluey captured a screenshot.
- Raw browser capture failure details are moved to debug logs under `bluey::screen`.
- The old visible question body for `overlay screenshot analyse` no longer injects "Analyse the captured screenshot context" when the user has a real question.

## Verification

- `cargo fmt --all --check`
- `cargo test -p cue-daemon provider_messages -- --nocapture`
- `cargo test -p cue-daemon answer_overlay_artifact -- --nocapture`
- `cargo test -p cue-daemon llm_overlay_artifact_preserves_managed_code_canvas -- --nocapture`
- `git diff --check`
- `cargo build -p cue-cli -p cue-daemon --release`
- Refreshed local install:
  - `/Users/uno/.bluey/bin/bluey --version` -> `bluey 0.1.13`
  - `/Users/uno/.bluey/bin/bluey-daemon --version` -> `bluey-daemon 0.1.13`

## Still Pending

- Live Screen -> Answer smoke with a funded account after restarting Bluey.
- Server-side provider smoke for vision, because local logs previously showed managed provider 502s.
- If full-screen JPEGs are still too large on high-resolution displays, add a native downscale/compress step before upload instead of only omitting oversized images.
