# Round 509 — Bluey workspaces, readiness, and local security

Date: 2026-07-12
Repository: `/Users/uno/Downloads/cue-bluey-jobs`
Branch: `codex/bluey-jobs-20260710`
Status: implementation and scoped current-tree automated verification complete; release gates remain

Research lineage: the [DMG audit index](../research/dmg-audit/INDEX.md) contains
the current five-product matrix through Round 509; [Round 507](ROUND-507-BLUEY-COACH-JOBS-HANDOFF-AND-CONTEXT-RELIABILITY.md)
records the preceding Coach, Jobs handoff, screenshot, persistence, and native
audio slice.

## Executive summary

This round turns the single Coach profile from Round 507 into a durable,
owner-scoped workspace system and hardens the local boundaries that make that
system safe to operate. The result is one coherent path from account sign-in and
audio readiness through Jobs-grounded coaching:

```text
OS credential store + stable account id
  -> owner-scoped workspace
  -> seven-mode Coach profile + reference-only history
  -> receipt-bound Jobs context
  -> authenticated loopback daemon mutation
  -> active meeting and answer runtime
```

The implemented code now provides:

1. Durable workspace metadata with typed list/get/create/update/activate/delete,
   optimistic revisions, soft deletion, an owner-bound active pointer, bounded
   storage, and atomic private files.
2. A workspace-first Coach that exposes seven modes, explicit activation,
   create/rename/delete, stale-write recovery, Jobs refresh handling, and
   reference-only Activity, Context, Artifacts, Instructions, and Linked Job
   sections.
3. A default OS-credential-store token policy. Plaintext account-file tokens are
   available only after explicit configuration. A stable cloud `account_id`,
   atomic profile writes, an in-process plus cross-process operation lock, and
   refresh compare-and-swap protect owner identity and prevent stale refreshes
   from resurrecting logout or overwriting a newer login.
4. A fixed-duration, local-only readiness probe with bounded in-memory PCM and
   diagnostic output, truthful privacy flags, structured source states, trusted
   helper-path checks, and an exact `ready` then `duration_complete` stop
   contract.
5. Authenticated loopback daemon IPC: only Ping is public; protected reads,
   mutations, and shutdown require a per-boot bearer and unique request id.
   Request size, accepted client response size, deadlines, concurrent clients,
   and replay memory are bounded. Windows deliberately fails closed until
   owner-only IPC ACLs are implemented.
6. Strict account isolation in workspace/session operations. A malformed account
   file is an error, not a silent downgrade into the local-owner scope.
7. Workspace meeting scans moved off the async runtime with `spawn_blocking`.

This is a production-quality architecture slice, not proof that Bluey is
universally faster than every compared application. The remaining performance
claim requires an indexed workspace metadata path and measured cold/warm p50,
p95, and p99 results. Windows desktop release is blocked by the intentionally
fail-closed IPC capability implementation.

## Observed facts and bounded inferences

### Observed facts

- `WorkspaceRecord` persists owner, revision, profile, instructions, bounded
  reference collections, linked Jobs metadata, deletion state, and timestamps
  (`crates/cue-core/src/workspace.rs:20-48`). It validates that linked Jobs
  metadata and profile provenance are the same source
  (`crates/cue-core/src/workspace.rs:86-147`).
- A meeting stores both `workspace_id` and `owner_account_id`; workspace views
  derive references from meeting records instead of copying transcript, context,
  or artifact bodies (`crates/cue-core/src/meeting.rs:317-343`,
  `crates/cue-daemon/src/app.rs:7292-7358`).
- The daemon exposes typed workspace requests and revision-bound delete
  (`crates/cue-core/src/ipc.rs:104-120`) and owner-checks every workspace
  operation before returning a view (`crates/cue-daemon/src/app.rs:2609-2731`).
- Coach displays all seven modes defined by the shared UI contract
  (`crates/cue-dashboard/ui/src/pages/assistantProfile.ts:8-16,45-95`) and five
  workspace sections (`crates/cue-dashboard/ui/src/pages/Coach.tsx:671-683`).
- The default token path is the OS credential store; file storage requires an
  explicit environment policy and secure-store failures do not silently switch
  to plaintext (`crates/cue-cloud-client/src/tokens.rs:1-12,212-247`).
