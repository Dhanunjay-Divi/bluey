# Round 594 - Jobs Irreversible Submission Reconciliation

**Date:** 2026-08-02

**Branch:** `feat/phase-jobs-full-autonomy-20260802`

**Status:** Implemented and verified; employer-facing distribution remains off

## Outcome

Bluey now has one durable answer to the most dangerous browser-automation
failure: Submit was clicked, but the employer's response was lost.

It never guesses and never blindly retries. The application remains blocked in
`side_effect_unknown` until one of two authoritative facts arrives:

1. a trusted, fenced runner provides the real employer confirmation receipt; or
2. the signed-in owner checks the employer portal and explicitly confirms the
   application was not submitted.

Those outcomes serialize on the same application row. Exactly one can win.

## State Contract

```text
running
  -> submitted                 trusted receipt + immutable evidence
  -> side_effect_unknown       submit response lost

side_effect_unknown
  -> submitted                 late trusted receipt within 24 hours
  -> failed / confirmed absent owner confirms not submitted
```

The second path releases the prior reservation and execution authority, writes
an audit receipt, and leaves a future retry as a new deliberate decision. It
does not automatically queue another employer-facing attempt.

## Transaction Boundary

The reconciliation transaction locks and validates:

- account and application;
- exact run and browser session;
- local ticket or cloud execution lease;
- attempt reservation;
- application state; and
- reconciliation receipt.

The browser session, execution authority, attempt, receipt, and application
either all change or all roll back. Repeat owner confirmation is idempotent.

## Portal Experience

Applications with an unknown submit result display a focused action:
`I checked: not submitted`.

The action opens an accessible Bluey confirmation dialog explaining that the
user must first verify the employer portal. It does not expose a generic reset,
retry, or "mark submitted" control.

## Fault Evidence

Automated tests prove:

- local submit uncertainty is terminal until reconciliation;
- cloud submit uncertainty leaves the browser in intervention state;
- owner reconciliation releases the exact cloud attempt and is idempotent;
- a late trusted cloud receipt produces one Submitted application with real R2
  evidence;
- the losing race receives a conflict rather than mutating state;
- submitted rows cannot be reverted; and
- full Jobs library behavior remains intact.

## Verification

```text
Portal focused tests: 6 passed
Portal full suite: 89 passed
Portal strict typecheck: passed
Portal production build: passed
Jobs HTTP integration slice: 18 passed
Jobs Rust library suite: 262 passed
Rust strict Clippy: passed
Rust formatting: passed
git diff --check: passed
```

## Production Boundary

No production flag is enabled by this round. Model generation, local Browser
distribution, cloud Browser distribution, and mailbox sync remain off. Real ATS
tenant certification and the final multi-process/browser fault matrix are still
required before employer-facing execution can launch.
