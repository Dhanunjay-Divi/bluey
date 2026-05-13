# Bluey Architecture

## Processes

```text
cue CLI
  |
  | JSON over local TCP
  v
cue-daemon
  |
  | JSONL over stdin/stdout
  v
native overlay sidecar
  |
  | short-lived raw PCM helper calls on macOS
  v
native audio sidecar
```

The daemon owns long-running state. The overlay is intentionally separate because macOS and Windows need direct native window APIs for capture exclusion. Audio helpers are also separate so ScreenCaptureKit/CoreAudio and WASAPI permission/capture code stays native without adding driver installs to the user flow.

## Current IPC

CLI to daemon uses one JSON request per TCP connection. The first version keeps this boring and debuggable:

- `ping`
- `status`
- `shutdown`
- `overlay_show`
- `overlay_hide`
- `overlay_toggle`
- `overlay_clear`
- `overlay_set_opacity`
- `overlay_set_position`
- `push_card`
- `audio_status`
- `audio_start`
- `audio_stop`
- `ai_status`
- `cloud_status`
- `cloud_sync_now`

Daemon to overlay uses JSON lines over stdin. Overlay events are JSON lines on stdout.

## Native Overlay Requirements

macOS:

- non-activating `NSPanel`
- `sharingType = .none`
- `screenSaver` level
- all spaces and fullscreen auxiliary behavior
- click-through by default

Windows:

- topmost layered tool window
- `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`
- click-through via `WS_EX_TRANSPARENT`
- no taskbar entry

## Next Modules

1. STT provider trait and streaming adapters
2. VAD and partial transcript pipeline
3. Managed AI answer orchestrator
4. Authenticated cloud sync client
5. Cloud RAG memory service
6. Dashboard/history service
7. Background recap and memory extraction

## Pipeline Seams

The core crate now contains the long-lived contracts:

- `audio.rs`: dual system/microphone capture planning, device/source status, audio chunks, and STT segment metadata.
- `ai.rs`: managed provider routing, fallback policy, budgets, privacy/safety flags, answer requests/responses, streaming events, and provider health.
- `cloud.rs`: cloud auth/sync state, retention, sync events, memory chunks, RAG queries/results/citations, export, and deletion requests.

## Commercial Cloud Direction

The local JSON store is a development surface. Production should use authenticated Bluey cloud storage with workspace/tenant scoping, encrypted artifact storage, metadata database, vector retrieval, retention controls, and billing-aware access.
