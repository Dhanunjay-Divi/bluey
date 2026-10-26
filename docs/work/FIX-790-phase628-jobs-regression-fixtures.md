# FIX-790 — Phase 628 Jobs regression fixture corrections

Codex preflight: `bluey-ops`. Updated 2026-10-26. Jobs-only local work.

## Issue and root causes

The first complete server library run reported 1,681 passes and ten failures.
It was not a release pass. The failures included:

- An ATS child-count test assigned `2` even though the real signed fixture now
  contains two layout observations. It did not corrupt that column.
- General-channel ATS page loading performs nine reads; its stale assertion
  counted a canary membership query which is not executed for that channel.
- Packet preference drift used `UPDATE` on a preferences row which had never
  been persisted. The update changed zero rows, so the expected rejection was
  never exercised. The inactive-Track test also asserted an obsolete outer
  error message instead of the current eligibility context.
- A static lease-scan test counted calls inside the public wrapper after those
  calls had moved into the shared SQLite/PostgreSQL decision helpers.
- A migration-registration test assumed pagination would forever be the last
  migration. A failure-injection fixture omitted the migration ledger now
  required before migration 058 executes.
- Membership repair returns repaired rows, not only newly enrolled sources;
  the existing-source repair test expected zero instead of one.
- The leaseable v3 startup test still called the legacy startup API.
  Once ported, its genuine positive preconditions exposed a production clock
  precision defect; that correction is documented separately in FIX-789.
- The large export fixture received a real runtime-authority denial during
  repeated positive verification. Positive fixture publication now refreshes
  its authenticated heartbeat before each lease instead of depending on a
  heartbeat installed at the beginning of a long fixture loop. Its exact
  source/ATS/heartbeat gates and production TTLs remain intact.
- A read-only native process sample found the export setup repeatedly
  republishing a growing full discovery snapshot for each added job. The
  stopped diagnostic had already exceeded six minutes; it is a lower bound,
  not a completed benchmark. The fixture now performs its initial one-job
  public setup and one final 102-job snapshot, with all imports saved through
  public APIs and all 102 source/ATS/integrity authorities still installed.
  Final runtime is recorded separately; no product-latency claim is inferred.
  Since batch publication makes every assignment due together, authority
  installation now follows the real verifier FIFO order rather than workspace
  rank. The original exact leased-job assertion remains unchanged.
  Shared FIFO selection and authenticated heartbeat refresh select the actual
  backend connection; SQLite-only fixture access is not used for PostgreSQL.

## Corrections

Tests now create actual drift (`count + 1`), assert the exact general-channel
query budget, persist preferences before preparation and assert one changed
row, examine the correct shared scan region, and check migration membership
without assuming it remains the head. The failed-migration fixture installs
its prerequisite empty ledger. Membership repair is checked as one repair
followed by a zero-effect repeat. The v3 leaseable test uses real boot authority
and checks that the rollback probe also leaves no attempt boot bindings.

The separate generation/lease/readiness defects found by static review are
production corrections documented in FIX-789, not relabeled fixture failures.

## Files

- `server/src/db/jobs/ats_certification_page.rs`
- `server/src/db/jobs/tests.rs`
- `server/src/db/jobs/original_source_verification.rs`
- `server/src/db/mod.rs`
- `server/src/api/jobs.rs`
- `server/src/db/jobs/workspace.rs`

## Verification and limitations

See IMPL for the final exact-source gate results. Neither the initial failed
full run nor a focused pass substitutes for the mandatory all-target pass.
No production guard, application effect, billing rule, TTL, or deletion fence
is relaxed by these test corrections. No cloud or production state changed.
