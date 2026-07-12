# Round 481 - Jobs Automation Gap Audit

## Scope

Analysis-only competitive audit for Bluey Jobs automation. Public pages only.
No accounts were created, no purchases were made, no forms were submitted, and
screenshots were captured only for public competitor evidence.

Local sources reviewed:

- `jobs/README.md`
- `jobs/ARCHITECTURE.md`
- `jobs/OPERATIONS.md`
- `docs/rounds/ROUND-479-JOBS-AUTOMATION-EXECUTION-PIPELINE.md`
- Visible code in `jobs/automation`, `jobs/browser`, `jobs/runner`, and
  `jobs/workflows`

Public sources reviewed:

| Product | URLs |
| --- | --- |
| AIApply | https://aiapply.co/ and https://aiapply.co/auto-apply |
| ApplyBlast | https://applyblast.com/ and https://applyblast.com/get_hired |
| LazyApply | https://lazyapply.com/ and https://lazyapply.com/indeed-auto-apply-bot |
| Sonara | https://www.sonara.ai/ |
| JobCopilot | https://jobcopilot.com/automate-job-search/ and https://jobcopilot.com/automate-job-applications/ |
| LoopCV | https://www.loopcv.pro/ and https://www.loopcv.pro/auto-apply-for-jobs/ |
| Simplify | https://simplify.jobs/ and https://simplify.jobs/copilot |
| Teal | https://www.tealhq.com/ |
| Huntr | https://huntr.co/ and https://chromewebstore.google.com/detail/huntr-job-search-tracker/mihdfbecejheednfigjpdacgeilhlmnf |
| Jobscan | https://www.jobscan.co/ and https://www.jobscan.co/auto-apply |

Screenshot evidence captured under
`docs/rounds/ROUND-481-JOBS-AUTOMATION-GAP-AUDIT.assets/`:

- `jobwizard-gmail-integration.png`
- `loopcv-auto-apply.png`
- `simplify-copilot.png`

## Competitive Read

The market splits into three automation postures.

1. High-volume auto-submit systems: AIApply, ApplyBlast, LazyApply, Sonara,
   JobCopilot, and LoopCV emphasize applying to many jobs automatically after
   the user sets preferences and uploads a resume/CV. JobCopilot and LoopCV are
   most explicit about repeated background scans, filters, dashboard tracking,
   and daily/email recaps.
2. Extension/autofill systems: Simplify, Huntr, Teal, and parts of LoopCV use a
   browser extension or job clipper to save jobs, autofill forms, tailor
   answers, and track submitted applications while the user remains near the
   final action.
3. Review-first systems: Jobscan's Auto Apply page is the clearest public
   articulation of "prepare the application, but require human approval before
   submission." JobCopilot also offers an autofill-and-save-for-review mode.

The most relevant product pressure for Bluey is not raw "applications per day."
It is making a reliable, inspectable automation loop feel as easy as the
competitors' promises: set preferences, discover matches continuously, prepare
job-specific materials, pause only when necessary, preserve context for manual
takeover, and give the user a clear record afterward.

## 1. Already Implemented In Bluey

Bluey already has unusually strong automation plumbing compared with public
claims from competitors:

- ATS coverage foundation: deterministic adapters exist for Workday,
  Greenhouse, Lever, Ashby, and SmartRecruiters, plus a constrained semantic
  adapter boundary.
- Shared execution contract: local Browser and cloud runner consume the same
  typed adapter contract.
- Discovery safety: public ATS discovery uses known HTTPS endpoints,
  host-pinning, bounded pagination, redirect rejection, response caps, retry,
  normalization, and canonical dedupe.
- Form planning: confirmed facts and scoped Answer Memory fill known fields;
  company answers override Career Track answers, which override account
  answers; direct application answers win over saved memory.
- Non-guessing policy: unknown required fields, sensitive questions,
  CAPTCHA, assessments, and 2FA become interventions instead of fabricated
  answers.
- Compliant challenge handling: CAPTCHA, SMS/app/authenticator 2FA, and
  assessments preserve the browser for account-owner takeover. Email OTP can be
  matched from connected Gmail/Outlook and offered for explicit approval without
  persisting the raw code.
- Local browser continuity: an Electron controller uses a user-owned Chromium
  profile per verified application email, preserves the visible page during
  interventions, and reports terminal results through the Jobs API.
- Cloud continuity: cloud runner uses identity-isolated Chromium sessions,
  encrypted profile snapshots, preserved intervention sessions, and exact
  receipts.
- Durable orchestration: Temporal workflow allocates a browser, runs the
  application, waits up to 24 hours for interventions, supports resume signals,
  caps repeated interventions, and records final state.
