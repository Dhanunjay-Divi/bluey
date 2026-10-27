# REVIEW: Phase 629 Jobs non-production integration validation

Date: 2026-10-27 (America/New_York)
Base: `130ef807bc952874a2cb391e43511d7364ab1b0a`
Reviewer role: independent, read-only product/test-source review
Verdict: 🟢 accept the bounded local Phase 629 source/evidence batch; no open finding

## Scope reviewed

- `jobs/scripts/run-nonprod-postgres-tests.mjs`, complete new file.
- `server/src/db/jobs/managed_cloud_release_authority.rs`, complete Phase 629 diff from the base.
- `server/src/db/jobs/original_source_verification.rs`, complete Phase 629 diff from the base.
- `server/src/db/jobs/production_positive_authority_fixture.rs`, complete Phase 629 diff from the base.
- `server/src/api/jobs_managed_cloud_releases.rs`, complete Phase 629 PostgreSQL HTTP-test diff.
- `server/src/db/jobs/local_runner.rs` and
  `server/src/db/jobs/postgres_local_authority_tests.rs`, complete Phase 629 diff from the base.
- All six Jobs workspace manifests, `jobs/package-lock.json`, the Browser source-contract
  Electron expectation, and the Jobhive CSV advisory regressions.
- `docs/work/FIX-795-postgres-source-verifier-ready-sequence-type.md` through
  `docs/work/FIX-801-postgres-unknown-result-capacity-parity.md`.
- PostgreSQL migrations 040–043 and the legacy heartbeat/immutability behavior they replace or extend.
- `scripts/run-bluey-tests.sh` environment isolation and cleanup boundary.

No product or test source was edited by this reviewer. No Cargo command, database, service,
container, provider, production account, application, or email was started or accessed.

## Resolved source finding

### Resolved P1 statically — PostgreSQL pre-authorization unknown-result path could not create protected capacity

`finalize_local_side_effect_unknown` has intentionally symmetric reserve/retain helpers. For a
`claimed` or `needs_input` ticket, the helper may create an exact protected submission-evidence
capacity row and then extend it through the reconciliation grace window. For `click_started` and
terminal replay, the helper never creates replacement capacity; it can only extend an exact active,
unexpired row, so missing or changed capacity fails closed.

The PostgreSQL branch previously performed an unconditional capacity-existence query and bailed before
calling that helper. The SQLite branch has no such precheck. Consequently PostgreSQL could not execute
the intended pre-authorization recovery case with zero initial capacity, even though the shared
PostgreSQL helper already enforces account ownership, scope, runner, exact byte/object reservation,
quota, idempotency, expiry and transaction rollback.

The frozen correction retains the exact `FOR UPDATE` prelock before PostgreSQL database time is
sampled, avoiding stale-time extension after a lock wait. Its missing-row branch is now
status-sensitive: only `claimed` and `needs_input` continue without a row so the existing helper
can reserve one; `click_started` and terminal replay still fail missing capacity. The positive
pre-authorization regression covers zero initial capacity, creation and exact replay. A new
synthetic-corruption regression forces `click_started` without capacity and proves the exact
missing-capacity error plus unchanged ticket, application, attempt, browser-session and capacity
families.

Locations:

- `server/src/db/jobs/local_runner.rs:3133-3146`
- `server/src/db/jobs/local_runner.rs:3170-3195`
- `server/src/db/jobs/local_runner.rs:3293-3334`
- `server/src/db/object_uploads.rs:1306-1364`
- `server/src/db/object_uploads.rs:1459-1518`

The adjacent PostgreSQL test expectation after deleting an identity was also stale but did not
require a production change. Its first successful submit authorization had already transitioned the
ticket to `click_started`; an exact retry intentionally uses stored click-started authority rather
than reacquiring mutable identity authority. The corrected test expects success and snapshots the
replay as read-only, matching the existing SQLite contract.

## Resolved validation finding

### Resolved P1, test-only — PostgreSQL local-authority fixture was not admitted

The isolated configured run reaches three legacy PostgreSQL local-authority cases, but their
shared `local_authority_fixture` inserts an unverified account and never establishes a durable
public-beta enrollment. In test builds the outer master gate is already deterministically enabled;
the missing verified email, eligible cohort state and enrollment make the production beta effect
gate deny the fixture before the intended local-claim assertions. This explains the observed
`None` results in the two positive local-claim tests.

