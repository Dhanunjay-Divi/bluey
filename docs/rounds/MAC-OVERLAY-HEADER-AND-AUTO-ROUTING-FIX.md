# Mac Overlay Header + Auto Routing Fix

Date: 2026-05-23
Branch: `feat/phase-3-round-12`

## Why

During Mac smoke, the expanded overlay stayed at the right outer bounds but the
header chrome disappeared again. That meant the user lost session navigation,
model/routing, balance, hide, and close controls. Separately, Bluey Auto still
surfaced the older draft-then-replace behavior by default, which is fast but
confusing in a live conversation.

## What Changed

- Expanded overlay frames are fitted to the visible screen before display.
- Header, transcript strip, attachment strip, and composer are treated as fixed
  chrome; only the middle workspace is allowed to compress and scroll.
- Header chrome is explicitly re-fronted during layout so feed/content cannot
  cover it.
- The collapsed pill is now the default launcher state: centered, compact,
  identity-only, and replaced by the expanded panel when clicked.
- Hide in the expanded panel returns to the centered pill instead of leaving a
  second launcher visible behind the panel.
- Expanded panel mouse handling is limited to fixed controls/drawer/composer;
  the host content band is pass-through-oriented for remote control safety.
- Composer growth remains ChatGPT-like, but capped lower so it cannot squeeze
  the fixed chrome rows out of the 520px panel.
- Deep and managed lanes now prefer streaming.
- Bluey Auto remains default-on for classification/routing, but normal product
  mode streams one visible selected-lane answer. Parallel cheap-draft + deep
  replacement is now internal-only behind `BLUEY_PARALLEL_DRAFTS=1`.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `scripts/macos-overlay-visual-smoke.sh`
  - expanded overlay stable at `820x520`
  - header controls visible in screenshot
  - transcript updates did not resize the panel
- `cargo fmt --all --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --all-targets`
- `cd crates/cue-dashboard/ui && npm run build`
- `git diff --check`

## Notes For Next Agent

- Do not remove the fixed-header re-fronting unless replacing it with a
  stronger AppKit layout contract that is visually smoked.
- Do not make `BLUEY_PARALLEL_DRAFTS=1` customer default without a product
  decision; visible answer replacement was explicitly demoted.
- The server `/router/complete/stream` still emits chunks after the full
  upstream completion today. True upstream token streaming remains the next
  backend connection improvement.
