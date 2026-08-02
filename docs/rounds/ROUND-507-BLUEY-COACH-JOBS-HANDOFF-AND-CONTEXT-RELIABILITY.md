# Round 507 — Bluey Coach, Jobs handoff, and context reliability

Date: 2026-07-12  
Status: Implementation and automated verification complete; signed runtime canary remains

Research inputs:

- [Round 504 DMG cross-product audit](ROUND-504-BLUEY-DMG-CROSS-PRODUCT-AUDIT-AND-LIVE-CAPTURE-HANDOFF.md)
- [DMG audit index](../research/dmg-audit/INDEX.md)
- Reformed recovery index: `/Users/uno/Downloads/dmg_backtrack_code/reformed/INDEX.md`
- Second-pass recovery report: `/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md`

## Executive summary

The five recovered products support one clear product decision: Littlebird is the strongest model for a coherent workspace and readiness experience, but it is not sufficient by itself. The best Bluey composition is:

| Source | Pattern adopted | Bluey improvement |
|---|---|---|
| Littlebird | Durable activity context, explicit readiness and failure states | Coach setup, visible context policy, durable Jobs import/retry, operational UI states |
| Cluely | Compact session/overlay lifecycle | Reuse Bluey's existing authenticated overlay and session model instead of adding a second shell |
| LockedIn | Interview/coding/system-design presets | Seven typed Coach modes with bounded role, company, instructions, and priority questions |
| ParakeetAI | Native activity/audio contracts and AEC evidence | Truthful PCM contract, structured helper lifecycle, bounded diagnostics; AEC held behind a measured rollout gate |
| Final Round | Transactional lifecycle, recovery, structured panels | Atomic active-session persistence, crash recovery, HMAC-bound import authorization, idempotent receipts and retry-safe import |
| Bluey | Jobs, evidence, leases, account isolation, RAG and receipts | Preserved as the authoritative foundation; no duplicate automation or billing stack |

This round delivers a coherent Bluey-owned slice rather than a visual clone:

1. A typed Coach profile used by both daemon and dashboard answer paths.
2. A polished Coach UI with real persistence, validation, accessibility, and verified Jobs provenance.
3. Consent-first screenshot capture with a private preview, account/session-bound exact-once attach, local-only retention and a narrow renderer asset scope.
4. A 90-second, random, single-use, account/audience-bound Jobs-to-desktop handoff.
5. Durable pending import and retry without mixing job context into a live or non-empty session.
6. Clone-serialized, atomic private meeting writes with backup recovery, bounded artifact cleanup and corrupt-file quarantine.
7. Structured native-helper readiness/errors, bounded stderr draining, correct i16 metadata, and repeatable meeting-app transitions.

Bluey is now stronger than the compared products at the application-to-interview truth boundary: the desktop Coach can be grounded in the exact submitted resume, receipt and evidence ledger without placing credentials, resume text, answers, receipt contents or application identifiers in the custom URL.

## Evidence-backed findings

### Why Littlebird helps most

Observed recovered Littlebird source shows projects with durable instructions, files, conversations and explicit ingestion states; meetings have separate recording, transcript and summary state; onboarding checks microphone and optional integrations. See the exact evidence register in [Round 504](ROUND-504-BLUEY-DMG-CROSS-PRODUCT-AUDIT-AND-LIVE-CAPTURE-HANDOFF.md) and `/Users/uno/Downloads/dmg_backtrack_code/reformed/littlebird-0.81.11/`.

Inference: Littlebird feels more complete because related work has one visible home. Bluey previously exposed stronger primitives as disconnected Sessions, Live, Answers and Jobs screens.

Decision: use Littlebird's cohesion and state clarity, not its renderer/backend boundary or code. The present Coach is the smallest truthful first workspace: Bluey currently persists one active `AssistantProfile`, so labeling it a multi-workspace system would overstate the backend.

### Why the other DMGs still matter

