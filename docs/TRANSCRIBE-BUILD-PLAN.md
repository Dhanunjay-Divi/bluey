# cue-transcribe — Fresh Build Plan (real-time, cross-platform, on-device STT)

> The single source of truth for the transcription engine rebuild. Goals locked
> with the user 2026-06-29. We copy the proven `nifty-brahmagupta` prototype,
> make it cross-platform + production-grade, and wire it into Bluey. Research is
> grounded (see Sources at bottom) so we build each hard part right the first time.

## 1. Goal (one sentence)

A lean, scalable, production-grade engine that transcribes **system audio (them) +
mic (you)** in **real time, on-device, accurately**, on **macOS (Apple Silicon +
Intel) and Windows**, and saves a clean transcript.

## 2. What we are NOT doing (the traps that wasted a day)
- ❌ NOT capturing audio inside the Tauri overlay webview (`getUserMedia`) — the
  browser can't capture native-app/system audio on macOS, and it dragged in
  .app-bundle/permission/respawn hell.
- ❌ NOT relying on CoreML — parakeet-rs's own author says CoreML is unstable for
  this model and **CPU is faster than Whisper-metal on M-series anyway**.
- ❌ NOT a bare CLI helper for macOS system audio — a plain executable **cannot
  appear in System Settings → Screen Recording**, and ad-hoc signing **wipes the
  grant on every rebuild**. (This was the root of today's silent-audio nightmare.)
- ❌ NOT rebuilding the UI — the existing overlay UI stays.

## 3. Architecture (grounded, cross-platform)

```
 capture (per-OS)  ──16kHz mono PCM16──►  STT engine (shared)  ──►  transcript out
 ────────────────                         ─────────────────         ─────────────
 macOS system : ScreenCaptureKit          parakeet-rs Nemotron      labeled lines
   (signed .app, not a bare CLI)            streaming, 560ms chunks  (You / They),
 macOS mic    : cpal                        ort CPU EP (default)     saved + live
 Windows sys  : wasapi crate (loopback)     same model, same code    feed to Bluey
 Windows mic  : cpal/wasapi
```

### 3a. Capture layer (the hard, per-OS part)
| Source | macOS | Windows |
|--------|-------|---------|
| **System (them)** | ScreenCaptureKit, `capturesAudio=true`. **MUST ship as a codesigned `.app` bundle** with a stable signing identity + deployment target ≥ 14.4, and call `CGRequestScreenCaptureAccess()` at startup so it (a) appears in Screen Recording settings and (b) the grant survives rebuilds. | `wasapi` crate loopback: `get_default_device(Direction::Render)` → loopback capture client, `Direction::Capture`, `ShareMode::Shared`. **Poll for data — event mode does NOT work for loopback.** |
| **Mic (you)** | `cpal` default input | `cpal` (or `wasapi` capture) |

All capture normalizes to **16kHz mono PCM16** (the model's input), framed in
~100ms chunks (the prototype's proven `pcm-worklet` ratio, done natively here).

### 3b. STT engine (shared across platforms — copied from prototype)
- `parakeet-rs` Nemotron **streaming** (cache-aware, 560ms chunks, punctuation).
- `ort` with the **CPU execution provider as the default everywhere** (stable +
  fast). DirectML (Windows) / CoreML (macOS) are optional, behind cargo features,
  only if measured to help — default is CPU.
- ONE provider per source (mic + system are separate stateful streams — proven
  earlier: sharing one stateful model across speakers corrupts the transcript).

### 3c. Cargo feature/target matrix (from the prototype, extended for Windows)
```toml
[target.'cfg(target_os = "macos")'.dependencies]
parakeet-rs = { version = "0.3", default-features = false, features = ["sortformer"] } # CPU default
# (CoreML feature available but OFF by default — unstable for this model)

[target.'cfg(target_os = "windows")'.dependencies]
parakeet-rs = { version = "0.3", default-features = false, features = ["sortformer"] } # CPU
wasapi = "0.x"  # system-audio loopback
```

## 4. Crate layout: `crates/cue-transcribe` (new, self-contained)
```
cue-transcribe/
  Cargo.toml
  src/
    lib.rs           # public API: start(sources) -> stream of TranscriptLine
    engine.rs        # parakeet Nemotron streaming (copied from prototype nemo_session.rs)
    capture/
      mod.rs         # trait AudioCapture -> 16k mono PCM16 frames
      macos_system.rs  # ScreenCaptureKit (or shells the signed .app helper)
      macos_mic.rs     # cpal
      windows_system.rs# wasapi loopback
      windows_mic.rs   # cpal/wasapi
  native/macos/BlueyAudio.app   # the SIGNED .app bundle for system capture (the real fix)
```
Bluey's daemon depends on `cue-transcribe` and consumes its transcript stream —
the daemon stops owning STT/capture internals.

## 5. Build order (one clean pass, verify each step before the next)
1. Scaffold `cue-transcribe`; copy the prototype's Nemotron streaming engine; prove
   it transcribes a WAV (we already measured this works: 0.12x RTF, correct text).
2. macOS mic capture (cpal) → live transcript. Simplest real capture.
3. macOS system capture: build the **signed `.app`** helper (the permission fix).
   Verify it appears in Screen Recording settings + captures non-silent audio.
4. Windows system (wasapi loopback) + mic. Research-backed; build + verify on Win.
5. Wire into Bluey daemon; feed the existing UI. Save transcript (clean).

## 5b. Progress (2026-06-29)
- ✅ `cue-transcribe` crate built + proven (transcribes WAV; 0.12x RTF).
- ✅ `BlueyAudio.app` — system helper as a grantable, signed `.app` bundle
  (`native/macos/cue-audio/bundle-app.sh`). The fix for the silent-capture wall:
  a bare CLI can't hold the Screen Recording TCC grant; a `.app` can.
- ✅ Daemon auto-discovers `BlueyAudio.app` (`system_capture.rs::platform_binary_path`)
  and transcribes live system audio, labeled `[system]` — PROVEN.
- ✅ Daemon ASR migrated to `cue-transcribe::SttEngine` (one engine; the old
  `parakeet.rs` is now a thin SttProvider adapter over it; sortformer diarization
  kept). 231 tests pass.
- ✅ Build/install ship the `.app`: `scripts/build-macos.sh` bundles + stages it;
  `install.sh` copies it (recursive); `run-local.sh` dev-installs so `bluey on`
  uses the latest build. Capture starts on the overlay "Listen" button.
- ⏳ Signing: ad-hoc for now (grant resets on rebuild). Real persistence needs an
  Apple Developer cert via `BLUEY_CODESIGN_IDENTITY` (user-provided).
- ⏳ TODO: mic capture (Step 2); Windows wasapi loopback (Step 4).

### Cross-platform caveat (flagged during migration)
On **Intel macOS**, the daemon overrides `parakeet-rs` to `load-dynamic` while
`cue-transcribe` requests `ort-defaults` (prebuilt). Cargo feature unification
would enable both on that target → potential link conflict. arm64 unifies cleanly
on `ort-defaults` (verified). Resolve before an Intel build (align both crates on
one ort linking mode per target).

## 6. Definition of done
- One `cargo build` per target produces a working binary.
- System + mic transcribe live, labeled, accurate, on-device.
- macOS grant survives rebuilds (stable signing).
- No webview capture, no dead code, lean.

## Sources (research, 2026-06-29)
- wasapi loopback (Direction::Capture, ShareMode::Shared, poll not event): https://docs.rs/wasapi , https://github.com/HEnquist/wasapi-rs
- cpal WASAPI loopback is unreliable (PR added then removed): https://github.com/RustAudio/cpal/issues/476
- ort execution providers (CoreML/DirectML/CPU availability): https://ort.pyke.io/perf/execution-providers
- parakeet-rs (CoreML unstable, CPU faster than whisper-metal; 560ms streaming): https://github.com/altunenes/parakeet-rs
- macOS: bare CLI can't appear in Screen Recording; .app bundle does; ad-hoc signing wipes grant on rebuild; CGRequestScreenCaptureAccess registers the app: https://developer.apple.com/documentation/screencapturekit/ , https://dgrlabs.co/blog/2026-04-25-capturing-system-audio-on-macos-in-2026.html
