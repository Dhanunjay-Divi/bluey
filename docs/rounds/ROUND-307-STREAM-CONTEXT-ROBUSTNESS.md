# Round 307: Stream And Context Robustness

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User-Visible Problem

The latest attached photo showed Bluey answering a simple data-structures question, then failing the next coding request with:

> Bluey's connection dropped before the answer finished...

The same area also exposed confusing wording around "local memory" or saved context when no documents were attached. For production beta behavior, Bluey should keep running for hours without scary diagnostic copy, should preserve complete-looking streamed answers, and should use clear user-facing language.

## What Changed

- OpenAI-compatible streaming routes now tolerate provider/proxy streams that close after delivering text but before `[DONE]`.
- Anthropic streaming routes now tolerate streams that close after delivering text but before `message_stop`.
- In both cases, Bluey estimates output tokens from delivered text and emits a normal terminal stream event, while still failing empty streams.
- The desktop daemon now preserves a complete-looking streamed answer if only terminal metadata is missing.
- Truly incomplete answer shapes, such as unclosed code fences or dangling tables/headings, still fail and are not saved.
- User-facing incomplete-stream copy no longer tells users to check server logs.
- "Saved Bluey memory", "saved context", and "local RAG" user-facing labels were replaced with "conversation context" wording.
- The AnswerPlan eval suite now includes the screenshot-style Java palindrome code request so it routes as coding/code-artifact.

## Files Changed

- `server/src/routing/dispatcher.rs`
- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-core/src/ai.rs`

## Verification

```bash
cargo test --manifest-path server/Cargo.toml stream_without --lib
cargo test --manifest-path server/Cargo.toml answer_plan --lib
cargo test -p cue-daemon incomplete_stream --lib
cargo test -p cue-daemon terminal_metadata --lib
cargo test -p cue-core --lib
cargo fmt -p cue-daemon -p cue-core -p cue-llm
cargo fmt --manifest-path server/Cargo.toml
git diff --check
```

All listed checks passed.

## Notes

This round improves the exact failure class from the screenshot but does not replace the need for live provider telemetry. If this happens again in the wild, logs should now show whether the provider closed after text, whether Bluey estimated final usage, and whether the desktop preserved the answer.
