# Round 496 - Jobs Competitor Completeness And Robustness Gate

Date: 2026-07-11

Repository: `/Users/uno/Downloads/cue-bluey-jobs`

Branch: `codex/bluey-jobs-20260710`

## Executive Answer

Yes: the collected evidence is sufficient to design and build Bluey Jobs as a
more trustworthy and technically robust product than the publicly observable
AIApply and ApplyBlast experiences.

No honest audit can say that every competitor implementation detail is known.
Their private job-source contracts, crawlers, browser fleet, queue scheduler,
prompts, ranking models, retry fences, security controls, and production success
rates are not publicly observable. Those unknowns are not blockers. Bluey should
not copy hidden implementation details; it should implement a stronger,
measurable product contract.

The research now covers:

- public landing, feature, pricing, checkout, help, terms, and privacy pages;
- account creation and unpaid onboarding paths where authorized;
- AIApply's authenticated resume/LinkedIn import and Application Kit entry;
- ApplyBlast's complete public help-center inventory and dashboard screenshots;
- public browser-client routes and contracts without calling protected APIs;
- resume, cover-letter, answer, matching, queue, tracker, mailbox, and
  cancellation behavior described by first-party sources;
- the current Bluey Jobs portal, automation, runner, workflows, server, data
  model, operations documents, tests, and remaining release gates.

No payment, real employer submission, CAPTCHA bypass, 2FA bypass, protected
endpoint probing, or extraction of private credentials was used.

## Evidence Inventory

The competitive record is intentionally spread across specialized rounds:

| Round | Coverage |
| --- | --- |
| 480 | Product and UX across AIApply and additional job tools |
| 481 | Automation, ATS behavior, interventions, retries, and acceptance tests |
| 482 | Pricing, allowances, margin, abuse controls, cancellation, and retention |
| 483 | Terms, privacy, deletion, cookies, email/calendar, and document handling |
| 484 | Deduplicated P0/P1/P2 competitive roadmap |
| 485 | Account and portal inspection |
| 491 | Two-portal account/onboarding/checkout deep dive with screenshots |
| 492 | Authenticated recheck, Bluey UX comparison, and implementation handoff |
| 493 | Review boundary, hard filters, packet review, identity, and Answer Memory |
| 494 | Public client contracts, automation logic, source-level Bluey P0 audit |
| 495 | Exact application-to-interview Bluey coaching loop |

The strongest visual references are:

- AIApply product picker and checkout:
  `ROUND-491-JOBS-TWO-PORTAL-APPLICATION-FLOW-DEEP-DIVE.md`
- AIApply authenticated import/Application Kit and Bluey gap screenshots:
  `ROUND-492-JOBS-COMPETITIVE-RECHECK-AND-IMPLEMENTATION-HANDOFF.md`
- ApplyBlast Jobs, Tracker, Auto Apply, resume library, tailoring, cover letter,
  and subscription screenshots:
  `ROUND-494-JOBS-COMPETITOR-AUTOMATION-LOGIC-AUDIT.md`
- Bluey application-to-interview practice and coaching screenshots:
  `ROUND-495-JOBS-APPLICATION-TO-INTERVIEW-PREP.md`

## Current First-Party Sources Rechecked

AIApply:

- https://aiapply.co/
- https://aiapply.co/product
- https://aiapply.co/auto-apply
- https://aiapply.co/interview-answer-buddy
- https://try.aiapply.co/interview
- https://aiapply.co/privacy-policy
- https://aiapply.co/terms-of-service

ApplyBlast:

- https://applyblast.com/
- https://applyblast.com/get_hired
- https://applyblast.com/login
- https://applyblast.com/privacy
- https://applyblast.com/terms
- https://help.applyblast.com/en/

ApplyBlast's public help center currently exposes six articles across Welcome,
Dashboard, Account Management, Resumes, and Applications. The five substantive
product/account articles were already captured in Round 494; the remaining
Welcome article adds positioning but no additional product capability.

## Evidence Levels

| Level | What It Can Support |
| --- | --- |
| Verified UI | A visible control, flow, state, or page exists |
| Verified first-party documentation | The vendor publicly promises or discloses behavior |
| Verified public client | The served browser code expects a route or state contract |
| Inference | A likely implementation needed to support observable behavior |
| Unknown | Not responsibly knowable from public evidence |

