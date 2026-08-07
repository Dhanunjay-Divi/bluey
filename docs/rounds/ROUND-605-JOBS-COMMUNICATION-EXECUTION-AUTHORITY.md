# Round 605 — Jobs Communication Execution Authority

**Date:** 2026-08-06

**Branch:** `feat/phase-605-jobs-communication-execution`

**Status:** SOURCE COMPLETE AND LOCALLY VERIFIED — source-only; Rust 1.97 and Browser
workflow-definition compatibility are locally closed, fresh hosted exact-SHA checks remain a
merge gate, all provider write/write-consent/reconciliation flags remain disabled, and no live
provider or production authority is claimed

## Objective

Complete the source authority needed to deliver an explicitly reviewed recruiter reply or
interview-calendar action through the exact connected Google or Microsoft account, and to
reconcile an ambiguous provider outcome without a blind retry.

The worker runs in-process inside the server binaries rather than through a credential-bearing
HTTP lease. Both `bluey-jobs-api` and `bluey-server` embed the worker; the production Jobs routes
run under `bluey-jobs-api.service` on port 8081. Decrypted OAuth credentials remain in the existing
server-only credential boundary and are never returned through a Jobs API, worker lease, log,
portal response, or evidence object. A flag change must coordinate every running service that
loads `bluey-jobs.env`, including `bluey-api.service` when its Jobs environment drop-in is installed.

## Release Boundary

This round does not request production OAuth scopes, dispatch a real message or event, enable a
worker, modify a live tenant, or claim a provider canary. These independent gates default off:

```text
BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED=0
BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED=0
BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED=0
```

Existing protected Jobs flags also remain `0`. Authorized Google/Microsoft applications,
reviewed redirect URIs, elevated-consent review, provider sandboxes, live PostgreSQL concurrency,
monitoring, retention approval, and production canaries remain external gates.

## Authority Model

1. A public account API may create an immutable encrypted draft and expose it for review.
2. Approval records a new revision over the exact payload hash, application, connection,
   provider, source message where required, and current server-derived provider grant.
3. The server-owned worker may claim only while dispatch is explicitly enabled and every bound
   authority remains current after the database lock is acquired.
4. An append-only request-start record is durable before any provider write request.
5. Success requires exact provider evidence. Transport loss, provider 5xx, or malformed success
   becomes `side_effect_unknown` and is never dispatched again automatically.
6. Reconciliation is a separate read-only lookup authority. Exact lookup may prove success;
   bounded authoritative absence returns the action to `needs_input` with prior approval
   invalidated. Inconclusive or conflicting lookup stays unknown.

## Acceptance Criteria

1. `reply` accepts only Gmail or Outlook Email; `calendar` accepts only Google Calendar or
   Outlook Calendar.
2. Payload schemas are closed and bounded. Reply headers reject CR/LF injection; calendar times,
   IANA time zone, and attendees are validated exactly.
3. A reply is bound to one stored inbound provider message on the same account, connection, and
   application, including the provider message/thread or conversation identity needed for reply.
4. Public action summaries never expose payload text, idempotency keys, provider object IDs,
   worker identity, lease material, attempts, OAuth scopes, or credentials.
5. Public detail returns the immutable reviewed payload only to its owning account and includes a
   server-authoritative execution availability boolean and bounded reason.
6. Approval is unavailable unless the exact provider connection is connected, the required
   write capability was derived from actually granted scopes, and dispatch is release-enabled.
7. Every persisted action transition increments an atomic action revision. Approval records a
   separately monotonic approval revision plus exact payload/grant binding; the API
   compare-and-swaps the reviewed action revision and payload hash, so stale reapproval cannot
   authorize a later state.
8. Awaiting, needs-input, and approved drafts may be cancelled before dispatch; cancellation
   permanently blocks claim. Dispatching, unknown, and terminal actions cannot be cancelled.
9. Legacy `failed` actions are never reclaimed automatically. Every new dispatch after a proved
   no-side-effect outcome requires a fresh user approval revision.
10. Dispatch claims use database serialization and revalidate account deletion, connection,
    provider, credential revision, granted scopes, payload hash, approval revision, due time,
    attempt bound, and dispatch flag after the lock is acquired.
11. Each attempt has a unique monotonically fenced identity and deterministic opaque provider
    request marker committed before network I/O.
12. Gmail replies use an opaque RFC Message-ID and exact source thread; success requires a real
    Gmail message/thread identity consistent with the request.
13. Outlook replies bind the immutable source message/conversation and issue one exact MIME reply;
    success requires Microsoft Graph evidence consistent with the request marker.
14. Google Calendar uses a deterministic opaque event ID plus private marker; an existing exact
    event is idempotent success, not a duplicate create.
15. Microsoft Calendar uses a stored transaction ID plus private marker; success or lookup
    requires exactly one matching event.
16. A provider write is never issued when the credential is missing, expired without a valid
    refresh path, revoked, disconnected, under-scoped, capability-mismatched, or action-stale.
17. Token refresh remains server-only, stores rotated refresh tokens atomically, and derives write
    capabilities only from exact returned grants. A refresh response may preserve the exact prior
    grant only when the provider omits its scope field; initial or changed consent requires exact
    scope evidence.
18. Write-scope OAuth is a separate explicit upgrade/reconnect flow and is unavailable while its
    release flag is off; the existing read-only consent is never silently broadened.
19. A definitive provider rejection before any possible side effect records a bounded failure and
    invalidates the approval for reviewed retry.
