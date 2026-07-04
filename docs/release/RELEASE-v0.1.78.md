# Bluey 0.1.78

Complete code artifact guard.

## Changes

- Rejects code canvas artifacts that are only an inner control-flow fragment, such as a bare `for` loop without a class/function/signature.
- Keeps complete algorithm artifacts, including full `class Solution` style answers.
- Keeps patch/diff artifacts valid for follow-up edits.
- Strengthens code prompts so language-change follow-ups such as "same code in Python" regenerate the complete implementation with the wrapper/signature.
- Applies the guard on both the managed server path and the shared daemon fallback detector.

## Verification

```bash
cargo fmt --check
cargo test --manifest-path server/Cargo.toml response_artifact_ --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_python --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_java --lib -- --nocapture
cargo test -p cue-daemon answer_overlay_artifact_ -- --nocapture
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.78
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Release

- Desktop release `0.1.78` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.78/bluey-0.1.78-darwin-arm64.tar.gz`
- Artifact SHA256:
  `3418bf8ae4f66fd3864fd6de2be08a461e985a98fab39ca85fda6440edf21aad`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.78`
- Public installer smoke installed `0.1.78` locally and both installed binaries report `0.1.78`.
- The non-interactive shell could not provide sudo credentials, so install correctly fell back to the user-local command symlink at `/Users/uno/.local/bin/bluey`.
