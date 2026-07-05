# ROUND-384 Human Coding Follow-Up Style

Date: 2026-07-05
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Why

Bluey was answering coding follow-ups with technically useful but stiff explanation blocks. In live interview or call usage, follow-ups like "why is this O(N)" or "can we make it better" should sound like a person answering the exact concern first, then adding the reasoning and tradeoff.

## Changed

- Updated server AnswerPlan prompt style for coding and coding follow-up answers.
- Added a spoken lead-in requirement for first-time coding answers before structured sections.
- Made coding follow-ups answer like a live call: direct conclusion first, then reason, caveat, or better option.
- Tightened complexity follow-up instructions so Bluey explains which part has that complexity and whether the whole algorithm can actually improve.
- Mirrored the same style in local overlay mode instructions and the local answer LLM prompt.
- Added regression assertions so the generated prompts keep the live-call style contract.

## Notes

- This round changes prompt and routing instructions only.
- It does not change provider routing, billing, transcript capture, or UI layout.
- No private transcript, resume, screenshot text, API key, or user document content was copied into this round doc.

## Verification

- `cargo test --manifest-path server/Cargo.toml answer_plan --lib -- --nocapture`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture`
- `cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes -- --nocapture`
- `cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions -- --nocapture`
- `cargo test -p cue-daemon llm::answer -- --nocapture`
- `git diff --check`