20. Timeout, connection loss, provider 5xx, or 2xx without exact proof records
    `side_effect_unknown`; the complete post-marker provider future is bounded by the absolute
    attempt lease deadline, so suspension, restart, expiry, or another worker never polls a stale
    write future.
21. Reconciliation has its own fenced lease and append-only observations; it cannot perform a
    provider write or call a write transport method.
22. Exact lookup found atomically records provider evidence and the correct success status.
    Conflicting, multiple, or inconclusive matches remain `side_effect_unknown`.
23. Bounded repeated authoritative absence transitions to `needs_input`, clears lease material,
    and invalidates the prior approval. It never becomes directly retryable.
24. Account deletion and mailbox disconnect fence new work before side effects and conservatively
    preserve unresolved audit authority until the existing deletion lifecycle may purge it.
25. Account export includes customer-visible communication action state without credentials,
    lease secrets, private provider evidence, or raw provider errors.
26. The portal lists only privacy-safe summaries, fetches payload on demand, verifies its canonical
    hash, requires an explicit review acknowledgement, labels approval as approval rather than
    send, and offers no retry for an unknown outcome.
27. The portal fails closed on malformed kind/provider/status/payload/time/hash data and truthfully
    explains when provider execution is unavailable.
28. SQLite and PostgreSQL schemas stay structurally equivalent; append-only attempts,
    reconciliation observations, uniqueness, fencing, and deletion behavior are tested.
29. Tests cover all four transports with fixtures, exact scopes, token rotation/revocation,
    disconnect/deletion races, timeout/5xx/malformed success, restart without duplicate write,
    exact lookup outcomes, stale fences, concurrency, encryption, API privacy, and log redaction.
30. Operations, environment examples, changelog, implementation evidence, and independent review
    identify source completion separately from live provider certification and keep every release
    flag at `0` until its external gate is proved.

## Verification Plan

- Focused Rust database, API, OAuth, dispatch, transport, deletion/export, and integration tests.
- Portal unit tests, strict TypeScript, and production bundle rebuild.
- SQLite/PostgreSQL schema parity and migration registration checks.
- Full Jobs workspace tests/typechecks/builds plus server fmt, check, strict Clippy, and tests.
- Privacy/secret scan, CI guard self-tests, diff hygiene, and independent line-by-line review.

## Final Local Verification

| Gate | Result |
|------|--------|
| Jobs workspace tests | Passed: 1,507 tests across 127 files |
| Jobs typechecks and builds | Passed: all five workspaces |
| Portal | Passed: 299 tests across 19 files; 2,292-module, 27-file production bundle; no source maps; aggregate SHA-256 `8eadf40bc36d19daaae2169342d08dd5a140a3bd0e0d676b989c87e9cff0bcee` |
| Server formatting/check/strict Clippy | Passed with warnings denied |
| Complete server test matrix | Passed: 1,159 library, 101 integration, and 6 other-target tests; 1,266 total |
| Focused Jobs server matrix | Passed: 554 library and 33 integration tests |
| SQLite/PostgreSQL source parity | Passed: 69 tables and 63 indexes |
| Privacy | Passed on a temporary-index snapshot containing tracked and untracked Phase 605 files |
| Provenance/license | Passed: 663 lock entries, 631 package versions, 1 audited override, and 14 commit-pinned repositories |
| CI guards and account-deletion browser guard | Passed; account deletion 3/3 |
| Diff, conflict-marker, and source-map hygiene | Passed |
| Independent review | Backend and portal source accepted; documentation findings closed in the Phase 605 review record |
| Rust 1.97 compatibility | Server fmt/all-target Clippy and 7 final-submit tests passed; native macOS Clippy/14 tests/release build and Linux cross-target Clippy passed |
| Browser release workflow | 10 release-authority tests, exact 25-input contract validation, real materialized promotion path, and `actionlint` passed |

GitHub's first hosted checks exposed inherited rolling-toolchain findings and a Browser release
workflow that GitHub could not schedule with 29 dispatch inputs. FIX-656 and FIX-657 close those
defects and add independent regression coverage. The Browser promotion envelope preserves signed
authority schema order, verifies exact digests, and does not authorize a release. Fresh hosted
checks on the final pull-request SHA remain required before merge and are not production evidence.

The next exact-SHA rerun passed every Browser, observability, native Darwin, server, and
cross-platform CI gate, plus every preceding Jobs step, before the combined Ubuntu runner exhausted
its ephemeral disk at the final server integration test. FIX-658 bounds CI artifact growth and
reclaims only already-verified ephemeral build outputs before the server matrix. Its fresh exact-SHA
rerun remains required; this is CI reliability work, not production authority.

The optional PostgreSQL cases compiled but self-skipped because no authorized
`BLUEY_TEST_POSTGRES_URL` was supplied. This is source evidence only, not live PostgreSQL,
provider, tenant, canary, or production evidence.

## External Production Boundary

The following remain parked behind separately authorized work:

- approved Google and Microsoft applications, exact redirect URIs, consent-screen review, and
  credential custody;
- authorized Gmail, Outlook, Google Calendar, and Microsoft Calendar sandbox/live matrices;
- live PostgreSQL migration, multi-process concurrency, failover, and recovery;
- retention, monitoring, alerting, support ownership, unknown-outcome escalation, and canary stop
  conditions; and
- an explicit reviewed production change coordinating every service that loads
  `bluey-jobs.env` before any flag changes from `0`.

No fixture, local test, bundle, source review, or this document authorizes provider delivery.