The frozen correction is test-only. It creates the account with `email_verified_at`, puts the
retained `public-v1` cohort in `closed_to_new` with its bounded hard cap, inserts the normal durable
`jobs_public_beta_enrollments` row and reconciles `assigned_count`. Cleanup deletes the account,
relies on the enrollment's declared `ON DELETE CASCADE`, and reconciles `assigned_count` again.
This mirrors the existing `tests.rs::test_pool` durable setup rather than bypassing or weakening
`public_beta_effect_authorized_postgres_tx`.

The deletion-intent case had a separate stale expectation. Both backends intentionally reject an
account with a deletion intent inside the beta effect gate. PostgreSQL therefore returns
`ExecutionLeaseError::Conflict` before the later explicit write-fence call; moving that later call
does not produce `AccountDeleting`. The renamed regression now preserves exact `Conflict` and
proves that zero leases were created. Production error precedence and fence order are unchanged.

Locations:

- `server/src/db/jobs/postgres_local_authority_tests.rs:1658-1698`
- `server/src/db/jobs/postgres_local_authority_tests.rs:2567-2621`
- `server/src/db/jobs_beta_access.rs:199-206`
- `server/src/db/jobs_beta_access.rs:211-249`
- `server/src/db/jobs_beta_access.rs:254-296`
- `server/src/db/jobs/execution_leases.rs:1864-1889`

No parent/child lock-order defect was established. The actual PostgreSQL path acquires the account
row `FOR KEY SHARE` inside the beta gate before the discovery advisory lock and authority work.
The application discovery is an unlocked snapshot read. The current-authority helper then again
acquires the account row `FOR KEY SHARE` before locking account-policy child rows `FOR SHARE`.
Thus the account parent is acquired before child row locks, consistent with the account-deletion
ordering invariant; the later `FOR UPDATE` write fence revalidates deletion state rather than being
the first parent acquisition. The local-run path is stricter still: beta parent lock, discovery
advisory lock, explicit write fence, then authority child prelock.

Evidence locations:

- `server/src/db/jobs.rs:179-196`
- `server/src/db/jobs/execution_leases.rs:902-920`
- `server/src/db/jobs/execution_authority.rs:336-376`
- `server/src/db/jobs/taxonomy_policy.rs:944-975`
- `server/src/db/jobs/local_runner.rs:395-427`
- `server/src/db/object_uploads.rs:1208-1219`

## Resolved review findings

### Resolved P1 statically — typed PostgreSQL boot-trigger denial maps to HTTP conflict

The real PostgreSQL trigger rejects a forged bound-ready heartbeat with SQLSTATE `P0001`. Treating
all storage errors as HTTP 500 made that expected authority conflict differ from SQLite/API
behavior. The frozen mapper now traverses the `anyhow` source chain but accepts only an actual
`postgres::Error` whose database error has both `RAISE_EXCEPTION` and the exact migration-043
trigger message. It returns the existing generic 409 body; all other storage errors remain generic
500 responses. A plain `anyhow` error containing the same text is explicitly proven insufficient,
so this is typed database classification rather than string matching arbitrary errors.

The configured test additionally invokes the production DB function directly, proves the exact
code/message and snapshots all six runtime authority table families before and after the rejected
write. It then exercises HTTP 409, valid degraded heartbeat/startup and byte-identical read-only
replay. This path is included in the final parent-reported 30/30 configured PostgreSQL pass.

Locations:

- `server/src/api/jobs_managed_cloud_releases.rs:446-494`
- `server/src/api/jobs_managed_cloud_releases.rs:580-601`
- `server/src/api/jobs_managed_cloud_releases.rs:750-760`
- `infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql:47`

### Rejected candidate correction — do not create a generation-three v3 test namespace

A shared-database PostgreSQL run reached the production manifest immutability boundary and
correctly rejected a v3 canonical manifest that reused an already-imported immutable release
identity. A candidate test-only correction changed the v3 fixture to generation and sequence
three. Per-case execution then correctly rejected that first import with `SequenceRegression`:
the production first-import rule is contiguous and a fresh database cannot begin at generation
three. That candidate patch and its unit assertion were reverted.

