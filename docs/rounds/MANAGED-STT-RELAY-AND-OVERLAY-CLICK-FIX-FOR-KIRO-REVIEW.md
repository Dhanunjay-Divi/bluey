# Managed STT Relay + Overlay Click Fix

Date: 2026-06-13
Branch: `codex/bluey-ai-site`

## Summary

This round closes two user-facing gaps in the Listen flow:

1. **Managed live STT now uses a websocket relay by default.** A signed-in desktop session creates a Bluey STT session, opens a Bluey relay websocket, streams native helper PCM to the server, and receives Deepgram transcript events back into the overlay. The path is:

   `desktop native audio helper -> bluey-server /stt/session + /stt/relay -> Deepgram websocket -> daemon transcript events -> overlay`

2. **Expanded overlay buttons are clickable again.** The expanded panel no longer flips the whole `NSWindow.ignoresMouseEvents` flag from a timer. That timer could leave the window ignoring events at the exact moment the user clicked Listen, Answer, Screen, Tone, or close.

## Implementation Notes

- Default signed-in STT transport changed from chunked `/router/transcribe` to live relay.
- The old chunked managed path remains available for debugging with:
  - `BLUEY_STT_FORCE_CHUNKED=1`
  - `BLUEY_MANAGED_STT_CHUNKED=1`
- Live relay starts one continuous native helper per enabled audio source.
- Relay source tasks send PCM16 chunks over the websocket as binary frames.
- Deepgram partial and final transcript frames are parsed through the existing Deepgram parser.
- Partial captions now flow into the overlay as non-final transcript segments.
- Final captions still drive persistent transcript/RAG indexing.
- Idle auto-stop still uses the 5-minute no-transcript guard.

## Files Changed

- `crates/cue-daemon/src/app.rs`
  - Adds `BlueyManagedRelay` STT transport.
  - Adds relay websocket URL construction and relay source loops.
  - Emits Deepgram partial transcripts live.
  - Adds relay URL/duration unit tests.

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Keeps the expanded overlay window clickable.
  - Leaves click-through behavior for collapsed/hidden states instead of the active control surface.

## Verification

```bash
cargo fmt --all
cargo test -p cue-cloud-client --all-targets
cargo test -p cue-daemon --all-targets
cd server && cargo test stt
cargo build -p cue-daemon -p cue-cli
cargo clippy -p cue-daemon --all-targets -- -D warnings
swift build -c release --package-path native/macos/cue-overlay
bash native/macos/cue-overlay/build.sh
git diff --check
```

All commands passed locally.

## Reviewer Focus

- Verify the relay session URL construction matches the server `/stt/relay` contract.
- Verify partial transcripts appearing as non-final segments do not create unwanted persistent/RAG rows.
- Verify the old chunked managed STT fallback still works with `BLUEY_STT_FORCE_CHUNKED=1`.
- Real-device smoke still matters: click Listen in the visible overlay and confirm mic/system captions animate and arrive from real audio, not mock data.

## Known Tradeoff

The expanded overlay no longer offers whole-window click-through in non-control regions. That is intentional for this fix because the timer-based approach broke primary controls. If we want click-through regions later, it should be implemented as explicit view-level hit testing instead of toggling `NSWindow.ignoresMouseEvents` for the whole panel.