- The credential bundle includes access, refresh, email, and stable account id
  (`crates/cue-cloud-client/src/tokens.rs:575-617`). Routine `Debug` output
  redacts both bearer values (`crates/cue-cloud-client/src/tokens.rs:35-53`).
- The readiness result carries inspectable `retained_audio=false`,
  `transcribed=false`, and `uploaded=false` invariants
  (`crates/cue-core/src/audio.rs:59-81`). The UI rejects a response that violates
  them (`crates/cue-dashboard/ui/src/pages/onboardingReadiness.ts:84-133`).
- IPC accepts only numeric loopback addresses
  (`crates/cue-core/src/ipc_auth.rs:220-230`), treats only Ping as public
  (`crates/cue-core/src/ipc_auth.rs:158-213`), and refuses to publish a Windows
  capability without verified owner-only ACLs
  (`crates/cue-core/src/ipc_auth.rs:232-270`).

### Inferences, explicitly labeled

- **Product inference:** grouping a persistent profile, instructions, activity,
  context, artifacts, and Jobs provenance under one named workspace should make
  Bluey's Coach easier to understand than the previous disconnected screens.
  This is consistent with the Littlebird cohesion finding in
  [Round 507](ROUND-507-BLUEY-COACH-JOBS-HANDOFF-AND-CONTEXT-RELIABILITY.md), but
  it still requires usability observation to quantify.
- **Reliability inference:** optimistic revisions plus a UI three-way merge
  should reduce silent lost edits. The code proves conflict detection and merge
  policy, not the frequency of real multi-view conflicts.
- **Performance inference:** moving full meeting deserialization into
  `spawn_blocking` protects async executor responsiveness, but it does not make
  an O(number of meeting files) scan cheap. No broad speed claim is justified
  until the index and benchmarks in the release gates are complete.
- **Security inference:** per-boot bearer authentication materially raises the
  bar above unauthenticated loopback mutations. It is not server authentication,
  sandboxing, or a defense against a process already running as the same OS user.

## Implemented architecture

### 1. Durable owner-scoped workspaces

The core schema places strict, Unicode-aware bounds on workspace fields and
reference counts (`crates/cue-core/src/workspace.rs:9-18`). Editable profile
fields are normalized before persistence, revisions are positive and monotonic,
and the linked-job/profile-source invariant is validated on every record
(`crates/cue-core/src/workspace.rs:50-161`). `WorkspaceUpdateRequest` requires an
exact `expected_revision` and distinguishes unchanged, clear, and set instruction
operations (`crates/cue-core/src/workspace.rs:267-297`).

`WorkspaceStore` creates one stable default per owner, preserves an owner-scoped
active pointer, and serializes operations across clones
(`crates/cue-daemon/src/workspace_store.rs:28-105`). Create rejects forged Jobs
provenance, update rejects a stale revision or provenance substitution, and
delete requires the current revision before publishing a replacement active
pointer and writing a tombstone (`crates/cue-daemon/src/workspace_store.rs:129-275`).

Persistence properties:

- maximum 512 workspace records, 128 active workspaces per owner, and bounded
  retained tombstones (`crates/cue-daemon/src/workspace_store.rs:15-22`);
- unreadable records are isolated during list rather than making all other
  workspaces unreachable (`crates/cue-daemon/src/workspace_store.rs:380-418`);
- reads reject cross-owner access and validate file identity plus the complete
  schema (`crates/cue-daemon/src/workspace_store.rs:421-450`);
- writes use same-directory unique temporary files, private/no-follow creation,
  `sync_all`, atomic replacement, and directory sync
  (`crates/cue-daemon/src/workspace_store.rs:631-655`);
- tombstone compaction removes workspace metadata only; meeting records and their
  workspace ids remain intact (`crates/cue-daemon/src/workspace_store.rs:534-583`).

The authoritative payload remains the meeting. Workspace Activity, Context, and
Artifacts contain identifiers, labels, states, and timestamps only. The schema
comments prohibit payload duplication (`crates/cue-core/src/workspace.rs:31-43`),
and the derived view enforces reference limits while preserving a persisted Jobs
link or backfilling only one whose source matches the current profile
(`crates/cue-daemon/src/app.rs:7292-7358`).

### 2. Workspace-first Coach and Jobs provenance

