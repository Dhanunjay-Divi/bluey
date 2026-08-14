# Round 610 — Jobs Workflow Cleanup Authority

> **Codex preflight:** Load `$bluey-ops` before implementation or review. Use the local handoff and
> current repository first; the SSD archive is only a fallback for a specifically missing historical
> fact.

**Status:** LOCAL SOURCE ACCEPTED; ALL RELEASE FLAGS PARKED; NO DEPLOYMENT AUTHORITY

## Goal

Make Temporal workflow cleanup a durable, exact prerequisite of Jobs account deletion without
claiming that local source can prove hosted deletion, retention, visibility convergence, or physical
erasure.

Round 610 must replace every caller-owned legacy-zero or process-memory cleanup decision with:

1. a namespace-, cutoff-, query-, page-chain-, and two-pass-bound global protocol-v1 inventory;
2. exact per-run absence evidence for every discovered protocol-v1 execution;
3. exact account-scoped protocol-v2 cleanup bound to the frozen command and deletion generation;
4. one database-owned completion authority and pseudonymous tombstone; and
5. an account-deletion transaction that performs no object sweep or hard delete until that exact
   workflow authority and the existing runner-purge authority are both current.

## Product Boundary

- The browser-delivered portal and managed cloud path remain the product direction.
- Installed Bluey Browser work remains parked.
- This round does not enable workflow dispatch, workflow cleanup, or customer cloud distribution.
- This round does not deploy, contact production, enumerate a hosted Temporal namespace, or delete a
  real execution.
- The production protocol-v1 namespace, cutoff, zero inventory, retention, and visibility evidence
  are external gates. Source code must not guess them.

The relevant checked-in release defaults remain:

```text
BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED=0
BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
```

Round 610 changes no deployment, hosted provider, tenant, credential, canary, or customer state.

## Phase 609 Baseline Failure Mode

At the frozen Round 609 boundary, cleanup deliberately remained fail-closed:

- `/workflow-cleanup` was not registered by the production gateway;
- cleanup progress in the retained TypeScript scaffold was not durable authority;
- the paired command migrations constrained `legacy_reconciled` to false, so a cleanup generation
  could not complete;
- no Rust cleanup dispatcher existed;
- account deletion did not freeze or recheck workflow cleanup before object deletion; and
- the source had no canonical account-scoped search attribute for retained protocol-v1 workflows.

An account-specific legacy query would therefore be invented authority. Round 610 uses a global,
source-known protocol-v1 inventory/cutoff authority instead. An account generation may bind that
global zero receipt only after every page and discovered target is durably reconciled.

## Closed Protocol

The authenticated private gateway accepts one exact schema-version-3 union at
`POST /workflow-cleanup` only while cleanup is explicitly enabled:

- `legacy_inventory_page` — one bounded page of the fixed `applicationWorkflow` visibility query,
  bound to namespace, cutoff, query digest, scan pass, page index, predecessor digest, page token,
  request ID, and cleanup fence;
- `reconcile_legacy_target` — exact protocol-v1 run identity and inventory authority; a running
  execution is reported pending and is never force-terminated by this round; and
- `reconcile_v2_target` — exact `applicationWorkflowV2` type plus
  `bluey_jobs_command_v2` memo/request/payload authority before any termination or deletion.

The gateway is stateless. It may return provider observations, but only the database can advance a
page chain, start the second zero pass, satisfy the confirmation age, or complete an account
generation.

The fixed visibility query is exactly the workflow-type predicate. The cutoff is separately bound
cutover authority and is deliberately not a `StartTime` filter: late-visible starts and
continue-as-new runs must still reopen inventory instead of falling outside the scan.

An absence observation requires all of the following for every exact run:

1. `DescribeWorkflowExecution` returns exact NotFound;
2. raw History retrieval returns exact NotFound (an empty successful history is not absence); and
3. an exhaustive exact visibility query returns zero results.

