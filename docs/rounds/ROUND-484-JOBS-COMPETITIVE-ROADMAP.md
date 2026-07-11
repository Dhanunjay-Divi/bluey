# Round 484 - Jobs Competitive Roadmap

Date: 2026-07-11
Base branch: `codex/bluey-jobs-20260710`

Source of truth: Rounds 480-483, `jobs/README.md`, `jobs/ARCHITECTURE.md`,
`jobs/OPERATIONS.md`, `docs/rounds/ROUND-479-JOBS-AUTOMATION-EXECUTION-PIPELINE.md`,
and targeted Jobs implementation inspection. No browsing was needed.

## P0 Before Internal Dogfood

### 1. Make Review Mode the default product posture

- Customer value: Internal users can see exactly what will be sent, why Bluey
  believes it is safe, what it costs, and where it will pause before any real
  employer submission.
- Implementation size: Medium. Mostly portal copy/layout plus a small capability
  field in match/application payloads if badges should be server-authored.
- Dependencies: Existing `review_first` and `auto_submit` modes; existing
  receipt/evidence model; existing runner entitlement checks.
- Risk: Low product risk, medium UX risk if the review panel becomes too dense.
  Keep it as a checklist: resume, answers, email identity, site capability,
  runner, allowance/overage, and receipt expectations.
- Exact existing Bluey modules affected: `jobs/portal/src/views/MatchesView.tsx`,
  `jobs/portal/src/views/ApplicationsView.tsx`, `jobs/portal/src/views/BrowserView.tsx`,
  `jobs/portal/src/types.ts`, `jobs/portal/src/data/preview.ts`,
  `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

### 2. Surface automation boundaries before queueing

- Customer value: Users know whether a job is auto-submit supported,
  review-first only, handoff required, or unsupported before investing time in
  a packet.
- Implementation size: Small to medium. Reuse policy/adapters first; defer a
  large compatibility database.
- Dependencies: Existing `submissionPolicy`, standard ATS adapters, LinkedIn
  and Indeed handoff policy, match source metadata.
- Risk: Medium. Overstating support is worse than hiding support. Start with
  conservative statuses and "why" copy.
- Exact existing Bluey modules affected: `jobs/automation/src/policy.ts`,
  `jobs/automation/src/standard-adapters.ts`, `jobs/automation/src/public-ats.ts`,
  `jobs/portal/src/views/MatchesView.tsx`, `jobs/portal/src/views/BrowserView.tsx`,
  `jobs/portal/src/types.ts`, `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

### 3. Add queue governance and kill switches

- Customer value: Dogfood users can pause a Career Track, stop queued runs,
  enforce daily limits, avoid duplicate-company attempts, and recover from a bad
  search loop without support intervention.
- Implementation size: Medium to large. Some controls exist as settings; the
  missing work is enforcement at queue/start time and visible run cancellation.
- Dependencies: Existing Career Track active state, daily limits, one active
  application per identity, packet metering, Temporal workflow state.
- Risk: Medium. Cancellation must not create duplicate submits or ambiguous
  metering. Only allow stopping queued or pre-submit runs unless the runner can
  prove no submit occurred.
- Exact existing Bluey modules affected: `jobs/portal/src/views/SettingsView.tsx`,
  `jobs/portal/src/views/BrowserView.tsx`, `jobs/portal/src/views/ApplicationsView.tsx`,
  `server/src/api/jobs.rs`, `server/src/db/jobs.rs`,
  `jobs/workflows/src/workflows.ts`, `jobs/workflows/src/activities.ts`,
  `jobs/workflows/src/contracts.ts`, `jobs/runner/src/server.ts`.

### 4. Ship dogfood observability and retry taxonomy

- Customer value: Internal dogfood can distinguish product issues from ATS
  drift, missing answers, upload failures, ambiguous confirmation, challenge
  timeouts, and provider setup gaps.
- Implementation size: Medium. Event taxonomy plus dashboards/log queries; no
  new customer flow required.
