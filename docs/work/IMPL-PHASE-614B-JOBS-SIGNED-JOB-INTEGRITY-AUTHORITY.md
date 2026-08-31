# IMPL: PHASE-614B — Jobs Signed Job Integrity Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against the current
> repository and the final Phase 614 authority record. The SSD archive was not used.

**Status:** Source implementation complete through FIX-766. Final JavaScript, native, full Rust,
explicit integration, release-containment, and fresh PostgreSQL 17.10 `r8` evidence is green.
Independent source/security and Phase 620 architecture reviews are green with no remaining P0–P2.
Production/release posture remains 🟡 for hosted/external gates and every external effect stays
disabled.

**Base commit:** `be925f89ccc5af2fb4b2ea41ba123c571fefadd6`

**Branch:** `feat/phase-614b-jobs-signed-integrity-authority`

**Frozen non-doc source manifest:** 44 files at Git `HEAD`
`be925f89ccc5af2fb4b2ea41ba123c571fefadd6`; canonical SHA-256 of sorted per-file SHA-256 lines
`fd49f94a6c8e22c373a848f74956abb0af57b62c090d8b4f40c15856b46359aa`.

## Outcome

Phase 614B now implements the signed job-integrity authority and the reviewed admission,
representation, lock-order, fixture, replay, and migration corrections recorded in FIX-725 through
FIX-766. Local evidence is green for JavaScript workspaces, native runner storage, the explicit
108-test integration target, the 1,586-test library aggregate, all remaining Rust targets, both
strict-Clippy profiles, default release containment, and the fresh PostgreSQL 17.10 `r8` 19-test
manifest. This is still source readiness, not production readiness: hosted/Docker/provider
evidence is absent, the PR stack is not merge-ready, and no flag or external effect was enabled.

## Scope

**Does:**

- add strict `JobIntegrityAttestationV1` and dual disjoint-role
  `JobIntegrityAuthorizationV1` authority;
- add monotonic delegated trust, revocation, immutable attestations, transitions, and current heads
  in exactly seven paired SQLite/PostgreSQL tables;
- compose current Phase 614 source authority with signed employer identity and job risk;
- freeze the exact integrity authority into application receipts and recompare it at every
  preparation-to-pre-Submit boundary;
- repair reservation/running admission so ATS and integrity drift cannot consume new capacity;
- make prepared-application approval freeze and queue transition atomic under the complete
  prelock;
- put the actual local-browser claim route under the canonical prelock while preserving
  `RunnerClaim` holds;
- require managed authority at the managed-effect database boundary;
- restore production-representative positive test fixtures only through public signed import and
  candidate-preference paths;
- preserve legitimate unmanaged cloud FinalSubmit only when durable workflow state proves that
  classification, while denying managed tuple omission or mismatch before mutation;
- recover historical exact-replay metadata from immutable transition history; and
- contain SQLite migration-058 failure by restoring autocommit and the incoming foreign-key state.

**Does NOT:**

- implement Round 615 source operations or freshness SLOs;
- build evidence generation, crawler, ATS adapter, competitor-derived UI, MCP, C2C, inbox, resume,
  or messaging features;
- inspect private competitor sessions or copy proprietary code/UI;
- add a new hold capability, activation/application-binding/signature table, production key, or
  provider credential;
- deploy, enable flags, perform provider writes, or claim production readiness;
- treat Tsenta, Giraffy, or other public product observations as authority evidence;
- implement the separate Phase 620 messaging, mailbox, MCP, or omnichannel plans; or
- merge, retarget, rebase, push, deploy, enable a flag, or perform an application/message/email.

## Frozen Authority Decisions

- The account-independent canonical envelope is `JobIntegrityAttestationV1` with audience
  `bluey-jobs-job-integrity-attestation-v1`.
- The same canonical bytes require separate authorizations from disjoint delegated roles
  `employer_identity` and `job_risk`; a third delegated role owns revocation.
- The offline root signs only a monotonic delegated-policy chain. Runtime trust starts from
  `BLUEY_JOBS_JOB_INTEGRITY_ROOT_TRUST_ANCHOR_JSON`.
- Employer statuses are `verified|unverified|mismatch`; risk statuses are
  `clear|review_required|blocked`. Only `verified + clear`, complete required evidence, and zero
  risk signals can yield `JobIntegrityCurrentAuthority`.
