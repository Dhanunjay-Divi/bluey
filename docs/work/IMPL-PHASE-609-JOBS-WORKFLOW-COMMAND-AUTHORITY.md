# IMPL: PHASE-609 - Jobs Workflow Command Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the Phase 609 successor
> worktree before documenting this batch. No SSD/archive, production service, provider credential,
> live tenant, deployment, or external write was used.

## Scope

**Does:**

- Commits encrypted immutable start/resume workflow commands with local business authority.
- Removes direct request-handler workflow delivery and compensation after transport ambiguity.
- Adds a disabled-by-default, lease-fenced command dispatcher with request-start evidence.
- Implements a closed Temporal protocol v2 with exact conflict identity and intervention-bound
  Updates.
- Keeps deterministic workflow history opaque and converts failures to closed reason codes.
- Implements server-owned two-phase intervention prepare/publish authority.
- Finalizes exact runner failure, runner ambiguity, intervention timeout, and intervention-limit
  outcomes and marks trusted submitted receipts terminal across replay ordering.
- Recovers an exact resume Update across the running-to-closed race while keeping timeout-only
  absence ambiguous, and accepts only exact closed Jobs API errors.
- Binds runner results to the exact request/run identity, persists minimal encrypted ambiguity
  authority before reply, and makes restart recovery aware of durable submitted/failed results.
- Proves the account/application-identity-derived profile scope before startup persistence and
  first tries current restore for an exact expired committed intervention. Only an expired claim
  rejected as `lease_unavailable` may use old token/fence reconciliation/removal; other errors fail
  closed.
- Retries cleanup credential drift after canonical commit and enforces one symmetric safe grammar
  for the workflow bearer token at both ends.
- Keeps the browser-delivered `/jobs/automation` product state recoverable while command delivery
  remains queued or ambiguous.
- Retains an isolated cleanup-service library and fail-closed database scaffolding for Phase 610,
  while the gateway imports/registers no cleanup route and environment cannot enable one.

**Does NOT:**

- Implement authenticated, paginated inventory or drain evidence for legacy Temporal workflows.
- Allow a workflow cleanup generation to complete; caller-provided legacy-zero claims are rejected.
- Wire workflow cleanup into account deletion, object sweep, or hard deletion.
- Prove cleanup observation/fence behavior against hosted Temporal or accept it as complete.
- Enable workflow-command dispatch, workflow cleanup, cloud/local Browser distribution, model
  generation, mailbox, communication, ATS, provider, tenant, or production flags.
- Deploy any service, use credentials, mutate a live tenant, or claim a hosted canary.
- Merge the parked Phase 607 installed-Bluey-Browser branch.

## Files Created / Modified

This is the exact Phase 609 working-tree inventory at the documentation checkpoint, including these
documentation and operations updates. Generated build directories are not tracked.

