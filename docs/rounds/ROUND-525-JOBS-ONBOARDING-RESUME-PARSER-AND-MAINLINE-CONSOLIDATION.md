# Round 525 - Jobs onboarding resume parser and mainline consolidation

Date: 2026-07-16

Status: complete; verified source and generated Jobs portal assets are ready for
direct mainline promotion

## Objective

Fix the two Bluey Jobs onboarding failures reported by the owner:

1. Importing a PDF or DOCX resume populated only basic contact information
   instead of building the structured Career Profile.
2. Finishing onboarding could return the user to the first setup page after a
   partial save or reload.

This round also rechecks the surviving Codex branches and the deferred
Jobs-to-Coach work so reviewed Jobs behavior remains on main without merging old
experiments or touching Sashreek-owned branches.

## Root causes

### Resume import

The portal previously joined every PDF text item with spaces and inferred only a
name, email, and phone number. That erased visual rows and section boundaries, so
employment, education, skills, certifications, projects, dates, locations, and
links could not be recovered reliably.

### Onboarding restart

Setup progress lived primarily in component state until the final action. The
final action also issued independent profile, preferences, and Career Track
requests. A partial failure or reload could therefore leave a new track without
the completed profile, or a completed profile without the response needed to
route the portal forward.

The generated Career Track ID was not stable across an interrupted retry.
Separately, the create endpoint treated any caller-supplied non-empty ID as an
update, which allowed a new track to bypass the plan limit.

## Implementation

### Structured resume baseline

`jobs/portal/src/lib/documents.ts` now:

- reconstructs PDF rows from text coordinates instead of flattening each page;
- recognizes common summary, experience, education, skills, certification, and
  project headings;
- extracts employment titles, employers, locations, date ranges, current-role
  state, and accomplishment bullets;
- extracts schools, degrees, fields, locations, and dates;
- extracts skills, certifications, projects, technologies, links, headline,
  location, summary, email, and phone;
- creates stable imported-entry IDs so repeated inference does not create
  random identities;
- fills only empty profile fields and collections, preserving user edits;
- reports a compact extraction summary before the user continues.

The parser is deliberately a baseline extractor. The onboarding review remains
the source of truth, and an imported draft is never submitted by itself.

### Durable setup navigation

Every Continue and Back action now saves the current profile, preferences, and
onboarding step before changing pages. A reload resumes the last persisted
step.

The final action uses one dedicated
`POST /api/jobs/onboarding/complete` contract. The server:

1. validates the profile, preferences, and Career Track together;
2. applies the authenticated account email;
3. enforces the plan's Career Track limit;
4. saves preferences and the stable onboarding track;
5. saves `onboarding_complete` last;
6. returns a fresh authoritative workspace.

Saving completion last means a partial request cannot make the portal skip an
unfinished setup. The stable `onboarding-primary-track` identity makes retries
idempotent even after a browser reload.

The general Career Track endpoints now also reject unknown update IDs and treat
an unfamiliar caller-supplied ID as a new track for entitlement enforcement.

### Final-step recovery

If a resume did not contain a recognizable role or education entry, the final
page stays visible and offers direct `Review experience` and `Review education`
actions. It no longer silently jumps the user back to an earlier page.

### Reproducible visual fixture

The existing preview system now supports
`/jobs/?preview=1&scenario=onboarding`, giving desktop and mobile QA a stable
incomplete-profile fixture without changing production behavior.

## Mainline and branch reconciliation

The clean implementation base was `origin/main` at
`1d12b444a70c8f782c90a620fd681bc11a0e5f39`.

Reviewed non-Sashreek branch state:

- `origin/codex/bluey-jobs-20260710` has no patch-unique product work missing
  from main.
- `origin/codex/bluey-branch-reconciliation-20260712` has no patch-unique
  product work missing from main.
