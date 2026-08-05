# Round 602 - Jobs Signed Runner-Volume Purge

**Date:** 2026-08-05
**Branch:** `feat/phase-602-runner-volume-purge`
**Status:** Source complete and locally verified; no runtime deployment or flag change

## Objective

Close the account-deletion gap on persistent Bluey Browser runner volumes before
local or cloud Browser distribution can be enabled. Account deletion must not
claim success while an enrolled runner volume can still hold a browser profile,
checkpoint, staged result, temporary file, or restored copy for that account.

## Authority Model

- The existing fleet HMAC authenticates transport to the private Jobs API. It
  is not evidence that a particular persistent volume erased data.
- Every managed runner volume owns a durable Ed25519 identity. The server binds
  each immutable public-key epoch to a worker identity and provider-managed
  resource fingerprint. Enrollment consumes a one-time, expiring admission
  grant; an operator cannot silently replace a key on an existing epoch.
- Before serving account work, the live process signs a chained current-storage
  attestation. It binds the exact enrollment and process instance, predecessor
  attestation, tombstone cursor, retained-root device and inventory, complete v2
  subject layout, signed locator set, and a globally empty legacy inventory.
  Retries reuse the exact signed attestation but consume a fresh outer authority
  proof; an append-only fleet hash binds the latest current epoch for every
  non-destroyed volume.
- A response-ambiguous storage attestation is resolved before any later storage
  mutation. An exact committed successor is promoted; an unchanged predecessor,
  enrollment, tombstone binding, and local storage-evidence revision retries the
  byte-identical signed body with a fresh outer proof; changed tombstone bindings
  discard the obsolete transaction; every other conflict fails closed.
- A short-lived process-instance lease is required before the volume may claim
  account work or acknowledge a command. It detects concurrent use of one key,
  but is not embedded in an offline volume's immutable purge command.
- The server signs an immutable purge command for each frozen target volume.
  The command binds its audience, request and command IDs, volume and key epoch,
  key fingerprint, opaque purge subject, global purge generation, issued time,
  minimum runner build, and server signing-key ID. It contains no account ID,
  email address, application answer, or resume data.
- Runner build IDs use one canonical monotonic `runner-GENERATION[.REVISION]`
  ordering. The command field is a minimum compatibility floor: an equal or
  newer canonical build may execute it, while an older or malformed build
  fails closed. A compatible runner rollout therefore cannot strand an
  already-issued deletion command.
- A volume signs its acknowledgement with its own private key. The
  acknowledgement binds the volume, epoch, process instance, purge generation,
  exact command digest, before/after inventory digests and counts, runner build,
  and completion time.
- Exact acknowledgement replay is idempotent. A conflicting replay, a stale
  epoch, a different public key, a concurrently active clone, or a forged
  fleet-HMAC-only acknowledgement fails closed.
- The server signing-key ring and every volume key epoch are durable authority;
  fleet HMAC rotation cannot create, acknowledge, or complete a purge.
- Request generations are allocated when fan-out freezes. A separate monotonic
  tombstone-completion generation is allocated only when a purge completes, so
  volumes cannot skip a lower request that finishes after a higher request.

Bluey does not claim that a software key distinguishes a powered-off byte-for-byte
clone of the same volume. Concurrent clones are fenced by the instance lease;
stronger sequential-clone identity requires provider, KMS, TPM, or equivalent
managed-resource attestation and remains a rollout gate.

## Deletion State Machine

1. Irreversible `click_started` or `side_effect_unknown` work blocks deletion
   before a deletion fence is created so the owner can reconcile the employer
   side effect.
2. Otherwise the server persists the account-deletion fence before fan-out.
   New leases, object writes, profile publications, and local reconciliation
   reject the fenced account.
3. In a separate bounded transaction, under one fleet generation, the server
   freezes every enrolled, non-destroyed managed volume, including suspended,
   retired, or offline volumes. A missing, corrupt, or legacy local residency
   index is never treated as evidence that the volume has no account data. The
   server signs and stores one replay-safe command per volume and key epoch.
   Current-storage attestation gates work and distribution cutover but never
   narrows this conservative deletion fan-out; any future narrowing requires a
   separate reviewed authority model.
4. The HTTP request releases database and lifecycle locks while volumes work
   and returns `202 Accepted` with durable pending status. Clients retain their
   account credentials and poll/retry; they never present pending as deletion.
   Deletion waits until every frozen target has either a verified
   acknowledgement or separately authorized, provider-bound destruction
   evidence.
5. Only after runner purge completion, live object PUT drain, artifact/audit
   namespace sweeps, and the existing database lifecycle checks may Bluey hard
   delete the account.
6. Completion writes an indefinitely retained pseudonymous tombstone before
   the account row is removed. A later restored or re-enrolled volume that
   reports the same local purge subject receives a signed enforcement command
   and cannot serve that account data again.
7. Browser distribution cannot leave reconciliation mode until every enrolled
   volume and legacy storage root is accounted for at the current fleet
   generation and an authorized cutover record binds the exact evidence hash.

