# IMPL: PHASE-606 - Jobs Launch Safety Control Plane

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the local-first
> handoff against the Phase 605 exact-green successor worktree. No SSD/archive,
> production service, live tenant, provider credential, or external write was
> used.

## Scope

**Does:**

- Adds paired append-only operational-hold history and monotonic CAS heads.
- Adds admin-only hold mutation, redacted listing, fail-closed readiness, and
  privacy-safe Prometheus aggregates on both server deployment surfaces.
- Stops new discovery, generation, queue, runner-claim, final-submit,
  mailbox-sync, and communication-dispatch authority transactionally.
- Preserves exact post-marker completion, receipt, and reconciliation paths.
- Reconstructs and validates every canonical current head, including released
  heads, before admission, readiness, listing, replay, or release.
- Fences PostgreSQL application/job scope as one account snapshot shared with
  every posting, source, membership, materialization, and Track writer.
- Validates canonical, relational, and encrypted mailbox-provider projections
  before direct or batch sync leasing.
- Freezes relevant Track mutation for an unexpired discovery lease, validates
  relational/JSON active parity, excludes inactive curated Tracks, and retains
  bound inactive-Track Region scopes.
- Rejects a curated materialization orphan until its exact managed account
  membership exists, and omits `runner_kind` for an unassigned reservation
  until a concrete cloud/local claim evaluates and binds that scope.
- Preserves local `click_started` recovery across an accepted current-server
  deployment change while keeping the exact frozen Browser build/release
  binding and denying an unaccepted server release ID.
- Keeps only durable local submit/result recovery reachable through v2 submit
  grace, a distribution pause, and object-storage maximum drift. Signed v1/v2
  result recovery reuses capacity only for `click_started` or
  `side_effect_unknown`; claimed/`needs_input`/new paths derive current
  configuration, and invalid durable authority remains denied.
- Composes with native source pause and ATS circuit authority without allowing
  one control to override another.
- Removes raw discovery source keys from the operational health diagnostic and
  keeps database, diagnostic, data, and provider secrets out of child argv and
  inherited environments.

**Does NOT:**

- Enable a Jobs, model, Browser, mailbox, communication, or provider flag.
- Use credentials, send a provider request, change a live tenant, deploy, or
  claim live PostgreSQL/device/canary evidence.
- Let a generic release close a signed ATS circuit or resume a paused source.
- Treat an operational release as feature enablement or production authority.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `infra/{sqlite,postgres}/server-runtime/*jobs_operational_holds.sql` | Created | Immutable event history, exact ancestry, CAS heads, and indexes. |
| `server/src/db/jobs/operational_holds.rs` | Created | Canonical validation, append/replay/CAS, context matching, and transactional enforcement. |
| Server DB admission modules | Modified | Enforce holds before new work while preserving recovery paths. |
| `server/src/db/jobs/local_runner.rs` | Modified | Recover an exact durable local click-started authorization without issuing new irreversible authority. |
| `server/src/api/jobs.rs`, `server/src/db/object_uploads.rs` | Modified | Keep only durable click-started HTTP recovery reachable and reconstruct its exact active capacity under current upload limits. |
| `server/src/db/jobs_provider_cost_holds.rs` | Modified | Recover an exact provider request-start marker before applying a later generation hold. |
| `server/src/db/jobs/{operational_holds,profile_postings,discovery,mailbox_sync,global_materialization}.rs` | Modified | Fence PostgreSQL scope snapshots, mailbox projection, Track leases, and curated materialization authority. |
| `server/src/db/jobs/eligibility.rs` | Modified | Keep unassigned reservation state out of the closed runner-kind scope. |
| `server/src/api/jobs_operations.rs` | Created | Admin mutation, redacted list, and readiness endpoints. |
| `server/src/{db,api}/metrics.rs` | Modified | Closed-label Jobs hold/readiness metrics. |
| `server/src/{db,api}/mod.rs` | Modified | Runtime migrations and both router surfaces. |
| `jobs/scripts/{check-jobs-schema-parity,ci-guards-self-test}.mjs` | Modified | Paired schema registration plus semantic trigger/function/ancestry drift verification. |
| `ops/check-bluey-jobs-discovery.sh` | Modified | Opaque overdue-source references and sanitized child-process secret boundaries. |
| `jobs/OPERATIONS.md`, changelog, Round/IMPL/REVIEW/FIX docs | Modified | Operational contract, evidence, limitations, and handoff. |

