# ROUND-464 Fast Answer Preludes And Slow Start Logs

Date: 2026-07-09
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User Issue

The user reported that simple Bluey answers are taking 4-5 seconds, while they used to feel closer to 1-2 seconds. They asked for Bluey to at least show a useful filler/status immediately before the full answer appears, and for small questions to stay fast and reliable.

## Changes

- Made the initial visible answer status intent-aware:
  - `Answering directly...` for short concept/explanation questions.
  - `Working out the approach...` for implementation/debug/code questions.
  - `Explaining the logic...` for code explanation follow-ups.
  - `Structuring the design...` for system-design questions.
  - Existing context-specific messages remain for screen, files, transcript, and saved context.
- Added a fast context path for short standalone conceptual questions with no visible context and no explicit memory/follow-up signal.
  - These questions skip transcript and recent Q&A context gathering so they can route to the instant lane faster.
  - Code follow-ups, screen/document questions, transcript questions, previous-code questions, and explicit memory/session questions keep the richer context path.
- Added slow-start diagnostics:
  - Provider first-event and first-text warnings when managed streaming is slow.
  - `ui_answer_slow_start` session audit event when first visible answer text takes at least 2.5 seconds.
  - Logged fields include request ref, route, provider, context prep time, overlay card time, route time, intent, question size, and visible context count.

## Notes

- This does not fake answer text. The prelude is a UI status line that gets replaced as soon as real model text streams.
- The answer prompt still avoids assistant filler like "Sure" unless the answer type itself naturally calls for it.
- No deploy was performed in this round.

## Verification

- `cargo test -p cue-daemon initial_answer_progress --quiet`
- `cargo test -p cue-daemon fast_conceptual_questions_skip_session_context_lookup --quiet`
- `cargo test -p cue-daemon overlay_auto_routes_short_conceptual_questions_to_instant --quiet`
