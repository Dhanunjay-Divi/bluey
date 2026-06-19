# Windows v0.1.x — Implementation Brief for Codex

> **For:** Codex agent on uno (via Tailscale to the Windows test machine).
> **From:** Kiro post-v0.1.0 GA (macOS arm64 + x86_64 universal).
> **Goal:** ship Windows x86_64 as a v0.1.x point release with feature parity
> on capture, transcription, and overlay UX. macOS GA is already shipped;
> Windows does NOT block macOS testing.
>
> **Estimate:** 2–3 days code + clean-Windows QA.

---

## 1. Scope

Ship a Windows x86_64 build that has:

1. Real `whisper.cpp` transcription (today: stub).
2. Functional native overlay matching the macOS pill UX.
3. System audio + microphone capture via WASAPI.
4. Installer path + symlink/shim handling appropriate for Windows.
5. End-to-end smoke pass on a clean Windows 10/11 (no dev toolchain).

**Out of scope for v0.1.x Windows:**

- Vision capture (defer to v0.2).
- Process masquerading (Windows path exists in code; Windows-specific
  mechanics differ from macOS argv-overwrite — not a blocker for first ship).
- Auto-update flow.

---

## 2. Items, in order

### W1 — Real whisper.cpp on Windows

**Today:** `native/windows/cue-whisper/main.c` documents the
`BLUEY_WHISPER_MODEL` env var and emits a "not implemented" error JSON. macOS
has the real `SwiftWhisper` integration shipped in R10.

**Approach:**

1. Add `whisper.cpp` as a git submodule under `native/windows/cue-whisper/whisper.cpp/`,
   pinned to the same revision the Swift binding uses (check `native/macos/cue-whisper/Package.resolved`).
2. Build via CMake from `native/windows/cue-whisper/build.ps1`:
   ```pwsh
   cmake -S whisper.cpp -B whisper.cpp/build -DBUILD_SHARED_LIBS=OFF
   cmake --build whisper.cpp/build --config Release --target whisper
   ```
3. Static-link `whisper.lib` into `cue-whisper.exe`. The C source already
   parses NDJSON and emits status JSON; replace the stub with real
   `whisper_init_from_file` + `whisper_full` calls.
4. Match the JSON IPC the daemon already speaks to the macOS variant:
   - Inbound: `{"type":"transcribe","path":"...","language":"en"}`
   - Outbound: `{"type":"partial","text":"..."}`, `{"type":"final","text":"..."}`,
     `{"type":"error","message":"..."}`.
