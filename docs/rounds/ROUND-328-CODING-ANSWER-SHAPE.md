# Round 328 - Coding Answer Shape

Date: 2026-07-04
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner showed a coding challenge answer where Bluey gave only a short approach sentence in chat and a cramped code artifact in the canvas. The expected first-pass coding answer is:

- approach explanation
- complete code with useful comments
- clear time and space complexity
- edge cases when useful
- follow-up changes as inline code deltas or patches, not unnecessary full rewrites

## Root Cause

- The managed server coding prompt asked for full code and line notes, but did not force the complete answer skeleton strongly enough.
- The desktop daemon fallback prompt also allowed generic `Approach/Patch/Explanation` wording for first-time implementation prompts, which is better for edits than for fresh coding challenge answers.
- The prompt did not explicitly forbid one-line compressed code output such as putting `class`, `def`, assignments, and `return` on the same wrapped line.
- It did not require explicit `Time Complexity` and `Space Complexity` headings for every algorithm/code answer.

## Fix

- Hardened managed server coding output:
  - first-time code answers use `Approach`, `Code`, `Explanation`, `Complexity`, and `Edge cases`
  - approach must be 2-4 clear bullets before code
  - code must be fenced with a language tag
  - Python/LeetCode answers must include imports when type hints need them, or avoid those type hints
  - every statement must be on its own line with correct indentation
  - non-trivial code gets concise inline comments for important decision lines
  - algorithm answers must explicitly include Time Complexity and Space Complexity
- Hardened coding follow-up behavior:
  - preserve the existing implementation by default
  - use a changed block, patch, or unified diff for edits
  - only provide full replacement when requested or materially safer
- Applied the same contract to daemon fallback prompts and Code/General mode instructions.
- Expanded overlay inline-heading splitting for `Code`, `Changed block`, `Time Complexity`, `Space Complexity`, and `Line notes`.
- Bumped desktop release version to `0.1.66`.

Mac/Windows parity: the shared daemon prompt and server prompt apply to both macOS and Windows clients. The macOS release artifact is being published first because that is the currently live downloadable desktop artifact.

## Verification

Passed locally:

```bash
cargo test --manifest-path server/Cargo.toml answer_plan_code_request_uses_deep_code_artifact -- --nocapture
cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes -- --nocapture
cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions -- --nocapture
cargo test -p cue-daemon response_artifact_separates_code_line_notes -- --nocapture
cargo check -p cue-daemon
cargo fmt --check
cargo check --manifest-path server/Cargo.toml --bin bluey-server
```

## Deployment

Pending:

- commit/push
- deploy production API server
- package and publish desktop release `0.1.66`
- install locally and restart Bluey

## Remaining QA / Gates

- Retest a coding challenge prompt from screen context.
- Expected first response:
  - `Approach` section
  - readable fenced code
  - useful inline comments
  - `Explanation`
  - `Time Complexity` and `Space Complexity`
  - follow-up edits as patch/changed block unless full rewrite is requested

