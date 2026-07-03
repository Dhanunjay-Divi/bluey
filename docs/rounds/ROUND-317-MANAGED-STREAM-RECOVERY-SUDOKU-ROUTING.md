# ROUND-317 Managed Stream Recovery and Sudoku Routing

Date: 2026-07-03
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner reported that Bluey connection dropped before a Python Sudoku answer finished.

User-visible ref:

- `CAED38EE`

## Production Trace

Prod logs for `CAED38EE` showed:

- Full request id: `caed38ee-7895-4428-9ee2-f0423df25001`
- Session ref: `550DD019`
- Answer plan: `coding`
- Output: `code_artifact`
- Requested/effective lane: `balanced`
- Provider/model: `zai` / `glm-5.2`
- First token latency: `1484ms`
- Total latency: `25370ms`
- Output tokens: `712`
- Customer charge: `2c`
- Bluey upstream estimate: `1c`
- Server marked the answer completed and billed.
- `request_idempotency.response_json` contained a completed cached response of about `4475` bytes.

Conclusion: the server completed and cached the final answer, but the desktop stream path could still leave the overlay with a partial/broken answer if the managed SSE connection dropped before the final billing/artifact event reached the daemon.

## Changes

### Daemon Stream Recovery

- Added `recover_managed_stream_from_cached_answer`.
- If managed streaming errors after partial text, the daemon now calls the non-stream managed completion endpoint with the same request id.
- Because the same request id is used, the server returns the idempotency-cached response when the original stream already completed, instead of generating/billing a second answer.
- If recovery returns a complete answer, the overlay card is finished with the recovered final body, cost label, artifact, and sources.
- Added diagnostic logs:
  - `managed provider stream cached-answer recovery unavailable`
  - `managed provider stream cached-answer recovery returned empty answer`
  - `managed provider stream cached-answer recovery returned incomplete answer`
  - `managed provider stream cached-answer recovery was shorter than partial stream`
  - `managed provider stream recovered from cached final answer`
- If recovery is unavailable and the partial answer looks complete, the previous preservation path remains.

### Algorithmic Code Routing

- Tightened `looks_like_simple_coding_question`.
- Short code prompts now route to `deep` when they include algorithmic/solver signals:
  - `algorithm`
  - `leetcode`
  - `solver`
  - `sudoku`
  - `backtracking`
  - `binary search`
  - `dfs`
  - `bfs`
  - `tree`
  - `heap`
  - `stack`
  - `queue`
  - `memoization`
- Tiny/simple snippets such as `Write a tiny Python Fibonacci function` still route to `balanced`.

## Verification

```bash
cargo fmt --all
cargo check -p cue-daemon
cargo test -p cue-daemon stream -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture
cargo build --manifest-path server/Cargo.toml
git diff --check
```

Result:

- `cargo check -p cue-daemon` passed.
- Daemon stream-related tests passed.
- Server answer-plan tests passed, including the new Sudoku regression.
- Server build passed.
- `git diff --check` passed.

New regression:

- `answer_plan_algorithmic_solver_code_uses_deep_code_artifact`

## Expected Behavior

- If the server completes a managed streamed answer but the daemon loses the final stream event, the overlay should recover the cached final answer and finish the card instead of leaving a broken response.
- `Give me Python code which solves Sudoku` should use the deep code lane and return a code artifact.
- The shareable ref id remains enough to trace request, session, provider, model, route, billing, and recovery logs without exposing user content in logs.