- Idempotency and retries: queue requests, activity retries, resume requests,
  and browser-step results use durable ids so a completed submit action should
  not rerun or charge twice.
- Receipts/evidence: receipt bundles keep job snapshot, resume version,
  final answers, verified claims, documents, events, final URL, screenshots keys,
  runner, adapter, and a deterministic fingerprint. Local machine paths are
  stripped before account storage.
- Network guardrails: top-level navigation blocks private, loopback,
  link-local, carrier-grade NAT, and credential-bearing URLs.
- Product surface: portal exposes applications, interventions, receipts,
  local/cloud run choices, email-code approval, and takeover affordances.

## 2. Better Automation UX/Product Ideas Worth Adopting

- Review-first mode as a first-class setting. Jobscan publicly frames control
  as "review and approve before every submission"; Bluey should make this a
  prominent mode beside Auto-submit, not only an intervention fallback.
- Daily match/application digest. JobCopilot and LoopCV emphasize daily recaps
  and dashboards. Bluey should send a digest with discovered matches, queued
  runs, submitted applications, failed runs, interventions waiting, and
  receipts ready.
- Search loops. LoopCV's "loop" model is easy to understand: one saved search
  continuously scans, filters, applies, and tracks. Bluey Career Tracks can
  adopt this vocabulary without copying the product.
- Match selectivity slider. JobCopilot exposes selectivity/balance controls.
  Bluey already has hard filters and thresholds; a user-facing strictness
  control would make automation safer and easier to tune.
- Company/title exclusion UX. Competitors expose excluded companies, keywords,
  salary floors, seniority, location, and remote filters. Bluey should ensure
  these controls are surfaced before any automated run.
- Application tracker density. Simplify, Huntr, Teal, LoopCV, and JobCopilot
  all sell "everything in one place." Bluey's receipts are stronger, but the
  tracker should make source, state, next action, date applied, resume used,
  email identity, and evidence visible at a glance.
- Answer training loop. JobCopilot says edited AI answers are remembered and
  future responses improve. Bluey has Answer Memory; product UX should make
  "save this answer for company/track/account" obvious after each intervention
  and review edit.
- One-click clipper/import path. Huntr, Teal, and Simplify win on quickly saving
  jobs from arbitrary browsing. Bluey should add a compliant manual "Save job
  URL / import posting" flow for user-provided public URLs, then route supported
  ATS pages through the same planning pipeline.
- Pause controls and kill switch. LoopCV highlights pausing auto-apply. Bluey
  should expose per-track pause, per-source pause, and immediate stop for queued
  but unsubmitted applications.
- Evidence-forward trust. Competitors mostly claim tracking; Bluey can
  differentiate with "show me exactly what was submitted" receipts, hashes, and
  final confirmation evidence.

## 3. Missing Technical Capabilities

- Production takeover gateway. Docs list browser takeover streaming and
  short-lived authorization URLs as an external gate. This is required before
  cloud interventions feel real.
- Provider credentials. Licensed job source credentials, Gmail/Outlook OAuth,
  R2/S3 upload, and Temporal Cloud production credentials are still launch
  gates.
- Live ATS certification. The adapters are fixture-backed; each supported ATS
  needs sandbox/live tenant certification across real multi-step variants.
- Broad direct employer form strategy. Bluey has a constrained semantic
  boundary, but public competitors claim broad company-career-page coverage.
  Bluey needs a measured expansion path with review-first default, not blind
  auto-submit.
- Job-board extension/clipper. There is no production browser extension or
  universal user-initiated clipper comparable to Simplify/Huntr/Teal.
- Status sync from employer email/calendar. Receipt structures support linked
  provider evidence, but production inbox/calendar sync is still gated.
- Search-loop scheduling UX. Discovery workers exist conceptually, but there is
  no complete public-facing loop scheduler with per-track cadence, pausing,
  quotas, and digest reporting.
- Queue governance. Bluey needs explicit per-day/per-week caps, per-company
  cooldowns, duplicate employer detection, old-posting limits, and source-level
  throttles visible to the user.
- Rich retry taxonomy. Temporal retries exist, but product and telemetry should
  distinguish transient network failure, ATS layout drift, missing answer,
  upload failure, ambiguous confirmation, challenge timeout, and submit-result
  uncertainty.
- Observability dashboards. Before dogfood, teams need per-ATS pass/fail,
  intervention rates, P50/P95 run duration, field-fill failure buckets, retry
  counts, receipt generation failures, and duplicate-prevention counters.

## 4. Claims Not Verifiable Publicly

