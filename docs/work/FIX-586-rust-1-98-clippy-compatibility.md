# FIX-586: Rust 1.98 Warning-As-Error Compatibility

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The rolling stable CI toolchain advanced to Rust 1.98 and failed otherwise
unchanged workspace and server warning-as-error gates.

## Root Cause

Rust 1.98 introduced or enabled stricter diagnostics for exact byte chunks,
draining an entire vector into another vector, irrefutable array patterns, and
the intentionally complete Axum JSON error envelope.

## Fix Summary

- Replaced two-byte and fixed-width `chunks_exact` loops with typed
  `as_chunks` slices and explicit remainder assertions.
- Replaced full-vector `drain(..).collect()` with a preallocated buffer swap.
- Simplified the now-irrefutable fixed-array binding.
- Added narrowly scoped `result_large_err` allowances to the Axum handlers and
  helpers that intentionally return the complete bounded API error envelope.
  Response status, body, billing, and routing behavior are unchanged.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-core/src/ipc_auth.rs` | Fixed-width byte conversion |
| `crates/cue-daemon/src/app.rs` | PCM and metadata pair conversion |
| `crates/cue-daemon/src/audio/framer.rs` | Preallocated full-buffer swap and refill regression test |
| `crates/cue-daemon/src/audio/system_capture.rs` | Typed PCM pairs |
| `crates/cue-rag/src/store.rs` | Typed embedding-value bytes |
| `server/src/api/stt.rs` | Typed PCM pairs |
| `server/src/api/router.rs` | Narrow Axum error-envelope lint annotation |
| `server/src/api/router/completion.rs` | Narrow completion lint annotation |
| `server/src/api/router/streaming_completion.rs` | Narrow streaming lint annotations |
| `server/src/api/router/embeddings.rs` | Narrow embedding lint annotations |
| `server/src/api/router/transcribe.rs` | Narrow transcription lint annotation |

## Edge Cases Handled

- Odd-length PCM buffers continue to ignore the incomplete trailing byte.
- Metadata budgets remain paired exactly with retained context entries.
- Audio flush moves the padded allocation into the emitted chunk while
  immediately installing a preallocated replacement buffer, preserving fast
  refill behavior after both sequence gaps and shutdown-tail delivery.
- HTTP error response shapes remain wire-compatible.

## How to Test

```bash
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 clippy --workspace --all-targets -- -D warnings
(cd server && cargo +1.98.0 fmt --all -- --check)
(cd server && cargo +1.98.0 clippy --all-targets -- -D warnings)
```

## Known Limitations

- CI still follows rolling stable. A separate release-toolchain policy can pin
  an explicit version if reproducibility is preferred over immediate stable
  compiler coverage.
