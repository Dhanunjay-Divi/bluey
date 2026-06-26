# Round 035 - Overlay UX + Production Safeguard Pass — 2026-06-08

## Scope

This pass focused on the remaining product polish that could be completed locally without provider keys, billing live-mode changes, or a multi-server rollout.

## Changes Made

- Improved the unauthenticated overlay state:
  - The sign-in card now reads as a centered "local ready, open login for cloud" state.
  - Removed the small corner "login" status text that made the card look misaligned.
  - Widened the card so the copy wraps cleanly.
  - Renamed the CTA to "Open login".
  - The signed-out card now clears stale lifecycle cards so "Meeting started" / "Meeting ended" do not compete with the login state.
  - Replaced internal "knowledge base" wording in the card with "documents".
- Fixed expanded-header visibility:
  - The top navigation/model/docs/balance controls are added as a normal late root subview instead of using the fragile `positioned: .above, relativeTo: nil` path.
  - The defensive layout pass now pins the header to the visible `NSWindow` frame height instead of trusting a sometimes-stale view `bounds.height`, which is what clipped the top controls in the Mac smoke.
  - The visual smoke now inspects the screenshot crop for bright/accent header pixels so a missing eye/close/top bar fails the test instead of slipping through.
- Kept the existing chat layout contract:
  - Transcript and typed user/question cards stay on the right.
  - Bluey answer cards stay on the left.
  - Canvas auto-opens only for code, system-design, and screen-heavy outputs.
- Polished controls:
  - Renamed the bottom "Style" control to "Tone".
  - Kept the answer-style editor as an inline overlay with a high-contrast white input.
  - Updated placeholder copy to be more natural and customer-facing.
- Cleaned document language:
  - Replaced internal "KB" labels with "Docs" labels in the overlay.
  - Updated attached-file status text to say "Docs empty", "Docs loading", "Docs locked", and "Docs N ready".
  - Updated the native macOS picker and AppleScript fallback to clearly list supported file types and say video/audio/apps/certificates are skipped.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay` passed.
- `swift build -c release --package-path native/macos/cue-picker` passed.
- `cargo fmt --all --check` passed.
- `bash -n ops/backup-bluey-db.sh` passed.
- `bash -n scripts/macos-overlay-visual-smoke.sh` passed.
- `bash scripts/macos-overlay-visual-smoke.sh` passed twice.
- Visual smoke screenshot confirmed:
  - Expanded overlay remains bounded at `820x520`.
  - Header stays visible.
  - Header pixel assertion reports visible controls in the captured screenshot.
  - Login card is centered and readable.
  - Transcript strip scrolls horizontally instead of growing the window.
  - Bottom controls remain aligned.

## Production Safeguard Check

- Secret scan found no real provider keys committed in code. Matches were placeholders, docs examples, redaction tests, or local smoke-test flags.
- `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` remains documented and used only for local visual smoke. The release overlay ignores capture-visible debug mode through the existing DEBUG-only gate.
- Backup script syntax validates locally. A real restore drill on the droplet is still required before broad launch.

## Still Open Before Production Launch

- Add a signed release manifest. Current installer verifies SHA256 over HTTPS; it does not yet verify an offline signature.
- Run a real droplet backup + restore drill.
- Finish monitoring/log export wiring for production operations.
- Move provider 429/capacity state into the shared Redis ledger before scaling beyond one server instance.
- Do final privacy/terms copy review for prompts, screenshots, attachments, transcripts, retention, and deletion.
- Re-run Mac smoke steps 4-10 with a funded managed test account.
