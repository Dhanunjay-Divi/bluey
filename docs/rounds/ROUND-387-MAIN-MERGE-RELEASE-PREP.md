# ROUND-387 Main Merge Release Prep

Date: 2026-07-05
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Prepare the accumulated Bluey branch work for merge into `main` and deployable production-beta artifacts without losing local work.

## What Changed In This Release Prep

- Kept the branch as the source of truth and verified it can fast-forward `main`.
- Re-ran the server, daemon, and cloud-client validation suites before merge.
- Fixed the router integration failures that showed up during release validation:
  - Wiremock integration harness now pins `BLUEY_ROUTE_POLICY=quality_first` so operator routing policy does not make tests nondeterministic.
  - Wiremock integration harness disables AnswerPlan lane override so legacy router tests continue to test the requested lane contract.
  - Streaming router now replays the first in-band provider stream event, including error events, instead of converting those into pre-stream HTTP failures.
  - OpenAI-compatible and Anthropic streaming paths now treat missing terminal stream events as incomplete instead of billing an estimated success after a dropped stream.
  - Updated router integration expectations for the current OpenAI fallback model and provider-capacity stream error reason.

## Validation

Passed:

- `cargo fmt --all --check`
- `cargo test --manifest-path server/Cargo.toml --lib --quiet`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e --quiet`
- `cargo test -p cue-daemon --lib --quiet`
- `cargo test -p cue-cloud-client --lib --quiet`
- `git diff --check`
- Secret scan for previously pasted GLM/DeepSeek key material outside build artifacts and local databases.

## Notes

- The branch remains ahead of `main`; `main` can be updated by fast-forward after this commit.
- The stream completion change is intentionally conservative: if the provider drops before a terminal event, Bluey should surface an incomplete-stream error and avoid treating the partial answer as a billed success.