These claims appear in public marketing or review pages, but the mechanism was
not verifiable without accounts, purchases, private dashboards, or non-public
docs:

- AIApply: how "applies to hundreds/thousands" is executed; whether it uses
  cloud browsers, local browsers, extensions, partner APIs, human operations, or
  mixed routing; how it handles CAPTCHA/2FA, retries, and receipts.
- ApplyBlast: exact public pricing, whether approval is always possible, what
  evidence users receive after each application, and how resumes are tailored
  per role. The public home page exposes little crawlable detail.
- LazyApply: the "profiles will never get blocked" claim and the full set of
  platforms/ATSs supported by its automation. Public pages do not show a
  compliant mechanism for avoiding platform enforcement.
- Sonara: whether it submits autonomously or autofills for user submit across
  all supported employers; public pages say it finds and applies, but do not
  explain challenge handling, receipts, or browser architecture.
- JobCopilot: the implementation behind "500,000+ company pages," whether
  submissions happen through cloud browsers or another method, and exact
  evidence retained after submission.
- LoopCV: cloud automation architecture, CAPTCHA/2FA behavior, and whether
  "employers see no indication of automation" reflects technical evidence or
  marketing framing.
- Simplify, Huntr, Teal: exact ATS-specific coverage and failure behavior for
  autofill on complex multi-step applications.
- Jobscan Auto Apply: precise ATS list beyond public examples, how pending
  review drafts are represented technically, and whether any cloud submission
  occurs after approval.

## 5. Ideas To Deliberately Avoid

- Do not bypass CAPTCHA, phone/app 2FA, assessments, site rate limits, or
  platform anti-abuse systems.
- Do not market hidden automation or claim that employers/platforms cannot
  detect automation.
- Do not automate restricted job boards against their rules. Existing Bluey
  policy treating LinkedIn and Indeed as handoff-only for background automation
  should stay.
- Do not auto-submit answers to sensitive questions unless the answer is
  confirmed by the user and policy permits it.
- Do not fabricate work authorization, sponsorship, clearance, demographic,
  disability, veteran, relocation, salary, or availability facts.
- Do not optimize for "hundreds per day" if it sacrifices match quality,
  accountability, or employer trust.
- Do not store raw OTP codes, job-site passwords, unencrypted browser profiles,
  or machine-local file paths in account receipts.
- Do not scrape prohibited pages or recommend account-creation workarounds.

## 6. Reliability Gaps

- Fixture confidence is not live confidence. Real Workday, Greenhouse, Lever,
  Ashby, and SmartRecruiters tenants vary by custom questions, optional modules,
  redirects, locales, file widgets, consent blocks, and confirmation patterns.
- Ambiguous confirmation remains risky. Bluey correctly requires confirmation
  text or URL for submitted state, but that will create review handoffs on
  employers with weak confirmation pages.
- Interventions can expire. Temporal waits 24 hours and caps repeated
  interventions; the product must make urgency and expiration clear.
- Cloud takeover is still an external gate. Without streaming takeover, cloud
  CAPTCHA/2FA becomes less useful than local browser takeover.
- Email OTP support depends on provider setup and message matching. Gmail and
  Outlook OAuth are gated; sender/domain matching may miss forwarded,
  localized, delayed, or branded verification emails.
- Resume/file upload failures need hardening. ATS upload widgets often include
  virus-scan delays, hidden file inputs, drag/drop-only components, and file
  type restrictions.
- Multi-step validation needs tenant variants. Required radio groups,
  conditional fields, legal acknowledgements, EEO pages, and duplicate-candidate
  notices can alter submit paths.
- Discovery dedupe may miss employer duplicates across ATS migrations,
  location clones, staffing reposts, or multiple boards pointing to the same
  application URL.
- Queue caps and fraud controls are not yet productized enough for dogfood.
  A broken search loop could otherwise create many low-fit interventions or
  duplicate attempts.
- Receipts need object-store production wiring and retention/deletion policy
  validation before customer use.

## 7. Proposed Acceptance Tests

Run each suite in both local Browser and cloud runner. Every test must assert:
no duplicate submit on retry, correct final state, exact receipt bundle, no
machine-local paths in stored receipt, correct application email/profile, and
preserved browser/takeover behavior for interventions.

### Workday

- Public discovery: bounded POST search against a pinned Workday tenant returns
  normalized jobs, rejects redirects/private hosts, dedupes repeated postings,
  and respects max posting age.
- Basic apply: single-step profile/contact/resume flow fills required fields,
  uploads resume PDF, submits, captures confirmation URL/text, and stores
  receipt.
