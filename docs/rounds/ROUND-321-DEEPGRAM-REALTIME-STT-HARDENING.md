# Round 321 - Deepgram Realtime STT Hardening

Date: 2026-07-03
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner reported that Deepgram still felt slower than expected, live captions were not streaming like a realtime horizontal caption rail, words were being missed, and linked a Deepgram Rust/Twilio realtime streaming example as the expected direction.

## Root Cause

- Bluey was already using Deepgram interim streaming, but there was not enough timing telemetry to prove where a slow or empty Listen session was failing.
- The live relay close path could terminate the WebSocket without giving Deepgram a short tail window to flush final transcript frames, so the last words of a Listen run could disappear.
- Round 318 lowered endpointing to `200 ms` for speed. That can make finals feel snappier, but it is too aggressive for conversational speech and can split or finalize phrases before late words arrive.
- The Deepgram realtime URL did not default to a language, so the provider could spend effort on language detection in the common English beta case.

## Fix

- Changed managed Deepgram realtime defaults:
  - `endpointing=300`
  - `utterance_end_ms=1000`
  - `interim_results=true`
  - `vad_events=true`
  - `no_delay=true`
  - `language=en-US` by default
- Kept `BLUEY_DEEPGRAM_LANGUAGE=auto|detect|none|off` as an escape hatch for non-English or auto-detect testing.
- Added privacy-safe server relay frame telemetry:
  - provider text frame count
  - provider transcript frame count
  - partial/final/empty/control frame counts
  - first provider frame ms
  - first transcript ms
  - first partial ms
  - first final ms
  - transcript character/word counts only, no transcript text or audio bytes in logs
- Added desktop relay tail finalization:
  - when a live STT source stops, Bluey sends Deepgram `CloseStream`
  - Bluey drains provider frames for up to `850 ms`
  - the tail wait is configurable with `BLUEY_LIVE_STT_FINALIZE_WAIT_MS` or `BLUEY_STT_FINALIZE_WAIT_MS`
  - then Bluey closes the relay and kills the native helper
- Bumped workspace desktop version to `0.1.60`.

## Why This Should Feel Better

- Interim captions still drive realtime display.
- Endpointing is now tuned for phrase completeness instead of racing every short pause.
- Stop/Enter should be less likely to lose the final spoken words.
- Logs now show whether the slow leg is local audio, relay open, Deepgram first partial, Deepgram finalization, or an empty/silent audio stream.

## Verification

Passed locally:

```bash
cargo fmt --all
cargo test --manifest-path server/Cargo.toml deepgram_url -- --nocapture
cargo test --manifest-path server/Cargo.toml deepgram_frame_inspection -- --nocapture
cargo test --manifest-path server/Cargo.toml stt::tests -- --nocapture
cargo test -p cue-daemon live_stt_ -- --nocapture
cargo test -p cue-daemon parse_frame -- --nocapture
cargo check -p cue-daemon
cargo check --manifest-path server/Cargo.toml
```

## Deployment

Desktop release:

- Published desktop release `0.1.60` to `bluey.sh`.
- Live release metadata:
  - `https://bluey.sh/latest.json`
  - artifact: `https://bluey.sh/releases/v0.1.60/bluey-0.1.60-darwin-arm64.tar.gz`
  - artifact SHA256: `16164a213784b2d02e7293e5980bde5f8d328f37b1313ae87da52932d7cdcf2f`
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- `/install.sh` returned `application/x-shellscript`.
- `/install.ps1` returned `application/x-powershell`.
- Unpacked macOS release binaries reported `0.1.60`.

Production API relay:

- Synced source into existing droplet build tree:
  `/opt/bluey-build-codex-round318-latency`
- Built Linux production binary on droplet:
  `/opt/bluey-build-codex-round318-latency/server/target/release/bluey-server`
- Installed binary:
  `/usr/local/bin/bluey-server`
- Binary SHA256:
  `860b236c1e311357c3ecb025b8c5a1e4a67183ee7f3956920f8ebc0bc5ee8e0a`
- Previous binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260703T170447Z`
- `bluey-api.service`: active
- `NRestarts`: `0`
- Public health returned OK.
- Recent production API warning/error log check after restart returned no entries.

## Current State

Desktop `0.1.60` is live and production API relay code is deployed.

## Remaining QA / Gates

- Run a signed-in live Listen smoke and inspect timing logs for:
  - first audio
  - first audible audio
  - first provider frame
  - first provider transcript
  - first partial
  - first final
  - tail finalize frame count
- Compare live caption feel against the expected realtime target:
  - first visible partial: roughly `300-900 ms` after speech starts in a healthy network path
  - final caption: usually around `1-2 s` after a pause
