# v0.1.0 — release notes

**Released:** 2026-05-17 (GA)<br>
**Predecessor:** v0.1.0-alpha (2026-05-16, internal alpha cut)<br>
**Tag:** `v0.1.0` on `main`<br>
**Codex chain review:** 🟢 ACCEPT (R7-fix-3 / R8 / R9 / R10 / R11 / R12)

## What's in this release

This is the first general-availability cut of Bluey/Cue. It bundles the work
from Phase 3 Rounds 7 through 12 into a single shippable artifact.

The internal alpha (`v0.1.0-alpha`, 2026-05-16) and this GA share the same
implementation; v0.1.0 GA differs only in (a) cleared Round 12 carry-over
nits from the R11 final review and (b) corrected release/support-matrix
documentation per Round 12 review feedback.

## Supported platforms (v0.1.0)

| Platform | Status | Notes |
|---|---|---|
| macOS arm64 (Apple Silicon, macOS 13+) | ✅ shipped | The only artifact built and smoke-tested for this release. |
| macOS x86_64 (Intel) | ❌ not shipped | Cross-compile + clean-machine smoke test scheduled for Round 13 (R13.5). |
| Linux x86_64 | ❌ not shipped | Cross-compile + audio-capture validation scheduled for Round 13. |
| Windows x86_64 | ❌ not shipped | Codebase has Windows overlay + anti-debug helpers, but the Windows whisper.cpp port is still a stub (R12.5 / R13.4). End-to-end testing on a clean Windows machine is also pending. |

The codebase contains code paths for all four platforms. v0.1.0 only
**ships and supports** macOS arm64; the other platforms are works in
progress and should not be installed from this release.

## What's in this release

### Capture
- Real `whisper.cpp` transcription on macOS via SwiftPM (R10).
- System and microphone audio capture pipelines.
- LLM streaming for Anthropic, OpenAI, and Ollama with delta-chunk semantics; the dashboard renders responses incrementally as they arrive.

### Native overlays
- macOS overlay (Swift, capture-excluded `NSWindow` so screen recordings exclude the pill).
- Windows overlay code (C, `WS_EX_NOREDIRECTIONBITMAP`) — present in source but not shipped in v0.1.0.
- IPC handshake: per-session 256-bit token (R12.3, OS entropy via `getrandom`), length-capped event fields, single `Arc<Mutex<OverlayUiState>>` state machine that gates inner-form events (R12.2).

### Stealth
- Process masquerading: Terminal, Settings, or Activity Monitor identity at launch.
- Anti-debug helpers in source: `PT_DENY_ATTACH` (macOS), `IsDebuggerPresent` (Windows), `TracerPid` check (Linux); only macOS path is enabled in the v0.1.0 binary.
- Compile-time `obfstr` on API endpoint URLs and auth header names.

### Dashboard (developer tool, NOT shipped)
- Tauri/React surface for cue history, settings, and live transcript view.
- Settings route with keyring-backed STT API key storage; non-secret settings rejected for secret-shaped keys with explicit error.
- Streaming response reducer hardened (R12.1) so non-empty `finished:true` chunks are preserved instead of dropped.

The dashboard is built as part of the development workflow but is not
included in the v0.1.0 distribution tarball; it will be reintroduced once
it has been bundled and signed for end-user distribution.

## Code signing

v0.1.0 is **not** code-signed or notarized. Distribution is terminal-only
(curl + tar, or a brew tap consuming the same tarball). Signing /
notarization are deferred to a later release; per `INSTALL.md`, browser
downloads may need `xattr -d com.apple.quarantine` remediation.

## Known gaps (Round 13)

| Item | Source | Status |
|---|---|---|
| Reset overlay UI state on cancel/error (not just submit) | Codex R12 review nit | R13.1 — non-blocking |
| `generate_session_token` returns `Result` instead of `expect()`-panic | Codex R12 review nit | R13.2 — non-blocking |
| RAG vector search (linear scan today) | Codex R11 final review | R13.3 — sqlite-vec/ANN, ~1 day |
| Windows real whisper.cpp | R10 deferral | R13.4 — blocked on Windows test bench |
| Cross-platform matrix (macOS x86_64, Linux, Windows) | Codex R12 review | R13.5 — schedule per priority |
| Telemetry counter for overlay reader rejections | Codex R12 review | R13.6 — gated on telemetry sink + privacy review |
| Tested `scripts/install.sh` | Codex R12 review | R13.7 — only if `curl|sh` becomes the primary install path |

See `docs/work/PHASE-3-ROUND-13-PLAN.md`.

## Verification at release time

- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets --release` ✅
- `cargo test --all-targets` ✅ **361 tests, 0 failures**
- `cd crates/cue-dashboard/ui && npm test` ✅ **13 vitest tests** (new in R12)
- `cd crates/cue-dashboard/ui && npm run build` ✅
- `swift build -c release --package-path native/macos/cue-overlay` ✅
- `swift build -c release --package-path native/macos/cue-whisper` ✅
- `bash scripts/smoke-test.sh` ✅ (daemon + overlay + transcript + instructions + context + memory + audio + AI routing + cloud + ask + action-items + recap + archive)
- `git -P diff --check main` ✅

## macOS arm64 artifact

| File | Notes |
|---|---|
| `bluey-0.1.0-macos-arm64.tar.gz` | the shipped tarball |
| `bluey-0.1.0-macos-arm64.tar.gz.sha256` | per-archive checksum |
| `SHA256SUMS` (inside the tarball) | per-binary checksums |

## Test count progression

```
Phase 0 / Phase 1 / Phase 2:        <100 tests
Phase 3 Round 1-6 (main pre-R7):     201 tests
Phase 3 Round 7:                     213 tests
Phase 3 Round 8:                     224 tests
Phase 3 Round 9:                     284 tests
Phase 3 Round 10:                    299 tests
Phase 3 Round 11 (initial):          331 tests
Phase 3 Round 11 fix wave:           345 tests (+14 production-path)
Phase 3 Round 11 recheck #2:         351 tests (+6 Windows + state)
Phase 3 Round 11 + R8 nits:          354 tests (+3 char-safe masking)
v0.1.0-alpha:                        354 tests
Phase 3 Round 12 R12.3:              357 tests (+3 token tests)
Phase 3 Round 12 R12.1:              357 tests + 13 vitest tests (new)
Phase 3 Round 12 R12.2:              361 tests + 13 vitest tests
v0.1.0 GA:                           361 tests + 13 vitest tests
```
