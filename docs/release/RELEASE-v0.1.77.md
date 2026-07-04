# Bluey 0.1.77

Canvas click-through fix.

## Changes

- Fixes macOS canvas mode click-through so the canvas body and blank canvas area click through to the app behind Bluey.
- Keeps canvas controls usable in click-through mode:
  - header buttons
  - canvas scroller
  - canvas divider
  - blue move handle
- Prevents selectable canvas text from accidentally making the whole canvas mouse-active.
- Updates shortcuts copy to explain the canvas click-through rule.

## Verification

```bash
cd native/macos/cue-overlay && swift build -c release
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.77
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Deployment

- Desktop release `0.1.77` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.77/bluey-0.1.77-darwin-arm64.tar.gz`
- Artifact SHA256:
  `40013718b5a79394f166186274d1b12782aa52bc93a49465182a6a5ed5c4bca2`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.77`
- Public installer smoke installed `0.1.77` locally and both installed binaries report `0.1.77`.
- The non-interactive shell could not provide sudo credentials, so install correctly fell back to the user-local command symlink at `/Users/uno/.local/bin/bluey`.