Public client code can reveal route names and browser-side state. It cannot
prove how a private service discovers jobs, chooses a model, isolates sessions,
or prevents duplicate submissions.

## Final Completeness Matrix

| Capability | Competitor Evidence | Bluey State | Required Bluey Gate |
| --- | --- | --- | --- |
| Product chooser | AIApply asks Auto Apply, Resume Builder, or Interview Buddy first | Jobs opens directly into an operations product | Keep Jobs focused, but offer Resume, Find Jobs, or Prepare as first-session goals |
| No-resume start | ApplyBlast explicitly supports starting from work history | Manual profile entry exists but is not framed as a primary path | Ship a named no-resume Quick Start |
| Resume/LinkedIn import | AIApply exposes both; ApplyBlast supports uploads and cloud-picker scripts | Structured resume model exists | Add authorized imports with provenance and field-by-field review |
| Application Kit | AIApply makes a generated kit the activation moment | Packet review now shows real version, diff, answers, identity, capability, and metering | Make the first sample kit the center of activation |
| Multiple baselines | ApplyBlast supports multiple resumes and a starred default | Versioned resumes exist; track-specific baseline UX is incomplete | Add track-level defaults and explain which baseline each packet used |
| Truthful tailoring | Both claim job-specific tailoring without invented experience | Claim-aware resume versions and exact submitted-version evidence exist | Add claim-level edit provenance and a hard no-new-facts validator |
| Tailoring permissions | ApplyBlast exposes separate title, experience, skills, and summary controls | Enhance/preserve modes are broader | Add granular permissions only after claim provenance is complete |
| Cover-letter behavior | ApplyBlast exposes source, unchanged/style modes, and length; AIApply generates letters | Packet represents cover-letter state; end-to-end behavior remains partial | Make generated, uploaded, omitted, and employer-required states explicit |
| Discovery | ApplyBlast says it scans company hiring systems; AIApply exposes a job board | Public ATS discovery library exists but no scheduled production loop | Wire one real source end to end before claiming continuous discovery |
| Feed provenance | Neither vendor publishes source contracts or freshness proof | Server now owns pasted-job authority fields and availability starts unknown | Persist source proof, fetch time, content hash, and availability verifier |
| Public company pages | ApplyBlast publishes large company/job landing-page inventory | Not implemented | Treat as later SEO, only after source licensing and freshness are proven |
| Hard filters | ApplyBlast promises salary, location, seniority, and dealbreaker filtering | Server-side hard filters and atomic attempt reservations now exist | Maintain zero-violation tests and expose every applied rule in UI |
| Match explanation | Both show scores and reasons; calibration is undisclosed | Reasons and missing requirements exist | Separate hard eligibility from ranked relevance and calibrate by track |
| Rejection learning | AIApply public client supports rejection reasons and proposed preference edits | Answer Memory exists; job-decision feedback loop is incomplete | Add Approve/Pass reasons with explicit preference-change confirmation |
| Review/auto modes | Both describe manual/review and automatic modes | Review boundary is enforced; unsupported sites stay review/handoff | Auto-submit only after observed successful review packets and certification |
| Queue controls | AIApply exposes pause reasons; ApplyBlast exposes Auto Apply toggle | Per-run interventions exist; account/track pause remains incomplete | Add structured user, billing, source, mismatch, and incident pause states |
| ATS adapters | Vendors do not publish exact coverage or success by ATS | Five named adapters still share a generic form engine | Build provider-specific state machines and certify representative tenants |
| CAPTCHA/2FA | Both acknowledge checks or required intervention | Bluey pauses for CAPTCHA, 2FA, assessment, sensitive, or unknown questions | Preserve browser state and never claim bypass capability |
| Cloud/local execution | ApplyBlast documents server-side processing; AIApply markets background automation | Bluey distinguishes local and cloud runners | Add durable leases, heartbeats, TTLs, takeover, and multi-replica tests |
| Crash recovery | Not disclosed | Temporal marks uncertain side effects, but runner maps/locks are process-local | Reconcile crash-before/after-submit from durable evidence before retry |
| Attempt limits | ApplyBlast terms cap 1,000/month; AIApply may queue, cap, or delay | Atomic company/daily reservations now exist | Use user-time-zone periods, explicit release semantics, and abuse alerts |
| Submission receipt | Vendors show tracker states but not their evidence standard | Typed receipt checks, object evidence, hashes, and immutable grounding exist | Require 100% complete receipts and zero false Submitted transitions |
| Tracker model | ApplyBlast combines Reviewing, Applied, Interview, Offer, Rejected | Bluey has application state and evidence | Separate execution, job availability, and employer outcome dimensions |
| Inbox/mailbox | Both disclose generated application mailboxes; ApplyBlast forwards messages | Identity/mailbox data model exists; production workers do not | Prefer Gmail/Outlook OAuth first; add relay mail only with export/deletion controls |
| Calendar | Competitors imply interview tracking; implementation is not public | Calendar connection model and receipt-grounded prep exist | Add production OAuth, evidence grading, disconnect, and correction flows |
| Follow-up | Both market tracking/follow-up behavior | Not complete end to end | Trigger reminders only from verified submissions and employer outcomes |
| Interview preparation | AIApply has mock interviews and live Interview Buddy; ApplyBlast markets prep | Round 495 grounds prep in the exact submitted packet | Persist sessions, add multi-turn practice, and keep every answer evidence-linked |
| Live interview support | AIApply markets hidden screen-sharing behavior and real-time audio prompts | Bluey's existing interview assistant is technically stronger | Keep consent/recording-law guidance, truth constraints, and visible privacy controls |
| Mobile | AIApply markets mobile Interview Buddy; both use responsive funnels | Portal mobile prep and core views work | Prioritize intervention triage and interview practice, not mobile bulk setup |
| Localization | AIApply markets many languages and resume translation | Jobs is primarily English | Add locale-aware resumes/fields only after ATS and legal review by region |
| Pricing | Both mix subscriptions and usage/credit concepts | Free/Pro/Cloud model exists | Keep one allowance, reset, overage, pause, and refund story everywhere |
| Cancellation | ApplyBlast exposes cancel/pause behavior; both limit refunds | Bluey account/billing foundation exists | Show effective date, retained access, data consequences, and refund policy plainly |
| Data deletion | Both disclose deletion limits; ApplyBlast uses email requests | Bluey has account export/delete foundations | Add Jobs-specific export/delete coverage and employer-side limitation copy |
| Model/data use | AIApply states minimum necessary prompts and no provider training | Bluey router and privacy model can enforce minimization | Publish provider classes, retention, training posture, and support access |
| Observability | Neither publishes ATS-specific reliability | Bluey has events, receipts, and operational docs | Report success, intervention, stale-job, and false-submit rates per adapter version |
| Marketing claims | Both use large counters and outcome claims that are not fully verifiable | Bluey has softened unsupported copy | Require metric owner, query, date window, and audit link for every numeric claim |