- Final Round exposes the clearest start/active/end/recovery model and bounded realtime failure behavior.
- ParakeetAI contains the strongest recoverable native audio/AEC evidence, including exact Sonora dependency source and process-owned input evidence.
- Cluely supplies the best compact overlay/session mental model.
- LockedIn supplies useful preset vocabulary but an unsuitable generic IPC/remote-input boundary.
- Bluey remains materially stronger at job discovery, browser identity isolation, interventions, leases, irreversible-submit control, evidence and receipts.

## Implemented architecture

### Typed Coach profile

`AssistantMode` and `AssistantProfile` are defined in `crates/cue-core/src/assistant.rs:12` and `:81`. The profile supports:

- general, interview, behavioral interview, coding, system design, meeting and writing modes;
- role, company, multiline user instructions and up to 12 priority questions;
- immutable application, receipt and resume provenance;
- normalization, Unicode-aware limits, safe identifiers and schema versioning.

`MeetingRecord` embeds the profile at `crates/cue-core/src/meeting.rs:318`, and cloud sync serializes/restores it. Provider-visible context deliberately excludes provenance identifiers. Role, company and priority questions remain lower-trust `AnswerContext`; only the selected mode and explicit user-authored custom rules gain instruction authority (`crates/cue-daemon/src/app.rs:13467-13506`). Normal Coach edits must present the exact current Jobs source or reload, so stale UI state cannot clear or replace verified provenance.

Jobs import uses the single `JobsHandoffImport` IPC variant at `crates/cue-core/src/ipc.rs:105`. `JobsHandoffImportRequest` and HMAC authorization start at `crates/cue-core/src/jobs_handoff.rs:23,93`; `import_jobs_handoff` at `crates/cue-daemon/src/app.rs:16281` verifies and commits context plus profile as one daemon-side session transaction.

Both answer paths now consume the same effective mode/profile/session instructions:

- daemon provider runtime: `crates/cue-daemon/src/app.rs:13467-13532`;
- dashboard Auto and legacy paths: `crates/cue-dashboard/src/commands.rs:3214-3320`;
- answer/suggestion prompt composition: `crates/cue-daemon/src/llm/answer.rs:36-44` and `suggest.rs:8-16`.

### Coach UX

The dashboard exposes `/coach` and redirects the old `/prompts` route. `crates/cue-dashboard/ui/src/pages/Coach.tsx` includes:

- seven accessible mode cards;
- role/company, bounded instructions and priority-question editing;
- real loading, dirty, save, retry and error states;
- immutable linked-job provenance notice;
- successful/imported/recovered Jobs banners and profile rehydration;
- stale in-flight save recovery.

The boundary commands are `get_assistant_profile` and `save_assistant_profile` at `crates/cue-dashboard/src/commands.rs:1076-1092`.

### Consent-first screen context

`crates/cue-dashboard/ui/src/pages/ScreenContext.tsx` replaces the placeholder screenshot route. The flow is:

```text
choose source → private preview → visible review/title → attach or discard
```

Properties:

- capture never attaches or uploads automatically;
- region/window and full-screen actions are explicit;
- Windows disables unsupported region/window capture;
- attach is unavailable until the preview renders;
- each preview has a private consent record binding the operation UUID, capture-time account/session, exact preview hash and deterministic retained path;
- attach copies through no-follow file handles, then the daemon revalidates account, session, path, hash and normalized title while holding the meeting lock;
- the operation UUID is also the artifact UUID, so a lost response safely replays to an active or archived receipt rather than creating a duplicate;
- leaving the page requests deletion only for an unambiguous unattached preview;
- ambiguous receipt outcomes keep retry material and disable discard until the same Attach operation reconciles;
- discard can remove only the preview/binding and never a retained or possibly committed image;
- attached screenshots are `local_only`: excluded from object upload, session/audit attachment metadata, background cloud sync and managed RAG;
- the exact consented bytes are opened no-follow and re-hashed immediately before an explicit vision request and before thumbnailing;
- after the one-shot vision use, persisted `vision_send_consumed` routes even explicit reselection to text memory rather than silently uploading again.

Renderer-side enforcement starts at `crates/cue-dashboard/src/commands.rs:1128,1207,1257`. The daemon commit and replay boundary is `attach_screenshot_context_exactly_once` at `crates/cue-daemon/src/app.rs:15394`; final vision re-hashing is at `:12973`, and bounded orphan cleanup starts at `:15844`. `ContextCloudSyncPolicy` and the persisted artifact integrity/one-shot fields start at `crates/cue-core/src/meeting.rs:125,168`.

