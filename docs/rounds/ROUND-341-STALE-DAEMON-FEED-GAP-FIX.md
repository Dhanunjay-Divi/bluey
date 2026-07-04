# Round 341: Stale Daemon Feed Gap Fix

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner shared `IMG_3713.HEIC` showing two failures:

- A previously fixed coding-answer issue appeared again as smashed inline code, such as `Codecppclass...`.
- The conversation feed had a very large empty vertical gap before the next question.
- A follow-up question, `can u generate pdf for this?`, failed with `Ref: DDAC3482`.

## Diagnosis

The installed binaries on disk were `0.1.79`, but the active daemon process was still an older runtime:

```bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey status
ps -p 43866 -o pid,lstart,command
```

Observed:

- Installed CLI: `bluey 0.1.79`
- Installed daemon binary: `bluey-daemon 0.1.79`
- Running daemon pid: `43866`
- Running daemon start time: `Sat Jul 4 04:47:03 2026`
- Installed daemon binary mtime: `Jul 4 13:50:09 2026`

Local logs confirmed the active runtime handling the failing request was old:

```text
request_ref=DDAC3482
session_code=817CCEF3
version=0.1.72
route_primary=bluey_managed/vision
error=provider error: server error: 400
```

So the root problem was not that the `0.1.79` fix failed. The old daemon process stayed alive after the update and continued rendering/serving the old behavior.

The large blank space came from the macOS feed stack being forced to fill the entire scroll viewport. With a short/malformed legacy answer row, AppKit could stretch the row and create a large empty gap before later cards.

## Changes

- `bluey on` and daemon start paths now detect stale running daemons.
- If the installed `bluey-daemon` binary is newer than the running daemon's `started_at`, the CLI gracefully shuts down the old daemon and starts the installed one.
- Added a CLI unit test for the daemon-binary mtime restart decision.
- Tightened macOS feed spacing and removed the scroll-stack viewport-fill constraint that created large blank feed gaps.
- Added defensive overlay display cleanup for legacy compressed inline code blobs like `Codecppclass...`, replacing them in chat with a short pointer while the code remains in canvas/artifact flows.
- Desktop workspace version bumped to `0.1.80`.

## Verification

```bash
cargo fmt --check
cargo test -p cue-cli daemon_binary_change_check_detects_newer_installed_binary -- --nocapture
cargo check -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.80
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

## Deployment

- Desktop release `0.1.80` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.80/bluey-0.1.80-darwin-arm64.tar.gz`
- Artifact SHA256:
  `ba945ff51c285a2cc5e44b58fc27684c4f83678e49658fa7a03fe8fea92f1ce4`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.80`
- Public installer smoke installed `0.1.80` locally and both installed binaries report `0.1.80`.
- `bluey on` started fresh daemon pid `5303` at `Sat Jul 4 14:48:24 2026`.
- Local daemon log tail now contains `version=0.1.80` entries after the restart.

## Notes

- Existing saved cards are historical data; the new overlay defensively sanitizes legacy compressed code on render, but it does not rewrite local session JSON.
- The stale-daemon guard is intentionally based on installed binary mtime versus daemon `started_at`, so it covers future desktop releases even when the daemon protocol version is unchanged.
