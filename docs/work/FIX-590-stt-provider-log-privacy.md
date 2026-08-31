# FIX-590: STT Provider Log Privacy

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

Speech-to-text provider and helper errors could contain echoed transcript text,
credentials, authenticated URLs, local paths, provider response bodies, or
vocabulary hints. Formatting those values through `Display`, derived `Debug`,
or tracing fields could copy private content into local logs or an opt-in
support bundle.

## Root Cause

- `SttError` variants retained arbitrary string details and rendered them in
  `Display` and derived `Debug` output.
- Transcript events, word timings, stable-agreement events, STT config, provider
  config, and parsed Deepgram frame types derived `Debug` over private fields.
- OpenAI and Deepgram JSON error paths retained remote messages or complete
  provider payloads.
- Provider supervisors and the factory formatted whole errors; Deepgram/OpenAI
  traces also included a masked key suffix even though no secret fragment is
  needed for diagnosis.
- Local Whisper spawn and parse diagnostics formatted helper paths, operating
  system details, or unparseable event text.

## Fix Summary

- Made `SttError` `Display` and custom `Debug` expose only a closed diagnostic
  category: authentication, quota, network, protocol, provider, audio format,
  or inactive session.
- Added metadata-only custom `Debug` implementations for transcript events,
  word timings, stable transcript events, and STT configuration. They expose
  counts, timing/confidence shape, source, and configured/not-configured state,
  never text or vocabulary.
- Added redacted provider config and Deepgram-frame `Debug` implementations.
  Keys, endpoints, transcripts, and words are replaced by redaction markers or
  character counts.
- Stop retaining OpenAI and Deepgram provider error messages or raw error
  payloads. Parse failures become closed protocol details, and transport errors
  retain only bounded classifications such as the I/O kind.
- Removed masked API-key suffixes from traces. Provider and factory logs now use
  one shared `stt_trace_error_category` field.
- Added closed local-Whisper categories for missing helper, spawn failure, and
  helper protocol failure. Invalid helper events log only their byte length and
  the `invalid_event` category.
- Added sentinel regressions containing transcript text, tokens, URLs, and
  paths across error formatting, config/frame debug, provider parsing, factory
  trace fields, and helper diagnostics.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-core/src/stt.rs` | Closed error categories and metadata-only Debug implementations |
| `crates/cue-daemon/src/stt/openai.rs` | Redacted config, payload-free provider errors, closed trace fields |
| `crates/cue-daemon/src/stt/deepgram.rs` | Redacted config/frame Debug, payload-free errors, shared trace category |
| `crates/cue-daemon/src/stt/factory.rs` | Category-only fallback construction logs |
| `crates/cue-daemon/src/stt/whisper/mod.rs` | Closed helper error and invalid-event diagnostics |

## Edge Cases Handled

- Provider error JSON that echoes the live transcript.
- Rate-limit messages containing arbitrary provider text.
- Malformed JSON containing a token or transcript sentinel.
- WebSocket URL, protocol, TLS, I/O, and closed-connection errors.
- Provider config with an authenticated custom endpoint.
- Deepgram frame type, transcript, and word fields containing private text.
- `Debug` formatting of a stable transcript whose agreement sidecar contains
  committed and tentative text.
- STT vocabulary and language values embedded in configuration debug output.
- Local helper binary paths, spawn details, and invalid NDJSON event bodies.

## How to Test

```bash
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- \
  cargo +1.98 test -p cue-core stt::tests
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- \
  cargo +1.98 test -p cue-daemon stt
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- bash -c \
  'cargo +1.98 clippy -p cue-core -p cue-daemon --all-targets -- -D warnings'
```

The focused Cue Core STT suite passed 13 tests. The focused Cue Daemon STT
suite passed 86 tests. The complete root/server suites and strict Clippy also
passed in disposable workspaces. An independent privacy review found no
remaining P0 or P1 disclosure path in these five STT files.

## Known Limitations

- Provider payloads are still parsed in memory to perform transcription; this
  fix prevents them from being retained or formatted into diagnostics. It does
  not change the network provider's own processing contract.
- Metrics expose bounded counts, categories, timing, confidence, and provider
  family. They intentionally cannot reconstruct the transcript or provider
  response needed for content debugging.
- Physical microphone/system-audio and packaged helper certification remains a
  release gate, separate from source-level log-privacy verification.
