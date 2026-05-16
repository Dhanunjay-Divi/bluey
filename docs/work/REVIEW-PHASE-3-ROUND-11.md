# REVIEW: Phase 3 Round 11 — R10 Fixes + Overlay Injection Hardening

**Commit range:** `cf1ae9f..030a63c`
**Reviewer:** Codex
**Date:** 2026-05-16

## Per-Task Review

### R11.A — R10 Streaming Fixes

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/llm/answer.rs`, `crates/cue-daemon/src/llm/recap.rs`, `crates/cue-daemon/src/llm/suggest.rs`, `crates/cue-dashboard/ui/src/routes/Responses.tsx`, `crates/cue-llm/src/openai.rs`, `crates/cue-llm/src/anthropic.rs`, `native/macos/cue-whisper/*` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The live streaming UI still duplicates streamed text. The daemon callbacks emit cumulative text (`acc`) on every chunk in `crates/cue-daemon/src/llm/answer.rs:36-43` (same pattern in recap/suggest), but the dashboard appends `chunk.partial_text` to the existing in-flight card in `crates/cue-dashboard/ui/src/routes/Responses.tsx:53-65`. A stream like `Hello`, `Hello world`, `Hello world!` renders as `HelloHello worldHello world!`. Fix either side of the contract: emit deltas and keep appending, or emit cumulative text and replace the existing in-flight text.
- 🟡 If a streaming provider errors after partial chunks, the command returns `Err` without emitting a final/error `cue_response_chunk`. That can leave the dashboard typing state stuck until a later response overwrites it. Consider emitting a terminal chunk or a dedicated response-error event before returning the error.
- 🟢 The R10 provider-level fixes look good: streaming auth header names are obfuscated, SwiftWhisper is pinned to `.exact("1.2.0")`, and the macOS PCM16 decode now uses `loadUnaligned`.

---

### R11.B1 — Production Overlay Hardening Wiring

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/src/overlay.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The new hardened overlay manager is not on the production daemon path. Startup still calls the legacy `spawn_overlay(...)` in `crates/cue-daemon/src/app.rs:486-487`, and the restart path calls the same legacy function in `crates/cue-daemon/src/app.rs:1063-1068`. The new `NativeOverlayHandle`, `resolve_overlay_path`, `verify_overlay_binary`, token handshake, state machine, and field limits are therefore not active for the actual app.
- 🔴 The legacy production path still honors `BLUEY_OVERLAY_BIN` / `CUE_OVERLAY_BIN` directly without the `BLUEY_DEV_OVERLAY=1` gate in `crates/cue-daemon/src/app.rs:4804-4808`. That leaves the original overlay-binary injection path open despite the new gated resolver existing in `overlay.rs`.
- 🔴 The legacy stdout reader in `crates/cue-daemon/src/app.rs:4822-4833` deserializes `cue_core::OverlayEvent` directly and forwards it with no session token validation, no state validation, and no field length validation.

---

### R11.B2 — Overlay Token Handshake

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/overlay.rs`, `native/macos/cue-overlay/Sources/cue-overlay/main.swift`, `native/windows/cue-overlay/main.c` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The Windows native overlay does not include the session token in emitted commands. `native/windows/cue-overlay/main.c:179-192` still prints tokenless events such as `{"type":"ask_requested",...}`. The macOS overlay has token plumbing, but Windows parity is missing and the docs currently claim both native overlays implement it.
- 🟡 `generate_session_token()` in `crates/cue-daemon/src/overlay.rs:146-151` concatenates two UUID v4 values. That is still strong enough for this use, but it is not literally a random 32-byte token as the comment/docs claim because UUID v4 has fixed version/variant bits. Either adjust the claim or use 32 random bytes.
- 🟡 The docs use the term `session_token`, while the Rust/macOS envelope field is named `token`. This is minor, but the review handoff should describe the actual wire format exactly.

---

### R11.B3 — Event State Machine and Field Length Limits

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/overlay.rs`, `crates/cue-core/src/overlay_ipc.rs`, `crates/cue-daemon/tests/overlay_security_integration.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 Even inside the new `NativeOverlayHandle`, parsed overlay events are forwarded after token validation without applying `validate_command_lengths(...)` or `OverlayEventKind::is_allowed_in(...)`. The reader path in `crates/cue-daemon/src/overlay.rs:330-369` accepts `AskRequested`, `AttachFilesRequested`, and `InstructionsUpdated` directly once the token matches.
- 🔴 The state-machine tests are currently lower-level tests of `cue_core::overlay_ipc`; they do not prove the daemon receiver enforces the state machine. A malicious or compromised overlay process with a valid token can still send state-inappropriate commands through the new reader.
- 🟡 `OverlayEventKind::AskRequested` is allowed in every state in `crates/cue-core/src/overlay_ipc.rs:119-124`, and tests assert that in `crates/cue-core/src/overlay_ipc.rs:309-314`. That may be the desired product behavior, but it does not match the R11 handoff language that says ask requests are only allowed when idle.

---

### R11.C — Windows Safe JSON Type Extraction

| Field | Value |
|-------|-------|
| Files | `native/windows/cue-overlay/main.c` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The safe top-level JSON type extractor is a meaningful improvement over broad `strstr` checks for daemon-to-overlay messages.
- 🟡 This improvement only hardens inbound messages consumed by the Windows overlay. It does not harden overlay-to-daemon events until the production daemon path uses token/state/length validation.

## Cross-Task Findings

- The main cross-task issue is integration drift: R11 added a hardened overlay module and tests around it, but `app.rs` still uses the older overlay process implementation. The security story should be judged against the production path, not the isolated helper.
- The R10 streaming fix has the same shape: provider streaming tests pass, but the UI contract still mismatches daemon payload semantics, so the user-facing stream is not correct yet.

## Build & Test Verification

```bash
cargo fmt --all --check                              # ✅
cargo clippy --all-targets -- -D warnings            # ✅
cargo build --all-targets                            # ✅
cargo test --all-targets                             # ✅ 331 passed, 14 ignored locally
cd crates/cue-dashboard/ui && npm run build          # ✅
cargo test -p cue-daemon --test cue_streaming_integration
                                                       # ✅ 5 passed
cargo test -p cue-daemon --test overlay_security_integration --test overlay_pipe_integration
                                                       # ✅ 13 passed
cd native/macos/cue-overlay && swift build            # ✅
cd native/macos/cue-whisper && swift build            # ✅
cd native/macos/cue-audio && swift build              # ✅
git diff --check feat/phase-3-round-10..HEAD          # ✅
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved.

The individual tests are green, and several R10 fixes are good, but R11 does not yet close the overlay injection class in the running daemon. The hardened overlay implementation must be wired into production, and the streaming UI contract must be corrected before this can merge.

## Recheck 1 — Commit `2524154`

**Date:** 2026-05-16

**Verdict:** 🔴 **REQUEST CHANGES**

The fix wave addressed the largest integration gap: `app.rs::spawn_overlay()` now uses the gated resolver, verifies the overlay binary path, passes `BLUEY_OVERLAY_SESSION_TOKEN`, and validates overlay lines before forwarding. Streaming callbacks also now emit deltas instead of cumulative text.

Remaining blockers:

- 🔴 Windows `ask_requested` events are still tokenless. `native/windows/cue-overlay/main.c:205-209` correctly appends the token for simple events, but `emit_ask_event` in `native/windows/cue-overlay/main.c:217-220` writes the full JSON object and closes it without calling `emit_token_field()`. This means the Send button / Enter path on Windows will be rejected by the hardened daemon as `TokenMismatch`, so blocker 4 is only partially fixed.
- 🔴 The production state machine now rejects the request events that are supposed to open the relevant workflows. `validate_and_decode_overlay_line` gates `OverlayEvent::AttachRequested` and `OverlayEvent::InstructionsRequested` behind `AttachOpen` / `InstructionsOpen` in `crates/cue-daemon/src/app.rs:5051-5057`, but those events are the initial user requests handled by `handle_attach_requested` and `handle_instructions_requested` in `crates/cue-daemon/src/app.rs:1140-1147`. The Windows overlay emits them directly from normal idle button clicks in `native/windows/cue-overlay/main.c:1209` and `native/windows/cue-overlay/main.c:1223`, so Attach and Style are dropped in the default state. Keep `AttachFilesRequested` and `InstructionsUpdated` gated, but allow `AttachRequested` / `InstructionsRequested` in idle.
- 🟡 The daemon owns `overlay_ui_state`, but the initial spawn passes a separate `Arc<Mutex<Idle>>` into the reader in `crates/cue-daemon/src/app.rs:496-503`, and `overlay_ui_state` is not updated anywhere outside initialization/restart. If future UI-modal states are needed, wire a single shared state handle and add tests for state transitions.
- 🟡 The dashboard still ignores the text on a `finished: true` streaming chunk in `crates/cue-dashboard/ui/src/routes/Responses.tsx:58-65`. The current OpenAI/Anthropic parsers send an empty terminal chunk, so this is not blocking the current providers, but Ollama or mocks can produce a final chunk containing text. Safer behavior is to append non-empty `partial_text` before deleting the in-flight card.

Additional verification run for recheck:

```bash
cargo fmt --all --check                              # ✅
cargo test -p cue-daemon --test overlay_production_path --test cue_streaming_integration
                                                       # ✅ 19 passed
git diff --check 030a63c..HEAD                        # ✅
```

## Follow-ups for Next Batch

- Add token emission to Windows `emit_ask_event()` and a Windows-style ask-event fixture/test.
- Allow `AttachRequested` and `InstructionsRequested` from idle; keep only the follow-up payload events (`AttachFilesRequested`, `InstructionsUpdated`) modal-gated.
- Wire a single shared overlay UI state if modal states are still needed, or remove the unused daemon-level field until there is a real transition source.
- Add production-path tests for idle attach/style request acceptance and Windows ask-token acceptance.
- Harden `Responses.tsx` to append non-empty text before deleting an in-flight card on `finished: true`.