The dashboard-to-daemon bridge preserves the tagged IPC response shape for all
six workspace operations (`crates/cue-dashboard/src/commands.rs:1187-1253`). The
TypeScript boundary validates schema version, UUIDs, safe positive revisions,
field bounds, all nested reference shapes, timestamps, deletion state, and exact
profile/linked-job source equality (`crates/cue-dashboard/ui/src/lib/workspaces.ts:167-315`).

Coach behavior is deliberately explicit:

- loading/error/retry and reference-only disclosure
  (`crates/cue-dashboard/ui/src/pages/Coach.tsx:515-549`);
- viewing a workspace does not activate it; switching a dirty draft requires a
  decision (`crates/cue-dashboard/ui/src/pages/Coach.tsx:282-322`);
- create does not silently replace the active workspace
  (`crates/cue-dashboard/ui/src/pages/Coach.tsx:330-356`);
- save sends the exact base revision and ignores stale in-flight results if the
  selected workspace, Jobs revision, or draft generation changed
  (`crates/cue-dashboard/ui/src/pages/Coach.tsx:359-420`);
- a stale response reloads the current revision and three-way merges only
  non-conflicting draft fields (`crates/cue-dashboard/ui/src/pages/Coach.tsx:424-453`);
- delete presents the candidate revision, verifies the returned id, and tells the
  user that linked meetings remain (`crates/cue-dashboard/ui/src/pages/Coach.tsx:466-496`);
- Activity, Context, Instructions, Artifacts, and Linked Job are visible sections,
  and all seven typed modes remain selectable per workspace
  (`crates/cue-dashboard/ui/src/pages/Coach.tsx:671-772,884-922`).

Jobs provenance is not editable workspace decoration. Ordinary create/update
cannot introduce or replace `AssistantProfile.source`
(`crates/cue-daemon/src/workspace_store.rs:129-151,164-200`). The verified Jobs
handoff checks the signed authorization, exact current account, context hash and
active-session boundary before applying the receipt-backed profile
(`crates/cue-daemon/src/app.rs:17228-17280`). It preserves existing user-authored
custom rules/questions, verifies the source, binds the import id and context
SHA-256, then synchronizes the selected workspace and meeting
(`crates/cue-daemon/src/app.rs:17338-17378`).

### 3. Account credentials and stable owner identity

`Tokens` now carries the stable cloud account id alongside access, refresh, and
email; the id is validated and bearer fields are redacted
(`crates/cue-cloud-client/src/tokens.rs:35-92`). `AccountConfig::linked_owner_id`
prefers `cloud_account_id`, falls back to a non-local user id for migration, and
does not depend on bearer-token fields (`crates/cue-core/src/config.rs:12-66`).
This means an email change does not move one cloud account's local data to a new
owner scope when the stable id is present.

The token policy and transaction boundary are:

- default `SecureOnly`; explicit `BLUEY_TOKEN_STORE=file` or plaintext opt-in is
  required for account-file credentials
  (`crates/cue-cloud-client/src/tokens.rs:212-247`);
- one process lock is shared per account file and one private lock file is held
  for up to five seconds across Bluey processes
  (`crates/cue-cloud-client/src/tokens.rs:25-33,256-338`);
- secure save writes the OS credential bundle before scrubbing token fields from
  account metadata (`crates/cue-cloud-client/src/tokens.rs:350-361`);
- load migrates a complete legacy pair secure-first, recognizes the signed-out
  local marker, and does not silently downgrade after a credential-store failure
  (`crates/cue-cloud-client/src/tokens.rs:394-449`);
- secure clear resets the account to the local signed-out marker only after the
  secure credential clear succeeds; an error retains the owner marker while a
  credential may still exist (`crates/cue-cloud-client/src/tokens.rs:451-470`);
- profile-plus-token save holds the operation lock through the final metadata
  write (`crates/cue-cloud-client/src/tokens.rs:500-530`);
- account/profile JSON itself is private, temp-written, flushed, atomically
  replaced, and directory-synced (`crates/cue-core/src/config.rs:132-190`);
- refresh persists only if the stored credential still matches the pair that
  initiated the request, so logout or a newer login wins
  (`crates/cue-cloud-client/src/client.rs:458-491`).

