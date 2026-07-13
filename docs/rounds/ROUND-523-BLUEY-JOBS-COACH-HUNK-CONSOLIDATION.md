# Round 523 - Bluey Jobs and Coach hunk consolidation

Date: 2026-07-13

Status: complete; reviewed Jobs behavior is on main and incomplete Coach work is
preserved outside main

## Objective

Finish the hunk-level review requested after the branch reconciliation rounds. The
goal is to ensure that complete Bluey Jobs and interview-preparation work is present
on the canonical mainline without exposing a partial desktop Coach flow or merging an
older experimental worktree wholesale.

This round changes documentation only. It does not rebuild or deploy the API, Jobs
service, Caddy configuration, web bundle, or signed native release.

## Source of truth

- Audit branch: `codex/bluey-jobs-coach-reconcile-20260713`
- Audit base: `origin/main` at `0f2933c4259e09a351a617bf94f0ed7a4b852f11`
- Clean audit worktree: `/private/tmp/bluey-round523-jobs-coach-reconcile`
- Preserved Jobs experiment: `/Users/uno/Downloads/cue-bluey-jobs`
- Concurrent answer-quality/source-recovery work remains untouched in
  `/Users/uno/Downloads/cue`
- Sashreek-owned branches remain explicitly excluded as listed in Round 521

## Hunk inventory

The preserved Jobs worktree is not a feature branch ready for merge. It is an older
base with 188 dirty paths: 108 tracked modifications and 80 untracked files. Its
tracked diff alone contains 15,345 additions and 2,951 deletions across Jobs, server,
daemon, dashboard, native, audio, IPC, storage, security, generated assets, and
research documentation.

The desktop Jobs/Coach implementation is largely untracked rather than a reviewed
commit. Its key files include:

- `server/src/api/jobs_handoff.rs`
- `server/src/db/jobs_handoffs.rs`
- `server/tests/jobs_handoff_api.rs`
- `server/migrations/postgres/003_jobs_bluey_handoffs.sql`
- `crates/cue-core/src/jobs_handoff.rs`
- `crates/cue-core/src/workspace.rs`
- `crates/cue-daemon/src/workspace_store.rs`
- `crates/cue-dashboard/ui/src/components/JobsHandoffProvider.tsx`
- `crates/cue-dashboard/ui/src/pages/Coach.tsx`
- `crates/cue-dashboard/ui/src/pages/ScreenContext.tsx`
- `jobs/portal/src/lib/bluey-handoff.ts`
- `jobs/portal/src/lib/bluey-handoff.test.ts`

No file in that worktree was staged, reset, deleted, or rewritten during this audit.

## Already integrated

The launch-safe Jobs and interview-preparation behavior is already represented on
main:

- reviewed Jobs application packets and explicit approval boundaries;
- shared server-owned eligibility, freshness, company collision, and daily limits;
- answer memory and intervention handling;
- identity-scoped application data and exact resume/evidence bindings;
- local and cloud runner capability boundaries;
- durable worker authentication and replay protection;
- typed submission evidence and receipt validation;
- evidence-grounded web interview preparation;
- the staged Jobs portal, billing integration, and account navigation.

The following files in the preserved Jobs experiment are byte-identical to current
main and therefore require no port:

- `jobs/automation/src/interview-prep.ts`
- `jobs/portal/src/components/InterviewPrepDialog.tsx`

The current mainline Applications view exposes the working `Prepare interview` flow.
It does not expose `Open in Bluey`, and the current server does not register
`bluey-handoff` issue or redeem routes. This is intentional: there is no visible dead
end or server route whose desktop consumer is absent.

## Why the desktop handoff is not a small UI port

The only meaningful Applications-view difference in the experiment is 37 added lines
for an `Open in Bluey` action. Shipping those lines safely requires the complete
dependency chain below:

1. Portal response validation and custom-URL launch.
2. Account-scoped server issue endpoint.
3. Hashed, encrypted, expiring, single-use handoff persistence.
4. Account-scoped server redeem endpoint.
5. Desktop custom-URL registration and nonce intake.
6. Authenticated local IPC import capability.
7. Durable pending import and crash recovery.
8. Context-file hashing and frozen submission evidence.
9. Assistant profile and Coach/workspace persistence.
10. Dashboard Coach route, activation, deletion, and recovery behavior.
11. Signed macOS and Windows runtime/install validation.

Merging only the portal action would create a polished dead button. Merging only the
server exchange would publish an unused security surface. Merging the whole old
worktree would overwrite newer runtime, security, billing, storage, and release work.

## Intentionally deferred work

### Jobs-to-desktop Coach handoff

The one-time handoff design has useful unit and integration coverage in the preserved
worktree, but it is coupled to the unmerged Coach/workspace stack. It remains deferred
until the complete chain is fresh-ported onto current main and certified as one
release unit.

### Coach and local Workspaces

Round 509 records two launch blockers that still apply:

- Windows local IPC intentionally fails closed until owner-only named-pipe/ACL and peer
  verification are implemented.
- Workspace/session discovery still needs an indexed path and scale proof instead of
  an O(all meeting files) scan.

The same round requires signed macOS and Windows integration canaries before release.
These are production gates, not optional follow-ups.

### One-click Live and screen-context replay

Round 505's one-click Live flow and Round 507's consent-first context replay are useful
fresh-port candidates. They overlap the concurrent Round 522 answer-quality, audio,
storage, IPC, and recovered-source work. They must be reviewed against that settled
source and then pass signed hardware/permission canaries. They are not part of this
Jobs/Coach merge.

### Credential and local IPC migration

The experiment also changes credential defaults and local trust boundaries. That is a
separate compatibility and migration round, not supporting glue to be pulled in with
a UI feature.

## Verification

The clean mainline source passed:

- full Jobs package tests: 225 passed;
  - automation: 122;
  - browser: 30;
  - runner: 35;
  - workflows: 23;
  - portal: 15;
- all Jobs package TypeScript typechecks;
- full Jobs production build;
- rebuilt Jobs portal generated byte-for-byte with no Git diff;
- focused server interview-prep tests: 3 passed;
- full server unit suite: 374 passed;
- clean Git status after verification.

The server suite includes the authoritative Jobs tests for review-first state,
hard-filter enforcement, atomic company/daily attempt reservations, unknown-site
review boundaries, job freshness, unique packet metering, identity binding, receipt
evidence, worker replay protection, and local capability scoping.

## Future port order

If desktop Coach is resumed, use a fresh branch from then-current main and land it in
this order:

1. Indexed, owner-scoped workspace store with migration and scale tests.
2. Cross-platform owner-verified local IPC, including Windows named-pipe ACLs.
3. Coach UI and workspace lifecycle without Jobs integration.
4. Signed macOS/Windows Coach canaries and rollback evidence.
5. Server handoff persistence and issue/redeem endpoints.
6. Desktop custom-URL intake, durable import, and account-binding tests.
7. Portal `Open in Bluey` action only after the target is installed and certified.

Each step must remain hidden or fail closed until its downstream consumer exists.

## Outcome

All reviewed, launch-safe Jobs work is on main. No complete product-code hunk is
missing from the preserved Jobs experiment. The incomplete Coach/workspace/handoff
implementation remains preserved for a fresh dependency-ordered port and is not
allowed to hitch a ride into main.

This is the intended consolidation result: a working Jobs portal and web interview
preparation today, with no dead Coach UI, no orphan handoff API, no stale bulk merge,
and no interference with concurrent Round 522 work or Sashreek-owned branches.