- `applicationDomain` remains a Phase 614 source/destination fact. The independently signed
  `canonicalEmployerDomain` is a distinct corporate-identity fact.
- Positive expiry is the minimum of source, attestation, policy, delegated-key, and evidence
  expiry under database time. Revoked, expired, or replaced positive authority never falls back.
- The paired relational model is exactly seven tables: trust policies, trust keys, attestations,
  revocations, head transitions, heads, and one control singleton.
- Exact replay is read-only. Historical replay returns the original immutable transition digest
  and revision even after the head advances. Changed identity, fork, predecessor gap, stale
  expected head, or compare-and-swap loss causes zero authoritative mutation.
- Effect-capable PostgreSQL readers use
  `H -> M -> ATS -> D -> integrity control FOR SHARE -> exact head FOR SHARE`.
- Operational-hold employer scope consumes a caller-resolved signed corporate domain after the
  common prelocks. Hold helpers do not relock authority or trust mutable posting JSON for that fact.
- Existing-reservation mutations occur only after complete ATS/source/integrity revalidation and
  current running-capability resolution.
- Prepared-kit finalization cannot commit `queued` before the exact `approved_execution` receipt.
  The implemented design performs approval freeze and queue transition atomically under the full
  prelock.
- The production local-browser API/browser-release route owns the complete prelock before mutation;
  standalone local-runner parity is secondary and cannot replace route evidence.
- The explicit managed-execution API requires its complete managed-authority tuple and rejects
  omission or mismatch before mutation. The shared FinalSubmit boundary classifies durable
  workflow state: it accepts tuple absence only for a proven unmanaged application and rejects an
  absent or mismatched tuple for a managed application. FIX-729 is the historical explicit-managed
  boundary record; FIX-762 owns the later shared managed/unmanaged classification.
- PostgreSQL effect paths use post-lock database time and the canonical dependency order. SQLite
  preserves the same logical no-mutation boundary in one authoritative transaction.
- SQLite migration 058 failure recovery rolls back any open transaction, restores and verifies the
  incoming foreign-key state, and runs `foreign_key_check` before returning.
- No `JobIntegrity` operational-hold capability is added.
- Positive fixtures save sponsorship `not_required` via the public preference path and import a
  real signed Phase 614 v2 source plus dual-signed Phase 614B authority through public paths. No
  direct-SQL shared positive helper is acceptable.

## Files Created / Modified

The frozen non-document source manifest contains 36 modified files and eight new files.

**Modified source (36):**

```text
.github/workflows/jobs-ci.yml
.github/workflows/release.yml
jobs/scripts/check-jobs-schema-parity.mjs
jobs/scripts/ci-guards-self-test.mjs
server/Cargo.toml
server/src/api/jobs.rs
server/src/api/jobs_worker_auth.rs
server/src/api/mod.rs
server/src/db/jobs.rs
server/src/db/jobs/applications.rs
server/src/db/jobs/ats_certification_authority.rs
server/src/db/jobs/browser_release_authority.rs
server/src/db/jobs/browser_release_registry.rs
server/src/db/jobs/communication_actions.rs
server/src/db/jobs/customer_data.rs
server/src/db/jobs/discovery.rs
server/src/db/jobs/eligibility.rs
server/src/db/jobs/evidence.rs
server/src/db/jobs/execution_authority.rs
server/src/db/jobs/execution_leases.rs
server/src/db/jobs/local_runner.rs
server/src/db/jobs/managed_cloud_release_authority.rs
server/src/db/jobs/operational_holds.rs
server/src/db/jobs/original_source_verification.rs
server/src/db/jobs/postgres_local_authority_tests.rs
server/src/db/jobs/profile_postings.rs
server/src/db/jobs/runner_volume_purge.rs
server/src/db/jobs/tests.rs
server/src/db/jobs/workflow_cleanup.rs
server/src/db/jobs/workflow_commands.rs
server/src/db/jobs/workspace.rs
server/src/db/mod.rs
server/src/db/object_uploads.rs
server/src/db/stripe_auto_reload.rs
server/src/lib.rs
server/tests/integration_e2e.rs
```

**New source (8):**