The accepted isolation mechanism is one fresh private PostgreSQL database per exact test case.
The v1, v2 and v3 helper identities therefore remain their original generation-one fixtures;
production immutability and contiguous-sequence fences are unchanged. No resolved finding or
runtime pass is claimed for the reverted namespace patch.

Location: `jobs/scripts/run-nonprod-postgres-tests.mjs`, repeated `--case-filter` execution mode.

### Resolved P2 — retained unknown-extra-column compatibility lacked a positive upgrade regression

FIX-796 explicitly retains the importer contract that unique unknown headers are accepted. The
four new cases prove that duplicate prototype/constructor names and replacement of a required
column fail before row emission, and production `validateHeaders` remains permissive for unique
extra names.

The corrected test now sends all 22 required fields plus either `future_extension` or a unique
`__proto__` header through the upgraded parser. Both cases assert one emitted row in one batch,
the exact normalized candidate, absence of the ignored extra as an own property, an ordinary
candidate prototype, byte-for-byte unchanged `Object.prototype` descriptors, and no inherited
pollution marker. This closes the upgrade-evidence gap without changing the production parser.

Locations:

- `jobs/automation/tests/jobhive-artifact.test.ts:17-22`
- `jobs/automation/tests/jobhive-artifact.test.ts:228-274`
- `jobs/automation/src/jobhive-artifact.ts:249-270`
- `docs/work/FIX-796-jobs-csv-parser-advisory.md:26-36`

### Resolved P1 — PostgreSQL boot-ready sequence parameter inferred as `int4`

The first configured test build exposed a test-helper compile error, which was corrected without
changing runtime behavior. The subsequent configured run passed the migration/catalog test but
failed the concurrent startup test because PostgreSQL inferred the boot-ready update's `$3-1`
parameter as `int4` while production binds the sequence as Rust `i64`. That prevented the real
PostgreSQL boot-ready transition before lifecycle assertions could run.

The frozen correction extracts the exact production update into one shared constant and casts `$3`
to `BIGINT` in both the assignment and predecessor predicate. This matches the existing generic
PostgreSQL heartbeat update. The production parameter order remains runtime ID `TEXT`, epoch
`INT8`, sequence `INT8`, time `INT8`; its exact `changed == 1` atomic fence is unchanged. The
configured regression prepares that same constant, asserts those four inferred parameter types,
and actually binds `i32::MAX + 1` for the sequence against a guaranteed-missing runtime ID before
running the concurrent lifecycle. Thus it covers both server inference and the Rust driver
serialization boundary without mutating fixture state.

Locations:

- `server/src/db/jobs/managed_cloud_release_authority.rs:18841-18885`
- `server/src/db/jobs/managed_cloud_release_authority.rs:21633-21665`

### Resolved P1 — live PostgreSQL legacy readiness compatibility

`043_jobs_source_verifier_heartbeat_boot_bindings.sql` replaces the PostgreSQL source-verifier
heartbeat trigger. The new configured tests cover v3 boot startup, readiness, replay, replacement,
lease and attempt fences, but `original_source_verifier_v2_preserves_direct_ready_and_ready_heartbeat_compatibility`
remains SQLite-only.

The corrected regression now installs, on the configured fresh PostgreSQL database after migration
043, a legacy non-boot source-verifier manifest/runtime and proves its authorized direct-ready and
ready-refresh behavior remains accepted. The adjacent v3 lifecycle test proves a v3
boot-protocol runtime without an exact current boot binding is rejected. Catalog presence and v3
positive behavior do not prove the legacy branch of the replaced trigger.

Resolution observed statically: the new PostgreSQL v2 test uses a unique signed scope so it cannot
replace the default v3 head, proves heartbeat sequences one and two remain genuinely non-boot, and
checks that migration 043 is present. The v3 lifecycle test now separately rejects an unbound ready
heartbeat.

Locations:

- `infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql:45-54`
- `server/src/db/jobs/managed_cloud_release_authority.rs`, SQLite-only v2 compatibility test after
  the new configured migration regression.

### Resolved P2 — complete replay snapshot

`postgres_original_source_account_snapshot` currently includes assignments, attempts, events and
attempt boot bindings. The assertion after stale calls and exact terminal replay therefore does not
detect unexpected writes to observations, receipts, transitions or heads.

