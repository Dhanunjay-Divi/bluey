# IMPL: Phase 631 — Jobs candidate journey qualification

Preflight: `bluey-ops` and Round 631. Base: `f4191d09`.
Status: bounded local qualification complete; corrected runtime 5/5 passed.
No production release approval.

## Scope and files

- Actual browser child: full manual onboarding/search policy/Track persistence,
  account isolation and core-view disabled-effect behavior.
- Launcher and pure report validator/tests: exact five-case versioned evidence,
  private report metadata, distinct clean absent API processes, bounded counters.
- Durable Jobs completion ledger and this round's evidence/review records.

No product boundary is relaxed. Current code/local handoffs are authority;
no SSD fallback or competitor private session is used. Jobs remains isolated
from desktop/overlay/Pinky and production effects.

## Initial observed checks

- New report boundary tests: 4/4 passed, zero skips/failures.
- Launcher syntax and `git diff --check`: passed.
- Normal `npm run test:nonprod-reports --prefix jobs`: 6/6 passed; includes
  four new browser-report cases and the two existing Temporal-report cases.
- CI guard self-tests, provenance/license inventory (663 lock entries, 631
  unique versions, 14 pinned references) and schema parity (113 tables,
  91 indexes): passed. No workflow configuration changed.
- Canonical test launcher self-test: passed, including its intentional
  retained-process negative and residue checks.
- Runtime and independent review results are recorded below.

## Review refinements before runtime acceptance

Parent review corrected premature profile-reload counting, a stale post-finish
Work-history assertion, duplicate case-counting, unenrolled-account gate denial
masquerading as tenant isolation, toggle-based checkbox setup and insufficient
persisted policy assertions. Independent review then required a complete
candidate-state comparison across the second account's actual mutation and
explicit no-application/no-runner/no-authorization and visible blocked-state
checks. These are test-strength corrections, not weakened product guards.

The launcher hashes both the current portal sources and qualification harness
files and validates private report ownership/mode as well as its closed schema.
Pure report checks are now part of normal `npm test --prefix jobs`, not an
unlisted manual-only test command.

## Next stitched packet boundary

Read-only follow-on analysis identified a safe next slice: a feature-gated Rust
test helper can install existing signed positive synthetic source/ATS/integrity
authority in the same disposable PG; the normal default-feature API can then
prepare a Review-first packet. No source-resume asset means Track queue review
must still block approval before reservation/metering. The shared generated
Jobs data key and exact owned database must match across processes. Existing
fixture trust is persisted, not process-local. This bridge is not implemented
or runtime-proven in Phase 631; it must never become a public seeding endpoint
or a production fixture key/artifact.

## Baseline versus new proof

Phase 630 tests are retained baseline evidence, not a rerun. Only checks
explicitly recorded below may be reported as new Phase 631 results.

## First actual browser attempt — failed, not qualification

The fresh default-feature Rust binaries compiled in 82 seconds and the actual
private browser/API/PG path passed profile reload, complete onboarding, exact
policy/Track reload and the initial two-account isolation checks. It then
failed at `view_automation`; aggregate exit 1. PostgreSQL exited cleanly and
the exact root was removed, leaving its task parent empty.

The cause was a test expectation, not an observed product defect: the text
`No applications ready` exists only in the unopened queue dialog. A free,
disabled account cannot open that dialog. The corrected test requires the
visible locked card to show the real API reason, asserts no Start action or
active run, and requires actual runner/distribution/auto-submit availability
false. Settings checks the disabled control without attempting a click. Fresh
API reads after navigation must still contain no application/run/intervention
or authorization. A separate review refinement now also requires the second
account to have empty employment/role/location data, not merely another name.
No product UI or authority behavior was changed to make this test pass.

## Corrected actual runtime — passed

Fresh default-feature Rust binaries compiled in 82 seconds. Actual private
PostgreSQL 17.10/vector 0.8.3, Chromium, production Vite output and both real
HTTP APIs passed all five ordered cases:

1. partial profile save and Work-history reload;
2. all six onboarding screens and Matches navigation;
3. reloaded exact employment, target role/location, Full-time and W-2/C2C
   preferences, Review-first policy and persisted Track;
4. unauthenticated 401, second enrolled account's isolated reads/write readback
   and unchanged first-account profile/preferences/Tracks/applications; and
5. visible empty Applications, honest locked Automation reason, no start/active
   run, missing source/policy authority, disabled Auto-submit and unchanged
   empty effect families after navigation.

The complete v2 report had 5 tests/5 passes, no failures, 32 Jobs and 6 account
requests. Both API PIDs (371, 441) exited code 0/signal null and the outer
launcher required ESRCH. Source and harness fingerprints were unchanged.
PostgreSQL reported status 0, shutdown attempted and server exited. The canonical
root was removed and both Phase 631 runtime parents were empty; free space
returned to about 39 GiB. No private test browser/API/PG process or preview
was intentionally retained.

Terminal session: `45745`, wrapper exit 0. Exact removed root:
`/private/tmp/bluey-test-runs-phase631-browser-final-20261027/run.1793133823.97566.27399285530`.
First failed attempt was session `38243`; its removed root is
`/private/tmp/bluey-test-runs-phase631-browser-20261027/run.1793133649.94022.28394165050`.

The existing portal chunk-size warning remains visible; no threshold changed.
Full workspace 2,039-test/typecheck/build and full Rust 1,838-test/Clippy evidence
are unchanged-source baselines from Phases 630/629, not new reruns here. This
round freshly built/executed the Rust binaries and rebuilt the portal, but
changed no Rust, SQL, product portal, host/overlay, native, root web or workflow
configuration source. Review acceptance is only for this bounded local batch.
