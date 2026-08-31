# Round 590 - Bluey Diagnostic Spine And Test Isolation

Date: 2026-08-31

## Executive Summary

Round 590 makes Bluey diagnosable without turning diagnostics into a private
content archive. A closed metadata-only event spine now correlates native input,
daemon work, audio/STT readiness, RAG, model connection and first text,
persistence, and native rendering. The hot path never waits for diagnostic disk
or network I/O.

The same round closes the account-replacement and deletion races around that
spine. Exact credential snapshots prevent delayed Account A work from clearing
or publishing against refreshed Account A2 or Account B. Explicit append-only
support consent, upload/delete tombstones, and capability-bound account deletion
receipts prevent stale uploads and sync hydration from reviving deleted data.

It also establishes a permanent disposable Rust-test launcher. Local tests no
longer need to recreate and retain 7 to 20 GiB per hour or inherit live
database/provider/keychain authority.

This is source readiness only. It is not merged, deployed, published, or
physically certified.

## What Changed

### Metadata-only latency diagnostics

- Added schema-versioned, closed events for overlay actions/lifecycle, answer
  acceptance/context/card/status/first text/completion/failure, native first and
  final rendering, audio start/readiness/first chunk/stop, STT connection/first
  partial/final, RAG completion, model attempt/connection/first event/first
  text/completion, persistence, and queue loss.
- Event payloads are limited to validated UUID correlation fields, closed
  provider/model/route/action/error labels, timestamps, durations, queue depth,
  attempts, generation/sequence, booleans, and bounded counts.
- Ordinary and terminal queues are separately bounded. Producers use
  `try_send`; one worker batches disk writes and records dropped/coalesced
  counts.
- The server independently validates exact schema/content-policy headers,
  rejects unknown fields and secret/content-shaped keys, and caps one bundle at
  4,096 events.

Primary evidence:

- `crates/cue-daemon/src/diagnostics.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/cloud/sync.rs`
- `server/src/api/support_diagnostic_schema.rs`
- `server/src/api/sync.rs`

### Explicit support consent and cleanup

- Settings exposes an explicit account-scoped metadata-only diagnostic sharing
  control and visible pending-cleanup state.
- The server records append-only grant/revoke receipts for policy
  `2026-08-30` and content policy `metadata_only`.
- Upload requires the persisted local consent bit, the operator-side upload
  enablement, and matching current server consent. An environment flag alone
  cannot authorize sharing.
- Revocation advances the local consent epoch, atomically disables new server
  reservations, tombstones session/account scope, and queues durable object
  cleanup. A later grant cannot resurrect the older epoch.

Primary evidence:

- `crates/cue-dashboard/src/commands.rs`
- `crates/cue-dashboard/ui/src/pages/Settings.tsx`
- `server/src/db/support_diagnostics.rs`
- `server/src/db/object_uploads.rs`
- `infra/postgres/server-runtime/019_support_diagnostic_consent.sql`
- `infra/postgres/server-runtime/020_account_deletion_fences.sql`

### Exact account and credential authority

- `CredentialSnapshot` binds owner, credential generation, API origin, device,
  and exact access/refresh pair in one atomic value.
- Custom `Debug` output redacts owner, API origin, device, email, and tokens.
- Background cleanup must call `clear_if_current` with the snapshot captured
  before its await/network boundary. Token refresh uses snapshot CAS.
- Balance, listen, audio/STT, cloud sync, answer work, RAG, overlay hydration,
  and account deletion revalidate owner/generation before publishing or
  mutating state.
- The default customer path remains Bluey's private local account profile; this
  round does not enable the OS Keychain.

Primary evidence:

- `crates/cue-cloud-client/src/tokens.rs`
- `crates/cue-cloud-client/src/client.rs`
- `crates/cue-core/src/config.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/cloud/balance.rs`
- `crates/cue-daemon/src/rag_indexer.rs`

### Deletion without resurrection

- Local account deletion first stops/fences capture and records a durable
  owner-bound purge intent.
- The server publishes `deletion_pending_at_ms` before enumerating objects and
  rejects new auth/data/upload authority for that account.
- Reserved and in-flight object puts cannot finalize after the fence.
- Opaque operation/capability hashes allow a response-lost delete to be checked
  without returning account identity or recreating authentication.
- Session and child tombstones dominate stale sync records. Identifier-only RAG
  provenance lets every owned device delete derived memory without restoring
  text or embeddings.

Primary evidence:

- `server/src/api/account.rs`
- `server/src/db/account_data.rs`
- `server/src/db/object_uploads.rs`
- `server/src/db/sync.rs`
- `infra/postgres/server-runtime/021_cloud_child_tombstone_provenance.sql`
- `infra/postgres/server-runtime/022_account_deletion_receipts.sql`
- `infra/migrations/013_cloud_session_tombstones.sql`

### STT privacy hardening

- `SttError` display/debug output is category-only.
- Transcript events, words, agreement, vocabulary, provider config, endpoint,
  and provider frame debug are shape-only or redacted.
- OpenAI and Deepgram error messages/raw payloads are not retained as error
  detail. Masked key fragments were removed from traces.
- Local Whisper reports only missing/spawn/protocol/invalid-event categories and
  event size.
- Sentinel tests prove transcripts, tokens, URLs, and paths do not appear in
  formatting or trace-field helpers.

Primary evidence:

- `crates/cue-core/src/stt.rs`
- `crates/cue-daemon/src/stt/openai.rs`
- `crates/cue-daemon/src/stt/deepgram.rs`
- `crates/cue-daemon/src/stt/factory.rs`
- `crates/cue-daemon/src/stt/whisper/mod.rs`

### Disposable test and smoke workspaces

