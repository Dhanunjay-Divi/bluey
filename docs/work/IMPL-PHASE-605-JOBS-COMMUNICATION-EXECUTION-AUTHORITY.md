# IMPL: PHASE-605 - Jobs Communication Execution Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the active
> Round 605 worktree, local handoff, current source, and verification evidence.
> No SSD/archive history, production service, live tenant, provider credential,
> or external write was used.

## Scope

**Does:**

- Adds disabled-by-default, server-owned execution for one explicitly reviewed
  Gmail/Outlook reply or Google/Microsoft calendar action.
- Preserves OAuth access and refresh tokens inside the in-process server
  boundary (`bluey-jobs-api` or `bluey-server`), adds an explicit
  connection-bound write-consent upgrade, derives capability from
  provider-returned grants, and rotates credentials transactionally.
- Binds each immutable encrypted payload to the owning account, application,
  mailbox connection, exact source/reply target where required, provider,
  provider grant, approval revision, and monotonic action revision.
- Persists a unique fenced request-start attempt before provider I/O and
  requires exact transport-specific read-back evidence before recording a
  message or calendar success.
- Classifies transport loss, timeout, provider 5xx, and malformed or incomplete
  success as `side_effect_unknown`, with no automatic write retry.
- Adds a separately gated read-only reconciliation loop with exact lookup,
  append-only attempt-bound evidence, bounded absence observations, and fresh
  review after provider-authoritative absence.
- Adds durable account-deletion and mailbox-disconnect draining fences that
  block new work while preserving exact finish/reconciliation authority for an
  already-started attempt.
- Adds privacy-safe account APIs/export, strict portal runtime decoders,
  canonical cross-language payload verification, explicit review/cancel
  controls, exact OAuth navigation validation, and truthful provider status.
- Adds paired SQLite/PostgreSQL ancestry, uniqueness, fencing, append-only,
  control-character, lifecycle, and cleanup enforcement plus parity guards.
- Rebuilds the checked-in Jobs portal and adds CI enforcement that a clean build
  must not change `web/jobs`.

**Does NOT:**

- Enable write-consent, dispatch, reconciliation, or mailbox-sync flags.
- Request or use production Google/Microsoft credentials, send a real message,
  create a real calendar event, or alter a live tenant.
- Claim live provider certification, approved redirect URIs, consent-screen
  approval, live PostgreSQL concurrency, production canary success, monitoring,
  retention approval, or production launch.
- Convert an unknown side effect into retry authority or allow read-only inbox
  consent to imply mail/calendar write authority.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `infra/{sqlite,postgres}/server-runtime/*jobs_communication_execution.sql` | Created | Paired action revision, attempt/evidence/reconciliation, provider grant, ancestry, drain, and lifecycle authority. |
| `server/src/db/jobs/communication_actions.rs` | Modified | Immutable payload, approval, claim, attempt, completion, reconciliation, cancellation, and exact proof state machine. |
| `server/src/jobs_communication_dispatch/` | Created | Server-owned Gmail, Outlook, Google Calendar, and Microsoft Calendar dispatch/read-back/lookup transports. |
| `server/src/jobs_provider_auth.rs` | Created | Server-only provider config, token bounds, capability derivation, refresh CAS, and grant revisions. |
| `server/src/api/jobs_mailbox_oauth.rs` | Modified | Explicit write-consent state, PKCE, callback, replay, account, and connection fences. |
| `server/src/db/jobs/{mailbox_sync,customer_data}.rs` | Modified | Exact reply identity, bounded provider data, connection scope, deletion/export, and privacy projections. |
| `server/src/jobs_mailbox_sync/` | Modified | Exact Reply-To normalization, bounded provider parsing, and redacted error reasons. |
| Server API/router/runtime modules | Modified | Public action endpoints and independently gated in-process workers. |
| Server database/API/provider tests | Modified | Provider, concurrency, ambiguity, deletion, OAuth, export, privacy, and dialect matrices. |
| `jobs/portal/src/lib/{communication-actions,mailbox-oauth}.ts` | Created | Strict action/hash/revision/readiness and OAuth navigation policy. |
| Shared communication hash vectors | Created | Identical Unicode/escape/array/integer canonical bytes for Rust and TypeScript. |
| Portal API/types/components/views/tests | Modified | Write-consent controls, exact review, polling, approval/cancel, and launch truth. |
| `jobs/scripts/{check-jobs-schema-parity,ci-guards-self-test}.mjs` | Modified | Paired schema and migration-registration enforcement. |
| `.github/workflows/{jobs-ci,release}.yml` | Modified | Reject a stale checked-in Jobs portal bundle after a clean build. |
| `web/jobs/` | Rebuilt | Final production Jobs portal bundle without source maps. |
| `jobs/OPERATIONS.md`, `ops/bluey-jobs.env.example` | Modified | Disabled gates, startup behavior, canary requirements, reconciliation, and incident truth. |
| `CHANGELOG.md`, Round 605, `FIX-642` through `FIX-655`, IMPL/REVIEW docs | Created/modified | Scope, defect closure, evidence, limitations, and handoff. |