The frozen required-target set never shrinks because a runner is offline.
Legacy volumes without complete residency proof keep deletion pending until an
authorized reconciliation or physical wipe is recorded.

## Runner Filesystem Contract

- The runner requires one canonical, owner-private persistent root. Account
  data lives beneath a subject-scoped directory derived from a local SHA-256 of
  the opaque purge subject; direct account identifiers are not persisted in
  locator names.
- Managed account-data operations cross a native root-handle boundary. On
  supported platforms, traversal, publication, inventory, and removal are
  directory-handle-relative, no-follow, beneath-root, and same-device; a
  platform without the reviewed native implementation is a distribution
  blocker rather than a pathname-check fallback.
- Production acquires and exclusively locks that native root from the original
  configured path before identity or compatibility-control-file I/O. A
  symlinked ancestor, root replacement, or second runner therefore fails before
  durable identity state can be read or created.
- The production runner and Chromium run as a dedicated unprivileged OS
  principal. The native storage boundary protects against corrupted/restored
  roots, unsafe entries, crash remnants, and a second cooperative runner; it
  does not claim protection from host root, kernel compromise, or an attacker
  that can use the same OS credential and volume private key.
- Account residency is indexed durably before any profile restore, checkpoint,
  receipt, result, or staging write for that subject. Immutable locator records
  avoid rewriting one growing manifest during concurrent runs.
- Purge first journals the signed command, stops matching active work, and then
  inventories only paths derived from validated profile and result scopes.
- Inventory walks use `lstat`, reject symlinks and non-file/non-directory
  entries, start from a canonical owner-private data root, remain beneath that
  root, and hash a sorted canonical inventory. Locators identify bounded scopes;
  they are not trusted as arbitrary filesystem paths.
- Purge removes active profiles, encrypted snapshots and generations,
  checkpoints, committed or staged results, and matching temporary/staging
  remnants. It rescans to an empty inventory before creating the local durable
  tombstone or signing an acknowledgement.
- A crash resumes the same journaled command. A corrupt index, path escape,
  symlink, incomplete rescan, or conflicting generation never produces an
  acknowledgement.
- Before orphan sealing, checkpoint restore, or accepting traffic, the runner
  reconciles every retained local tombstone and performs a final closed-world
  storage audit. Online enforcement commands arrive in server
  completion-generation order. Local tombstones do not encode that server
  cursor, so startup deterministically revalidates all of them and keeps no
  local high-watermark that could skip a lower request completed later.
- Classified legacy storage is not mistaken for an empty current layout and is
  not a process-crash condition. The control listener remains online but
  health/work stay unavailable while signed purge commands remove exact targets
  or an authorized reconciliation/wipe establishes global legacy zero.
- Purge command polling uses a signed, volume-and-epoch-scoped keyset cursor.
  The authority proof binds nullable `after_command_id` and `limit`; both
  databases order by `(order_class, order_generation, command_id)`, fetch one
  look-ahead row, and reject unknown or wrong-volume cursors. Retained command
  rows keep a cursor valid across acknowledgement or supersession races.
- Global legacy deletion is two phase. Phase A follows every bounded page,
  durably prepares each command, and emits no local tombstone or ACK. A bounded
  cursor resumes across work-budget yields. Only after the complete pass proves
  global legacy zero may phase B restart from the null cursor, re-prepare pending
  commands, and finalize them. A crash between phases reuses the durable
  journals. Cursor pages never report readiness; only an empty null-cursor poll
  may evaluate it.

## Acceptance Criteria

1. A shared worker signing key alone cannot forge another volume's purge
   acknowledgement; command and acknowledgement test vectors match Node and
   Rust verification.
2. Admission grants are one-time and expiring, enrollment is idempotent for the
   same key epoch and managed-resource fingerprint, and key history is
   immutable. A changed key, stale epoch, expired/replaced process instance, or
   concurrent clone cannot claim account residency or acknowledge a purge.
3. The account fence is durable before fan-out, the required target set is
   immutable, and no database connection or lifecycle lock is held while an
   offline volume is pending.
4. Exact command and acknowledgement replay succeeds; any same-identity payload
   conflict is rejected.
5. Runner purge closes active work, rejects unsafe filesystem entries, removes
   every indexed account path (including plaintext receipt directories), proves
   a zero rescan, and emits no acknowledgement after corruption or partial
   failure.
6. All required targets must acknowledge. An offline or lost target remains
   pending unless an authenticated administrator records separate destruction
   evidence.
7. Indefinite opaque tombstones survive account-row deletion and force repurge
   after a volume or data-root restore without retaining direct account
   identifiers.
8. SQLite and PostgreSQL migrations and operations remain structurally paired.
9. A local three-volume fault matrix proves success, offline blocking, exact
   replay, clone/stale-epoch rejection, crash resume, and restored-volume
   tombstone enforcement.
