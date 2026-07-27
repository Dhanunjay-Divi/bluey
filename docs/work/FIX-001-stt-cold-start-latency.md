# FIX-001: On-device STT Cold-Start Latency

## Issue

The first live utterance could wait several seconds for the Parakeet/Nemotron
model to load after the user clicked Listen. Starting microphone and system
capture together could also load two copies of the approximately 650 MB model.

## Root Cause

`ParakeetProvider::connect` created a worker immediately, but that worker did not
load `SttEngineHandle` until capture had already started. The process-wide cache
checked for a handle, released its mutex, and then loaded outside the lock. Two
sources that missed the cache together therefore both performed the expensive
load before either populated the cache.

The 2026-07-26 live daemon log measured 2.68 seconds from provider construction
to `STT engine ready`. It also recorded two System-source ready events 55 ms
apart, confirming concurrent duplicate initialization. First-run model download
was a separate, much larger delay because provisioning also began only on Listen.

The inference worker additionally wrote every decoded fragment synchronously to
stderr, adding avoidable hot-path I/O and exposing transcript text in terminal
logs.

## Fix Summary

Startup provisions and prewarms Parakeet in the background, and the provider
factory now waits for that same single-flight warm model before it reports a
source as connected. An immediate Listen click therefore stays in Connecting
instead of launching capture and queuing stale audio behind a cold model load.

- Provision and prewarm Parakeet in a background startup task while onboarding
  and the overlay initialize.
- Load model weights through a condition-variable single-flight cache. Concurrent
  sources wait without holding the load mutex and then share one handle.
- Run one full 560 ms encoder window of silence through a disposable stream.
  This initializes ONNX execution buffers without contaminating real decoder
  state.
- Serialize first-run provisioning so startup prewarm and an immediate Listen
  action cannot race the same `.part` downloads.
- Replace synchronous transcript-bearing `eprintln!` diagnostics with structured
  debug timing fields that do not include meeting content.
- Follow the existing provider policy: skip the large local-model prewarm when
  Mock, Deepgram, OpenAI, or LocalWhisper is configured, unless Parakeet is
  explicitly forced on.
- Keep `BLUEY_STT_PREWARM=0` as an explicit escape hatch for memory-constrained
  deployments.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Start fail-soft STT provisioning/prewarm off the async runtime |
| `crates/cue-daemon/src/stt/model_setup.rs` | Single-flight first-run model provisioning |
| `crates/cue-daemon/src/stt/parakeet.rs` | Single-flight model cache, prewarm entry point, privacy-safe diagnostics, concurrency tests |
| `crates/cue-transcribe/src/engine.rs` | Disposable full-window inference warmup and coverage test |

## Edge Cases Handled

- A user clicks Listen before startup prewarm finishes: the provider joins the
  in-progress load rather than loading duplicate weights.
- Startup provisioning fails: the daemon stays available and the existing lazy
  provider path can retry later.
- The first load fails: all waiters wake and a later caller can retry.
- A development model-directory override changes: the cache loads the new
  directory instead of reusing incompatible weights.
- Memory-constrained environments can disable eager residency without disabling
  on-device STT.
- Cloud/LocalWhisper users do not download or retain Parakeet merely because the
  binary was built with local STT support.

## How to Test

```bash
cargo test -p cue-transcribe
cargo test -p cue-daemon --features parakeet-stt stt::parakeet::tests
cargo clippy -p cue-transcribe --all-targets -- -D warnings
cargo clippy -p cue-daemon --features parakeet-stt --all-targets -- -D warnings
RUST_LOG=cue_daemon::stt::parakeet=debug bluey on
```

For the live check, wait for `Parakeet STT is warm before capture`, then click
Listen. Provider startup should reuse the cached handle immediately, and only
one `Parakeet model loaded and inference path prewarmed` event should appear.

## Known Limitations

- Prewarming intentionally keeps the shared model resident while the daemon
  runs. Set `BLUEY_STT_PREWARM=0` to retain lazy loading on constrained systems.
- Nemotron commits on an intrinsic approximately 560 ms streaming window.
  Prewarming removes cold-start delay and avoidable I/O, but does not claim to
  remove that model-level cadence.
- A missing first-run model still has to download once. Startup moves that wait
  into visible onboarding progress; the installer `--preload-models` path remains
  the preferred way to avoid it during a meeting.
