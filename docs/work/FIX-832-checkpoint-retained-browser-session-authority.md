# FIX-832: Checkpoint recovery still assumes session ID equals execution run ID

Preflight:bluey-ops; current Phase638 source/local handoff, independent review.

## Issue

The ordinary cloud queue uses a distinct execution run and reusable browser
session. Active claims now resolve that binding, but checkpoint reconciliation
still reads session.id=run_id. This is a source-supported availability defect;
the new ordinary private-PG causal case is not yet executed at creation.

## Root Cause

reconcile_execution_lease_checkpoint selects jobs_browser_sessions by run ID in
both databases and updates that same predicate. checkpoint_reconciliation_plan
and exact_checkpoint_recovery_replay also require browser_session.id=run_id.
Existing checkpoint tests mostly construct aligned IDs, hiding the ordinary seam.
PostgreSQL additionally requires a session before its trusted-submitted early
return; SQLite checks the retained submitted application/evidence first.

## Fix Summary

Reproduction only; no production correction yet. Add separate explicit
--expect-checkpoint-session-missing private scenario using ordinary review-first
profile/source/packet approval/cloud queue and the real signed lease client.
Require claim200/exact lease+volume/no session alias, observed valid heartbeat,
then stop heartbeat and call real reconcileCheckpoint with exact v2/prepared
owner/token/fence. Only recovery/request_failed404 with the exact public response
and unchanged20-family snapshot qualifies. Prove original lease still heartbeats,
then safe release/fresh terminal replay, zero effects and complete cleanup.

The PostgreSQL missing-session lookup precedes validate_checkpoint_lease_access;
therefore preceding observed heartbeat is essential evidence of valid current
credentials and volume identity. This test does not claim validation order changed.
Do not infer the cause from any other404 or count400/409/500 as this red.

## Files Modified

- Private Node controller/report validator, tests and launcher (three files).
- Round638 implementation/review/handoff and this FIX/command record.
- No production Rust, schema or legacy compatibility change in reproduction.

## Edge Cases Handled

Strict separate scenario rejects absent claim/heartbeat, wrong error/body, missing
request/fresh nonce/exact credentials, v1/irreversible checkpoint, state mutation,
unusable original lease, capacity/canary/receipt authority, cleanup failure and
private report fields. It remains sourceAcceptedfalse even if expected-red exits0.

## How to Test

156 Node report tests pass, zero skips, including23 new recovery report cases.
Node syntax checks pass. Run COMMAND-PHASE-638-CHECKPOINT-SESSION-RED.md only after
independent source review and prospective-index privacy/whitespace freeze.

## Known Limitations

Not yet actual runtime proof, not a fix. No employer submission, email, browser,
Temporal/VM, production/owner database, flag or pricing change is authorized.
The case covers active prepared recovery only, not submitted/unknown/expired/
takeover recovery or current release readiness. Keep all broader gates open.

## Candidate retained-authority design, not implementation approval

Both modes retain managed_cloud_execution_command_id on the lease. Existing
managed admission maps RESUME to the original START binding, while storing the
request command separately. Immutable START payload freezes session/profile/
identity/packet; command parser can validate row/envelope without live heads.
Prefer an exact historical resolver from the lease's complete retained authority,
not any current START, latest session, derived alias or ATS-only fallback.

Validate canonical stored managed authority against run/fence/token hash, exact
request and execution START chains plus request-start events. Recognized terminal
command states must not invalidate immutable history. Pass retained session into
plan/replay/select/update. Prevent old-run mutation of a reused session by requiring
application's exact run still equals checkpoint run. Trusted submitted recovery
must return using retained lease/application/evidence without requiring a session,
with SQLite/PG parity. Final DB-time/account/volume checks and lock order remain.
All-null legacy leases need an explicit separately reviewed compatibility rule;
partial managed metadata must fail closed, not fall back. No migration is yet
proposed. Actual causal proof and independent design review must precede any fix.

## Actual causal red — 29416

Canonical shared-mac-build/private-PG run29416 TERMINAL0 confirmed the intended
availability defect, not a fix. Build1m40s. Real ordinary review-first claim200
and successful heartbeat preceded exact original v2/prepared real-client recovery;
it returned404/recovery/request_failed with exact public response. All20 protected
families stayed unchanged. Fresh separately signed original-token heartbeat200
extended the same prepared lease; safe release204/fresh terminal replay204 passed.
No ATS/effect/capacity/canary/irreversible receipt authority; one owned gateway503/
no second>=11s was not Temporal acceptance.3 children/PIDs/listeners, API/PG/root
cleanup passed;parentempty/38GiBfree. No test process remains live.

774-source manifest79f252d90da5b58ea027f379d4727941ad2284f0b6ea7e7e54778f99fd042463,
HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/scopedstatus
6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5 matched
before/after. API SHA9a1619d550d93004eb53c877e445dc7c17b5b633cad84514daa622346c3a5c94;
fixture SHAa463ecc88b9f2cf1c89f5a4cfb7388e6bf897d1ba7758bd594d1c1ef269c8c2b.
Pre-run privacy2925tracked/2649text passed tree77442c97688256b8b66ca4f861ee40aaa08488c7;
156 Node tests pass. Source remains unaccepted; no production correction yet.

## Reviewed implementation direction

