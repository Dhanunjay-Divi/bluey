# Bluey 0.1.87

Released: 2026-07-05

## Summary

Production-beta merge release for the `codex/bluey-web-ui-parallel-20260704` branch.

## Changes

- Merged the parallel web UI, account, billing, device-link, diagnostic log, trial, and overlay hardening work into `main`.
- Fixed streaming router behavior so in-band provider stream errors are sent as stream error frames instead of becoming abrupt pre-stream failures.
- Treats OpenAI-compatible and Anthropic streams that drop before their terminal event as incomplete, avoiding estimated billed success for partial streams.
- Keeps router integration tests deterministic by isolating test route policy and AnswerPlan lane overrides from operator deployment settings.

## Verification

```bash
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml --lib --quiet
cargo test --manifest-path server/Cargo.toml --test integration_e2e --quiet
cargo test -p cue-daemon --lib --quiet
cargo test -p cue-cloud-client --lib --quiet
git diff --check
```

Round doc:

- `docs/rounds/ROUND-387-MAIN-MERGE-RELEASE-PREP.md`
