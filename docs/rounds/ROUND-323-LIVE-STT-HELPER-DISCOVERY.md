# Round 323 - Live STT Helper Discovery

Date: 2026-07-03
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner reported that mic/listen transcription still felt delayed, inaccurate, and not realtime after the Deepgram tuning round.

## Root Cause

The active local session logs showed Listen using:

```text
bluey-managed:deepgram/nova-3 chunked
```

instead of the intended live relay:

```text
bluey-managed:deepgram/nova-3 live
```

The daemon selected live relay only when every resolved audio source used the native helper. Installed Bluey had the helper at `~/.bluey/bin/bluey-audio-macos`, but daemon helper discovery only checked the current executable directory and repo-relative paths. When launched through `/usr/local/bin/bluey-daemon`, that could miss the installed helper and fall back to FFmpeg/chunked STT.

## Fix

- Added installed-bin helper discovery for macOS and Windows.
  - Checks the canonical executable path.
  - Checks `~/.bluey/bin` from `HOME` / `USERPROFILE`.
  - Keeps env-var overrides first.
- Added privacy-safe STT source/transport diagnostics.
  - Healthy live mode remains info-level.
  - Chunked/fallback mode is warn-level, so it appears under production `RUST_LOG=warn`.
- Added installer helper symlinks in both:
  - `ops/install/install.sh`
  - `scripts/install.sh`

## Windows Parity Check

Windows helper discovery received the same installed-bin fallback and helper symlink behavior. The Windows native helper names checked are `bluey-audio.exe` and `cue-audio.exe`.

## Verification

Passed locally:

```bash
cargo fmt --check
cargo check -p cue-daemon
bash -n scripts/install.sh && bash -n ops/install/install.sh
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

Local package verification:

```text
bluey 0.1.62
bluey-daemon 0.1.62
```

Local IPC audio smoke after hot-installing the fixed daemon:

```text
stt_provider: bluey-managed:deepgram/nova-3 live
system device: Native system audio
microphone device: Default microphone
```

## Deployment

- Published desktop release `0.1.62` to `bluey.sh`.
- Live release metadata:
  - `https://bluey.sh/latest.json`
  - artifact: `https://bluey.sh/releases/v0.1.62/bluey-0.1.62-darwin-arm64.tar.gz`
  - artifact SHA256: `b05acbfc3a07a58436075cc30368b132add5a2b4b09f45044dcce7b481f88ab3`
  - artifact size: `9181487` bytes
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- `/install.sh` returned `application/x-shellscript`.
- `/install.ps1` returned `application/x-powershell`.
- Unpacked macOS release binaries reported `0.1.62`.

## Current State

The local running daemon and public release are both on `0.1.62`. IPC audio start now selects live Deepgram relay with native system and microphone devices.

## Remaining QA / Gates

- Owner should do a real spoken Listen test from the overlay, because this environment can prove the live STT route but cannot speak through the user's microphone with natural audio.
