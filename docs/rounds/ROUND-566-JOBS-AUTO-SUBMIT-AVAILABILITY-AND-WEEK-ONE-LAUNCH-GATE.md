# Round 566: Jobs Auto-submit Availability and Week-one Launch Gate

Date: 2026-07-23

## Goal

Make Auto-submit availability truthful and actionable across Matches, Applications,
Browser, and the Jobs API. A disabled control must always explain the exact reason
and the next usable path.

This round does not enable model generation or distribute either runner.

## Customer Contract

Bluey now distinguishes five application-system states:

| State | Customer meaning | Available path |
| --- | --- | --- |
| Certified | The ATS and server-side eligibility rules passed | Review first; Auto-submit only when an included runner is actually distributed |
| Beta review | The ATS integration is not certified for unattended submission | Prepare and review the application kit |
| Handoff | Bluey prepares the kit but the user finishes on the job site | Download or open the original job |
| Unknown review | Bluey has not certified the application system | Review the kit and complete it on the job site |
| Blocked | Bluey will not prepare or queue the application | Resolve the displayed hard-filter or policy failure |

Runner access is separate from ATS certification:

- a plan may require an upgrade;
- an included local or cloud runner may still be in invited beta;
- only a runner marked distributed by the server may accept a run.

The portal displays those states directly instead of presenting an inert
Auto-submit control.

## Server Authority

`JobsWorkspace` now carries authoritative local and cloud runner availability:

- `status`
- `available`
- `plan_included`
- `distribution_enabled`
- `reason`
- `next_action`

The Jobs API computes this state from the account entitlement and runtime
distribution flags. It is not inferred in the browser.

The API now enforces the same boundary at every irreversible transition:

1. Auto-submit preparation is rejected before expensive resume generation when
   the ATS or runner is ineligible.
2. Explicit packet approval is required before an `awaiting_review`
   application can enter any runner queue.
3. Queue creation re-evaluates the shared eligibility decision.
4. Local run creation requires an entitled and distributed local runner.
5. Cloud run creation requires an entitled and distributed cloud runner.
6. Review, download, and handoff remain available without a runner.

The error returned by the API contains the same ATS- or runner-specific reason
shown by the portal.

## Mixed-version Safety

The portal treats a workspace response without `runner_availability` as locked.
This fails closed during a partial API/portal rollout and prevents an older API
response from making Auto-submit appear usable.

Preview scenario query parameters are retained across Jobs navigation so every
capability state remains reproducible during visual QA.

## Portal Changes

### Matches

- Auto-submit explains hard Career Track failures before capability failures.
- Certified ATS jobs explain plan upgrades or invited-runner beta status.
- Beta ATS jobs explain the required packet review.
- Handoff jobs explain that Bluey prepares the kit and the user finishes on the
  job site.
- Unknown ATS jobs explain that the application system is not certified.
- Blocked jobs cannot be prepared or queued.

### Applications

- Packet review shows resume version and mode, application email, answers,
  cover-letter state, ATS capability, pause reasons, metering, and evidence
  policy.
- Handoff and unknown-review packets expose `Open job site`, not a misleading
  runner approval action.
- The Auto-submit panel names the exact reason and usable next step.

### Browser

- Each runner card reports `Available`, `Upgrade required`, or `Invited beta`.
- Runner buttons lead to the corresponding run, plan, or review workflow.
- No runner is implied to work while its distribution flag is disabled.

## Files

- `server/src/api/jobs.rs`
- `server/src/db/jobs.rs`
- `server/src/db/jobs/workspace.rs`
- `jobs/portal/src/App.tsx`
- `jobs/portal/src/components/AppShell.tsx`
- `jobs/portal/src/data/preview.ts`
- `jobs/portal/src/lib/application-flow.ts`
- `jobs/portal/src/lib/runner-access.ts`
- `jobs/portal/src/types.ts`
- `jobs/portal/src/views/ApplicationsView.tsx`
- `jobs/portal/src/views/BrowserView.tsx`
- `jobs/portal/src/views/MatchesView.tsx`
- focused portal and server tests for the same paths
- generated `web/jobs/` production bundle

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`: 758 Rust unit tests, 76 HTTP integration tests, and doc tests
- Jobs Free/Pro/Cloud runner policy matrix
- `npm test -- --run`: 11 files, 71 tests
- `npm run typecheck`
- `npm run build`
- `git diff --check`

Desktop browser QA covered:

- certified ATS with an included runner still in invited beta;
- beta-review ATS;
- handoff ATS;
- unknown-review ATS;
- local runner invited beta;
- cloud runner upgrade required;
- application packet review with handoff-only action.

Mobile QA used a real 390 x 844 iframe viewport:

- the navigation collapses;
- metrics and active-search details stack;
- primary actions remain full width;
- copy remains readable without overlap or horizontal overflow.

## Week-one Launch Scope

The credible one-week launch is a staged, review-first beta:

1. Greenhouse and Lever can move toward certified runner canaries only after
   authorized fixture and live test jobs pass.
2. Every other ATS remains beta review, handoff, unknown review, or blocked
   according to server policy.
3. No public copy may promise universal unattended submission.
4. The production flags remain off until canary evidence exists:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Owner Inputs Required Before Enabling a Runner

- Two or three authorized test vacancies for each week-one ATS.
- The exact plans allowed into the canary and an account/day concurrency cap.
- A support owner for stuck, uncertain-submit, and intervention cases.
- Confirmation whether local or cloud runner distribution is part of week one.
- Signed local-browser installers if local distribution is included.
- A production worker/browser pool and secure takeover endpoint if cloud
  distribution is included.
- Gmail/Outlook OAuth approval only if outcome tracking is included in launch
  claims.

## Rollback

Deploy the Jobs API and portal as one release. If either side fails:

1. restore the previous Jobs API binary;
2. restore the previous `/var/www/bluey/jobs` directory;
3. confirm all three production feature flags remain `0`;
4. verify `/health`, unauthenticated workspace `401`, and the signed-in
   review-first flow.

No native overlay, audio, meeting runtime, or desktop release is changed by this
round.
