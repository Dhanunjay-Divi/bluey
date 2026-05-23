# Bluey Cloud Sync, RAG, and STT Auth Pass

Date: 2026-05-21

## Goal

Close the product architecture gap for the first 1,000-user shape:

- desktop remains local-first and fast;
- managed Bluey cloud stores sessions, transcripts, answer metadata, context previews, and RAG chunks;
- old sessions can be listed/loaded from the server;
- cloud RAG is account-scoped;
- STT provider secrets stay server-owned instead of shipping static Deepgram/OpenAI keys to the desktop.

## Implementation

Server:

- Added migration `0012` in `server/src/db/mod.rs`:
  - `cloud_sessions`
  - `cloud_transcript_segments`
  - `cloud_cue_responses`
  - `cloud_context_artifacts`
  - `cloud_rag_chunks`
  - `stt_sessions`
- Added `server/src/db/sync.rs` with idempotent upserts, session listing, session bundle loading, and lexical/vector-ready RAG query.
- Added `server/src/api/sync.rs`:
  - `POST /sync/batch`
  - `GET /sync/sessions`
  - `GET /sync/sessions/:session_id`
  - `POST /rag/query`
- Added `server/src/api/stt.rs`:
  - `POST /stt/session`
  - `GET /stt/relay`
  - returns a Bluey-scoped session token and relay URL;
  - never returns static provider API keys.
  - relay requires the normal Bearer account token plus the Bluey STT token, single-claims sessions only for the same account, connects to Deepgram with the server-held key, forwards WebSocket frames, records usage, and bills elapsed seconds.
- Account export now includes synced sessions, transcript, responses, context previews, and RAG count.

Cloud client:

- Added typed request/response structs for sync, RAG, and STT session creation.
- Added `CloudClient` methods:
  - `sync_batch`
  - `list_cloud_sessions`
  - `load_cloud_session`
  - `query_rag`
  - `create_stt_session`
- Session/RAG response structs now serialize as well as deserialize so CLI JSON output works.

Daemon:

- Added `crates/cue-daemon/src/cloud/sync.rs`.
- `bluey cloud sync` / `DaemonRequest::CloudSyncNow` now performs a real upload:
  - local sessions from `MeetingStore`;
  - transcript segments;
  - attached context previews;
  - locally persisted cue responses with cost/artifact metadata;
  - conversation fallback responses;
  - lexical RAG chunks from transcript, context, summaries, instructions, and answers.
- Sync batches are capped below the server request limit so long sessions split safely.
- Cloud status no longer claims “sync client is not wired” once credentials exist.
- Balance polling and sync now respect the account API URL saved by first
  `bluey on` sign-in, not only the default production URL.
- Env-token mode still works via `BLUEY_CLOUD_TOKEN` / `BLUEY_API_TOKEN` / `CUE_CLOUD_TOKEN` / `CUE_API_TOKEN`.
- The chunked real-audio path now defaults to managed `/router/transcribe` when the user is logged in and no explicit developer STT key is set, so customer builds do not need a desktop Deepgram/OpenAI STT key.

CLI:

- Added testable cloud inspection commands:
  - `bluey cloud sync`
  - `bluey cloud sessions --limit 20`
  - `bluey cloud show <session_id>`
  - `bluey cloud rag "query words" --limit 8`
- Existing `bluey usage`, `bluey credits`, `bluey portal`, `bluey export`, and `bluey delete-account` now use the same account-aware cloud client path.

## Flow

New customer path:

1. User runs `bluey on`.
2. Bluey records locally first.
3. User runs `bluey on`; Bluey opens sign-in if needed.
4. `bluey cloud sync` uploads local sessions and searchable RAG chunks.
5. `bluey cloud sessions` lists previously synced sessions from any synced device.
6. `bluey cloud show <id>` loads the server-side session bundle.
7. `bluey cloud rag <query>` searches account-scoped cloud memory.

STT authorization path:

1. Desktop asks `POST /stt/session` for a Bluey-scoped STT session.
2. Server checks account balance/trial state.
3. Server stores the short-lived STT session token.
4. Response points at `/stt/relay` and returns no provider secret.
5. `/stt/relay` validates the account JWT and single-claims the Bluey token for that account, then proxies to Deepgram using only the server-held provider key.

Chunked audio path:

1. If a customer is logged in and no explicit developer STT key is configured, Bluey posts WAV chunks to `bluey-server /router/transcribe`.
2. `bluey-server` dispatches to Deepgram, handles billing/idempotency, and returns transcript JSON.
3. Explicit `BLUEY_STT_API_KEY` / `OPENAI_API_KEY` remains a developer override path only.

## Security Notes

- Static Deepgram/OpenAI STT keys are still developer-only env paths.
- The managed customer path must use server relay or future provider-issued ephemeral scoped tokens.
- `/sync/*`, `/rag/query`, and `/stt/session` are all protected by account auth.
- Cloud tables are tenant-scoped by `account_id`.
- Batch limits protect the sync endpoint from oversized uploads.
- Transcript and answer text caps prevent a single request from becoming an accidental dump pipe.

## Scale Notes

This is deliberately not Kubernetes-shaped yet.

- SQLite is fine for local dev and early alpha validation.
- For 1,000 users, move `bluey-server` to Postgres before public launch.
- `cloud_rag_chunks.embedding_json` is a compatibility bridge; production should become `pgvector`.
- One DigitalOcean region is acceptable for v0.2 alpha. If STT relay latency becomes visible, add regional relays later while keeping billing/session issuance centralized.

## Verification

Commands run:

```bash
cargo fmt --all
cargo test -p cue-cloud-client
cargo test -p cue-daemon cloud::sync
cd server && cargo test sync_batch_session_bundle_and_rag_roundtrip
cd server && cargo test sync_batch_round_trips_session_bundle_and_rag
cd server && cargo test validate_rejects_empty_or_huge_batches
cd server && cargo test random_token_is_url_safe_and_long
cargo check -p cue-cli
cargo clippy -p cue-daemon --all-targets -- -D warnings
cargo clippy -p cue-cli --all-targets -- -D warnings
cd server && cargo clippy --all-targets -- -D warnings
cargo check -p cue-daemon
cd server && cargo check
cd server && cargo test stt::tests
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo build --all-targets --release
cargo test --all-targets
cd server && cargo test
cd server && cargo build --all-targets --release
cd crates/cue-dashboard/ui && npm test
cd crates/cue-dashboard/ui && npm run build
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git diff --check main
```

## Still Deferred

- Move server storage from SQLite to Postgres + pgvector for production.
- Add dashboard UI for cloud session history and “continue from cloud”.
- Push live sync in the background instead of manual `bluey cloud sync`.
- Add provider-cost optimizer data in server config so routing can pick cheapest acceptable provider per lane.
- Add cloud-side document binary storage; this pass stores metadata and parsed previews, not original files.
- Switch the continuous streaming daemon STT provider to the managed `/stt/relay`; the default chunked real-audio path now uses managed `/router/transcribe`.