The secure bundle contains both token values, email, and `account_id`, preventing
a mixed pair and preserving the durable owner across refresh and email changes
(`crates/cue-cloud-client/src/tokens.rs:575-617`). `SecureAccountStore`, the
keyring implementation, and the in-memory test store all implement explicit
compare-and-swap (`crates/cue-cloud-client/src/tokens.rs:472-487,750-758,800-811`).

Daemon startup no longer waits on the OS credential backend. Initial cloud
status uses only environment configuration and the non-secret linked-account
profile marker (`crates/cue-daemon/src/app.rs:17736-17755`). Balance polling and
managed-RAG initialization start only after local state is initialized and the
IPC capability is published (`crates/cue-daemon/src/app.rs:1841-1846,2022-2036`),
and credential-backed client construction runs on a blocking worker
(`crates/cue-daemon/src/app.rs:2983-3033,8103-8112`). The final empty-config
canary below confirms local startup remains available when no token environment
override exists.

### 4. Local-only operational readiness

Onboarding now states the real visibility boundary: Bluey is a desktop window
that the user should hide before sharing or recording if they do not want it
visible (`crates/cue-dashboard/ui/src/pages/Onboarding.tsx:124-155`). After
browser sign-in, users can run an explicit private audio check or continue with
limited audio and retain Jobs, writing, and text Coach features
(`crates/cue-dashboard/ui/src/pages/Onboarding.tsx:183-287`).

The daemon probe:

- refuses to overlap another readiness probe, meeting shutdown, or active audio
  runtime (`crates/cue-daemon/src/app.rs:4571-4592`);
- samples microphone and system sources concurrently for 1.2 seconds, with a
  3.5-second deadline, 96 KiB maximum PCM, and 16 KiB diagnostics
  (`crates/cue-daemon/src/app.rs:89-92,4604-4613`);
- pipes only in memory, kills and waits for failed/timed-out helpers, and rejects
  oversized streams (`crates/cue-daemon/src/app.rs:4620-4735`);
- distinguishes ready, silent, structured permission denial, and unavailable
  without forwarding private unstructured helper errors
  (`crates/cue-daemon/src/app.rs:4737-4770`);
- requires exact 16-kHz mono PCM16 readiness followed by the expected source's
  `stopped` event with reason `duration_complete`
  (`crates/cue-daemon/src/app.rs:6013-6049`).

In release builds, helper discovery is limited to current-executable and installed
Bluey-bin candidates; environment, repository, and current-working-directory
overrides are debug-only (`crates/cue-daemon/src/app.rs:4834-4898`). A candidate
must be a non-symlink regular file; on Unix it must be executable and owned by
root or the current user (`crates/cue-daemon/src/app.rs:4905-4928`).

### 5. Authenticated bounded daemon IPC

The wire envelope contains a per-boot UUID, unique request UUID, and 32-byte
bearer. The bearer uses constant-time comparison and redacted debug formatting
(`crates/cue-core/src/ipc_auth.rs:16-45,68-123`). Authorization is exhaustive:
adding a request variant requires an explicit public/read/mutation/shutdown
classification, and only Ping is public (`crates/cue-core/src/ipc_auth.rs:150-213`).

On Unix the capability is published through an owner-only `0700` runtime
directory and `0600`, one-link, current-uid, no-follow file. Publication is
temp-write, `sync_all`, rename, and directory sync; cleanup checks boot id and
inode so an old daemon cannot unlink a newer record
(`crates/cue-core/src/ipc_auth.rs:232-337,366-479`).

The daemon binds a validated loopback address before publishing the capability
(`crates/cue-daemon/src/app.rs:1803-1818,2022-2030`). It rejects non-loopback
peers and caps active handlers at 64 (`crates/cue-daemon/src/app.rs:2038-2058`).
Each request has a three-second read deadline and a 256 KiB limit; malformed,
oversized, or incomplete requests are rejected before dispatch
(`crates/cue-daemon/src/app.rs:2062-2122`). Shutdown is derived only after
successful authorization (`crates/cue-daemon/src/app.rs:2123-2134`).

Server authorization rejects stale boot ids, mismatched bearers, and duplicate
request ids while bounding replay memory to 4,096 ids
(`crates/cue-daemon/src/app.rs:1572-1625`). Dashboard and CLI clients validate
the destination before loading the bearer, cap request and response bodies,
apply a six-second operation timeout, and retry once only for a stale boot with a
new request id (`crates/cue-dashboard/src/commands.rs:70-148,175-245`,
`crates/cue-cli/src/app.rs:2823-2860,2889-2935`).

