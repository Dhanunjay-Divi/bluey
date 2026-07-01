# Round 282 - STT Audible Gate Relay Diagnostics

## Trigger

The owner reported that Deepgram transcription was not detecting speech well and that both microphone and system Listen felt broken. They also previously saw balance drops from quickly toggling Listen, so the STT path needed a production pass rather than another UI-only patch.

## Root Cause

- The managed live STT relay started a paid server/Deepgram session as soon as the native helper produced bytes, even if those bytes were room silence.
- System audio on macOS can produce no packets when no app is actively emitting captured audio; the overlay had no clear diagnostic for that state.
- The desktop and server logs recorded byte counts, but not privacy-safe audio levels, so user-specific failures were hard to diagnose without reproducing their personal audio environment.
- The server relay Deepgram URL had the core PCM settings, but it did not mirror the direct Deepgram path's endpointing/VAD tuning.

## Fix

- Added a live STT audible gate in the daemon:
  - waits for audible PCM before creating the paid `/stt/session`
  - keeps a short prebuffer so the first audible words are forwarded after the session opens
  - avoids starting paid transcription for pure silence or no-packet startup
- Added user-visible quiet-source notices:
  - `Mic is quiet` when mic bytes are present but below the speech threshold
  - `System audio is quiet` when system audio is silent or not producing packets
- Added privacy-safe desktop diagnostics:
  - sample count
  - RMS dBFS
  - peak dBFS
  - nonzero percentage
  - no transcript text or raw audio is logged
- Added privacy-safe server relay diagnostics for forwarded audio chunks using the same level fields.
- Added no-transcript provider-frame logging on desktop with only the provider frame type and payload size.
- Tuned the server Deepgram live URL to include:
  - `endpointing=300`
  - `utterance_end_ms=1000`
  - `vad_events=true`
  - optional `BLUEY_DEEPGRAM_LANGUAGE`
- Windows parity:
  - the audible gate, server relay tuning, billing avoidance, and diagnostics are shared Rust/server behavior and apply to Windows desktop builds.
  - no Windows native audio-helper code change was required; its helper already emits 16 kHz mono i16 PCM into the same daemon path.
- Bumped desktop workspace version to `0.1.37`.

## Verification

Passed locally:

```bash
cargo fmt --all
cargo check -p cue-daemon --quiet
cargo test -p cue-daemon pcm16_i16le_stats_detect_silence_and_audible_samples --quiet
cargo test --manifest-path server/Cargo.toml stt::tests --quiet
cargo check --manifest-path server/Cargo.toml --quiet
bash native/macos/cue-audio/build.sh
timeout 2s native/macos/cue-audio/.build/bluey-audio-macos --source microphone --continuous > /tmp/bluey-mic-test-after.pcm
```

Direct helper smoke:

- Microphone helper produced PCM bytes on this Mac.
- System helper produced zero bytes in one smoke when no active system audio packets were available; that is now surfaced as a waiting/quiet-source state instead of starting paid transcription or looking silently broken.

Passed release/deploy checks:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

- Production `bluey-api.service` was rebuilt on the droplet, installed to `/usr/local/bin/bluey-server`, and restarted active.
- Previous server binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260701T185425Z`
- Live `https://bluey.sh/health` returned `status=ok`.
- Live `https://bluey.sh/latest.json` reported `0.1.37`.
- Live macOS artifact:
  `https://bluey.sh/releases/v0.1.37/bluey-0.1.37-darwin-arm64.tar.gz`
- Live SHA256:
  `8c698f5faf664823bb304d790c743bc3f1b78baf67725d19b15d4ce4c6a0e942`
- Live `/install.sh` returned `application/x-shellscript`.
- Live `/install.ps1` returned `application/x-powershell`.

## Current State

- `v0.1.37` was published and the server relay was deployed.
- A follow-up Round 283 immediately supersedes the desktop artifact for click-through move-handle reliability.

## Remaining QA

- Run a real audible mic smoke after installing `0.1.37` and confirm:
  - no billing/session starts during silence
  - first spoken words are not clipped
  - transcript segments appear after speech
  - logs include level diagnostics but no transcript text/raw audio
- Run system-audio smoke while a browser/video/meeting is producing sound.
