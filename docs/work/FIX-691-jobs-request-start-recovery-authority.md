# FIX-691: Request-Start Recovery Could Downgrade into a New Effect

> **Codex preflight:** Loaded `$bluey-ops` and verified the Phase 611 command/recovery boundary in
> the current worktree. No hosted Temporal namespace, live tenant, credential, deployment, canary,
> or flag change was used.

## Issue

After a workflow command crossed durable request-start, a retry could depend on the mutable current
release head and ordinary dispatch configuration. Losing those gates could strand ambiguity; using
the normal delivery path to recover it could instead create a second Temporal effect.

## Root Cause

Phase 609 durably recorded that delivery started, but it did not freeze the canonical managed-cloud
release memo used for that first request. The dispatcher had one effect-capable claim path, and the
gateway had no closed recovery-only command. Re-evaluating current release B after response loss
would either reject legitimate lookup/receipt persistence or allow a miss to fall through to
`start`/Update on different authority.

## Fix Summary

- Persist one immutable request-start authority atomically with the first attempt: the exact
  workflow-command binding, canonical release-A memo bytes/digest, and request/attempt identity.
- Reuse those original A bytes for every later attempt. Same-attempt replay is idempotent; a new
  attempt is marked reconciliation-only and never recomputes authority from the current head.
- Split effect claims from ambiguity-recovery claims. New effects require a managed command that
  has never crossed request-start; recovery requires prior durable request-start evidence and can
  run under its own default-off dispatcher flag.
- Add a closed managed schema-v3 `reconcileOnly: true` request and a dedicated historical schema-v2
  reconciliation endpoint. Both perform only exact Describe/Update-handle lookup and never call
  Temporal `start`, `executeUpdate`, or another effect API on a miss or ambiguity.
- Keep reconciliation responses delivery-unknown unless they are the exact matching accepted
  receipt; malformed successes, authentication/configuration failures, misses, conflicts, and
  provider ambiguity are never converted into terminal rejection or new-effect authority.
- Allow recovery and receipt/result persistence to bypass mutable current-head, rollout,
  entitlement, and operational gates, while account deletion and active cleanup retain ownership
  of their separately fenced recovery path.

## Files Modified

| File | Change |
|------|--------|
| `infra/{postgres,sqlite}/server-runtime/*jobs_managed_cloud_release_authority.sql` | Persist immutable canonical request-start release-A authority |
| `server/src/db/jobs/workflow_commands.rs` | Separate fresh-effect preflight/claims from exact post-request-start reconciliation and preserve idempotent replay ordering |
| `server/src/jobs_workflow_dispatch.rs` | Run independent effect/recovery loops, send managed lookup-only schema v3 or historical lookup-only schema v2, and classify all nonexact recovery responses as unknown |
| `jobs/workflows/src/contracts.ts` | Add the exact optional true-only managed reconciliation marker without changing legacy bytes |
| `jobs/workflows/src/gateway-service.ts` | Implement exact lookup-only managed and historical reconciliation behavior |
| `jobs/workflows/src/gateway.ts` | Register the authenticated historical reconciliation route and keep recovery reachable independently of new-effect readiness |
| `jobs/workflows/tests/{gateway-service,gateway-http}.test.ts` | Cover exact recovery, miss, ambiguity, closure race, malformed marker, and no-effect behavior |

The complete Phase 611 path inventory is maintained in
`docs/work/IMPL-PHASE-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md`.

## Edge Cases Handled

- response loss after Temporal accepts start or an Update;
- process loss after durable request-start but before external I/O;
- retry of the same lease/attempt versus a newly leased reconciliation attempt;
- rollback, revocation, head loss, flag-off, entitlement drift, or operational hold after request;
- managed reconciliation miss, running workflow, closed workflow, and absent Update handle;
- historical protocol-v2 ambiguity without manufacturing managed authority;
- malformed or mismatched HTTP 202 recovery receipts and 4xx/5xx responses;
- unstarted historical commands that must never become a new effect; and
- deletion/cleanup ownership that prevents two recovery systems from racing.

## How to Test

```bash
npm --workspace @bluey/jobs-workflows test
npm --workspace @bluey/jobs-workflows run typecheck
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  jobs_workflow_dispatch::tests::
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  db::jobs::workflow_commands::tests::
git -P diff --check
```

Interim evidence confirmed 40 focused gateway-service tests, 63 combined gateway HTTP/service
tests, workflow typecheck, and a 290-test workflows run before the final recovery assertion was
added. The frozen post-edit workflow and Rust gates remain pending at this documentation checkpoint.

## Known Limitations

- Recovery proves only provider-visible exact command/update identity; a missing or ambiguous
  provider observation remains `delivery_unknown` and never authorizes a retrying effect.
- Hosted Temporal failure injection, task-queue behavior, aged visibility, and live network-loss
  rehearsal remain external evidence.
- Workflow effect dispatch and reconciliation dispatch remain disabled until explicit rollout
  approval; no production state changed.