```text
infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql
infra/sqlite/server-runtime/058_jobs_signed_job_integrity_authority.sql
jobs/automation/tests/fixtures/job-integrity-authority-vectors.json
jobs/automation/tests/job-integrity-authority-vectors.test.ts
server/src/api/jobs_job_integrity.rs
server/src/db/jobs/job_integrity_authority.rs
server/src/db/jobs/job_integrity_composition.rs
server/src/db/jobs/production_positive_authority_fixture.rs
```

The documentation ledger is this Round, this IMPL, the independent REVIEW record, and FIX-725
through FIX-766. The separate Phase 620A and Phase 620B–F plans and the clean-room reference-package
audit are not part of the 44-file Phase 614B source manifest or its acceptance. No file under
`docs/reviews/` was changed. `CHANGELOG.md` contains the final Phase 614B/FIX-766 and design-only
Phase 620 entries.

## Implementation Checklist

- [x] Add the strict canonical envelope and role-specific authorization schemas.
- [x] Add root/delegated-policy, threshold, key-role separation, and revocation validation.
- [x] Add exactly seven paired SQLite/PostgreSQL tables and normal migration registration.
- [x] Enforce immutable rows, restrictive deletion, monotonic policy/revocation/head state, and
      exact replay/no-mutation rules.
- [x] Add the public trust-policy, attestation, and revocation package import paths.
- [x] Add the composed `JobIntegrityCurrentAuthority` resolver over exact current Phase 614 source
      authority.
- [x] Preserve application and canonical employer domains as distinct signed facts.
- [x] Bind the exact resolved integrity authority into frozen application receipts.
- [x] Recompare current authority at preparation, approval, queue, reservation, running, claim,
      dispatch, and pre-Submit.
- [x] Repair PostgreSQL effect ordering and add SQLite transactional parity.
- [x] Move existing-reservation mutations after complete revalidation and require current running
      capability.
- [x] Resolve signed corporate employer domain at callers after common prelocks; remove mutable
      posting/domain aliases from operational-hold authority.
- [x] Prelock prepared-application finalization and make approval freeze plus queue atomic.
- [x] Correct the actual local-browser API/browser-release claim order, preserve `RunnerClaim`
      holds, and add standalone-helper semantic parity.
- [x] Require complete managed authority for managed execution while preserving legitimate
      durable unmanaged FinalSubmit.
- [x] Replace shared positive fixtures with public sponsorship and signed source/integrity imports.
- [x] Add mapped rejection, replay, revocation, freshness, zero-mutation, privacy, and lock-order
      tests.
- [x] Preserve independently bound SmartRecruiters provider and application hosts end to end.
- [x] Evaluate PostgreSQL source, ATS, and integrity freshness at one post-fence database time.
- [x] Recover historical exact-replay transition metadata and harden migration-058 interruption
      recovery.
- [x] Freeze exact changed-file scope and source digests after implementation.
- [x] Complete the final full Rust aggregate, check, and strict-Clippy gates against the frozen
      source.
- [ ] Close the bounded evidence gaps retained by FIX-733 through FIX-735, FIX-739 through FIX-743,
      and any remaining exact-tip external checks.
- [x] Obtain independent corrective-diff source/security rereview; no remaining P0–P2 found.
- [x] Complete the final independent document review after the final aggregate evidence is
      attached; no remaining P0–P2 found.
- [x] Update `CHANGELOG.md` after the implementation scope is final.

## Build & Test Evidence

Only observed results are recorded. Local aggregate evidence is green but does not promote the
release verdict while hosted, Docker/Linux, provider, and runtime gates remain open.

### JavaScript, portal, and repository guards

| Evidence | Observed result |
| --- | --- |
| `node jobs/scripts/ci-guards-self-test.mjs` | Green: privacy scanned 2,648 tracked paths / 2,373 text files; schema parity 102 tables / 86 indexes; provenance 663 lock entries / 631 unique package versions / 1 override / 14 commit-pinned repositories |
| Browser release gate | 10/10 passed |
| Managed release gate | 17/17 passed |
| `npm test --prefix jobs` | 1,896 passed / 1 skipped: automation 720/1, browser 219, runner 308, workflows 300, portal 349 |
| `npm run typecheck --prefix jobs` | Green |
| `npm run build --prefix jobs` | Green; Vite chunk-size warnings only |
| Account-delete test | 3/3 passed |
| Checked-in portal bundle freshness | `git diff --exit-code -- web/jobs` green |

