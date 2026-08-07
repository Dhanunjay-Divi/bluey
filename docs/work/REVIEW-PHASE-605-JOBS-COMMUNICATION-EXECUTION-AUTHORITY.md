# REVIEW: PHASE-605 - Jobs Communication Execution Authority

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the complete Phase 605
> temporary-index snapshot against the current source, local handoff, Round 605
> acceptance criteria, and recorded local evidence. No SSD/archive history,
> production service, live tenant, provider credential, or external write was
> used.

**Reviewed snapshot:** complete Phase 605 snapshot through `18a84689`, plus the
FIX-656 through FIX-658 hosted-CI compatibility diff

**Repository index:** empty; review did not rely on the actual Git index

**Reviewers:** independent backend/security, portal/documentation, hosted-CI
security, and Rust/documentation review agents; final evidence assembled by the
root agent

**Date:** 2026-08-07

## Per-Task Review

### Immutable payload, review CAS, and monotonic lifecycle

| Field | Value |
|-------|-------|
| Files | Communication action database/API modules, paired migrations, canonical vectors, portal decoder/API |
| Verdict | 🟢 accept |

**Findings:**

- Closed and bounded reply/calendar schemas, exact provider/kind matching,
  canonical cross-language hashing, invisible-control defenses, IANA zones, and
  attendee validation prevent a user from approving bytes different from those
  persisted for execution.
- Approval and cancellation compare-and-swap the exact positive JavaScript-safe
  `action_revision` and payload SHA-256. Every persisted transition increments
  the action revision, while readiness-only projections cannot manufacture a
  state transition.
- One effective monotonic timestamp drives approval audit ordering and
  `approved_at_ms`; wall-clock `next_attempt_at_ms` remains scheduling time, so a
  future audit timestamp cannot delay an otherwise eligible action.
- FIX-642, FIX-643, FIX-644, FIX-645, and FIX-650 close payload drift,
  idempotency-marker reuse, stale reapproval/cancellation, blind retry,
  cross-ancestry, revision-range, and timestamp-order defects.

### OAuth grant, exact provider execution, and ambiguous-outcome proof

| Field | Value |
|-------|-------|
| Files | Provider auth, OAuth API, communication dispatch/providers, mailbox sync, action completion/reconciliation |
| Verdict | 🟢 accept |

**Findings:**

- Write-scope consent is a separate connection-bound OAuth purpose and remains
  release-disabled. State is consumed before provider-error handling; PKCE,
  replay, account, provider, connection, grant-revision, token-size, refresh-CAS,
  and exact returned-scope checks fail closed.
- Credentials stay inside the server process. The worker persists one fenced
  request-start marker before I/O, and the complete post-marker provider future
  is bounded by the absolute database lease deadline. An expired future is
  dropped and cannot be polled again.
- Gmail success requires the exact opaque operation Message-ID plus exact bound
  thread hash. Outlook uses one exact MIME reply and requires the exact bound
  conversation hash. Google Calendar uses a deterministic event ID/private
  marker; Microsoft Calendar uses its transaction ID/private marker. Calendar
  dispatch preserves IANA time zone and attendee-invitation intent.
- Timeout, connection loss, 5xx, malformed or incomplete 2xx, and conflicting
  lookup become `side_effect_unknown` and never automatic retry authority.
  Reconciliation is read-only and separately gated.
- FIX-647, FIX-649, FIX-651, and FIX-652 close inferred write grants, generic
  success proof, OAuth callback/refresh races, provider parsing, exact Reply-To,
  source identity, and mailbox metadata-preservation gaps.

### Deletion, disconnect, privacy, and reconciliation lifecycle

| Field | Value |
|-------|-------|
| Files | Account/customer/mailbox/action database modules, public APIs/export, response cache policy, tests |
| Verdict | 🟢 accept |

**Findings:**

- Durable account- and connection-scoped draining fences serialize deletion or
  disconnect against create, approval, claim, request start, OAuth grant change,
  and mailbox writes. Exact completion and read-only reconciliation remain
  available only for the already-started attempt.
- A reconciliation cycle claims at most 25 actions; one action is bounded to 20
  claims. Exact found/inconclusive lookup may occur immediately, while an
  absence counts only after 15 minutes. Three authoritative absences require
  fresh review, and persisted unknown observations back off at least five
  minutes.
