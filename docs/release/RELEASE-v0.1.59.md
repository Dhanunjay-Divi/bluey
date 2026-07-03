# Bluey 0.1.59

## Focus

Live answer QA hardening.

## Changes

- `bluey ask` now defaults to the managed balanced route without silently falling back to local context answers.
- Artifact-backed managed code answers now recover when the streamed prose has a dangling Markdown code fence.
- Code previews keep `LINE NOTES` outside runnable code blocks, so copied code stays valid.
- Added regressions for CLI managed routing, code-fence recovery, and code-preview line-note separation.

## Verification

- `cargo fmt`
- `cargo test -p cue-cli cli_ask_defaults_to_managed_balanced_without_local_fallback -- --nocapture`
- `cargo test -p cue-daemon code_artifact -- --nocapture`
- `cargo check -p cue-daemon --quiet`
- `cargo build -p cue-cli -p cue-daemon`
- Fresh live QA answer matrix under `/tmp/bluey-live-qa-round320-final-20260703T102110Z`
- `BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh`

## Deployment

- Desktop release `0.1.59` published to `bluey.sh`.
- Darwin arm64 artifact SHA256:
  `dfcc71ffc35d5d5f981d941dee90ef9112c1ce5b1d0e9789924559d68feec3ac`
- Live release signature verified.
- Live installer MIME checks passed.
- Live macOS artifact SHA verified.
- Unpacked macOS release reports `bluey 0.1.59` and `bluey-daemon 0.1.59`.
