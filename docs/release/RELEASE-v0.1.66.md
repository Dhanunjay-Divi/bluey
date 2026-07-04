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

Live.

- Production API server deployed from commit:
  `04ca504024dfcfa0cf3dd5ccc628807d2bc83a85`
- API build tree:
  `/opt/bluey-build-codex-round328-code-shape`
- Installed API binary SHA:
  `53e8ee7ced0371338b5f4298bc8aea55f7a67e76597c271f063d5a446d1ff967`
- API health:
  local and public `/health` returned `status=ok` with the expected commit
- Desktop release:
  `0.1.66` published to `bluey.sh`
- macOS artifact SHA:
  `36443359c5979d06055f0bec890b11d0add7afd7c2cd0db18041f06732c5ada3`
- Release verification:
  `latest.json` signature verified, installer MIME checks passed, live artifact SHA verified, unpacked binaries report `0.1.66`
- Local install:
  installed and restarted successfully on this Mac
