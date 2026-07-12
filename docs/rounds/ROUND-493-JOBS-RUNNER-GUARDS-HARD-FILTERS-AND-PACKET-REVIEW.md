# Round 493 - Jobs Runner Guards, Hard Filters, and Packet Review

Date: 2026-07-11

## Context

This round incorporates the competitive recheck from `ROUND-492-JOBS-COMPETITIVE-RECHECK-AND-IMPLEMENTATION-HANDOFF.md`.

The goal was to close the highest-risk gaps without touching the existing Bluey meeting overlay, audio, or host runtime:

- review-first packets must not be runnable until explicitly approved
- unknown public job sites must not default to unattended automation
- stored hard filters must be enforced before packet generation and Auto-submit eligibility
- browser runs must carry an application identity and browser profile
- packet review must show real packet details instead of hardcoded placeholders
- public copy must stop implying reply/interview tracking or cover-letter behavior that is not complete end to end

## Implemented

### Review-first runner boundary

Changed the browser runner path so `awaiting_review` packets cannot be selected or queued directly.

- `jobs/portal/src/views/BrowserView.tsx`
  - runner dialogs now list only `queued` applications
  - empty states tell users to approve the packet in Applications first
- `jobs/portal/src/App.tsx`
  - `queueRun` now rejects anything that is not already `queued`
  - removed the implicit `commitApplication` call from runner selection
- `server/src/api/jobs.rs`
  - queue API rejects `awaiting_review`
  - queue API only starts `queued`, `running`, or `needs_input` records

### Unknown ATS policy

Changed unknown public URLs from automate-by-default to review/handoff.

- `jobs/automation/src/policy.ts`
  - known ATS URLs return runner-capable policy metadata
  - LinkedIn/Indeed remain handoff
  - unknown public URLs return `unknown_review`
- `jobs/automation/tests/policy.test.ts`
  - covers known Greenhouse automation capability
  - covers unknown public URL review/handoff behavior

### Server hard-filter enforcement

Moved core hard filters into the backend packet-preparation gate.

Enforced now:

- excluded companies
- excluded title terms
- clearly below-minimum compensation ranges
- clear employment type mismatch
- clear sponsorship blockers
- daily application cap
- one active application per company
- unknown or handoff sources cannot Auto-submit

Files:

- `server/src/db/jobs.rs`

Tests added:

- `unknown_public_sites_never_enter_background_auto_submit`
- `hard_filters_block_ineligible_packets`
- `account_limits_block_duplicate_company_and_daily_overflow`

### Identity-scoped browser packet

The queued runner payload now freezes:

- `applicationIdentityId`
- `applicationEmail`
- `browserProfileId`
- `resumeVersionId`

The server derives `browserProfileId` from account + application identity, matching the browser-profile isolation model required for multiple application emails.

Files:

- `server/src/api/jobs.rs`
- `jobs/automation/src/packet-guards.ts`
- `jobs/automation/src/execute.ts`
- `jobs/automation/tests/packet-guards.test.ts`
- `jobs/automation/tests/receipts.test.ts`
- `jobs/automation/tests/standard-adapters.test.ts`

### Answer Memory

Added shared automation helpers for reusable answers.

- account, Career Track, and company scopes
- confirmed answers only
- precedence: company > track > account
- normalized question keys

Files:

- `jobs/automation/src/answer-memory.ts`
- `jobs/automation/tests/answer-memory.test.ts`

### Packet review

Improved packet review so it shows real packet fields instead of hardcoded copy.

Now shown:

- actual resume version number and mode
- application email
- answer count
- cover-letter state
- site capability
- metering state
- real `ResumeVersion.diff`

Files:

- `jobs/portal/src/views/ApplicationsView.tsx`
- `jobs/portal/src/styles.css`

### Public copy softened

Updated the unauthenticated Jobs page to describe what exists now:

- unique application kit
- exact resume and answers together
- application receipt

Removed forward-looking claims about automatic reply, follow-up, assessment, and interview tracking from the hero/flow copy.

File:

- `jobs/portal/src/App.tsx`

## Verification

Passed:

- `npm test --workspace @bluey/jobs-automation`
- `npm run typecheck --workspace @bluey/jobs-automation`
- `npm run typecheck --workspace @bluey/jobs-portal`
- `npm test --workspace @bluey/jobs-portal`
- `npm run build --workspace @bluey/jobs-portal`
- `cargo test jobs::tests --manifest-path server/Cargo.toml`
- `cargo test restricted_sites_never_enter_background_auto_submit --manifest-path server/Cargo.toml`
- `git diff --check`

## Still Not Complete

This round does not complete public automation.

Remaining production work:

- full Greenhouse and Lever `prepare/fill/validate/submit`
- full Workday, Ashby, and SmartRecruiters adapters
- real cloud browser pool
- packaged macOS/Windows Bluey Browser installers
- Gmail/Outlook OAuth workers
- licensed discovery-provider credentials
- sandbox/live ATS certification
