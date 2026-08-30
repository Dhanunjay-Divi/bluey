# FIX-747: Jobs PostgreSQL Representation Lockable Snapshot

**Severity:** P1 production availability

**Status:** Implemented; focused live PostgreSQL evidence rerun pending; independent review pending

## Issue

The current-authority workspace/list/detail projection opened a PostgreSQL `REPEATABLE READ READ
ONLY` transaction and then invoked Phase 614 authority reads that use `SELECT ... FOR SHARE`.
PostgreSQL rejects row-locking reads inside a read-only transaction, so these routes would fail on
the production database backend even though SQLite tests pass.

## Required Fix

- Preserve one coherent representation snapshot while allowing required authority locks. PostgreSQL
  uses a lockable `READ COMMITTED` transaction whose complete
  `H -> M -> ATS -> D -> integrity-control SHARE` fence blocks every relevant writer before one
  database-time sample; integrity publishers take the control row exclusively, including first-head
  insertion.
- Load profile, preferences, tracks, applications, and attempt reservations inside that same
  transaction. One ordered `UNION ALL` statement fingerprints both mutable tables before and after
  representation, including every loaded value and PostgreSQL tuple `xmin`. A changed fingerprint
  detects endpoint membership drift and exact-value ABA rewrites from every current writer, then
  retries the entire representation at most three times before failing closed. Account deletion,
  the only current direct-delete path, is excluded by the account fence already held.
- Do not take application/reservation row locks from the reader. Production writers legitimately
  use both cross-table orders; tuple-version validation preserves a coherent representation
  without adding a reader/writer deadlock edge.
- Apply the same lock-first `READ COMMITTED` rule to the public composed-integrity resolver; its
  first advisory-lock wait must not establish a transaction-wide pre-publication snapshot.
- Add a real PostgreSQL workspace/list/detail regression against an isolated database.
- Keep the route read-only by behavior: zero application/provider/effect mutation.

## Evidence

The implementation no longer relies on an early repeatable-read snapshot that can become stale
while waiting for authority locks. A static regression pins the complete prelock, publication
fence, single database clock, mutable-input retry, and absence of `REPEATABLE READ`. Configured
PostgreSQL regressions prove the shared control fence blocks its publication-UPDATE counterpart,
an application phantom changes the final mutable-input fingerprint, an exact-value reservation ABA
rewrite changes its `xmin` fingerprint, and a lock-first reader sees a writer commit made while it
waited on `H`. Static evidence pins the absence of application/reservation row locks. The
workspace/list/detail/export cases and final aggregate evidence will be recorded after the current
fix set settles.
