# Bluey AI recovery: next developer starts here

**Preservation handoff only. Source is incomplete, not mergeable, and not deployable.**
The owner authorized publishing this checkpoint so another developer can retrieve
it. That does not authorize this task to resume implementation or deploy.

## Get the checkpoint

Repository: <https://github.com/Dhanunjay-Divi/bluey>

Branch: `feat/phase-625-recovery-mainline`

In an existing clone, first check for local changes; do not overwrite them:

```sh
git status --short
git fetch origin feat/phase-625-recovery-mainline
git log -5 --oneline origin/feat/phase-625-recovery-mainline
```

Use a clean task-owned checkout of that remote branch. If the local branch is
absent, `git switch --track origin/feat/phase-625-recovery-mainline` creates it;
if it already exists, compare it before switching or updating. Do not reset an
existing developer's tree. On the owner's laptop, the prepared checkout is
`/Users/uno/.codex/worktrees/bluey-phase625-recovery/cue`.

## Read in this order

1. [Portable Bluey Ops](.agents/skills/bluey-ops/SKILL.md) and [AGENTS.md](AGENTS.md).
2. [Complete recovery handoff](docs/work/HANDOFF-PHASE-625-RECOVERY-20260921.md):
   exact inputs, blocked replay, missing files, open findings, CI failures,
   remaining ownership, and ordered recovery/merge/release gates.
3. [Implementation checkpoint](docs/work/IMPL-PHASE-625-RECOVERY.md) and
   [blocking review](docs/work/REVIEW-PHASE-625-RECOVERY.md).
4. [Round checklist](docs/work/BLUEY-AGENT-ROUND-CHECKLIST.md) and
   [round template](docs/work/TEMPLATE-ROUND.md). Record your actual assignment,
   file ownership, tests, remaining work, and handoff in each resumed/new round.

## What is saved, and what is not finished

- `512a94b1`: partial recovered product source, not the complete lost worktree.
- `ce371f8f`: portable skill, ownership/checklist/template and handoff links.
- `2e1d6976`: the previously uncommitted 36-line Windows account-state edit,
  saved separately as **unverified WIP**. It has no new build/test proof and
  is not yet entered in the replay journal. Inspect `git show 2e1d6976` first;
  do not assume it fully restores the missing predecessor.
- Later documentation commits update this starting point. Inspect current Git
  state; do not confuse a preservation commit with a completed implementation.
- No merge, release, deployment, production flag change, or Jobs implementation
  is part of this handoff publication. PR #38 still has unresolved findings.

## Important: Git does not contain the private recovery evidence

The public branch contains source and instructions, **not** raw Codex journals,
credentials, user data, extracted application bundles, or the private replay
corpus. On the owner's laptop, the reviewed corpus and journal are under
`/Users/uno/Downloads/cue/.git/bluey-recovery-20260921/`; the full handoff names
the exact inputs, hashes, and source journals.

A developer on another host must arrange an owner-approved private transfer of
the required evidence before exact recovery. A clone alone cannot reconstruct
the missing files. Do not upload those inputs to this public repository or
replace missing source evidence with guesses. Without the evidence, review the
checked-in checkpoint and report the recovery dependency instead of replaying.

## Instructions to give the next agent

> Read this file and the linked recovery handoff before writing. Once the owner
> assigns you to continue, finish evidence-based Phase 625 recovery first, then
> resolve the documented correctness and CI failures. Preserve other agents'
> work and coordinate shared server/auth changes. Bluey Jobs is outside scope:
> do not change its files, services, flags, data, or deployment. Keep source
> recovery, tests, review, merge, and release statuses separate. Merge only via
> a reviewed successor PR with passing required CI; deployment remains a separate
> exact-artifact Windows/macOS validation gate. If evidence, authority, or a
> required platform is missing, record the precise blocker and next action.
