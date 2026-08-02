# Round 582 - Jobs Runner Authority And Allowance Atomicity

Date: 2026-08-01

Status: implementation complete; reviewed locally; not deployed

## Objective

Close the last public-customer paths that could imitate a verified Jobs runner,
make final submission evidence transactional, and preserve correct packet
allowance accounting across generation failures and billing-period rollover.

## Authority Boundary

Bluey now separates customer intent from employer-facing execution:

1. A customer may review, approve, pass, or cancel an application packet.
2. Approval commits packet metering exactly once and moves the application into
   the queue through the dedicated approval route.
3. Generic customer updates cannot set `queued`, `running`, `needs_input`,
   `side_effect_unknown`, or `submitted`.
4. Customer routes cannot create application evidence.
5. Only an authenticated runner finalization may store submission evidence and
   transition an application to `submitted`.

The finalization transaction binds:

- account and application;
- cloud lease or one-time local run ticket;
- runner and run ID;
- server-computed request fingerprint;
- exact resume version and evidence object hash;
- one real confirmation evidence record;
- attempt reservation;
- terminal browser-session state;
- immutable receipt payload and submitted timestamp.

An identical receipt replay returns the existing result. A different receipt
for an already-submitted application fails closed.

## Browser Writeback

The Playwright bridge no longer hides ambiguous controls with `.first()`. Text,
select, checkbox, and file writes are read back from the employer page. If the
ATS does not retain the value or document, the runner stops before submission.

This is a correctness guard, not a claim of universal ATS certification.

## Allowance Accounting

Generation reservations now carry period authority through packet commit:

- a same-period reservation is reused without consuming a second included
  packet;
- an old-period reservation cannot suppress current-period metering;
- a generation that fails before provider exposure may release its reserved
  packet slot;
- packet commit remains idempotent per canonical application packet.

## Verification

The complete local gate passed:

- 473 Jobs TypeScript tests across 78 files;
- Jobs TypeScript typecheck and production build;
- full server Rust test suite across all targets;
- strict Rust formatting and Clippy with warnings denied;
- Jobs privacy, source-provenance/license, schema-parity, client/server
  boundary, and CI guard self-tests;
- `git diff --check`.

The portal production build retains the existing Vite large-chunk advisory;
there is no build failure.

## Deployment Boundary

This round is not deployed. It does not change production services or enable:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED
BLUEY_JOBS_MAILBOX_SYNC_ENABLED
```

The change must be independently reviewed and merged before an exact artifact
can enter the standard backup, canary, promotion, and rollback workflow.

## Result

The public Jobs portal can express reviewed customer intent, but it can no
longer manufacture the evidence or terminal state of a real employer
submission. Submission truth belongs to the verified runner transaction, and
packet allowance accounting remains correct across failure and rollover.
