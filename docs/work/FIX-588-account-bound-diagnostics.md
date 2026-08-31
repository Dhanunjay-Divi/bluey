# FIX-588: Account-Bound Diagnostics And Deletion Fences

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

Bluey's new end-to-end latency diagnostics needed to remain useful across the
native overlay, daemon, STT, RAG, model, persistence, and support boundaries
without creating a second path for private user content. At the same time,
long-running background work could outlive an account replacement or token
refresh, and deletion/revocation could race a queued upload, local hydration,
or delayed response.

Those races could let stale Account A work clear or publish against newer
Account A2 or Account B authority, and a response-lost deletion did not have an
opaque, replay-safe way to distinguish `pending` from `deleted`.

## Root Cause

- Account profile fields, token pairs, API origin, device identity, and the
  profile generation could be loaded or compared independently. A delayed 401,
  balance response, listen verification, sync task, or provider completion
  therefore lacked one exact authority snapshot to revalidate.
- Background credential cleanup could reload the current token state before
  clearing it. That substitutes newer authority for the snapshot that actually
  received the revocation response.
- Phase 623's local action log did not provide one closed, typed schema or a
  bounded non-blocking queue across the complete answer/audio path.
- Support-diagnostic sharing did not yet have append-only server consent
  receipts, consent-epoch fencing, account/session tombstones, or atomic
  revocation plus cleanup scheduling.
- Account and session deletion could overlap reserved object uploads, sync
  hydration, local RAG projections, and response loss. A delete acknowledgement
  alone was not enough to prove that every side effect had either completed or
  remained durably fenced.

## Fix Summary

- Added `CredentialSnapshot`, which atomically binds owner account, credential
  generation, normalized API origin, device identity, and the exact access and
  refresh token pair. Its custom `Debug` output redacts identity, origin,
  device, and tokens.
- Added mandatory `clear_if_current` and snapshot compare-and-swap operations.
  Delayed background work clears only the exact authority it captured before an
  await boundary; explicit user logout remains the only force-clear path.
- Bound balance events, listen verification, audio/STT dispatch, cloud sync,
  answer generation, overlay hydration, and local purge work to the captured
  account/credential generation, with revalidation before publication.
- Added a metadata-only diagnostic schema with closed event names, labels, and
  counters. Producers use bounded `try_send` queues; one background writer
  persists batches and reports dropped-event counts without blocking answer or
  audio hot paths.
- Added explicit account-scoped support-diagnostic consent in Settings and
  append-only server receipts for policy `2026-08-30` with content policy
  `metadata_only`. An operator flag cannot enable upload without persisted user
  consent and matching current server consent.
- Added server-side closed-schema validation, forbidden-content-key rejection,
  account/session-scoped deletion, consent-epoch fencing, durable tombstones,
  bounded cleanup retries, and upload finalization checks that prevent a stale
  reserved object from becoming ready after revocation or deletion.
- Added account-deletion preparation, local capture/content fences, opaque
  capability-bound reconciliation receipts, durable pending state, ordered
  object cleanup, and account/session/child tombstones so delayed sync or local
  writes cannot resurrect deleted content.
- Preserved RAG child provenance on identifier-only tombstones so every owned
  device can remove derived local memory without restoring deleted text or
  embeddings.
