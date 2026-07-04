# Round 343: Screen Context Sent Chips Code Comments

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed a screen-context answer where:

- The sent question card said `Answer using the attached screen context.` but did not show the attached `Screen context` chip inside the question bubble.
- The same screen context chip still stayed in the bottom attachment strip after the question was sent.
- Coding answers still did not reliably show useful explanatory comments in the code, even though the canvas was open.

## Diagnosis

- The daemon only used `visible_context_ids` from the overlay request to decide which chips belong on the sent question card.
- Some screen/doc context is inferred server-side or daemon-side from the current session, so the answer could use the screen context while the visible question card had no explicit attachment id to render.
- The macOS overlay only hid the pending attachment strip after send when `pendingContextItemIds` or `showingSavedContextItems` were set. If the strip visually had a stale inferred item but the pending id list was empty, it stayed visible.
- The code-answer prompt said to add inline comments, but that instruction was too weak for providers. The canvas can style comments and line notes, but it can only render them when the provider actually emits them.

## Changes

- The daemon now derives question attachment ids from the actual `AnswerContext` when explicit overlay ids are missing.
- Sent question cards now fall back to document/screenshot `AnswerContext` so `Screen context` and document chips still appear in the sent bubble when context was inferred.
- Persisted conversation turns now store the inferred attachment ids too, so history/copy/replay paths can refer to the real context used.
- The macOS overlay now always clears the bottom attachment strip after a send. If files still exist in the session, the header remains as `Show N file(s)` so the user can open them intentionally.
- Desktop and server prompt contracts now require non-trivial code to include comments above major blocks and on important decision lines.
- Coding prompts still require a separate `Line notes:` block outside the fence so Bluey can show explanation without polluting copied code.
- Desktop workspace version bumped to `0.1.82`.

## Product Rule

For screen/doc questions:

1. The question card should show the context chips that were actually used.
2. After sending, the bottom pending strip should hide.
3. Existing files remain accessible through the header `Show file(s)` control.

For code questions:

1. Chat should explain the approach.
2. Canvas should show complete code, not fragments.
3. Non-trivial code should include comments above important blocks/decisions.
4. Extra explanation should live in `Line notes:` so copied code stays clean.

## Verification

Passed locally:

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo test -p cue-daemon inferred_answer_context_produces_question_attachment_chips -- --nocapture
cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes -- --nocapture
cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_prompt_explains_unavailable_web_search --lib -- --nocapture
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.82
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
rsync -az --delete --exclude='target' --exclude='.git' Cargo.toml Cargo.lock crates server infra root@165.227.77.152:/opt/bluey-build-codex-round343-screen-context-comments/
ssh root@165.227.77.152 'cd /opt/bluey-build-codex-round343-screen-context-comments/server && BLUEY_GIT_COMMIT=2a0da7dbb4bc0e02cb29c1e7933195adb00a04f8 /root/.cargo/bin/cargo build --release --bin bluey-server'
ssh root@165.227.77.152 'journalctl -u bluey-api.service --since "5 min ago" -p warning --no-pager'
curl -fsS https://bluey.sh/health
```

## Deployment

- Desktop release `0.1.82` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.82/bluey-0.1.82-darwin-arm64.tar.gz`
- Artifact SHA256:
  `0dce9487c5376da7fb4839ccad1043c029559ac66aa4c1257dd5bc9dce124d0f`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.82`
- Public installer smoke installed `0.1.82` locally and both installed binaries report `0.1.82`.
- The non-interactive Codex shell had no `/dev/tty` for sudo, so installer fallback used `/Users/uno/.local/bin/bluey`; that fallback is expected in this shell.
- `bluey on` started a fresh `0.1.82` daemon pid `52242`.

Production API deployment details:

- Production API server deployed from `/opt/bluey-build-codex-round343-screen-context-comments`.
- Commit baked into health: `2a0da7dbb4bc0e02cb29c1e7933195adb00a04f8`.
- Production binary SHA256:
  `c7c62ed2603e0ea71b2ab42b31a0c78b30876c7aae8c9b0cb4eee72365693672`
- Previous API binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260704T203443Z`
- `bluey-api.service` status after restart: `active`.
- `bluey-api.service` `NRestarts`: `0`.
- `https://bluey.sh/health` reports commit `2a0da7dbb4bc0e02cb29c1e7933195adb00a04f8`.
- Recent production warning/error scan after restart returned no entries.

## Windows Parity

The sent question chip derivation, prompt contract, and server AnswerPlan changes are shared Rust/server code, so Windows receives the same behavior when packaged from this branch. The macOS-only strip-clearing patch is in the AppKit overlay; the Windows overlay should use the same product rule when its attachment strip is touched next.
