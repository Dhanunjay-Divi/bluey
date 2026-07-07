# ROUND-403-CAPACITY-RETRY-AND-CHAT-GAP

Date: 2026-07-06
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Branch/worktree: `/Users/uno/Downloads/cue-answerplan-fix`

## Goal

Fix the production rough edges seen in live Bluey testing:

- Brief provider/key cooldowns surfaced as final "Capacity busy" chat answers even though another retry a moment later would likely work.
- One user could hit visible capacity messages despite Bluey having multiple providers configured.
- Large blank gaps appeared between answers and the next question in the macOS overlay history feed.
- Keep enough behavior documented so future logs/screenshots can be traced without relying on private user content.

## Changes

### Backend Capacity Smoothing

- Added a hidden one-time retry sweep when all provider routes fail only because of a short provider/key cooldown.
- The retry waits only 1-2 seconds, then tries the full route list again.
- Account, billing, deleted-account, abuse, and real longer-capacity guards still fail closed and are not silently retried.
- Reduced the default upstream 429 cooldown with no provider `Retry-After` header from 30 seconds to 2 seconds so one transient provider response does not make Bluey look broken for a whole half-minute.
- Added structured logs for the internal retry sweep:
  - `all streaming routes briefly capacity busy; waiting before internal retry sweep`
  - `all routes briefly capacity busy; waiting before internal retry sweep`

### macOS Chat Gap Fix

- Made macOS feed rows, bubbles, labels, and the feed stack strongly hug their vertical content.
- Added a shared `finishInstallingCardView` path so newly inserted and streaming-replaced cards get the same sizing behavior.
- Forced a layout/intrinsic-size refresh after card replacement so streaming updates cannot leave stale oversized rows behind.
- Kept the visible chat spacing unchanged; this fixes row stretching, not the intentional 8px feed spacing.

## Files Changed

- `server/src/api/router.rs`
- `server/src/routing/dispatcher.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

## Verification

- `cargo test --manifest-path server/Cargo.toml --lib --quiet`
  - Passed: 261 tests
- `swift build -c release` in `native/macos/cue-overlay`
  - Passed
- `git diff --check`
  - Passed

## Notes

- Windows does not use the same multi-card AppKit feed stack, so the chat-gap patch is macOS-specific.
- This does not remove legitimate capacity limits. It only hides brief provider/key cooldown noise from the user when Bluey can safely retry.
- If "Capacity busy" still appears after this change, that should now mean either all routes are genuinely unavailable past the short retry window, or an account-level guard is intentionally blocking work.
