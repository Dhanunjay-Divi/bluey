# Phase 628 Independent Jobs Protocol Review — 2026-10-27

## Review identity and verdict

- Review snapshot: `4ef949732ee50e0d8691e5eb809462e97eae0199`
- Comparison base: `fc7e535143a262689da8e99ce7638aee42f40407`
- Scope: Jobs Rust and SQLite/PostgreSQL SQL changed by Phase 628.
- Product-source handling: read-only. This report is the reviewer's only authored file.
- Initial verdict at this snapshot: **request changes** for two P2 source findings.
- Bounded working-tree re-review: both source findings are resolved statically; release/test
  holds remain. No new source blocker was found in the final reviewed fixes.
- This is not merge approval or release approval.

The review checked authentication, protocol binding, replay, boot-generation fencing,
lease/heartbeat/terminal behavior, migration replay/failure handling, lock order, cascade
behavior, and v1/v2 compatibility. No P0 or P1 issue was found. In particular, the reviewed
v3 lease path requires the current and ready boot generation, and a historical terminal
result is read-only: a replacement boot cannot adopt or publish work for an older attempt.

## Actionable source findings

### P2 — committed startup replay is incorrectly rejected after the boot head advances

Locations at the reviewed snapshot:

- `server/src/db/jobs/source_verifier_boot.rs:787-845` (SQLite)
- `server/src/db/jobs/source_verifier_boot.rs:904-941` (PostgreSQL)

Both implementations authenticate the runtime/session and then call
`require_source_verifier_boot_head_*` before looking up the already committed
`startup_request_id`. This contradicts the function contract at lines 765-767: an exact
retry should return the stored canonical response without re-running the probe.

Reproduction sequence:

1. Register generation 1, record its degraded heartbeat, and commit its startup request.
2. Register generation 2 for the same runtime instance and epoch, advancing the current head.
3. Retry the exact committed generation-1 startup request after a lost/ambiguous response.

The current code rejects step 3 as a stale generation before reading the committed response.
This is an availability/idempotency defect, not an authority bypass: the current/ready-head
checks still prevent generation 1 from obtaining new leases or publishing new work after
generation 2 advances the head. The bounded HTTP startup retry makes the race reachable when
another process registers a replacement boot during the old process's response-loss retry.

Required correction and regression:

- Preserve runtime/session/release-authority authentication before replay.
- For an existing request ID, verify the canonical request digest and return the stored bytes,
  time, and `replayed=true` without requiring that generation to remain current.
- Keep the current-generation fence for first publication.
- Add the explicit sequence above for SQLite and the shared PostgreSQL behavior: exact replay
  succeeds after head advancement, changed payload conflicts, and an unpublished stale startup
  remains rejected. The existing stale-startup test proves only the last case.

Resolution re-review: **resolved in the bounded working-tree fix**. SQLite and PostgreSQL now
perform the existing runtime identity, session proof, current release/runtime authority, and
protocol-admission checks before reading the immutable committed result. The current boot-head
check remains immediately before probe/publication when no committed request exists. The new
SQLite regression commits generation 1, registers generation 2, verifies exact canonical
response/time replay, rejects a changed payload, a fresh stale request ID, and a bad session,
rechecks that the old binding cannot lease, and snapshots the five boot/heartbeat authority
tables before and after. PostgreSQL follows the same production ordering; native PostgreSQL
execution remains a release hold below.

### P2 — SQLite migration failures can return a transaction-open, FK-disabled connection to the pool

Locations at the reviewed snapshot:

- `server/src/db/mod.rs:2712-2720`
- `infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql:2-3`
- `infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql:276-278`

The generic post-058 loop executes 063, 064, and 065 with raw `execute_batch`. Migration 065
turns foreign keys off and starts `BEGIN IMMEDIATE`; it restores/commits only at the end. A
mid-script error therefore exits without rollback or restoration. This is not repaired by the
pool: the locked dependency is `r2d2_sqlite 0.25.0` (`server/Cargo.lock:2447-2448`), whose
local source at `r2d2_sqlite-0.25.0/src/lib.rs:163-169` validates using an empty batch and
implements `has_broken` as unconditional `false`. The contaminated connection can be reused.

This violates migration atomicity and may leave later requests running inside the abandoned
transaction with referential enforcement disabled. Migration 065 also rebuilds a table on
replay, so the failure surface is not limited to a pristine installation. The dedicated 062
executor in `server/src/db/mod.rs:2799-2875` demonstrates the needed cleanup pattern.

Required correction and regression:

- Execute the prospective Phase 628 SQLite migrations through a failure-safe wrapper that
  rolls back an open transaction and restores the connection's original FK setting on every
  error. Do not alter the committed 063/064 migration bytes.
- Inject a deterministic mid-script failure and assert the returned pooled connection is in
  autocommit mode with its original `PRAGMA foreign_keys` value restored; also verify a normal
  replay still succeeds and preserves the manifest/authority bytes.