### Native runner storage

- Rust formatting, check, strict Clippy, tests, and release build were green for the native runner
  scope; 14 tests passed (one unit and 13 integration).
- The exact CI-like private-root addon smoke passed.
- A preliminary `/tmp` smoke was rejected with the expected `unsafe_entry`; it is not represented
  as a source failure.
- Docker was unavailable. No Docker/Linux runtime result is claimed.

### Focused server evidence

- Final formerly failing set: 16/16 passed in 20.74 seconds.
- Final certified sweep: 15/15 passed in 41.82 seconds.
- `job_integrity_authority_tests`: 18/18 passed; three configured-PostgreSQL branches self-skipped
  because that filtered run was not configured.
- FIX-763 exact lifecycle: SQLite 1/1 and configured PostgreSQL 17.10 passed; the final `r8`
  manifest supersedes the earlier `r6` checkpoint.
- FIX-764 injected SQLite migration interruption: 1/1 passed.
- `managed_unmanaged_submit_pairing`: 2/2 passed; its PostgreSQL branch self-skipped without a
  configured database.
- Legitimate unmanaged full boundary: 1/1 passed.
- `irreversible_submit`: 3/3 passed.
- Managed cloud FinalSubmit omission/wrong-worker no-mutation boundary: 1/1 passed.
- Stripe/Auto Reload lock-order evidence: static 1/1 and final configured PostgreSQL `r8` 1/1
  passed.
- An earlier full Rust aggregate was interrupted after approximately 900 tests with no observed
  failure. It is not a passing aggregate and is not release evidence.

### Final PostgreSQL 17.10 exact-name manifest

The first 19-case manifest is **SUPERSEDED** because it predates the final source digest:
`2026-08-30T05:37:39Z–05:38:16Z`, summed wall 37.10 seconds, test-binary SHA-256
`2aab625238a10ca16d164c819be7bb78c7750a5f2d1f1804e1938021d93fa7c8`.

The authoritative frozen-source `r8` manifest passed 19/19 sequentially on PostgreSQL 17.10
Homebrew aarch64 against a database created immediately before the run,
`bluey_phase614b_pg17_r8`, from `2026-08-30T09:32:42Z–09:33:44Z`, with summed observed real time
30.49 seconds. Test binary `server/target/debug/deps/bluey_server-0ad0add04d272872` had SHA-256
`1eb9fb796a5789c53dfbacd7d47fc90f4347ddbbc15c50b99d61d6bd6e947f0b`; every exact filter
reported 1 passed / 0 failed / 1,585 filtered out. A reused-`r6` identity conflict and the clean
partial `r7` diagnostic are not accepted as final evidence.

1. `db::jobs::job_integrity_authority_tests::postgres_positive_lifecycle_when_configured`
2. `db::jobs::job_integrity_authority_tests::postgres_representation_fence_blocks_integrity_publication_when_configured`
3. `db::jobs::job_integrity_authority_tests::postgres_public_integrity_apis_enter_blocking_boundary_when_configured`
4. `db::jobs::job_integrity_composition_tests::postgres_lock_first_read_committed_observes_waited_writer_when_configured`
5. `db::jobs::postgres_local_authority_tests::postgres_execution_authority_locks_integrity_before_account_policy`
6. `db::jobs::queue_admission_tests::postgres_read_committed_refreshes_authority_after_waiting_for_prelock`
7. `db::jobs::queue_admission_tests::postgres_application_first_row_order_prevents_reserve_save_cycle`
8. `db::jobs::queue_admission_tests::postgres_evidence_wait_expiry_leaves_prepared_rows_unmodified`
9. `db::jobs::queue_admission_tests::postgres_packet_account_first_avoids_account_entitlement_inversion`
10. `db::jobs::fix_753_postgres_communication_contention_uses_post_lock_time`
11. `db::jobs::submission_post_lock_time_tests::postgres_evidence_namespace_wait_precedes_final_expiry_clock`
12. `db::object_uploads::tests::postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing`
13. `db::jobs::workspace_representation_tests::postgres_workspace_list_and_detail_allow_lockable_snapshot_reads`
14. `db::jobs::postgres_local_authority_tests::postgres_auto_submit_revocation_and_execution_share_one_fence`
15. `db::jobs::postgres_local_authority_tests::postgres_local_run_claim_and_submit_recheck_current_authority`
16. `db::jobs::postgres_local_authority_tests::postgres_operational_context_holds_account_fence_through_admission_commit`
17. `db::jobs::tests::postgres_ats_head_replacement_cannot_reserve_or_start_when_configured`
18. `db::jobs::managed_cloud_release_authority_tests::postgres_managed_unmanaged_submit_pairing_guards_are_fail_closed_when_configured`
19. `db::stripe_auto_reload::tests::postgres_auto_reload_reversal_and_account_metering_share_lock_order_when_configured`