The Tauri asset protocol is enabled only for Bluey's `capture-previews` directory, including the legacy `cue` location (`crates/cue-dashboard/tauri.conf.json`). Attached captures are outside renderer scope.

### Secure Jobs-to-Coach handoff

Issue and redeem endpoints are implemented in `server/src/api/jobs_handoff.rs:44-137`, with storage in `server/src/db/jobs_handoffs.rs`.

The protocol is:

```text
submitted application
  → authenticated issue
  → 32-byte OsRng nonce, 90-second TTL
  → bluey://jobs/interview-prep?nonce=<only-value>
  → authenticated desktop redeem
  → atomic one-time consume
  → local binding validation
  → durable private pending record
  → redacted model context + immutable Coach provenance
```

Security properties:

- raw nonces are returned once and never stored in server persistence;
- the database stores SHA-256 nonce digests and encrypted snapshots;
- redemption is account-, application- and audience-bound;
- expiry/replay/account mismatch returns no snapshot;
- issue/redeem bodies are capped and responses are `no-store`;
- the custom URL permits exactly one `nonce` parameter and no fragment, credentials, token or content;
- the portal revalidates the returned custom URL before navigation;
- the desktop independently revalidates audience, application, receipt, resume and document-hash bindings;
- only coaching-relevant job/resume/answer/outcome data enters model context;
- application IDs, receipt IDs, resume IDs, evidence IDs and hashes stay out of provider-visible context;
- a live or non-empty session blocks import instead of silently mixing contexts;
- consumed handoffs are saved locally before daemon import and remain retryable after restart;
- idempotency binds the opaque import ID, account, context SHA-256 and exact profile provenance; an archived replay returns an inactive receipt instead of creating a second active context;
- normal Coach saves cannot erase verified provenance.

Primary code:

- Portal action: `jobs/portal/src/views/ApplicationsView.tsx:146-160`.
- Portal URL validator: `jobs/portal/src/lib/bluey-handoff.ts`.
- Cloud client: `crates/cue-cloud-client/src/client.rs:239-254`.
- Desktop deep-link, durable pending import and result-retention boundaries: `crates/cue-dashboard/src/lib.rs:763-1011,1301-1389,1683-1774`.
- App-level event/retry UX: `crates/cue-dashboard/ui/src/components/JobsHandoffProvider.tsx`.
- SQLite migration 0026 and Postgres migration `server/migrations/postgres/003_jobs_bluey_handoffs.sql`.

Transient handoffs are intentionally omitted from account export because they are short-lived capabilities duplicating already-exported submitted-application data. Account/application foreign keys cascade their deletion.

### Crash-safe local session persistence

Meeting JSON is no longer written in place. `MeetingStore` at `crates/cue-daemon/src/storage.rs:25`, its staged writer at `:430` and platform atomic replacement at `:553-622` now:

- shares one operation lock across every clone so reads, writes, recovery, archive, rename and delete cannot observe each other's intermediate state;
- creates no-follow, same-directory temporary files with `0600` from creation and bounded task-owned names;
- writes and `sync_all`s before publishing;
- atomically publishes the prior valid primary as the one backup without first removing the primary;
- atomically replaces the primary with POSIX `rename` or Windows `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`;
- fsyncs the parent directory on Unix;
- restores a valid backup after interrupted legacy rotation and copies corrupt bytes to quarantine before replacement;
- bounds startup cleanup of stale Bluey-owned temporary and quarantine artifacts;
- clears active backups on archive/delete so ended sessions cannot resurrect.

Bluey-owned Jobs context, capture, prepared-image and thumbnail files are deleted when their context/session is removed. Cleanup requires exact deterministic names, an anchored app-owned directory and a regular non-symlink file; external JSON/images and symlink targets are not deleted.

### Native audio and meeting activity

The native helper contract now says what the bytes actually are: 16-kHz, mono, i16 (`crates/cue-core/src/audio.rs:242-249`).