The snapshot now includes account-scoped canonical snapshots of observations,
receipts, transitions and heads before asserting byte-for-byte equality. This is necessary for the
test's claim that exact replay and rejected stale publication are read-only.

Locations:

- `server/src/db/jobs/original_source_verification.rs:8627-8688`
- `server/src/db/jobs/original_source_verification.rs:8782-8806`

### Resolved P2 — exact-parent cascade isolation

The test proves the first attempt and companion disappear when its posting is deleted, then deletes
the account and proves the second pair disappears. It does not assert that the unrelated live
attempt and companion remain after the first posting deletion.

After deleting `first_posting` and before deleting the account, the corrected test asserts the
unrelated `live` attempt and its boot companion are still exactly `(1, 1)`. The later account
deletion then proves that pair cascades to `(0, 0)`.

Location: `server/src/db/jobs/original_source_verification.rs:8724-8751`.

## Resolved catalog-strength note

The migration catalog test now joins `pg_class`, `pg_namespace` and `pg_proc`, binds each enabled
trigger to the expected relation/schema/function, handles PostgreSQL's 63-byte identifier limit,
checks the installed replacement heartbeat function, and verifies the validated ten-capability
manifest constraint.

## Positive static observations

- The regenerated lock has 674 package entries (673 excluding the root), 38 changed entries and no
  added or removed package entries. The only major-version transition is the documented, exact
  `csv-parse` 6-to-7 correction. Every other changed package remains on its existing major line.
- All five workspace dependency/devDependency maps exactly match their lockfile workspace records.
  The changed transitive versions are audited targets named by FIX-797 or required Vitest and
  Browserslist closure. The root Node floor satisfies the new Electron, Vite, Vitest and
  brace-expansion engine declarations.
- The current-source Browser package contract expects Electron 43.8.3 while historical signed
  fixture versions remain unchanged. No host-overlay dependency is changed.
- The PostgreSQL harness now requires both exit code zero and null termination signal after normal
  shutdown. A killed or otherwise non-clean postmaster therefore cannot be reported as a pass.
- Repeated `--case-filter` mode discovers exact Rust test names asynchronously, rejects empty or
  overlapping selections and bounds the set to 128. It then runs each exact case sequentially
  against a distinct random private database with pgvector in the same owned cluster/compiler
  root, continues after individual failures, reports the complete counts and fails overall if any
  case failed. Generated database URLs and the random password are never logged.
- The configured HTTP regression enters the real Axum registration, heartbeat and startup routes
  with an explicitly injected already-authenticated worker identity and real PostgreSQL. It does
  not claim to test fleet HMAC or sockets. It proves premature and unbound ready heartbeats and a
  client-supplied probe result are read-only failures, then proves persisted readiness, stored
  canonical response bytes and exact read-only replay.

- The harness rejects inherited database URLs, generates the test URL only inside the isolated
  child, leaves the primary backend on SQLite, uses SCRAM over loopback, verifies the exact
  postmaster PID/data directory/address/port, requires PostgreSQL 17, executes a pgvector distance
  query, and delegates exact-root removal to the canonical launcher.
- Previously reported harness defects were corrected: spawn-error completion is registered before
  events can fire, signal termination is checked through `signalCode`, forced `SIGQUIT` fails the
  gate, and the child receives an empty private home rather than owner credential/config files.
- The configured v3 lifecycle fixture uses two real discovery imports and two due assignments.
  Expected PostgreSQL trigger failures run in separate transactions and roll back the aborted
  transaction before reuse.
- The extracted production-positive helper retains source projection, signed ATS authority,
  signed integrity authority and composed verified-job checks; it does not manufacture eligibility
  by changing the production resolver.
- Attempt binding checks exercise exact runtime/epoch/boot values, direct child update/delete and
  direct attempt deletion. Authorized deletion proceeds through the owning posting/account parent.
- The rejected generation-three fixture namespace and its unit assertion are absent. The current
  `managed_cloud_test_release_import` still uses generation and sequence one.