| File | Action | Purpose |
|------|--------|---------|
| `CHANGELOG.md` | Modified | Record the durable v2 command authority and parked cleanup boundary |
| `docs/rounds/ROUND-609-JOBS-WORKFLOW-COMMAND-AUTHORITY.md` | Created | Freeze scope, contracts, gates, and honest completion boundary |
| `docs/work/FIX-687-workflow-command-delivery-ambiguity.md` | Created | Record the ambiguity root cause and source fix |
| `docs/work/IMPL-PHASE-609-JOBS-WORKFLOW-COMMAND-AUTHORITY.md` | Created | Implementation inventory and verification handoff |
| `docs/work/REVIEW-PHASE-609-JOBS-WORKFLOW-COMMAND-AUTHORITY.md` | Created | Record independent review and accepted local-source verdict |
| `infra/postgres/server-runtime/031_jobs_workflow_commands.sql` | Created | PostgreSQL command/execution/intervention/finalization/cleanup schema |
| `infra/sqlite/server-runtime/053_jobs_workflow_commands.sql` | Created | SQLite parity schema and fail-closed constraints |
| `jobs/OPERATIONS.md` | Modified | Document independent disabled flags and staged rollout truth |
| `ops/bluey-jobs.env.example` | Modified | Keep dispatch disabled and cleanup configuration reserved/inert |
| `jobs/automation/src/worker-auth.ts` | Modified | Sign exact internal workflow material/intervention/finalization paths |
| `jobs/automation/tests/worker-auth.test.ts` | Modified | Cover the closed internal path/auth contract |
| `jobs/package-lock.json` | Modified | Freeze the workflows data-converter dependency graph |
| `jobs/portal/src/App.tsx` | Modified | Preserve durable queued workflow state in the cloud-first portal |
| `jobs/portal/src/lib/application-flow.test.ts` | Modified | Cover queued/ambiguous workflow application projection |
| `jobs/portal/src/lib/application-flow.ts` | Modified | Decode and present durable workflow command state |
| `jobs/portal/src/types.ts` | Modified | Add closed command-state response types |
| `jobs/runner/src/certified-final-submit.ts` | Modified | Persist the conservative pre-I/O final-submit recovery boundary |
| `jobs/runner/src/run-checkpoint-store.ts` | Modified | Store exact V2 checkpoint and minimal encrypted ambiguity authority |
| `jobs/runner/src/server.ts` | Modified | Bind runner results and restart recovery to the exact opaque request/run identity |
| `jobs/runner/src/submitted-result-recovery.ts` | Modified | Promote an exact staged submitted result through fenced finish replay |
| `jobs/runner/tests/execution-lease.test.ts` | Modified | Cover the pre-I/O final-submit marker boundary |
| `jobs/runner/tests/run-checkpoint-store.test.ts` | Modified | Cover request-ID checkpoint and ambiguity-tombstone authority |
| `jobs/runner/tests/server-result-recovery.test.ts` | Modified | Cover result binding, restart, and expired-claim reconciliation fallback |
| `jobs/runner/tests/submitted-result-recovery.test.ts` | Modified | Cover submitted-result promotion and identity conflicts |
| `jobs/workflows/package.json` | Modified | Declare the Temporal common data-converter dependency |
| `jobs/workflows/src/activities.ts` | Modified | Materialize opaque commands, run two-phase interventions, and finalize outcomes |
| `jobs/workflows/src/contracts.ts` | Modified | Define closed v2, cleanup, intervention, and terminal contracts |
| `jobs/workflows/src/failure-converter.ts` | Created | Replace history-visible raw failures with closed opaque failures |
| `jobs/workflows/src/gateway-cleanup-service.ts` | Created | Retain the unimported Phase 610 cleanup-service scaffold |
| `jobs/workflows/src/gateway-service.ts` | Created | Implement exact start/resume protocol-v2 gateway behavior |
| `jobs/workflows/src/gateway.ts` | Modified | Serve exact authenticated commands and register no cleanup route |
| `jobs/workflows/src/worker.ts` | Modified | Register the opaque v2 workflow and failure converter |
| `jobs/workflows/src/workflows.ts` | Modified | Run opaque v2 lifecycle, Updates, interventions, and terminalization |
| `jobs/workflows/tests/activities-auth.test.ts` | Modified | Cover signed internal calls, closed headers/contracts, and failures |
| `jobs/workflows/tests/failure-converter.test.ts` | Created | Prove raw failure data is not serialized into history |
| `jobs/workflows/tests/gateway-cleanup-service.test.ts` | Created | Exercise the isolated scaffold directly while excluding its route |
| `jobs/workflows/tests/gateway-http.test.ts` | Created | Cover exact HTTP method/path/auth/body/header behavior |
| `jobs/workflows/tests/gateway-service.test.ts` | Created | Cover fresh start, exact replay, mismatch, closed reuse, and Update binding |
| `jobs/workflows/tests/protocol-v2-privacy.test.ts` | Created | Scan deterministic v2 surfaces for private data |
| `jobs/workflows/tests/workflows.test.ts` | Modified | Cover two-phase intervention and terminal lifecycle behavior |
| `server/src/api/jobs.rs` | Modified | Admit commands atomically and expose closed worker material/prepare/publish/finalize routes |
| `server/src/api/jobs_worker_auth.rs` | Modified | Authorize exact opaque internal workflow endpoints |
| `server/src/bin/bluey-jobs-api.rs` | Modified | Start the command dispatcher only when independently enabled |
| `server/src/db/jobs.rs` | Modified | Export the workflow-command database module |
| `server/src/db/jobs/workflow_commands.rs` | Created | Own command, lease, execution, intervention, finalization, and cleanup state machines |
| `server/src/db/mod.rs` | Modified | Apply paired migrations and verify dialect/schema guards |
| `server/src/jobs_workflow_dispatch.rs` | Created | Deliver frozen commands to the exact v2 gateway with ambiguity-safe retries |
| `server/src/lib.rs` | Modified | Export the dispatcher module |
| `server/src/main.rs` | Modified | Start the independently gated dispatcher in the shared server binary |
| `server/tests/integration_e2e.rs` | Modified | Cover durable API admission, worker routes, limits, and replay boundaries |
| `server/tests/jobs_runner_plan_matrix.rs` | Modified | Keep plan/cloud availability aligned with durable workflow authority |
| `web/jobs/index.html` | Modified | Regenerate the production portal entry against the Phase 609 source |
| `web/jobs/assets/ApplicationsView-CIFDd8qd.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/AutomationView-CALLx7Vd.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/MatchesView-Cpq-wqxD.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/ResumeView-BCcWlUw_.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/SettingsView-DYgFyc9k.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/candidate-events-DvnejbjA.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/chevron-right-DvDnS7o1.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/index-CJk1Orrz.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/index.es-D65cf-jo.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/jspdf.es.min-DKUH5PwK.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/mail-BeXuzY8O.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/monitor-up-BcbMmm63.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/pencil-BOd0ADl2.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/play-DouVOa6h.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/resume-diff-BlY0x8VI.js` | Removed | Replace the prior hashed portal chunk |
| `web/jobs/assets/ApplicationsView-DicwM3p_.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/AutomationView-DkslewE1.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/MatchesView-HW9k_UPa.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/ResumeView-D4s3IwL6.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/SettingsView-B56lgyda.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/candidate-events-W6EOwYnA.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/chevron-right-C3cAtRY3.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/index-CZOVDDyb.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/index.es-NzB6eH72.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/jspdf.es.min-DZNANvoU.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/mail-CYLv0TQ-.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/monitor-up-nkl1PB_A.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/pencil-CO4Nsidg.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/play-B5J9tzVR.js` | Created | Regenerated production portal chunk |
| `web/jobs/assets/resume-diff-BK6QgW5j.js` | Created | Regenerated production portal chunk |

