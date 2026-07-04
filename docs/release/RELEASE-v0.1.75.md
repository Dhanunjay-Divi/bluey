# Bluey 0.1.75

Code canvas reliability and stale-artifact guard.

## Changes

- Prevents the right-side code canvas from showing previous code as if it belongs to a newer failed or prose-only code answer.
- Adds server-side `code_artifact_missing` validation for code-artifact plans before billing.
- Gives code/canvas answers a larger server-side output budget.
- Treats fenced code inside screen/canvas-detail answers as a code artifact.
- Adds regression tests for code artifact extraction and prose-only code-artifact failures.

## Verification

```bash
cargo test --manifest-path server/Cargo.toml response_artifact_for_output --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml code_artifact_plan_rejects_prose_only_answer --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_uses_screen_context_code_signals --lib -- --nocapture
cargo check --manifest-path server/Cargo.toml
cargo check -p cue-daemon -p cue-cli
cd native/macos/cue-overlay && swift build -c release
```

## Deployment

- Pending.
