# IMPL: Phase 626 — integration preprod preparation

Load `bluey-ops` and `pinky-ops`; verify current source before acting.

## Scope

Does: establish the separate integration environment contract, ordered tests,
code provenance and coordinated ownership; prepare Pinky's offline isolation
profile validator in its own feature branch.

Does not: provision a hostname, deploy, promote, import the old AI branch,
change Bluey runtime/pricing, touch Jobs or alter existing Pinky preprod.

## Files

- `docs/rounds/PHASE-626-PINKY-INTEGRATION-PREPROD.md`: durable shared plan.
- `CHANGELOG.md`: preparation scope.
- This implementation record and matching review: verification/limitations.

## Verification

Documentation checks and Pinky validator results are recorded in the matching
review after execution. Rust/Swift/Windows product builds are not claimed:
Bluey product code is unchanged. Full product CI and physical/live integration
remain required before merge/deploy of later runtime changes.

## Follow-ups

I1–I7 in the round document remain open. Obtain accepted Pinky runtime base and
real isolated resource identities before signing/deploy preparation. Continue
in the existing feature worktrees; do not create stale Downloads copies.