### Final Rust, compile-hygiene, and release-containment gates

- `CARGO_INCREMENTAL=0 cargo test --locked --manifest-path server/Cargo.toml
  --no-default-features --features integration-test-support --test integration_e2e --
  --test-threads=1` passed 108/108 with zero failed/ignored/measured/filtered in 543.78 seconds.
  Binary SHA-256:
  `4cc05b9abe006f8dc23bdf79ba2bc56323f579fa09e755f409ce842838e96b10`.
- `CARGO_INCREMENTAL=0 cargo test --locked --manifest-path server/Cargo.toml --lib` passed
  1,586/1,586 with zero failed/ignored/measured/filtered in 2,178.55 seconds. Binary SHA-256:
  `1eb9fb796a5789c53dfbacd7d47fc90f4347ddbbc15c50b99d61d6bd6e947f0b`.
- Remaining targets passed: ConnectInfo 1/1, context migration 1/1, GDPR cleanup 2/2, runner-plan
  matrix 2/2, configured PostgreSQL schema 1/1, startup binary 1/1, and doc tests 0/0.
- Locked default and support-feature checks passed. Locked default all-target and explicit
  support-feature integration Clippy passed with `-D warnings` and zero warnings.
- Default locked release builds passed without warnings. `bluey-server` SHA-256 is
  `4cc6937745138957f91946981fbf5206f7f03f2bb672e783bc3ebb2952f3550f`; `bluey-jobs-api`
  SHA-256 is `4c662fcfcaa4e9b54adb12a27e09e1a2b937b4a3ec6bdb15779231c8e26ec30a`.
- The support feature is excluded from normal dependencies and feature-off aggregates. A release
  check with it enabled exited 101 only at the deliberate `compile_error!`, and release-binary
  `strings` scans found none of its feature/fixture identifiers.

Independent source/security and final documentation rereviews are green with no remaining P0–P2.
Exact-tip hosted CI, Docker/Linux, production key custody, and production read-back remain pending.

## Clean-Room Public Product Findings (Non-Authority)

These public observations are product-roadmap inputs only. They are not integrity, implementation,
release, or provider-source evidence, and no private competitor session or undocumented endpoint
was used.

- Bluey's public `/jobs/` is a healthy review-first marketing surface for discovery, application
  kits, and invited-beta local/cloud runners; it does not prove this unpushed source.
- Giraffy's public surfaces describe discovery/matching, resumes, contacts, alerts, market data,
  tracking, manual/extension final submission, C2C Autopilot, Agent Connect/MCP, opportunity maps,
  upskilling, salary, and sponsorship. Its public source classes include employer/ATS pages,
  boards/direct pipelines, curated or hidden leads, and recruiter email/network requirements.
  Those are vendor claims, not independently verified behavior.
- Tsenta's public pages describe find/prep/apply/track, 50,000 career pages and 19 ATS families.
  `/messaging` presents Text to Apply and WhatsApp entry points; `/mcp` describes OAuth and a
  Streamable-HTTP-style endpoint at `https://api.autojobs.me/api/v1/mcp` plus Claude, Cursor, and
  Codex setup. No endpoint was probed.
- Existing public clean-room notes for LazyApply, Sonara, AIApply, LoopCV, Simplify, ApplyCot, and
  similar services remain roadmap input; they are not recharacterized as implementation evidence.

The defensible product lesson is a unified career command center with explicit source provenance,
resume-first activation, reviewable packets, a transparent tracker, and C2C, messaging, and MCP as
separately authorized services. It does not authorize copying UI, code, or private logic.