### 6. Strict owner isolation

The daemon obtains the local owner through the token-independent account profile
and propagates account parse errors rather than converting them to an unowned
scope (`crates/cue-daemon/src/app.rs:7133-7150`). New meetings inherit that owner
and the active owner workspace (`crates/cue-daemon/src/app.rs:7152-7175`).
Activation verifies both workspace ownership and active-meeting visibility before
persisting profile/instruction changes (`crates/cue-daemon/src/app.rs:7202-7231`).
Owner equality is exact: signed-in data is visible only under the same stable
owner, while signed-out local data must have no owner
(`crates/cue-daemon/src/app.rs:7137-7142`,
`crates/cue-daemon/src/workspace_store.rs:612-623`).

### 7. Performance boundary

Workspace list/get derives reference metadata by reading meetings. That blocking
filesystem/JSON work now runs in `tokio::task::spawn_blocking`, then owner
filtering and bounded view derivation complete before returning
(`crates/cue-daemon/src/app.rs:7234-7290`). This prevents a large meeting store
from directly blocking Tokio worker threads.

It does **not** remove the full scan. Each workspace list still calls
`MeetingStore::all_meetings`, deserializes all eligible meeting JSON, and groups
it in memory. Complexity is O(meeting files + referenced items), bounded per
workspace only after deserialization. This is the primary remaining speed and
scale gate.

## Security and privacy findings

### Improved in this round

- Bearer tokens default to the OS credential store, are grouped into one
  versioned bundle, and are redacted from routine debug output.
- Plaintext token storage is explicit; a secure-store failure does not silently
  select it.
- Token/profile writes are coordinated across threads and Bluey processes, and a
  stale refresh cannot overwrite logout or newer login state.
- Stable cloud account identity is separate from mutable email and bearer-token
  presence.
- Workspace and meeting access is exact-owner scoped; malformed account state
  fails closed.
- Workspace files, pointers, and account profiles use private atomic writes.
- Jobs source metadata cannot be forged through ordinary workspace create/update
  and must match the profile source.
- The readiness probe is explicit, local, bounded, non-persistent, non-transcribed,
  and non-uploaded; the renderer validates those privacy invariants.
- Release readiness does not execute an arbitrary helper found through CWD or an
  environment override, rejects symlinks, and validates structured lifecycle.
- Local daemon protected reads, mutations, and shutdown require a per-boot
  capability; clients reject non-loopback destinations before reading it.
- Slow or oversized clients cannot consume unbounded daemon connections or
  request memory.

### Residual boundaries

- Workspace and meeting metadata is private to the OS account but not encrypted
  at rest. The threat boundary does not include malware already running as the
  same user.
- Explicit plaintext mode necessarily places credentials in the private account
  file. Operations and support copy must say when that mode is enabled.
- An unavailable or locked OS credential service no longer blocks local daemon
  startup or IPC. It can still delay or fail the cloud operation that first needs
  credentials, so sign-in, balance, sync, and managed-RAG surfaces need a clear
  degraded state, timeout, and retry path.
- Helper ownership/executable checks establish a local file boundary, not code
  signing or immutable install provenance. A same-user process that can replace
  an allowed user-owned helper path remains inside this threat boundary.
- The loopback protocol authenticates the client to Bluey but does not provide a
  cryptographic proof of the responding server. Binding before publication
  narrows startup races; it does not make TCP equivalent to an owner-verified
  Unix socket or Windows named pipe.
- The replay cache is intentionally bounded. A request id falls out after 4,096
  later requests in the same boot; bearer secrecy and unique client-generated
  request ids remain the primary controls.
- Clients enforce the 256 KiB response ceiling, but the daemon currently
  serializes its response before the client can reject it
  (`crates/cue-daemon/src/app.rs:2141-2153`). The server should enforce the same
  ceiling before write.
- Windows cannot currently publish/load the capability at all; this is a safe
  release blocker, not Windows support.

## Verification evidence

The shared worktree was changing during this round. Only commands and results
actually observed by the agents are recorded here; older broad results are not
used as evidence for later edits.

Current-tree results personally observed by the backend/security agent:

