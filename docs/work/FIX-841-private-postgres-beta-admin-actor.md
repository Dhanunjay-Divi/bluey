# FIX-841: Private PostgreSQL fixture used candidate as audited cohort admin

Preflight: bluey-ops/current AGENTS/local handoff; security acceptance and bounded
existing native fixture worker/independent reviewer. Test-only correction.

## Issue / root cause

Run62964 TERMINAL1. Format/strictbothfeaturealltargetClippy passed1m59s;
SQLite testbuild2m37/PG testbuild2m41. Exact singleton discovery succeeded for all5
upfront cases; all5 plus7retainedpreflights passed. Actual private PostgreSQL
matrix43=41passed/2failed. Only two new FIX838 cases failed at tests.rs20885,
AdminActorRequired, before their claim/submit assertions. Later23focus/full not
reached. Existing41 include source, identity, locking, capacity, deletion and
reconciliation preservation; their pass is not acceptance of the two failed cases.

postgres_authenticated_worker_pool_or_skip created/verifies a NON-admin candidate
but passed that candidate ID to update_public_beta_cohort_audited. The production
guard correctly requires a current non-deleting admin actor. Verification is not
admin authority. This is a fixture prerequisite error, not a guard to relax.

## Fix summary / files modified

- tests.rs helper only: keep fixed candidate creation/non-admin role unchanged;
  assert ordinary Account::fetch_by_id returns is_admin=false before/after setup.
- Create a separate unique @example.test synthetic admin using existing
  Account::create_with_admin(...,true), then ordinary mark_email_verified. Pass
  only admin.id to audited cohort update; no role/reset SQL or candidate promotion.
- Enroll only acct-jobs, and bind its identity/Track. Admin has no candidate Track/
  profile/enrollment. Preserve required-PG/fresh-DB guards and all test names/counts.
- Pattern matches jobs_beta_access.rs admin_account fixture and current admin-actor
  denial/revocation tests. Does not disable admin checks or alter production flags.
- Frozen tests SHA bc24abd478512f85c9c5c3e8040d24a519906e4843c66fd618445db5fc303a82.
  Scoped rustfmt/whitespace and independent exact review clear; runtime pending.

## Edge cases / how to test

Different actors cannot be conflated: candidate remains unprivileged; setup admin
is separate and private/fresh per case. Re-run both actualPG cases through claim/
start/replay and complete-managed omission/wrong-worker denials, preserving all16
snapshot families. Retain5SQLite regressions/7preflights/41existingPG/23focus/full
alltargets and strict lint in COMMAND-PHASE-638-FIX841-RUST-GATES.md. Prospective
privacy and review precede dispatch; no older-source pass qualifies changed bytes.

## Observed failed source and cleanup

Parent captured FINAL identity BEFORE authorizing edits: admitted774-source
d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46,
HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/status6754505e matched exact beforeafter.
PG wrapper status1 (failed cases) but clean serverExitedtrue shutdown; canonical
exact root run.1793233496.28456.294284640 absent, parent/index empty,37GiBfree.
No runtime remains live from62964. Do not poll or relabel that failed handle.

## Known limitations

Current56416 bounded evidence: fmt/strictClippy2m00,35focused cases and all43actual
freshDBPG cases PASS on774-source50f7a7f78cc2a96d5664580833f5546ced067993e5d4ac146062ea61f2dc0df4.
Both previously failing PG cases now run through their actual claim/start/replay
or managed-denial assertions. PG cleanstop/PIDabsence precedes fullSQLite. Parent
midrun source/HEAD/status match. Fullalltarget stage is still LIVE; no final
source/rootcleanup or full source acceptance is claimed yet.

Corrected current source remains unqualified overall until full acceptance. Wider
browser/native/cloud/provider/OAuth/cost/latency/release gates remain open. No real
application/email, production/deploy/price/flag or owner DB change is authorized.