5. Model files live under `%LOCALAPPDATA%\bluey\whisper\`. Default to
   `tiny.en-q5_1.bin` like macOS.

**Acceptance:** transcribe a 10s WAV on the Windows test machine and verify
the daemon receives the same JSON shape as macOS.

### W2 — Native overlay (Windows)

**Today:** `native/windows/cue-overlay/main.c` exists with R11 token
handshake and length caps, but the UX layer is older and has not been
reconciled with the post-R12 macOS pill UX.

**Approach:**

1. Mirror the macOS pill UX:
   - 112×28 borderless top-window pill at top-center of primary display.
   - Click-to-expand to 480×560 panel (feed + composer + buttons).
   - Drag-to-move via `WM_NCHITTEST` returning `HTCAPTION`.
   - Capture-excluded via `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`.
2. Honor every `OverlayCommand` variant (Show/Hide/Toggle/Boot/PushCard/
   UpdateCard/SetOpacity/SetPosition/Clear/Shutdown). The protocol reference
   is `crates/cue-core/src/overlay.rs::OverlayCommand`.
3. Emit OverlayEvent variants (AskRequested, AttachRequested, etc.) wrapped
   with the per-session token from `BLUEY_OVERLAY_SESSION_TOKEN` env var.
   The Windows event emitter helpers exist; verify they include the token
   on every emitted line (the pattern is the same as macOS
   `emit_token_field()`).
4. Pill-first behaviour: `bluey on` does NOT auto-expand on Windows. The
   pill appears; user clicks to open feed.

**Acceptance:** `bluey on` on Windows shows a top-center pill that:
- is excluded from screen captures (validated with Snipping Tool),
- expands on click,
- emits `ask_requested` when the composer Ask button is pressed,
- closes on `bluey off`.

### W3 — Audio capture (WASAPI)

**Today:** Cross-platform audio scaffold via CPAL is in place. Windows-side
WASAPI capture works for microphone via CPAL. System audio capture (loopback)
needs the WASAPI loopback path; verify it currently works on the Windows test
machine.

**Approach:**

1. Confirm `cpal` `WasapiHost` enumeration produces both mic and system
   loopback devices in `crates/cue-daemon/src/audio/system_capture.rs`.
2. If loopback is missing, switch to a direct `IAudioClient` capture with
   `AUDCLNT_STREAMFLAGS_LOOPBACK` flag (~150 lines of C/Rust FFI; see
   reference apps in `_refs/`).
3. Verify `pcm16` frames flow into the daemon's STT pipeline.

**Acceptance:** while a YouTube video plays on the Windows test machine,
`bluey on --title "test"` + `bluey ask "what was just said?"` returns a
transcript-aware answer.

### W4 — Installer / packaging

**Today:** The Makefile has a `package-windows-x86_64` target that produces a
.zip. There is no Windows installer.

**Approach:**

1. Build artifact: `bluey-0.1.x-windows-x86_64.zip` containing:
   - `bin\bluey.exe`
   - `bin\bluey-daemon.exe`
   - `bin\bluey-overlay.exe`
   - `bin\bluey-whisper.exe` (with whisper.cpp statically linked)
2. PowerShell installer `ops\install\install.ps1`:
   - Extracts to `%LOCALAPPDATA%\Bluey\bin`.
   - Adds `%LOCALAPPDATA%\Bluey\bin` to user PATH (without sudo).
   - Mirrors the macOS `ops/install/install.sh` behavior with release-manifest
     discovery and SHA256 verification.
3. Daemon startup: on Windows the daemon should self-register as a Task
   Scheduler entry OR rely on the user manually running `bluey on`. Pick the
   latter for v0.1.x simplicity.

**Acceptance:**
```pwsh
$env:BLUEY_VERSION = "0.1.x"
$env:BLUEY_ARTIFACT_URL = "https://bluey.sh/releases/v0.1.x/bluey-0.1.x-windows-x86_64.zip"
$env:BLUEY_ARTIFACT_SHA256 = "<sha256>"
.\ops\install\install.ps1
bluey on
# pill appears
bluey off
```

### W5 — Helper discovery

**Today:** `crates/cue-daemon/src/app.rs::discover_overlay_bin` has a
Windows branch looking at `native/windows/cue-overlay/build/bluey-overlay.exe`
(dev path) and falling back to siblings of the daemon executable for installed
mode. Same shape exists for `audio/system_capture.rs` and `stt/whisper/mod.rs`.

**Action:** verify the install path discovery works after `install.ps1`
extracts to `%LOCALAPPDATA%\Bluey\bin`. The daemon's
`current_exe()` plus sibling lookup should resolve all helpers.

### W6 — Process masquerading (deferred from W1.0)

**Today:** `crates/cue-stealth/src/windows.rs` exists but the Windows
masquerade mechanics are not as exercised as macOS. Test path:

```pwsh
bluey on --title "perf monitor"
# verify bluey-daemon.exe appears in Task Manager with the chosen process
# title (or accept that v0.1.x ships without on-Windows masquerade).
```

If the existing implementation works on the test machine, ship it. If it
does not, document the gap and ship without masquerade on Windows; mention
in the v0.1.x notes that Windows masquerading is v0.2 work.

### W7 — Smoke test on a clean Windows 10/11 box

**Acceptance:**
1. Wipe `%LOCALAPPDATA%\Programs\bluey`, `%LOCALAPPDATA%\bluey`, user PATH
   entry for bluey.
2. Install via PowerShell installer.
3. `bluey on` → pill appears.
4. Configure OpenAI key (`bluey set-stt-api-key openai sk-...` or the
   appropriate equivalent).
5. Drive a real cue request, verify streaming works end-to-end.
6. `bluey off` → both processes exit.
7. Re-install over the existing version, verify upgrade works.

---

## 3. Pinned references for the Codex agent

- macOS pill source as the implementation reference:
  `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- Daemon protocol (commands + events):
  `crates/cue-core/src/overlay.rs`, `crates/cue-core/src/overlay_ipc.rs`
- Production reader gate (token + length + state):
  `crates/cue-daemon/src/app.rs::validate_and_decode_overlay_line`
- Helper discovery:
  `crates/cue-daemon/src/app.rs::discover_overlay_bin`
- Installer reference (macOS):
  `scripts/install.sh`
- macOS whisper integration as the reference shape:
  `native/macos/cue-whisper/Sources/CueWhisper/main.swift`
- Auto Router (will inform Windows runtime once it dispatches):
  `crates/cue-router/`, `docs/AUTO-ROUTING-USP.md`

## 4. Pipeline expectations for any Windows commit

```pwsh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --target x86_64-pc-windows-msvc --release
cargo test --target x86_64-pc-windows-msvc
make package-windows-x86_64
.\ops\install\install.ps1
bluey on; bluey off
```

Existing macOS pipeline must stay green. Windows changes must not regress
the macOS Universal artifact.

## 5. Handoff back to Kiro

When Windows reaches the W7 acceptance criteria, write
`docs/work/PHASE-3-WINDOWS-IMPL-FROM-CODEX.md` with:

- Per-item verdict (W1–W7).
- Final pipeline output.
- Artifact path on the Tailscale Windows machine + sha256.
- Any deviations from this brief and the rationale.
- Checklist of clean-Windows test results.

Kiro then folds the artifact + docs into `dist/` on uno + tags `v0.1.x` GA.
