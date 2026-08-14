# REVIEW: PHASE-610 — Jobs Workflow Cleanup Authority

> **Codex preflight:** Load `$bluey-ops` before review and verify its memory against the current
> repository state and frozen source snapshot.

**Reviewed snapshot:** Final 27-path Phase 610 working tree after the complete Jobs, Rust, schema,
privacy, generated-output, and diff gates.

**Reviewer:** Independent workflows/Temporal, server, and database reviewers; consolidated by the
primary Phase 610 agent

**Date:** 2026-08-14

**Review status:** 🟢 ACCEPT — no blockers or minors in the local Phase 610 source batch

## Per-Task Review

### Global protocol-v1 inventory and privacy

| Field | Value |
|-------|-------|
| Files | Paired migrations and `server/src/db/jobs/workflow_cleanup.rs` |
| Verdict | 🟢 ACCEPT — no blockers or minors |

**Verified:**

- Exact fixed namespace/type/cutoff/query authority.
- Complete immutable page chain and repeated-token/cap failure behavior.
- No protocol-v1 payload/history decode or private log/durable receipt.
- Two database-aged exhausted zero scans and late-target reopening.
- First account binding unconditionally advances a complete inventory into one new database-time
  proof epoch; an exact binding replay does not reopen it again.
- Completion compacts raw legacy identity/token ciphertext, while later pseudonymous rediscovery
  safely repopulates encrypted identity only into a current non-complete generation.

### Protocol-v2 cleanup and dispatcher

| Field | Value |
|-------|-------|
| Files | Workflow contracts/gateway, Rust dispatcher, workflow-command lock integration |
| Verdict | 🟢 ACCEPT — no blockers or minors |

**Verified:**

- Exact type/memo/request/payload/first-run/target/generation/lease/fence binding.
- Request-start before I/O, stable retry identity, no cross-fence evidence aggregation.
- Describe/History/visibility exact NotFound and response-loss/restart recovery.
- Disabled route/worker and strict configuration behavior.

### Account deletion

| Field | Value |
|-------|-------|
| Files | Account API/database integration, runner-purge gate, HTTP integration tests |
| Verdict | 🟢 ACCEPT — no blockers or minors |

**Verified:**

- Atomic deletion-intent and cleanup-generation freeze after irreversible fences.
- Zero object deletion while pre-sweep cleanup is pending.
- One atomic pre-I/O authorization binds current workflow proof, runner authority, every storage
  scope, exact sorted object manifest, and required prefix sweep.
- Durable, idempotent per-key/per-scope progress plus hard-delete rejection after post-sweep drift.
- Exact workflow-cleanup and runner-purge tombstones in the final transaction.
- Transaction-scoped cascade token exists only across the parent account delete and child cascades.

### Paired schema and release boundary

| Field | Value |
|-------|-------|
| Files | Paired migrations, migration registry/tests, env, operations, and round/work docs |
| Verdict | 🟢 ACCEPT — no blockers or minors |

**Verified:**

- SQLite/PostgreSQL object, constraint, trigger, lock, and replay parity.
- No cleanup/account-deletion claim beyond the local source boundary.
- All release flags remain `0`; no deployment or hosted mutation.
- Exact 27-path status/purpose inventory and no `docs/reviews/` edits.

## Cross-Task Findings

- The first account binding must use database time to reopen a complete legacy inventory; the API
  process timestamp is only an opaque account-generation key.
- Raw legacy workflow/run IDs and page tokens must be compacted before a global proof can authorize
  account deletion, while pseudonymous replay proof remains.
- Pre-I/O authorization must bind both runner and workflow authority plus every exact storage
  manifest. A later drift may retain partial deletion progress but must never authorize hard delete.
- Installed Bluey Browser work remains parked; this source batch is for the browser-delivered portal
  and managed-cloud direction and does not launch either Browser distribution mode.

## Build & Test Verification

```text
PASS  npm --workspace @bluey/jobs-workflows test
      267 tests / 12 files
PASS  npm --workspace @bluey/jobs-workflows run typecheck
PASS  npm --workspace @bluey/jobs-workflows run build
PASS  (cd jobs && npm test)
      1,694 tests / 133 files
      automation 644/35; Browser 219/34; runner 293/32;
      workflows 267/12; portal 271/20
PASS  (cd jobs && npm run typecheck)
      all five workspaces
PASS  (cd jobs && npm run build)
      all five workspaces; generated portal byte-identical with no status changes
PASS  temporary jobs/node_modules dependency symlink removed and absent
PASS  docs-only 27-path inventory, whitespace, and stale-current-claim checks
PASS  cargo fmt --all -- --check
PASS  CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
PASS  CARGO_INCREMENTAL=0 cargo build --all-targets
PASS  CARGO_INCREMENTAL=0 cargo test --all-targets
      library 1,301; main 1; connect-info 1; context migration 1; GDPR 2;
      integration_e2e 105; runner-plan 2; usage schema 1; Jobs API binary 0
PASS  focused cleanup authority 10; dispatcher 25; account authority 16;
      workflow-command lock/order 6; account-delete HTTP 10; delete-account HTTP 4
PASS  PostgreSQL 17.10 and SQLite full-chain fresh install plus exact migration replay
PASS  paired live parity: 294 table columns, 15 view columns, 85 foreign keys,
      76 shared triggers, zero PostgreSQL-only triggers, and two expected split SQLite triggers
PASS  privacy/secret/line-length/conflict-marker/default-off flag scans
PASS  tracked and untracked full-tree diff checks; exact 27-path inventory; no docs/reviews edits
PASS  independent workflows, server, and database verdicts: no blockers or minors
```

Final database evidence hashes:

```text
SQLite 054             a0aacff89716bce9153b7fb60f474cb0fe3ff38f7173cfeeb785f017238dc611
PostgreSQL 032         715240d6cfba791cdd5463e1db5fb99e9b61c83de4bb855378c582cce2ce9b1d
workflow_cleanup.rs    d0b7fa227a9a463c8bbe545bdf5abea7230f91eb504a9b4ce718a0a5b199a6b7
jobs.rs                2881b52e1bea6c01b76400f7e0fc5b63aaef9d669dd1d2e0ea9df5dd7b18b1e2
db/mod.rs              3c8f1d9b3d71d6a080ec64ca73568cf7a267040e7ba128688de7d5bd329cbf84
```

## Overall Verdict

🟢 **ACCEPT** — The local Phase 610 source batch has no blockers or minors and is ready for its
scoped commits and stacked draft PR. This verdict does not authorize deployment, flag enablement,
hosted cleanup, production inventory, or customer distribution.

## Follow-ups for Next Batch

- Owner-approved production namespace/cutoff and complete protocol-v1 inventory/drain evidence.
- Hosted Temporal deletion, visibility convergence, retention, archival, codec, and KMS behavior.
- A genuine retained protocol-v1 history replay fixture.
- Live PostgreSQL multi-replica/network-fault rehearsal and an authorized end-to-end deletion canary.
- Explicit release approval before enabling cleanup, workflow dispatch, or either Browser
  distribution mode.
