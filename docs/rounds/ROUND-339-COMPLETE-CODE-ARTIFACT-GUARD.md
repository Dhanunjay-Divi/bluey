# Round 339 - Complete Code Artifact Guard

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Trigger

The owner showed a coding answer where Bluey's right-side code canvas contained only an inner loop:

```cpp
for (char move : moves) {
    if (move == 'U') {
        y++;
    }
}
```

The chat answer said `Code`, but the canvas did not include the full class, function signature, initialization, return path, or imports. A follow-up asking for the same code in Python also risked keeping the prior canvas instead of regenerating the full implementation.

## Root Cause

- The prompt already asked for complete algorithm code, but the provider can still return a fenced middle fragment.
- Server artifact extraction treated any fenced code block that looked code-like as a valid `code` artifact.
- The desktop fallback artifact detector had the same weakness.
- For follow-up language changes such as "same code in Python" or "same code in Java", the prompt did not state strongly enough that Bluey must regenerate a complete solution with the wrapper/signature.

## Fix

- Added server-side complete-code validation before accepting a code artifact.
- Added desktop daemon-side complete-code validation before inferring a local code canvas.
- Rejects top-level control-flow fragments that start with `for`, `if`, `while`, `switch`, `case`, or `else` when they do not include an entrypoint/wrapper signal such as:
  - `class`
  - `def`
  - `function`
  - `fn`
  - `public`
  - `bool`
  - `void`
  - `const` / `let` / `var`
- Keeps valid complete code artifacts, including the robot-return-to-origin `class Solution` example.
- Keeps patch/diff artifacts valid so follow-up code edits still work.
- Strengthened code and coding-follow-up instructions:
  - first-time algorithm answers must include full wrapper/signature, loop/body, return path, complexity, and edge cases
  - "same code in another language" must regenerate the complete implementation in that language
  - never emit only the inner loop as the code artifact
- Bumped desktop workspace version to `0.1.78`.

## Product Rule

Bluey should never show a partial middle fragment in the code canvas as if it is the full answer.

If the answer plan expects code, the acceptable outcomes are:

- a complete code artifact with the wrapper/signature and return path
- a patch/diff artifact for a true follow-up edit
- a retryable typed failure before billing, not a completed answer with fake/incomplete code

## Verification

Passed locally:

```bash
cargo fmt --check
cargo test --manifest-path server/Cargo.toml response_artifact_ --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_code_request_uses_deep_code_artifact --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_python --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_java --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_leetcode_statement_uses_deep_code_artifact --lib -- --nocapture
cargo test -p cue-daemon answer_overlay_artifact_ -- --nocapture
cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes -- --nocapture
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.78
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
rsync -az --delete --exclude='target' Cargo.toml Cargo.lock crates server infra root@165.227.77.152:/opt/bluey-build-codex-round339-code-artifact/
ssh root@165.227.77.152 'cd /opt/bluey-build-codex-round339-code-artifact/server && BLUEY_GIT_COMMIT=210a4f7559ffd13fe263b3891d75b5e93c39c9f3 /root/.cargo/bin/cargo build --release --bin bluey-server'
ssh root@165.227.77.152 'install -m 0755 /opt/bluey-build-codex-round339-code-artifact/server/target/release/bluey-server /usr/local/bin/bluey-server && systemctl restart bluey-api.service'
curl -fsS https://bluey.sh/health
ssh root@165.227.77.152 'journalctl -u bluey-api.service --since "5 min ago" -p warning --no-pager'
```

## Release

Desktop release `0.1.78` is live on `bluey.sh`.

Darwin arm64 artifact:

`https://bluey.sh/releases/v0.1.78/bluey-0.1.78-darwin-arm64.tar.gz`

Artifact SHA256:

`3418bf8ae4f66fd3864fd6de2be08a461e985a98fab39ca85fda6440edf21aad`

Release verification passed:

- `latest.json` signature verification
- installer MIME checks
- Darwin arm64 artifact SHA verification
- unpacked `bluey` and `bluey-daemon` version checks for `0.1.78`
- public installer smoke installed `0.1.78` locally

## Deployment

Desktop release is live.

Production API server was built from the committed source at:

`/opt/bluey-build-codex-round339-code-artifact`

Production API deployment details:

- Commit baked into health: `210a4f7559ffd13fe263b3891d75b5e93c39c9f3`
- Production binary SHA256:
  `06a607833a7ea7f59b1bb0e283d334432fe8c7048a623c23dd8be45d4bbabb58`
- Previous API binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260704T144812Z`
- `bluey-api.service` status after restart: `active`
- `bluey-api.service` `NRestarts`: `0`
- `https://bluey.sh/health` reports commit `210a4f7559ffd13fe263b3891d75b5e93c39c9f3`
- Recent production warning/error scan after restart returned no entries.

## Windows Parity

The server-side artifact validation applies to both macOS and Windows clients because both talk to the same managed API.

The daemon-side fallback validation is shared Rust code, so Windows desktop builds will inherit the same "do not infer canvas from inner loop" rule when packaged from this branch.
