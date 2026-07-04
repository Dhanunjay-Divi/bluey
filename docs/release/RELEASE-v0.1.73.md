# Bluey 0.1.73

Live STT quality and speed pass.

## Changes

- Improved macOS and Windows audio helper downsampling by averaging each source window before emitting 16 kHz PCM.
- Lowered managed Deepgram realtime endpointing to `200 ms` for faster finalization.
- Added default technical Deepgram keyterms for coding/interview vocabulary.
- Added `BLUEY_DEEPGRAM_KEYTERMS` and `BLUEY_DEEPGRAM_DEFAULT_KEYTERMS` deployment controls.

## Verification

```bash
cargo test --manifest-path server/Cargo.toml stt::tests -- --nocapture
bash native/macos/cue-audio/build.sh
x86_64-w64-mingw32-gcc -Wall -Wextra -Werror -D_WIN32_WINNT=0x0601 native/windows/cue-audio/main.c -lole32 -luuid -o /tmp/bluey-audio.exe
cargo test -p cue-daemon live_stt_ -- --nocapture
```

