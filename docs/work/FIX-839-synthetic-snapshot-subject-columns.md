# FIX-839: Synthetic regression snapshot assumed a plain account column

Preflight: bluey-ops, current AGENTS/handoff, scoped security acceptance and
existing native reviewer/fixture worker. This is a test-only correction.

## Issue and root cause

FIX838 successor89202 exited1. Formatting and strict both-feature all-target
Clippy passed1m59s; test build2m35s. The original unmanaged authenticated start
regression passed1/0failed/0ignored in3.87s. The next new SQLite claim/start/replay
test failed in its snapshot query before its full assertions completed:
tests.rs20954: SELECT * FROM jobs_managed_cloud_workflow_bindings WHERE
account_id = 'acct-jobs' returned no such column: account_id.

The test helper treated most subject tables as if they had a plain account_id.
Managed workflow bindings instead store an HMAC subject. Initial static review
missed this schema fact; REVIEW638 retracts that earlier clearance. This failure
is neither a production-guard defect nor a new successful end-to-end test.
Remaining focused cases, PostgreSQL and full all-target tests were not reached.

## Fix summary / files modified

- tests.rs only: snapshot every closed-list16 table family in the tiny owned
  synthetic DB, without any account-column predicate. Keep sorted complete rows
  and the existing acct-jobs decoded-application snapshot. This includes unexpected
  orphans instead of excluding them. No assertion, production authority, seed trust,
  admission, signature, lease, ATS or storage guard changes.
- Snapshot callers use isolated SQLite test_pool or the new required-fresh private
  PG fixture. Whole-family reads are not an owner-database or production operation.
- Independent review checked all16 table definitions in both dialects and all
  callers. SQLite typed-cell JSON and PG to_jsonb(row) compare same-backend before/
  after snapshots; no cross-dialect byte-equivalence claim is made.
- Frozen tests.rs SHA256d00abbdb4cd67d606ee328a0977329b56a07e2fcaf573b69473205efb8b6f9f0.
  Scope rustfmt/whitespace and independent static review clear; runtime pending.

## Failed-run cleanup and source evidence caveat

Exact root run.1793232521.22993.2021017080 removed; parent/index empty,37GiBfree.
No PG was started. No runtime remains live from89202. Its admitted774-source
manifest was83ea8ce0d6c1061f3b6ccefcd10d3924f2f7d24f9c9818a0b1ae660a1fdd647a
at HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/status6754505e.

Parent dispatched the test-only correction AFTER terminal but BEFORE the parent
post-run rehash. The resulting transient a0e24000 manifest is therefore NOT an
exact unchanged after-run pass. Read-only reconstruction from current774 entries,
replacing only tests.rs SHA with admitted bf0a534f3b9408ed57f3e90bed1b70b71bbf703b6887c73107bde463dc1a4004,
reproduces the exact83ea8ce0 manifest: all other source bytes match. Do not claim a
stronger runtime-final equality from that reconstruction. Future sequence is
terminal → post-run identity/cleanup → only then dispatch edits.

## Edge cases / how to test

Repeat the unchanged5 upfront FIX838 SQLite cases, genuine managed omission claim
and final-submit denial in both dialects,43 actual fresh-per-case PostgreSQL cases,
retained30 focused cases, strict lint and mandatory full no-PG all-target gate.
COMMAND-PHASE-638-FIX839-RUST-GATES.md changes only private run parent and source pin
from the reviewed FIX838 recipe; no selection/exclusion or predicate is relaxed.
Use the shared mac-build lock, canonical isolated Cargo/home/data and cleanup.
Re-run prospective-index privacy before dispatch and verify exact final identity
before any worker edits, even after a failure.

## Known limitations

Current source is unqualified until actual tests pass. No real employer/browser/
mail/provider/VM/Temporal/production/price/flag or owner DB authority follows.