- Dependencies: Existing run events, worker state endpoints, Temporal retries,
  runner result store, receipt status/issues.
- Risk: Low customer risk, high operational value. The main risk is logging
  sensitive content; taxonomy must use categories and ids, not raw resume,
  answers, OTPs, screenshots, or cookies.
- Exact existing Bluey modules affected: `server/src/api/jobs.rs`,
  `server/src/db/jobs.rs`, `jobs/workflows/src/activities.ts`,
  `jobs/workflows/src/workflows.ts`, `jobs/runner/src/server.ts`,
  `jobs/runner/src/result-store.ts`, `jobs/automation/src/receipts.ts`,
  `jobs/automation/src/contracts.ts`.

### 5. Define Jobs-specific trust, retention, export, and deletion rules

- Customer value: Dogfooders know what Bluey stores, where application data
  goes, what can be deleted/exported, what employers retain after submission,
  and how receipts differ from general Bluey session data.
- Implementation size: Medium for policy/docs, large if full export/delete UI
  must ship immediately. For dogfood, policy plus admin-verifiable controls is
  enough.
- Dependencies: Counsel/product approval; existing receipt, browser profile,
  application identity, mailbox, evidence, and answer-memory models.
- Risk: Medium. General Bluey 90-day synced-session retention can conflict with
  durable Jobs receipts unless Jobs retention is explicitly separated.
- Exact existing Bluey modules affected: `jobs/README.md`, `jobs/ARCHITECTURE.md`,
  `jobs/OPERATIONS.md`, `jobs/portal/src/views/SettingsView.tsx`,
  `jobs/portal/src/views/ApplicationsView.tsx`, `server/src/api/jobs.rs`,
  `server/src/db/jobs.rs`, `jobs/runner/src/profile-store.ts`,
  `jobs/automation/src/receipts.ts`.

## P1 Before Invited Beta

### 1. Replace first-run homework with a Quick Start path

- Customer value: A candidate can import a resume, set role/location/salary and
  dealbreakers, then review one sample packet before completing every profile
  detail.
- Implementation size: Medium. It reshapes onboarding but can reuse current
  import, profile, preferences, and track save APIs.
- Dependencies: Existing browser-side resume import, Career Profile,
  JobPreferences, Career Track, preview data.
- Risk: Medium. Too little data can create low-quality matches. Gate real
  submission until required facts, email identity, and review checks are
  complete.
- Exact existing Bluey modules affected: `jobs/portal/src/components/Onboarding.tsx`,
  `jobs/portal/src/App.tsx`, `jobs/portal/src/lib/documents.ts`,
  `jobs/portal/src/views/MatchesView.tsx`, `jobs/portal/src/types.ts`,
  `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

### 2. Add a Today view as the Jobs home

- Customer value: Invited beta users get a short queue of what matters now:
  review strong matches, answer paused applications, connect an inbox, follow up
  soon, or improve setup.
- Implementation size: Medium. New portal view backed initially by workspace
  aggregation.
- Dependencies: Existing applications, interventions, browser sessions,
  evidence, mailbox connections, entitlements, and matches.
- Risk: Low to medium. Bad prioritization can hide important items. Keep counts
  and links to the underlying tabs.
- Exact existing Bluey modules affected: `jobs/portal/src/App.tsx`,
  `jobs/portal/src/components/AppShell.tsx`, `jobs/portal/src/types.ts`,
  `jobs/portal/src/data/preview.ts`, `jobs/portal/src/views/ApplicationsView.tsx`,
  `jobs/portal/src/views/MatchesView.tsx`, `jobs/portal/src/views/SettingsView.tsx`,
  `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

### 3. Turn dealbreakers into a visible policy summary

- Customer value: Users can audit what Bluey will skip: salary floor, remote or
  on-site preference, seniority/title exclusions, sponsorship, excluded
  companies, duplicate-company rule, max posting age, daily limits, and
  selectivity threshold.
