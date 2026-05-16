# CueWhisper Smoke Test

## Prerequisites

1. Download the whisper model:
   ```bash
   bash infra/scripts/download-whisper-model.sh
   ```
   Model: `~/.cache/bluey/whisper/tiny.en-q5_1.bin` (~31 MB)

2. Build:
   ```bash
   cd native/macos/cue-whisper
   swift build -c release
   ```

## Test: Sine wave (non-speech audio)

Generate a 440Hz tone (3 seconds, PCM16 16kHz mono):
```bash
python3 -c "
import struct, math
sr=16000; dur=3
samples = [int(16000*math.sin(2*math.pi*440*i/sr)) for i in range(sr*dur)]
with open('/tmp/tone.pcm','wb') as f:
    for s in samples:
        f.write(struct.pack('<h', max(-32768, min(32767, s))))
"
cat /tmp/tone.pcm | .build/release/CueWhisper 2>/dev/null
```

Expected output (non-speech detected):
```json
{"type":"partial","text":"[transcribing...]"}
{"type":"final","text":"(vocalizing)","confidence":0.65}
```

## Test: Silence (should produce no output)

```bash
dd if=/dev/zero bs=96000 count=1 2>/dev/null | .build/release/CueWhisper 2>/dev/null
```

Expected: no stdout output (silence skipped).

## Test: Real speech (requires a WAV file)

```bash
# Convert any WAV to raw PCM16 16kHz mono:
ffmpeg -i speech.wav -f s16le -acodec pcm_s16le -ar 16000 -ac 1 /tmp/speech.pcm
cat /tmp/speech.pcm | .build/release/CueWhisper 2>/dev/null
```

Expected: NDJSON lines with actual transcription text and confidence scores.

## Environment variables

- `BLUEY_WHISPER_MODEL`: Override model path (default: `~/.cache/bluey/whisper/tiny.en-q5_1.bin`)