| Command | Result |
| --- | --- |
| `cargo test -p cue-core ipc_auth --lib` | Passed, 4/4 |
| `cargo test -p cue-daemon ipc_ --lib` | Passed, 4/4, including the 4,096-entry replay bound/oldest-eviction case |
| `cargo test -p cue-dashboard daemon_ipc --lib` | Passed, 6/6 |
| `cargo test -p cue-cli --lib` | Passed, 65/65 |
| `cargo test -p cue-cloud-client --lib` | Passed, 33/33 |
| `cargo test -p cue-daemon --lib readiness` | Passed, 4/4 |
| `cargo test -p cue-daemon --lib strict_owner_lookup_rejects_malformed_account` | Passed, 1/1 |
| `cargo test -p cue-daemon --lib workspace_` | Passed, 14/14 |
| `cargo test -p cue-daemon --lib completed_helper_diagnostics_require_exact_ready` | Passed, 1/1 |
| `cargo test -p cue-core --lib config::tests` | Passed, 2/2 |
| `cargo test -p cue-core --lib workspace::tests` | Passed, 3/3 |
| `cargo test -p cue-daemon readiness_flag_blocks_audio_start_and_meeting_end_inside_lifecycle_guards --lib` | Passed, 1/1 |
| `cargo test -p cue-daemon stale_delete_cannot_hide_a_newer_workspace_revision --lib` | Passed, 1/1 |
| `cargo test -q -p cue-core -p cue-cloud-client -p cue-llm -p cue-daemon -p cue-dashboard -p cue-cli --lib` | Passed, 698 passed, 5 ignored, 0 failed |
| `cargo check -p cue-core -p cue-cloud-client -p cue-llm -p cue-daemon -p cue-dashboard -p cue-cli --all-targets` | Passed |
| `cargo check --target x86_64-pc-windows-gnu -p cue-core -p cue-cloud-client -p cue-llm -p cue-daemon -p cue-dashboard -p cue-cli --lib` | Passed |
| `cargo clippy -p cue-core -p cue-cloud-client -p cue-llm -p cue-daemon -p cue-dashboard -p cue-cli --all-targets -- -D warnings` | Passed |
| `cargo build -p cue-daemon --bin bluey-daemon -p cue-cli --bin bluey` | Passed |
| `npm test -- --run` from `crates/cue-dashboard/ui` | Passed, 8 files and 70 tests |
| `npm run build` from `crates/cue-dashboard/ui` | Passed, 1,726 modules; JS 452.50 kB/133.27 kB gzip; CSS 53.60 kB/9.35 kB gzip |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed after the final product edits |
| `bash -n scripts/observability-acceptance-smoke.sh` | Passed |
| `git diff --no-index --check /dev/null docs/rounds/ROUND-509-BLUEY-WORKSPACES-READINESS-AND-LOCAL-SECURITY.md` | No whitespace diagnostics; exit 1 is the expected no-index content difference |

These checks prove compilation and focused contracts in the current tree at the
time observed. They do not replace signed runtime or performance validation.
After the last balance-poll lifecycle generation change, the daemon library suite
was rerun with 407 passed, 5 ignored, and 0 failed; the Windows GNU daemon-library
cross-check also passed. The six-crate warnings-denied Clippy command,
`cargo fmt --all -- --check`, and `git diff --check` were then rerun successfully
on the frozen product tree.

Two isolated daemon/CLI canaries also passed with temporary data, config, and
runtime directories and no production account or endpoint:

- With an empty account config and no environment token, the daemon published a
  `0600` capability in about 0.5 seconds. Authenticated status plus the capability
  mode/owner check completed in 0.73 seconds; public Ping completed in 0.1 seconds.
  Authenticated stop removed both capability and listener. This resolves the
  earlier startup dependency on OS credential-store availability.
- The authenticated status/stop path returned status then `ok`, the capability
  was `-rw------- uno`, and shutdown reported `capability-removed`. A status
  attempt pointed at `192.0.2.10:58437` failed at local validation with `daemon
  IPC refused non-loopback address 192.0.2.10:58437`, before a bearer could be
  loaded or sent.

The following still require a final release-candidate run after the shared
worktree is frozen:

- full unfiltered all-target Rust tests for core, cloud client, daemon,
  dashboard, CLI, and LLM;