- Implementation size: Small to medium. Mostly UI and preflight copy if current
  filters remain unchanged.
- Dependencies: Existing JobPreferences, CareerProfile thresholds, match
  filtering, Career Track settings.
- Risk: Medium. If a rule is displayed but not enforced in discovery/queueing,
  trust is damaged. Wire every displayed hard rule to a server-side check.
- Exact existing Bluey modules affected: `jobs/portal/src/views/SettingsView.tsx`,
  `jobs/portal/src/views/MatchesView.tsx`, `jobs/portal/src/components/Onboarding.tsx`,
  `jobs/portal/src/types.ts`, `server/src/api/jobs.rs`, `server/src/db/jobs.rs`,
  `jobs/automation/src/public-ats.ts`.

### 4. Make receipts and tracker rows evidence-forward

- Customer value: The tracker becomes more than a status list: source, next
  action, date applied, resume used, application email, runner, receipt
  completeness, and confirmation evidence are visible at a glance.
- Implementation size: Medium. Mostly customer UI, with possible additions to
  workspace payloads to avoid per-row fetches.
- Dependencies: Existing ApplicationEvidence, receipts, selected identity,
  run events, submitted timestamps.
- Risk: Low. Avoid exposing raw storage keys or sensitive screenshot content in
  list rows.
- Exact existing Bluey modules affected: `jobs/portal/src/views/ApplicationsView.tsx`,
  `jobs/portal/src/types.ts`, `jobs/portal/src/api.ts`,
  `jobs/portal/src/data/preview.ts`, `server/src/api/jobs.rs`,
  `server/src/db/jobs.rs`, `jobs/automation/src/receipts.ts`.

### 5. Wire status sync and daily digest around inbox connections

- Customer value: Users see employer replies, interview events, failed runs,
  waiting interventions, and receipts ready without living in the app.
- Implementation size: Large. Requires provider OAuth gates plus message
  classification, digest preferences, and notification delivery.
- Dependencies: Gmail/Outlook OAuth apps, mailbox connection model, provider
  message references, intervention/email OTP handling, application evidence.
- Risk: High. Mailbox scopes, retention, support access, and false-positive
  classification need careful beta limits.
- Exact existing Bluey modules affected: `jobs/portal/src/views/SettingsView.tsx`,
  `jobs/portal/src/views/ApplicationsView.tsx`, `jobs/portal/src/types.ts`,
  `jobs/portal/src/api.ts`, `server/src/api/jobs.rs`, `server/src/db/jobs.rs`,
  `jobs/workflows/src/activities.ts`, `jobs/workflows/src/contracts.ts`.

### 6. Add pricing preflight, self-serve policy copy, and plan explainers

- Customer value: Users know what counts, when overage applies, what happens on
  retry/handoff, how cancellation/refund works, and why Pro/Cloud are different.
- Implementation size: Medium for preflight and copy; large for full Square
  recurring checkout and add-on billing.
- Dependencies: Existing entitlement policy, atomic packet metering, shared
  balance, plan UI, external Square recurring product mapping.
- Risk: Medium. Overage must fail before queueing, not after a user expects a
  run to start. Avoid annual plans until real cost data exists.
- Exact existing Bluey modules affected: `jobs/portal/src/views/SettingsView.tsx`,
  `jobs/portal/src/views/MatchesView.tsx`, `jobs/portal/src/views/BrowserView.tsx`,
  `jobs/portal/src/types.ts`, `server/src/api/jobs.rs`,
  `server/src/db/jobs.rs`, `server/src/bin/bluey-jobs-api.rs`.

## P2 Before Public Launch

### 1. Certify and publish the compatibility matrix

- Customer value: Public users can see supported auto-submit ATSs, handoff-only
  sites, planned coverage, and beta confidence before trusting automation.
- Implementation size: Large. Requires live tenant certification, acceptance
  tests, public matrix, and conservative fallback behavior.
- Dependencies: Live sandbox/certification for Workday, Greenhouse, Lever,
  Ashby, SmartRecruiters; R2/S3 receipt uploads; takeover gateway; provider
  credentials.