Resolution re-review: **resolved in the bounded working-tree fix**. The migration dispatcher
routes all three transaction-owning boot migrations 063, 064, and 065 through one guarded
executor without changing their SQL bytes. The executor refuses a pre-existing transaction,
captures and restores the caller's prior FK mode, detects a successful batch that omitted
`COMMIT`, rolls back failures, verifies autocommit, and runs `foreign_key_check` after success.
Unit coverage exercises FK ON/OFF, statement failure, missing commit, success, and caller-owned
transaction refusal. The populated boot fixture injects a failure before the real `COMMIT` in
each of 063/064/065, checks the complete `sqlite_master` plus seven authority tables for no
change, then performs two clean full-chain replays.

## File-by-file coverage

Ranges below are line numbers in the reviewed HEAD. “Changed hunks” means every base-to-HEAD
hunk was read together with the relevant callers, transactions, SQL triggers, and fixtures.

| File | Reviewed range |
| --- | --- |
| `server/src/api/jobs.rs` | All changed hunks at 867, 1481, and 12324-12494; backfill and tests |
| `server/src/api/jobs_managed_cloud_releases.rs` | All changed hunks, 7-14, 23-24, 51-62, 340-403, 423-438, 508-526 |
| `server/src/api/jobs_worker_auth.rs` | All changed hunks, 312-328, 551-567, 905-921 |
| `server/src/db/jobs.rs` | 2384 include registration |
| `server/src/db/jobs/applications.rs` | Every changed hunk, 627-5706; packet/account fences and test-fixture adjustments |
| `server/src/db/jobs/ats_certification_page.rs` | Changed test hunks, 2186 and 2871-2875 |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Every changed hunk across 1-23172; production contracts/validation 1-1041, 1920-3345, 4618-4975, 5821, 8765-9525, 10158-10182, 13552-16578, 17333-19125; changed tests 19263-23172 |
| `server/src/db/jobs/original_source_verification.rs` | Every changed hunk; binding/validation 172-2017, claim/probe/lease 3034-3420, heartbeats 3465-3624, projections 4174-4634, load/replay/complete 5321-7068, changed tests 8001-11859 |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | All changed fixture/lock-order hunks, 310-313, 570, 682, 762-765 |
| `server/src/db/jobs/source_verifier_boot.rs` | Entire new file, 1-1878 |
| `server/src/db/jobs/tests.rs` | All changed fixture hunks, 13164-13254 |
| `server/src/db/jobs/workspace.rs` | Every changed hunk, 204-3635; pagination/representation code and ordering fixtures |
| `server/src/db/mod.rs` | Every changed hunk; migration registration/execution 383-394, 1752-1759, 2713-2875, 2989-3008, 3150-3165; tests 3457-3597, 6371-6497, 7013-7017 |
| `server/src/jobs_managed_cloud_runtime.rs` | Changed heartbeat binding at 617-618 and its caller context |
| `infra/postgres/server-runtime/040_jobs_original_source_verifier_startup_authority.sql` | Entire file, 1-223 |
| `infra/postgres/server-runtime/041_jobs_source_verifier_boot_generation_authority.sql` | Entire file, 1-279 |
| `infra/postgres/server-runtime/042_jobs_source_verifier_heartbeat_boot_generation_fence.sql` | Entire file, 1-51 |
| `infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql` | Entire file, 1-150 |
| `infra/sqlite/server-runtime/062_jobs_original_source_verifier_startup_authority.sql` | Entire file, 1-277 |
| `infra/sqlite/server-runtime/063_jobs_source_verifier_boot_generation_authority.sql` | Entire file, 1-236 |
| `infra/sqlite/server-runtime/064_jobs_source_verifier_heartbeat_boot_generation_fence.sql` | Entire file, 1-46 |
| `infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql` | Entire file, 1-278 |

Review conclusions for the remaining covered code:

- API worker authentication admits the two boot endpoints without broadening unrelated paths;
  path/body runtime identity remains checked, and responses use canonical JSON with no-store
  and no-sniff headers.
- Optional boot ID/generation fields remain omitted for legacy v1/v2 bindings. New v3 lease,
  heartbeat, load, completion, and terminal-replay paths bind the actual attempt's immutable
  companion row to runtime, worker, and boot generation.
- The active source-verifier query joins registration, startup authority, heartbeat boot
  binding, and a head whose current and ready generations are equal. Startup promotion is from
  the exact degraded heartbeat sequence. A stale ready heartbeat cannot restore lease authority.
- Companion-row deletion is limited to parent-attempt cascade; insertion checks the actual
  attempt worker/runtime and the current+ready head in both prospective migrations.
- PostgreSQL acquisition continues to lock release head, manifest/runtime authority, boot
  authority, then domain/assignment state; no lock-order inversion was found in reviewed calls.