## Build & Test

The complete local matrix for code snapshot
`72056e6db026dcd25ebe54bd41e1681e4185c727` is green:

```text
Jobs workspace                         1,507 tests / 127 files passed
  automation                             644 tests / 35 files
  browser                                219 tests / 34 files
  runner                                 269 tests / 32 files
  workflows                               76 tests /  7 files
  portal                                 299 tests / 19 files
Package typecheck gates                   5/5 passed
Package build gates                       5/5 passed

Portal production bundle               2,292 modules / 27 files
Portal source maps                      none
Portal aggregate SHA-256                8eadf40bc36d19daaae2169342d08dd5a140a3bd0e0d676b989c87e9cff0bcee
Portal chunk advisory                   502.51 kB, nonfailing

Server Rust formatting/check           passed; all targets
Server strict Clippy                    passed; all targets, warnings denied
Server library                         1,243 tests passed
Server integration_e2e                   101 tests passed
Server auxiliary targets                   7 tests passed
Complete server total                  1,351 tests passed

Schema parity                             71 tables / 66 indexes passed
Dependency provenance                    663 lock entries / 631 versions
Approved provenance override               1 verified
Pinned repository provenance              14 repositories verified
CI guard self-tests                     passed
Browser release contract                  10/10 tests passed
Account-deletion browser guard              3/3 passed
Native runner storage                      14 tests + release build passed
Discovery diagnostic                    passed
Bluey operations docs                   passed
Tracing/privacy analysis                passed; 0 transitional / 0 PII findings
Staged-index privacy scan               passed; 2,516 paths / 2,243 text files
Diff/conflict/source-map hygiene        passed
Independent line-by-line review         GREEN; no blocker or minor
```

Commands were run from the repository root with explicit
`--manifest-path server/Cargo.toml` where applicable, plus
`npm test --prefix jobs`, `npm run typecheck --prefix jobs`, and
`npm run build --prefix jobs`. The optional PostgreSQL migration/concurrency
tests compiled and self-skipped because `BLUEY_TEST_POSTGRES_URL` was not
configured. Docker is not installed in this local environment, so the managed
runner image build/smoke remains a hosted/external gate; no modified runner
image source is part of this batch.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Live PostgreSQL evidence remains optional and parked | No authorized test URL or production credential was available; source includes isolated rollback-only migration and concurrency coverage. |
| Current server deployment is not frozen as first-call identity | The exact Browser build/release binding is frozen. Recovery may use another server ID only when the immutable bound activation already accepted it, so a deploy cannot strand `click_started` recovery. |

## Known Follow-ups

- Phase 607 installed Bluey Browser update, rollback, and crash-recovery
  authority before local distribution can be certified.
- Authorized live PostgreSQL migration/concurrency rehearsal.
- Production monitoring ownership, alert thresholds, and canary stop exercise.
- Exact artifact promotion and immutable public read-back.
- Credentials, provider writes, live tenants, and physical-device gates remain
  parked.

## Review Checklist (for reviewer)

- [x] Every protected admission occurs inside the same serialization boundary as the side-effect authority.
- [x] Every post-marker completion/reconciliation path remains unheld.
- [x] Expired v2 submit grace and a distribution pause reach only durable
      `click_started` database recovery.
- [x] Storage-config drift reuses only the exact active durable capacity while
      current upload limits remain enforced.
- [x] Source pauses and ATS circuits remain independent deny authorities.
- [x] Admin and metrics surfaces cannot expose raw scope identifiers.
- [x] No external or production authority is claimed.
