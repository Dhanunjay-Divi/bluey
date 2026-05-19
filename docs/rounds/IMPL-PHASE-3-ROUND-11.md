# IMPL — Phase 3 Round 11 (R10 Codex Fixes + Overlay Injection Hardening)

**Branch**: `feat/phase-3-round-11`
**Base**: `feat/phase-3-round-10` tip (`cf1ae9f`, 299 tests)
**Tip**: `5335b12` (11 commits ahead of R10, 331 tests)

## Scope

**Two themes: fixing 4 R10 codex blockers + 7 overlay injection hardening requirements.**

Round 11 addresses preemptive fixes for R10 review gaps (streaming wiring, obfstr coverage, SwiftWhisper pin, PCM alignment) and implements the full overlay security hardening suite: production binary override gate, path verification, IPC session token handshake, event state machine with UI-state allowlist, safe JSON parsing for Windows, field length limits, and prompt-injection security tests.

**Does:**

1. **End-to-end UI streaming** — specialized LLMs gain `run_streaming(callback)` method; `request_cue` and `auto_recap` Tauri commands generate `response_id` up-front, emit `cue_response_chunk` per chunk, then emit final `cue_response`.
2. **obfstr on streaming auth headers** — `complete_stream()` paths in OpenAI + Anthropic now obfuscate `Authorization`, `Bearer`, `x-api-key`, `anthropic-version`, `2023-06-01`.
3. **SwiftWhisper exact 1.2.0 pin** — `.exact("1.2.0")` in Package.swift + updated Package.resolved.
4. **PCM16 alignment-safe decode** — `loadUnaligned(fromByteOffset:as:)` replaces `bindMemory`.
5. **Production overlay-bin override gate** — `BLUEY_OVERLAY_BIN`/`CUE_OVERLAY_BIN` env vars ignored in release builds unless `BLUEY_DEV_OVERLAY=1` is also set.
6. **Overlay binary path verification** — canonical path must resolve inside the app install directory before spawn.
7. **IPC session token handshake** — 64-hex-char random token generated at daemon startup, passed to overlay via `BLUEY_OVERLAY_SESSION_TOKEN` env var, validated on every incoming IPC event.
8. **Event state machine + UI-state allowlist** — `OverlayUiState` enum (Idle/AttachOpen/InstructionsOpen) with `OverlayEventKind::is_allowed_in()` gating which events are valid in which state.
9. **Safe Windows JSON parsing** — replaced `strstr`-based type extraction with `json_type_extract.h` hand-rolled safe top-level `"type"` field extractor.
10. **Field length limits** — question (4 KB), instructions (16 KB), path (1 KB/entry), paths[] (16 max), error (4 KB), text (64 KB), line (128 KB).
11. **Prompt-injection security tests** — 8 integration tests proving transcript text containing `"type":"ask_requested"` cannot trigger overlay commands.

**Does NOT:**

- Enable SHA-256 hash verification of overlay binary (scaffold present, hash uncommitted).
- Protect token from `/proc` readers (defense-in-depth, not strong auth).
- Terminate process on Windows debug attach (watchdog logs only).
- Resolve symlinks/relative paths in AttachFiles paths (state-machine gates Idle; full resolution deferred).
- Stream from daemon-side auto_recap (no AppHandle in daemon process).
- Support Anthropic/Ollama in env-var LLM build (OpenAI only).

## Commits (11, chronological bottom → top)

| # | Hash | Title | Theme |
|---|------|-------|-------|
| 1 | `6ff8a14` | `fix(daemon): emit cue_response_chunk while LLM stream is active [P3.R11]` | R10 fix |
| 2 | `9f23c18` | `fix(security): obfstr on streaming auth header names + base URLs [P3.R11]` | R10 fix |
| 3 | `cbaeb51` | `fix(whisper): pin SwiftWhisper to exact 1.2.0 [P3.R10 fix]` | R10 fix |
| 4 | `cadd362` | `fix(whisper): use loadUnaligned for PCM16 decode (alignment-safe) [P3.R10 fix]` | R10 fix |
| 5 | `18c2db4` | `fix(overlay): production builds ignore BLUEY_OVERLAY_BIN override unless BLUEY_DEV_OVERLAY=1 [P3.R11 hardening]` | Overlay hardening |
| 6 | `3df1b31` | `feat(overlay): IPC session token handshake - wire protocol and stubs [P3.R11 hardening]` | Overlay hardening |
| 7 | `c686cff` | `feat(overlay): token handshake in native overlays + integration tests [P3.R11 hardening]` | Overlay hardening |
| 8 | `15ad835` | `fix(overlay): replace strstr JSON parsing with safe type extractor [P3.R11 hardening]` | Overlay hardening |
| 9 | `a2173ab` | `feat(overlay): event state machine + field length limits` | Overlay hardening |
| 10 | `351f99d` | `test(overlay): prompt-injection and state-machine security tests` | Overlay hardening |
| 11 | `5335b12` | `chore(p3r11): reconcile C+D overlay merges (OverlayEvent + ui_state plumbing + clippy allows)` | Reconciliation |

## Files Created / Modified

