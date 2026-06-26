# Round 135 - Adaptive Answer Depth - 2026-06-23

## What changed

- Rewired Bluey's answer-shape rules so response length is chosen from the user's intent, not just the topic.
- Added explicit depth tiers:
  - Tiny: greetings, yes/no checks, quick status, "is this right", and "which one" get 1-2 useful sentences.
  - Short: definitions, quick explanations, and "what is X" get 2-4 natural sentences, usually no bullets.
  - Medium: normal how/why, product, and debugging guidance gets the answer plus the main reason, tradeoff, or next step.
  - Deep: interview stories, hard debugging, algorithms, architecture, system design, edge cases, and explicit depth requests can be longer.
- Added guardrails so Bluey does not pad a simple technical question, and does not over-compress a complex answer the user needs to defend.
- Applied the same rule to both the managed provider path and the local answer LLM path.

## Expected behavior

- "What is a VPC?" should be a short human explanation, not a full reference article.
- "Design a scalable interview copilot" should use the canvas/workbench for architecture depth while keeping chat readable.
- "Why vector of vector?" should answer the follow-up directly without replacing the code canvas.
- "Tell me about a time..." should still produce a complete speakable interview answer because that question needs a longer talk track.

## Verification

- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture`
- `cargo test -p cue-daemon test_answer_llm -- --nocapture`
