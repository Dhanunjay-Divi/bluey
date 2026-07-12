# Round 515: Jobs Activation And Competitor Pass

Date: 2026-07-10

## Goal

Review AIApply's public product and onboarding surfaces, identify the product
communication patterns that reduce setup anxiety, and apply the useful lessons
to Bluey Jobs without copying its branding, visual system, claims, or code.

## Research

Reviewed the public pages at:

- `https://aiapply.co/`
- `https://aiapply.co/product`
- `https://aiapply.co/auto-apply`
- `https://aiapply.co/product/auto-apply/form`

The strongest patterns were immediate outcome framing, a visible product
demonstration before signup, a short explanation of the auto-apply sequence,
and an obvious indication that matching and applications continue after setup.

Bluey already has deeper product controls that should remain differentiators:

- one resume version per canonical job;
- exact application receipts and evidence;
- company, Career Track, and account Answer Memory;
- recent-job filtering and company conflict protection;
- local and cloud execution through one adapter contract;
- review-first defaults and explicit intervention handling.

No AIApply source code, media, testimonials, brand assets, or proprietary copy
was downloaded or reused.

## Changes

### Signed-out Jobs entry

Replaced the sparse authentication gate with a complete Bluey Jobs entry
experience that:

- explains fresh matching, job-specific resumes, review-first, and background
  execution in the first viewport;
- demonstrates the real product shape with a compact match and packet preview;
- states the free starting allowance without an oversized pricing pitch;
- shows the four-step path from profile import to reviewed or background runs;
- presents Free, Pro, and Cloud plans in a restrained comparison table;
- keeps Terms, Privacy, Help, Bluey login, and account creation immediately
  available;
- follows the saved or system light/dark preference before authentication.

### Onboarding framing

Kept Bluey's required six-step baseline instead of adding a marketing quiz.
Updated the framing so customers understand that setup is done once and every
job receives a separate tailored resume and answer set.

### Matches activation state

Added a compact active-search band showing:

- active Career Track count and current role/location context;
- maximum posting age;
- review-first or Auto-submit mode;
- daily application pace;
- a direct path to search settings.

This avoids claiming that a worker is currently running. It reflects the saved
Career Track configuration and clearly shows a paused state when no track is
active.

## Files

- `jobs/portal/src/App.tsx`
- `jobs/portal/src/main.tsx`
- `jobs/portal/src/components/Onboarding.tsx`
- `jobs/portal/src/views/MatchesView.tsx`
- `jobs/portal/src/styles.css`
- generated `web/jobs/` production assets

## Verification

- `npm run typecheck`: passed for automation, browser, workflows, and portal.
- `npm run test`: 26 automation tests and 1 portal test passed.
- `npm run build --workspace @bluey/jobs-portal`: passed.
- `git diff --check`: passed.
- Browser console: no warnings or errors.
- Verified the signed-out entry in light and dark themes.
- Verified desktop layout at the normal in-app browser viewport.
- Verified entry and Matches at `390 x 844`.
- Confirmed the mobile hero exposes the start of the next section.
- Confirmed the Matches status band does not overlap navigation, filters, job
  rows, or the fixed mobile tab bar.

## Scope

This round changes only the Bluey Jobs portal and its generated web assets. It
does not modify the host overlay, meeting runtime, audio, transcription,
billing authority, ATS adapters, workflow execution, or browser pool.
