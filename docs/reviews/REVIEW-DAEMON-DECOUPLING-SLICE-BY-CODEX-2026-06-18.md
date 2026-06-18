# Review: Daemon Decoupling Slice

Verdict: 🟡 ACCEPT WITH FOLLOW-UP

## Findings

No functional blocker found in this slice. The extraction preserves the existing overlay UI state semantics and RAG indexing safety checks while reducing direct `app.rs` ownership of raw RAG pipeline and lock fields.

## Follow-Ups

- `P2` The daemon is still too large. This round removes one coupling point, but `app.rs` remains the central owner of audio, sessions, cloud, overlay, and answer assembly. Continue with module extraction before adding more product behavior.
- `P2` RAG indexing is still serialized globally. Keep this for alpha safety, but move to per-session indexing queues once usage grows.
- `P2` Managed/cloud RAG should eventually move server-side. The local pipeline remains useful for offline/dev and immediate session memory, but the production architecture should not require users to host vector infrastructure locally.

## Checks

- `cargo fmt --all --check`
- `cargo test -p cue-daemon overlay_ui_state --lib`
- `cargo test -p cue-daemon rag --lib`
- `cargo test -p cue-daemon --lib`
- `cd server && cargo test`
- `cargo test --all-targets`
- `cargo clippy --all-targets -- -D warnings`
- `python3 scripts/analyze-tracing-calls.py --check-only`
- `bash scripts/observability-acceptance-smoke.sh`

## Notes

This is intentionally a refactor-only slice. It does not claim the daemon is fully decoupled. It makes the next extraction safer by removing overlay UI state scope logic and RAG indexing/rebuild/delete/query ownership from the main daemon file.

The only non-refactor code touched is `server/tests/integration_e2e.rs`, where stale model-route assertions were aligned with the current routing table. No production server behavior changed there.
