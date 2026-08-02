# Round 593 - Jobs Discovery Evidence and Execution Gates

**Date:** 2026-08-02

**Branch:** `feat/phase-jobs-full-autonomy-20260802`

**Status:** Implemented and verified; production capability flags unchanged

## Outcome

Bluey Jobs no longer treats a URL, an external feed row, an ATS response, or an
`active` status as enough evidence to run an employer-facing application.
Discovery truth is now server-owned, explicit, bound to the exact job, and
rechecked by the same eligibility engine used throughout the application
lifecycle.

## Why This Was Required

Continuous feeds intentionally ingest broad leads. Those leads can be stale,
reposted, duplicated, redirected, impersonated, or detached from the employer's
current application form. Previously, several import paths populated freshness
fields that could look authoritative even when no current source verification
had occurred. That was acceptable for browsing, but not for irreversible
submission.

The new contract separates discovery value from execution authority.

## Evidence Contract

Every `JobPosting` carries `JobDiscoveryEvidence` with:

- provenance;
- canonicalization status and canonical job ID;
- employer verification status and employer ID;
- canonical employer and application domains;
- scam-risk state and typed signals;
- original-source status, check time, expiry, hash, and mismatch fields; and
- an explicit original-source revalidation requirement.

The default is unknown and requires revalidation. Deserialization of an old row
therefore cannot silently grant new execution authority.

## Execution Boundary

### Rankable lead

An external feed entry may appear in Matches after normal filtering and ranking.
It cannot produce or queue an application until its original employer source is
checked.

### Review-first source snapshot

A fresh response from an allowlisted hosted ATS can support packet preparation
when the response is bound to the exact canonical key and application domain.
This remains Review-first: ATS availability is not independent proof of employer
identity or a scam-clear decision.

### Queue-eligible original source

A runner queue requires all of the following at decision time:

- exact canonical key match;
- verified employer identity;
- exact application-domain binding;
- scam status `clear` with no blocking signals;
- original source `verified_open`;
- a non-empty evidence hash;
- a check within its recorded freshness window; and
- no closed, reposted, duplicate, malformed, mismatched, or impersonated state.

These checks supplement, rather than replace, Career Track role, experience,
location, work authorization, employment type, company collision, daily limit,
claim, identity, resume, and Auto-submit authority checks.

## Files

- `jobs/automation/src/discovery-quality.ts`
- `jobs/automation/tests/discovery-quality.test.ts`
- `server/src/db/jobs.rs`
- `server/src/db/jobs/eligibility.rs`
- `server/src/api/jobs_import.rs`
- `server/src/db/jobs/discovery.rs`
- `server/src/db/jobs/global_materialization.rs`
- `server/src/api/jobs.rs`
- focused Jobs fixtures and integration tests

## Acceptance Evidence

The implementation proves:

- external feeds remain leads until revalidation;
- unknown evidence fails closed;
- hosted ATS imports prepare only in Review-first mode;
- canonical-key and domain mismatch hard-fail;
- employer mismatch, impersonation, scam block, closure, stale evidence,
  duplicate, and repost states prevent queueing;
- fully bound, fresh original-source evidence can queue only after every other
  server eligibility rule passes; and
- the runner-plan matrix cannot use made-up canonical evidence.

Verification:

```text
Jobs automation: 28 files / 223 tests passed
Jobs automation typecheck and build: passed
Rust formatting: passed
Rust strict clippy: passed
Rust unit tests: 818 passed
Rust integration tests: 77 passed
Runner plan matrix and supporting suites: passed
```

## Production Boundary

No deployment, service restart, or feature-flag change is part of this round.
Model generation, local Browser distribution, cloud Browser distribution, and
mailbox sync remain disabled. Scheduled source revalidation, operational source
canaries, and broad provider certification remain required before unattended
execution can be launched.
