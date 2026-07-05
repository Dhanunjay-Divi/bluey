# ROUND-386 Code Line Follow-Up Grounding

Date: 2026-07-05
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Why

Bluey answered a code follow-up with wording like "line 32 is probably..." even though the code was already in Bluey's own canvas. That is the wrong product behavior: if Bluey generated the workbench code, follow-up questions about visible line numbers should be grounded in that exact artifact, not guessed from vague memory.

The underlying gap was two-part:

- Recent coding context included the prior artifact body, but not the same display line numbers the user sees in the code panel.
- Follow-up detection did not treat "explain line 3 / line 32" as a coding follow-up unless the user also said "code".

## What Changed

- Added a line-numbered code excerpt to the recent coding context for immediate coding follow-ups.
- Expanded contextual coding follow-up detection so line-number references and current code panel references use the latest code artifact.
- Updated native overlay prompt rules to treat prior code artifact display line numbers as authoritative.
- Updated managed/server AnswerPlan coding-follow-up instructions with the same no-guessing rule.
- Updated the LLM answer prompt used by the local path with the same line-number grounding rule.
- Added regression coverage proving line-numbered code is supplied for follow-ups like "Can you explain line 3 and line 4?"

## Expected Behavior

If the user asks:

- "What does line 32 do?"
- "What is the difference between line 32 and 33?"
- "Explain this line."
- "What does `board[row][col]` mean?"

Bluey should use the current code artifact as the source of truth. If the referenced line is present, it should answer directly. It should not say "probably", "likely", or "I think". If the exact line is not available in context, it should say the exact line is not available instead of guessing.

## Verification

Passed:

- `cargo test -p cue-daemon meeting_context_adds_display_line_numbers_for_code_followups -- --nocapture`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture`
- `cargo test -p cue-daemon llm::answer -- --nocapture`
- `cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes -- --nocapture`
- `cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions -- --nocapture`
- `cargo test -p cue-daemon meeting_context_keeps_focused_code_prompt_for_same_java_follow_up -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml answer_plan --lib -- --nocapture`

## Notes

This does not change line rendering or copy behavior in the UI. It fixes the answer-grounding path so follow-ups about the code panel can reference the same code and line numbers the user sees.