Both native helpers emit newline-delimited lifecycle events:

- macOS: `native/macos/cue-audio/Sources/cue-audio/main.swift:11-68`;
- Windows: `native/windows/cue-audio/main.c:209-252`.

The daemon drains stderr concurrently through a bounded incremental parser (`crates/cue-daemon/src/audio/helper_diagnostics.rs`) and consumes exact ready, permission-denied, error and stopped events at `crates/cue-daemon/src/app.rs:5271-5428`. Raw helper messages do not cross the renderer boundary.

Meeting-app detection now publishes active/inactive transitions and can re-emit the same app after an inactive gap (`crates/cue-daemon/src/cloud/meeting_detect.rs:31-113`). It remains advisory and never starts recording by itself.

## Security and privacy findings

Resolved in this round:

- the Jobs custom URL contains only the opaque one-time nonce, never account credentials, resume/application content or internal identifiers;
- no raw nonce in server persistence;
- no cross-account/audience redemption;
- no silent context mixing with active work;
- no provenance loss through a stale Coach save;
- no provenance IDs in provider prompts;
- no automatic screenshot attachment;
- no arbitrary screenshot discard path or post-consent byte substitution;
- no screenshot background upload, managed indexing or silent second vision send;
- no broad asset-protocol filesystem scope;
- no in-place session JSON overwrite;
- no missing-primary window during current meeting-store writes;
- no unbounded native-helper stderr pipe/tail;
- no raw native error detail in renderer messages.

Still open:

- Jobs import is HMAC capability-authenticated, but most generic local loopback daemon mutations are not;
- a same-OS-user process can read the Unix `0600` capability, and an explicit Windows ACL invariant has not been verified;
- capability loss or rotation invalidates already-persisted pending imports;
- pending handoff, local context and result files are private but not encrypted; pending and unacknowledged result files expire after seven days;
- cloud restore preserves the Coach profile but intentionally does not restore local import-ID/context-hash idempotency metadata;
- full Jobs evidence remains local and is never object-uploaded; only bounded redacted preview context syncs, with absolute local paths replaced by logical labels;
- server rows retain encrypted snapshots for the bounded retired-row window and cleanup is opportunistic on issue, not a scheduled retention worker;
- desktop account tokens still default to a private account file rather than OS secure storage;
- full multi-workspace deletion/sync policy does not exist because multi-workspace persistence is not implemented yet;
- the dashboard updater endpoint and public key remain placeholders;
- signed runtime capture, permission and cross-account handoff canaries are required before release.

## AEC decision

AEC was not enabled in this round. This is a quality gate, not a missing analysis result.

The recovered Parakeet/Sonora evidence shows that production AEC needs one coordinator owning both streams, render-before-capture ordering, exact 10-ms framing, bounded holdback, explicit delay handling and engine reset on session/device changes. Bluey's current helpers are independent pipes without capture timestamps. Enabling AEC now would risk microphone-prefix loss, drift and double-talk regressions.

Next implementation requirements:

1. Add monotonic timestamps or a tagged dual-source helper protocol.
2. Split capture production from STT transport.
3. Add 320-byte i16 frame assembly with odd-byte carry and bounded channels.
4. Integrate pinned Sonora behind `off`, `shadow`, `on` modes.
5. Fail open to raw mic; never modify system audio sent to STT.
6. Gate release on latency, ERLE, double-talk WER, drift and zero-prefix-loss tests.

Sonora is BSD-3-Clause and its notice must be added to release materials if selected. The reviewed exact revision requires a newer Rust toolchain and still needs Windows MSVC/release-link verification.

## Verification

Passed:

