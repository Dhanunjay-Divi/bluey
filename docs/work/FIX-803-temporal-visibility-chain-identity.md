# FIX-803: Resolve omitted Temporal visibility chain identity through exact Describe

Preflight: `bluey-ops`, security review and Round 630.

## Issue and root cause

Real workflow qualification passed eight cases, then a valid cleanup received
409 identity conflict. Direct provider diagnostics showed matching workflow,
run, type, status, timestamp and exact opaque memo bytes. Describe supplied the
expected first-run identity; the visibility result did not. The real SDK's
protobuf decoder materializes an omitted `firstRunId` as an empty string. Mock
fixtures had always supplied it and did not exercise that transport difference.

Forwarding that missing identity unchanged made the cleanup service's strict
chain binding reject a valid run. It is not safe to fill it from a caller's
proposed root, the current/latest workflow, or the visible run ID itself.

## Bounded fix

Only the raw Temporal visibility adapter changes. Before returning a page:

- Reject non-array or >100-row pages before any extra lookup.
- Validate missing-root lookup identities, then process ordered batches of at
  most eight exact workflow-ID/run-ID Describe requests in the caller's existing
  deadline context.
- Require the described workflow, run and type to match that visible row, with
  a valid nonempty provider first-run identity. Copy only that one field.
- Preserve every original visibility status, timestamp and memo. Never replace
  a nonempty identity; contradictory values still fail the service's exact guard.
- Throw on missing/stale/error/mismatch. Existing callers return unavailable or
  pending, never an absence proof, partial page or authorization to mutate.

No cleanup reconciliation, memo validation, generation, first-run binding,
termination, deletion or final absence fence changes.

## Regression coverage

Tests use real protobuf encode/decode and cover omitted/default identity for
both workflow protocols, exact lookup arguments and AsyncLocalStorage deadline
inheritance. Negatives cover foreign nonempty roots, wrong described identities,
type or root, stale NotFound, oversize/malformed pages before RPC, maximum-eight
concurrency, preserved order and no later batch after a failure. Real service
cleanup and raw omission observations still require the final runtime rerun;
actual results are recorded in IMPL-630, not inferred from static review.

Observed final evidence: 63 focused cleanup units passed, workflow typecheck
passed, and all ten real Temporal cases passed. Raw provider visibility did
contain an empty first-run field. Exact cleanup preserved the wrong-digest
denial, then proved Describe/history/visibility absence after about 35 seconds
of asynchronous deletion; both owned service processes exited cleanly.