- The reviewed production Rust changes are exactly three bounded corrections: the PostgreSQL
  boot-ready statement's explicit `BIGINT` sequence cast; typed PostgreSQL trigger-denial mapping
  to the existing generic HTTP 409; and the lock-preserving, status-sensitive local unknown-result
  capacity guard. The remaining Rust changes are tests or the test/integration-support-only
  production-positive fixture module.

## Evidence status and limits

Reviewer-observed static checks:

- `git diff --check` for the reviewed source and independent-report paths: pass.
- Node syntax check for the harness: pass.

Parent-reported harness evidence, not independently executed by this reviewer:

- PostgreSQL 17.10 / pgvector 0.8.3 positive smoke: exit 0, owned root removed.
- PostgreSQL 14 wrong-major negative: expected exit 1 before initialization.
- Owned-postmaster `SIGKILL` with child exit 0: expected harness exit 1 through `signalCode`, owned
  root removed.
- All reported private run parents empty afterward.

Parent-reported configured PostgreSQL attempts, not independently executed by this reviewer:

- The first configured harness (`85107`) exited 101 at compile time before tests because a new
  SQLite branch attempted tuple-field access on a `Result`. The helper was corrected to map the
  successful tuple while preserving its error propagation. PostgreSQL stopped and the exact root
  was reported absent.
- The next configured harness passed the PostgreSQL migration/catalog test, then exited 101 when
  the concurrent startup test reproduced the production `i64`/inferred-`int4` bind defect described
  above. PostgreSQL stopped and the exact root was reported removed. This is not an overall
  configured-suite pass.
- The shared-statement `BIGINT` correction and its boundary regression have only been reviewed
  statically by this reviewer. A later parent-run batch advanced past that original bind failure,
  but did not produce a complete pass: shared-database cases encountered immutable release-identity
  collisions. This motivated the fresh-database-per-exact-case harness mode, not a production-fence
  relaxation.
- The next per-case parent run reported 28 cases run, 22 passed and 6 failed. Three failures were
  `SequenceRegression` from the now-reverted generation-three v3 fixture candidate. The remaining
  three isolated failures are the PostgreSQL local-authority fixture/expectation issue documented
  above. This historical run was not a configured-suite pass.
- Superseding final evidence: the fresh-database-per-exact-case configured PostgreSQL matrix passed
  all 30 cases. PostgreSQL shut down cleanly and its postmaster PID file was absent afterward.

Parent-reported Jobs JavaScript evidence, not independently executed by this reviewer:

- 2,025 tests across five workspaces, all workspace typechecks and builds, and the full installed
  dependency audit passed with zero findings. The actual Chromium exact-submit fixture passed and
  was not skipped. Release checks passed 32/32; schema validation reported 113 tables and 91
  indexes; CI guards passed.

Other parent-reported evidence, not independently executed by this reviewer:

- Strict Rust Clippy for all targets passed.
- The Linux amd64 QEMU Docker build and hardened exact-image smoke passed. Native storage validation
  passed one unit and 13 integration tests. The exact owned VM, containers and volume were deleted.
  The tested image digest was
  `bcab3b4e6cf7386acec09b1d48bfc2fd8f5293e2f495b30f7fd51f199083f385`; it is a
  base-`130ef807` plus seven-manifest overlay and is not a final-source signed release artifact.
- Final session `79633` exited zero: 1,703 library, one binary and 134 integration tests passed
  (1,838 total), with zero failures and zero ignored. The HTTP integration subset passed 113/113
  in 538.61 seconds; the library suite completed in 2,509.19 seconds.
- The exact canonical test root
  `/private/tmp/bluey-test-runs-phase629-final30-20261027/run.1793120675.5618.2087088370`
  was reported absent after cleanup, all 20 Phase 629 private parent directories were empty, and
  host free space increased from 32 GiB to 39 GiB.
- A post-run source comparison against the staged source, manifests and PostgreSQL harness passed
  with no differences. Fresh read-only Colima inventory showed four unrelated pre-existing profiles
  still stopped; the owned profile, instance and disk were absent from the actual Colima/Lima
  storage, and the Docker context remained `default`.

This acceptance is limited to the reviewed local Phase 629 source and evidence batch. Hosted
PostgreSQL, production accounts, provider behavior, registry read-back, native physical amd64
hardware, rollout and rollback remain outside this review. The QEMU image is not a signed release
artifact, and this document is not merge, release, deployment or production-flag approval.