## Phase 620 Separation

`PLAN-PHASE-620A-JOBS-SIMULATOR-FIRST-BUSINESS-MESSAGING-CONTROL-PLANE.md` is a separate,
design-only plan that was independently re-reviewed green as a design. It is not implemented,
launched, or included in the Phase 614B 44-file source manifest or acceptance. It keeps all flags
at `0` and all egress disabled, separates WhatsApp Business Platform from Third Party Agent
Platform, treats personal WhatsApp and unattended personal iMessage as unsupported, and models
Apple Messages for Business separately. Its simulator is no-egress; WhatsApp eligibility remains
fail-closed, WhatsApp data is excluded from training/improvement, and OAuth write authority stays
`0`. Its SHA-256 is
`3076d531f4de783df256c7431ad88a072e5779ffcc8f68e83ad556df89aff701`.

`PLAN-PHASE-620B-F-JOBS-OMNICHANNEL-OUTREACH-AND-AGENT-CONTROL-PLANE.md` extends that design with
delegated-only Gmail/Microsoft mailbox authority and recovery, hostile-content isolation, MCP
session/resumption and cross-node binding, client inventory, cross-channel routing/deduplication,
fallback, and STOP semantics. Independent rereview found no remaining P0–P2 at SHA-256
`968fd808b2da9f1730d2d188844c0811b3c5a3c4e88be15ef2cb850e3092bd12`.

`AUDIT-JOBS-REFERENCE-PACKAGES-CLEAN-ROOM.md` records the static clean-room review of the three
user-supplied ZIPs at SHA-256
`4243690318e57ce2b313dace663a3c79668070d07fa2b72c24f335e4fb89273c`; no code was imported.

## PR #26–#32 Read-Only Audit

All seven pull requests are open drafts, GitHub reports them mergeable, and none has a review,
review decision, or review request. This is a metadata audit, not authorization to mutate the
stack.