- `origin/codex/bluey-stream-attachments-20260704` remains an older,
  non-launch-ready experiment. Round 520 records that its useful behavior is
  represented by newer mainline implementations and that merging it wholesale
  would regress trial, provider, device, and release hardening.

The following Sashreek-owned refs were inspected only as branch names and were
not opened, changed, merged, reset, or deleted:

- `origin/agent/agent-bridge`
- `origin/agent/agent-bridge-fixes`
- `origin/agent/meeting-frontend`
- `origin/agent/parakeet-stt`
- `origin/meeting-main`

No historical branch is merged merely to make its branch name disappear.
Mainline consolidation is behavioral and patch-reviewed; obsolete remote refs
remain available as audit history.

## Bluey Jobs, Coach, Workspaces, and IPC

The earlier Bluey Jobs work is not discarded:

- the staged Jobs portal, application review, answer memory, eligibility,
  billing, runner boundaries, receipts, and web interview preparation remain on
  main;
- `jobs/automation/src/interview-prep.ts` and
  `jobs/portal/src/components/InterviewPrepDialog.tsx` remain the working
  interview-preparation path;
- the regular overlay, answers, files, screen context, sessions, dashboard, and
  Jobs portal remain independent of the deferred Coach chain.

Round 523 remains authoritative for the future desktop flow:

```text
Bluey Jobs
-> Open in Bluey
-> encrypted single-use handoff
-> account-scoped workspace
-> owner-verified local IPC
-> Coach interview-preparation view
```

That chain is intentionally not partially exposed. Shipping only the portal
action would create a dead button; shipping only the server exchange would
publish an unused security surface. A future port must land the indexed
workspace store, secure cross-platform IPC including Windows owner
verification, Coach lifecycle, signed platform canaries, one-time handoff, and
portal action in dependency order.

## Verification

Automated:

- Jobs package tests: 227 passed;
  - automation: 122;
  - browser: 30;
  - runner: 35;
  - workflows: 23;
  - portal: 17;
- all Jobs package TypeScript typechecks passed;
- Jobs production builds passed;
- server unit tests: 402 passed;
- server HTTP integration tests: 71 passed;
- remaining server integration targets passed;
- Rust formatting passed;
- `git diff --check` passed;
- generated Jobs output contains no source maps.

Focused coverage includes:

- structured resume sections and contact extraction;
- preservation of existing user-confirmed profile values;
- PDF row reconstruction;
- client-supplied Career Track IDs cannot bypass limits;
- retrying the stable onboarding track is idempotent.

Visual:

- desktop imported-resume view:
  [desktop-imported-resume.png](ROUND-525-JOBS-ONBOARDING-RESUME-PARSER-AND-MAINLINE-CONSOLIDATION.assets/desktop-imported-resume.png)
- desktop final review:
  [desktop-final-review.png](ROUND-525-JOBS-ONBOARDING-RESUME-PARSER-AND-MAINLINE-CONSOLIDATION.assets/desktop-final-review.png)
- mobile imported-resume view:
  [mobile-imported-resume.png](ROUND-525-JOBS-ONBOARDING-RESUME-PARSER-AND-MAINLINE-CONSOLIDATION.assets/mobile-imported-resume.png)
- mobile final review:
  [mobile-final-review.png](ROUND-525-JOBS-ONBOARDING-RESUME-PARSER-AND-MAINLINE-CONSOLIDATION.assets/mobile-final-review.png)

Both viewports had zero horizontal overflow. The scripted onboarding flow reached
step 6, selected `Find my matches`, and navigated to `/jobs/matches` without
returning to setup.

## Outcome

Resume import now creates a useful structured Career Profile baseline, setup
progress survives navigation and reloads, final completion is retry-safe, and
the user reaches Matches through one authoritative completion path.

The change is scoped to Jobs portal/server behavior and generated Jobs assets.
It does not modify the native overlay, audio, meeting runtime, Coach, workspace,
or local IPC implementations.
