# Round 169 - AI Status And Attached Context Fix

Date: 2026-06-25 14:13 EDT

## Summary

Fixed two live Bluey friction points from the compaction handoff:

- `bluey ai status` now treats saved Bluey account tokens as valid managed-cloud credentials, matching `bluey cloud status`.
- Answers can use relevant current-session attachments even when no explicit pending/visible attachment IDs are present, which covers CLI/reopened-session cases where context is visible in the session but not attached to the answer request.

## Code Changes

- `crates/cue-daemon/src/app.rs`
  - Made AI status path-aware so it can check `cue_cloud_client::tokens::tokens_available(&paths)`.
  - Marked Bluey managed as live-capable in provider status once credentials are configured.
  - Added a relevance fallback for current-session attachments when `visible_context_ids` is empty.
  - Kept explicit pending attachment IDs as the primary path.
  - Kept saved artifacts from being resent on unrelated questions.
  - Added regressions:
    - `ai_status_counts_saved_account_tokens_for_managed_cloud`
    - `relevant_current_attachment_context_matches_current_doc_without_pending_ids`
    - `relevant_current_attachment_context_ignores_unrelated_questions`
- `crates/cue-cli/src/app.rs`
  - Accepted `BLUEY_CLOUD_API_TOKEN` as an env-token alias alongside existing cloud token env vars.
- `crates/cue-cloud-client/src/tokens.rs`
  - Included `BLUEY_CLOUD_API_TOKEN` in `tokens_available`.

## Verification

Targeted tests:

```bash
cargo fmt --check -p cue-daemon -p cue-cli -p cue-cloud-client
cargo test -p cue-daemon relevant_current_attachment_context -- --nocapture
cargo test -p cue-daemon ai_status_counts_saved_account_tokens_for_managed_cloud -- --nocapture
```

Release build and local install:

```bash
cargo build --release -p cue-cli -p cue-daemon
install -m 755 target/release/bluey "$HOME/.bluey/bin/bluey"
install -m 755 target/release/bluey-daemon "$HOME/.bluey/bin/bluey-daemon"
./scripts/bluey-visible-local.sh
```

Live status after restart:

- `bluey cloud status`: `TokenConfigured`, endpoint `https://bluey.sh`.
- `bluey ai status`: `bluey_managed / bluey-router-v1: Healthy`, `vision: yes`, `STT: yes`.
- `bluey audio status`: native capture still `yes`.
- Short `bluey audio start` / `audio stop` smoke used native runtime, selected `bluey-managed:deepgram/nova-3 live`, and emitted chunks on both sources before stopping:
  - system chunks: 127
  - microphone chunks: 26
  - transcript segments: 0, likely because no speech was present in the short smoke.

Live attached-context smoke:

```bash
bluey context add docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md
bluey ask "In one sentence, what is the Bluey compaction handoff about?"
```

Result:

```text
It's a saved checkpoint doc that lets a new Codex session pick up exactly where the previous one left off on the Bluey codebase, without losing any in-progress work or reverting user changes.
```

RAG DB check:

- Active session `d4d535ff-f253-47c8-8a04-9572ab3c6b9d` has 34 chunks.
- `rag_chunks` count: 288.
- `rag_embeddings` count: 288.

## Current Local State

- Bluey is running in visible local overlay mode from `~/.bluey/bin`.
- Daemon pid after restart: `52089`.
- Active meeting id: `d4d535ff-f253-47c8-8a04-9572ab3c6b9d`.
- Active meeting title: `BLUEY-COMPACTION-HANDOFF-2026-06-25 Md`.
- Active context items: 1, the compaction handoff doc.
- Overlay visible: true.
- Overlay capture excluded: false, because this is local visible test mode only.
- Latest `bluey credits` after live checks: `$7.02`.

## Notes

- The first live `bluey ask` before the attachment fallback fix reproduced the bug: it said it did not have context even though the active meeting had the handoff doc and RAG chunks.
- After the fix, the same question answered from the attached document.
- The relevance fallback is intentionally conservative: it only applies when no explicit visible context IDs are present and the question matches current-session attachment title/path/note/preview text.
