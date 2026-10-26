# FIX-789 — Source-Verifier Startup/Readiness Authority

Updated: 2026-10-26. Scope: Jobs-only Phase 628, local implementation.

## Issue and root cause

Startup identity used only (runtime instance, epoch). If degraded N advanced to
ready N+1 but the response was lost, a restarted client could send another
degraded heartbeat and a conflicting startup request. Its failure report could
then lower committed readiness. Ordering client replay before heartbeat was
not a protocol guarantee.

The continuation also found two gaps hidden by the old fake-instance tests:
boot authority did not prove the real runtime session, and the v3 signed-release
fixture could not enter the database because its ten capabilities exceeded
the historical manifest CHECK upper bound of nine.

A separate GPT-5.6-Sol static pass then found that readiness alone did not
fence operational leases: an old process could inherit a replacement boot's
ready heartbeat. It also found that a declared `probe_failed` heartbeat could
be promoted by startup. Those were real implementation blockers, not stale
test assertions.

The real leaseable positive fixture also exposed a clock precision defect:
managed SQLite startup time was truncated to seconds, but assignment eligibility
uses millisecond database time. A job created earlier in the same second could
be incorrectly reported as `idle`. The SQLite leaseability probe now samples
the real lease path's millisecond database clock, never earlier than its
authority timestamp. No sleep, TTL widening or fixture backdating hides this.

## Correction

- Stable process boot identity; server-assigned monotonic generation.
- Idempotent registration/startup tokens with exact canonical replay and
  explicit changed-payload conflicts.
- Exact worker-HMAC admission plus actual instance/session authentication,
  immutable runtime/release binding and current-generation fencing.
- A server-only rollback probe through the real lease-decision path. Startup
  commits probe result, ready generation, heartbeat and audit atomically.
- Generic heartbeat binding validates generation even on exact replay.
  Old/completed failure reports cannot lower readiness.
- Verifier leases, attempt heartbeats and new terminal publications require
  the process's exact current ready boot. Each real claim persists an
  immutable attempt/boot binding; a replacement boot cannot adopt an older
  attempt. Exact response-only terminal replay retains its original binding.
- Startup and its SQL insert guard require exactly `degraded/startup`, not
  `probe_failed`. Attempt bindings follow authorized parent-attempt deletion
  but reject direct deletion or rebinding.
- Bounded transport/transient retries send identical bodies/tokens. No v1
  capability downgrade, client probe result, fabricated idle or speculative
  failure heartbeat after ambiguous startup.
- Prospective SQLite 065 / PostgreSQL 043 migrations preserve committed
  063/041 and 064/042, keep legacy predicates for non-boot manifests and bind
  new ready transitions to boot startup authority.
- The same successor admits the exact ten-capability v3 metadata shape while
  preserving historical manifest fields/bytes and immutability.
- Release/CI guards bind the ordered boot protocol, current migration heads,
  authenticated client/lifecycle and server sources. Parity covers boot tables.
- Remove one unused Jobs test helper and repair three proven test-only strict
  Clippy findings; do not suppress warnings or change production semantics.

## Regression coverage

Real managed-runtime fixtures cover response-loss replay, identical stored
bytes/time, changed token payloads, unknown instance/bad session, registration
generation advance, SQLite duplicate startup concurrency, stale startup,
stale failure heartbeat, replacement-boot lease fencing, failed-probe startup
rejection and missing readiness authority. Migration replay
checks populated manifest/child rows, ready authority and foreign keys.
The legacy v3 startup endpoint is tested as unable to bypass boot authority;
historical v1/v2 release and runtime compatibility remain separately tested.

## Evidence

Use the current IMPL verification ledger. Historical Kiro counts describe
earlier source states, not current executable proof. A passing test that returns
early because PostgreSQL is unconfigured is not PostgreSQL runtime evidence.

## Remaining release boundaries

The independent reviewer must inspect the complete Phase 628 diff and rerun
critical gates. Configured non-production PostgreSQL and exact Docker/native
evidence must be stated explicitly as passed or pending. Production flags,
accounts, credentials, applications and messages are untouched by this batch.