- Risk: High. ATS variants change. The public matrix must distinguish
  certified, beta, review-first, handoff, and unsupported.
- Exact existing Bluey modules affected: `jobs/automation/src/standard-adapters.ts`,
  `jobs/automation/src/public-ats.ts`, `jobs/automation/src/form-intelligence.ts`,
  `jobs/automation/tests/standard-adapters.test.ts`,
  `jobs/automation/tests/public-ats.test.ts`, `jobs/portal/src/views/BrowserView.tsx`,
  `jobs/portal/src/views/MatchesView.tsx`, `server/src/api/jobs.rs`.

### 2. Build a measured direct-employer form expansion path

- Customer value: Bluey feels more complete beyond the first five ATS families
  without pretending every arbitrary form is safe for background submission.
- Implementation size: Large. Expand semantic form intelligence under
  review-first default, with adapter confidence and strict submit confirmation.
- Dependencies: Existing constrained semantic boundary, form intelligence,
  Playwright bridge, receipt confirmation rules, intervention flow.
- Risk: High. This is where competitors overpromise. Keep auto-submit off until
  form families have live evidence and deterministic confirmation.
- Exact existing Bluey modules affected: `jobs/automation/src/form-intelligence.ts`,
  `jobs/automation/src/adapters.ts`, `jobs/automation/src/execute.ts`,
  `jobs/automation/src/playwright-page.ts`, `jobs/automation/src/receipts.ts`,
  `jobs/runner/src/server.ts`, `jobs/browser/src/main.ts`,
  `jobs/workflows/src/workflows.ts`.

### 3. Productize job-search health metrics

- Customer value: Users see response rate, no-response aging, interview
  conversion, stale follow-ups, weak-match patterns, and what to adjust next.
- Implementation size: Medium to large depending on whether email/calendar sync
  is available.
- Dependencies: Submitted applications, status email evidence, interview event
  evidence, match scores, application timestamps, follow-up reminders.
- Risk: Medium. Metrics can feel judgmental or misleading with sparse data.
  Show confidence and use suggestions, not guarantees.
- Exact existing Bluey modules affected: `jobs/portal/src/views/ApplicationsView.tsx`,
  `jobs/portal/src/App.tsx`, `jobs/portal/src/types.ts`,
  `jobs/portal/src/data/preview.ts`, `server/src/api/jobs.rs`,
  `server/src/db/jobs.rs`, `jobs/workflows/src/activities.ts`.

### 4. Add post-application interview prep from submitted context

- Customer value: After a receipt locks, Bluey can produce role-specific likely
  questions, STAR notes from the exact submitted resume, company talking points,
  and follow-up drafts.
- Implementation size: Medium. It can begin as generated artifacts tied to
  submitted applications.
- Dependencies: Submitted receipt, exact resume, job description, answer set,
  evidence, optional company data.
- Risk: Medium. Must not invent experience or imply guaranteed interviews.
  Keep prep explicitly grounded in the submitted packet.
