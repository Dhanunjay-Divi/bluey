# Bluey v0.1.66

Date: 2026-07-04
Branch: codex/bluey-overlay-spacing-20260626

## Summary

This release tightens coding answers so first-pass algorithm/code responses include a clear approach, readable complete code, explanation, complexity, and edge cases. Follow-up code changes prefer changed blocks or patches instead of unnecessary full rewrites.

## Changes

- Managed server coding prompts now require:
  - `Approach`
  - `Code`
  - `Explanation`
  - explicit Time Complexity and Space Complexity
  - edge cases when useful
- Code blocks must keep each statement on its own line with correct indentation.
- Python/LeetCode-style answers must include imports when type hints need them, or avoid those type hints.
- Non-trivial code should include concise inline comments for important decision lines.
- Code follow-ups preserve the existing artifact by default and use patches/changed blocks for edits.
- Desktop daemon fallback prompts and Code/General modes now follow the same rules.

## Verification

```bash
cargo test --manifest-path server/Cargo.toml answer_plan_code_request_uses_deep_code_artifact -- --nocapture
cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes -- --nocapture
cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions -- --nocapture
cargo test -p cue-daemon response_artifact_separates_code_line_notes -- --nocapture
cargo check -p cue-daemon
cargo fmt --check
cargo check --manifest-path server/Cargo.toml --bin bluey-server
```

## Deployment Status

Pending final package and deploy.