Independent source review confirmed no migration is needed. Complete managed lease
authority already pins its exact request and original START binding by foreign keys,
including after RESUME. Command authority/payload/HMAC fields, bindings, request-
start authority and event rows are immutable; authorized account deletion is fenced.
Reject partial metadata. For all-null legacy, reuse existing require_unmanaged_cloud_
execution_* classifier (marker OR binding OR request-start history), never merely
absence of a binding. Only explicitly unmanaged legacy retains its aligned-ID rule.

After locking the exact lease, use plain MVCC historical reads, ideally one joined
read or final assembled-result revalidation. Do NOT acquire command/binding SHARE
or UPDATE locks after the lease: that inverts active claim/effect lock order.
The surviving locked lease/FKs pin referenced history; mutable delivery state is
irrelevant. SQLite IMMEDIATE already serializes writers. Canonically validate the
complete retained metadata against exact run/fence/token hash, original START and
request chain/events, frozen application/identity/profile/session; no current head,
admission, verified-identity selection or time-dependent release check.

Pass retained session through both plan and exact replay and the SQL read/update.
Require application's exact run before mutating a reused session. PostgreSQL trusted
submitted recovery must validate final DB-time volume, lease owner/token/fence and
retained application/evidence before returning WITHOUT any history/session dependency.
For mutation, keep existing lease/session/capacity/final-time rechecks; add no lock
class. Missing historical decryption key/material fails closed. Retention/key-rotation,
complete/partial/all-null compatibility, stale/cross-run, terminal command/head changes,
submitted parity and SQLite/PG/ordinary real-client green proof remain required.
This direction authorizes bounded local implementation only, not release or effects.

## Implementation checkpoint, not runtime acceptance

Backend correction is frozen in execution_session_authority.rs, execution_leases.rs
and execution_session_authority_tests.rs. Historical plain reads validate exact
START/RESUME chain, bindings/request-start events and canonical12-field lease
digest; application comparison uses the existing four-field transport normalization,
not a new trust policy. Partial/legacy handling and submitted/mutating ordering
follow the reviewed direction. Expected focused count18 (13pure/5SQLite), seven
new tests; NOT executed yet. Submitted-absence coverage is structural ordering,
not a fabricated submitted DB receipt or actual PostgreSQL parity proof.

Two private fixture/example files implement the approved REPEATABLE READ READ ONLY
before/after/replay inspector for both modes, exact8-field requests/20-field results,
canonical stable application/session hashes and closed7-field recovery summaries.
Five focused fixture/example tests are implemented but unrun.

Parent added separate review-first and auto-submit real-client green scenarios:
recover200 and fresh exact recovery replay200, typed before/after/replay comparison,
unchanged16 other families/seven immutable families/stable attempt+lease authority,
safe terminal finish204 and fresh finish replay, zero effects and6/7 children.
Independent review found a mode-binding false-green; FIX833 binds expected mode
to argv at both acceptance boundaries.186 Node tests pass. No actual corrected
recovery/API, full-source or release evidence follows until review and runtime.

## Actual corrected dual-mode prepared recovery

After FIX834 test-fixture repair,77890 passed23focused Rust tests (13pure/5SQLite/
3fixture/2example), then exposed PG Boolean typing defect FIX835. Its one-expression
correction was source/prepare-probed and independently reviewed. Build-only actual
review-first86077 and auto20772 subsequently both exited0 on current source.

Review-first build1m27s; auto build1m24s. Both ordinary API/real clients completed
claim/observed heartbeat, stopped heartbeat, READONLY typed before inspection,
actual prepared recovery200 to failedapplication/session and releasedlease/attempt,
stable canonical packet/application/session hashes, exact closed recovery marker,
unchanged16 other families/sevenimmutables/stableattempt+completeleaseauthority,
fresh actual recovery replay200 with unchanged20families+typed state, real terminal
finish204 with unchanged20families and fresh separately signed finish204 replay.
No employer effect/capacity/canary/irreversible receipt. Review-first requires
zeroATS/6cleanchildren; auto requires ordinary Track consent, production-decoded
initialPhaseA+wholeATSrowunchanged and7cleanchildren. Every check passed the inner
and independent outer mode-bound strict report validators; sourceAcceptedfalse.

Both API/process/PID/listener/PG/root cleanup passed, parents empty/38GiBfree and
realindexempty.774-source manifestd66a712f17e2cbced73fc1b2006836e771cc8abe7c5e5c2db2760861d932cb5d,
HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/scopedstatus
6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5 matched
before/after both. Review-first API SHA2cd9dc12646c878dca93119a26afb42e555638d44081b436f2919bb63a7b8cb8;
fixture SHA f33428b266eb5f4756ade7b63c81f6c63dbdb31e42049bffeb0e770739c617b0.
Auto API SHA80b6c0ce238cac8f25894c22cc01cfa57a18ac5ddd23d780cdc0a7ead28655d3;
fixture SHA b8ecbcba8c870aad3d6c52cfde5f97e3439a7bb889167e9b4b09897ea0518340.
187 Node tests pass; pre-run privacy2929tracked/2653text passed
tree4968586bdbf7aa02819c45f9b9dacd582b769041.

This closes bounded prepared recovery, NOT full-source/release/overall goal.
Unsafe/unknown/submitted actual-PG recovery/parity, key rotation/retention,
policy/head/terminal-command/concurrency/session reuse negatives, current-source
prepared-repeat preservation, full server/Jobs/Clippy/native/cloud/provider/receipt
and cost/latency/economics gates remain open. No production, browser or employer
action occurred. Historical failure records above remain failures at their source.