## Newly Found Or Underweighted Details

### 1. AIApply Is A Multi-Surface Career Suite

AIApply's first choice is not simply manual versus automatic applications. It
offers Auto Apply, Resume Builder, and Real-Time Interview Answer Buddy. Its
public suite also includes mock interviews, resume scanning, translation,
hosting, job board, LinkedIn import, and free content tools.

Bluey should not copy the breadth all at once. Its better loop is:

`profile -> verified match -> truthful packet -> controlled submission ->
evidence-backed outcome -> exact-application interview practice`

Round 495 now proves the last transition. That connected context is more
defensible than a collection of isolated generators.

### 2. Interview Privacy And Ethics Need Their Own Contract

AIApply's privacy policy says audio is captured on device, streamed to an STT
provider, raw audio is not retained, transcripts remain until deletion, and
providers may be selected for latency/reliability/cost. Marketing also says the
tool can remain hidden during screen sharing.

Bluey should be better by separating:

- practice from live assistance;
- raw audio from transcript retention;
- local capture from provider transmission;
- permitted coaching from prohibited misrepresentation;
- user consent from recording-law obligations;
- exact resume/job evidence from unsupported answer suggestions.

### 3. Public Metrics Are Not A Technical Contract

Current competitor pages contain counters for users, roles applied to, total
jobs, new jobs, response likelihood, and time saved. Different AIApply sections
show different user totals, and AIApply privacy has conflicting one-month and
three-month mailbox-retention language. ApplyBlast pages expose different job
and user totals across current and older conversion routes.