Every RPC, page, token, target set, body, and complete request has a fixed cap and deadline. Token
loops, malformed identity, type/query/namespace drift, unknown status, cap exhaustion, and provider
errors remain pending or identity conflict; none imply absence.

## Durable Authority

Paired additive SQLite/PostgreSQL migrations must persist:

- global legacy inventory generations and exact page chains;
- encrypted legacy page tokens and workflow/run identities with HMAC-only lookup columns;
- encrypted companion rows for the protocol-v2 known-run set, while retained Phase-609 target and
  observation columns continue binding bounded opaque, non-account-derived protocol-v2 authority;
- immutable discovered legacy targets;
- lease/fence/request-bound observations and two-pass zero confirmations;
- account-deletion-to-workflow-cleanup bindings;
- combined global-legacy and account-v2 completion authority; and
- a pseudonymous cleanup tombstone safe to retain after account deletion.

Reversible protocol-v1 page tokens and workflow/run identities exist only while a generation needs
them. Before the global inventory can become complete, the database nulls those ciphertexts and
marks the page/target rows scrubbed. HMACs, identity digests, page digests, proof epochs, and the
completion digest remain so replay and deletion authority can be checked without retaining the raw
legacy identifiers. If a later scan rediscovers the same pseudonymous target, the database
repopulates encrypted raw identity only into the non-complete current generation, increments the
proof epoch, and requires new evidence.

The Phase 609 tables remain compatibility scaffolding and fail closed. No migration rewrites their
historical meaning or permits a caller to set `legacy_reconciled=true`.

## Account Deletion Order

After the existing irreversible submission and communication checks, one transaction must freeze:

- the account deletion intent;
- the workflow cleanup generation and target digest;
- every request-started protocol-v2 target, including closed/no-run/ambiguous executions; and
- the exact current global protocol-v1 inventory generation required for deletion.

For a new account binding, the first confirmed delete never consumes a previously complete global
inventory as fresh deletion proof. Under the same database transaction, SQLite/PostgreSQL obtain
their own current time, advance that completed global proof to a new completion epoch, reset it to
scan pass one, freeze the exact account protocol-v2 target set, and commit the account deletion
fence. The API therefore returns HTTP 202 `pending_workflow_cleanup` (or
`pending_workflow_cleanup_configuration` when no global authority exists) with zero object deletes.
An exact retry reuses the existing binding rather than advancing another epoch.

Until cleanup is complete, the API returns HTTP 202 with `pending_workflow_cleanup`, retains the
credentials needed to finish cleanup, and performs zero object deletion.

Immediately before the first object delete, the API rechecks the same generation, target digest,
global zero authority, cleanup tombstone, and existing runner-purge state. Pre-sweep drift returns
pending and deletes nothing. Before external I/O, one database transaction must persist a stable
authorization attempt bound to that proof, the exact sorted storage-namespace set, and the frozen
known-object and prefix-sweep manifests. Every per-key and per-scope receipt references that attempt,
so a delete-success/receipt-write-loss cannot later be presented as a pre-sweep state. Once that
exact check authorizes the existing idempotent object sweep, partial object erasure may be durably
resumed because the user already accepted data loss. The hard-delete transaction checks cleanup
again under the existing account/fleet lock order. Drift found after a sweep began retains the
fenced account and object-deletion progress, returns
`pending_workflow_cleanup_revalidation`, and never claims completion; deleted objects are not
invented back into existence.

The final success response is deliberately bounded: it says the account and Bluey records covered
by the verified deletion were removed from configured active storage. It does not certify provider
retention, backups, archival, payload-codec/KMS destruction, or physical erasure.

## Frozen Local-Source Result

The frozen batch contains 27 paths after the required changelog and operations updates. The exact
status and purpose inventory is in
`docs/work/IMPL-PHASE-610-JOBS-WORKFLOW-CLEANUP-AUTHORITY.md`.

Verified on the accepted local source:

