# ROUND-404 System Design Follow-Up Canvas

Date: 2026-07-07
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Make system-design follow-ups behave naturally in the overlay:

- A full system-design answer owns the right-side canvas.
- A continuation or section request, such as "what about failure modes?" or "continue with rollout", appends to the active canvas instead of replacing it.
- A focused explanation, such as "why did you choose Redis for counters?", answers on the left while preserving the canvas on the right.
- A clearly new design prompt still creates a new canvas.

## What Changed

- Added backend AnswerPlan detection for system-design follow-ups with previous design context.
- Split design follow-ups into two behaviors:
  - `SystemDesign` + `CanvasDetail` for section/continuation follow-ups that should update the canvas.
  - `FollowUp` + `Compact` for explanation-only follow-ups that should stay in the conversation.
- Updated the system-design prompt contract so a canvas continuation answers only the requested section and does not repeat the whole earlier design.
- Updated macOS overlay canvas routing so system-design section follow-ups append to the active canvas.
- Expanded macOS canvas preservation signals for Redis, counters, gateways, services, workers, tokens, and related design language.

## Expected User Behavior

If the user asks:

```text
Design a URL shortener.
```

Bluey opens a system-design canvas on the right.

If the user later asks:

```text
What about failure modes?
```

Bluey appends a failure-modes follow-up section to that same canvas. The earlier requirements, API, data model, and architecture remain available.

If the user asks:

```text
Why did you choose Redis for counters?
```

Bluey answers on the left as a normal explanation, and the right-side design canvas stays open unchanged.

## Verification

- `cargo test --manifest-path server/Cargo.toml answer_plan_system_design_ -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml --lib --quiet`
- `swift build -c release` from `native/macos/cue-overlay`
- `git diff --check`

## Notes

- This round changes routing and overlay source behavior only.
- Server deployment is needed for backend AnswerPlan behavior.
- Desktop binary publication still depends on the normal signed release path; do not ship unsigned local binaries as production downloads.
