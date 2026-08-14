# Round 609 - Jobs Workflow Command Authority

**Date:** 2026-08-13

**Branch:** `feat/phase-609-jobs-workflow-command-outbox`

**Status:** LOCAL SOURCE ACCEPTED; ALL RELEASE FLAGS PARKED; NO DEPLOYMENT AUTHORITY

Cloud Browser distribution, workflow-command dispatch, and workflow cleanup remain independently
disabled. No deployment, credential, provider, tenant, or production authority is claimed.

## Objective

Make managed Jobs workflow start and intervention resume durable across process death, transport
ambiguity, and replay. The server must commit customer intent before external delivery, and every
retry must carry one immutable opaque identity to an exact Temporal protocol-v2 workflow.

The source-complete scope is deliberately narrower than external workflow-history erasure and
account deletion. Round 609 retains an unimported cleanup-service library and fail-closed database
scaffolding, but the gateway physically registers no cleanup route and environment configuration
cannot enable one. It does not claim an authenticated legacy Temporal inventory, a complete
database-to-cleanup dispatcher, account-deletion integration, hosted erasure evidence, or a safe
cleanup enablement.

## Failure Boundary Closed by This Batch

Phase 608 correctly made the browser-delivered portal and managed Background runner the product
surface, but the retained start and resume paths crossed PostgreSQL/SQLite and Temporal directly:

1. queue admission could commit local attempt, metering, and application state before workflow
   start, then lose the gateway response; and
2. intervention approval could commit before a non-idempotent signal, then be reopened after a
   timeout even when Temporal may have accepted it.

A generic conflict did not prove existing-workflow identity, a delayed signal was not bound to the
exact intervention it intended to resolve, and private application material could reach Temporal
history through workflow/activity/failure payloads.

## Source-Complete Core

### Database-first command authority

Start and resume admission now write an encrypted immutable command in the same local transaction
as the corresponding business transition. Request handlers return durable queued authority; they
do not call the workflow gateway and have no direct-delivery fallback.

The disabled-by-default dispatcher is the sole server-to-gateway command-delivery boundary. It
leases a command, records request-start evidence before I/O, uses a bounded no-redirect client,
and reuses the frozen request ID, workflow ID, intervention ID when present, and authenticated
payload digest after ambiguity. Delivery is at least once; database fences and exact Temporal
identity make duplicate effects replay-safe. This batch does not claim exactly-once transport.

```text
pending -> claimed -> delivering -> accepted
              |            |       identity_conflict
              |            |       rejected
              |            +-----> delivery_unknown -> claimed
              +------------------> pending (lease expired before request start)

pending -> cancelled (only while durable state proves request-start never occurred)
```

`delivering` and `delivery_unknown` never authorize compensation. Retry exhaustion does not prove
non-delivery, and a changed idempotency meaning cannot mint a replacement command.

### Exact Temporal protocol v2

The gateway accepts only the closed `POST /workflow-commands` version-2 envelope and returns a
closed receipt that echoes the exact command identity plus the actual first Temporal execution run
ID.

- Start uses an opaque workflow ID, conflict failure, and exact describe-on-conflict identity
  comparison. A generic HTTP conflict is not success.
- Resume uses a Temporal Update whose Update ID is the durable request ID and whose payload binds
  the exact open intervention. Identical replay returns the original outcome; changed meaning or
  delayed intervention identity fails closed. If the execution closes after a running Describe,
  the gateway recovers only that exact Update receipt before accepting it; an exact absent handle
  proves closure, while an RPC timeout with no visible handle remains `delivery_unknown`.
- Workflow arguments, memo, Update payloads, activity arguments/results, workflow results, and
  converted failures carry opaque authority rather than customer packets, resumes, answers, OTPs,
  URLs, employer fields, direct tenant identifiers, or raw external errors.
- Protocol-v1 direct start/signal compatibility is not a delivery fallback. A real legacy drain is
  a production rollout prerequisite.

### Two-phase interventions and terminal evidence

Interventions use a server-owned prepare/publish sequence. Preparation reserves the next bounded
intervention without exposing it to the customer. Publication atomically makes that exact
intervention current and moves the application/session to `needs_input`. An answer resumes only
the exact published intervention.

Workflow closure is also server-authoritative:

- runner failure finalizes as `failed` / `runner_failed` with no open intervention;
- uncertain runner side effect finalizes as `side_effect_unknown` / `runner_ambiguous` with no open
  intervention;
- intervention timeout finalizes only the exact current published intervention as `failed` /
  `intervention_timeout`; and
