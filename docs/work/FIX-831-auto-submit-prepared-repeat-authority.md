# FIX-831: Auto-submit prepared repeats conflict with immutable ATS Phase A

## Issue

Preservation33250 subsequently passed on774-source
d66a712f17e2cbced73fc1b2006836e771cc8abe7c5e5c2db2760861d932cb5d
after FIX832/FIX835, terminal0/build1m24s. Exact source/HEAD/status matched before
and after. API SHA3765dfc6bb40590a8977bf3d3b385d20118c3e67de634b709b9a74a03dacb243;
fixture SHA113dbedc181dfbda96818b5e0bbcd5e3389c34b7fc2ee284489889f9a7361314.
Actual first200/repeat200/newtoken+higherfence/completeATSunchanged, oldcredential
heartbeat+safe-finish409/20familiesunchanged, currentheartbeat200/release204/fresh
terminalreplay204, zeroeffects and4children/API/PG/rootcleanup passed; parentempty,
38GiBfree. This is bounded current-source preservation, not full-source acceptance.

Round638 D0.3 requires prepared repeats to return a new token/higher fence and
reject the old authority. The qualified review-first80446 case had no ATS row,
so it could not establish this behavior for auto-submit. Independent source review
predicted a409 for the exact ordinary auto-submit repeat. Actual76811 now
confirms that contract failure under the intended preconditions. The subsequent
bounded correction and green qualification55819 are recorded below; earlier
reproduction-stage statements describe that historical checkpoint only.

## Root Cause

Each claim creates a fresh lease token/hash in execution_leases.rs and passes
that hash as the ATS PhaseA nonce. The context builder also proposes an expiry
from current DB time. Existing ATS bindings are found by application/attempt or
run, but their replay predicate requires the exact original nonce and requested
expiry. Consequently a repeat cannot reach generic prepared token/fence rotation.
A one-field nonce relaxation would not resolve the expiry or PhaseB contract.

## Fix Summary

Initial reproduction stage, before the selected correction and green result below:

- Preserve the passing review-first v1 fixture and its zero-ATS expectation.
- Add separate closed auto bootstrap/post_queue/claim-inspection operations,
  using existing synthetic signed authorities and the production ATS decoder.
- Use matching synthetic source/profile/posting facts that meet default80;
  authorize the Track and prepare auto_submit through ordinary APIs, without a
  review-first /approve call or a seeded authorization row.
- Prove first claim/one exact frozen ATS PhaseA, observed heartbeat, then stop
  heartbeat and replay current queue/readiness before a fresh-proof exact repeat.
- Label the expected409 as auto_submit_repeat_causal_red/sourceAcceptedfalse;
  prove20 protected families unchanged, original lease still heartbeats, safe
  release/fresh terminal replay and zero irreversible-effect authority.
- Only after causal evidence, review a complete ATS/lease stability or renewal
  model compatible with PhaseB and immutable history. Do not weaken equality,
  add a session alias, or call the rejected repeat a completed auto-submit path.

## Files Modified

- Debug/integration-only runner fixture and guarded example.
- Private Node controller, wrapper and strict report tests.
- Round638 implementation/review/handoff records.

## Edge Cases Handled

Reports distinguish first-claim rejection from the intended repeat failure and
reject missing consent, absent/corrupt/consumed ATS state, stale proof/nonce,
changed inputs, missing readiness, changed protected state, unusable retained
lease, incomplete cleanup, effects or private payload fields. The physical
private-root/PG/key checks and no-args fixture guard remain mandatory.

## How to Test

102 Node report tests pass, zero skipped, including65 existing cases. Six new pure
Rust fixture/example tests and the actual private auto-submit case are implemented
but unexecuted at creation. Use COMMAND-PHASE-638-AUTO-SUBMIT-REPEAT-RED.md only
after source review and complete prospective-index privacy/whitespace freeze.

Actual76811 subsequently exited0 as the expected causal-red reproduction. Rust
test build2m38s;5 module tests passed in0.01s;7 example tests passed in0.00s after
1.85s build. API/fixture build2.36s. Strict native/controller report validation
proved ordinary Track consent, schema3 factual auto preparation without /approve,
first managed claim200/one exact production-decoded PhaseA bound to the distinct
START session and first lease token hash, observed heartbeat, fresh readiness
replay and exact fresh-proof repeat409/lease_unavailable/public body fingerprint.
All20 protected families and frozen ATS bytes stayed unchanged. Original-token
fresh signed heartbeat200/extended expiry, release204 and fresh signed terminal
replay204 succeeded. Capacity/canary/irreversible-receipt and base effect counts
were observedzero; one owned gateway503/no repeat>=11s was not Temporal acceptance.