## Build & Test

The frozen Jobs aggregate reported the following green matrix:

```text
Automation tests                             644 tests / 35 files passed
Browser tests                                219 tests / 34 files passed
Runner tests                                 293 tests / 32 files passed
Workflows tests                              257 tests / 12 files passed
Portal tests                                 271 tests / 20 files passed
Aggregate                                  1,684 tests / 133 files passed
All five Jobs workspace typechecks           passed
All five Jobs workspace builds               passed
```

The final Rust all-target gate passed `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets`: 1,263 library tests,
101 `integration_e2e` tests, one main test, one `connectinfo` test, one migration test, two GDPR
tests, two runner-plan tests, and one usage-schema test. Runner/workflows full tests, typechecks,
builds, and scoped diff checks are green. The independent Temporal command-path review accepted;
the independent runner review accepted with no blockers or minor findings, including a focused
91-test/four-file recovery matrix. The consolidated Phase 609 local-source verdict is accept.
Schema parity, privacy scans, generated portal byte comparison, and diff hygiene were separately
reported green. This evidence does not authorize deployment or flag enablement.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Workflow cleanup is physically unregistered | Safe account erasure needs authenticated legacy Temporal inventory, complete dispatch, and account-deletion integration; an environment flag cannot expose the unfinished scaffold. |
| Database cleanup cannot reach complete | Rejecting caller-provided legacy-zero evidence is safer than manufacturing deletion authority from local state. |
| Account deletion remains unchanged | Wiring incomplete cleanup into hard deletion would create a false erasure claim. |
| No genuine protocol-v1 history fixture | A synthetic fixture would not prove compatibility or a real production drain. |

## Known Follow-ups

- Phase 610: implement a signed/authenticated, paginated Temporal-v1 inventory and drain receipt.
- Complete and independently review database-to-cleanup-gateway dispatch and all lease/fence proof
  epochs against a real Temporal namespace.
- Wire exact cleanup generation, target digest, and legacy-zero evidence into account deletion,
  object-sweep recheck, and same-transaction hard-delete authorization.
- Run live PostgreSQL multi-replica and hosted network-ambiguity fault tests.
- Commit the accepted source and open its draft PR without enabling any parked release flag.
- Deploy gateway v2 before any command-dispatch canary; keep cloud Browser distribution parked
  until its independent ATS, capacity, monitoring, rollback, and tenant gates pass.

## Review Checklist (for reviewer)

- [x] Files match the final frozen scope and inventory
- [x] No unrelated changes are included
- [x] Start/resume request handlers have no direct gateway fallback
- [x] Command replay, ambiguity, exact identity, and lease fences fail closed
- [x] Temporal v2 history surfaces remain opaque
- [x] Two-phase intervention and terminal submitted/ambiguity evidence are exact
- [x] Cleanup and account deletion are not overclaimed or accidentally enabled
- [x] Full test, format, strict Clippy, schema, privacy, generated-output, and diff gates pass
- [x] No TODOs lack a tracked follow-up
