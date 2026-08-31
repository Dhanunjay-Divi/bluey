# Round 621 — Bluey Jobs Public Beta Gate

> **Codex preflight:** Load `$bluey-ops` and reconcile this production-boundary branch against
> `origin/main`, the live review-first Jobs posture, and the limited-public-release request before
> implementation or review.

**Date:** 2026-08-30

**Branch:** `feat/phase-621-jobs-public-beta-gate`

**Aggregate branch:** `feat/phase-622-jobs-opt-in-autonomy-v1`

**Base:** `origin/main` at `755e7d71c5ec15ea7079f7dce5a32f02b7b1fcab`

**Status:** local source candidate accepted; deployment and cohort opening are not authorized

**Aggregate integration:** Phase 621 is contained in source commit
`55d24e95234c1fcc2db22a912c739dfd5760eb66` with deterministic generated portal commit
`95bb966fd897db94599a0c7e2defe1fb02ea3912`. `3d41a455` itself is only the Phase 620A1 base and
does not contain the public-beta gate. The durable cohort gate remains dark by migration default;
aggregation alone does not open access or authorize an external effect.

## Outcome Target

Add a production-safe, public, first-come Bluey Jobs beta cohort to the small existing production
boundary. Ordinary verified Bluey accounts may enroll during a bounded public window until an
exact durable hard cap is consumed. Admission is sticky, capacity never churns when an account is
revoked or deleted, and operators can suspend all access immediately without weakening the
existing `BLUEY_JOBS_BETA_ENABLED` master kill switch.

This is not an invitation list or a random percentage rollout. It is also not a vehicle for the
39-commit managed-autonomy stack or Phase 620 messaging work.

## In Scope

- one durable public-beta cohort with states `draft`, `open`, `closed_to_new`, and `suspended`;
- a bounded enrollment window and monotonic hard cap;
- atomic first-come admission with exact SQLite/PostgreSQL concurrency parity;
- sticky account admission and cumulative slot consumption;
- revisioned administrative cohort changes and account denial overrides;
- bounded administrative grants that consume normal capacity and never exceed the cap;
- an authenticated access-status endpoint outside the customer cohort middleware;
- independent customer-route middleware for shared and standalone Jobs API composition;
- the existing master flag as the ultimate fail-closed kill switch;
- a customer-facing public-beta full, closed, verification-required, and suspended experience;
- aggregate-only operational metrics and hashed operations-audit identities;
- additive SQLite 060 and PostgreSQL 038 migrations, deliberately leaving 059/037 reserved for
  the separate Phase 620A durable messaging plan; and
- exact tests, operations documentation, changelog, implementation record, and review record.

## Out Of Scope

- invitations, email allowlists, random edge/CDN sampling, or client-owned cohort decisions;
- auto-submit, local/cloud Browser distribution, Temporal workflows, mailbox sync, communication
  writes, provider writes, WhatsApp, iMessage, MCP, C2C chat, or Phase 620 runtime capability;
- changing Bluey core authentication, meeting, router, billing, or native-client authority;
- enabling model generation, provider credentials, Auto Reload, or employer-facing effects;
- silently bundling any Phase 599–620 autonomy branch;
- deploying, opening the cohort, changing a production flag, or granting production accounts in
  this implementation round.

## Durable Authority

### Cohort

`jobs_public_beta_cohorts` stores one canonical `public-v1` cohort with:

- closed state;
- bounded `opens_at_ms` and `closes_at_ms`;
- monotonic `hard_cap` and cumulative `assigned_count`;
- compare-and-swap `revision`; and
- database-owned update timestamps.

The migration seeds `draft`, cap `0`, and no active window. No migration may open access.

### Enrollments

`jobs_public_beta_enrollments` stores one account/cohort admission with source
`public_window|admin`. Admission and the cohort counter advance in one transaction. Account
deletion may remove account-scoped enrollment data, but never decrements cumulative
`assigned_count`; a replacement account consumes another slot.

