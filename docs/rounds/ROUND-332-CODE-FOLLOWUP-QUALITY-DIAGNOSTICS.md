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

## Deploy And Live Smoke

Server and desktop deployment:

- Server API deployed to production with commit `bb4aca3b10cdb66d40f7b3438939f956a6aabbd7`.
- Desktop release `0.1.72` published to `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.72/bluey-0.1.72-darwin-arm64.tar.gz`
- Publish verification passed:
  - release artifact dev-flag/secret scan
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.72`
- Local install from public `install.sh` verified `/Users/uno/.bluey/bin/bluey` and `/Users/uno/.bluey/bin/bluey-daemon` both report `0.1.72`.
- Local daemon restarted into session `8491bd10-812a-4532-8426-add8ac023e36`.

Live smoke:

- Direct Alice/Bob coding prompt returned a complete coding answer with approach, code, explanation, complexity, and edge cases.
- Request `e99d2770-b163-4609-ba9a-9709bac9d80c` for `So can you give me Java code for the same?` used neutral `Recent coding context` and returned a complete Java code artifact.
- Server logs for `e99d2770-b163-4609-ba9a-9709bac9d80c` showed `answer_plan_source=rules`, `answer_intent=coding_followup`, `answer_output=code_artifact`, `effective_lane=deep`, route `deepseek-v4-pro`, `canvas_artifact_type=code`, and no internal disclosure guard block.

During the first 0.1.71 smoke, the server's internal-disclosure guard falsely blocked a synthetic follow-up context because the desktop label used prompt-like wording. That was fixed before the final 0.1.72 deploy by renaming the block to neutral coding context.

## Sync Follow-Up

After deploy, production logged one `/sync/batch` warning:

```text
sync endpoint failed error=error serializing parameter 5
```

The object uploads immediately before it succeeded, so this was most likely metadata persistence, not file-byte upload. Live Postgres schema for `cloud_context_artifacts.title` is correctly `TEXT NOT NULL`; the likely cause is a raw NUL byte in a text field coming from local artifact/session metadata. Follow-up fix:

- Sanitize raw NUL bytes out of all Postgres sync text fields before binding.
- Add table and record id context to each Postgres sync insert/update error.
- Log the full safe error chain for `/sync` failures instead of only the top-level error.
- Deployed the server-side sync hardening to production with commit `bddfe86379bff57dfb90714d1f2ad2261392d6c5`; `/health` reported that commit and recent logs showed no sync failure after restart.

Additional verification:

```bash
cargo test --manifest-path server/Cargo.toml db_text_removes_nul_bytes_before_postgres_bind --lib
cargo test --manifest-path server/Cargo.toml sync_batch_round_trips_session_bundle_and_rag --lib
cargo test --manifest-path server/Cargo.toml answer_plan_ --lib
cargo check --manifest-path server/Cargo.toml
```

## Notes

This round fixes future continuity and traceability. It cannot rewrite already-saved compact turns in old sessions, but the next follow-up from a freshly updated `0.1.72` desktop will preserve the prior coding context explicitly.
