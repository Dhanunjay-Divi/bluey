# Round 528 - Bluey Jobs five-portal competitive audit

Date: 2026-07-16

Source branch: `codex/bluey-interrupted-asks-round519-20260712`

Source commit: `7b816343d` (the exact `origin/main` tip at audit start)

Status: research complete; implementation decisions and launch gates recorded;
no product code changed in this round

## Executive decision

Bluey should not become a high-volume clone of any one competitor. The useful
market pattern is consistent across the five products: remove profile-entry
friction, show useful jobs immediately, prepare a job-specific packet, make the
review boundary obvious, preserve application evidence, and keep repetitive
answers reusable.

Current Bluey mainline already has the harder trust foundations that several
competitor experiences obscure: server-authoritative hard filters, Review first,
job-specific resume versions, scoped Answer Memory, application-email isolation,
same-company collision protection, explicit capability labels, durable receipts,
and side-effect-unknown recovery.

The next launch work is therefore not a visual rewrite. The four P0 gaps are:

1. freeze the exact approved answers and cover-letter content into the committed
   packet so later Career Profile edits cannot change what a runner submits;
2. acknowledge runner dispatch durably before presenting an application as
   queued/running, with a visible recovery or re-credit path for failed launches;
3. prove a scheduled, licensed discovery source with source-health and stale-job
   reconciliation in production; and
4. certify provider-specific Greenhouse and Lever behavior before permitting
   unattended submission, then certify the remaining ATS families individually.

No historical Jobs worktree should be merged wholesale to obtain these changes.
They are scoped follow-on patches against current mainline.

## Research boundary

The audit used owner-authorized accounts and normal browser interaction. All
candidate data and job scenarios were fictional. No payment was made and no real
employer application was submitted.

The audit inspected visible UI, accessibility/DOM state, publicly delivered
frontend behavior, official public documents, and normal same-origin requests
caused by authorized UI actions. It did not invoke hidden state-changing APIs,
retain passwords or session credentials, or store OTPs/private mailbox content.
No CAPTCHA or 2FA challenge was encountered that needed owner intervention.

A synthetic PDF was prepared for resume-import checks, but the browser-control
file chooser boundary prevented attaching it. This is a test-harness limitation,
not evidence that a competitor's upload failed.

Sorce is an iPhone application rather than a web account product. Its journey was
therefore inspected through its public site, official App Store listing, public
terms/pricing, and visible reviews; no Sorce account was created.

Pricing and inventory counts are date-sensitive observations, not durable facts.

## Evidence labels

- **Directly observed** - visible in the normal authorized product journey.
- **Officially documented** - stated in an official product, terms, privacy, help,
  pricing, or app-store document.
- **Public-client contract** - behavior exposed to the normal browser client by
  an authorized UI action.
- **Responsible inference** - a bounded design inference from observed behavior.
- **Unknown/unverified** - not safely provable in this audit.

## Screenshot index

All captures were made on 2026-07-16 CDT. Images were reviewed before inclusion
and contain no email address, password, token, OTP, or private message content.