- Exact existing Bluey modules affected: `jobs/portal/src/views/ApplicationsView.tsx`,
  `jobs/portal/src/views/ResumeView.tsx`, `jobs/portal/src/types.ts`,
  `jobs/automation/src/documents.ts`, `jobs/automation/src/receipts.ts`,
  `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

### 5. Publish responsible automation and Jobs privacy materials

- Customer value: Public users, employers, and counsel get a coherent statement
  of human approval, no fabricated facts, no duplicate spam, restricted-site
  handoffs, provider processing, subprocessors, support access, export/delete,
  and post-submission limits.
- Implementation size: Medium. Mostly policy/content plus in-product links.
- Dependencies: Counsel approval, provider/subprocessor list, retention
  decisions, support access model, cancellation/refund rules.
- Risk: Medium. Claims must match production behavior, especially AI training,
  mailbox scopes, browser profile storage, and receipt retention.
- Exact existing Bluey modules affected: `jobs/README.md`, `jobs/ARCHITECTURE.md`,
  `jobs/OPERATIONS.md`, `jobs/portal/src/components/Onboarding.tsx`,
  `jobs/portal/src/views/SettingsView.tsx`, `jobs/portal/src/views/BrowserView.tsx`,
  `jobs/portal/src/views/ApplicationsView.tsx`, `server/src/api/jobs.rs`,
  `server/src/db/jobs.rs`.

## Later Experiments

### 1. Browser extension or clipper

- Customer value: Users can save jobs from arbitrary browsing and route
  supported ATS pages back into Bluey's planning pipeline.
- Implementation size: Large.
- Dependencies: Extension distribution, URL policy, account auth, manual import
  flow, ATS capability detection.
- Risk: High. Extension permissions and restricted-site behavior need careful
  review.
- Exact existing Bluey modules affected: `jobs/portal/src/views/MatchesView.tsx`,
  `jobs/automation/src/policy.ts`, `jobs/automation/src/public-ats.ts`,
  `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

### 2. Company research and networking packet

- Customer value: Users can understand company fit, role risk, public context,
  and possible outreach angles before applying.
- Implementation size: Large if live research/contact discovery is included;
  medium for a static job-description-based version.
- Dependencies: Licensed/public data source decisions, privacy policy updates,
  match-detail UI.
- Risk: Medium to high. Contact discovery can feel invasive and data quality can
  be poor.
- Exact existing Bluey modules affected: `jobs/portal/src/views/MatchesView.tsx`,
  `jobs/portal/src/types.ts`, `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

### 3. Additional ATS families

- Customer value: Broader coverage reduces handoffs for iCIMS, Taleo, Avature,
  Workable, Jobvite, BambooHR, UKG, Oracle Recruiting, and other common systems.
- Implementation size: Large per family.
- Dependencies: Live test tenants, adapter fixtures, receipt confirmation,
  upload variants, legal/platform review.
- Risk: High. Coverage breadth should not weaken receipt quality or
  non-guessing policy.
- Exact existing Bluey modules affected: `jobs/automation/src/standard-adapters.ts`,
  `jobs/automation/src/adapters.ts`, `jobs/automation/src/public-ats.ts`,
  `jobs/automation/tests/standard-adapters.test.ts`,
  `jobs/automation/tests/public-ats.test.ts`, `jobs/runner/src/server.ts`,
  `jobs/browser/src/main.ts`.

### 4. Offer and negotiation tracker

- Customer value: Bluey can support the job search after interviews with offer
  comparison, salary notes, and follow-up reminders.
- Implementation size: Medium to large.
- Dependencies: Application status model, calendar/email evidence, new offer
  entities.
- Risk: Medium. Compensation advice can become high-stakes; keep it factual and
  user-controlled.
- Exact existing Bluey modules affected: `jobs/portal/src/views/ApplicationsView.tsx`,
  `jobs/portal/src/types.ts`, `server/src/api/jobs.rs`, `server/src/db/jobs.rs`.

## Explicit Non-Goals

- Compete on unlimited applications, hundreds per day, or "apply while you
  sleep" volume.
- Bypass CAPTCHA, phone/app 2FA, assessments, platform rate limits, or
  restricted job-board policies.
- Background-automate LinkedIn or Indeed; keep them handoff-only unless policy
  and platform posture materially change.
- Fabricate work authorization, sponsorship, clearance, demographic,
  disability, veteran, relocation, salary, availability, or experience claims.
- Store raw job-site passwords, raw OTP codes, unencrypted browser profiles, or
  machine-local file paths in account receipts.
- Hide submitted jobs, submitted materials, pricing rules, cancellation paths,
  or post-submission non-reversibility.
- Introduce opaque application credits, browser-minute pricing, ATS-family
  pricing, or annual Jobs discounts before real beta cost and refund data exist.
- Promise interviews, jobs, undetectable AI use, or guaranteed outcomes.
