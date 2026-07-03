# ROUND-316 Autosend Fast Caption Settle

Date: 2026-07-03
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Make auto-send fire quickly after the user stops speaking, instead of waiting long enough that pressing Stop appears to trigger the send.

## Diagnosis

- Deepgram endpointing is already configured at roughly `300ms`, so the speech service can finalize short pauses quickly.
- The native overlays added another fixed `900ms` settle delay after a final caption before sending the answer.
- That extra overlay delay made auto-send feel slow, and made it easier for the user to hit Stop while an auto-send was still pending.
- The behavior existed on both macOS and Windows.

## Changes

- macOS overlay:
  - Replaced the hard-coded `0.9s` auto-send settle delay with named constants:
    - `autoSendCaptionSettleDelayMs = 300`
    - `autoSendCaptionSettleDelaySeconds = 0.3`
  - Updated autosend lifecycle logs to report the constant value.
  - Updated tooltips from "captions settle" to "captions pause briefly".
- Windows overlay:
  - Added `AUTOSEND_CAPTION_SETTLE_DELAY_MS = 300`.
  - Replaced `SetTimer(..., 900, ...)` with the shared constant.
  - Updated autosend lifecycle logs and tooltip copy.

## Expected Behavior

- Auto-send should trigger about `300ms` after Bluey receives the final caption for the last spoken chunk.
- Stop still cancels pending auto-send if the user intentionally stops before Bluey sends.
- Short speech such as "explain LRU cache" should not sit for almost another second after the transcript is already available.

## Verification

```bash
rg -n "delay_ms=900|ID_AUTOSEND_TIMER, 900|\\+ 0\\.9|captions settle" \
  native/macos/cue-overlay/Sources/cue-overlay/main.swift \
  native/windows/cue-overlay/main.c

rg -n "autoSendCaptionSettleDelay|AUTOSEND_CAPTION_SETTLE_DELAY|autosend_answer_scheduled|pause briefly" \
  native/macos/cue-overlay/Sources/cue-overlay/main.swift \
  native/windows/cue-overlay/main.c

swift build -c debug --package-path native/macos/cue-overlay
git diff --check
```

Result:

- No stale `900ms` autosend settle delay remains in the macOS or Windows overlay source.
- macOS overlay build passed.
- `git diff --check` passed.

Release verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" \
  make package-darwin-arm64

BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem \
  PUBLISH_DO=1 \
  PUBLISH_HOST=root@165.227.77.152 \
  PUBLISH_PATH=/var/www/bluey \
  scripts/deploy-bluey-sh-manual.sh
```

Result:

- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- `install.sh` content-type verified as `application/x-shellscript`.
- `install.ps1` content-type verified as `application/x-powershell`.
- Darwin arm64 artifact SHA verified.
- Unpacked Darwin arm64 `bluey` and `bluey-daemon` binaries report `0.1.55`.

## Deployment

- Public release version: `0.1.55`
- Live manifest: `https://bluey.sh/latest.json`
- Download URL: `https://bluey.sh/releases/v0.1.55/bluey-0.1.55-darwin-arm64.tar.gz`
- Local artifact: `dist/bluey-0.1.55-darwin-arm64.tar.gz`
- Artifact SHA256:
  `97086d226f9a9fe83f9597c2e387b6880cbfb9a054b09277ec29046806166aa6`
- Live manifest reports the same SHA and size `9165571` bytes.

## Notes

- This round intentionally keeps provider STT endpointing unchanged. The immediate delay found in the product path was the overlay's post-caption timer.
- If autosend still feels slow after this, the next audit should look at STT final-caption latency and websocket delivery time, using the `autosend_answer_scheduled` delay log plus transcript lifecycle logs.
