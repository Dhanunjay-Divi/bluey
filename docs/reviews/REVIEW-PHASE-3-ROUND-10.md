# REVIEW: Phase 3 Round 10 — AI Hookup + Streaming + Hardening + Whisper

**Commit range:** `feat/phase-3-round-9..feat/phase-3-round-10`
**Reviewer:** Codex
**Date:** 2026-05-16

## Per-Task Review

### R10.1 — AI Hookup and Auto Recap

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/tests/auto_recap_integration.rs`, `crates/cue-daemon/tests/whisper_stub_e2e.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `request_cue` and auto-recap now exercise the specialized LLM path and persist `CueResponse` records.
- 🟢 The no-provider path is graceful, and the mock integration tests cover response shape and error propagation.
- 🟡 Daemon-side auto-recap still cannot emit Tauri events because it does not own an `AppHandle`; acceptable for alpha because persisted responses remain available.

---

### R10.2 — Streaming LLM

| Field | Value |
|-------|-------|
| Files | `crates/cue-llm/**`, `crates/cue-daemon/src/llm/**`, `crates/cue-dashboard/ui/src/routes/Responses.tsx`, `crates/cue-daemon/tests/cue_streaming_integration.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 The original cumulative-vs-append streaming bug is fixed by the R11 recheck wave. Specialized LLM callbacks now emit deltas, and targeted tests assert delta semantics.
- 🟢 OpenAI/Anthropic/Ollama stream parsers are covered by provider tests and the daemon-level streaming tests pass.
- 🟡 `Responses.tsx` still drops a non-empty final chunk if a provider ever emits `finished: true` with text. Current OpenAI/Anthropic terminal chunks are empty, so this is not blocking, but harden this in Round 12.

---

### R10.3 — Hardening Basics

| Field | Value |
|-------|-------|
| Files | `crates/cue-stealth/**`, `crates/cue-llm/src/openai.rs`, `crates/cue-llm/src/anthropic.rs`, `crates/cue-llm/src/ollama.rs`, `crates/cue-daemon/src/stt/deepgram.rs`, `crates/cue-daemon/src/stt/openai.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 Anti-debug installation is non-fatal and covered by platform tests where practical.
- 🟢 obfstr coverage for the documented streaming/provider constants is in place.
- 🟡 This is basic hardening, not a complete security boundary. Memory locking, SQLCipher, and signing remain follow-ups.

---

### R10.4 — macOS whisper.cpp

| Field | Value |
|-------|-------|
| Files | `native/macos/cue-whisper/**`, `native/windows/cue-whisper/main.c` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 SwiftWhisper is pinned to exact `1.2.0`, the package resolves/builds, and PCM16 decoding uses `loadUnaligned`.
- 🟢 The existing NDJSON helper ABI stays stable for daemon integration.
- 🟡 Windows whisper remains a documented stub; acceptable because the branch does not claim Windows real whisper.cpp yet.

## Cross-Task Findings

- R10's blocking items are resolved by the R11 fix wave. No remaining R10 blocker should stop the v0.1 alpha merge.

## Build & Test Verification

Verified on the current stacked branch `feat/phase-3-round-11`:

```bash
cargo fmt --all --check                              # ✅
cargo clippy --all-targets -- -D warnings            # ✅
cargo build --all-targets --release                  # ✅
cargo test --all-targets                             # ✅ 354 passed, 14 ignored
cd crates/cue-dashboard/ui && npm run build          # ✅
swift build -c release --package-path native/macos/cue-overlay   # ✅
swift build -c release --package-path native/macos/cue-whisper   # ✅
git diff --check                                     # ✅
```

## Overall Verdict

🟢 **ACCEPT** — Ready to merge as part of the v0.1 alpha stack.

## Follow-ups for Next Batch

- Harden final streaming chunk handling in the dashboard.
- Add Windows real whisper.cpp once the MSVC/CMake helper path is ready.
- Continue hardening with key memory handling, encrypted local DB, signing, and updater signatures.
