# Round 028 - Overlay Signed-Out Login CTA — 2026-06-04

## Why

`bluey on` correctly starts the local overlay before the user is logged in, but the expanded overlay still looked like a normal ready session. That made the product feel broken: the pill appeared, yet cloud-backed session continuation, balance, managed answers, sync, and RAG were not available.

## What Changed

- `bluey on` now sends a distinct boot title when the account is not linked: `Sign in to Bluey`.
- The signed-out boot payload includes a machine-readable `login_url:` line.
- The macOS native overlay detects that login URL and renders the boot card as a setup card with a centered `Sign in` button.
- Clicking the button opens the login URL from the overlay with `NSWorkspace`.
- The expanded header now reflects signed-out state:
  - status: `Login needed`
  - route badge: `Sign in`
  - balance: `Login`
  - knowledge badge: `KB locked`
  - pill dot: warning color instead of green
- The linked-account boot path remains unchanged: `Bluey online`, new recording ready, managed answers ready.

## Verification

- `cargo fmt --all --check`
- `cargo test -p cue-cli bluey_on_boot`
- `cargo clippy -p cue-cli --all-targets -- -D warnings`
- `swift build -c release --package-path native/macos/cue-overlay`
- `git diff --check`

## Notes

The pill still appears before login because it is the local control handle. The expanded panel now makes the limitation explicit and gives the user a single clear next action.