- Kept the default customer credential path in Bluey's private account profile;
  this batch does not enable the OS Keychain or add repeated Keychain prompts.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-cloud-client/src/tokens.rs` | Exact snapshot, redacted debug, conditional clear/CAS |
| `crates/cue-cloud-client/src/client.rs` | Bound clients, delayed clear, consent/delete APIs |
| `crates/cue-cloud-client/src/types.rs` | Diagnostic, tombstone, and deletion contracts |
| `crates/cue-core/src/config.rs` | Credential-generation and account-scoped consent/revocation settings |
| `crates/cue-core/src/ipc.rs` | Typed account deletion and support-diagnostic daemon contracts |
| `crates/cue-core/src/ipc_auth.rs` | Account-generation authority validation |
| `crates/cue-core/src/observability.rs` | Privacy-safe correlation metadata |
| `crates/cue-daemon/src/diagnostics.rs` | Bounded bus, local writer, consent epoch, purge fences |
| `crates/cue-daemon/src/app.rs` | Account-bound runtime, exact clears, local deletion |
| `crates/cue-daemon/src/cloud/balance.rs` | Snapshot-bound balance events and typed failures |
| `crates/cue-daemon/src/cloud/sync.rs` | Safe bundles, consent, delete outbox, tombstones |
| `crates/cue-daemon/src/db/*` | Account-scoped local storage, RAG, search, and speaker deletion fences |
| `crates/cue-daemon/src/rag_indexer.rs` | Owner/generation checks before derived-memory writes |
| `crates/cue-dashboard/src/commands.rs` | Account delete and diagnostic consent workflow |
| `crates/cue-dashboard/ui/src/pages/Settings.tsx` | Explicit metadata-only sharing control and cleanup status |
| `server/src/api/support_diagnostic_schema.rs` | Closed wire schema, bounded counters, forbidden-content validation |
| `server/src/api/sync.rs` | Authenticated support consent, upload, session delete, and cleanup routes |
| `server/src/api/account.rs` | Capability-bound account deletion and response-loss reconciliation |
| `server/src/api/auth_routes.rs` | Reject new auth authority for deletion-pending accounts |
| `server/src/db/support_diagnostics.rs` | Append-only consent receipts for SQLite and PostgreSQL |
| `server/src/db/object_uploads.rs` | Reservation/finalization fences and durable object cleanup |
| `server/src/db/account_data.rs` | Account deletion fence, opaque receipts, and ordered hard-delete authority |
| `server/src/db/sync.rs` | Session and child tombstones with anti-resurrection checks |
| `server/src/db/refresh_tokens.rs` | Account/device token revocation during deletion |
| `server/src/api/jobs.rs` | Deletion-pending authority checks for Jobs data |
| `server/src/api/jobs_resume_assets.rs` | Account-bound resume-object deletion coverage |
| `infra/migrations/013_cloud_session_tombstones.sql` | Durable local account/session deletion fence |
| `infra/postgres/server-runtime/019_support_diagnostic_consent.sql` | Append-only support consent receipts |
| `infra/postgres/server-runtime/020_account_deletion_fences.sql` | Deletion marker and session tombstones |
| `infra/postgres/server-runtime/021_cloud_child_tombstone_provenance.sql` | Identifier-only child-source provenance |
| `infra/postgres/server-runtime/022_account_deletion_receipts.sql` | Opaque bounded reconciliation receipts |
| `server/tests/integration_e2e.rs` | HTTP coverage for consent, deletion, object cleanup, and reconciliation |

## Edge Cases Handled

- Account A request returns after an external switch to Account B.
- Account A1 receives a delayed 401 after Account A2 refreshed its tokens.
- A balance success or revocation event arrives after the owning generation was
  replaced.
- Listen verification is cached, then the owner, token pair, device, API
  origin, or credential generation changes.
- Continuous audio dequeues work immediately before an account switch.
- STT partial/final events and answer completion arrive after account change.
- Support consent is revoked while an object is reserved, uploading, or ready
  to finalize.
- Consent is later regranted; the previous consent epoch remains fenced.
- A session diagnostic delete races a same-session upload.
- Account deletion races a live object put or a sessionless account object.
- A delete HTTP response is lost after the server committed the deletion.
- A client restarts after preparing or completing only part of local deletion.
- The same session identifier exists under two accounts; deletion remains
  owner-scoped.
- A stale cloud bundle or local write attempts to recreate a session, child,
  artifact, or RAG row after a durable tombstone.

## How to Test

```bash
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh all
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- bash -c \
  'cargo +1.98 clippy --workspace --all-targets -- -D warnings &&
   cargo +1.98 clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings'
(cd crates/cue-dashboard/ui && npm test && npm run build)
```

The complete root and server Rust suites passed in the disposable launcher.
The server run included 839 unit tests and 80 HTTP integration tests. Strict
Rust 1.98 Clippy passed for both workspaces, and the dashboard passed 35 tests
across six files plus TypeScript/build verification. The five Jobs workspaces
also passed 489 tests (automation 198, browser 100, runner 50, workflows 54,
portal 87), their typechecks, and production builds. The temporary Rust
workspaces were removed after each run.

The hermetic product smoke passed daemon/overlay startup, transcript,
instructions, context, memory, routing/cloud scaffolds, the signed-out provider
fence, action items, recap, archive, and confirmed shutdown. The first
strengthened observability acceptance rerun also passed trace/request UUID
round-trip, authenticated daemon trace logging, Phase 3 regressions, explicit
`DAEMON_PID` termination with exit status zero, and workspace cleanup. The root
workspace and server binary release builds also passed in isolated workspaces.
The final static-policy rerun, packaged physical artifacts, and final
independent whole-diff review remain pending.

## Known Limitations

- When a browser/device sign-in succeeds remotely but a later local
  compare-and-swap rejects it because another login replaced the account, the
  abandoned attempt's server refresh/device credential may remain until normal
  expiry or revocation. The P1 follow-up is an idempotent,
  capability-only `device_link_attempt_id`/session-abandon endpoint that revokes
  only that exact attempt. Broad device logout would risk removing valid newer
  authority and is explicitly not the solution.
- Support diagnostics contain only closed metadata. User transcript, question,
  answer, prompt, document, screenshot, path, URL, token, cookie, or provider
  response content is not part of this channel.
- Source readiness is not deployment. Production migrations, object-store
  deletion, packaged clients, and physical macOS/Windows account deletion still
  require release-environment certification.