- The rollback-only leaseability probe uses `max(managed DB now, original-source DB now)` before
  the real lease operation, avoiding the prior split-clock false-idle result.
- Historical 040/062 startup migrations and committed 041/042/063/064 protocol migrations keep
  legacy behavior; boot-specific triggers apply only when the new boot binding is present.

## Reviewer-observed checks

- `git diff --check fc7e535143a262689da8e99ce7638aee42f40407..4ef949732ee50e0d8691e5eb809462e97eae0199`: passed.
- One authorized isolated offline harness used `scripts/run-bluey-tests.sh`, fresh Cargo/DB/config/
  data/runtime/log/temp roots, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_DEV_DEBUG=0`, both
  support features, `--offline --locked --all-targets`, and the three HANDOFF package opt-level
  overrides. PostgreSQL environment variables were asserted absent.
- `source_verifier_boot`: 20 passed, 0 failed, 1673 filtered; 26.33 seconds.
- `original_source_verifier_startup_leaseable_probe`: 1 passed, 0 failed, 1692 filtered;
  2.34 seconds.
- Harness status was 0. The exact owned root was
  `/private/tmp/bluey-test-runs-phase628-independent-20261027-4ef9497/run.1793079038.40518.2766431170`
  and `OWNED_ROOT_ABSENT_AFTER_EXIT=yes`.

These existing tests do not reproduce either P2 sequence above. This reviewer did not run a
full suite, Clippy, formatting, Docker, or a native PostgreSQL server. Any broader pass counts
or strict-Clippy statements in Phase documents are implementation-team claims, not checks
independently observed in this review.

The bounded fixes described above were reviewed statically by this reviewer. Parent-observed
full-suite evidence and the reviewer's later independent focused evidence are separated below.

## Parent-observed post-fix evidence

The following results were reported by the parent from isolated harness session `84478`; they
were not independently reproduced by this reviewer:

- Focused boot filter: 24 passed, 0 failed in 27.31 seconds, including the replacement-boot
  exact-replay regression, real 063/064/065 failure/replay coverage, and shared executor tests.
- SQLite migration replay filter: 23 passed, 0 failed.
- Strict all-target Clippy: passed.
- Full all-target suite: 1832 passed, 0 failed: 1697 library, 1 binary, and 134 integration
  tests. The library target took 2722.01 seconds and the 113-test HTTP suite took 545.73 seconds.
- Parent reported that both reviewed Rust file hashes remained unchanged from harness start.

## Reviewer-observed post-fix evidence

After the parent harness exited 0 and its owned root was confirmed absent, the reviewer ran one
fresh isolated launcher with no inherited PostgreSQL URLs. It set both test/dev debug levels to
zero, enabled both support features, used `--offline --locked --all-targets`, and applied the
HANDOFF opt-level 3 overrides for `curve25519-dalek`, `ed25519-dalek`, and `sha2`. The two Cargo
commands differed only by these filters:

```text
cargo [three package opt-level overrides] test --offline --locked \
  --manifest-path server/Cargo.toml --all-targets \
  --features integration-test-support,business-messaging-simulator-test-support \
  source_verifier_boot -- --test-threads=2
cargo [three package opt-level overrides] test --offline --locked \
  --manifest-path server/Cargo.toml --all-targets \
  --features integration-test-support,business-messaging-simulator-test-support \
  sqlite_migration_replay_tests -- --test-threads=2
```

Observed results:

- `source_verifier_boot`: 24 passed, 0 failed, 1673 filtered; 26.09 seconds.
- `sqlite_migration_replay_tests`: 23 passed, 0 failed, 1674 filtered; 24.41 seconds.
- Counts overlap because the first filter selects the three new migration-executor tests too.
- Harness exit status: 0. Initial compile: 2 minutes 40 seconds.
- Exact owned root:
  `/private/tmp/bluey-test-runs-phase628-independent-postfix-20261027/run.1793083219.99359.7835160200`.
  It was absent after exit, and its parent directory was empty.
- Both before/after source hashes were identical:
  `source_verifier_boot.rs` =
  `4ea1aafc9a0380d3785160a80b1f8541d0f50451f72c45a71b56a996bdc13554` and
  `db/mod.rs` = `fb28044343b86e59d8c2ad4005f58482df7d5786d1dc25315e89f789fe2247ea`.

## Release holds distinct from source findings

- The two P2 source findings are statically resolved and now have both parent-observed and
  independently observed focused execution evidence.
- Native PostgreSQL execution of 040-043 remains unproven here, including syntax, migration
  replay, trigger/cascade behavior, and concurrent registration/startup/lease behavior.
- Full all-target suite and strict all-target Clippy have parent-observed pass evidence only;
  formatting and remaining release evidence remain owned by the release process.
- Canonical review disposition, CI, Docker/Linux validation, and external-account deployment
  evidence were intentionally not approved or exercised by this review.
