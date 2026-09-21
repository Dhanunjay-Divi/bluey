# Bluey agent and round checklist

Load `$bluey-ops` from the branch's
[portable skill](../../.agents/skills/bluey-ops/SKILL.md), then read `AGENTS.md`.
This checklist applies to new rounds and updates to active rounds, not retroactive
rewrites of historical records. A handoff is not authorization to resume a paused task.

## Start and ownership

- Record actual checkout, branch, source SHA, dirty files, user scope, and non-goals.
- Read the latest relevant handoff and reconcile it with current source. List
  unknowns rather than assuming an earlier agent completed or deployed a feature.
- Inspect `docs/rounds/ROUND-*` and coordinate the next available round number
  with active owners. Do not overwrite another round or edit frozen `docs/reviews/`.
- In each active round, identify the coordinator and each assigned agent/task,
  exact owned files or modules, dependencies, current status, and evidence link.
  Write **unassigned** for a future owner; never invent an active agent.
- Give a delegated agent the actual checkout, portable skill path, scope,
  acceptance criteria, ownership, non-goals, and handoff destination. Tell it
  other agents may be working and not to revert their edits. Shared-file changes
  need coordination before writing.

## Required round record

Use [TEMPLATE-ROUND.md](TEMPLATE-ROUND.md) and link the corresponding
[IMPL](TEMPLATE-IMPL.md), [REVIEW](TEMPLATE-REVIEW.md), and bug-specific
[FIX](TEMPLATE-FIX.md) records. Update `CHANGELOG.md` for the PR.

Each round must contain:

- Objective and measurable acceptance criteria; included and excluded requests.
- Ownership table and source/provenance references for significant conclusions.
- Actual changes and affected files, not only a proposed design.
- Exact verification commands, source SHA, platform/artifact, outcomes, and
  unrun gates. Label historical results and inference separately.
- Every unresolved P0/P1/P2 item (or other established priority), its next owner,
  bounded action, dependency/blocker, and completion evidence required. Never
  drop an item just because the current round ends.
- Separate states for source checkpoint, implementation, tests, review, PR,
  merge, release artifact, and production deployment. Use **not done** or
  **unverified** where appropriate; a local commit is not a merge or deployment.
- Next-agent starting checkout/branch, first safe action, relevant files, exact
  stop conditions, and links to remaining work. Document local-only evidence and
  any private-transfer requirement without committing sensitive evidence itself.

## Validation and handoff

- For implementation, use the relevant test/release runbooks and isolated
  `scripts/run-bluey-tests.sh` launcher. Check its self-test and owned-root cleanup
  before heavy local validation; if it fails, record the blocker and fix/review
  that boundary before resuming heavy tests. Do not create stale Downloads trees.
- For documentation-only work, use proportionate documentation/skill checks;
  do not represent skipped product tests as passes.
- Preserve other agents' work. For AI-only batches, leave Jobs source, services,
  flags, and data unchanged; coordinate shared server/auth impacts explicitly.
- Before stopping, list committed and uncommitted work separately, with owner
  and next action. Never use bulk staging to make unrelated changes look finished.
- Update the active handoff and round with review blockers and remaining tasks.
  A successor must read these before implementation. Do not automatically resume
  stopped agents or send them new work when the owner has asked to stop.
- Before an authorized merge/deploy, require actual green gates and review;
  link the PR, merge SHA, exact artifact hashes, smoke evidence, and rollback
  evidence when applicable. Do not deploy other agents' unreviewed work by assumption.
