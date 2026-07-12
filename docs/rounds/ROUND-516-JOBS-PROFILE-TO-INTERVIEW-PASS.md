# Round 478: Jobs Profile-To-Interview Pass

Date: 2026-07-10

## Goal

Review Perplexity Computer's public job-application workflow and make the
complete Bluey Jobs outcome easier to understand before signup. Keep the
experience compact and specific to Bluey instead of adding a generic AI
feature wall.

## Research

Reviewed the public surfaces at:

- `https://www.perplexity.ai/gen/computer/job-applications`
- `https://www.perplexity.ai/products/computer`
- Perplexity's public Computer and Scheduled Tasks help pages

The strongest communication pattern is one continuous promise: analyze a
profile, find roles, tailor materials, keep the work running, and organize the
results. The reference also makes connected tools and persistent memory easy
to discover.

Bluey Jobs already has the product concepts needed to go further:

- separate Career Tracks for role and location combinations;
- one job-specific resume version per canonical job;
- Answer Memory scoped by account, Career Track, and company;
- review-first and Auto-submit controls;
- local and cloud runners sharing the same application adapters;
- application receipts with the exact resume and answers used;
- interventions, inbox status sync, calendar sync, and follow-up reminders.

No Perplexity code, copy, media, product images, or brand assets were reused.

## Changes

### Complete outcome in the first viewport

Updated the signed-out Jobs entry to explain that Bluey:

- starts from one Career Profile;
- finds recent high-fit roles;
- creates a unique application for every job;
- applies locally or in the cloud on the user's terms;
- turns inbox replies into visible next steps.

The primary action now says `Start my job search`. The free allowance is
described as five complete applications instead of internal `packet`
terminology.

### Product preview

Expanded the preview from matching alone to a small pipeline snapshot. It now
shows:

- a connected inbox;
- fresh matches and average fit;
- a new reply;
- one application ready for review;
- one interview update;
- retained resume, answers, activity, and submission receipt.

### Profile-to-interview sequence

The entry flow now presents five concrete stages:

1. Build one Career Profile.
2. Find fresh, relevant roles.
3. Create a unique application.
4. Review or keep the local/cloud runner moving.
5. Track replies, follow-ups, assessments, and interviews through connected
   Gmail or Outlook accounts.

The responsive layout uses five restrained columns on wide screens, two
columns plus a full-width final stage on medium screens, and one column on
mobile.

### Customer language

Removed customer-facing `application packet` language from the complete Jobs
experience, including onboarding, Matches, Applications, Resume, Browser,
Settings, notifications, and the plan comparison. Customers now see
`application`, `tailored application`, or `application materials` according to
the state. Internal entitlement and metering identifiers remain unchanged.

## Files

- `jobs/portal/src/App.tsx`
- `jobs/portal/src/components/Onboarding.tsx`
- `jobs/portal/src/views/ApplicationsView.tsx`
- `jobs/portal/src/views/BrowserView.tsx`
- `jobs/portal/src/views/MatchesView.tsx`
- `jobs/portal/src/views/ResumeView.tsx`
- `jobs/portal/src/views/SettingsView.tsx`
- `jobs/portal/src/styles.css`
- generated `web/jobs/` production assets

## Verification

- `npm run typecheck`: passed for automation, browser, workflows, and portal.
- `npm run test`: 26 automation tests and 1 portal test passed.
- `npm run build`: passed for the complete Jobs workspace.
- `git diff --check`: passed.
- Browser console: no warnings or errors.
- Verified the normal desktop viewport.
- Verified the signed-out entry at `390 x 844` with no horizontal overflow.
- Confirmed the next section remains visible below the mobile hero.
- Confirmed Matches, Applications, Resume, Browser, and Settings no longer
  expose the internal `packet` term.

## Scope

This round changes only the Bluey Jobs portal, its generated web bundle, and
this documentation. It does not modify the host overlay, meeting runtime,
audio, transcription, billing authority, workflow execution, browser runner,
or ATS adapters.
