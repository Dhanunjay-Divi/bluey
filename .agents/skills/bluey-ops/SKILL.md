---
name: bluey-ops
description: Repository-local operating guidance for Bluey/Cue desktop, server, Jobs, testing, release, and agent handoffs. Use for work in this repository or on bluey.sh.
---

# Bluey Ops

This portable project skill travels with the branch. It does not require a
developer's personal skill directory. Resolve the paths below from the Git
repository root containing this skill, not from a hardcoded local checkout.

## Preflight

1. Read `AGENTS.md`, `AGENT-HANDOFF.md`, and the task-specific handoff or round.
2. Check the actual checkout, branch, HEAD, and dirty files before editing.
   Historical docs and personal skill memories are not current Git or production
   evidence. Preserve unrelated changes and confirm ownership of overlapping files.
3. Follow `docs/work/BLUEY-AGENT-ROUND-CHECKLIST.md`. Every active implementation
   round needs explicit ownership, acceptance criteria, verification, remaining
   work, and a next-agent handoff. Use `docs/work/TEMPLATE-ROUND.md` for new rounds.
4. Respect the latest user scope and stop instruction. Reading a backlog or
   handoff never authorizes resuming work, spending credits, deploying, or
   changing another agent's workstream.

## Product and authority boundaries

- Bluey AI is the native desktop assistant; its CLI is the launch/support surface.
  Preserve the compact pill and validate expanded UI, shortcuts, light/dark mode,
  streaming, and sign-in on the actual shipped macOS and Windows surfaces.
- Keep provider secrets server-side. Do not add credential-store prompts as a
  side effect of UI or diagnostics work. Never ship
  `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` or promise undetectability.
- Diagnostics and user-content storage are different consent boundaries. Never
  log raw prompts, transcripts, answers, credentials, or private recovery journals
  into operational telemetry or the public repository by default.
- Bluey Jobs is a separate workstream. AI-only work must not change Jobs files,
  data, services, flags, or deployments. Shared server/auth/storage changes require
  explicit impact review and coordination, not an assumption of isolation.
- For authorized Jobs work, read its newest relevant rounds and architecture.
  Preserve identity/resume/evidence revision binding, server-owned eligibility,
  original-source verification, and review/submit authority. Unknown submit
  outcomes must not be automatically retried. Local execution cannot run while
  the computer is off; UI availability is not permission to enable automation.

## Verification and release

Read the relevant sections of `docs/TESTING-RUNBOOK.md` for validation and
`docs/RELEASE-RUNBOOK.md` before release work; use `docs/MODEL-ROUTING.md` for
provider/routing changes. Do not load unrelated historical records as instructions.

- Use `scripts/run-bluey-tests.sh` for isolated local Rust tests and heavy
  non-release Cargo checks. Verify its lightweight self-test before heavy work;
  a cleanup failure is a blocker, not permission to create unbounded targets.
  Never aim cleanup at release artifacts, shared caches, or another task's files.
- Record actual test commands, platforms, source SHA, results, and missing gates.
  Reconstructed source needs fresh tests; historical passes are not certification.
- Follow the feature-branch/PR process in `AGENTS.md`; no direct main push.
  Build once, smoke the exact artifacts, then promote unchanged. Cross-compilation
  is not physical Windows/macOS runtime validation. Deployment requires its own
  authorization and evidence, not just a merge.
- Check `git diff --check` and `bash scripts/check-bluey-ops-docs.sh` for skill
  or workflow documentation changes. Keep frozen `docs/reviews/` untouched.

Keep durable rules here and dated state in handoffs/rounds. Do not copy private
host details, credentials, binary extracts, or raw session journals into this skill.