| PR | Base → head | GitHub state/checks | Blocker |
| --- | --- | --- | --- |
| [#26](https://github.com/Dhanunjay-Divi/bluey/pull/26) `feat(jobs): add ATS certification authority` | `feat/phase-603-jobs-local-browser-release-authority` `427e3e3dda1367d302608225dadbecbfe9fac904` → `feat/phase-604-jobs-ats-certification` `8845566bcb4a2c8c56a3c525e196676273b3433c` | CLEAN; 0 checks | No checks; base anchor has no discovered PR |
| [#27](https://github.com/Dhanunjay-Divi/bluey/pull/27) `feat(jobs): add reviewed communication execution authority` | `8845566bcb4a2c8c56a3c525e196676273b3433c` → `feat/phase-605-jobs-communication-execution` `936fba2419e093eb6dd9d27a764d3c4bc8c6fb25` | CLEAN; 10 success / 5 skipped | Draft/no review |
| [#28](https://github.com/Dhanunjay-Divi/bluey/pull/28) `feat(jobs): add launch safety control plane` | `936fba2419e093eb6dd9d27a764d3c4bc8c6fb25` → `feat/phase-606-jobs-launch-safety` `b1ed19024b2a24488771e2cef6328bb655274fbb` | CLEAN; 9 success | Draft/no review |
| [#29](https://github.com/Dhanunjay-Divi/bluey/pull/29) `feat(jobs): make web automation cloud-first` | `b1ed19024b2a24488771e2cef6328bb655274fbb` → `feat/phase-608-jobs-cloud-first-web` `792506b0281d29aaedaa1e87be284432b2353bb6` | CLEAN; 9 success | Draft/no review |
| [#30](https://github.com/Dhanunjay-Divi/bluey/pull/30) `feat(jobs): add durable workflow command authority` | `792506b0281d29aaedaa1e87be284432b2353bb6` → `feat/phase-609-jobs-workflow-command-outbox` `2e2a18a919b03a500e56713e4c0be2aa081f80ab` | CLEAN; 9 success | Exact ancestry diverges: ahead 3 / behind 1, merge base `7d96fb68738e8069c36db9b9d9e7ff079f1fa0ab`; explicit lineage decision required |
| [#31](https://github.com/Dhanunjay-Divi/bluey/pull/31) `feat(jobs): add durable workflow cleanup authority` | `2e2a18a919b03a500e56713e4c0be2aa081f80ab` → `feat/phase-610-jobs-workflow-cleanup-authority` `89d6b820ae46a42557c8ce76d9d632465a67b99c` | CLEAN; 9 success | Draft/no review; depends on #30 lineage |
| [#32](https://github.com/Dhanunjay-Divi/bluey/pull/32) `feat(jobs): add managed cloud launch authority` | `89d6b820ae46a42557c8ce76d9d632465a67b99c` → `feat/phase-611-jobs-managed-cloud-launch-authority` `423fba5c45ff20f00631d650b28f5a277a97985d` | UNSTABLE; 8 success / 1 failure | Managed-runner image build/smoke failed; steps 23–27 skipped; local `f50103e8` fix is absent remotely |

The chain is not merge-ready. The phase-603 base is 14 commits ahead / 0 behind `main`
`755e7d71c5ec15ea7079f7dce5a32f02b7b1fcab` without a discovered anchoring PR; #30 diverges; #32
is failed and lacks the local handoff fix; and every PR remains draft and unreviewed. #32's remote
tip is 31 commits ahead / 0 behind `main`.

## Flag And Side-Effect Ledger

```text
sourceVerification in every current activation            unchanged false
directDiscovery                                            unchanged false
globalDiscovery                                            unchanged false
All production/provider-write flags                        unchanged 0
Authenticated provider or competitor-private access       not performed
Applications, emails, messages, deployment, key creation   not authorized or performed
Customer cohort or production flag read-back               not performed
Phase 620 simulator/provider egress                         unchanged 0 / none
```

## Deviations From Plan

| Deviation | Rationale | Status |
| --- | --- | --- |
| Defect ledger expanded from FIX-725–752 through FIX-766 | Review and verification found authority, fixture, replay, pairing, PostgreSQL, migration-recovery, clock-margin, and test-containment gaps | Implemented and locally verified; evidence posture remains per FIX record |
| Shared FinalSubmit classification supersedes the broad historical FIX-729 wording | Legitimate unmanaged workflow execution must remain possible while managed applications require the full tuple | Implemented in FIX-762 |
| Phase 620 messaging/MCP work is separate | Public product findings informed simulator-first and omnichannel designs, not Phase 614B source or acceptance | Design-only; flags/effects zero |

## Known Follow-Ups

- Keep Round 615 reserved for source enrollment, rights, scheduling, and operated freshness SLOs.
- Close the explicit yellow evidence retained by FIX-733 through FIX-735 and FIX-739 through
  FIX-743 before promoting the verdict.
- Resolve the PR-chain anchor, #30 ancestry divergence, #32 failed job, and missing remote handoff
  fix through separately authorized Git operations.
- Keep clean-room Tsenta/Giraffy and broader competitor findings in separately reviewed roadmap
  phases; do not merge Phase 620 into this acceptance record.
- Run production key custody, live PostgreSQL, Docker/Linux, hosted CI, canary, rollback, and
  activation evidence only under separate authority.
- Carry the Phase 614 dependency advisories and load-sensitive runner sentinel as separately scoped
  production-readiness work; do not silently absorb or waive them here.

## Review Checklist

- [x] Current files match the frozen Phase 614B 44-file source manifest.
- [x] No unrelated work or file under `docs/reviews/` is included.
- [x] Canonical/signature, authority separation, public positive-fixture, and current-authority
      mechanisms are implemented through FIX-766.
- [x] Observed focused, JavaScript, native, final PostgreSQL, and full Rust aggregate evidence is
      recorded without promoting incomplete external evidence.
- [x] Production/write flags remain false/`0`; no deployment or private competitor action occurred.
- [ ] Close the explicit behavioral/evidence gaps retained by yellow FIX records.
- [x] Independent corrective-diff source/security rereview is green with no remaining P0–P2.
- [x] Record the final full Rust, check, and strict-Clippy results against the frozen source.
- [x] Complete final independent document review after the evidence records are resolved; no
      remaining P0–P2 found.
- [ ] Resolve or explicitly accept every PR-chain blocker before any merge decision.
- [ ] External-only evidence is attached before any activation or release verdict is promoted.
