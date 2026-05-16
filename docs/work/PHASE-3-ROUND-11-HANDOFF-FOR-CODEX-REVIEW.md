# Phase 3 Round 11 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-11`
**Base**: `feat/phase-3-round-10` tip (`cf1ae9f`, 299 tests)
**Authors**: kiro (4 parallel subagents with worktree isolation + manual reconciliation), uno (user — oversight)

## Scope

Round 11 of Phase 3. Two themes: fixing 4 R10 codex blockers and implementing 7 overlay injection hardening requirements. Delivered via 4 parallel subagents (A: streaming fix, B: obfstr+whisper fixes, C: token handshake, D: state machine + length limits) with a final reconciliation commit for C+D conflicts.

### Commits (11 ahead of R10)

```
5335b12 chore(p3r11): reconcile C+D overlay merges (OverlayEvent + ui_state plumbing + clippy allows)
a2173ab feat(overlay): event state machine + field length limits
351f99d test(overlay): prompt-injection and state-machine security tests
15ad835 fix(overlay): replace strstr JSON parsing with safe type extractor
c686cff feat(overlay): token handshake in native overlays + integration tests [P3.R11 hardening]
3df1b31 feat(overlay): IPC session token handshake - wire protocol and stubs [P3.R11 hardening]
18c2db4 fix(overlay): production builds ignore BLUEY_OVERLAY_BIN override unless BLUEY_DEV_OVERLAY=1 [P3.R11 hardening]
cadd362 fix(whisper): use loadUnaligned for PCM16 decode (alignment-safe) [P3.R10 fix]
cbaeb51 fix(whisper): pin SwiftWhisper to exact 1.2.0 [P3.R10 fix]
9f23c18 fix(security): obfstr on streaming auth header names + base URLs [P3.R11]
6ff8a14 fix(daemon): emit cue_response_chunk while LLM stream is active [P3.R11]
```

### Commit roles

| Hash | Role | Theme |
|------|------|-------|
| `6ff8a14` | R10 fix: wire streaming chunks to UI | R10 blocker |
| `9f23c18` | R10 fix: obfstr streaming auth headers | R10 blocker |
| `cbaeb51` | R10 fix: exact SwiftWhisper pin | R10 blocker |
| `cadd362` | R10 fix: alignment-safe PCM decode | R10 blocker |
| `18c2db4` | Overlay hardening: prod override gate | Security |
| `3df1b31` | Overlay hardening: token wire protocol | Security |
| `c686cff` | Overlay hardening: native token impl + tests | Security |
| `15ad835` | Overlay hardening: safe JSON parser | Security |
| `a2173ab` | Overlay hardening: state machine + limits | Security |
| `351f99d` | Overlay hardening: prompt-injection tests | Security |
| `5335b12` | Reconciliation: merge parallel work | Housekeeping |

