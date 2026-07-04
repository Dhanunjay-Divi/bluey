# ROUND-332 Code Follow-Up Quality Diagnostics

Date: 2026-07-04
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User Report

The user showed session `6CC7D7A4` where an algorithm prompt about Alice/Bob received a short generic answer, then follow-ups like `Can you give me Python code?` and `So can you give me Java code for the same?` either asked for missing context or failed with refs such as `61B3DA69` and `B522EE81`.

Expected behavior:

- First algorithm answer should look like a polished ChatGPT/Claude-style coding answer: approach, complete code, explanation, complexity, and edge cases.
- Short code follow-ups should use the previous problem statement and prior artifact without asking for the prompt again.
- Failure refs should be traceable in local diagnostics.

## Production Evidence

Live API logs for session `6CC7D7A4` showed:

- Initial request `27CB318D` was classified as `answer_intent=general`, `answer_output=compact`, `context_chars=0`, and completed with no artifact. This explains the short answer.
- Follow-up request `77732480` was classified as coding, but only had `context_chars=110`, which was not enough problem statement for a full solution. Gemini first hit `429`, then OpenAI succeeded after fallback.
- Screen resend request `9574DFDB` succeeded as coding with a code artifact, but had `input_tokens=10879` and `first_event_latency_ms=16119`, which explains the slow start.
- The desktop error refs are generated from the local request UUID. When exact refs are absent from server logs, the daemon needs to log the local request ref, session code, context counts, and error chain.

## Root Cause

Two paths were not strong enough:

1. Server AnswerPlan treated `Can you give me Python code?` as a fresh coding request even when prior coding session context existed, instead of a coding follow-up.
2. Desktop context building relied on generic recent Q&A. If the prior answer was compact, the follow-up could inherit only a tiny summary instead of explicit prior coding context.

## Changes

### Server

- Added contextual code-generation follow-up detection for prompts like:
  - `Can you give me Python code?`
  - `Can you give me Java code?`
  - `full code`
  - `code for the same`
- When prior planning/session context is coding-shaped, these now resolve to:
  - `intent = coding_followup`
  - `output = code_artifact`
  - `lane = deep`

### Desktop Daemon

- Added a focused `Recent coding context` block for immediate code follow-ups.
- The block carries:
  - full prior coding question
  - prior answer summary
  - prior code artifact body when available
- The context wording intentionally avoids prompt/instruction-like labels so server-side private-instruction guards do not false-positive on normal follow-ups like `give me Java code for the same`.
- Added failure diagnostics for local answer refs:
  - request id and short ref
  - session id and session code
  - route primary and fallback count
  - visible/pending context counts
  - context kind counts
  - question intent/word/char counts
  - full safe error chain

### Version

- Bumped desktop release version to `0.1.72`.

## Verification

Passed:

```bash
cargo check -p cue-daemon
cargo test -p cue-daemon meeting_context_ --lib
cargo test -p cue-daemon answer_error --lib
cargo check --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml answer_plan_ --lib
```

Targeted coverage added:

- `meeting_context_keeps_recent_qa_for_short_code_regeneration_follow_up`
- `meeting_context_keeps_focused_code_prompt_for_same_java_follow_up`
- `answer_plan_python_request_with_prior_coding_context_is_followup`
- `answer_plan_java_request_for_same_prior_coding_context_is_followup`

## Notes

This round fixes future continuity and traceability. It cannot rewrite already-saved compact turns in old sessions, but the next follow-up from a freshly updated `0.1.72` desktop will preserve the prior coding context explicitly.
