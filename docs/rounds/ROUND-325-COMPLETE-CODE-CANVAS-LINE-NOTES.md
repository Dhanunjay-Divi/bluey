# Round 325 - Complete Code Canvas Line Notes

Date: 2026-07-03
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner showed a live histogram-largest-rectangle answer where Bluey explained the monotonic stack, but the canvas only contained the inner loop fragment instead of a full runnable solution. The owner also expected explanatory notes above/alongside lines so a beginner can understand what each important line does.

Session reference from the screenshot: `724C3B7E`.

## Root Cause

- The older daemon build that produced the screenshot persisted a code artifact from code-shaped text, even when the text was only a loose inner-loop fragment.
- The managed server prompt asked for complete code, but it did not explicitly forbid algorithm answers that only include the interesting middle of the implementation.
- The daemon-side prompt and mode instructions did not consistently require algorithm/interview answers to include full signature, initialization, loop/body, return path, sentinel/cleanup, and `Line notes:`.

## Fix

- Hardened the managed server coding prompt:
  - start code answers with one short approach sentence
  - include full class/function signature, initialization, loop/body, return value, and sentinel/cleanup
  - never provide only the inner loop or pseudocode fragment
  - add `Line notes:` outside code fences for non-trivial code
- Hardened daemon direct-provider prompts and Code/General mode instructions with the same complete-code and line-note contract.
- Changed server response artifact extraction so code canvas artifacts require fenced code blocks.
- Added a regression test for the exact failure shape: a loose histogram inner loop no longer becomes a code canvas artifact.

Line notes intentionally stay outside the copied code fence. This lets the UI render the explanation as visual/grey annotations without polluting the code users copy.

Windows parity: this is shared server and daemon/provider logic, so macOS and Windows get the same answer-shaping behavior. Existing Windows UI may still render line notes as text until its canvas renderer reaches Mac parity.

## Verification

Passed locally:

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

## Deployment

Pending:

- Desktop release `0.1.64`
- Production API server rebuild/restart
- Live `latest.json` verification
- Public API health verification

## Current State

The code changes are ready for packaging/deploy. After deployment, retest a fresh algorithm prompt such as:

```text
Given an array of integers heights representing histogram bars where width is 1, return the largest rectangle area.
```

Expected:

- chat starts with a short approach sentence
- canvas contains a full runnable implementation
- canvas does not show only the inner loop
- answer includes `Line notes:` outside the code block for important lines

## Remaining QA / Gates

- Live overlay smoke with the histogram prompt after `0.1.64` is installed.
- Confirm copied code omits line-note prose.