```text
Focused workflows tests      267 tests / 12 files passed
Jobs aggregate tests       1,694 tests / 133 files passed
All five Jobs typechecks     passed
All five Jobs builds         passed
Generated portal             byte-identical; no status changes
Focused Rust gates           cleanup 10; dispatcher 25; account 16; command locks 6;
                             account-delete HTTP 10; delete-account HTTP 4
Rust formatting/Clippy/build passed
Rust all-target tests        library 1,301; integration_e2e 105; every other target passed
SQLite/PostgreSQL            fresh install, exact replay, integrity, parity, adversarial proof pass
Independent review           workflows/server/database accepted; no blockers or minors
Privacy/diff/inventory       exact 27 paths; flags 0; no docs/reviews edit; checks passed
```

This is local source acceptance only. Production namespace/cutoff/inventory evidence, hosted
Temporal behavior, a genuine retained protocol-v1 history fixture, live multi-replica rehearsal,
deployment, canary, and flag enablement remain external gates.

## Acceptance Criteria

1. Cleanup is default-off; when disabled, authenticated `/workflow-cleanup` is 404, the existing
   generic unauthenticated boundary remains 401, and no cleanup worker starts.
2. Enabling configuration is validated before startup, but tests never enable customer distribution.
3. Inventory uses only the fixed protocol-v1 workflow-type query, with namespace and cutoff bound
   separately; callers cannot supply arbitrary visibility queries or a zero count.
4. Every page is immutable, predecessor-bound, bounded, replay-safe, and exact-conflict rejecting.
5. Two exhausted zero scans use one database-owned confirmation interval; gateway or client time
   cannot satisfy it.
6. A late visible protocol-v1 run appends to the target set and returns inventory to drain.
7. Running protocol-v1 executions remain pending and are never force-terminated by this batch.
8. Protocol-v2 mutation requires exact type, memo, request ID, payload digest, first-run identity,
   target digest, cleanup generation, lease, and fence.
9. Only exact Describe + History + visibility NotFound can produce one absence observation.
10. Lease loss, expiry, retry, response loss, gateway restart, or replica handoff cannot aggregate
    evidence across proof epochs or advance stale authority.
11. Raw legacy workflow/run IDs and page tokens are encrypted at rest because historical workflow
    IDs can embed account-scoped values. Protocol-v2 known-run companion rows are encrypted, while
    retained Phase-609 target/observation columns bind only bounded opaque IDs that never embed
    account data and cascade with the account. Logs, public responses, durable receipts, and
    tombstones contain no account ID, packet, answer, URL, token, or raw provider error.
12. Account deletion and cleanup freeze atomically; a concurrent command/materialization either
    precedes the frozen target set or fails the deletion fence.
13. `pending_workflow_cleanup` before sweep authorization performs zero object sweep and zero hard
    delete. Sweep authorization is durable before I/O and binds every storage scope/object manifest;
    post-sweep authority drift retains the account and returns an explicit revalidation state.
14. Hard delete requires the exact workflow cleanup tombstone and existing runner purge tombstone in
    the same final transaction.
15. SQLite/PostgreSQL migrations and lock/state behavior have parity and are replay-safe on upgrade.
16. Full Jobs and Rust formatting, strict lint, build, test, privacy, schema, generated-output, and
    independent review gates pass before a draft PR.

## External-Only Evidence

Local acceptance cannot provide:

- the owner-approved production namespace, protocol-v1 cutoff, or full inventory count/digest;
- a genuine retained protocol-v1 replay/history fixture;
- hosted Temporal Describe/History/Delete/visibility permissions and eventual-consistency behavior;
- namespace retention, archival, payload codec, KMS, or crypto-shredding proof;
- live PostgreSQL multi-replica/network fault evidence;
- a real account deletion that remains pending before cleanup and completes only afterward; or
- deploy, canary, flag enablement, or customer distribution authority.

Those remain mandatory release evidence. API-level absence must not be described as physical or KMS
erasure without provider proof.
