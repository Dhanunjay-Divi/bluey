# IMPL: PHASE-610 — Jobs Workflow Cleanup Authority

> **Codex preflight:** Load `$bluey-ops` before implementation and verify its memory against the
> current repository state. Use the SSD archive only for a specifically missing historical fact.

## Scope

**Does:**

- add an authenticated, exact, stateless Temporal cleanup protocol behind a default-off route;
- persist global protocol-v1 inventory, immutable pages/targets, two-pass zero authority, and exact
  account protocol-v2 cleanup in paired SQLite/PostgreSQL schema;
- encrypt legacy identities/tokens and the protocol-v2 known-run companion set while retaining the
  bounded opaque Phase-609 protocol-v2 target/observation authority needed for exact joins;
- dispatch cleanup through durable request-start, lease, token, and fence authority;
- force a new database-time global inventory epoch on the first account-deletion binding;
- compact reversible legacy workflow/run identity and page-token ciphertext before completion;
- atomically bind current runner/workflow proof and exact storage manifests before object I/O;
- retain durable partial sweep progress and require exact revalidation before account hard delete;
  and
- retain only a pseudonymous workflow-cleanup completion tombstone after account deletion.

**Does NOT:**

- discover or approve the production Temporal namespace/cutoff;
- run a real inventory, terminate/delete a hosted workflow, deploy, canary, or enable a flag;
- claim physical/KMS erasure from API-level NotFound;
- fabricate a retained protocol-v1 history fixture; or
- resume installed Bluey Browser work.

## Files Created / Modified

Frozen working-tree inventory after this documentation pass: **27 paths**.
`M` means modified; `??` means created and currently untracked. Nothing is staged.

| Status | Path | Purpose |
|--------|------|---------|
| M | `CHANGELOG.md` | Record the bounded source result, parked flags, and no-deployment boundary |
| ?? | `docs/rounds/ROUND-610-JOBS-WORKFLOW-CLEANUP-AUTHORITY.md` | Define protocol, deletion order, acceptance, and external-only evidence |
| ?? | `docs/work/FIX-689-workflow-cleanup-erasure-gap.md` | Record the original erasure-authority gap and its bounded fix |
| ?? | `docs/work/IMPL-PHASE-610-JOBS-WORKFLOW-CLEANUP-AUTHORITY.md` | Freeze implementation inventory, verification, and follow-ups |
| ?? | `docs/work/REVIEW-PHASE-610-JOBS-WORKFLOW-CLEANUP-AUTHORITY.md` | Record independent review slices and the accepted local-source verdict |
| ?? | `infra/postgres/server-runtime/032_jobs_workflow_cleanup_authority.sql` | Add PostgreSQL global-v1, account-v2, sweep, tombstone, privacy, and cascade authority |
| ?? | `infra/sqlite/server-runtime/054_jobs_workflow_cleanup_authority.sql` | Add the SQLite parity authority and guards |
| M | `jobs/OPERATIONS.md` | Replace Phase 609 scaffold language with the disabled Phase 610 rollout runbook |
| M | `jobs/workflows/src/contracts.ts` | Close the exact schema-v3 cleanup request/receipt union |
| M | `jobs/workflows/src/gateway-cleanup-service.ts` | Implement the stateless bounded v1 inventory and exact v1/v2 reconciliation service |
| M | `jobs/workflows/src/gateway.ts` | Register cleanup only for exact `true` and enforce closed HTTP/auth/body behavior |
| M | `jobs/workflows/tests/gateway-cleanup-service.test.ts` | Cover paging, identity, mutation, absence, budget, and privacy boundaries |
| M | `jobs/workflows/tests/gateway-http.test.ts` | Cover disabled/enabled route, exact auth, headers, method, content type, and body limits |
| M | `ops/bluey-jobs.env.example` | Keep cleanup `0` and reserve explicit approved-only configuration |
| M | `server/src/api/account.rs` | Fence deletion, return pending states, authorize/resume sweeps, and bound success wording |
| M | `server/src/bin/bluey-jobs-api.rs` | Validate/start the optional cleanup dispatcher before background workers |
| M | `server/src/db/account_data.rs` | Atomically bind deletion cleanup and require runner/workflow/sweep proof at hard delete |
| M | `server/src/db/jobs.rs` | Export the workflow-cleanup database authority module |
| M | `server/src/db/jobs/runner_volume_purge.rs` | Prove runner-purge-only authority cannot bypass the new workflow/sweep gate |
| M | `server/src/db/jobs/tests.rs` | Preserve strict production cascade guards during test teardown |
| M | `server/src/db/jobs/workflow_commands.rs` | Serialize PostgreSQL command locks with account/deletion cleanup authority |
| ?? | `server/src/db/jobs/workflow_cleanup.rs` | Implement DB-owned inventory, leases, proof epochs, compaction, bindings, sweeps, and tombstones |
| M | `server/src/db/mod.rs` | Register paired migrations and add replay/parity/schema guard tests |
| ?? | `server/src/jobs_workflow_cleanup.rs` | Implement the default-off durable Rust cleanup dispatcher and closed receipt validation |
| M | `server/src/lib.rs` | Export the cleanup dispatcher module |
| M | `server/src/main.rs` | Validate/start the optional dispatcher before shared-server background workers |
| M | `server/tests/integration_e2e.rs` | Cover fence-first deletion, configuration, revalidation, partial sweep, cascade, and privacy behavior |