Some counters may be live, cached, scoped differently, or stale. Bluey should
never publish a number without a definition, source query, time window, and
owner. Product telemetry is not social-proof decoration.

### 4. ApplyBlast Uses Discovery As Acquisition

ApplyBlast publishes company/job pages and says it scans company hiring systems
directly. Those pages can generate search traffic and route a visitor into a
job-specific onboarding funnel.

This is worth a later experiment, not an immediate core requirement. Bluey
must first prove source rights, canonicalization, availability freshness, and
removal behavior. A large public index of stale or weakly sourced jobs would
damage trust.

### 5. Third-Party Import And Analytics Surfaces Expand The Data Map

Public ApplyBlast pages load Google Picker, Dropbox Chooser, and OneDrive
scripts. Both products use substantial analytics/advertising tooling. These
signals support cloud import and growth attribution, but do not prove private
backend behavior.

Bluey should use fewer trackers during beta, never place raw resumes, answers,
emails, screenshots, OTPs, or job-search history into analytics payloads, and
make every cloud import a narrow, revocable consent.

### 6. Referenced Terms Are Not Always Discoverable

AIApply's general terms say AutoApply and Interview Buddy have service-specific
terms, but the current general terms page does not expose direct links to those
agreements. This is a disclosure gap, not an implementation clue.

Bluey should place every controlling Jobs, automation, mailbox, and interview
term next to the feature and keep a versioned acceptance record.

## What Remains Unknown

The following cannot be established responsibly from the two websites:

- exact licensed feeds, ATS APIs, crawlers, and scraping schedules;
- job-source coverage, duplication, staleness, and removal error rates;
- ranking features, weights, calibration, and feedback-learning behavior;
- model vendors, prompts, fine-tunes, and routing for every feature;
- browser framework, profile isolation, worker fleet, proxy strategy, and
  multi-region topology;
- irreversible-submit fencing, restart reconciliation, and duplicate rates;
- real ATS-by-ATS completion, intervention, and false-submission rates;
- how a tracker declares Applied without an employer confirmation email;
- production staff access, tenant isolation, key management, and incident
  response controls;
- whether every advertised deletion, mailbox, and privacy control works for
  every plan and client;
- causal interview/offer lift versus self-selection and marketing attribution.

These are not reasons to scrape more aggressively. They are Bluey acceptance
tests and disclosure requirements.

## Bluey's Robustness Invariants

Bluey should be considered better only when these are measured continuously:

1. Hard-filter violation rate is exactly zero.
2. Unknown or uncertified sites never enter unattended submission.
3. A user-entered URL cannot self-certify source, score, freshness, or ATS
   capability.
4. Every employer-facing attempt has one atomic reservation and one durable
   side-effect state.
5. Crash after Submit never causes an automatic blind retry.
6. Submitted means a complete typed receipt plus independently stored evidence.
7. False Submitted and duplicate employer submission rates are exactly zero in
   fault-injection tests.
8. Every tailored claim points to user-approved source evidence.
9. Unknown, demographic, authorization, sponsorship, assessment, CAPTCHA, and
   2FA steps pause for the user.
10. Automatic outcomes require provider evidence and remain correctable.
11. Interview coaching uses the exact submitted job, resume, claims, and
    answers, never mutable current profile data.
12. Every privacy-sensitive integration supports disconnect, export, deletion,
    retention disclosure, and an auditable access trail.
13. Every public capability and number is generated from shipped behavior and
    an owned metric definition.

## Updated Delivery Order

### P0 Before More Automation Dogfood

1. Wire one scheduled discovery provider through source proof, normalization,
   dedupe, availability, matching, and portal health.
2. Replace the generic shared form engine with provider-specific Greenhouse
   and Lever state machines; keep both Review-only until certified.
