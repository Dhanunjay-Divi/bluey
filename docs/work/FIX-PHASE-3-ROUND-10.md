# FIX-PHASE-3-ROUND-10: Codex Blockers (4 issues)

## Issue

Four blockers identified during R10 codex review (pending — preemptive fixes based on known gaps):

1. End-to-end UI streaming not wired (daemon emits `cue_response_chunk` but never fires it)
2. Streaming auth header names not obfuscated (only base URLs were covered)
3. SwiftWhisper version not pinned exactly (used `from:` instead of `.exact()`)
4. macOS PCM16 decode uses `bindMemory` which is alignment-unsafe on odd-byte buffers

Reference: `docs/work/PHASE-3-ROUND-10-HANDOFF-FOR-CODEX-REVIEW.md` — deferrals #1 and review checklist items.

## Root Cause

| # | Issue | Root Cause |
|---|-------|------------|
| 1 | No streaming chunks reach UI | `request_cue` and `auto_recap` Tauri commands called `run()` (non-streaming). Needed `run_streaming(callback)` that emits per-chunk events via `AppHandle::emit_all()`. |
| 2 | Auth headers visible in binary | `obfstr!()` was applied to base URLs but not to `"Authorization"`, `"Bearer"`, `"x-api-key"`, `"anthropic-version"`, `"2023-06-01"` in the streaming `complete_stream` code paths. |
| 3 | SwiftWhisper version drift | `Package.swift` used `.package(url:..., from: "1.2.0")` which allows 1.2.x+ semver-compatible updates. |
| 4 | PCM alignment crash | `bindMemory(to: Int16.self)` on raw `Data` buffer requires 2-byte alignment. Audio buffers from stdin pipe are not guaranteed aligned. |

## Fix Summary

### 1. End-to-end UI streaming (commit `6ff8a14`)

- Specialized LLMs (`AnswerLlm`, `RecapLlm`, `WhatToAnswerLlm`) gain `run_streaming(callback)` method.
- `request_cue` Tauri command generates `response_id` (UUID) up-front, passes streaming callback that calls `app_handle.emit_all("cue_response_chunk", ChunkPayload { response_id, text, finished })` per chunk.
- After stream completes, emits final `cue_response` event with full accumulated text.
- `auto_recap` Tauri command uses same pattern.
- Daemon→dashboard hop NOT needed: cue commands run inside the Tauri dashboard process with direct `AppHandle` access.

### 2. obfstr on streaming auth headers (commit `9f23c18`)

- Applied `obfstr!()` to header name strings in `complete_stream()` implementations for both OpenAI and Anthropic providers.
- Covered: `"Authorization"`, `"Bearer"`, `"x-api-key"`, `"anthropic-version"`, `"2023-06-01"`.
- Verified: `strings target/release/cue-dashboard | grep -cE '(Authorization|x-api-key|anthropic-version)'` returns 0.

### 3. SwiftWhisper exact pin (commit `cbaeb51`)

- Changed `Package.swift` dependency to `.exact("1.2.0")`.
- Updated `Package.resolved` lockfile to match.
- Ensures reproducible builds — no surprise whisper.cpp API changes.

### 4. PCM16 alignment-safe decode (commit `cadd362`)

- Replaced `bindMemory(to: Int16.self)` + subscript loop with `loadUnaligned(fromByteOffset:as: Int16.self)` loop iterating by stride 2.
- Works correctly regardless of buffer alignment.
- Same Float32 conversion: `Float(sample) / 32768.0`.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/llm/answer.rs` | Added `run_streaming(callback)` method |
| `crates/cue-daemon/src/llm/recap.rs` | Added `run_streaming(callback)` method |
| `crates/cue-daemon/src/llm/suggest.rs` | Added `run_streaming(callback)` method |
| `crates/cue-dashboard/src/commands.rs` | `request_cue` + `auto_recap` emit `cue_response_chunk` per chunk |
| `crates/cue-llm/src/openai.rs` | `obfstr!()` on auth header names in `complete_stream()` |
| `crates/cue-llm/src/anthropic.rs` | `obfstr!()` on auth header names in `complete_stream()` |
| `native/macos/cue-whisper/Package.swift` | `.exact("1.2.0")` pin |
| `native/macos/cue-whisper/Package.resolved` | Updated lockfile |
| `native/macos/cue-whisper/Sources/main.swift` | `loadUnaligned` PCM decode loop |

## Edge Cases Handled

- Streaming callback tolerates empty chunks (skips emit if `text.is_empty() && !finished`)
- `response_id` generated before stream starts — UI can track from first chunk
- `loadUnaligned` handles odd-length buffers (last byte ignored if buffer length is odd)
- obfstr covers both the non-streaming `complete()` and streaming `complete_stream()` paths

## How to Test

```bash
# Verify obfstr coverage
cargo build --release --all-targets
strings target/release/cue-dashboard | grep -cE '(Authorization|x-api-key|anthropic-version|Bearer)'
# Expected: 0

# Verify streaming tests pass (5 new tests)
cargo test --all-targets -- streaming
# Expected: 5 tests pass (chunk emission, callback invocation, response_id propagation)

# Verify SwiftWhisper pin
grep -A1 'exact' native/macos/cue-whisper/Package.swift
# Expected: .exact("1.2.0")

# Full pipeline
cargo test --all-targets  # 331 pass
```

## Known Limitations

- Daemon-side `auto_recap` (spawned from IPC handler without AppHandle) still uses non-streaming `run()`. Only the dashboard-side `auto_recap` Tauri command streams. This is by design — daemon process has no Tauri AppHandle.
- `AnswerLlm` env-var build supports OpenAI only; Anthropic/Ollama from settings deferred.
- obfstr does NOT cover `cue-rag` crate (documented deferral, out of scope for R10 fixes).
