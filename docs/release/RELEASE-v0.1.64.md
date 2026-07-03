# Bluey v0.1.64

Date: 2026-07-03
Branch: codex/bluey-overlay-spacing-20260626

## Summary

This release hardens coding answers so algorithm/interview prompts produce complete implementations instead of partial loop fragments.

## Changes

- Coding answers now explicitly require:
  - one short approach sentence before code
  - complete fenced code with language tag
  - full class/function signature, initialization, loop/body, return path, and cleanup/sentinel step when relevant
  - `Line notes:` outside the code fence for non-trivial code
- Server code artifacts now require fenced code blocks.
- Loose code-shaped fragments no longer become right-side code canvas artifacts.
- Added regression coverage for the histogram inner-loop fragment case.

## Verification

```bash
cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture
cargo test -p cue-daemon mode_instructions -- --nocapture
cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions -- --nocapture
cargo test --manifest-path server/Cargo.toml response_artifact -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_code_request_uses_deep_code_artifact -- --nocapture
cargo fmt --check
cargo check -p cue-daemon
cargo check --manifest-path server/Cargo.toml --bin bluey-server
```

## Deployment Status

Pending final package and production deploy.