- Multi-page apply: profile, experience, voluntary disclosures, review, and
  submit pages execute in order with validation after each page.
- Required radio/select: unanswered required radio group creates an
  `unknown_question` intervention; saved answer memory then resumes and submits.
- Candidate sign-in/email OTP: email verification pauses; matching inbox
  message offers explicit approval; raw code is not logged or persisted.
- CAPTCHA/assessment: challenge creates browser takeover and never attempts to
  solve or bypass it.
- Ambiguous result: if submit returns to a non-confirmation page, state becomes
  `needs_input`, not `submitted`.

### Greenhouse

- Public discovery: board token search normalizes departments, locations, and
  canonical URLs with host-pinning and dedupe.
- Standard form: contact, resume, cover letter, pronouns/phone, and required
  custom fields fill correctly.
- File validation: unsupported/missing resume path fails before submit with a
  blocking issue; no partial submission occurs.
- EEO block: optional demographic fields remain optional unless confirmed
  answers exist; required consent gets intervention if unknown.
- Duplicate candidate notice: duplicate/application-already-received copy is
  classified separately from successful submit.
- Confirmation evidence: receipt includes final Greenhouse URL, confirmation
  text, adapter version, documents, and screenshot key.

### Lever

- Public discovery: Lever site source returns normalized jobs and rejects
  malformed site identifiers.
- Basic apply: name, email, phone, resume, links, and comments fill and submit.
- Questionnaire: required yes/no and long-form questions use Answer Memory or
  stop for intervention.
- Conditional fields: answer that reveals a new required child field causes
  planning/validation before submit.
- Retry idempotency: simulated network loss after submit returns recorded result
  instead of clicking submit again.
- Manual takeover: CAPTCHA/security challenge preserves same page and resumes
  after user resolution.

### Ashby

- Public discovery: Ashby board source normalizes jobs and handles pagination
  boundaries.
- Basic apply: Ashby-hosted form uploads resume and fills contact fields.
- Structured custom questions: required multi-select, radio, and select fields
  map by label/value and stop on ambiguity.
- Consent/privacy checkbox: only checked when the label matches a safe required
  acknowledgement pattern; otherwise intervention.
- Confirmation detection: submitted only on explicit confirmation text or URL.
- Locale/format variant: phone, location, and salary fields do not silently
  mangle user-provided values.

### SmartRecruiters

- Public discovery: company identifier source normalizes title, location,
  department, canonical URL, and description.
- Basic apply: contact, resume, and initial questions fill; submit yields
  confirmation evidence.
- Account/login wall: sign-in-required flow becomes intervention/takeover, not
  credential collection.
- Additional documents: optional attachments are skipped unless packet includes
  a confirmed cover letter/attachment; required attachment stops if missing.
- Multi-step review: final review page is inspected before submit, with final
  answers captured in receipt.
- Timeout/retry: slow pages and transient 5xx responses retry within bounds and
  fail with classified issues after maximum attempts.

## 8. Recommended Pre-Dogfood Automation Priorities

1. Certify live ATS sandboxes for Workday, Greenhouse, Lever, Ashby, and
   SmartRecruiters with the acceptance tests above.
2. Ship review-first automation mode and make it the default for dogfood.
   Auto-submit should require stricter thresholds and explicit per-track opt-in.
3. Finish production browser takeover gateway for cloud runs, with short-lived
   URLs, session ownership checks, and audit events.
4. Wire object-store receipt upload, retention, tenant deletion, and receipt
   download/viewing before real submissions.
5. Add per-track search loops with pause/stop controls, daily caps, company
   exclusions, stale-posting filters, and a digest.
6. Build intervention and answer-memory review UX around "save for this
   company/track/account" so the system gets better without guessing.
7. Add observability dashboards for adapter pass rate, intervention rate,
   retry rate, run duration, receipt failures, and duplicate-submit prevention.
8. Harden email OTP through production Gmail/Outlook OAuth, message expiry,
   domain matching, and one-click approval audit trails.
9. Add a compliant manual job URL import/clipper flow for unsupported public
   employer pages, defaulting to review-first and handoff-only where rules or
   confidence require it.
10. Preserve the current avoid list: no CAPTCHA bypass, no 2FA bypass, no
    restricted-board background automation, no hidden automation claims, and no
    unconfirmed facts.

## Bottom Line

Bluey's technical foundation is already stronger than most public competitor
claims on identity isolation, intervention safety, idempotency, and receipts.
The competitive gap is packaging and proof: make automation feel continuous and
easy through search loops, review-first controls, daily digests, visible pause
controls, and dense tracking, then validate the five initial ATSs against live
tenant variants before dogfood.