3. Persist runner ownership, browser leases, heartbeats, intervention sessions,
   and side-effect reconciliation outside one process.
4. Complete production object-storage, malware/document checks, evidence
   upload, and receipt fault drills.
5. Add crash-before-submit, crash-after-click, response-loss, worker-restart,
   duplicate-delivery, and stale-job fault tests.

### P1 Before Invited Beta

1. Ship the no-resume/Quick Start path and first sample Application Kit.
2. Add Approve/Pass reasons and explicit preference-change proposals.
3. Complete question fingerprints and proposed -> reviewed -> confirmed Answer
   Memory lifecycle.
4. Add a Today view with queue/source health and one next best action.
5. Separate execution, availability, and employer-outcome states.
6. Implement Gmail/Outlook OAuth workers, evidence grading, disconnect,
   deletion, and correction.
7. Persist receipt-grounded multi-turn interview-practice sessions.

### P2 Before Public Launch

1. Certify Workday, Greenhouse, Lever, Ashby, and SmartRecruiters by adapter
   version and representative tenant fixtures.
2. Run canaries and publish a current compatibility/status matrix.
3. Complete Jobs-specific export, deletion, retention, provider, browser-cookie,
   and support-access controls.
4. Add evidence-backed follow-up, outcome tracking, and mobile intervention
   triage.
5. Validate accessibility, localization boundaries, billing/cancellation copy,
   and public metric provenance.

### Later Experiments

- licensed public company/job pages;
- optional Bluey relay mailbox;
- cloud-drive imports;
- multilingual resume/application packs;
- privacy-safe aggregate outcome benchmarks;
- live interview assistance with explicit consent and policy boundaries;
- offer and negotiation tracking.

## Explicit Non-Goals

- copying minified code, private APIs, prompts, or proprietary backend design;
- bypassing CAPTCHA, 2FA, assessments, anti-bot controls, or platform rules;
- bulk crawling without source rights and removal obligations;
- optimizing for application count instead of fit and verified outcomes;
- claiming exactly-once side effects without reconciliation evidence;
- guessing sensitive or legal answers;
- stealth interview behavior as a primary value proposition;
- publishing unverified jobs, user counts, interview lift, or urgency;
- calling generic selectors a certified provider adapter.

## Decision

No further competitor scraping is required before implementing the P0 core.
Additional public research should be triggered only by a specific product or
technical question, a changed first-party page, or a failed Bluey acceptance
test.

The implementation risk is no longer missing competitor ideas. It is trying to
ship too many surfaces before discovery, provider certification, distributed
execution, evidence, OAuth, privacy, and observability are production-grade.

## Next-Agent Prompt

```text
Repository: /Users/uno/Downloads/cue-bluey-jobs
Branch: codex/bluey-jobs-20260710

Read in order:
- docs/rounds/ROUND-494-JOBS-COMPETITOR-AUTOMATION-LOGIC-AUDIT.md
- docs/rounds/ROUND-495-JOBS-APPLICATION-TO-INTERVIEW-PREP.md
- docs/rounds/ROUND-496-JOBS-COMPETITOR-COMPLETENESS-AND-ROBUSTNESS-GATE.md
- jobs/ARCHITECTURE.md
- jobs/OPERATIONS.md

Do not perform more general competitor scraping. Implement the first P0
robustness slice from Round 496: wire one scheduled public ATS discovery source
end to end with server-owned source proof, canonical dedupe, availability,
matching, and truthful source health.

Preserve current shared-worktree changes. Do not weaken review-first, hard
filters, atomic attempt reservations, typed receipts, evidence persistence, or
the Round 495 server-authoritative interview-prep boundary. Keep every ATS
Review-only until a provider-specific state machine and certification suite
exist.

Required tests:
- caller input cannot set source authority, score, freshness, availability, or
  certification;
- redirects and unapproved hosts are rejected;
- retries are bounded and respect provider throttles;
- canonical duplicates collapse deterministically;
- stale/closed jobs cannot queue;
- source degradation is visible and pauses automation;
- discovery replay is idempotent;
- no raw resume, answer, email, or job-search text enters telemetry.

Write the next numbered round document. Do not commit or push unless explicitly
requested.
```
