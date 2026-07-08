# ROUND-415-CODING-CANVAS-COMPLEXITY-PARITY

Date: 2026-07-07
Repo: /Users/uno/Downloads/cue-answerplan-fix
Branch: main
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Problem

Bluey could explain both time and space complexity in the left answer, but the right-side code canvas sometimes kept only the first complexity line, usually `Time Complexity`, and dropped `Space Complexity` plus the explanatory sentences. This made the canvas look incomplete even when the answer text was correct.

The visible chat body also still had legacy behavior that could echo code on the left while the code panel owned the implementation, which made it feel like code first appeared in one place and then moved.

## Changes

- Updated server code-artifact formatting to extract a full `Complexity` block, not only individual time/space lines.
- Updated desktop overlay artifact parsing with the same full-block complexity behavior.
- Added a desktop merge guard: if a managed server artifact is missing complexity lines that appear in the final answer, Bluey merges the missing lines into the code artifact before rendering or saving.
- Changed code-canvas chat rendering so the left answer keeps approach/explanation/complexity, while the full code stays in the code panel.
- Updated history restore expectations so old saved code artifacts keep their button/artifact without duplicating code into the chat text.
- Removed now-unused local code-preview language helpers.

## Verification

- `cargo test --manifest-path crates/cue-daemon/Cargo.toml answer_overlay_artifact --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml visible_answer_body --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml code_artifact --quiet`
- `cargo test --manifest-path server/Cargo.toml response_artifact --quiet`
- `cargo check -p cue-daemon -p cue-cli`
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `git diff --check`

All focused checks passed.

## Deploy

- Committed core fix: `fe49e598 Fix coding canvas complexity parity`
- Prepared release: `2e533ce1 Prepare Bluey 0.1.91 release`
- Tagged and pushed: `v0.1.91`
- Deployed `bluey-server` to production with commit `2e533ce1346b70b6137587da95738fe4e7f3d984`.
- Production API health returned `status=ok`, commit `2e533ce1346b70b6137587da95738fe4e7f3d984`, and no warning/error journal entries after restart.
- Published live downloads for:
  - `darwin-arm64`: `bluey-0.1.91-darwin-arm64.tar.gz`
  - `windows-x86_64`: `bluey-0.1.91-windows-x86_64.zip`
- Verified live `latest.json` signature, installer MIME types, macOS artifact SHA/version, and Windows artifact SHA.

## Notes

This fixes the final rendered/saved state. During raw token streaming, a provider can still briefly emit code text before the final artifact arrives; the finalization path now cleans that up. A deeper future improvement would be an early streaming artifact placeholder so code tokens stream directly into the right panel from the first detected code fence.

The Intel macOS GitHub Actions runner for `darwin-x86_64` remained queued during this publish; the user-visible current Mac Apple Silicon and Windows artifacts were released so users do not keep pulling the older build.