- `scripts/run-bluey-tests.sh` creates one marked short temporary workspace and
  owns Cargo, SQLite, app data, config, runtime, logs, cache, and `TMPDIR`.
- It clears PostgreSQL, provider, cloud, billing, object-store, mail, Redis,
  Jobs, updater, installer, test-service, and helper authority inherited from
  the shell. Automated runs do not access the OS Keychain.
- The scrub includes `BLUEY_ACCESS_TOKEN`, Bluey API base/URL/host aliases,
  `FFMPEG_PATH`, `BLUEY_FFMPEG_PATH`, and the Bluey context-picker override.
  Poison assertions specifically cover the managed token, API base and host,
  generic FFmpeg path, and context picker.
- Cleanup stops the entire owned process group and runs after success, failure,
  `SIGHUP`, `SIGINT`, and `SIGTERM`. Marker and basename checks prevent broad
  deletion.
- The self-test proves success, exit-37 failure, signal, and ignoring-orphan
  cleanup.
- Product smoke uses an owned overlay protocol peer and unique loopback daemon
  port. Native platform builds remain separate certification gates.
- The first smoke exposed a debug helper-discovery mismatch: the owned override
  was selected but verified against the checkout/install root. The fix permits
  it only in debug builds, only below the exact canonical marked
  `bluey-tests.*` root, and never in release behavior. The focused
  missing-marker/inside/outside test passes.
- Opt-in pre-commit Clippy now enters the disposable launcher too.

Primary evidence:

- `scripts/run-bluey-tests.sh`
- `docs/TESTING-RUNBOOK.md`
- `scripts/smoke-test.sh`
- `scripts/observability-acceptance-smoke.sh`
- `scripts/check-bluey-ops-docs.sh`
- `AGENTS.md`

## Security And Privacy Decisions

- Support diagnostics never contain transcript, question, answer, prompt,
  audio, screenshot, document preview, local path, source URL, cookie, token,
  raw object key, or provider response body.
- Cloud session transcript/history sync remains a separate user-controlled
  product capability; it is not silently bundled into support diagnostics.
- Diagnostics upload is opt-in and account-scoped. Revocation and account
  replacement fail closed.
- No new Keychain access is introduced.
- HuddleMate research material remains available for the owner's final review,
  but no application code was copied or executed in this round. Bluey's public
  language continues to reject guarantees of invisibility or zero detection
  risk.

## Verification Snapshot

Passed before this document was drafted:

- isolated launcher success/failure/`SIGTERM`/orphan self-test;
- complete root Rust suite in a disposable workspace;
- complete server suite: 839 unit and 80 HTTP integration tests plus auxiliary
  suites;
- root and server Rust 1.98 strict Clippy for all targets;
- Cue Core STT 13-test and Cue Daemon STT 86-test focused suites;
- dashboard 35 tests across six files, TypeScript check, and production build;
- Jobs 489 tests across automation, browser, runner, workflows, and portal,
  plus all five typechecks and production builds;
- macOS arm64/x86_64 overlay plus audio/Whisper/picker source and build gates;
- Windows overlay protocol/capture/audio tests and MinGW source/build gates;
- active-runbook documentation guard and final documentation diff check;
- cleanup of every completed `bluey-tests.*` workspace;
- hermetic Bluey smoke through daemon/overlay, transcript, context, memory,
  routing/cloud scaffolds, signed-out provider fence, action-items, recap,
  archive, and confirmed shutdown;
- strengthened observability acceptance rerun for trace/request UUID
  round-trip, authenticated daemon trace, Phase 3 regressions, explicit
  `DAEMON_PID` termination with exit status zero, and cleanup of
  `/private/tmp/bluey-tests.j1T1G7`;
- isolated `cargo +1.98 build --workspace --release` and
  `cargo +1.98 build --manifest-path server/Cargo.toml --bins --release`;
- a repeated focused debug-override ownership test and a warning-free
  `cargo +1.98 check -p cue-daemon --release` after compiling the marked
  test-workspace helper only under `cfg(any(debug_assertions, test))`;
- cleanup of release-build workspaces `/private/tmp/bluey-tests.ZKLxrm` and
  `/private/tmp/bluey-tests.xpYbGC`.

Pending at draft time:

- final static release/policy rerun;
- final independent whole-diff verdict;
- packaged/signed exact-artifact smoke and physical macOS/Windows certification.

Pending items are not implied to have passed.

## Remaining P1

If a browser/device link succeeds on the server but the desktop rejects that
result because a newer account/profile generation won the local CAS, the
abandoned attempt can leave its exact server refresh/device credential alive
until normal expiry or revocation.

The follow-up must add a random `device_link_attempt_id` or session identifier
and an idempotent capability-only abandon endpoint. The client should abandon
the exact attempt before clearing local provisional state. Broad device logout
must not be used because it could revoke valid newer authority.

## Phase 625 Handoff

1. Finish the static and final review gates and update the provisional review
   verdict.
2. Merge through a feature-branch pull request only after those source gates
   pass; do not deploy from the dirty worktree.
3. Use the diagnostic milestones to benchmark accepted input to context ready,
   model connected, first provider event, first usable text, and native first
   paint.
4. Keep persistence, session recap, cloud sync, and embeddings outside the
   first-usable-text critical path.
5. Refine the macOS/Windows overlay without enlarging the 112 by 30 compact
   pill. Preserve light/dark readability, clear onboarding, and the distinction
   between route depth (`Auto`, `Quick`, `Thorough`) and answer format
   (`Default`, `Concise`, `Bullets`, `STAR`, `Search`).
6. Build once and certify the exact packaged macOS and Windows artifacts before
   any customer release.

## Deployment

No merge, production migration, server deployment, release publication, flag
change, or physical package installation was performed by Round 590.
