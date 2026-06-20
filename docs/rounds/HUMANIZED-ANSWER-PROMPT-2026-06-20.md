# Humanized Answer Prompt Round - 2026-06-20

## Why

The user shared `/Users/uno/Downloads/prompt (1).txt` as a reference for answers that sound like a person responding live instead of an AI writing a generic memo. The useful product-level pattern is not the Amazon interview specifics; it is the answer discipline:

- infer the question type from framing;
- answer in a natural spoken voice;
- use context and documents as the source of truth;
- ask clarifying questions only when they materially change the answer;
- keep follow-ups short and focused;
- for technical answers, state assumptions, tradeoffs, and a clear call.

## What Changed

- Updated the managed provider prompt in `crates/cue-daemon/src/app.rs`.
- Updated the direct answer prompt in `crates/cue-daemon/src/llm/answer.rs`.
- Added test assertions so prompt regressions are caught.

## Prompt Contract

Bluey now tells providers to:

- start with a short answer the user could say naturally;
- infer whether the task is a quick answer, follow-up, coding/debugging, system design, meeting recap, writing, or screen analysis;
- answer first, then add the reason, assumption, tradeoff, or example;
- ask at most 1-3 clarifying questions only when needed;
- use the latest relevant transcript/screen/document context and avoid stale repeats;
- answer follow-ups as deltas instead of restarting the full answer;
- avoid assistant preambles and AI-sounding filler;
- keep deeper code/design details in the structured sections or canvas artifact.

## What Stayed Out

The reference prompt's Amazon-specific role, resume, leadership-principle mapping, story inventory, and interview-only rules were intentionally not copied into Bluey. Those belong in a user-provided Tone/session prompt, not the product default.

## Verification

- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape test_answer_llm`
- `cargo fmt --all --check`

