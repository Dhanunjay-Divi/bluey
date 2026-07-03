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

## Notes

- This round intentionally keeps provider STT endpointing unchanged. The immediate delay found in the product path was the overlay's post-caption timer.
- If autosend still feels slow after this, the next audit should look at STT final-caption latency and websocket delivery time, using the `autosend_answer_scheduled` delay log plus transcript lifecycle logs.
