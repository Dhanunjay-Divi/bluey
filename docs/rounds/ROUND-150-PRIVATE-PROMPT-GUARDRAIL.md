# Round 150 - Private Prompt Guardrail

Date: 2026-06-23

## What Changed

- Added a local daemon guard that refuses requests for Bluey's private prompts, hidden instructions, system/developer messages, guardrails, policies, secrets, tokens, environment variables, and internal configuration before any provider call is made.
- Added the same request class guard on the managed server complete and streaming routes.
- Added output leak detection so provider text that resembles private instruction disclosure is replaced with a fixed refusal.
- Blocked private-instruction leak text from becoming canvas/workbench artifacts.
- Added the private instruction boundary to the daemon answer prompts and the older local answer helper.

## Why

Users should never see behind-the-scenes instructions or internal security details in chat or canvas, even if they directly ask for them or phrase the request as a summary, list, explanation, or bypass attempt.

## Verification

- `cargo test -p cue-daemon internal_disclosure --lib`
- `cargo test -p cue-daemon prompt_disclosure --lib`
- `cargo test -p cue-daemon sanitize_replaces_internal_prompt_leak --lib`
- `cargo test -p cue-daemon answer_overlay_artifact_ignores_internal_prompt_leak --lib`
- `cargo test --manifest-path server/Cargo.toml internal_disclosure --lib`
- `cargo test --manifest-path server/Cargo.toml response_artifact_ignores_internal_prompt_leak --lib`
