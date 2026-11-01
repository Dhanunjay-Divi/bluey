# FIX-842: Final-submit source guard matched the SQLite branch first

Preflight: bluey-ops/current AGENTS/handoff, scoped security and existing native
fixture worker/independent reviewer. Test-only correction; no production change.

## Issue / actual evidence

Run56416 TERMINAL1. Library1766passed/1failed/0ignored in2882.19s; later all-target
binaries/integration/examples not reached. Captured panic tests.rs883:13:
cloud final-submit admission performs work before prelock.
Failing test: db::jobs::tests::postgres_operational_hold_lock_is_first_in_every_protected_admission.
The earlier35 focused cases and43 actual isolatedPG cases passed. Those bounded
passes do not turn a failed full gate into source/release acceptance.

## Root cause

The lexical assertion used .find on the whole SQLite+PG cloud-submit operation.
FIX838's correct inner-Some selection introduced the exact marker
if let Some((Some(input), _)) = managed_cloud_context in the SQLite branch first.
The helper then scanned SQLite's statements before reaching PG's prelock and
correctly found semicolons in that WRONG slice. Its source region was unscoped.

Actual PostgreSQL code still starts the managed fresh path with the managed-cloud
admission prelock; input-absent path starts hold→registry→ATS. Beta/discovery and
unmanaged classification follow the existing locks; the classifier adds plain
reads, not a new lock class. Independent diagnosis and captured panic agree on
the test boundary. No production guard/order/replay behavior is changed to pass.

## Fix summary / files modified

- tests.rs guard caller only: move the already-existing cloud_submit_postgres
  extraction before the managed-prelock assertion and pass that scoped substring.
- Keep every exact marker, semicolon/forbidden-work scan, first-lock order and
  durable click-started replay-before-hold assertion. Missing PG marker still
  fails; the helper is not relaxed and cannot validate the SQLite marker instead.
- Production code, authenticated unmanaged fix and separate-admin fixture stay
  unchanged. Frozen tests SHA cca8f7aa8fa16ae98a84c5a7e5a23d638375e7802f0ee04a4ac684905cf7bb72.
  Scoped rustfmt/whitespace and independent exact source/command review clear;
  runtime pending.

## Edge cases / how to test

An identical marker in a different backend must not redirect a PG source guard.
Run the failed exact guard FIRST under singleton discovery before the old5
upfront cases and expensive PG/full stages. Retain all strict checks and all43
actualPG cases;36focusedtotal and fullalltargets must be observed on new bytes.
COMMAND-PHASE-638-FIX842-RUST-GATES.md contains the complete separate recipe.

## Failed source / cleanup

Parent FINAL rehash BEFORE authorizing edits matched exact774-source
50f7a7f78cc2a96d5664580833f5546ced067993e5d4ac146062ea61f2dc0df4,
HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/status6754505e.
PG had already cleanly stopped/PIDfile absence passed before SQLite. Exact root
run.1793234488.36773.2708630970 absent, parent/index empty,37GiBfree. No runtime
remains live from56416. Do not poll, restart or relabel that failed run as green.

## Known limitations

New current source remains unqualified until focused/actualPG/full gates pass.
The prepared API command's50f7 pin is obsolete; it needs current-source full0,
exact four FIX831 passes/final identity/cleanup, new pin/privacy/review before run.
Browser/native/cloud/provider/OAuth/cost/latency/release remain open. No external
write, owner DB, production deploy/flag/price or real application/email authority.
