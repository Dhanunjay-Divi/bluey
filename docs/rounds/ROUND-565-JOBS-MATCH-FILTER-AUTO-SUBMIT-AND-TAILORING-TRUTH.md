# Round 565: Jobs Match Filters, Auto-submit, and Tailoring Truth

Date: 2026-07-23

## Goal

Fix the user-visible mismatch between Career Track preferences and the Matches list, explain why Auto-submit is unavailable for some application systems, and make the job-specific resume behavior clear without overstating unsupported model generation or fabricating candidate history.

## Root Causes

1. The server already returned authoritative eligibility failures for location, workplace, employment type, engagement type, work authorization, sponsorship, freshness, experience bounds, exclusions, daily limits, company collisions, and ATS capability.
2. The portal rendered every returned match in the default list, even when the match had hard eligibility failures.
3. Auto-submit controls were disabled for beta, handoff, unknown-review, and blocked application systems, but the portal did not explain the specific capability boundary.
4. The resume controls did not clearly explain that preparation creates a new job-specific version from verified evidence or distinguish Factual from supported Enhance behavior.

## Changes

### Default Match Visibility

- The default Matches list now excludes jobs with hard Career Track failures.
- Candidate leads from managed feeds remain visible while Bluey verifies posting facts.
- A user can explicitly reveal excluded jobs through `Show jobs outside my rules`.
- The empty state reports how many jobs are outside the active Career Track and offers `Review excluded` or `Adjust Career Track`.
- Revealed excluded rows are labeled `Outside your rules`.
- Search, match score, workplace, prepared-state, and Career Track visibility filters compose in one portal selector.

### Auto-submit Truth

- Auto-submit now explains the actual application-system capability:
  - beta systems require packet review;
  - handoff systems require the user to finish on the employer site;
  - unknown systems remain review-only;
  - blocked systems cannot be queued.
- This round does not enable uncertified runners or change any distribution flag.
- Review first remains the default and the production-safe behavior.

### Job-specific Resume Tailoring

- `Prepare application` is described as creating a new resume for the selected job from verified candidate experience and showing every change before submission.
- `Factual` keeps verified wording and prioritizes the strongest relevant evidence.
- `Enhance` can rewrite, reorder, shorten, and emphasize supported skills and achievements.
- Enhance cannot invent employers, clients, dates, credentials, years of experience, or unsupported responsibilities.
- Existing server tailoring remains deterministic while production model generation is disabled.
- Existing resume-truth validation continues to reject unsupported employer, role, date, skill, and evidence claims.

## Files

- `jobs/portal/src/views/MatchesView.tsx`
- `jobs/portal/src/views/MatchesView.test.tsx`
- `jobs/portal/src/styles.css`
- generated `web/jobs/` production bundle

## Verification

- `npm run typecheck`
- `npm test -- --run`: 10 files, 65 tests
- `cargo test jobs_tailoring`: 7 passed
- `cargo test shared_eligibility_enforces_location_and_survives_into_receipt`: passed
- `cargo test engagement_preferences_block_known_mismatches`: passed
- `npm run build`
- `git diff --check`
- Desktop browser QA:
  - default list shows only eligible matches;
  - filter dialog exposes excluded jobs only on demand;
  - match review explains Factual, Enhance, Review first, and capability-specific Auto-submit behavior.
- Mobile browser QA at 390 x 844:
  - no horizontal overflow;
  - dialog width 374 px;
  - all controls and copy remain visible.

## Visual Evidence

- `docs/rounds/assets/round-565-matches-filter.png`
- `docs/rounds/assets/round-565-match-tailoring.png`
- `docs/rounds/assets/round-565-matches-mobile.png`

## Deployment Boundary

This is a static Jobs portal deployment only. It must not restart the API, enable model generation, or enable local/cloud browser distribution. The required production flags remain:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Rollback

Restore the pre-Round-565 `/var/www/bluey/jobs` backup and reload the static route. No database, API binary, native client, or schema rollback is required.