- `cargo test -p cue-core --lib`: 98 passed.
- `cargo test -p cue-cloud-client`: 26 passed.
- `cargo test -p cue-daemon --lib`: 385 passed, 5 hardware tests ignored.
- `cargo test -p cue-dashboard`: 33 passed.
- Storage concurrency/crash/cleanup subset: 12 passed.
- Screenshot attach/replay/final-byte subset: 3 passed; discard-retention subset: 1 passed.
- Server `cargo test --lib`: 332 passed.
- Server `cargo test --test jobs_handoff_api`: 1 passed.
- Dashboard UI `npm test`: 48 passed; production build passed.
- Jobs portal `npm test`: 24 passed; typecheck and production build passed.
- Main-workspace and server Clippy with `-D warnings`: passed.
- macOS debug and strict-concurrency release `swift build` for `cue-audio`: passed.
- Windows helper cross-compile with MinGW `-Wall -Wextra -Werror`: passed.
- Windows GNU daemon library cross-check: passed.
- All five owner-supplied DMG SHA-256 values match the audit index.
- All 21,737 second-pass derived ledger entries and all 211,607 immutable raw-recovery ledger entries pass SHA-256 verification; the reformed top-level index/report anchor passes.
- Markdown local-link audit: all targets across 43 Markdown files total (the DMG audit pack plus this round document) exist.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.

Automated tests did not install applications, initiate a real screen capture, use production credentials or mutate production data.

## Cross-application feature matrix

`Bluey stronger`, `Equivalent`, `Partial` and `Missing` describe the post-Round-507 Bluey implementation relative to each supplied artifact. Static absence is only “not observed in the supplied desktop artifact”; opaque server behavior remains unknown.

| Capability | Cluely | Littlebird | LockedIn | ParakeetAI | Final Round | Bluey implementation decision |
|---|---|---|---|---|---|---|
| Jobs discovery, ATS execution, leases and receipts | Bluey stronger | Bluey stronger | Bluey stronger | Bluey stronger | Bluey stronger | Preserve Bluey's existing Jobs authority. |
| Submitted application → desktop coaching | Bluey stronger | Bluey stronger | Bluey stronger | Bluey stronger | Bluey stronger | Account-bound, one-time handoff with exact receipt/resume evidence. |
| Coherent named workspace | Partial | Partial | Partial | Partial | Partial | Current-session Coach is truthful; named durable workspaces remain P1. |
| Interview modes/presets | Equivalent | Bluey stronger | Equivalent | Equivalent | Partial | Seven typed modes are live; specialized versioned panels remain P1. |
| Live mic/system capture and helper lifecycle | Bluey stronger | Equivalent | Equivalent | Equivalent | Equivalent | Keep Bluey's dual-source pipeline and structured diagnostics. |
| Screenshot consent and local retention | Bluey stronger | Bluey stronger | Bluey stronger | Bluey stronger | Bluey stronger | Preview-first, exact-byte consent, local-only background policy and one-shot provider use. |
| Crash/retry/idempotency | Bluey stronger | Bluey stronger | Bluey stronger | Bluey stronger | Equivalent | Clone-serialized meeting writes plus durable capability-bound Jobs recovery. |
| Native AEC | Missing | Missing | Missing | Missing | Missing | Do not enable until timestamped dual-stream shadow benchmarks pass. |
| Readiness onboarding/test recording | Partial | Partial | Partial | Partial | Partial | Operational permissions and 30-second test recording remain P1. |
| Email/calendar outcome context | Partial | Partial | Partial | Partial | Partial | Ship one least-scope production provider before claiming parity. |
| Local secret/mutation boundary | Partial | Partial | Partial | Bluey stronger | Partial | Jobs mutation is authenticated; generic daemon mutation and account-token storage remain P1. |

## Cross-application priority map

| Priority | Bluey change | Basis | State |
|---|---|---|---|
| P0 | Typed Coach profile consumed by all answer paths | LockedIn + Bluey modes | Implemented |
| P0 | Secure submitted-application desktop handoff | Bluey receipts + Final Round single-use state | Implemented |
| P0 | Durable import/retry and crash-safe meeting storage | Littlebird + Final Round recovery | Implemented |
| P0 | Consent-first screen context | Recurring competitor screenshot UX, Bluey privacy boundary | Implemented |
| P0 | Structured native helper diagnostics | Littlebird/Parakeet readiness evidence | Implemented |
| P1 | Named multi-workspaces with Activity/Context/Instructions/Artifacts/Linked Job | Littlebird projects | Not yet implemented |
| P1 | Operational onboarding and 30-second test recording | Littlebird readiness checklist | Not yet implemented |
| P1 | Versioned coding/system-design artifact sections | Final Round panels | Not yet implemented |
| P1 | Least-scope calendar context | Littlebird/Cluely calendar flows | Not yet implemented |
| P1 | Authenticated/bounded local daemon mutation channel | Cross-product security audit | Not yet implemented |
| P1 | OS-secure account-token migration | Cross-product security audit | Not yet implemented |
| P2 | Timestamped AEC shadow rollout | Parakeet/Sonora | Gated by measurements |
| P2 | Process-owned microphone activity on macOS/Windows | Parakeet native evidence | Designed, not implemented |
| P2 | Collaboration/sharing | Littlebird workspace model | After tenant/deletion boundaries |