- the bounded intervention limit finalizes as `failed` / `intervention_limit` only after the hidden
  over-limit preparation is discarded and no public intervention is open.

A trusted submitted receipt remains the canonical submitted transition. Receipt persistence and
workflow-execution terminal marking are replay-safe, including response-loss ordering; an
ambiguous employer Submit is never retried as a new application.

Runner result authority is request-bound end to end. A protocol-v2 result must echo the exact
request ID, account, application, application identity, browser session, and run before it can
affect the workflow. Possible side effects are persisted first as minimal encrypted, purgeable
ambiguity tombstones, and restart recovery is result-aware: exact submitted evidence completes
through the fenced lease replay, exact failed evidence stays terminal, and an unresolved
`final_submit_started` marker never reopens Submit.

Internal Jobs API errors can stop workflow retries only when their status, closed body, and private
JSON headers match exactly. Browser cleanup occurs only after canonical submitted or terminal state
commits; 401/403 credential drift remains retryable, while a bounded permanent cleanup rejection
cannot downgrade the committed application outcome.

### Independent review hardening

The source-freeze review found and closed the following fault-model gaps before the final verdict:

- running-Describe to closed-execution resume recovery now reads the exact Update ID;
- Update RPC timeout remains distinct from proven closed-workflow absence;
- only exact closed Jobs API errors can become non-retryable workflow failures;
- runner result wire receipts bind the exact request ID and complete run identity;
- ambiguity recovery uses a minimal encrypted, purgeable tombstone and must persist before reply;
- restart decisions reconcile durable submitted, failed, and ambiguous results before replay;
- an exact committed intervention remains recoverable after ordinary checkpoint expiry only
  by trying current server-authority restore first; only an expired claim rejected as exact
  `lease_unavailable` falls back to old token/fence reconciliation and removal, while every other
  restore or reconciliation error fails closed;
- startup recovery derives and proves the account/application-identity profile scope before any
  result or ambiguity-tombstone persistence;
- runner cleanup 401/403 responses retry as repairable credential drift; and
- the dispatcher and gateway enforce the same trimmed 32-to-8,192-byte safe bearer-token grammar.

## Cleanup and Account-Deletion Boundary

The source tree retains an isolated cleanup-service library and direct library tests, but the
gateway does not import it or register `POST /workflow-cleanup`. The environment remains parked at:

```text
BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED=0
```

Changing that environment cannot create a route in the frozen gateway. The scaffold is not part of
the source-complete claim and remains excluded for Phase 610. The database cleanup generation is
intentionally unable to report complete because no authenticated, paginated inventory or drain
proof exists for pre-v2 Temporal workflows. Caller-supplied `legacy_reconciled=true` is rejected,
and a positive unresolved legacy count is required. This prevents a synthetic local assertion from
becoming deletion authority.

Account deletion is not wired to the Round 609 cleanup generation. It therefore does not wait on,
re-read, or transact against the new Temporal cleanup evidence before object deletion or hard
deletion. Until a later reviewed batch implements the authenticated legacy inventory, durable
cleanup dispatcher, and exact account-deletion dependency, do not treat cleanup tables, gateway
fixtures, deletion RPC success, or local cascade behavior as erasure proof.

No claim is made here that cleanup observations survive every lease/fence race, cover every
execution chain, or satisfy hosted Temporal retention and visibility behavior. Those remain outside
this round's accepted scope.

## Release Flags and Rollout Truth

Keep all three independent gates disabled:

```text
BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED=0
BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

Gateway v2 and worker v2 must deploy before command-dispatch canary authority. A real legacy-v1
inventory/drain and the complete cleanup/account-deletion integration must be reviewed before the
Phase 610 gateway may register cleanup or its parked flag can change. Neither internal flag is
permission to enable customer cloud execution; the existing cloud Browser, ATS, capacity,
monitoring, and rollback gates remain independent.

The customer launch surface remains the web portal at `/jobs/automation`. A separate installed
Bluey Browser remains parked P2 work and is not required for this cloud-first path.

## Core Acceptance Criteria

1. Paired SQLite and PostgreSQL migrations create replay-safe workflow-command, attempt,
   execution, finalization, intervention, and cleanup scaffolding with dialect parity.
2. Start admission atomically revalidates current queue/business authority and commits the
   attempt, metering/binding transitions, and encrypted immutable command.
3. Resume admission locks the exact current intervention and active cloud execution, then commits
   one intervention-bound command and local transition.
4. Exact replay returns existing authority; a reused key or identity with changed semantics fails
   closed without another charge or workflow.
5. Claim, request-start, completion, retry, and reclaim validate the exact owner, hashed lease
   token, fence, request ID, and authenticated payload digest.
6. Timeout, connection loss, malformed success, unexpected conflict, 5xx, and process death after
   request start preserve ambiguity as `delivery_unknown`; no ambiguity path compensates state.
7. Every ambiguity retry sends the same frozen opaque command.
8. Fresh start returns the actual first execution run ID; already-started acceptance requires exact
   v2 identity; mismatch and closed reuse fail closed.
9. Resume uses an intervention-bound Update; exact duplicate delivery is harmless and delayed
   intervention identity cannot satisfy a replacement intervention. A running-to-closed race
   accepts only an exact recovered Update receipt, and a timeout alone never proves closure.
10. No start/resume request-handler gateway call or protocol-v1 delivery fallback remains.
11. Deterministic history surfaces and failure conversion exclude customer and direct tenant data.
12. Two-phase intervention publication and exact terminal evidence preserve customer-visible and
    application/run state across replay.
13. A trusted submitted receipt also marks workflow execution terminal without downgrading a
    committed employer side effect when later cleanup fails.
14. The command dispatcher is disabled by default, validates configuration only when enabled,
    uses bounded HTTP timeouts with redirects disabled, starts in both server binaries, and shares
    the gateway's safe bearer-token grammar.
15. Runner results, absence, ambiguity, and Jobs API errors require exact private headers, closed
    bodies, request identity, and full execution binding before they can change workflow state.
16. Restart recovery preserves exact submitted/failed results and persisted ambiguity without
    replaying an irreversible provider action; post-commit browser cleanup cannot downgrade truth.
17. Startup recovery recomputes the profile scope from the frozen account and application identity
    before reading or persisting a result/tombstone, and scope mismatch fails closed.
18. An exact committed intervention may outlive ordinary checkpoint expiry only by first trying
    current server-authority restore. Exact `lease_unavailable` on that expired claim alone may
    reconcile/remove with the frozen old token/fence; all other errors preserve fail-closed state.
19. The gateway does not import or register the unfinished cleanup service; changing cleanup
    environment values cannot expose a cleanup route in the Phase 609 build.

The following are explicitly not acceptance claims for Round 609: authenticated legacy workflow
inventory, successful cleanup-generation completion, account-deletion wiring, hosted history
erasure, multi-replica live PostgreSQL proof, Temporal-v1 replay fixtures, cloud distribution, ATS
certification, or production rollout.

## Verification Status

The frozen Jobs aggregate is green:

```text
Jobs automation tests                         644 tests / 35 files passed
Jobs browser tests                            219 tests / 34 files passed
Jobs runner tests                             293 tests / 32 files passed
Jobs workflows tests                          257 tests / 12 files passed
Jobs portal tests                             271 tests / 20 files passed
Jobs aggregate                              1,684 tests / 133 files passed
All five Jobs workspace typechecks             passed
All five Jobs workspace builds                 passed
```

The final Rust all-target gate is green: `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets` passed with 1,263 library
tests, 101 `integration_e2e` tests, one main test, one `connectinfo` test, one migration test, two
GDPR tests, two runner-plan tests, and one usage-schema test. Runner and workflows full tests,
typechecks, builds, and scoped diff checks are also green. The independent Temporal command-path
and runner reviews accepted their scopes with no blockers or minor findings. The runner's focused
recovery matrix passed 91 tests across four files. The consolidated Phase 609 local-source verdict
is accept. Schema parity, privacy scans, generated portal byte comparison, and diff hygiene were
separately reported green. No local gate supplies deployment or production authority.

## External Production Boundary

Local source cannot prove or authorize:

- a real Temporal namespace's version, retention, history deletion, codec/encryption, visibility,
  closed-workflow reuse, or legacy inventory;
- multi-replica live PostgreSQL contention, failover, dispatcher crash recovery, or hosted network
  ambiguity;
- provider-backed KMS destruction, account-scoped workflow-history erasure, or a complete legacy-v1
  drain;
- authorized ATS tenants, managed Chromium capacity, takeover routing, production monitoring, or
  on-call response; or
- deployment, canary, rollback, protocol promotion, or any distribution/cleanup flag change.

The strongest honest outcome of Round 609 is source-complete durable start/resume command authority,
exact Temporal v2 behavior, two-phase intervention binding, and terminal evidence, with cleanup,
account deletion, hosted proof, and customer cloud enablement still parked behind separate gates.