- Public summaries, detail, export, logs, and provider-facing evidence retain
  only their required authority. Private communication JSON sends
  `Cache-Control: private, no-store` and `Pragma: no-cache`; portal requests also
  set `cache: "no-store"`.
- FIX-646, FIX-654, and FIX-655 close deletion/export omission, cache/log/export
  privacy leakage, moving lifecycle targets, parent cleanup, reconnect, and
  legacy failed-row retention defects.

### Portal truth, service ownership, and disabled release gates

| Field | Value |
|-------|-------|
| Files | Portal types/API/libs/components/views/tests/bundle, workflows, operations, env example, changelog, Round/IMPL/FIX docs |
| Verdict | 🟢 source accepted; external activation blocked |

**Findings:**

- The portal fetches the immutable payload on demand, verifies the shared
  canonical hash, requires explicit acknowledgement, submits the exact CAS,
  presents approval rather than send, and offers no retry for unknown outcomes.
  Strict decoders reject malformed enums, times, hashes, revisions, timestamps,
  payloads, and OAuth navigation.
- FIX-648 and FIX-653 close the missing review UI and cross-tab/revision,
  readiness, Unicode, callback, and launch-truth gaps. Independent portal source
  review returned GREEN.
- Both server binaries embed the in-process worker, while production Jobs routes
  are owned by `bluey-jobs-api.service` on port 8081. A flag change must restart
  that service and every other worker-capable service loading `bluey-jobs.env`,
  including `bluey-api.service` when its Jobs drop-in is installed.
- The checked-in portal bundle is current and CI/release workflows reject stale
  generated output. It contains 2,292 modules across 27 files, no source maps,
  and aggregate SHA-256
  `8eadf40bc36d19daaae2169342d08dd5a140a3bd0e0d676b989c87e9cff0bcee`.
- `BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED`,
  `BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED`, and
  `BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED` remain exactly `0`.

### Hosted CI compatibility reopening

| Field | Value |
|-------|-------|
| Files | Rust final-submit/native-storage paths, Browser release gate/tests, Jobs and Browser workflows, FIX/IMPL/Round/changelog records |
| Verdict | 🟢 accept locally; fresh hosted exact-SHA checks required before merge |

**Findings:**

- Hosted Rust 1.97 surfaced `question_mark`, `useless_conversion`, and
  `unnecessary_wraps` findings are closed without changing the accepted
  final-submit filenames or Unix storage behavior. Both macOS native execution
  and Linux cross-target strict Clippy pass.
- The Browser release workflow now declares exactly GitHub's approved 25 input
  definitions. Promotion authority is carried in one 48 KiB bounded envelope,
  parsed through the actual signed activation, signature-set, and canary
  schemas, digest-checked, and written to fixed read-only files under a private
  directory.
- The workflow validator compares the exact approved input-key set and rejects
  missing, extra, inline, uppercase, hyphenated, or underscore-prefixed drift.
  Its real materialized bytes complete the existing cryptographic
  `createPromotionSet` path in tests.
- Jobs CI runs the Browser contract independently, so a malformed Browser
  workflow cannot hide its own scheduling failure. `actionlint` accepts both
  modified workflow definitions.
- Independent review initially found schema-order, input-counter, and inherited
  Linux test-lint blockers. All were reproduced, corrected, and included in the
  final regression matrix before this verdict.
- The first exact-SHA rerun passed every preceding Jobs gate but filled the
  combined runner disk at the final integration-test step. FIX-658 disables
  incremental/debug-heavy CI artifacts and removes only already-verified
  ephemeral Docker, native target, and Node dependency outputs before server
  tests; all test commands and authority boundaries remain intact.

## Cross-Task Findings