## Reuse and provenance notes

- The recovery corpus remains at `/Users/uno/Downloads/dmg_backtrack_code/reformed/` with exact/derived checksum manifests.
- The supplied artifacts are owner-authorized research inputs. Exact source, exact packaged slices, mechanically reconstructed code, inferred interfaces and native pseudocode remain separately labeled; these categories are not interchangeable.
- No recovered minified renderer, product prompt, private endpoint body, native binary or N-API wrapper was pasted into this Bluey slice. The implementation uses recovered behavior and architecture evidence to produce one typed Bluey-owned Rust/React/native design.
- Direct reuse remains possible from the exact-source candidates retained in the recovery tree. Each selected file should carry its source hash and ownership record; third-party dependencies should also carry their license/notice, SBOM and target-build verification.
- Server implementations, deleted pre-minification symbols/types/comments/tests, Git history and never-packaged native source remain unrecoverable bytes. Their observable behavior can be reimplemented; their original bytes cannot be truthfully recreated.

## Unknowns requiring runtime validation

- Signed/notarized macOS helper readiness and permission event behavior on allowed/denied/revoked permissions.
- Windows MSVC helper build plus real WASAPI loopback/microphone permission behavior.
- Authenticated portal → custom URL → desktop redemption against a staging account, including expiry, replay and account mismatch UI.
- Real screenshot preview rendering under packaged Tauri asset protocol on macOS and Windows.
- Sleep/wake, device switch, helper crash and long-running transcript drift.
- Comparative first-audio, first-transcript, first-answer, CPU and recovery timing on identical hardware/network/input.

## Concrete handoff for the next agent

Do not reopen recovery or replace this slice. Start with signed staging validation:

1. Build the dashboard and native helpers through release workflows.
2. Use two staging accounts to prove issue/redeem ownership, replay and expiry behavior.
3. Verify a submitted application opens Coach with the exact role/company, source notice and attached redacted context.
4. Verify an active/non-empty session blocks import, then End Session → Retry succeeds without duplicate context.
5. Verify normal Coach edits preserve source provenance.
6. Exercise screenshot selection/full-screen, visible preview, attach, discard, navigation cleanup and deletion.
7. Exercise mic/system permission allow/deny/revoke and confirm structured source-specific errors.
8. Record first-audio/transcript/answer latency and attach the results to Round 508.

After that canary, the smallest production expansion is named local Workspaces plus operational onboarding:

1. Introduce a versioned `WorkspaceRecord` in `cue-core` with owner, title, mode/profile, linked Jobs source, activity references, context/artifact references and deletion state. Keep `MeetingRecord` as session history rather than overloading it into a workspace.
2. Add daemon CRUD through narrow typed IPC with owner checks and idempotent delete. Migrate the current active `AssistantProfile` into one default local workspace without inventing cloud state.
3. Change Coach into five explicit sections: Activity, Context, Instructions, Artifacts and Linked Job. Reuse the current loading/dirty/retry patterns and do not claim collaboration until tenant and deletion boundaries exist.
4. Add permission readiness and a 30-second local test recording that displays actual source/format/helper diagnostics and deletes test audio by default.
5. Add versioned coding and system-design artifacts using Bluey's existing `CueCardArtifact` types; do not create a second answer engine.
6. Add contract, owner-transition, deletion, crash-recovery and local-only sync tests before UI polish.

Do not begin AEC `on` mode until timestamped coordination and shadow benchmarks satisfy the gates above.