### cue-daemon (streaming fix + overlay IPC)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/llm/answer.rs` | Modified | `run_streaming(callback)` method |
| `crates/cue-daemon/src/llm/recap.rs` | Modified | `run_streaming(callback)` method |
| `crates/cue-daemon/src/llm/suggest.rs` | Modified | `run_streaming(callback)` method |
| `crates/cue-daemon/src/app.rs` | Modified | Token generation, overlay spawn with env var, event validation |

### cue-dashboard (streaming commands)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/src/commands.rs` | Modified | `request_cue` + `auto_recap` emit `cue_response_chunk` per chunk |

### cue-llm (obfstr streaming)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-llm/src/openai.rs` | Modified | obfstr on auth headers in `complete_stream()` |
| `crates/cue-llm/src/anthropic.rs` | Modified | obfstr on auth headers in `complete_stream()` |

### cue-core (overlay IPC hardening)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-core/src/overlay_ipc.rs` | Modified | `OverlayEvent` struct, token validation, state machine, length limits |
| `crates/cue-core/src/overlay_ipc/overlay_stub.rs` | Modified | Stub overlay with token env read |

### native overlays (token + safe JSON)

| File | Action | Purpose |
|------|--------|---------|
| `native/macos/cue-overlay/Sources/main.swift` | Modified | Read `BLUEY_OVERLAY_SESSION_TOKEN`, include in IPC messages |
| `native/windows/cue-overlay/main.c` | Modified | Token env read + `json_type_extract.h` safe parser |
| `native/windows/cue-overlay/json_type_extract.h` | Created | Hand-rolled safe top-level `"type"` field extractor |

### native whisper (R10 fixes)

| File | Action | Purpose |
|------|--------|---------|
| `native/macos/cue-whisper/Package.swift` | Modified | `.exact("1.2.0")` pin |
| `native/macos/cue-whisper/Package.resolved` | Modified | Updated lockfile |
| `native/macos/cue-whisper/Sources/main.swift` | Modified | `loadUnaligned` PCM decode |

### Tests

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/tests/streaming_chunk_emit.rs` | Created | 5 tests: chunk emission, callback, response_id |
| `crates/cue-core/tests/overlay_security.rs` | Created | 8 prompt-injection tests |
| `crates/cue-core/tests/overlay_state_machine.rs` | Created | 7 state-machine transition tests |
| `crates/cue-core/tests/overlay_token.rs` | Created | 5 token handshake tests |
| `crates/cue-core/tests/overlay_length_limits.rs` | Created | 7 field length limit tests |

## Build & Test

```bash
cargo fmt --all --check                              # ✅ pass
cargo clippy --all-targets -- -D warnings            # ✅ pass (2 explicit allows: collapsible_if, single_match)
cargo build --all-targets                            # ✅ pass
cargo test --all-targets                             # ✅ 331 pass, 10 ignored
cd crates/cue-dashboard/ui && npm run build          # ✅ pass
git -P diff --check feat/phase-3-round-10..HEAD      # ✅ clean
strings target/release/cue-dashboard | grep -cE '(Authorization|x-api-key|anthropic-version)'  # ✅ 0
```

### Test count progression

| Tier | R10 final | R11 final | Δ |
|------|-----------|-----------|---|
| Streaming chunk emission | 0 | 5 | +5 |
| Overlay token handshake | 0 | 5 | +5 |
| Overlay state machine | 0 | 7 | +7 |
| Overlay length limits | 0 | 7 | +7 |
| Overlay prompt-injection security | 0 | 8 | +8 |
| Previous (carried forward) | 299 | 299 | — |
| **Total running** | **299** | **331** | **+32** |

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| R11 scope changed from sqlite-vec + native overlay passthrough to overlay hardening | User reprioritized: security hardening before feature work |
| Two clippy allows added (`collapsible_if`, `single_match`) | Reconciliation of parallel subagent work; readability preserved over mechanical collapse |
| SHA-256 hash verification scaffolded but not enabled | Requires committing expected hash; deferred to avoid build-time complexity |

## Reconciliation Note (commit `5335b12`)

Parallel subagents C and D both modified `overlay_ipc.rs` and `overlay_stub.rs`. Resolution:
- D's state-machine + length limits kept as base (more structural changes)
- C's `OverlayEvent` struct + token validation + `ui_state` plumbing re-added on top
- Two `#[allow(clippy::...)]` annotations added where parallel code created patterns clippy flagged

## Known Follow-ups

- SHA-256 hash verification: enable by committing expected hash + uncommenting check
- Token rotation on session boundaries (currently static per daemon lifetime)
- Windows overlay: real anti-debug with process termination (too aggressive for v0.1)
- Path canonicalization for AttachFiles entries (symlink/relative resolution)
- Daemon-side streaming (requires IPC redesign or AppHandle forwarding)
- Anthropic/Ollama env-var LLM provider support

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] R10 fixes address the 4 identified blockers correctly
- [ ] Overlay token is cryptographically random (64 hex chars = 256 bits)
- [ ] State machine transitions are exhaustive and correct
- [ ] Length limits match documented values
- [ ] Prompt-injection tests prove isolation between transcript content and IPC commands
- [ ] Safe JSON parser handles malformed input without buffer overflows
- [ ] Production override gate cannot be bypassed without `BLUEY_DEV_OVERLAY=1`
- [ ] Tests cover acceptance criteria
- [ ] Code style matches CLAUDE.md rules
- [ ] No TODOs without linked task IDs