| Acceptance criteria | Independent review result |
|---------------------|---------------------------|
| AC1-AC6 | 🟢 Exact provider/kind/payload/source/grant ownership, privacy-safe summaries, and server-derived availability are source-complete. |
| AC7-AC11 | 🟢 Revision/CAS approval, cancellation, fresh reapproval, serialized claims, and fenced attempt markers are source-complete. |
| AC12-AC18 | 🟢 Gmail, Outlook, Google Calendar, Microsoft Calendar, token refresh, and separate write-consent authority are source-complete under disabled flags. |
| AC19-AC23 | 🟢 Definitive rejection, absolute deadlines, ambiguity, read-only reconciliation, exact found/conflict/absence outcomes, and fresh-review boundaries are source-complete. |
| AC24-AC25 | 🟢 Durable lifecycle drains and privacy-safe account export are source-complete. |
| AC26-AC27 | 🟢 Portal hash/review/CAS/OAuth truth and fail-closed runtime decoding are source-complete. |
| AC28 | 🟢 Paired SQLite/PostgreSQL source schemas pass parity. Live PostgreSQL execution remains external. |
| AC29 | 🟢 Four-provider fixtures, race/deadline/privacy/log tests, and complete local server/Jobs matrices pass. Live provider evidence remains external. |
| AC30 | 🟢 Operations and release wording separate source completion from provider, tenant, canary, and production authority; every communication flag remains `0`. |

The line-by-line review covered FIX-642 through FIX-658 and the complete
temporary-index snapshot. The independent backend/security verdict was
`ACCEPT—SOURCE COMPLETE`; the independent portal source verdict was GREEN. The
documentation reviewer’s startup ownership, reconciliation bounds, conditional
launch wording, privacy-canary, timestamp, evidence, and missing-review findings
were corrected before this record closed. No unresolved source-testable blocker
remains.

## Build & Test Verification

```text
Complete temporary-index snapshot based on 8845566b

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
Optional PostgreSQL cases             compiled; self-skipped without BLUEY_TEST_POSTGRES_URL

Schema parity                            69 tables / 63 indexes passed
Dependency provenance                   663 lock entries / 631 versions
Approved provenance override              1 verified
Pinned repository provenance             14 repositories verified
CI guard self-tests                    passed
Account-deletion browser guard           3/3 passed
Temporary-index privacy scan           2,489 paths / 2,216 text files passed
Diff and conflict-marker hygiene       passed
Portal source-map hygiene              passed

Post-review CI compatibility closure
Rust 1.97 server fmt/all-target Clippy passed
Rust 1.97 final-submit tests            7/7 passed
Rust 1.97 native macOS Clippy/tests    14/14 passed
Rust 1.97 native Linux cross-Clippy    passed
Native macOS release build             passed
Browser release authority tests        10/10 passed
Browser workflow contract/actionlint   passed
Jobs tests/typechecks/builds            1,507 / 5 / 5 passed
```

The initial focused Rust run exposed an outdated cross-account test fixture and
strict Clippy exposed one unnecessary test-helper allocation. Both were fixed;
the focused source-freeze rerun, complete server matrix, strict Clippy, and final
privacy snapshot passed. Hosted verification later exposed rolling-toolchain,
workflow-definition, and combined-runner disk-capacity defects. The locally
reproduced FIX-656/FIX-657 corrections pass the post-review matrix above;
FIX-658's workflow bound passes actionlint and repository guards and awaits its
fresh exact-SHA run. These discoveries did not widen provider authority.

No authorized live PostgreSQL database, Google/Microsoft application, provider
sandbox/live tenant, real provider write or lookup, retention/monitoring
approval, canary, or production deployment is represented by these results.

## Overall Verdict

🟢 **ACCEPT—SOURCE COMPLETE, NOT PRODUCTION** - The complete Phase 605 source,
including the reviewed FIX-656 through FIX-658 compatibility corrections, passes
independent source review and all available local verification gates. Fresh
hosted checks on the final exact pull-request SHA remain mandatory before merge.

This verdict does not authorize OAuth scope elevation, provider dispatch,
reconciliation, a service restart, a flag change, provider certification, live
tenant access, canary, or production deployment. Any later source change beyond
confirmatory documentation/privacy hygiene reopens the affected review scope.

## Follow-ups for Production Authorization

- Complete approved Google and Microsoft application, redirect-URI,
  consent-screen, data-rights, credential-custody, and token-rotation reviews.
- Run authorized Gmail, Outlook, Google Calendar, and Microsoft Calendar
  sandbox/live matrices, including read-back, invitation, outage, throttling,
  revocation, disconnect, deletion, and duplicate-prevention cases.
- Apply and exercise the paired migration on live PostgreSQL with multi-process
  claim, refresh, reconciliation, failover, and recovery races.
- Approve retention, monitoring, alerting, manual unknown-outcome escalation,
  support ownership, and canary stop conditions.
- Use a separate reviewed production change to coordinate service restarts and
  authorize any exact communication flag transition away from `0`.
