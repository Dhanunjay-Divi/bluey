# FIX-687: Workflow Command Delivery Ambiguity

> **Codex preflight:** Loaded `$bluey-ops` and verified the Phase 609 worktree directly. No
> SSD/archive, production service, live tenant, provider credential, deployment, or external
> mutation was used.

## Issue

Cloud application start and intervention resume crossed the database-to-Temporal boundary through
direct HTTP calls. A process stop, timeout, or lost response could leave database authority and the
running workflow inconsistent, while the request handler could not know whether Temporal accepted
the command.

## Root Cause

`queue_application_run` reserved and metered local work before directly calling the workflow
gateway, then bound run/session authority afterward. It also treated a generic HTTP conflict as an
existing workflow without proving the exact request and payload identity.

`update_intervention` saved approval before a bare Temporal signal and reopened local state on
delivery failure even though the signal may already have arrived. The signal had no durable command
or exact intervention identity, and the workflow stored one replaceable resolution slot.

Legacy workflow, activity, and failure payloads also allowed private application material to reach
Temporal history.

## Fix Summary

- Start and resume admission now persist one encrypted immutable command in the same database
  transaction as local business authority.
- API request handlers return the durable queued state and perform no gateway fallback.
- A separately disabled dispatcher records request-start evidence before I/O, retries one exact
  opaque identity after ambiguity, and accepts only a closed Temporal-v2 receipt.
- Start conflict recovery proves protocol, request, digest, and run identity. Resume uses an Update
  whose Update ID is the durable request ID and whose payload binds the exact current intervention.
  A running-to-closed race recovers only that exact Update; timeout without a visible handle stays
  ambiguous instead of being misclassified as proven closure.
- Two-phase prepare/publish prevents a hidden or later intervention from being resolved by an old
  answer.
- Opaque workflow/activity contracts and a closed failure converter keep customer material and raw
  failures out of deterministic history.
- Exact terminal reason/state combinations and trusted submitted-receipt terminal marking preserve
  failure, ambiguity, timeout, intervention-limit, and submitted truth across replay.
- Workflow activities trust only exact closed Jobs API errors and exact request-bound runner result
  receipts. Status-only, malformed, missing-header, or mismatched responses remain retryable.
- The runner persists a minimal encrypted ambiguity tombstone before returning
  `side_effect_unknown`, binds every result to the exact request and run identity, and reconciles
  submitted, failed, and ambiguous durable results before any restart action.
- Startup recovery proves the profile scope derived from the frozen account and application
  identity before any result/tombstone persistence. An exact committed intervention may outlive
  ordinary checkpoint expiry by trying current authority restore first. Only an expired claim
  rejected as exact `lease_unavailable` may fall back to old token/fence reconciliation and
  removal; every other restore or reconciliation error fails closed.
- Browser cleanup begins only after canonical commit. HTTP 401/403 stays retryable as repairable
  credential drift; a bounded permanent cleanup rejection cannot rewrite the committed outcome.
- The Rust dispatcher and TypeScript gateway trim and validate the same 32-to-8,192-byte RFC 6750
  bearer-token grammar before constructing or accepting an authorization header.

The cleanup-service library and database cleanup schema are scaffolding, not closure of account
deletion. The gateway neither imports the library nor registers `/workflow-cleanup`, so environment
configuration cannot enable that route. Authenticated legacy Temporal inventory is not implemented,
the database generation cannot complete, and account deletion is not wired to it.

## Files Modified

The exact Phase 609 source and documentation inventory is recorded in
`docs/work/IMPL-PHASE-609-JOBS-WORKFLOW-COMMAND-AUTHORITY.md` from the current worktree status.

## Edge Cases Handled

- Concurrent duplicate start and resume admission.
- Same idempotency authority with changed request or payload semantics.
- Process loss before commit, after request-start evidence, after Temporal acceptance, after
  response delivery, and before database acknowledgement.
