# Round 333 - Live STT Speed And Accuracy

Date: 2026-07-04
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner reported that live transcript quality still felt poor, mic/system audio were missing words, Deepgram felt slower than Web Speech API-style realtime captions, and technical phrases such as coding terms were being misheard.

## Findings

- The current macOS live relay path already normalizes mic and system audio to `16 kHz` PCM before sending it through the managed `/stt/relay` path.
- The Deepgram websocket already uses `interim_results=true` and `no_delay=true`, so the pipeline is capable of partial realtime captions.
- The native macOS and Windows helpers were downsampling by effectively picking the current sample when an output sample was due. That is fast, but it can make speech harsher and less clear before it reaches Deepgram.
- The server relay did not send any Deepgram `keyterm` hints, so technical/interview terms like `LRU`, `API`, `SQL`, `Kubernetes`, `CI/CD`, and model/tool names had no vocabulary bias.
- Server relay default endpointing was `300 ms`, which favors phrase completeness over faster finalization.

## Changes

### Server Relay

- Lowered managed Deepgram realtime default endpointing from `300 ms` to `200 ms`.
- Added default technical keyterms for the live Deepgram URL.
- Added env extension points:
  - `BLUEY_DEEPGRAM_KEYTERMS`: comma, semicolon, or newline separated extra keyterms.
  - `BLUEY_DEEPGRAM_DEFAULT_KEYTERMS=0|false|off|none`: disables built-in keyterms for A/B tests.
- Keyterms are URL-escaped, deduplicated case-insensitively, and capped at 50 total terms.
- Kept private/customer-specific names out of source. Those can be configured per deployment through env instead of shipped inside downloadable binaries.

### Native Audio Helpers

- macOS helper now averages the small source-sample window that maps into each 16 kHz output sample instead of dropping/picking samples.
- Windows helper now uses the same averaging approach.
- Wire format is unchanged: `16 kHz`, mono, signed i16 PCM.

## Why This Should Help

- Partial captions should still arrive through the existing interim frame path.
- Final captions should become available slightly sooner because endpointing is back to `200 ms`.
- Technical words should be less likely to turn into unrelated English words.
- Cleaner downsampling should help Deepgram hear consonants and short terms more reliably without adding meaningful latency.

## What This Does Not Finish

- The overlay still needs the separate UI pass that makes live captions feel like a true horizontal scrolling partial stream.
- The best long-term audio fix is a higher-quality native converter or provider-side 48 kHz ingest with explicit sample-rate metadata. This round keeps the current relay contract stable.
- For resume/interview names and company-specific vocabulary, production should set `BLUEY_DEEPGRAM_KEYTERMS` from admin config or session context rather than hardcoding personal terms.

## Verification

Passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml stt::tests -- --nocapture
bash native/macos/cue-audio/build.sh
x86_64-w64-mingw32-gcc -Wall -Wextra -Werror -D_WIN32_WINNT=0x0601 native/windows/cue-audio/main.c -lole32 -luuid -o /tmp/bluey-audio.exe
cargo test -p cue-daemon live_stt_ -- --nocapture
```

Note: root `cargo fmt --manifest-path Cargo.toml` is not applicable because the workspace root has no direct targets; server formatting ran successfully.

## Deployment

Source commit:

- `0b1d40c72c4484668c29254e5e7b3566c959b8b8`

Desktop release:

- Published desktop release `0.1.73` to `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.73/bluey-0.1.73-darwin-arm64.tar.gz`
- Artifact SHA256:
  `ef01ad43308913dfffd5081f2780013ddfd2ea8df102f57b6487644e59c5d0ca`
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- Installer MIME checks passed:
  - `install.sh`: `application/x-shellscript`
  - `install.ps1`: `application/x-powershell`
- Unpacked release binaries report `0.1.73`.
- Public installer smoke passed:
  - `/Users/uno/.bluey/bin/bluey --version` -> `bluey 0.1.73`
  - `/Users/uno/.bluey/bin/bluey-daemon --version` -> `bluey-daemon 0.1.73`

Production API relay:

- Refreshed droplet Rust toolchain to `rustc 1.96.1` / `cargo 1.96.1` because the previous `cargo 1.75.0` could not read the current lockfile or 2024-edition transitive crate metadata.
- Built in:
  `/opt/bluey-build-codex-round333-stt-speed`
- Installed binary:
  `/usr/local/bin/bluey-server`
- Binary SHA256:
  `906db8b0b16c8045b02290020f1489a03b2956273a92baab4d6ccd5a07f4cc2c`
- Previous binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260704T102752Z`
- `bluey-api.service`: active
- `NRestarts`: `0`
- Public health returned:
  `commit=0b1d40c72c4484668c29254e5e7b3566c959b8b8`
- Recent production warning/error scan after restart returned no lines.

## Next STT Work

- Make the overlay live caption strip a true horizontal partial stream so interim captions feel immediate instead of final-caption delayed.
- Add dynamic per-session keyterms from attached resumes/docs/question context without logging or storing private terms in release binaries.
- Add a live STT latency smoke that speaks or replays deterministic audio and reports first-audio, first-partial, first-final, and transcript quality.
