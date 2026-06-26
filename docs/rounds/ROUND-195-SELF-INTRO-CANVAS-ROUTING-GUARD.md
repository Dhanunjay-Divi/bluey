# Round 195 - Self Intro Canvas Routing Guard

## Trigger

Owner showed a Bluey answer to "tell me about yourself" opening the canvas as `Q1 System Design` with 88% confidence. The answer itself was a resume/interview intro, but it mentioned APIs, throughput, distributed systems, and architecture.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 14:57 EDT

## Root Cause

- The managed server artifact detector classified finished answers by counting technical keywords.
- A self-introduction answer can naturally mention `APIs`, `throughput`, `distributed systems`, and `architecture`.
- The server then emitted a `system_design` artifact with confidence `0.88`.
- The macOS overlay trusted that artifact and named the canvas `Q1 System Design`.
- The local daemon and macOS fallback classifier also used keyword-style checks, so they needed the same guard even though the local daemon already had a stricter structured-shape gate.

## Implemented

- Managed server:
  - Replaced raw system-design keyword counting with `looks_like_system_design_artifact`.
  - Added `looks_like_interview_profile_answer` to block self-intro and behavioral interview answers from becoming system-design artifacts.
  - Kept real system-design answers eligible when they explicitly ask for system design or have structured design shape.
  - Added a regression test using the same self-intro pattern from the screenshot.
- Local daemon:
  - Added the same self-intro/behavioral guard to `looks_like_system_design_answer`.
  - Added a regression test so local/fallback answering does not open a system-design canvas for a profile answer.
- macOS overlay:
  - Added the same guard to the fallback `looksLikeSystemDesign` detector for older cards and local context-derived artifacts.
- Windows:
  - No Windows overlay canvas/artifact classifier exists in `native/windows/cue-overlay/main.c`, so there was no Windows-side equivalent to patch.

## Files Touched

- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-195-SELF-INTRO-CANVAS-ROUTING-GUARD.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

- `cargo fmt --check -p cue-daemon`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test --manifest-path server/Cargo.toml response_artifact_ -- --nocapture`
- `cargo test -p cue-daemon answer_overlay_artifact_ -- --nocapture`

Note:

- `cargo fmt --check --manifest-path server/Cargo.toml` still reports unrelated pre-existing rustfmt drift in server files, so this round did not blanket-format the server crate.

## Current State

- "Tell me about yourself" / "tell me about myself" style answers should stay as normal chat answers.
- Technical resume words like API, throughput, distributed systems, and architecture no longer trigger `Q1 System Design` by themselves.
- Real system-design answers still open canvas when the answer is explicitly a system-design artifact or has structured design shape.

## Remaining QA Gates

- Run one live managed-answer smoke with a resume/JD attached:
  - ask "tell me about yourself"
  - confirm chat answer appears
  - confirm no `Q1 System Design` canvas opens
- Run one live system-design smoke:
  - ask a real design question
  - confirm the canvas still opens when the answer contains design structure
