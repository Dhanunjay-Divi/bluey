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

- Desktop release `0.1.75` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.75/bluey-0.1.75-darwin-arm64.tar.gz`
- Artifact SHA256:
  `1ca94f358ebe5a15dd1a7d874cce0c610a04e52744d12d11468b4ed220f83ca6`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.75`
- Public installer smoke installed `0.1.75` locally and both installed binaries report `0.1.75`.
- Production API server deployed from `/opt/bluey-build-codex-round335-stale-canvas`.
- Production binary SHA256:
  `321e12a09e0d2875a7cfd9c832fedcd95ffcd570d435cbd664bbaf61726e5658`
- Previous API binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260704T121958Z`
- `https://bluey.sh/health` reports commit `e43fcec9ccc314ca217eef36df692b154a9e103e`.
- `bluey-api.service` is active with `NRestarts=0`.
- Recent production warning/error scan after restart returned no lines.
