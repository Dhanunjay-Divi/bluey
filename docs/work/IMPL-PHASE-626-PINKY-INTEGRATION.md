# IMPL: Phase 626 — integration preprod preparation

Load `bluey-ops` and `pinky-ops`; verify current source before acting.

## Scope

Does: establish the separate integration environment contract, ordered tests,
code provenance and coordinated ownership; prepare Pinky's offline isolation
profile validator in its own feature branch.

Does not: provision a hostname, deploy, promote, import the old AI branch,
change Bluey runtime/pricing, touch Jobs or alter existing Pinky preprod.

### 2026-10-09 scope/operations extension

Owner clarified that Pinky owns the customer AI UI/sign-in; standalone Bluey
UI is deferred and the old AI branch is reference only. Added the shared
integration runbook, Git-backed `pinky-bluey-integration-ops` skill and handoff
entry points in both repos. Local `bluey-ops`/`pinky-ops` route to that skill.

Owner approved one $7/month DigitalOcean VM with no paid addons. It was
provisioned separately, and its host key was independently read through the
authenticated provider console before strict SSH. The runbook records actual
identity/capacity; application resources/DNS/TLS/runtime are not deployed.
Package-cache cleanup was separately authorized on the existing full Bluey
host; retained backups, services, flags and Jobs data were left unchanged.
The original no-provision/no-cleanup statement above describes the 2026-10-08
preparation slice, not these later owner-authorized actions.

## Files

- `docs/rounds/PHASE-626-PINKY-INTEGRATION-PREPROD.md`: durable shared plan.
- `CHANGELOG.md`: preparation scope.
- This implementation record and matching review: verification/limitations.
- `docs/ops/PINKY-BLUEY-INTEGRATION-RUNBOOK.md`: current operations and evidence.
- `docs/skills/pinky-bluey-integration-ops/SKILL.md`: versioned integration memory.
- `AGENT-HANDOFF.md`: scoped current entry point before historical snapshots.

## Verification

Documentation checks and Pinky validator results are recorded in the matching
review after execution. Rust/Swift/Windows product builds are not claimed:
Bluey product code is unchanged. Full product CI and physical/live integration
remain required before merge/deploy of later runtime changes.

## Follow-ups

I1–I7 in the round document remain open. Obtain accepted Pinky runtime base and
real isolated resource identities before signing/deploy preparation. Continue
in the existing feature worktrees; do not create stale Downloads copies.
