# Bluey 0.1.76

Overlay content scrolling polish.

## Changes

- Adds macOS `Option+Up` / `Option+Down` content scrolling for the visible Bluey area under the mouse pointer.
- Supports answer feed, history drawer, and right-side canvas keyboard scrolling.
- Keeps normal text-editing behavior while the Ask box or another text editor is active.
- Documents that mouse wheel scrolling follows the pointer over answer, canvas, or history content.

## Verification

```bash
cd native/macos/cue-overlay && swift build -c release
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.76
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Deployment

- Desktop release `0.1.76` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.76/bluey-0.1.76-darwin-arm64.tar.gz`
- Artifact SHA256:
  `f95d32d983e41a4a3817e63217ae263d2a42d3c384b29d6db613b3cbe79dbf66`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.76`
- Public installer smoke installed `0.1.76` locally and both installed binaries report `0.1.76`.
- The non-interactive shell could not provide sudo credentials, so install correctly fell back to the user-local command symlink at `/Users/uno/.local/bin/bluey`.
