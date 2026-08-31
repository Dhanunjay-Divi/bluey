# FIX-760 — PostgreSQL 17 authority evidence

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> the current local handoff, and the authoritative worktree. The SSD archive was not used.

**Severity:** P1 configured-database release-evidence blocker

**Status:** Implemented and locally verified on PostgreSQL 17.10. The frozen-source `r8`
exact-name manifest passed 19/19. This closes the local configured-database evidence blocker only;
the overall release remains yellow pending external and hosted evidence.

## Issue

The Phase 614B configured-PostgreSQL lane exposed migration replay, SQL typing, SQLSTATE mapping,
contention-fixture naming, reservation-state, and stale assertion defects that SQLite could not
exercise. The authority design was fail closed, but the exact PostgreSQL evidence lane was not
credible until every test ran against a fresh PostgreSQL 17 database.

## Root Cause

- The new migration's relation guard did not isolate all preexisting-schema cases.
- PostgreSQL inferred ambiguous integer/operator types in several sequence predicates.
- A revocation constraint failure was mapped through the wrong SQLSTATE expectation.
- Managed release queries used stale aliases/casts.
- Long contention-test `application_name` values could be truncated and fail waiter detection.
- Some fixtures expected an unstarted reservation after a successful claim or asserted an obsolete
  integrity projection.

## Fix Summary

- Make migration replay relation checks isolated and deterministic.
- Use explicit `BIGINT` arithmetic for original-source and workflow sequence predicates.
- Correct revocation SQLSTATE handling and managed release aliases/casts.
- Shorten unique PostgreSQL contention labels while retaining collision resistance.
- Transition claimed reservations to `running` where the test models worker execution.
- Update only stale evidence assertions; preserve `H -> M -> ATS -> D`, account-fence, publication-
  fence, row-lock, and post-lock database-time invariants.
- Make the configured migration-catalog regression require exactly one ledger row for migrations
  035 and 036, the exact seven current Phase 614B tables, and exactly 20 enabled non-internal Phase
  614B triggers.
- Re-run the configured authority/contention set serially on a newly created PostgreSQL 17.10
  `r8` database after the final source freeze.

## Files Modified

| File | Change |
| --- | --- |
| `server/src/db/mod.rs` | Migration replay relation guard. |
| `server/src/db/jobs/job_integrity_authority.rs` | PostgreSQL revocation/error evidence. |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Correct aliases and casts. |
| `server/src/db/jobs/original_source_verification.rs` | Explicit `BIGINT` sequence arithmetic. |
| `server/src/db/jobs/workflow_cleanup.rs` | Explicit PostgreSQL integer arithmetic. |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Reservation-state and exact Phase 614B catalog evidence. |
| `server/src/db/jobs/applications.rs` | Bounded contention labels. |
| `server/src/db/jobs/customer_data.rs` | Bounded contention labels. |
| `server/src/db/jobs/communication_actions.rs` | Bounded contention labels and lock checks. |

## Evidence

- **PASS:** PostgreSQL 17.10 accepted connections on the isolated local authority server.
- **SUPERSEDED:** an earlier fresh-database `r4` run reported 17 passed and 0 failed, but no durable
  exact-name transcript and frozen-source digest survived. It is not accepted as release evidence.
- **SUPERSEDED/INCOMPLETE:** read-only manual catalog checks on
  `bluey_phase614b_pg17_r5` observed one migration-ledger row for each of 035/036, the exact seven
  current Phase 614B tables, and 20 enabled non-internal Phase 614B triggers. The source changed
  afterward, and `r5` has no durable frozen-source exact-test manifest; those observations are
  diagnostics only.
- **SUPERSEDED:** the preliminary `r6` manifest from `2026-08-30T05:37:39Z` through
  `2026-08-30T05:38:16Z` reported 19/19 against test-binary SHA-256
  `2aab625238a10ca16d164c819be7bb78c7750a5f2d1f1804e1938021d93fa7c8`. FIX-763, FIX-764,
  and fixture corrections changed the source afterward, so neither that digest nor its timings are
  final evidence.
- **SUPERSEDED:** the then-finalized `r6` manifest ran strictly serially
  from `2026-08-30T05:51:02Z` through `2026-08-30T05:51:39Z` with 19 passed and 0 failed.
  Each exact invocation reported `1 passed; 0 failed; 1585 filtered out`; summed observed wall time
  was `36.71s`. The local transcript was
  `/tmp/bluey-phase614b-pg17-r6-final.DKzlt0`. FIX-765/FIX-766 and the final warning cleanup changed
  source afterward, so this is retained history, not current evidence.
- **DIAGNOSTIC:** the frozen binary passed the first four exact cases on reused `r6`, then case 5
  encountered a fixed fixture-identity conflict left by prior aggregate/configured runs. A newly
  created `r7` database passed the same first five cases, proving the failure was reused fixture
  state rather than a source regression. Neither partial run is accepted as the final manifest.