### Overrides

`jobs_public_beta_overrides` stores a revisioned administrative denial. Denial never erases the
underlying admission or returns capacity. Clearing the denial restores a prior sticky admission or
ordinary public-window evaluation.

## Access Semantics

1. `BLUEY_JOBS_BETA_ENABLED` missing or false returns the existing dark-product response.
2. Authentication runs before customer cohort evaluation.
3. Temporary or unverified accounts cannot enroll.
4. `suspended` blocks every account, including an existing enrollment.
5. An active denial override blocks the account without freeing capacity.
6. An existing enrollment is admitted in `open` or `closed_to_new` state, including after the
   original window closes.
7. A new enrollment requires `open`, database time within the half-open window
   `[opens_at_ms, closes_at_ms)`, and `assigned_count < hard_cap`.
8. Admission uses an immediate SQLite transaction or one locked PostgreSQL cohort row. Races may
   never exceed the cap.
9. Database, schema, or account-binding errors fail closed.

The authenticated `GET /api/jobs/beta-access` endpoint returns only a closed status/reason schema.
It exposes no cap, count, account ID, position, or internal cohort revision. Customer Jobs routes
independently enforce the same database authority so skipping the status call cannot bypass it.

Local-runner and worker routes retain their existing independent authentication and effect flags;
the account-cohort middleware must not weaken or reinterpret those authorities.

## Administrative Semantics

- Cohort updates require the exact expected revision.
- The hard cap can only increase and remains bounded.
- Opening requires a valid future/current bounded window and a positive cap.
- Suspension is always available and is the database kill switch beneath the master flag.
- Administrative admission locks the same cohort row, consumes one slot, and rejects capacity
  overflow.
- Deny/clear operations are revisioned and idempotent.
- Audit records contain only the existing short account/actor hashes and closed aggregate metadata.

## Portal Contract

After authentication, the portal resolves beta access before loading the Jobs workspace. It shows
plain public-beta states for full, not-yet-open/closed, verification required, or suspended access.
It never says the whole product is invitation-only and never exposes internal capacity.

## Required Evidence

- master-off 404 and unauthenticated 401;
- verified/non-temporary enrollment and temporary/unverified denial;
- exact concurrent cap enforcement on SQLite and configured PostgreSQL;
- sticky admission after close, cap increase, denial/clear, suspension/resume, and deletion;
- administrative grant, denial, clear, compare-and-swap, and monotonic-cap tests;
- shared and standalone customer-router parity;
- local-runner/worker authority non-regression;
- response and telemetry privacy checks;
- private, non-storable status and error responses plus non-storing portal fetches;
- deletion-intent-first denial of enrollment, administrative grants, and override writes without
  consuming a slot or mutating a cohort/account row;
- complete owner export for an admitted or denied account that has no Jobs profile;
- paired schema parity and migration replay;
- focused/full Rust and portal tests, strict Clippy/typecheck/build, portal freshness, privacy,
  provenance, and release guards; and
- independent source/security and documentation review with no remaining P0–P2.

## Initial Rollout Ledger

Implementation defaults remain dark: master flag `0`, cohort `draft`, cap `0`, and every
model-generation, Browser, workflow, mailbox, communication, and provider-write flag `0`.

After separate release approval and exact-artifact proof, the intended public rollout is:

1. preproduction cap `2` for operator proof;
2. production public cap `25` for a 24–48 hour soak;
3. reviewed cap increases to `100`, then `250`; and
4. general availability only after a separate release decision.

No step is enabled by this Round alone.

## Rollback

Suspend the cohort, set `BLUEY_JOBS_BETA_ENABLED=0`, restart the Jobs API, and restore the paired
prior binary/static artifact if necessary. Additive tables remain inert. Existing enrollment rows
are preserved so rollback/redeploy does not reshuffle the cohort.