## Build & Test

Verified on the frozen workflow source:

```text
npm --workspace @bluey/jobs-workflows test
  PASS: 267 tests / 12 files

npm --workspace @bluey/jobs-workflows run typecheck
  PASS

npm --workspace @bluey/jobs-workflows run build
  PASS

(cd jobs && npm test)
  PASS: 1,694 tests / 133 files
  Automation: 644 / 35 files
  Browser:    219 / 34 files
  Runner:     293 / 32 files
  Workflows:  267 / 12 files
  Portal:     271 / 20 files

(cd jobs && npm run typecheck)
  PASS: all five workspaces

(cd jobs && npm run build)
  PASS: all five workspaces
  Generated web/jobs output was byte-identical; no status path changed.

Temporary jobs/node_modules dependency symlink
  removed and absent after the gate

Focused Rust authority gates
  PASS: cleanup authority 10; dispatcher 25; account authority 16;
        workflow-command lock/order 6; account-delete HTTP 10; delete-account HTTP 4

cargo fmt --all -- --check
  PASS

CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
  PASS

CARGO_INCREMENTAL=0 cargo build --all-targets
  PASS

CARGO_INCREMENTAL=0 cargo test --all-targets
  PASS: library 1,301; main 1; connect-info 1; context migration 1; GDPR 2;
        integration_e2e 105; runner-plan 2; usage schema 1; Jobs API binary 0

Paired migration and independent database gates
  PASS: PostgreSQL 17.10 and SQLite fresh install plus exact replay, FK/integrity,
        294 table columns, 15 view columns, 85 foreign keys, 76 shared triggers,
        and direct adversarial proof-authority execution

Privacy / secret / flags / line length / generated output / full-tree diff
  PASS: only the synthetic test bearer matched; all four release flags remain 0;
        exact 27 table path/status entries match git status; no docs/reviews edit

Independent review
  PASS: workflows, server, and database slices accepted with no blockers or minors
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Object sweep is idempotent rather than transactionally reversible | External object deletion cannot be rolled back. Pre-sweep drift deletes nothing; post-sweep drift retains the fenced account and blocks hard delete. |
| First deletion reopens a completed global inventory unconditionally | A process timestamp cannot establish post-fence freshness. One new DB-time epoch is required for the first binding; exact binding replay does not reopen again. |
| Raw protocol-v1 ciphertext is compacted before global completion | Historical workflow IDs may embed account material. Pseudonymous proofs are sufficient after absence is current; rediscovery repopulates encrypted identity only into a reopened generation. |

## Known Follow-ups

- Hosted Temporal namespace/cutoff/inventory/drain and aged visibility evidence.
- Genuine retained protocol-v1 replay/history fixture.
- Live PostgreSQL multi-replica and network-fault rehearsal.
- Provider retention, archival, payload-codec, KMS, and physical-erasure evidence.
- Production canary and explicit owner-authorized flag enablement.

## Review Checklist (for reviewer)

- [x] Exact 27-path file inventory replaces implementation-time grouping
- [x] No unrelated changes or `docs/reviews/` edits
- [x] Tests cover every locally provable Round 610 acceptance criterion
- [x] No caller-owned zero, process-memory authority, raw provider error, or private durable receipt
- [x] SQLite/PostgreSQL parity and lock order are independently reviewed
- [x] Account deletion performs zero pre-authority object deletion and no stale-authority hard delete
- [x] Cleanup/dispatch/distribution flags remain `0`
- [x] Full Jobs/Rust/privacy/generated-output gates pass
