# Round 588 - Bluey historical worktree and branch reconciliation

Date: 2026-08-02

Status: complete; historical work is preserved in Git, reviewed launch-safe work
is already on main, and the meeting worktree remains untouched

## Objective

Resolve the remaining local-work ambiguity without bulk-merging an obsolete
experimental checkout into production. Preserve every non-ignored historical file
in Git, compare all Codex-owned remote branches with current main, and carry forward
only complete changes that remain valid against current authority and release gates.

This round changes documentation only. It does not rebuild or deploy the main API,
Jobs API, Jobs portal, Caddy configuration, discovery workers, or native artifacts.

## Source of truth

- Reconciliation base: `origin/main` at
  `bb6d68a8ebb8ae58ebf9ce74f2f5fb3315b1dd2a`
- Clean reconciliation worktree:
  `/private/tmp/bluey-historical-reconciliation`
- Preserved historical worktree: `/Users/uno/Downloads/cue-bluey-jobs`
- Historical snapshot branch:
  `codex/bluey-jobs-historical-snapshot-20260802`
- Historical snapshot commit:
  `888547f8`
- Meeting-owned worktree: `/Users/uno/Downloads/cue` on `meeting-main`

The meeting worktree was inspected only for ownership and status. No file, ref,
index entry, or working-tree state in that checkout was modified.

## Historical snapshot

The old Jobs checkout contained substantial non-ignored work in its uncommitted
state:

- 87 modified tracked files;
- 21 tracked deletions;
- 199 untracked path entries from Git's recursive untracked-file listing;
- 292 committed paths after Git recognized 15 moves, leaving 184 additions,
  87 modifications, and 6 deletions;
- 35,847 insertions and 2,623 deletions in the preservation commit.

The snapshot includes multiple parallel July experiments rather than one coherent
release unit:

- Coach, Workspaces, screen-context, and Jobs-to-desktop handoff work;
- local capability and worker-auth experiments;
- Windows audio, capture, resampler, and integrity work;
- Cloudflare and origin-security scripts and evidence;
- DMG and Windows executable research;
- old generated Jobs assets and obsolete release files.

The complete state is now committed and pushed on the snapshot branch. The historical
checkout has a clean Git status. Ignored third-party reference clones remain external
research inputs and were not vendored into the Bluey repository.

## Security and hygiene review

Before preservation, the worktree was scanned for common Cloudflare, OpenAI, GitHub,
AWS, and private-key credential shapes while excluding ignored reference clones,
dependency trees, and build output. The only matches were intentionally synthetic
test strings in CI guard and log-redaction tests. No production credential was found.

`git diff --cached --check` identified one pre-existing trailing-space line in the
Round 507 historical document. It was retained verbatim because this commit is an
evidence snapshot, not a source cleanup or production candidate.

## Product-code disposition

Round 523 already performed the required hunk-level review of the same experimental
family. Its conclusions still hold on the much newer Round 588 mainline:

- reviewed Jobs packets, eligibility, metering, answer memory, identity binding,
  runner boundaries, worker authentication, receipt validation, and web interview
  preparation are already present on main;
- the historical Coach/Workspace/handoff chain is incomplete and cannot be released
  safely as isolated files;
- Windows owner-verified IPC, indexed workspace discovery, durable import recovery,
  and signed cross-platform canaries remain prerequisites before that chain can be
  freshly ported;
- old generated portal assets and release metadata must never replace current built
  artifacts;
- stale native, billing, trial, device, and security changes must not overwrite newer
  implementations.

No complete launch-ready product hunk remained to port from the preserved checkout.

## Remote branch audit

Every `origin/codex/*` branch was compared to current main after pruning remote refs.

Already merged into main:

- `bluey-ai-site`
- `bluey-fast-answer-latency-20260705`
- `bluey-interrupted-asks-round519-20260712`
- `bluey-jobs-coach-reconcile-20260713`
- `bluey-overlay-routing-hardening`
- `bluey-overlay-spacing-20260626`
- `bluey-round506-release-reconcile-20260712`
- `bluey-web-ui-parallel-20260704`
- `jobs-career-track-tailoring-p0-20260719`
- `jobs-one-week-launch-20260723`
- `round537b-dynamic-answer-routing`

Topologically unmerged but semantically reconciled:

- `bluey-branch-reconciliation-20260712`: one superseded documentation commit;
- `bluey-jobs-20260710`: one superseded Cloudflare evidence commit;
- `round584-bluey-jobs-link`: the accessible four-surface Jobs link and deployment
  evidence are present on main in Round 584 and current web source;
- `bluey-stream-attachments-20260704`: eleven historical patches whose required
  behavior was reimplemented with newer trial, device, routing, balance, overlay, and
  release safeguards, as documented in Rounds 520, 521, 524, 525, and 536;
- `bluey-jobs-historical-snapshot-20260802`: preservation evidence only and never a
  production merge candidate.

No Sashreek-owned branch was read as a merge candidate, modified, deleted, rebased,
or merged.

## Deployment disposition

There is no production deployment attached to Round 588 because no production source,
configuration, generated asset, or runtime artifact changed. Redeploying from the
historical snapshot would regress current authority and violate the current release
seal. Production remains the reviewed Round 587 deployment from current main.

The correct completion is therefore:

1. preserve the old state in a remote Git branch;
2. keep unsafe experiments out of main;
3. merge this reconciliation evidence through review;
4. verify current live production rather than replacing it with stale artifacts.

## Verification

- historical snapshot commit created successfully;
- snapshot branch pushed to `origin`;
- historical checkout Git status is clean;
- common credential-pattern scan found only synthetic test fixtures;
- all Codex-owned remote branches classified against current main;
- current web source contains the accessible `Apply for Jobs` action on all four
  intended Bluey surfaces;
- the meeting worktree remained untouched.

## Outcome

All historical Bluey work is now either:

- merged or semantically represented on current main;
- preserved on a named remote evidence branch; or
- explicitly deferred with its production prerequisites documented.

There is no uncommitted Bluey Jobs work left in the historical checkout, no hidden
bulk merge waiting for production, and no reason to redeploy any old runtime. The
meeting branch remains exactly as its owner left it.