| Product | Evidence | Source journey | Screenshot |
|---|---|---|---|
| Tsenta | Public signup and activation framing | [tsenta.com](https://tsenta.com/) | [tsenta-signup-desktop.png](ROUND-528-JOBS-FIVE-PORTAL-COMPETITIVE-AUDIT.assets/tsenta-signup-desktop.png) |
| Massive | Personalized plan transition | [usemassive.com](https://usemassive.com/) | [massive-custom-plan.png](ROUND-528-JOBS-FIVE-PORTAL-COMPETITIVE-AUDIT.assets/massive-custom-plan.png) |
| Massive | Authenticated pricing | [usemassive.com](https://usemassive.com/) | [massive-pricing-authenticated.png](ROUND-528-JOBS-FIVE-PORTAL-COMPETITIVE-AUDIT.assets/massive-pricing-authenticated.png) |
| Sorce | Public mobile-product landing | [sorce.jobs](https://www.sorce.jobs/) | [sorce-landing-desktop.png](ROUND-528-JOBS-FIVE-PORTAL-COMPETITIVE-AUDIT.assets/sorce-landing-desktop.png) |
| Sorce | Official iPhone distribution | [sorce.jobs/download](https://www.sorce.jobs/download) | [sorce-app-store.png](ROUND-528-JOBS-FIVE-PORTAL-COMPETITIVE-AUDIT.assets/sorce-app-store.png) |
| AIApply | Authenticated plan boundary | [aiapply.co](https://aiapply.co/) | [aiapply-checkout-authenticated.png](ROUND-528-JOBS-FIVE-PORTAL-COMPETITIVE-AUDIT.assets/aiapply-checkout-authenticated.png) |
| ApplyBlast | Authenticated plan boundary | [applyblast.com](https://applyblast.com/) | [applyblast-checkout-authenticated.png](ROUND-528-JOBS-FIVE-PORTAL-COMPETITIVE-AUDIT.assets/applyblast-checkout-authenticated.png) |

## Product comparison

| Product | Fast activation | Application model | Review/trust surface | Observed price surface | Important caution |
|---|---|---|---|---|---|
| Tsenta | Resume-first setup promises matches and a draft packet by setup completion | Broad career-page/ATS coverage, tailored resume, cover letter, answers, optional approval | Edit/review and receipt language are visible | 25 lifetime free; observed public tiers around `$19`, `$39`, and `$99` with application allowances | Large public inventory and broad automation claims require source/ATS proof before Bluey mirrors them |
| Massive | Questionnaire shows a match count before account creation | Manual or Autopilot, curated jobs, custom documents, application visibility | User controls filters; terms allow submission without another approval in some modes | Authenticated view showed about `$33/mo` quarterly and `$67/mo` monthly tiers | OAuth redirect exposed sensitive credential material in the URL fragment; values were not retained |
| Sorce | Swipe-first mobile feed with a simple free allowance | Mobile AI form filling, document upload, screening answers | Success/failure tabs and claimed failed-credit refund | Free with in-app purchases; 40 free swipes/day publicly described | App Store reviews report omitted answers, pending/failure ambiguity, wrong-role matches, and credits charged on failures |
| AIApply | Long guided onboarding before a plan gate | Automated matching and application allowances | Visible product contract includes queue, packet review, answers, pause reasons, and modes | About `$49/mo`, `$99/mo`, and `$199/quarter` in the observed checkout | Junior selection produced seniority-inconsistent copy; default salary was not a hard filter; stale validation appeared |
| ApplyBlast | Long guided onboarding plus social-proof feed | Auto Apply, multiple resumes, tailoring, tracker, forwarded inbox | Review/auto controls and outcome categories are publicly described | About `$49/mo`, `$99/mo`, and `$199/quarter` in the observed checkout | Same seniority-copy mismatch as AIApply; sensitive demographics were collected before the plan boundary |

## Screen-by-screen journeys

### Tsenta

```text
Landing
-> Google account
-> resume upload
-> profile extraction
-> generated draft packet
-> matches
-> edit/review or optional approval
-> form runner
-> receipt and recruiter routing
```

**Directly observed:** account creation and the resume-first onboarding page.

**Officially documented:** per-role resumes/cover letters, open answers, ATS score,
form filling, login/upload handling, optional auto-approval, receipts, and broad
ATS/career-page coverage. Official references include
[Terms](https://tsenta.com/terms), [Privacy](https://tsenta.com/privacy), and
[AI disclosure](https://tsenta.com/ai-disclosure).

**Bluey lesson:** use the first real application kit, not an empty Matches screen,
as the activation moment. Bluey should keep its stronger Review-first and
capability gates.

### Massive

```text
Landing
-> goals/urgency/application-volume quiz
-> role, level, location, salary
-> pre-account match count
-> Google account
-> personalized plan
-> feed and hard filters
-> manual or Autopilot execution
-> application visibility and hiring-team messages
```

**Directly observed:** the full synthetic questionnaire, a `769`-match preview,
Google account flow, custom-plan transition, and authenticated pricing.

**Officially documented:** up to 200 monthly applications, custom documents,
sponsorship filters, application visibility, and Autopilot. The terms place
responsibility for application accuracy on the user.

**Public-client contract:** the OAuth return placed authentication/provider
credential material in the URL fragment. This is recorded as a sanitized security
finding only; no value was retained.

**Bluey lesson:** showing plausible, recent matches before the plan gate is strong.
The urgency questionnaire and high-volume framing should not override hard filters
or informed approval.

### Sorce

```text
Public site
-> App Store
-> iPhone onboarding
-> swipe feed
-> AI form fill and document upload
-> submitted, pending, or failed
-> credit/refund and application tracking
```

**Directly observed:** public landing, App Store listing, visible ratings/reviews,
and iPhone-only distribution.

**Officially documented:** approximately 850K users, 30M swipes, 4M jobs, 1M
applications, 40 free daily swipes, AI screening answers, and credit-based usage.
The counts are marketing claims and were not independently verified.

**Bluey lesson:** a mobile intervention experience can make applying feel simple.
The reviews show why Bluey must preserve exact answers, distinguish packet/run/
employer outcomes, and automatically resolve failed metering rather than leaving a
user with a charged pending state.

### AIApply

```text
Landing
-> automation preference
-> employment and stress questions
-> role, seniority, salary, employment type
-> timing, relocation, workplace, authorization
-> optional demographics
-> Google account
-> checkout
-> matches, packet, review/auto mode, queue and tracker
```

**Directly observed:** synthetic onboarding and authenticated checkout. A Junior/
Associate selection later produced copy about an extensive background and top-tier
roles; authorization validation also retained a stale error. A `$120k` default was
presented as non-strict.

**Officially documented/publicly delivered:** job suggestions, queues, approve/
reject reasons, packet review, preferred answers, hybrid/automatic modes, tailored
resumes, pause reasons, issue reporting, and an application mailbox.

**Bluey lesson:** preserve the breadth of onboarding but make every summary derive
from the values the user actually entered. Bluey's server hard filters must remain
authoritative rather than treating salary and seniority as motivational copy.

### ApplyBlast

```text
Landing and live-looking inventory
-> 18-step candidate questionnaire
-> role, seniority, salary, authorization and preferences
-> optional sensitive demographics
-> masked outcome/social proof
-> Google account
-> checkout
-> Reviewing, Applied, Interview, Offer, Rejected tracker
```

**Directly observed:** the synthetic 18-step onboarding, Google account permission
scope, and authenticated checkout. The onboarding repeated AIApply's
seniority-inconsistent copy and collected optional sensitive demographic data before
the plan boundary.

**Officially documented:** job-specific resumes, cover letters and answers;
self-updating tracker; hard filters; review and auto modes; multiple resumes; an
application email alias; and application/outcome categories. Terms describe up to
1000 applications/month, expiring credits, and a conditional interview guarantee.

**Bluey lesson:** the tracker taxonomy and visible packet are useful. Avoid
high-pressure volume, masked outcome claims, and sensitive-data collection until a
specific employer form actually requires the information.

## Observable state machines

### Competitor composite

```text
profile_incomplete
-> profile_ready
-> matches_available
-> packet_preparing
-> packet_ready
-> awaiting_review | auto_eligible
-> queued
-> running
-> needs_input
-> submitted | failed | pending_unknown
-> interview | rejected | offer | withdrawn
```

Several competitors compress or blur `packet_ready`, `queued`, `running`, and
`submitted`. Public reviews show that this ambiguity damages trust when credits are
charged or an application remains pending.

### Required Bluey model

```text
matched
-> preparing
-> needs_confirmation | awaiting_review
-> approved_packet_locked
-> queued_dispatch
-> dispatch_accepted
-> running
-> needs_input | side_effect_unknown
-> submitted | failed

submitted
-> employer_acknowledged
-> assessment | interview | rejected | offer | withdrawn
```

Packet preparation, execution, and employer outcome must remain separate. A mailbox
keyword alone must not create an employer outcome without evidence and correction.

## Public-client contract observations

No private endpoint was probed. The table records only contracts exposed by normal
authorized UI actions; endpoint paths and hidden server implementation remain
unknown unless officially documented.

| UI action | Observable public-client contract | Label | Bluey requirement |
|---|---|---|---|
| Google account creation | OAuth returns the user to the product and establishes an authenticated browser session | Directly observed | Keep tokens out of URL/query/fragment; rotate and store in secure cookies or server session |
| Massive OAuth completion | Credential-like values were present in the return fragment | Public-client contract | Add a regression test that Bluey auth redirects contain no access/refresh/provider token material |
| Multi-step onboarding | Values survive forward navigation and feed a generated summary/plan | Directly observed | Bluey already persists onboarding steps; retain authoritative final completion |
| Pre-account match preview | Massive rendered a numerical match count before account creation | Directly observed | Bluey may show a small verified public sample, clearly labeled, without leaking proprietary ranking logic |
| Checkout | Plan, cadence, allowance, and cancellation language are rendered before payment | Directly observed | Keep one truthful entitlement contract and no hidden application-volume assumptions |
| Application mode | Review/automatic choice changes queue eligibility and approval behavior | Officially documented | Server, not browser state, owns capability and eligibility |
| Resume/answer packet | Per-job documents and answers are previewable/editable before approval in leading flows | Officially documented | Hash and freeze the approved answer map, cover letter, documents, identity and job |
| Intervention | Paused work can request missing input or browser takeover | Officially documented | Preserve one-time scoped takeover and reusable Answer Memory with provenance |
| Outcome tracking | Products render pending/applied/interview/rejected/offer categories | Officially documented | Separate execution evidence from employer outcome evidence and permit user correction |

## Current Bluey mainline comparison

### Already implemented

| Capability | Current evidence |
|---|---|
| Durable PDF/DOCX baseline import, structured Career Profile, no-resume path, resumable onboarding | `jobs/portal/src/lib/documents.ts`; `jobs/portal/src/components/Onboarding.tsx`; `POST /api/jobs/onboarding/complete`; Round 525 |
| Review first recommended for the first five application kits, daily volume and auto-submit threshold | `jobs/portal/src/components/Onboarding.tsx` |
| Server-authoritative freshness, salary, location, employment type, sponsorship, excluded company/title, daily and company limits, match threshold and ATS capability | `server/src/db/jobs.rs` eligibility and attempt-reservation paths |
| Unknown sites are Review-only/handoff | `jobs/automation/src/policy.ts` |
| Job-specific resume, real version/diff, answers, application identity, cover-letter state, pause reasons and metering disclosure | `jobs/portal/src/views/ApplicationsView.tsx` |
| Explicit packet approval and queued-only runner selection | `jobs/portal/src/views/ApplicationsView.tsx`; `jobs/portal/src/views/BrowserView.tsx`; `server/src/api/jobs.rs` |
| Scoped Answer Memory with company over Career Track over account precedence | `jobs/automation/src/answer-memory.ts`; `server/src/db/jobs.rs` |
| Multiple verified application emails and track-level identity selection | `jobs/portal/src/views/SettingsView.tsx`; Jobs identity API/database methods |
| Same-candidate company collision protection across tracks, resumes and emails | `server/src/db/jobs.rs` |
| Typed receipts, object hashes, screenshots, durable checkpoints, leases and terminal `side_effect_unknown` recovery | `jobs/automation/src/receipts.ts`; `jobs/browser/src/local-checkpoint-store.ts`; `jobs/runner/src/run-checkpoint-store.ts`; `server/src/db/jobs.rs` |
| Source-health and freshness surfaces in Matches | `jobs/portal/src/views/MatchesView.tsx` |
| Honest request-only inbox/calendar beta | `jobs/portal/src/views/SettingsView.tsx`; Jobs integration API |

### Missing or incomplete

| Gap | Why it matters | Exact Bluey area |
|---|---|---|
| Approved answers are not an immutable execution payload | Queue execution reconstructs standard answers from the current mutable Career Profile before overlaying stored application answers; approved content can drift | `server/src/api/jobs.rs` queue/execution-answer/receipt-validation paths |
| Queue state precedes durable runner acceptance | Missing credentials or unavailable workflow gateway can leave a metered, stranded queue state | `server/src/api/jobs.rs` queue ordering and gateway dispatch |
| No ATS family is certified for unattended production | Greenhouse/Lever are provider-specific but still `beta_review`; remaining adapters depend substantially on generic form behavior | `jobs/automation/src/providers/greenhouse.ts`; `lever.ts`; `execute.ts`; `policy.ts` |
| Scheduled production discovery is not proven | Worker contracts exist, but licensed-source configuration, canaries, scheduled ingestion and removal reconciliation lack current live evidence | `jobs/workflows/src/discovery-worker.ts`; `discovery-provider.ts`; operations/deploy configuration |
| Import remains a client heuristic | Scanned PDFs, OCR, complex columns, extraction confidence and claim-level review remain limited | `jobs/portal/src/lib/documents.ts`; `Onboarding.tsx` |
| Cover-letter generation is not end-to-end | Packet UI is truthful, but persisted cover-letter content starts empty and tailoring controls are shallow | `server/src/db/jobs.rs`; `jobs/portal/src/views/ApplicationsView.tsx` |
| Employer outcome lifecycle is incomplete | Current states stop at submitted/failed and inbox/calendar is request-only | `jobs/portal/src/types.ts`; `SettingsView.tsx`; Jobs integration workers |
| Failed-run billing resolution is not complete self-service | A user needs visible retry/reconcile/re-credit semantics without support ambiguity | Jobs metering ledger, application events, Billing and Applications views |
| First useful result is not guaranteed | Onboarding has a sample kit/trust ramp, but production discovery may not deliver a recent real kit immediately | onboarding completion, discovery workflow, Matches activation |

## Better ideas for Bluey

1. **Application Kit activation:** after onboarding, prepare one recent, verified,
   Review-only kit with resume diff, answers, identity and match explanation. Do not
   enable Auto-submit from a marketing promise.
2. **Trust ramp:** require review for the first five completed kits per Career
   Track, then offer automation only after successful receipts and no unresolved
   facts.
3. **Answer fingerprinting:** every approved answer carries scope, source facts,
   normalized question fingerprint, confirmation time, expiration policy and the
   application packet hashes that used it.
4. **Visible source proof:** show source, original URL, publication age, last
   verification time, closed/stale reason and ATS capability before preparation.
5. **Three-layer status:** display packet status, runner status and employer outcome
   separately so `pending` cannot hide a failed or uncertain submission.
6. **Failure fairness:** if a paid/allowance event produces no usable packet, resolve
   automatically; if a packet is valid but the runner fails, preserve the packet and
   offer a free handoff/retry without double metering.
7. **Sensitive-answer timing:** request demographic/disability/veteran information
   only when a particular employer form asks for it, explain its use, and preserve
   `Prefer not to answer`.
8. **Pass learning:** passing a match can optionally update a specific preference;
   never silently mutate hard filters from a single rejection.
9. **Mobile intervention:** provide a compact secure view for CAPTCHA, 2FA,
   assessment, missing-fact, and browser-takeover decisions without exposing the
   full browser profile.
10. **Public compatibility matrix:** list Certified, Beta Review, Handoff and Blocked
    ATS/site families with a current verification date.

## Avoid

- Generic resumes reused across jobs or Career Tracks.
- Using multiple application emails as multiple candidate identities or a way to
  bypass company cooldowns.
- High-pressure countdowns, stress-based upselling, unverifiable interview counts,
  or live-looking inventory without source/freshness proof.
- Collecting sensitive demographics during generic onboarding.
- Calling a packet `submitted` because a browser clicked a button or an email
  contains a keyword.
- Automatically retrying after an employer-facing side effect may have occurred.
- Advertising unknown or `beta_review` ATS versions as certified.
- Putting selectors, ranking rules, prompts, fraud logic, provider credentials or
  privileged feature flags in public client bundles.

## Priorities

### P0 - before unattended beta expansion

1. Create an immutable approved packet record containing hashes of resume,
   cover letter, final answers, identity, canonical job and capability decision.
   Runners consume only that record.
2. Add a durable dispatch outbox/lease and gateway acknowledgement. `queued` and
   `running` must reflect durable runner ownership, not an attempted HTTP call.
3. Configure one licensed/sanctioned discovery source in production with scheduled
   ingestion, canonical deduplication, stale/closed removal and source-health canary.
4. Certify Greenhouse and Lever tenant matrices separately. Keep every uncertified
   variant Review-only.
5. Add automatic reconciliation for failed launch, unusable packet and ambiguous
   side effect without double metering.

### P1 - invited-beta quality

1. Deliver a real first Application Kit immediately after onboarding.
2. Add OCR/layout confidence and claim-by-claim import review.
3. Finish generated cover-letter controls and immutable packet storage.
4. Add pass reasons, issue reporting, failed-run resolution and mobile
   interventions.
5. Complete self-service Jobs plan purchase/cancellation and transparent allowance
   ledger.

### P2 - outcome intelligence

1. Complete Gmail/Outlook OAuth, encrypted refresh-token lifecycle, revocation,
   evidence ingestion and deletion.
2. Add employer outcome states with user correction and follow-up reminders.
3. Add a calm Today/next-action view only after the underlying queue and outcomes
   are real.
4. Publish a live compatibility matrix and provider canary history.

### Later experiments

- A swipe/mobile discovery mode, provided ranking explanations and hard filters
  remain visible.
- Preference suggestions derived from pass/outcome patterns after at least ten
  comparable applications, never silent automatic mutation.
- Interview analytics tied to immutable application packets and verified outcomes.

## Acceptance tests

### Approved packet integrity

- Approve a packet, then edit the Career Profile, Answer Memory and default email.
- The runner must submit byte-identical resume/cover-letter files and the same
  normalized answer map and identity that were approved.
- Any mismatch fails closed before browser interaction and does not double-meter.

### Dispatch and metering

- Kill the workflow gateway before queueing; the application must not appear
  running or become stranded without a visible recovery action.
- Crash before and after runner acknowledgement; one durable owner resumes work.
- Crash after submit click and before response; state becomes `side_effect_unknown`
  and no automatic resubmit occurs.
- Repeated approval, dispatch, retry and handoff events meter the unique packet once.

### Discovery and freshness

- A scheduled provider run creates normalized jobs with source identity, canonical
  URL, publication time and verification evidence.
- Repeated listings deduplicate across sources without collapsing distinct roles.
- Closed, removed or stale jobs cannot be prepared or queued.
- Source outage and stale-watermark conditions are visible in Matches and alerts.

### ATS certification

- Greenhouse and Lever each pass multi-tenant fixture, sandbox and authorized live
  canaries covering required fields, document upload, custom questions,
  intervention, validation, receipt evidence and schema drift.
- Unknown tenant/version changes capability to Beta Review or Handoff before a run.
- Workday, Ashby and SmartRecruiters remain Review-only until their own matrices
  pass; generic fallback never inherits certification.

### Onboarding and activation

- PDF, DOCX, scanned PDF and no-resume paths reach final review without losing
  progress on reload.
- Extraction confidence and missing required facts are visible and editable.
- First useful recent match and Review-only kit appear without a blank dashboard.
- Junior/seniority, salary, authorization and location summaries exactly match
  saved values; stale validation is impossible.

### Identity, company and answers

- Different application emails remain one candidate for company-collision rules.
- SDE and Data Engineering Career Tracks cannot send the wrong resume to the same
  company without an explicit collision decision.
- Company Answer Memory overrides Track, which overrides Account; changed source
  facts invalidate affected answers.
- Unknown legal/work-authorization answers pause rather than guess.

### Receipts and outcomes

- Every submitted application preserves canonical JD, exact documents, final
  answers, identity, capability, confirmation evidence, screenshot/object hashes,
  timestamps and metering event.
- Execution status and employer outcome are separately testable and correctable.
- Failed/ambiguous work has a visible no-double-charge recovery path.

### Privacy, accessibility and responsive behavior

- No auth token appears in redirect URL, browser history, telemetry, screenshot or
  error output.
- Sensitive demographic answers are optional, purpose-labeled and deletable.
- Desktop/mobile, light/dark, keyboard, focus, screen-reader, loading, empty,
  failure and resumed-session states pass visual and automated checks.

## Mainline and worktree reconciliation

At audit start, `HEAD` and `origin/main` both resolved to `7b816343`. The checked
out branch name is historical, but its commit identity is current mainline. Its
remote branch is six commits behind and must not be treated as a second source of
truth.

`/Users/uno/Downloads/cue-bluey-jobs` remains a shared, heavily modified evidence
worktree and was not cleaned, reset, reformatted, switched or merged. It contains
mixed experiments and is behind its own remote branch. Current product comparison
was performed against `/Users/uno/Downloads/cue` at the mainline commit instead.

Rounds 520-527 remain authoritative for branch convergence. No reviewed
launch-ready non-Sashreek behavior is known to be missing from main. The following
Sashreek-owned branches remain excluded and untouched:

- `origin/agent/agent-bridge`
- `origin/agent/agent-bridge-fixes`
- `origin/agent/meeting-frontend`
- `origin/agent/parakeet-stt`
- `origin/meeting-main`

Historical branch names are audit evidence, not a reason to bulk merge obsolete or
unfinished code.

## Outcome

Bluey already has a more defensible application contract than the competitor
experiences audited here. The market-leading opportunity is to combine their fast
activation and clear packet value with Bluey's stronger review, provenance,
eligibility, collision and receipt controls.

This round intentionally changes no runtime. The next implementation should land
the P0 packet-integrity and dispatch-acknowledgement fixes first, then a real
scheduled discovery source and provider certification. Visual expansion should
follow those proofs, not precede them.