## Verification — ALL GREEN (7 checks)

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 331 pass, 10 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-10..HEAD      ✅ clean
strings target/release/cue-dashboard | grep -cE '(Authorization|x-api-key|anthropic-version)'  ✅ 0 matches
```

### Test count delta

| Category | R10 | R11 | Δ |
|----------|-----|-----|---|
| Streaming chunk emission | 0 | 5 | +5 |
| Overlay token handshake | 0 | 5 | +5 |
| Overlay state machine | 0 | 7 | +7 |
| Overlay length limits | 0 | 7 | +7 |
| Overlay prompt-injection | 0 | 8 | +8 |
| Carried forward | 299 | 299 | — |
| **Total** | **299** | **331** | **+32** |

## Architecture Diagram — Round 11 Additions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│              cue_response_chunk Streaming Flow [NEW in R11]                   │
│                                                                             │
│  request_cue / auto_recap (Tauri command, has AppHandle)                    │
│       │                                                                     │
│       ├── generate response_id (UUID)                                       │
│       ├── AnswerLlm::run_streaming(|chunk| {                               │
│       │       app.emit_all("cue_response_chunk", {response_id, text, false})│
│       │   })                                                                │
│       ├── on stream end:                                                    │
│       │       app.emit_all("cue_response_chunk", {response_id, "", true})   │
│       │       app.emit_all("cue_response", full_response)                   │
│       └── persist CueResponse to DB                                         │
│                                                                             │
│  Dashboard (React):                                                         │
│    listen("cue_response_chunk") → Map<id, accumulated> → typing indicator  │
│    listen("cue_response") → final card, remove from inflight               │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│              Overlay Token Handshake [NEW in R11]                             │
│                                                                             │
│  Daemon startup:                                                            │
│    token = rand::thread_rng().gen::<[u8;32]>() → hex::encode (64 chars)    │
│       │                                                                     │
│       ├── Spawn overlay process with env:                                   │
│       │     BLUEY_OVERLAY_SESSION_TOKEN=<token>                             │
│       │                                                                     │
│       └── Store token in OverlayManager                                     │
│                                                                             │
│  Native overlay (macOS Swift / Windows C):                                  │
│    read BLUEY_OVERLAY_SESSION_TOKEN from env on startup                     │
│    include "session_token":"<token>" in every IPC JSON message              │
│                                                                             │
│  Daemon IPC receiver:                                                       │
│    parse incoming JSON → extract session_token field                        │
│    if token != expected → drop event, log warning                           │
│    if token matches → proceed to state machine                              │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│              State-Machine Event Filter [NEW in R11]                          │
│                                                                             │
│  OverlayUiState enum:                                                       │
│    Idle              — overlay visible, no panel open                       │
│    AttachOpen        — file-attach panel showing                            │
│    InstructionsOpen  — instructions editor showing                          │
│                                                                             │
│  OverlayEventKind::is_allowed_in(state) → bool:                            │
│    AskRequested      → only in Idle                                         │
│    AttachFiles       → only in AttachOpen                                   │
│    SetInstructions   → only in InstructionsOpen                             │
│    Dismiss           → any state (always allowed)                           │
│    Error             → any state (always allowed)                           │
│                                                                             │
│  Flow: token OK → kind.is_allowed_in(current_state) → process or drop      │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│              Length-Limit Boundary [NEW in R11]                               │
│                                                                             │
│  Field limits (enforced at IPC parse time):                                 │
│    question     : 4,096 bytes (4 KB)                                        │
│    instructions : 16,384 bytes (16 KB)                                      │
│    path (each)  : 1,024 bytes (1 KB)                                        │
│    paths[]      : 16 entries max                                            │
│    error        : 4,096 bytes (4 KB)                                        │
│    text         : 65,536 bytes (64 KB)                                      │
│    line         : 131,072 bytes (128 KB)                                    │
│                                                                             │
│  Exceeding any limit → event dropped with OverlayValidationError::TooLong  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Per-Feature-Area Review Checklist

### Theme A: R10 Codex Fixes

#### Streaming chunk emission (`6ff8a14`)

- [ ] `run_streaming(callback)` on AnswerLlm, RecapLlm, WhatToAnswerLlm
- [ ] `response_id` generated as UUID before stream starts
- [ ] Callback emits `cue_response_chunk` with `{ response_id, text, finished }` payload
- [ ] Empty chunks (text="" && !finished) are skipped (no spurious events)
- [ ] Final chunk has `finished: true`
- [ ] `cue_response` event emitted after stream completes with full accumulated text
- [ ] 5 integration tests cover: multi-chunk emission, single-chunk fallback, error propagation, response_id consistency, finished flag

#### obfstr streaming headers (`9f23c18`)

- [ ] `obfstr!("Authorization")` in OpenAI `complete_stream()`
- [ ] `obfstr!("Bearer")` in OpenAI `complete_stream()`
- [ ] `obfstr!("x-api-key")` in Anthropic `complete_stream()`
- [ ] `obfstr!("anthropic-version")` in Anthropic `complete_stream()`
- [ ] `obfstr!("2023-06-01")` in Anthropic `complete_stream()`
- [ ] `strings` binary grep returns 0 for all covered strings

#### SwiftWhisper pin (`cbaeb51`)

- [ ] `Package.swift` uses `.exact("1.2.0")` (not `from:` or `.upToNextMajor`)
- [ ] `Package.resolved` committed and consistent

#### PCM alignment (`cadd362`)

- [ ] `loadUnaligned(fromByteOffset:as: Int16.self)` used instead of `bindMemory`
- [ ] Loop iterates by stride 2 over raw bytes
- [ ] Odd-length buffer handled (last byte ignored, no crash)
- [ ] Float conversion unchanged: `Float(sample) / 32768.0`

### Theme B: Overlay Injection Hardening

#### Production override gate (`18c2db4`)

- [ ] `BLUEY_OVERLAY_BIN` / `CUE_OVERLAY_BIN` env vars checked
- [ ] In release builds (`!cfg!(debug_assertions)`): override ignored unless `BLUEY_DEV_OVERLAY=1`
- [ ] In debug builds: override always honored (developer convenience)
- [ ] Canonical path verification: resolved path must be inside app install directory
- [ ] `OverlayVerifyError` enum with `OutsideInstallDir` variant
- [ ] `HashMismatch` variant scaffolded but check commented out

#### IPC session token (`3df1b31`, `c686cff`)

- [ ] Token generated with `rand::thread_rng().gen::<[u8;32]>()` → 64 hex chars
- [ ] Token passed to overlay process via `BLUEY_OVERLAY_SESSION_TOKEN` env var
- [ ] macOS Swift overlay reads token from `ProcessInfo.processInfo.environment`
- [ ] Windows C overlay reads token from `getenv("BLUEY_OVERLAY_SESSION_TOKEN")`
- [ ] Every IPC JSON message includes `"session_token":"<token>"` field
- [ ] Daemon extracts and validates token before processing event
- [ ] Mismatch → event dropped + `log::warn!` (no crash, no response to attacker)
- [ ] 5 integration tests: valid token accepted, missing token rejected, wrong token rejected, empty token rejected, token not leaked in error messages

#### Safe JSON parsing (`15ad835`)

- [ ] `json_type_extract.h` created in `native/windows/cue-overlay/`
- [ ] Extracts top-level `"type"` field value without full JSON parse
- [ ] Handles: missing field, non-string value, escaped quotes in value, nested objects
- [ ] No buffer overflow: bounded scan with explicit length checks
- [ ] Replaces all `strstr(buf, "\"type\"")` patterns in `main.c`

#### Event state machine (`a2173ab`)

- [ ] `OverlayUiState` enum: `Idle`, `AttachOpen`, `InstructionsOpen`
- [ ] `OverlayEventKind` enum covers all event types
- [ ] `is_allowed_in(state) -> bool` method with correct mapping
- [ ] `Dismiss` and `Error` allowed in any state
- [ ] State transitions: `AskRequested` → Idle only; `AttachFiles` → AttachOpen only; `SetInstructions` → InstructionsOpen only
- [ ] 7 state-machine tests: valid transitions, invalid transitions, dismiss-from-any, error-from-any

#### Field length limits (`a2173ab`)

- [ ] Constants defined: `MAX_QUESTION_LEN = 4096`, `MAX_INSTRUCTIONS_LEN = 16384`, etc.
- [ ] Validation runs after token check, before state-machine check
- [ ] `OverlayValidationError::TooLong { field, max, actual }` variant
- [ ] Exceeding limit → event dropped (not truncated)
- [ ] 7 tests: at-limit accepted, over-limit rejected (one per field type), paths count limit

#### Prompt-injection tests (`351f99d`)

- [ ] 8 tests proving transcript text cannot trigger overlay commands
- [ ] Test injects `{"type":"ask_requested","question":"..."}` as transcript segment text
- [ ] Verifies: no `AskRequested` event generated from transcript content
- [ ] Verifies: IPC parser only accepts events from validated overlay process (token-gated)
- [ ] Verifies: JSON-in-JSON nesting doesn't confuse type extractor
- [ ] Verifies: Unicode homoglyphs in "type" field don't bypass parser
- [ ] Verifies: extremely long type values don't cause buffer issues
- [ ] Verifies: null bytes in JSON don't truncate parsing

#### Reconciliation (`5335b12`)

- [ ] Only merges parallel subagent work — no new logic
- [ ] `#[allow(clippy::collapsible_if)]` on token validation fn (readability)
- [ ] `#[allow(clippy::single_match)]` in overlay_stub.rs (future match arms planned)
- [ ] All tests pass after reconciliation

## Explicit Deferrals (NOT in Round 11)

1. **SHA-256 hash verification of overlay binary** — `OverlayVerifyError::HashMismatch` scaffolded; enabling requires committing expected hash.
2. **Token via env var is defense-in-depth** — a process with /proc access could read it. Not strong auth.
3. **Windows real anti-debug process termination** — watchdog logs only; too aggressive for v0.1.
4. **Path canonicalization for AttachFiles** — state-machine gates Idle but full symlink/relative resolution deferred.
5. **Daemon-side auto_recap streaming** — uses non-streaming `run()` since daemon has no AppHandle.
6. **AnswerLLM multi-provider** — env-var build supports OpenAI only; Anthropic/Ollama from settings deferred.
7. **sqlite-vec swap** — reprioritized out of R11 (was original R11 plan).
8. **Native overlay passthrough** — reprioritized out of R11.

## Verdict Request

Codex: review the 11 commits (2 themes: R10 fixes + overlay hardening). Write `docs/work/REVIEW-PHASE-3-ROUND-11.md` with verdict.

- 🟢 **ACCEPT** → merge R7→R11 chain to main, ship v0.1 alpha
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 12
- 🔴 **REQUEST CHANGES** → kiro writes fix doc and re-hands