## Build & Test

The final source-freeze results are:

```text
Jobs workspace                       1,507 tests / 127 files passed
  automation                           644 tests / 35 files
  browser                              219 tests / 34 files
  runner                               269 tests / 32 files
  workflows                             76 tests /  7 files
  portal                               299 tests / 19 files
Package typecheck gates                 5/5 passed
Package build gates                     5/5 passed

Portal production bundle             2,292 modules / 27 files
Portal source maps                    none
Portal aggregate SHA-256              8eadf40bc36d19daaae2169342d08dd5a140a3bd0e0d676b989c87e9cff0bcee
Portal chunk advisory                 502.51 kB, nonfailing

Rust formatting                      passed
Rust bluey-jobs-api/check-tests       passed
Rust strict all-target Clippy         passed
Focused Jobs server library             554 tests passed
Focused Jobs integration                 33 tests passed
Complete server library               1,159 tests passed
Complete integration_e2e                101 tests passed
Other server targets                      6 tests passed
Complete server total                 1,266 tests passed

Schema parity                            69 tables / 63 indexes passed
Dependency provenance                   663 lock entries / 631 versions
Approved provenance override              1 verified
Pinned repository provenance             14 repositories verified
CI guard self-tests                    passed
Account-deletion browser guard           3/3 passed
Temporary-index privacy scan           passed
Diff/conflict/source-map hygiene       passed
```

The first full Jobs-focused Rust run exposed one outdated cross-account mailbox
fixture: the new account write fence correctly rejected a nonexistent second
account before the test reached its tenant-isolation assertion. FIX-655 records
the fixture correction. Its focused rerun and the clean 554-test source-freeze
rerun passed. Strict Clippy also exposed and closed one unnecessary test-helper
allocation before the final green run.

Optional PostgreSQL cases compiled but self-skipped because no authorized
`BLUEY_TEST_POSTGRES_URL` was supplied. No result above is represented as live
PostgreSQL, provider, tenant, canary, or production evidence.

## Deviations from Plan

| Deviation or refinement | Rationale |
|-------------------------|-----------|
| Dispatch runs inside the server process, not a credential-bearing HTTP worker. Both server binaries embed it; production Jobs routing is owned by `bluey-jobs-api.service`. | Provider credentials must not cross a new lease/API boundary, and a flag change must coordinate every worker-capable service loading `bluey-jobs.env`. |
| Provider success requires transport-specific evidence and Gmail post-send read-back. | A generic nonempty object ID cannot prove the exact reviewed action occurred. |
| Persisted action order uses an atomic revision instead of timestamps. | Multiple transitions can share one millisecond and runtime readiness can change without mutating the action. |
| Reconciliation stores repeatable exact absence observations. | Three separately timed authoritative absences must remain auditable without weakening uniqueness or ancestry. |
| Account deletion and disconnect enter durable drain state when an irreversible attempt is unresolved. | A transient refusal would let later actions begin and keep moving the deletion target. |

No refinement enables a provider flag, broadens consent implicitly, or converts
local fixtures into provider, tenant, canary, or production evidence.

## Known Follow-ups

- Complete approved Google and Microsoft application, redirect-URI,
  consent-screen, data-rights, and credential-custody reviews.
- Run authorized Gmail, Outlook, Google Calendar, and Microsoft Calendar
  sandbox/live matrices, including delivery/read-back, invitation, outage,
  throttling, revocation, disconnect, deletion, and duplicate-prevention cases.
- Apply and exercise the paired migration against live PostgreSQL with
  multi-process claims, token-refresh races, reconciliation, failover, and
  recovery.
- Approve retention, monitoring, alerting, manual unknown-outcome escalation,
  support ownership, and canary stop conditions before any flag change.
- Use a separate reviewed production change and restart
  `bluey-jobs-api.service` plus every other worker-capable service loading
  `bluey-jobs.env` before enabling any independently approved communication
  worker flag.

## Review Checklist (for reviewer)

- [x] Complete source and generated-bundle diff reviewed line by line
- [x] Every source-testable AC1-AC30 boundary has final passing evidence
- [x] SQLite/PostgreSQL parity, privacy, secrets, provenance, and CI guards pass
- [x] Server fmt/check/tests/strict Clippy and full Jobs tests/typechecks/builds pass
- [x] All three communication flags remain `0`
- [x] External provider, live PostgreSQL, canary, and production claims remain parked
