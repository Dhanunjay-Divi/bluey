# ROUND-312 Memory Lookup Startup Latency

Date: 2026-07-02

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Goal

Fix the confusing and slow-feeling answer startup where Bluey showed saved/conversation memory status before streaming and then sometimes failed with a generic message.

User-reported Bluey id: `25594F6D`

Resolved local session id:

```text
25594f6d-4cc7-4315-b99b-017b567851ae
```

## Diagnosis

- Local `bluey status` mapped `25594F6D` to the active meeting id.
- `active-meeting.json` showed `transcript_segments: 0` and `context_items: 0`.
- The active session contained two successful managed answers and no persisted failed turn.
- The native managed-provider path always pushed `Checking conversation context` for every non-screen streaming answer, even before the server selected a route.
- The server also performed saved-context/RAG lookup for every normal request before planning/provider dispatch. This was budgeted, but still added delay and made the UI blame memory when the real failure could be provider capacity, auth, stream, billing, or another downstream issue.
- Failed answer cards did not include a short request ref, so a screenshot/session id was not enough to locate the failed provider call later.

## Changes

- Native overlay no longer pushes `Checking conversation context` before every non-screen managed request.
- Native local RAG lookup is now explicit/follow-up only:
  - skipped for standalone direct asks like code, fresh live-caption wrappers, and selected-attachment cases;
  - kept for explicit memory/context requests and clear follow-ups such as previous code/answer/design.
- Server RAG lookup is now explicit/follow-up only before completion:
  - skipped for normal standalone/direct questions and generic live-caption wrappers with their own planning context;
  - kept for explicit saved-memory/session-context requests and short follow-ups.
- Server status wording now says `Using relevant conversation context...` only when relevant context is actually attached. It no longer emits a misleading long-running `Checking conversation context...` status.
- Failed overlay answers now append a short request ref like:

```text
Ref: 25594F6D
```

That makes future screenshots traceable without exposing raw prompt/content in logs.

## Verification

```bash
cargo fmt --all
cargo test --manifest-path server/Cargo.toml memory_lookup -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_context_wording -- --nocapture
cargo test -p cue-daemon answer_memory_lookup -- --nocapture
cargo test -p cue-daemon answer_error_ref -- --nocapture
cargo build -p cue-daemon --bin bluey-daemon
cargo build --manifest-path server/Cargo.toml
```

## Follow-Up

- Add a persisted failed-answer diagnostic row with request id, session id, sanitized error category, provider/lane, and timing, but no prompt/transcript text.
- Add a `bluey inspect <ref>` owner/dev command that summarizes the last failed request by short ref.
