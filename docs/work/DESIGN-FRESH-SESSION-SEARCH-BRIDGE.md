# Design: Fresh-session + attached-history search (beta)

> Status: IMPLEMENTED + tested (rag layer + daemon wiring). Feature is behind
> `local-memory` (daemon) / `local-embed` (cue-rag) and `BLUEY_AGENT_HISTORY`,
> as before — this change only refines HOW the existing search scopes.

## Correction to an earlier claim

An earlier note in this effort said "there is no meeting→session mapping stored
anywhere." **That was wrong.** `MeetingRecord.agent_session_id` (+ `agent_kind`)
IS that mapping — stamped per meeting by `stamp_meeting_agent_link`
(crates/cue-daemon/src/app.rs) and persisted. The overlay's "Continue meeting"
button re-attaches that stored session. `settings.attached_session` (global,
current attachment) and `meeting.agent_session_id` (per-meeting, durable) are the
SAME id-space — both the raw `SessionRef.id`.

## The model

The calendar OAuth integration is a **timing trigger only** — it fires
`warmup_open()` ahead of a meeting and pops the overlay. It carries NO agent
session id and does NOT decide which session to resume.

Instead of resuming the user's real agent session (which raises "which session?"
+ prompt-leak concerns), each meeting runs on a **fresh, clean session**. The
session the user **attaches** stops being a *resume target* and becomes a
**search scope**: the fresh meeting agent pulls relevant prior context on demand
via the `search_agent_history` MCP tool.

```
Calendar fires → FRESH CLEAN SESSION (no resume)
User's attached session → becomes SEARCH SCOPE (not resumed)
Meeting agent → search_agent_history → pulls relevant context on demand
```

### Why this resolves prior tensions
- "Which session to resume per meeting?" → no resume, always fresh.
- Bluey prompt leaking into user's real session → never touched; own clean session.
- In-meeting Q&A / context memory → on-demand search, not pushed reconstruction
  (this is why gating off the conversation-memory table is safe — see
  `crate::conversation` / `BLUEY_CONV_MEMORY`).

## What was built

Scope "attached session first, widen if empty" is applied at **search time**,
not by rebuilding the index. The index still builds by agent-kind (the broad
corpus); the filter happens per query. This is lean — no new index state, no
per-search rebuild.

- **cue-rag** (`AgentHistoryIndex`): new `search_prefer_session(query, emb, k,
  prefer_session)`. Refactored `search` → private `search_filtered(.., restrict:
  Option<&HashSet<usize>>)` so the ranking logic is shared, not duplicated.
  `search_prefer_session` runs it restricted to the preferred session's chunks
  first; if that yields nothing, it re-runs unrestricted. Blank/absent preference
  degrades to plain `search`. Both the semantic pool AND the BM25 doc set honor
  the restriction (so IDF stats don't leak from excluded chunks). 4 unit tests.
- **cue-daemon** (`AgentHistoryStore::search`): reads `settings.attached_session`
  and passes it as `prefer_session`. Unset → plain full-index search.

## Id-space (confirmed)
`settings.attached_session` and the index's chunk `session_id` are the SAME
`SessionRef.id` string (for Claude, the `.jsonl` file-stem UUID) — direct string
match, no translation. Compared after `trim()` on both sides.

## Verification
- cue-rag `--features local-embed`: 61 pass (incl. 4 new prefer-session tests).
- cue-daemon `--features local-memory`: full suite 291+ pass, 0 fail.

## Parked (post-beta, user: "take care later, working beta first")
- Google auth: keychain-vs-file token store precedence.
- Google auth: first-run/cold-start OAuth on a fresh non-local install.
- Google auth: hardcoded client-secret literal in `provider.rs` —
  MUST be removed + rotated before any commit (do NOT commit provider.rs as-is).