10. Documentation calls this managed runner-volume purge attestation. It does
    not claim physical-sector erasure, control of unmanaged copies, or live
    production certification.
11. Fleet reconciliation and authorized cutover bind exact enrolled-volume,
    legacy-root, destruction, and tombstone generations; distribution remains
    unavailable while any count or evidence hash is unresolved.
12. Web, dashboard, and CLI deletion clients treat `202 Accepted` as pending,
    retain credentials, and clear local authority only after a verified `200`
    hard-delete response.
13. Request and tombstone-completion counters are distinct. A fault matrix
    completes a higher request before a lower one, proves online delivery uses
    completion order, and proves restart revalidates both retained local
    tombstones without relying on a missing local completion cursor.
14. The managed storage root uses a native retained-handle implementation and
    an exclusive process lock. Parent swaps, symlinks/reparse points, hardlinks,
    special files, mount crossing, root replacement, and a second runner fail
    closed; unsupported platform implementations keep distribution disabled.
15. The runner container uses a dedicated unprivileged principal and a
    canonical owner-private data root. Documentation does not overstate this as
    resistance to host-root or same-credential key compromise.
16. A five-command, three-page matrix proves every legacy target is prepared
    before the first acknowledgement, command cursors remain stable across
    completion, work-budget state is bounded, and crash restart can resume
    between preparation and finalization.
17. Ambiguous storage-attestation tests prove exact committed promotion,
    byte-identical pre-commit retry, obsolete-tombstone replacement, mutation
    fencing, and post-purge successor retry while the predecessor remains
    server-current.
18. Linux and Darwin release gates stage the built native library under the
    actual `.node` loader path and execute the N-API smoke. Container inputs are
    immutable digest pins rather than mutable base tags.

## Implementation Summary

- Paired SQLite and PostgreSQL migrations add immutable volume/key epochs,
  admission grants, process leases, residencies, purge fan-out, enforcement,
  destruction evidence, tombstones, current-storage attestations, legacy
  inventory authority, and fleet cutover state.
- The server exposes signed enrollment, heartbeat, residency, command poll/ACK,
  storage-attestation, reconciliation, cutover, and administrator-destruction
  paths. Account deletion durably fences first, freezes every non-destroyed
  volume, releases database locks while offline targets remain pending, and
  completes only after all storage and database lifecycle evidence agrees.
- The runner uses a Rust N-API retained-handle boundary for its owner-private
  root, durable subject locators, purge journals, tombstones, account-residency
  binding, and storage evidence. Profile, checkpoint, recovery, result, and
  staging stores all enter that managed boundary before account bytes are
  written.
- CLI, dashboard, and web deletion clients preserve credentials across
  `202 Accepted` and clear them only after the verified `200` hard-delete
  response.
- Linux and Darwin CI/release paths build, stage, load, and smoke the exact
  native addon. The runner image uses a dedicated principal, a private canonical
  data root, and digest-pinned Rust and Playwright base images.

## Verification

```text
Focused runner client + purge       42 passed
Jobs automation                     530 passed
Jobs Browser                        151 passed
Jobs runner                         249 passed
Jobs workflows                       76 passed
Jobs portal                         121 passed
Jobs total                        1,127 passed
Jobs strict typecheck/build          five workspaces passed

Server library                       977 passed
Server signed HTTP integration        99 passed
Server auxiliary                       6 passed
Server total                       1,082 passed
Server fmt/check/all-feature Clippy  passed

Native retained-handle storage        14 passed
Native release build + Darwin load   passed
Root Rust tests                    1,045 passed, 17 ignored
Root fmt/strict Clippy/release build  passed
Dashboard UI                          35 passed; production build passed
macOS overlay + whisper release       passed
```

Schema parity covers 23 tables and 28 indexes. CI guard self-tests,
provenance/license inventory, operating-doc coverage, tracing/PII analysis,
workflow YAML parsing, account-deletion browser tests, automation export smoke,
and diff checks pass. The final staged privacy gate scanned 2,376 tracked paths
and 2,103 text files without a finding. Docker is unavailable on this local Mac,
so the real Linux image build and in-image native-addon smoke remain enforced by
CI and release workflows rather than claimed as local evidence.

## External and Rollout Gates

- Live PostgreSQL migration/concurrency and object-storage fault exercises need
  authorized isolated infrastructure.
- Provider-managed volume enrollment, legacy-root reconciliation or physical
  wipe, and sequential-clone attestation need approved credentials and managed
  resources.
- Linux and Windows native artifact/device matrices, power-loss testing, and
  production canaries need their real platforms.
- No production runner was enrolled, no database or volume was mutated, no
  credential was read or rotated, and no deployment or feature flag changed.

## Production Boundary

This round does not deploy a runner, enroll a production volume, rotate or read
credentials, mutate production storage, reconcile a live legacy volume, or
change any Jobs production feature flag. `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED`
and `BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED` remain `0` until source,
artifact, real-device, and authorized production rollout evidence all pass.
