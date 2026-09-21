# docs/work/ — Work Documentation Index

> **Codex preflight:** Load the branch-local
> [$bluey-ops](../../.agents/skills/bluey-ops/SKILL.md) before starting or reviewing
> a Bluey work batch. Follow the [agent round checklist](BLUEY-AGENT-ROUND-CHECKLIST.md)
> for ownership, remaining tasks, evidence, and next-agent instructions.

This directory contains implementation records, review reports, and bug fix docs for every batch of work.

## What Lives Here

| Pattern | Purpose | Template |
|---------|---------|----------|
| `IMPL-*.md` | Implementation doc per task batch | `TEMPLATE-IMPL.md` |
| `REVIEW-*.md` | Review report per task batch | `TEMPLATE-REVIEW.md` |
| `FIX-*.md` | Bug fix documentation | `TEMPLATE-FIX.md` |
| `../rounds/ROUND-*.md` | Round ownership, evidence, delivery state, and handoff | `TEMPLATE-ROUND.md` |

## Naming Conventions

- `IMPL-PHASE-X-BATCH-NAME.md` — e.g. `IMPL-WORKFLOW-DOCS.md`
- `REVIEW-PHASE-X-BATCH-NAME.md` — e.g. `REVIEW-WORKFLOW-DOCS.md`
- `FIX-NNN-short-slug.md` — e.g. `FIX-001-audio-dropout.md`

## Workflow

1. **Implement** — Developer/agent completes a task batch.
2. **Write IMPL doc** — Fill `TEMPLATE-IMPL.md` with what was done, files touched, deviations.
3. **Submit for review** — Open PR with IMPL doc included.
4. **Reviewer fills REVIEW doc** — Using `TEMPLATE-REVIEW.md`, per-task verdicts + overall.
5. **Iterate if needed** — Address 🔴 blockers, re-review.
6. **Merge** — Squash-merge into target branch.

## Rules

- Every PR must have a corresponding `IMPL-*.md`.
- Every review pass produces a `REVIEW-*.md`.
- Bug fixes get a `FIX-*.md` in addition to the commit.
- These docs are never deleted — they form the project audit trail.