- **PASS — frozen-source `r8` manifest:** `r8` was created immediately before the reviewed run.
  The complete 19-test manifest ran strictly serially from `2026-08-30T09:32:42Z` through
  `2026-08-30T09:33:44Z` with **19 passed and 0 failed**. Every exact invocation reported
  `1 passed; 0 failed; 1585 filtered out`; summed observed real time was `30.49s`.
- **Database identity:** database `bluey_phase614b_pg17_r8` on
  `PostgreSQL 17.10 (Homebrew) on aarch64-apple-darwin25.4.0`, compiled by Apple clang
  `21.0.0 (clang-2100.0.123.102)`, 64-bit.
- **Binary identity:** `server/target/debug/deps/bluey_server-0ad0add04d272872`, SHA-256
  `1eb9fb796a5789c53dfbacd7d47fc90f4347ddbbc15c50b99d61d6bd6e947f0b`.
- **Frozen source identity:** Git `HEAD` `be925f89ccc5af2fb4b2ea41ba123c571fefadd6`; canonical
  non-document changed-file digest
  `fd49f94a6c8e22c373a848f74956abb0af57b62c090d8b4f40c15856b46359aa`.
- **PASS — subsumed checkpoint:** the earlier standalone configured FIX-753 Auto Reload/account-
  metering contention pass is now covered by final-manifest test 19.
- **PENDING / release-yellow:** hosted PostgreSQL TLS/network/proxy/role behavior,
  interruption/failover, backup/restore, and production connection-pool evidence remain external
  gates. This local manifest is not deployment authorization.

### Final exact-name manifest

1. `db::jobs::job_integrity_authority_tests::postgres_positive_lifecycle_when_configured` —
   `0.17s`.
2. `db::jobs::job_integrity_authority_tests::postgres_representation_fence_blocks_integrity_publication_when_configured`
   — `0.36s`.
3. `db::jobs::job_integrity_authority_tests::postgres_public_integrity_apis_enter_blocking_boundary_when_configured`
   — `0.14s`.
4. `db::jobs::job_integrity_composition_tests::postgres_lock_first_read_committed_observes_waited_writer_when_configured`
   — `0.89s`.
5. `db::jobs::postgres_local_authority_tests::postgres_execution_authority_locks_integrity_before_account_policy`
   — `3.73s`.
6. `db::jobs::queue_admission_tests::postgres_read_committed_refreshes_authority_after_waiting_for_prelock`
   — `0.03s`.
7. `db::jobs::queue_admission_tests::postgres_application_first_row_order_prevents_reserve_save_cycle`
   — `0.03s`.
8. `db::jobs::queue_admission_tests::postgres_evidence_wait_expiry_leaves_prepared_rows_unmodified`
   — `0.53s`.
9. `db::jobs::queue_admission_tests::postgres_packet_account_first_avoids_account_entitlement_inversion`
   — `0.05s`.
10. `db::jobs::fix_753_postgres_communication_contention_uses_post_lock_time` — `0.80s`.
11. `db::jobs::submission_post_lock_time_tests::postgres_evidence_namespace_wait_precedes_final_expiry_clock`
    — `0.89s`.
12. `db::object_uploads::tests::postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing`
    — `0.96s`.
13. `db::jobs::workspace_representation_tests::postgres_workspace_list_and_detail_allow_lockable_snapshot_reads`
    — `0.43s`.
14. `db::jobs::postgres_local_authority_tests::postgres_auto_submit_revocation_and_execution_share_one_fence`
    — `0.55s`.
15. `db::jobs::postgres_local_authority_tests::postgres_local_run_claim_and_submit_recheck_current_authority`
    — `10.76s`.
16. `db::jobs::postgres_local_authority_tests::postgres_operational_context_holds_account_fence_through_admission_commit`
    — `3.22s`.
17. `db::jobs::tests::postgres_ats_head_replacement_cannot_reserve_or_start_when_configured`
    — `6.70s`.
18. `db::jobs::managed_cloud_release_authority_tests::postgres_managed_unmanaged_submit_pairing_guards_are_fail_closed_when_configured`
    — `0.03s`.
19. `db::stripe_auto_reload::tests::postgres_auto_reload_reversal_and_account_metering_share_lock_order_when_configured`
    — `0.22s`.

## How To Test

```bash
BLUEY_TEST_POSTGRES_URL=<fresh-isolated-postgres-url> \
  server/target/debug/deps/<exact-bluey-server-test-binary> \
  '<exact-test-name-from-the-reviewed-manifest>' \
  --exact --nocapture --test-threads=1
```

## Known Limitations

- Local PostgreSQL is not evidence for hosted networking, failover, backup, restore, or production
  connection-pool behavior.
- Docker/Linux, Temporal, registry publication/read-back, threshold signing, protected approvals,
  real runner capacity, ATS canaries, customer cohorts, kill switches, and rollback rehearsal remain
  outside this local PostgreSQL proof. Production flags remain `0`.
- No production database, flag, deployment, or customer data was touched.
