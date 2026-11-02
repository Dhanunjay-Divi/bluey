# Round642 — Bounded Jobs CI remediation

Date:2026-11-01. Base32ca96dbfbe151b1ca0294ad945ce58a4276087f.
Branch `feat/phase-642-jobs-ci-remediation` in the managed CI-remediation checkout.

## Scope / authority

Jobs only. The owner authorized feature pushes and one additional three-workflow
CI batch; PR39 triggered that exact batch. Its Jobs workflow failed Darwin strict
Clippy and Ubuntu dependency audit. Repair only proven causes, with source,
license/lock inventory, focused and full local tests plus independent review.
No suppression, audit threshold downgrade, force-upgrade-all or fabricated pass.

Use a separate checkout because Phase639 has independently bound image evidence and
Phase641 contains an uncommitted startup-fixture draft. This is a sibling
remediation from the failing CI head, not a merge/cherry-pick of unrelated work.
Do not touch desktop/overlay/Pinky or meeting-owned checkout working files.

## Ordered work / acceptance

1. Correct two native-storage test assertions to the current Clippy requirement
   without changing tested emptiness, native implementation or lint policy.
2. Read actual minimatch advisory and dependency graph; choose a compatible,
   minimal pinned remediation. Legacy callable imports forbid a blind major
   override. Any necessary parent-package migration requires exercised API,
   archive/unpack, glob/filter and package-contract regressions.
3. Use the canonical launcher/private HOME/cache plus shared heavy lock for
   install/compile/test work after the existing image run completes. No inherited
   provider credentials, owner DB, native signing or publication.
4. Require fresh lock install/audit, provenance inventory, formatting/Clippy/native
   tests and proportionate full Jobs/build/release guards. All source/input hashes
   and actual cleanup must be recorded. Do not credit current working-tree passes
   as CI on another SHA.
5. Source review and local qualification precede any commit/push. Updating PR39
   or opening a new PR would trigger another batch and requires a new allowance;
   the one approved batch is not permission for retries. No merge, deploy,
   publication, production flag or application/email action.

## Owner scope decision

The owner selected Web/cloud Jobs first and isolation of native packaging tools.
Keep the root moderate audit unchanged. Remove the optional release SDK from the
Web/cloud workspace graph and inventory its private manifest/lock separately.
Do not install or execute that SDK during its metadata audit. Native packaging
and all candidate/authorization/promotion paths must refuse deterministically,
including when a future metadata audit passes. This is not native availability.

Root release-contract scripts import asar themselves, so the root must declare
its own exact dependency rather than rely on workspace hoisting. Modern asar's
named async API and glob/minimatch compatibility must be exercised before any
local acceptance. Root overrides alone are not proof of the resolved graph.

## Current evidence

PR39 head32ca/base2b85339a,22 changed files, draft. Jobs run41367721820 failed:
Darwin125031348022 reports `assert_is_empty` at native tests65/198; Ubuntu
125031348272 reports15 moderate minimatch-related audit findings. This is source/
dependency failure, not runner acquisition or billing. The other two workflow
results remain separate: the shared workspace has STT Clippy and IPC capability
race-test failures outside this Jobs scope. Actual D1 is development evidence,
not release approval; its latest attempt failed on unsupported tar type `x`,
with normal cleanup and preserved diagnostics. No image acceptance is inferred;
any future candidate must contain the dependency corrections and be requalified.