- Timeout, connection loss, malformed success, unexpected conflict, gateway 5xx, expired lease,
  and stale owner/token/fence completion.
- Closed workflow-ID reuse and an already-started workflow with mismatched v2 identity.
- Workflow closure between running Describe and Update delivery, including exact prior-Update
  recovery, exact absence, mismatched recovery, and RPC-timeout ambiguity.
- Duplicate or delayed intervention resolution after a later intervention becomes current.
- Hidden intervention preparation, exact publication, exact timeout, and bounded-limit closure.
- Trusted submitted receipt before or after workflow-execution acceptance/finalization ordering.
- Runner ambiguity remains `side_effect_unknown` rather than becoming a duplicate Submit retry.
- Swapped request IDs or run bindings, status-only result absence, malformed private responses,
  restart with a staged submitted result, and restart with a durable failed result.
- Expired committed intervention recovery first attempts current authority; only exact expired
  claim `lease_unavailable` falls back to old token/fence reconciliation/removal, while current,
  differently rejected, transient, and reconciliation failures preserve fail-closed state.
- Startup rejection before persistence when the stored profile scope is not
  account/application-identity derived.
- Runner cleanup credential drift and symmetric unsafe-token rejection.

## How to Test

The final frozen Jobs aggregate passed:

```text
Automation: 644 tests / 35 files
Browser: 219 tests / 34 files
Runner: 293 tests / 32 files
Workflows: 257 tests / 12 files
Portal: 271 tests / 20 files
Aggregate: 1,684 tests / 133 files
All five Jobs typechecks and builds passed
```

Reproduce the final source gates before accepting the batch:

```bash
cd jobs
npm run typecheck --workspace @bluey/jobs-automation
npm run build --workspace @bluey/jobs-automation
npm test --workspace @bluey/jobs-automation
npm run typecheck --workspace @bluey/jobs-browser
npm run build --workspace @bluey/jobs-browser
npm test --workspace @bluey/jobs-browser
npm run typecheck --workspace @bluey/jobs-runner
npm run build --workspace @bluey/jobs-runner
npm test --workspace @bluey/jobs-runner
npm run typecheck --workspace @bluey/jobs-workflows
npm run build --workspace @bluey/jobs-workflows
npm test --workspace @bluey/jobs-workflows
npm run typecheck --workspace @bluey/jobs-portal
npm run build --workspace @bluey/jobs-portal
npm test --workspace @bluey/jobs-portal

cd ../server
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets

cd ..
git -P diff --check
```

The final Rust all-target gate passed formatting, strict Clippy, and all-target tests: 1,263 library
tests, 101 `integration_e2e` tests, one main test, one `connectinfo` test, one migration test, two
GDPR tests, two runner-plan tests, and one usage-schema test. Runner/workflows full tests,
typechecks, builds, and scoped diff checks are green. The independent Temporal command-path review
and runner review accepted with no blockers or minor findings; the focused runner recovery matrix
also passed 91 tests across four files. The consolidated Phase 609 local-source verdict is accept.
Schema parity, privacy, generated-output, and diff gates were separately reported green.

## Known Limitations

- `BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED`,
  `BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED`, and
  `BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED` remain `0`.
- The cleanup-service library is not imported or route-registered by the gateway and cannot be
  enabled by environment; complete cleanup dispatch and account deletion remain Phase 610 work.
- A caller cannot supply trustworthy legacy-zero evidence; no authenticated paginated legacy
  inventory/drain receipt exists, so the database cleanup generation is fail-closed and cannot
  complete.
- No hosted Temporal retention/history deletion, live PostgreSQL concurrency, provider KMS,
  network-fault, ATS tenant, managed browser, deployment, canary, or production flag was exercised.
- Protocol-v1 absence or deterministic replay was not proved by a genuine retained history fixture.
- Phase 610 must implement and review authenticated legacy inventory, cleanup dispatch, and exact
  account-deletion fencing before cleanup can become erasure authority.