- signed/notarized macOS and signed Windows helper canaries on real microphone
  and system-audio permissions;
- a signed desktop integration canary covering sign-in -> workspace create ->
  Jobs import -> activation -> readiness -> answer -> logout -> restart;
- workspace cold/warm latency and memory benchmarks at realistic meeting counts.

## Priorities and release gates

### P0 — before desktop release

1. Implement owner-verified Windows local IPC. Preferred design: Windows named
   pipe with an explicit current-user ACL and peer verification. If TCP is kept,
   implement and test owner-only capability ACLs plus a stronger server-auth
   handshake. Remove the fail-closed gate only after adversarial ACL/reparse tests.
2. Choose and prove the macOS/Linux transport boundary. Prefer a `0600` Unix
   domain socket with peer credentials; otherwise add server authentication to
   TCP and test port-squatting/stale-capability scenarios.
3. Pin native helper provenance in release: restrict to the installed app/helper
   root and verify the platform signature or a build-manifest digest before
   execution. Preserve debug-only override behavior for development.
4. Freeze the worktree and run the full Rust/UI/cross-platform gate listed above.
   No production claim should rely only on focused tests from a changing tree.
5. Run signed runtime canaries. Static tests cannot grant microphone/Screen
   Recording permissions or prove actual helper discovery and normal-stop output.

### P1 — production scale and operability

1. Add a compact per-meeting workspace metadata index updated atomically with
   meeting saves. Workspace list should read bounded metadata rather than every
   full meeting JSON file.
2. Benchmark 0/10/100/1,000/10,000 meeting stores; record cold/warm p50, p95,
   p99, bytes read, allocations, peak RSS, and async scheduling delay. Define and
   enforce a workspace-list p95 service-level target before claiming speed.
3. Enforce `IPC_MAX_RESPONSE_BYTES` in the daemon before write, add a bounded
   error response for oversized payloads, and test a worst-case workspace list.
4. Add structured metrics with no bearer, email, file path, or content values:
   workspace scan duration/count, stale-revision rate, token-lock contention,
   IPC rejection code, readiness state, helper timeout, and normal-stop failure.
5. Bound credential-backend waits at each cloud UX entry point and expose a
   retryable degraded state without blocking local IPC, readiness, or Coach.
6. Add recovery tests for failure between workspace synchronization and meeting
   persistence during Jobs import. Either make the two-record transition
   recoverable by journal/reconciliation or document the exact retry behavior.

### P2 — product refinement

1. Validate workspace IA with users and measure create/activate/switch/Jobs-link
   task success; refine copy based on evidence rather than competitor imitation.
2. Add pagination or incremental reference loading after the metadata index is
   authoritative. Preserve the reference-only privacy model.
3. Add an explicit diagnostics screen showing token policy, IPC transport,
   readiness helper identity/version, and last safe failure category without
   exposing secrets or private paths.
4. Evaluate encrypted local workspace/meeting metadata as a separate threat-model
   decision; do not imply the OS credential store encrypts non-token files.

## Concrete next-agent handoff

The next implementation agent should work in this order:

1. Freeze and record the exact source state. Re-run the verification matrix and
   replace any stale line citations/results before changing code.
2. Implement Windows named-pipe IPC with owner-only ACL and peer checks, while
   retaining the existing authorization classification, per-boot capability,
   unique request id, deadlines, request/response bounds, and replay limit.
3. Move Unix/macOS IPC to a private Unix domain socket with peer credentials or
   add an explicit server-authenticated TCP handshake. Add adversarial tests for
   port squat, stale capability, symlink/reparse paths, forged shutdown,
   slowloris, oversized request/response, replay, and capacity saturation.
4. Restrict production helper resolution to the immutable installed helper and
   verify code signature/build digest before launch. Keep current structured
   source/format/normal-stop validation.
5. Introduce a versioned workspace metadata index. Update it under the existing
   meeting-store operation lock and rebuild it safely from meetings when absent
   or corrupt. Never copy transcripts, context bodies, document bytes, or answer
   bodies into the index.
6. Add benchmarks and a regression threshold, then verify the complete flow on
   signed macOS and Windows artifacts.

Do not copy recovered proprietary bundle code into this implementation. The
architecture decisions above are Bluey-owned and are supported by Bluey's own
types, tests, and provenance records.
