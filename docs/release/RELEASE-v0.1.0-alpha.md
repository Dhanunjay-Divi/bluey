# v0.1.0-alpha — release notes

**Released:** 2026-05-16
**Tag:** `v0.1.0-alpha` on `main`
**Merge commit:** `1f8e626 Merge: Phase 3 Rounds 7-11 (v0.1 alpha)`
**Codex final chain review:** 🟢 ACCEPT (R7-fix-3 / R8 / R9 / R10 / R11)

## What's in this release

This is the first reviewable cut of Bluey/Cue. It bundles the work from Phase 3 Rounds 7 through 11 into a single shippable artifact for internal alpha testing.

### Capture
- Real `whisper.cpp` transcription on macOS via SwiftPM (R10).
- System and microphone audio capture pipelines.
- LLM streaming for Anthropic, OpenAI, and Ollama with delta-chunk semantics; the dashboard renders responses incrementally as they arrive.

### Native overlays
- macOS overlay (Swift, capture-excluded `NSWindow` so screen-recordings exclude the pill).
- Windows overlay (C, `WS_EX_NOREDIRECTIONBITMAP`).
- IPC handshake: per-session token, length-capped event fields, `OverlayUiState` state machine that gates inner-form events.

### Stealth
- Process masquerading: Terminal, Settings, or Activity Monitor identity at launch.
- Anti-debug helpers: `PT_DENY_ATTACH` (macOS), `IsDebuggerPresent` (Windows), `TracerPid` check (Linux).
- Compile-time `obfstr` on API endpoint URLs and auth header names.

### Dashboard
- Tauri/React surface for cue history, settings, and live transcript view.
- Settings route with keyring-backed STT API key storage; non-secret settings rejected from secret-shaped keys with explicit error.

## Known gaps (Round 12)

| Item | Status |
|---|---|
| Responses.tsx hardening for non-empty finished chunks | Reducer test pending (R12.1) |
| `overlay_ui_state` full plumbing | Architecturally split-brained today; safe but messy (R12.2) |
| Overlay session token | 244 random bits via 2x UUIDv4; should be true 256-bit OsRng (R12.3) |
| RAG vector search | Linear scan; migrate to sqlite-vec/ANN before beta (R12.4) |
| Windows real whisper.cpp | Stub only; macOS has the real impl (R12.5) |

See `docs/work/PHASE-3-ROUND-12-PLAN.md` for the full plan.

## Verification at release time

- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets --release` ✅
- `cargo test --all-targets` ✅ **354 tests, 0 failures** (3 consecutive runs)
- `cd crates/cue-dashboard/ui && npm run build` ✅
- `swift build -c release --package-path native/macos/cue-overlay` ✅
- `swift build -c release --package-path native/macos/cue-whisper` ✅
- `bash scripts/smoke-test.sh` ✅ (daemon + overlay + transcript + instructions + context + memory + audio + AI routing + cloud + ask + action-items + recap + archive)
- `git -P diff --check main` ✅

## macOS arm64 artifact

| File | Size | sha256 |
|---|---|---|
| `bluey-v0.1.0-alpha-macos-arm64.tar.gz` | 12 MB | `a03d4d9aefa86dd11dc0bc276511fe6a5a615be1707693f3f611948ce5943036` |

Per-binary checksums in `dist/bluey-macos-arm64/SHA256SUMS`.

## Test count progression

```
Phase 0 / Phase 1 / Phase 2: <100 tests
Phase 3 Round 1-6 (main pre-R7): 201 tests
Phase 3 Round 7:                  213 tests
Phase 3 Round 8:                  224 tests
Phase 3 Round 9:                  284 tests
Phase 3 Round 10:                 299 tests
Phase 3 Round 11 (initial):       331 tests
Phase 3 Round 11 fix wave:        345 tests (+14 production-path)
Phase 3 Round 11 recheck #2:      351 tests (+6 Windows + state)
Phase 3 Round 11 + R8 nits:       354 tests (+3 char-safe masking)
v0.1.0-alpha:                     354 tests
```