All4 recorded children exited0, PIDsabsent/listenersrefused; API/PG/root cleanup
passed, parentempty/38GiBfree.774-source manifest
085263571c034f05406bfb4dc28367e27e3722391ae83afa25a3511d3eeb3538
and HEAD/scoped status matched before/after. API SHA
af0f29fa650b458059ebc199ebe5c293873d42e57f7118e7af9cfb954f7d8c9c;
fixture SHA4253fac97ec875320ba0f43cb2ab140693427b1a70302b81d25fe8f7e80e5de2.
This is an observed repeat-contract defect, not an accepted auto-submit lifecycle.
The nonce/expiry conflict is the source-supported cause; no raw internal error
or private payload was disclosed. Compare complete renewal/stability designs
before changing production ATS or lease authority.

## Known Limitations

The bounded same-owner live prepared correction is qualified by55819 below; wider
retry/takeover/recovery and release gates remain open. This case does not click Submit,
start a browser/VM/Temporal engine, call an employer/provider, run model inference,
qualify native measurements, change production flags/pricing, or establish
source/release readiness. Retained checkpoint-session recovery is separately open.

## Selected bounded correction

DECISION-PHASE-638-PREPARED-ATS-RETRY.md records two bounded evidence-backed
alternatives and independent challenge. Stable original PhaseA reuse is selected
only for same-owner unexpired prepared managed-cloud retries, preserving normal
current authorization/runtime checks and exact raw replay. Live token/fence still
rotate; immutable nonce/requested expiry do not. Generic/local replay remains
strict, missing/expired/conflicting authority denies, no takeover is enabled.
No PhaseB/recovery/schema semantics change: they already read the stored ATS nonce
after separate live lease checks. Implementation is frozen in the two production
modules, with two new and two extended focused tests; runtime evidence follows.

The private marker is derived only from the already-read/locked existing lease;
SQLite hoists that read under IMMEDIATE, PostgreSQL keeps its existing final-time
locked read. A separate private context path requires exactly one canonical
unexpired preflight row (missing denies), preserves nonce/requested expiry and
reuses original target/consent/certification/runtime checks plus strict raw replay.
No caller-selected Boolean enables this path. Source-level formatting/diff checks
and independent static review pass; actual bounded qualification follows.

Parent added a separate --auto-submit-prepared-repeat green test contract rather
than relabeling the prior409.133 Node report tests pass, including31 green report
cases; these are validator tests, not actual API proof. The green orchestration
requires fresh real claim200 with new token/higher fence, whole ATS row equality,
exact current lease/volume/runtime, stable immutable/attempt/subject authority,
fresh signed stale-token heartbeat+safe-finish409 with unchanged20-family snapshot,
current heartbeat200, safe release204 and fresh terminal replay. Native-decoded
firstclaim authority stays bound to unchanged frozen bytes after retry. No PhaseB
or irreversible route is allowed in this controller. Actual private-PG and focused
production regressions follow independent review, not this document.

## Actual green qualification — 55819

Canonical isolated/shared mac-build/private PostgreSQL run55819 TERMINAL0.
All4 exact Rust regressions passed, zero ignored: pure managed-retry predicate,
canonical ATS record/runtime guards, unchanged generic nonce/expiry/authorization
strictness, and SQLite context PhaseA retry followed by its existing atomic
PhaseB/recovery. First test build2m59s; signed fixture tests2.18/2.13/1.91s;
API/fixture build2.78s. This last SQLite test is not an ordinary API irreversible
transition after an actually rotated live lease; that wider case remains open.

The real-client private-PG green report/wrapper both passed: normal Track consent
and factual auto preparation, first claim200/exact production-decoded PhaseA,
observed heartbeat, fresh readiness, fresh proof/transport nonce and exact-input
repeat200 with different token/strictly higher fence. Whole ATS row equality
retained frozen bytes, original nonce and requested/effective deadline. Exact live
lease/volume/current process runtime and two-generation runtime history, nine
immutable families/attempt binding/account subject stability passed. Fresh signed
stale heartbeat409 and stale safe finish409 each preserved20 protected families;
current heartbeat200 extended its own lease. Safe release204 and fresh signed
terminal replay204 preserved state. Base effects plus capacity/canary/irreversible
receipt counts werezero; one owned gateway503/no second>=11s was not Temporal
acceptance. All4 recorded children exited0/PIDsabsent/listenersrefused, API/PG/root
cleaned, parentempty/38GiBfree, realindexempty. No process remains live.

774-source manifesta970b9c46405079358550f3363d8568f9e3fc0df62b67a2198efb0840b8801fe
and HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/scopedstatus
6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5
matched before/after. API SHA06ce3f5b2f8fe7b5617d3f7bfc2734572175acb044872ae12067bc05058df040;
fixture SHA0d2ebc6f8b0dc9cb1ae9f07c77dd752e853d445d0df863f65c2933363416e7e4.
Pre-run prospective privacy2923tracked/2647text passed
tree1c90d3ea9a1281243f13bbfc1240d8d880a293b2;133 Node tests and launcher24516 passed.

No production/external-effect or full source acceptance follows. Current-source
review-first rerun, broader actual-PG negatives/concurrency/policy/session/recovery,
ordinary rotated-live-lease PhaseB, full server/Jobs/lint/native/cloud gates and
receipt-verified cost/latency/economics remain unqualified. Takeover/expired lease
or ATS renewal remains deliberately denied, not silently enabled by this correction.
