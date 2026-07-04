# Round 335 - Code Canvas Stale Artifact Guard

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Trigger

The owner reported that a latest coding follow-up did not produce code, while the right-side code canvas still showed code from a previous question. This made Bluey look like it had answered the current question with the old artifact.

## Root Cause

- The overlay canvas is a persistent workbench. If a new answer did not include an artifact, the old active canvas stayed open.
- This is correct for explanation-only follow-ups, but wrong for failed answers, fresh code requests, screen-code requests, or algorithm answers that produced prose without a current code artifact.
- The server accepted `AnswerOutput::CodeArtifact` responses even when the provider returned only prose and no fenced code artifact.
- Screen/canvas-detail answers that did contain fenced code were not preferred as code artifacts in the server extractor.
- Code/canvas answers were still using the small general-answer output budget in some paths.

## Changes

- Added a macOS overlay guard that hides a stale active canvas when a newer answer has no artifact and looks like a fresh code/screen/code-failure answer.
- Kept explanation-only follow-ups preserving the existing canvas, so asking "explain this code" does not unnecessarily close the workbench.
- Added metadata-only lifecycle logging: `canvas_hide_stale_without_artifact`.
- Added server-side answer-plan output budgets:
  - code artifact minimum output budget: `4096`
  - canvas detail minimum output budget: `3072`
- Added server-side `code_artifact_missing` validation before billing. If Bluey planned a code artifact but the response has no code artifact, the request becomes a typed retryable failure instead of a normal completed prose answer.
- Added ops-audit logging for `answer_failed/code_artifact_missing` with request/session refs, provider/model, answer intent/output, text length, and hashes only.
- Updated canvas-detail artifact extraction so a screen/canvas answer containing fenced code is surfaced as a `code` artifact.
- Added regression tests for:
  - screen/canvas-detail code being preserved as code artifact
  - code-artifact plans rejecting prose-only answers

## Verification

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml response_artifact_for_output --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml code_artifact_plan_rejects_prose_only_answer --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_uses_screen_context_code_signals --lib -- --nocapture
cargo check --manifest-path server/Cargo.toml
cargo check -p cue-daemon -p cue-cli
cd native/macos/cue-overlay && swift build -c release
```

## Deployment

- Desktop workspace version bumped to `0.1.75`.
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

## Notes

- This does not delete previous code artifacts. It only stops stale artifacts from appearing as if they belong to the latest answer.
- If a provider fails to generate required code, the user should now see a retryable failure and support can search for `code_artifact_missing`.
