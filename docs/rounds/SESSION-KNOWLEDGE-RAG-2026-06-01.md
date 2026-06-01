# Session Knowledge RAG - 2026-06-01

## Goal

Make Bluey's session memory behave the way a user expects:

- If a document, page, screenshot summary, or code file is attached in the
  middle of a session, it becomes part of that session's knowledge base.
- If a question depends on something that was said or attached earlier, Bluey
  should retrieve the relevant memory instead of relying only on the latest
  visible transcript window.
- Prompt compaction should keep provider calls bounded, but it should not mean
  Bluey forgets the underlying session.

## What Shipped

### Local Knowledge Indexing

- Final transcript segments are indexed into local RAG as they arrive.
- `bluey context add ...` now goes through the same attachment path as overlay
  docs, page capture, screenshot fallback, and other context artifacts.
- Ready context artifacts with extracted `text_preview` are indexed
  immediately into local RAG.
- Artifact chunks are prefixed with compact source labels:
  - `Attached context: <title>`
  - `Kind: <kind>`
  - `Source: <path>`
  - optional `Note: <note>`

This is intentionally source-labeled inside the chunk text because the current
local vector store does not yet have separate metadata columns for source cards.

### Answer Context Retrieval

When Bluey answers and no explicit context was supplied by the caller, the
daemon now builds context from:

1. Recent transcript.
2. Recent Q&A.
3. Latest attached artifacts first.
4. Top current-session local RAG hits.
5. Top older-session local RAG hits.

RAG retrieval is bounded:

- Live transcript: up to 32 latest transcript turns, capped at 8,000
  characters. This is the default answer-window setting because dual-channel
  calls produce many small turns, while long monologues need a hard prompt cap.
- Current session: top 4 hits.
- Global local memory: top 6 hits.
- Total retrieved memory contexts: max 8.
- Minimum score: `0.18`.
- Dedup key: `(session_id, chunk_text)`.

This keeps answers grounded without letting old memory flood the prompt.

### Analyse Screen

Screen analysis now uses the same answer-context path, so a screenshot answer
can include the current transcript, attached docs, and relevant remembered
session snippets.

### No-Embedding Behavior

Local RAG still depends on the local embedder being available. If the embedder
is not configured, indexing/retrieval is a no-op and Bluey still uses recent
transcript plus attached artifact previews. This preserves the current dev and
offline behavior.

## Product Flow

```text
User attaches file / captures page / analyzes screen
  -> Bluey extracts readable text
  -> artifact saved on active session
  -> local RAG indexes source-labeled chunks
  -> cloud sync queues artifact + rag chunks

User asks a later question
  -> Bluey builds compact answer context
  -> recent transcript + recent Q&A + latest artifacts
  -> local RAG retrieves current-session and older-session hits
  -> managed or local provider gets bounded context
  -> answer card streams back
```

## Storage Model

Local:

- Sessions, transcript, responses, and context artifacts live in local SQLite.
- Local RAG chunks live in the local vector store.
- `bluey-dev.db` is local runtime data and must not be committed.

Cloud:

- Sync already uploads sessions, transcript, responses, context artifacts, and
  RAG chunks.
- Production still needs managed cloud retrieval in the answer path using
  server-owned embeddings and a real vector index.

Compaction:

- Compaction is prompt-window management, not deletion.
- Raw session data and indexed memory remain available until retention/export/
  delete policies remove them.

## Review Focus For Kiro

- Is the `0.18` local score threshold reasonable for the current embedder?
- Are the source labels enough until we add first-class source metadata?
- Should older-session retrieval be opt-in per user/workspace before cloud GA?
- Should attachment indexing add UI states: `Added`, `Parsing`, `Indexed`,
  `Unavailable`?
- Should managed answers merge local RAG and cloud RAG, or should cloud RAG
  become authoritative once logged in?

## Verification

Run in `/Users/uno/Downloads/cue`:

```bash
cargo fmt --all --check
cargo test -p cue-daemon --test rag_integration
git diff --check
```

Additional full-gate commands should still run before a release merge:

```bash
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
(cd server && cargo test)
(cd crates/cue-dashboard/ui && npm run build)
```

## Next Implementation Step

For production-grade cloud memory:

1. Make server-side embeddings the source of truth for managed cloud RAG.
2. Store embeddings in Postgres + pgvector for alpha scale.
3. Add cloud RAG retrieval to `BlueyManagedProvider` / answer assembly.
4. Return source cards to the overlay so users see which doc/session informed
   the answer.
5. Add retention/export/delete tests that prove context artifacts and RAG
   chunks are removed together.
