# Round 485 - Jobs Account Portal Inspection

Date: 2026-07-11
Branch: `codex/bluey-jobs-20260710`
Scope: inspect competitor account/onboarding surfaces and record what Bluey Jobs should learn before the next implementation pass.

## Sites inspected

The screenshot assets for this round live in `ROUND-485-JOBS-ACCOUNT-PORTAL-INSPECTION.assets/`.

Primary flows captured:

- `aiapply` home, sign-in, quiz, work authorization, location, job-title, product handoff, OTP, and post-analysis states.
- `applyblast` home, login, OTP, get-hired flow, profile questions, and post-login route behavior.

Generated browser profile folders were intentionally removed from the asset set before commit. Screenshots are useful; browser cache, cookies, history, and profile data are not.

## What competitors do well

- They ask for the user's target role and location very early.
- They gather work authorization, salary, workplace preference, and timing before showing an empty dashboard.
- They use short, wizard-like steps instead of one giant profile form.
- They make "get hired" feel like a guided service rather than a blank automation console.
- They force enough baseline detail that first matches can be useful immediately after onboarding.

## What Bluey should do better

- Keep the Bluey Jobs dashboard useful after onboarding by showing first matches, application identity, daily plan, and next required action.
- Keep profile completion compact, but make it complete enough for real applications: work authorization, location policy, salary, roles, employers, dates, education, projects, and reusable answers.
- Avoid vague copy like "AI applies for you" unless the current plan and site support real submission.
- Show clear state for each application: preparing, waiting on user, ready to review, queued, running, submitted, failed, or stale.
- Keep proof beside every application: resume used, answers used, JD snapshot, screenshot, timestamp, and outcome.
- Ask for email/calendar integration only when it unlocks something visible: OTP handoff, application updates, reminders, and interview tracking.

## Product requirements confirmed

1. Baseline onboarding must finish before agents run.
2. Target role and location should be first-class, not optional preferences buried later.
3. Multiple application identities need separate browser profiles and email/calendar links.
4. Each Career Track needs its own resume strategy and company collision policy.
5. The Intervention Inbox should remember answers so repeated questions stop interrupting future runs.
6. A new user should reach useful matches before seeing a long empty portal.

## Bluey Jobs implementation notes

- The Bluey flow should keep the current `Matches`, `Applications`, `Resume`, `Browser`, and `Settings` shell, but onboarding should front-load the required questions from the competitor inspections.
- The `Settings` area should include Answer Memory, application identities, excluded companies, job freshness, daily volume, and site/account connections.
- The `Applications` area should be receipt-first. Users should never wonder which resume or answer packet was used.
- The `Browser` area should show local/cloud browser state, paused interventions, and takeover actions without exposing raw cookies or profile internals.

## Asset hygiene

The inspection generated screenshots and browser profile directories. The profile directories were removed because they can contain local browser state. Future competitive inspections should save screenshots to a dedicated assets folder and keep Playwright user data dirs under `/tmp` or another ignored scratch path.
