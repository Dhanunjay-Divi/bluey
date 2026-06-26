# Round 173 - Follow-Up Active Context Hydration

## Issue

Bluey could show old question and screen-context cards in the visible overlay after a local restart while the daemon's active meeting file was actually a fresh empty session. A follow-up such as "that's not the answer right?" then had no saved screenshot, document, or previous answer context to send, so the model asked for a fresh screen even though the UI appeared to show prior work.

## Change

- On overlay readiness, empty or missing active meetings now clear the overlay surface instead of leaving stale cards visible.
- Starting a fresh empty session also clears the overlay before showing new session state.
- Follow-up context reuse now has a fallback path: if a prior turn missed attachment ids but the active meeting still has usable saved screen/document memory, Bluey reuses the recent saved artifact for immediate follow-ups.
- Added a regression test for recovering a recent saved screen when attachment ids are absent.

## Verification

- `cargo fmt --check -p cue-daemon`
- `cargo test -p cue-daemon follow_up_context --lib`
- `cargo build --release -p cue-cli -p cue-daemon`
- Refreshed local visible Bluey binaries and restarted visible mode.

## Note

This fixes context reuse when the active meeting actually has saved context. If the active meeting has already been reset and contains zero context items, Bluey cannot reconstruct the old screenshot from stale overlay cards alone. The UI now clears that mismatch so the user sees the real state.
