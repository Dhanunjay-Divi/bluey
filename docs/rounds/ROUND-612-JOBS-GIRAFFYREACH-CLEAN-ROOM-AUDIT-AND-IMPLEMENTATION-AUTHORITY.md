# Round 612 — GiraffyReach Clean-Room Audit And Bluey Jobs Implementation Authority

> **Codex preflight:** Load `$bluey-ops` before implementation or review. Treat the current
> repository and local replacement handoff as authority. Use the SSD archive only for one
> specifically missing historical fact.

**Audit date:** 2026-08-25

**Status:** AUDIT COMPLETE; IMPLEMENTATION NOT STARTED; NO DEPLOYMENT AUTHORITY

## Goal

Translate every materially observable GiraffyReach product capability into an original,
implementation-ready Bluey Jobs design. The result must explain where the jobs appear to come
from, how the user-facing loops work, what is proven versus merely advertised, which capabilities
Bluey already has, and which independently designed batches should follow.

This round is a functional decomposition, not source-code recovery. It does not copy a private UI,
decompile or reuse proprietary code, bypass authentication, probe undocumented endpoints, install
the competitor extension, submit an application, send outreach, create an account, or acquire a
private feed. Those steps are unnecessary to specify the product behavior and would not establish
safe production authority for Bluey.

## Authority And Evidence Rules

The audit used, in order:

1. the current Phase 611 Jobs branch and its local worktree state;
2. the July 24 replacement handoff for the role-taxonomy and location audit;
3. current first-party GiraffyReach pages and public app documentation; and
4. public browser-visible behavior and response metadata observed on 2026-08-25.

The SSD archive was not accessed because no specific historical fact was missing.

Evidence labels in this document have exact meanings:

| Label | Meaning |
| --- | --- |
| **Observed** | Visible in a public page, public app route, store listing, or response on the audit date. |
| **Claimed** | Stated by GiraffyReach but not independently demonstrated. |
| **Contradicted** | Two current first-party surfaces materially disagree. |
| **Unknown** | Authenticated behavior, provenance, algorithm, or performance cannot be established publicly. |
| **Bluey fact** | Proven by current code, tests, or a current round document in this repository. |

No GiraffyReach performance claim becomes a Bluey requirement without our own definition,
measurement, and evidence.

## Executive Answer

GiraffyReach is not one secret job-finding algorithm. Its public product decomposes into four
execution surfaces and several shared services:

1. a web portal for fresh-job discovery, matching, resumes, contacts, alerts, market intelligence,
   outreach, and tracking;
2. a Chrome extension that fills supported ATS forms and leaves final submission to the user;
3. C2C Autopilot, which turns recruiter-distributed contract requirements into tailored Gmail
   outreach; and
4. Agent Connect, a Streamable HTTP MCP surface for search, preparation, form guidance, contacts,
   progress, and market data.

Their public acquisition story has at least four source classes:

- employer career pages and ATS portals;
- job boards and other public/direct pipelines;
- curated or "hidden" leads; and
- recruiter email blasts, mailing lists, hotlists, partner/vendor circles, and recruiter channels
  for C2C work.

The homepage's "100% direct" and "zero aggregators" message is not a reliable source invariant.
The public Market pages say the index includes employer sites, job boards, and direct pipelines,
and expose Dice and LinkedIn categories. Bluey must therefore label source class and provenance on
every lead instead of presenting a broad marketing claim as technical truth.

Bluey already has the more difficult safety substrate: very large global lead ingestion, five
direct ATS adapters, original-source evidence, server-owned Career Track rules, preparation and
approval boundaries, durable receipts, ambiguity handling, reviewed mailbox/calendar contracts,
and a managed-cloud release authority. The important limitation is operational: current direct,
global, verification, browser, model, workflow, and provider-write paths are either separate from
the Phase 611 managed runtime or deliberately flag-disabled. Shipping must begin by binding and
proving those workers, not by adding an unsafe generic auto-submit switch.

## Public Product Contract

### Surfaces

| Surface | Publicly exposed behavior | Evidence level | Bluey interpretation |
| --- | --- | --- | --- |
| Web portal | Job feed, filters, alerts, match scores, resumes, contacts, tracker, market views | Observed/claimed | One browser-first Jobs portal with server-owned policy |
| Chrome extension | Reads profile and resume, fills ATS fields, uploads documents, handles intervention, never submits | Observed in first-party and store copy | Managed runner/provider adapters; local extension remains optional P2 |
| C2C Autopilot | Matches recruiter requirements, tailors a PDF, sends through Gmail, watches replies | Claimed | Separate consented recruiter-outreach product, not an ATS application |
| Agent Connect | MCP search, job detail, preparation, answers, recipes, contacts, progress, settings | Observed in public docs | Scoped Bluey agent API with read/prep first and no credential export |
| Opportunity Map | Resume skills weighted by current job demand with linked roles and companies | Claimed | Explainable skill-demand graph derived from verified job facts |
| Upskilling Engine | Daily skill gaps, learning items, portfolio links, engagement | Claimed | Evidence-linked recommendations; no invented credential claims |
| Market Pulse | 24-hour job, role, domain, skill, salary, remote, sponsorship, and seniority aggregates | Observed | Reproducible aggregate views with cohort, timestamp, and methodology |

Primary public sources: [homepage](https://www.giraffyreach.com/),
[features](https://www.giraffyreach.com/features),
[C2C Autopilot](https://www.giraffyreach.com/autopilot),
[Agent Connect](https://www.giraffyreach.com/mcp),
[extension](https://www.giraffyreach.com/extension),
[Opportunity Map](https://www.giraffyreach.com/opportunity-map), and
[Upskilling Engine](https://www.giraffyreach.com/upskilling-engine).

### Advertised End-To-End Journey

1. Create an account or use Google sign-in.
2. Upload and review a master resume/profile.
3. Set target titles, skills, locations, workplace, salary or rate, engagement type, experience,
   work authorization, and sponsorship preferences.
4. Receive fresh jobs and alerts or explore demand views.
5. Select a job and generate a job-specific resume, optional cover letter, and application answers.
6. Apply through an employer page, browser helper, agent, or C2C recruiter email.
7. Contact recruiters or hiring managers.
8. Track preparation, application, opens, replies, interviews, and offers.
9. Use observed demand and gaps to adjust targets, skills, and search strategy.

The authenticated dashboard, billing, resume editor, matching weights, contact provenance,
application receipts, live OAuth scopes, and actual submission behavior remain unknown.

### Plans And Public Limits

| Plan | Public monthly price | Publicly described boundary |
| --- | ---: | --- |
| Free | $0 | Eight sectors, jobs delayed 24 hours, basic tracker |
| Feed | $19.99 | Real-time last-24-hour feed, alerts, C2C/W2 filters |
| Advantage | $39.99 | Feed plus 60 tailored resumes/day, open alerts, one-click apply, full tracker |
| Dominance | $69.99 | Up to 100 resumes/day, C2C automation, contacts, LinkedIn content automation |

These are competitor observations, not recommended Bluey pricing. Agent Connect separately states
50 preparations/day and 25 contact lookups/day. C2C pages describe operational send ranges of
roughly 5–40/day while pricing advertises up to 100/day. Bluey must maintain one server-authoritative
entitlement and reservation contract per unit rather than repeat contradictory counters.

Sources: [pricing](https://www.giraffyreach.com/pricing),
[Agent Connect app docs](https://app.giraffyreach.com/agent-connect), and
[C2C Autopilot](https://www.giraffyreach.com/autopilot).

## How The Jobs Are Acquired

### 1. Employer Career Pages And ATS Portals

GiraffyReach claims to monitor more than 106,000 employer career pages, recrawl hourly, and surface
jobs in under an hour. Workday and Greenhouse are named on the homepage. Public extension and
Agent Connect material also names Greenhouse, Workday, Lever, Ashby, Workable, JazzHR, BambooHR,
Rippling, and iCIMS as form-recipe or autofill families.

That establishes a provider-family strategy, not the exact source registry, tenants, schedules,
contracts, parser code, success rate, or completeness. Bluey must independently enumerate only
sources it can lawfully query and prove each enrolled employer's original page.

### 2. Boards, Public Feeds, And Direct Pipelines

The Market methodology says postings are collected from employer sites, job boards, and direct
pipelines, then de-duplicated and AI-classified. The sponsorship view publicly broke a current
cohort into curated matches, direct employer jobs, hidden jobs, Dice contracts, and a LinkedIn
category. These observations disprove a literal all-direct interpretation without revealing the
private supplier list.

Bluey may ingest a board or partner only through an allowed API, licensed feed, user-authorized
import, or other reviewed source contract. A board lead is discovery evidence, never employer-side
submission authority. LinkedIn or Dice scraping must not be inferred or added from a marketing
label.

### 3. Curated And "Hidden" Leads

GiraffyReach presents "curated" and "hidden" jobs as source or classification buckets. Public pages
do not define their suppliers or exact rules. Bluey should use explicit names:

- `direct_employer`: the current record came from the employer's original ATS or career page;
- `partner_feed`: a licensed or contractually allowed feed;
- `public_board_lead`: a lead requiring original-source revalidation;
- `curated_lead`: a reviewed internal or user-provided lead;
- `recruiter_requirement`: a consented C2C message or channel item; and
- `unknown`: visible for diagnosis but ineligible for execution.

### 4. Recruiter Email And Network Requirements

C2C is a separate acquisition loop. The site says requirements arrive through recruiter email
blasts, mailing/network lists, hotlists, partner/vendor circles, and recruiter channels; refresh
about every five minutes; and are parsed for stack, rate, location, and recruiter contact.

Bluey should accept only a user-connected mailbox, an explicitly enrolled recruiter channel, or a
licensed partner input. Every message needs provider identity, account scope, consent source,
message/thread identifiers, sender/domain, receipt time, content hash, parsed facts, confidence,
retention class, and opt-out state. A recruiter email must never silently become a verified
employer job or an ATS submission.

### Observed Scale And Freshness

On 2026-08-25 the public Market page displayed 42,658 United States jobs in its trailing 24-hour
index, while the separate Live Jobs page displayed roughly 2,541. The public pages do not explain
the inclusion difference. Market views also showed salary, sponsorship, skill, industry, workplace,
and seniority summaries. Salary uses explicit posting text and annualizes hourly pay at 2,080
hours; sponsorship classification claims to use explicit posting text only.

Bluey must show an aggregate's query definition, source classes, deduplication version, freshness
watermark, and generated-at time. We must never imply that a teaser count and an all-index count are
the same cohort.

Sources: [Market Pulse](https://app.giraffyreach.com/market),
[Salary Board](https://app.giraffyreach.com/market/salary),
[Sponsorship Tracker](https://app.giraffyreach.com/market/sponsorship), and
[Live Jobs](https://www.giraffyreach.com/live-jobs).

## Authenticated Product Surface

The user signed in to a legitimate GiraffyReach account in Chrome and explicitly authorized a
normal user-side audit. The audit claimed the already-open tab and read visible page structure. It
did not inspect cookies, storage, tokens, passwords, browser history, or private APIs; connect an
external account; upload a resume; generate a document; change a setting; send a message; purchase
a plan; or prepare or submit an application.

### Route Inventory

The signed-in sidebar exposed these current routes:

| Product area | Authenticated route | Observed purpose |
| --- | --- | --- |
| Command Center | `/candidate` | Onboarding, daily metrics, activity, gap analysis, opportunity entry point |
| Insights | `/candidate/usa-heatmap` | USA heatmap, market intelligence, and opportunity-map navigation |
| Jobs Today | `/candidate/jobs` | Fresh feed, search/filter/sort, job detail, preparation and outreach entry points |
| C2C Autopilot | `/candidate/autopilot` | Beta recruiter-requirement automation |
| C2C Chat | `/candidate/chat` | Beta conversational C2C surface |
| Gov Contracts | `/candidate/gov-contracts` | Beta government-contract discovery |
| MCP | `/candidate/agent-connect` | Beta Agent Connect account and tool surface |
| Master Profile | `/candidate/profile` | Candidate truth, preferences, and resume-derived profile |
| AI Resume Builder | `/candidate/manual-generator` | Manual job-specific resume generation |
| LinkedIn Auto Post | `/candidate/linkedin-auto-post` | Content generation/scheduling for LinkedIn |
| Application Tracker | `/candidate/history` | Application history, status, and outcomes |
| Billing | `/candidate/billing` | Plan, trial, entitlement, and payment UI |

The Insights landing page also exposed `/candidate/skill-search` and
`/candidate/opportunity-map` as direct subproducts.

### Command Center

The Command Center is a career-operations home rather than only a job list. Observed modules were:

- a trial/plan banner and daily resume credit counter;
- three first-session actions: connect Gmail, connect LinkedIn, and optionally upload a resume;
- matched-job, saved-job, generated-resume, average screening-match, applied-job, and interview
  counters;
- Market Intelligence navigation;
- a daily activity streak and calendar;
- Skill Gap Analysis; and
- Opportunity Map.

The activity calendar exposed "toggle active state" controls for individual dates. Bluey should
derive activity from immutable user/application events and provide a correction flow, not allow a
gamified indicator to become the source of truth.

### Jobs Today

The authenticated feed exposed:

- a trailing-24-hour count, industry/domain selector, employment-type selector, source selector,
  fielded search, More filters, and Sort;
- company, title, normalized location, observed time, skills, experience, work mode, sponsorship,
  clearance, source/freshness badges, and pagination;
- a selected-job panel with the original employer link, bookmark, Tailor Resume, Outreach Intel,
  Giraffy Scope, Job Description, and Reach Insights;
- parsed seniority, salary, work mode, sponsorship, clearance, required and nice-to-have skills,
  education, likely duties, and watch-outs; and
- an explicit warning that the content was AI-read and the user should verify the full description.

The visible list demonstrated why canonicalization needs evidence rather than presentation-only
cleanup: examples included multi-location counts, duplicate country suffixes, prose-like hybrid
locations, and a board-shaped company identity. Bluey must retain raw source text alongside typed
canonical facts and confidence.

The right rail exposed "Giraffy Intel" with All, Layoffs, Hiring, Funding, and Industry categories
and a claimed 30-minute refresh. This is an event-intelligence stream separate from job discovery.
Bluey should not commingle company news with verified job state.

### Resume Configuration

The authenticated Resume Settings surface exposed these independent controls:

- change authority: Polish, Expand, or Full Rewrite;
- career stage: Early, Mid, or Senior;
- design themes including Classic Serif, Modern Clean, Minimalist, Startup Tech, Executive Navy,
  Academic, Compact Dense, Bold Rules, and Elegant Serif;
- PDF or Word output;
- ATS-Max mode, which removes a separate skills section and weaves skills into experience; and
- C2C/vendor mode, which keeps a detailed skills section and more verbose bullets.

The UI says Expand may add bullets for user-authorized skills and Full Rewrite writes a new resume
from the ground up. Bluey must preserve a stricter claims ledger: a selected skill may influence
emphasis, layout, and truthful phrasing, but it cannot create experience, ownership, duration,
outcomes, credentials, or tools that the candidate has not attested.

### Insights

The authenticated Insights navigation separated:

1. a USA heatmap with search, filters, top states, top skills, and top companies;
2. Market Intelligence/skill search; and
3. Opportunity Map.

This is valuable product packaging, but all three views can be computed from the same versioned,
provenance-aware aggregate store. Bluey does not need separate crawlers or duplicated truth for
each visualization.

### Authenticated Source Systems

The signed-in Jobs Today source selector and source cards resolved the mixed acquisition model
more clearly than the public homepage. The current choices were:

| Source label | Authenticated description/behavior | Current observed volume |
| --- | --- | ---: |
| First to Apply | "Caught in minutes"; direct employer links frequently point to Workday/Greenhouse | 22.8k all-domain card; about 3.35k Software route |
| Curated | "High reply odds"; a mixed employer/staffing/board-shaped set | 14.5k all-domain card; about 5.85k Software route |
| Tech Giants | Source-selector cohort without a separate source card | 0 Software results when inspected |
| Exclusive Contracts | "Recruiter emails inside"; direct email contacts and C2C Auto Apply | about 2.16k |
| Dice Contracts | Direct links to Dice; marketed as roughly five minutes after posting | about 1.70k |
| LinkedIn Contracts | Direct links to LinkedIn job pages | about 1.54k |

Counts changed during the audit as the live window advanced. They are observations, not stable
benchmarks.

The authenticated USA Heatmap independently showed 14,712 Software jobs and this source split:

```text
First to Apply          3,415
Curated                 5,906
Exclusive Contracts    2,156
Dice Contracts          1,703
LinkedIn Contracts      1,532
```

That exact sum is 14,712. The source labels are therefore first-class product cohorts. "First to
Apply" is still not proof that every item is direct: its visible list included a board-shaped
publisher, and Curated mixed original-employer, staffing, and board-shaped records.

The source-specific views expose the common normalized job model, but different actions:

- First to Apply and Curated provide the original job link, tailoring, and outreach intelligence;
- Exclusive Contracts provides a named recruiter/contact, Select All up to 100, Gmail, and Auto
  Apply through C2C Autopilot;
- LinkedIn Contracts links to LinkedIn job URLs;
- Dice Contracts links to Dice job URLs; and
- all views use the same parsed scope, salary, work mode, sponsorship, clearance, skills,
  education, duties, and watch-outs.

### Authenticated Market Intelligence

The USA Heatmap showed jobs on the map, remote jobs, all 50 states plus DC, update age, top states,
source split, and a separate count for remote/country-only records that could not be drawn. Filters
covered domain, employment type, source category, maximum experience, sponsorship, and clearance.

A state drill-down showed:

- top skills and counts;
- hiring companies and counts;
- normalized role labels and counts;
- Onsite, Hybrid, Remote, and Unknown work-mode totals; and
- a link back to the matching Jobs view.

The state list and opened state panel disagreed slightly on the California total during the same
session. Bluey snapshots must freeze one aggregate revision for the page so list, map, and detail
cannot use different watermarks.

Market Intelligence had three tabs:

- **Skills:** job counts for token-sensitive skills such as `C`, `R`, `Go`, `C#`, `C++`, `.NET`
  adjacencies, and longer phrases;
- **Companies:** company counts, which visibly exposed canonicalization defects such as
  `Unknown Company`, encoded-looking names, inconsistent casing, staffing firms, and job-board
  brands; and
- **Pulse:** sponsorship totals/reality check, salary board, top skills, and top companies.

This validates the original audit priority: the visualization is only as trustworthy as the
server's token-safe skill and canonical employer/location authority.

### C2C Autopilot

The authenticated C2C setup showed:

- three prerequisites: Gmail, resume settings, and phone verification;
- one or more weekday send slots in the user's local time;
- per-slot application count, a displayed maximum of 25/hour, and a 50/day product counter;
- target titles and locations;
- paused state until targeting exists;
- recent run history;
- no duplicate application to the same company; and
- automatic stop when the plan lapses.

Its Auto Reply manager is a materially sensitive subsystem. It offers fields for full legal name,
email, phone, authorization/validity, location, relocation, experience, availability, education,
rate, LinkedIn, Teams/Skype, C2C employer and account-manager contacts, last-four SSN, date of birth,
and notes. Document categories include Green Card, OPT, H1B, H4 EAD, driver's license, I-94, travel
history, and custom uploads. A checkbox can allow visa-status documents to be attached when a
vendor asks, and changes are described as auto-saving.

Bluey must **not** copy this broad automatic-disclosure boundary. Last-four SSN and date of birth
must stay outside generic recruiter automation. Identity/immigration documents require exact
recipient, purpose, document, content hash, expiry, and action-time user approval; text facts need
the same sensitivity classification and minimization.

### C2C Chat

The authenticated beta is a natural-language facade over C2C data/actions. Suggested tasks were:

- daily brief;
- apply to a bounded set of jobs by skill;
- find jobs with no apparent applicants;
- follow up with unresponsive vendors;
- recommend a rate;
- show hot titles; and
- explain capabilities.

It supports voice input and prominently says sends ask the user first. Bluey can offer the same
convenience through the scoped Agent gateway and reviewed communication commands; chat text must
never become submit/send authority on its own.

### Agent Connect

The authenticated MCP page confirmed:

- one account-private connector link with copy, generate, rotate/revoke instructions;
- Streamable HTTP and a separate Bearer-header example;
- profile/form-answer/phone prerequisites;
- 50 agent applications/preparations per day;
- shared daily resume credits;
- recent activity; and
- current marketing that the agent searches feeds and prepares applications.

The page says anyone with the connector URL can act as the user. This is exactly why Bluey must not
use a durable URL secret. It also says phone verification is needed before an agent can apply,
which conflicts with preview copy elsewhere that says agent submission is unavailable.

### Master Profile And Manual Resume Builder

The Profile Builder exposed these steps:

- resume upload or manual entry/scan;
- personal/preferences, including professional summary, work setting, relocation, and domain;
- work history, total experience, employer, title, location, dates, and bullets;
- education, GPA/details, and certifications;
- projects; and
- skills and user-defined skill categories.

The manual builder takes job title, company, optional location, optional job URL, and full job
description, then generates an optimized resume using the shared Resume Settings. It tells users to
download and review before submission.

The fuller Resume Settings also exposed:

- tracked DOCX opens and a claimed search boost;
- professional-summary enablement and styles;
- Skills modes: keep, tailor, or expand; category creation; number per category; and an
  "Auto-Add Missing Skills" switch;
- Experience, Education, and Certification AI-improvement toggles;
- explicit copy that experience years, education, and certifications come from Master Profile;
- detailed/brief output; and
- ten writing styles.

`Auto-Add Missing Skills` is unsafe if "add" means asserting a skill the candidate has not
attested. Bluey may add a job requirement to a gap list or suggested review, never directly to the
resume claims ledger.

### LinkedIn Workspace

The authenticated LinkedIn product contained:

- profile connection;
- AI Headshot Studio;
- headline, About, skill, and per-company profile optimization;
- a feed preview;
- manual ghostwriting and automatic scheduling;
- skill ideas from recently matched/applied jobs;
- multiple brand tones and content categories;
- AI optimization and an editable draft workspace;
- scheduled or immediate posting;
- optional GiraffyReach attribution; and
- interview/offer "win post" drafting.

Enabling Automatic Scheduler would authorize daily posting, using user topics or top skills from
recent matches. The audit stopped before enablement. Bluey should treat image generation, profile
rewrites, drafts, schedules, and publication as separate capabilities; every public post remains a
reviewed representational action through an official integration.

### Government Contracts

The authenticated beta says it uses official records and offers title/state intelligence:

- observed contractor counts;
- staffing companies to contact;
- real hourly rates;
- roles hired most;
- top public buyers;
- federal IT annual spend; and
- a generated action plan.

The page did not identify the exact dataset, update pipeline, contract identifiers, or methodology.
Bluey should not prioritize this until source rights, customer demand, procurement identifiers,
geographic coverage, update cadence, and rate semantics are independently specified.

### Tracker And Billing

The authenticated tracker offered search plus All, Opened, Saved, Applied, Interview, Offer, and
Rejected states. Empty state directs the user to generate a tailored resume or track a curated job.
Bluey already has stronger execution/ambiguity states; it should add these career-outcome views
without weakening receipt truth.

Billing confirmed the $19.99/$39.99/$69.99 plans and described the source, resume, open-tracking,
one-click, C2C, contact, and LinkedIn entitlements. Checkout actions were not opened.

### Authenticated Data-Quality Findings

The current product visibly exhibited:

- employer records named `Unknown Company`, encoded-looking company values, board publishers, and
  inconsistent casing;
- role names with punctuation/formatting defects;
- locations mixing canonical places, multiple-location counts, visa/relocation prose, travel
  requirements, and duplicate country suffixes;
- work-mode contradictions between job titles, locations, and parsed categories;
- duplicate company/job rows in Curated;
- salary strings left unparsed or inconsistently annualized; and
- sponsorship often displayed as "Maybe" where no explicit text was visible.

These are not reasons to copy their parser. They are acceptance fixtures for Bluey's canonical
evidence model.

## Complete Functional Decomposition

### Discovery And Search

Required Bluey behavior:

1. register every source with owner, source class, legal basis, authentication type, cadence,
   geography, provider family, rate limit, expected volume, parser version, and risk tier;
2. acquire a fenced lease and fetch only within the source contract;
3. store an immutable raw observation or content-addressed reference before normalization;
4. parse title, employer, source employer ID, location, workplace, engagement/employment type,
   compensation, experience, seniority, skills, education, work authorization, sponsorship,
   clearance, description, original URL, and timestamps;
5. attach per-field evidence spans, parser version, and confidence;
6. canonicalize employer, job, role, skill, and location identities without discarding raw text;
7. deduplicate by source identity first, then employer/job fingerprint, with explainable merge
   receipts;
8. revalidate the original employer source before preparation or execution;
9. publish account-visible matches and global aggregates from one verified fact contract; and
10. expire or quarantine records using explicit reason codes and watermarks.

### Matching

Bluey must separate eligibility from ranking.

Hard eligibility is server-owned and fail-closed:

- Career Track role and title policy;
- selected locations, relocation, travel, and workplace;
- employment and engagement type, including W2, 1099, and C2C;
- compensation floor and currency/unit compatibility;
- experience and seniority bounds;
- work authorization, sponsorship, and clearance;
- company conflicts, prior applications, and duplicate jobs;
- current original-source availability and freshness;
- source risk, employer risk, and scam risk;
- required profile facts and application claims;
- ATS/provider capability, runtime readiness, entitlement, and daily reservations.

Only eligible jobs receive a rank. A suggested starting model is explainable and versioned:

```text
rank = role_alignment
     + verified_skill_coverage
     + preference_alignment
     + compensation_alignment
     + source_confidence
     + freshness
     + candidate_feedback_adjustment
     - missing_fact_penalty
     - source_or_employer_risk
```

Weights are Bluey product policy, not an inferred GiraffyReach algorithm. Every displayed score must
link to included facts, excluded facts, model/rule version, and the reason a hard rule passed or
failed.

### Job Detail And Intelligence

One Bluey job detail view should include:

- original and canonical employer/title/location;
- original-source URL and last successful verification;
- source class, source path, discovery time, posting time, and freshness;
- required, preferred, candidate-present, and candidate-missing skills with evidence;
- compensation normalization plus original text;
- explicit/unknown sponsorship and work-authorization language;
- work mode, travel, seniority, experience, education, clearance, and engagement type;
- employer and scam-risk evidence;
- short role summary, expected duties, watch-outs, and unknowns;
- Career Track eligibility and rank explanation;
- prior company/job application collision;
- Save, Pass, Prepare, Review, and original-source actions; and
- bounded company events such as hiring, funding, or layoffs, clearly labeled as separate sources.

### Resume And Application Kit

The Bluey pipeline should be:

```text
candidate claims + selected baseline + verified job facts
    → requirement extraction
    → claim-to-requirement coverage map
    → user-selected change authority
    → truth-preserving draft
    → deterministic ATS and format checks
    → claim diff and unsupported-claim gate
    → PDF/DOCX/TXT render
    → immutable application-kit receipt
```

Required modes:

- **Polish:** wording and layout only; no new claim-bearing bullet;
- **Expand:** may split or add phrasing only where every claim is backed by an attested source;
- **Rewrite:** may reorganize the document but cannot change the claims ledger;
- **ATS:** deterministic machine-readable layout with explicit format diagnostics; and
- **C2C/vendor:** detailed skill matrix and engagement facts, still bound to candidate truth.

Cover letters, recruiter briefs, interview briefs, and outreach drafts must cite the same selected
job snapshot and claims ledger so different generators cannot invent conflicting facts.

### Application Assistance

Treat these as separate authority classes:

| Class | Permitted action | Required boundary |
| --- | --- | --- |
| Direct link | Open verified employer URL | Fresh original-source receipt |
| Local/browser fill | Read and fill certified fields | Exact domain/provider, field readback, user takeover |
| Managed-cloud fill | Same through a managed runner | Signed live runtime, account scope, intervention path |
| Final submission | Irreversible employer action | Explicit scoped approval and post-submit receipt |
| Recruiter outreach | Send a named message | Recipient consent/legitimate interest, content approval, caps |
| Agent preparation | Search and assemble an application kit | Scoped short-lived grant; no credentials or OTP export |

Unknown sites, unknown fields, CAPTCHA, authentication, sensitive self-identification, conflicting
answers, changed descriptions, stale jobs, or ambiguous effects stop for intervention. A generic
form recipe or an agent's claim that it clicked Submit is not submission evidence.

### C2C Autopilot And Chat

C2C is a recruiter communication workflow, not an employer application counter. The independent
Bluey loop is:

```text
consented message/channel
    → parse requirement and recipient
    → dedupe by message/thread/requirement/company
    → Career Track and C2C eligibility
    → verify sender/domain and required facts
    → build truthful vendor resume + draft
    → explicit policy/approval
    → reserve cap and schedule
    → provider send
    → durable provider receipt or side-effect-unknown
    → reply/opt-out/bounce suppression
```

Auto-replies may use only a versioned, pre-approved answer set for rate, location, engagement,
authorization, employer, and interview windows. Any new question, changed rate, commitment,
attachment, sensitive fact, or scheduling conflict requires review. Per-recipient memory must
include last contact, thread, attempts, reply, bounce, complaint, opt-out, and cooldown.

Bluey should avoid invisible open pixels by default. Prefer provider delivery/reply evidence and
consent-aware tracked links. Any optional open telemetry needs a disclosed purpose, recipient
policy, retention bound, and regional compliance review.

### Agent API / MCP

The useful public GiraffyReach tool families can be expressed as an original Bluey API:

- `search_jobs`, `get_job`, and `get_market_snapshot`;
- `get_candidate_capabilities` and `get_usage_limits`;
- `prepare_application`, `get_application_kit`, and `get_preparation_status`;
- `get_provider_capability`, `get_form_plan`, and `classify_unknown_field`;
- `report_form_progress` and `request_intervention`;
- `get_contact_options` and `prepare_outreach`;
- `get_resume_settings` and `propose_resume_settings_change`; and
- later, explicitly separate `approve_outreach`, `approve_calendar_action`, and
  `approve_submission` challenges.

Security contract:

- OAuth 2.1/DCR or a header-only, short-lived, audience-bound token;
- per-tool and per-Career-Track scopes;
- no durable secret in a URL;
- no password, session cookie, job-site credential, mailbox token, or raw OTP returned to an agent;
- least-privilege candidate facts and redacted logs;
- immutable command/request/response hashes and user-visible revocation;
- exact revalidation at irreversible boundaries; and
- read/search/preparation available before any write tool is enabled.

### Market, Opportunity, And Upskilling

Market aggregates must be derived from one canonical snapshot and include denominator and cohort.
Minimum dimensions are domain, role family, canonical role, skill, company, geography, workplace,
engagement, seniority, compensation, sponsorship, clearance, source class, and freshness.

Opportunity Map edges should represent measured relationships:

- candidate claim → canonical skill;
- skill → verified job demand;
- skill → role family;
- role → company/location/compensation cohort; and
- current profile → adjacent role through explicit missing skills.

Upskilling suggestions must show demand evidence, estimated gap, recommendation source, time/cost,
and how the user can demonstrate the skill. Completing content cannot automatically add the skill
to the candidate claims ledger; the user must attest or attach evidence.

### Tracker, Inbox, And Career Operations

The tracker must distinguish:

- discovered, saved, passed, prepared, review-required, approved, queued, and filling;
- intervention-required, final-review, submit-approved, submitted-confirmed, and
  delivery-unknown;
- recruiter outreach drafted, approved, scheduled, sent, replied, bounced, opted out, and paused;
- employer acknowledgement, assessment, interview, rejection, offer, withdrawal, and closure; and
- inferred inbox signal versus user/provider/employer-confirmed outcome.

Dashboard metrics come from those events. They may not be manually rewritten by a streak widget or
changed by keyword inference. Every count exposes its time window and definition.

## Current Bluey Jobs Truth

This section uses the Phase 611 tip as current authority. A capability being source-complete does
not mean it is deployed, enabled, bound to the managed release, or permitted to perform customer
effects.

The latest proven production baseline deployed the Jobs API, portal, root marketing, and continuous
direct/global discovery. It kept model generation, local Browser, managed-cloud Browser, and
mailbox sync disabled. Phase 611 is the newer source authority, not a deployment: it is still in
progress, all protected launch flags are parked, and exact-tip resource-heavy/hosted proof remains
incomplete.

### What Bluey Already Has

| Area | Current fact | Status |
| --- | --- | --- |
| Global leads | 49 source families, 47 non-empty, with 4,458,802 canonical leads from 5,096,589 raw rows and 63,487 companies in the 2026-07-21 evidence set | Source-ready; original-source revalidation still required |
| Direct discovery | Greenhouse, Lever, Ashby, SmartRecruiters, and Workday employer enrollment/discovery | Implemented as separate legacy worker path |
| Source evidence | Canonical job/employer/domain, original-source status/hash/freshness, and scam-risk evidence; unknown fails closed | Implemented contract; production verifier worker not yet in managed release |
| Career Tracks | Server-owned hard policy for role, workplace, engagement, pay, experience, authorization, freshness, duplicates, and execution readiness | Implemented, with taxonomy/location gaps below |
| Matches | URL-stable search, track, score, workplace, preparation, passed, and outside-rules controls | Implemented and tested |
| Resume/profile | Resume import, correction warnings, candidate claims, application-kit preparation, and truth/evidence checks | Implemented; managed model generation remains off |
| Application execution | Certified provider plans, exact form readback, interventions, explicit approval, receipts, and side-effect-unknown recovery | Source-complete; distribution flags remain off |
| Inbox | Gmail/Outlook read-only provider synchronization with encrypted state and evidence correlation | Source-complete; mailbox sync flag remains off |
| Communications | Reviewed Gmail/Outlook replies and calendar actions with exact authority and ambiguity handling | Source-complete; provider writes remain off |
| Managed cloud | Signed immutable Phase 611 release/activation/runtime/admission contracts | In progress; every release flag parked; no deployment authority |
| Web product | Browser-delivered Jobs portal including `/jobs/automation` | Implemented surface; managed runner unavailable until gates pass |

The global-source evidence is documented in
`docs/rounds/ROUND-559-JOBS-GLOBAL-INGESTION-AND-COMPLETE-RESUME-EXPORT.md` and the
subsequent discovery rounds. The direct-source directory and enrollment evidence is in
`ROUND-552`; original-source execution gates are in `ROUND-593`; the browser-first boundary is in
`ROUND-608`; and managed release truth is in `ROUND-611`.

### Critical Role, Skill, And Location Gap

The July 24 handoff correctly identified a split between broad browser suggestions and narrower
server authority. Current-code verification shows that the later Round 574 fix is **not** part of
the Phase 611 ancestry:

- commit `4bf33301` (`feat/phase-jobs-round574-normalization`) is not an ancestor of current main,
  the full-autonomy branch, or Phase 611;
- its own round document labels it feature-branch-only and not deployed;
- the current portal does expose role aliases and a 32,238-entry Census-backed location
  autocomplete in `jobs/portal/src/data/career-suggestions.ts` and
  `jobs/portal/src/hooks/use-location-suggestions.ts`;
- current resume tailoring has token-boundary phrase matching and alias groups in
  `server/src/db/jobs_tailoring.rs`;
- the current server role-family policy covers only five families, and its inference can combine
  the declared track role with the posting title instead of making the track the sole authority;
- current eligibility still matches skills with raw substring checks in
  `server/src/db/jobs/eligibility.rs`, so a short skill such as `Go` can match `Golang`;
- current Career Track upsert clones and stores caller values without one server canonicalization
  pass in `server/src/db/jobs/profile_postings.rs`;
- the current hard location check reads account preferences rather than `track.locations`; and
- eligibility location normalization strips case and punctuation but does not establish canonical
  city/state/country/metro semantics.

This is P0. The fix must be independently reconciled against current Phase 611 code rather than
merging the old feature branch wholesale.

Round 582's documentation says selected role/location policy is server authority, but current code
does not fully support that claim. The code and tests, not the older prose, control this audit.

Required invariant:

```text
UI suggestion or imported text
    → server canonical role/skill/location parser
    → versioned canonical IDs + raw text + evidence
    → same Career Track policy at match, prepare, approve, queue, and submit
```

Acceptance must cover token-safe short skills (`Go`, `R`, `C`, `C++`, `.NET`, `AI`), punctuation
aliases, plural/acronym variants, city/state abbreviations, country aliases, metros, remote regions,
multi-location jobs, user-defined exclusions, and version migration without silently widening a
Career Track.

### Phase 611 Operational Boundary

Round 611 explicitly says direct discovery, global discovery, and original-source verification are
separate legacy workers whose leases are not yet bound to a managed-cloud runtime session at the
pre-effect boundary. An importable Phase 611 activation requires all three capabilities false.
Browser distribution, workflow dispatch, workflow cleanup, model generation, mailbox writes, and
provider communication writes remain parked under their respective flags.

Therefore, a competitive screen or API cannot make an unavailable worker production-ready. New
UI must read the same release authority and explain unavailable/preview states truthfully.

## Bluey Versus GiraffyReach

| Capability | GiraffyReach public/authenticated state | Bluey current state | Decision |
| --- | --- | --- | --- |
| Broad fresh discovery | Large hourly/24h claims; mixed source classes | Much larger source-ready lead corpus plus five direct ATS families | Finish verifier/runtime binding; publish truthful freshness |
| Source provenance | Marketing and methodology conflict | Typed evidence and original-source gate | Keep Bluey's stronger contract |
| Career policy | Authenticated filters, unknown enforcement | Server hard rules, but current taxonomy/location defects | Fix P0 canonical authority |
| Market Pulse | Polished public/authenticated aggregates | Under-productized despite large corpus | Build one reproducible aggregate service and portal |
| Job detail intelligence | Rich parsed scope, duties, skills, watch-outs | Evidence-rich backend, less consolidated UX | Build evidence-forward detail view |
| Resume modes/themes | Broad user-facing controls | Stronger truth boundary, less productized choice | Add safe modes/themes over claims ledger |
| Browser help | Published extension; final Submit claimed manual | Certified plans/readback/intervention/receipts | Prefer managed cloud; optional local helper later |
| Agent/MCP | Public 17-tool beta with broad PII/OTP exposure risk | No equivalent public Jobs MCP product | Build scoped read/prep API, then challenged writes |
| C2C requirements | Recruiter/email network plus Gmail automation | Communications substrate but no C2C product loop | Build separate consented C2C pipeline |
| Contacts/outreach | Hiring contacts, emails, opens, replies | Reviewed provider-write authority but no contact product | Add licensed provenance and recipient policy |
| Opportunity/upskilling | Strong product packaging, unverified internals | Inputs largely exist | Build explainable graph after canonicalization |
| Gov contracts | Authenticated beta route | Not established as Jobs scope | Audit demand/source rights before prioritizing |
| LinkedIn posting | Authenticated automation route | No equivalent approved product | P2 only through official permissions and review |
| Tracker/inbox | Broad claimed funnel and telemetry | Stronger immutable receipts and provider ambiguity handling | Productize existing evidence |
| Cloud launch | Publicly appears server-side | Signed stack exists but deliberately not activated | Complete external and worker gates first |

Additional current gaps that affect parity:

- grounded AI cover letters/application kits and exact DOCX package fidelity are source-gated;
- OCR resume import is absent;
- no ATS family is certified and enabled for production auto-submit;
- production mailbox synchronization is disabled and provider writes are source-gated;
- no Jobs MCP/agent product or Chrome extension exists;
- conversion/experiment analytics are incomplete;
- paid self-service billing is intentionally unlaunched; and
- purge/cleanup and per-object cloud deletion proof remain source/external gated.

## Target Bluey Architecture

The right inheritance is product coverage and operational lessons, not the competitor's web stack.
Bluey should retain its existing Rust/TypeScript, PostgreSQL, workflow, portal, evidence, and signed
release boundaries and add the missing Jobs services behind those contracts.

```text
                        ┌──────────────────────────────────────┐
                        │ Source control plane                 │
                        │ registry · rights · cadence · health │
                        └───────────────┬──────────────────────┘
                                        │ fenced assignments
              ┌─────────────────────────┼──────────────────────────┐
              ▼                         ▼                          ▼
      Direct ATS workers        Allowed feed workers      C2C/mail workers
              └─────────────────────────┬──────────────────────────┘
                                        ▼
                         Immutable raw observations
                                        ▼
                   Parse · field evidence · canonical taxonomy
                                        ▼
                    Canonical jobs + source assertion ledger
                              │                    │
                              ▼                    ▼
                  Original-source verifier   Market aggregates
                              │                    │
                              └──────────┬─────────┘
                                         ▼
                      Career Track eligibility + ranking
                         │              │              │
                         ▼              ▼              ▼
                     Jobs portal    Agent gateway   Alert scheduler
                         │              │              │
                         └──────────────┼──────────────┘
                                        ▼
                         Truth-preserving application kit
                                        ▼
                 Review · approval · managed runner · provider receipt
                                        │
                        ┌───────────────┴────────────────┐
                        ▼                                ▼
                Tracker/inbox                    Recruiter outreach
                        └───────────────┬────────────────┘
                                        ▼
                         Career outcomes and learning loop
```

### 1. Source Control Plane

One registry owns source authority. A source definition requires:

```text
source_id, source_class, provider_family, organization_id, enrollment_proof,
legal_basis, allowed_operations, auth_reference, regions, cadence, rate_budget,
parser_version, expected_volume, freshness_slo, risk_tier, retention_class,
owner, status, effective_at, expires_at, predecessor_digest
```

Only signed/enrolled definitions may issue assignments. Workers use database-time leases,
generation/fencing tokens, bounded retries, conditional fetch metadata, source-specific budgets,
and durable run manifests. A source can be healthy for discovery while ineligible for execution.

### 2. Observation And Evidence Plane

A raw observation is immutable and account-independent:

```text
observation_id, source_id, source_record_id, acquired_at, observed_posted_at,
retrieval_status, canonical_url, content_digest, bounded_raw_reference,
headers_digest, parser_version, run_id, predecessor_observation_id
```

Every parsed fact is an assertion:

```text
entity_id, field, typed_value, raw_value, evidence_span_or_pointer,
source_id, observation_id, confidence, extraction_method, asserted_at,
supersedes_assertion_id
```

This supports correction and audit without storing an unbounded duplicate of every page in every
account. Sensitive recruiter messages use an encrypted account-scoped variant with strict
retention and deletion.

### 3. Canonical Identity And Taxonomy

Canonicalization is a server service/library shared by ingestion, Career Track writes, matching,
generation, market aggregates, and execution revalidation.

Required versioned entities:

- employer and employer domain/ATS tenant;
- job and provider/source identities;
- role family, canonical title, seniority, and title aliases;
- canonical skill, phrase aliases, acronyms, and tokenization policy;
- country, subdivision, city, metro, postal region, coordinates, and remote region;
- workplace, employment type, engagement type, compensation unit/currency, sponsorship,
  authorization, and clearance; and
- explicit unknown/ambiguous values with review reason codes.

Normalization must be idempotent, deterministic, Unicode-aware, token-safe, evidence preserving,
and versioned. A taxonomy upgrade writes a new interpretation and migration receipt; it does not
rewrite historical evidence silently.

### 4. Original-Source Verification

Before preparation and again before queueing, Bluey resolves the current employer authority and
produces:

```text
verification_id, canonical_job_id, employer_id, original_url, provider_family,
verified_at, expires_at, status, job_identity_digest, content_digest,
material_change_digest, source_risk, employer_risk, scam_risk,
execution_capability, verifier_release_id, receipt_digest
```

`unknown`, `closed`, `redirected_to_unknown`, `identity_mismatch`, `materially_changed`,
`source_untrusted`, or expired evidence blocks execution. A board or email can nominate a job but
cannot satisfy this receipt.

### 5. Matching And Learning

Matching consumes one Career Track policy, one candidate-claims snapshot, and one verified job
snapshot. It emits a `MatchDecision` containing hard-rule decisions, rank features, missing facts,
model/rule versions, and explanation. User Save/Pass/rejection feedback may adjust ranking only; it
cannot weaken eligibility.

Outcome learning distinguishes:

- source/provider availability;
- execution success;
- employer acknowledgement;
- interview/offer outcome; and
- candidate preference feedback.

This prevents an ATS outage, a poor match, and a hiring rejection from being collapsed into one
negative signal.

### 6. Generation Plane

Generation is an asynchronous, metered service with exact input/output hashes, claims-diff gates,
format-specific validation, reproducible render metadata, and bounded model/provider authority.
The user can select multiple baseline resumes and change authority per kit. Model output never
becomes a claim merely because it appears plausible.

### 7. Execution Plane

Provider adapters, the workflow gateway, managed runner, command dispatcher, and cleanup dispatcher
remain under Phase 611 signed release authority. The original-source verifier, direct discovery,
and global discovery become closed, separately fenced roles before activation.

Submission follows the durable states:

```text
prepared → review_required → approved → queued → filling
    → intervention_required → final_review → submit_approved
    → request_started → submitted_confirmed
                           ↘ side_effect_unknown → reconciled
```

No retry crosses `request_started` without provider-specific reconciliation.

### 8. Communication Plane

Mailbox sync, outbound provider commands, calendar actions, C2C requirements, and contact outreach
share account/provider authority but retain distinct permissions and quotas. Every send binds an
approved content hash, exact recipient/thread, attachment hash, provider connection, purpose,
cap, scheduled window, and opt-out policy.

Contact data needs source provenance, permissible purpose, freshness, correction, suppression, and
deletion. "Verified contact" is not an acceptable value without a verification method and time.

### 9. Agent Gateway

The Agent gateway is a policy facade over existing services, never a second source of truth. It
returns bounded resources and signed capability descriptions, enforces tenant and Career Track
scope, challenges sensitive/write actions, and exposes receipts. Tool documentation is generated
from the same capability registry as the portal and public plan copy.

### 10. Product Analytics And Consent

Bluey needs an event dictionary before dashboards:

- event name and version;
- actor/account/pseudonymous identifiers;
- source and purpose;
- required versus optional classification;
- consent category and region;
- retention and deletion behavior; and
- metric definitions and exclusion rules.

Optional analytics scripts must not load before consent. Application, mailbox, resume, and
candidate facts must never enter ad-tech payloads. Product counts derive from server events, not
client-only counters.

## Competitor Technical Signals

Public response metadata indicates:

- a prerendered marketing SPA/PWA on Google infrastructure, with a Vite-shaped first-party bundle;
- a Next.js App Router candidate app on Google Frontend/GCP;
- Stripe, Google/email authentication, Gmail/Outlook, and Composio claims;
- a Streamable HTTP MCP endpoint documented at `/api/mcp`;
- analytics/advertising assets for Google Tag Manager/Analytics/Ads, Meta Pixel, and Microsoft
  Clarity; and
- a broad Content Security Policy allowing many analytics/advertising domains.

Those observations are not reasons to migrate Bluey to Next.js or copy their deployment. Bluey's
current architecture has stronger release and effect authority. The useful lessons are browser-first
delivery, shared product surfaces, and a clear source-to-action funnel.

The public app documents both bearer-header access and a connector URL containing a token. Bluey
must use short-lived header-only grants or OAuth and redact all authorization material from URLs,
history, logs, referrers, and support artifacts.

## Public Contradictions To Avoid

| Topic | Conflicting first-party statements | Bluey requirement |
| --- | --- | --- |
| Job sources | "100% direct/zero aggregators" versus employer sites, boards, direct pipelines, Dice, and LinkedIn categories | Per-record source class and provenance |
| ATS auto-submit | Agent toggle/marketing says submit; preview copy and extension say user submits | One generated capability registry and truthful state |
| Automation unit | C2C emails called applications alongside ATS applications | Separate outreach, preparation, and submission counters |
| Tool count | Public pages say 11, 13, or 17; current docs enumerate 17 | Generate docs from deployed tool registry |
| Resume latency | Under 10 seconds, 10 seconds, and about 30 seconds | Publish percentile telemetry by operation |
| Daily limits | 40, 50, 60, or 100 depending on page and unit | Atomic named-unit reservations |
| Freshness | Free jobs delayed 24h while another page says jobs over 24h are purged | Explicit hot/delayed/archive cohorts |
| Feed volume | Public Live Jobs and Market counts differ greatly | Define every count and denominator |
| Releases | Changelog says May is latest despite July/August products | Release-generated changelog/capability truth |
| Refunds | Structured pricing copy and Terms differ | One billing policy source and checkout preflight |
| Security/consent | Marketing says optional trackers wait for consent; public loads suggested otherwise | Automated clean-profile consent tests |
| Success claims | Large hires, match, interview, uptime, and safety claims lack public methodology | No claim without owned evidence and date |

## Deliberate Non-Goals

Bluey will not implement:

- credential, session-cookie, mailbox-token, or OTP export to a generic agent;
- durable secrets embedded in connector URLs;
- anti-bot fingerprint evasion, honeypot bypass, or CAPTCHA circumvention;
- unlicensed LinkedIn/Dice/board extraction;
- autonomous submission on unknown sites or from generic field recipes;
- self-attested `submitted` state without employer/provider evidence;
- hidden or undisclosed recruiter tracking pixels;
- fabricated resume claims or credentials;
- ambiguous combined counters for emails, preparations, and applications; or
- public capability or performance copy ahead of deployed evidence.

## Recommended Successor Batches

Each batch starts from the reviewed predecessor branch. None inherits deployment authority merely
because this audit describes it.

### Round 613 — Canonical Taxonomy And Career Track Authority

**Purpose:** reconcile the useful Round 574 behavior into current Phase 611 code without merging
the stale branch wholesale.

Deliver:

- one server role/skill/location canonicalization library and version contract;
- token-safe skill matching and canonical role aliases;
- typed country/subdivision/city/metro/remote normalization;
- server normalization on Career Track create/update/import;
- exact `track.locations` enforcement at match, prepare, approve, queue, and submit;
- migration/readback for existing tracks with visible review on policy widening; and
- portal suggestions generated from or checked against the server taxonomy version.

Exit gate: all current suites plus exhaustive alias/boundary/location/property tests, with every
production flag unchanged.

### Round 614 — Original-Source Verification Managed Worker

**Purpose:** implement the production worker explicitly deferred by Round 611.

Deliver:

- closed worker entrypoint and exact role identity;
- fenced assignment/lease and database-time heartbeats;
- provider-specific direct-employer verification;
- immutable verification/material-change receipts;
- fail-closed preparation and queue rechecks;
- signed manifest, protocol, capability, and activation fields; and
- revocation, quarantine, circuit, retry, and ambiguity tests.

Exit gate: source verification can be true only in a successor signed release containing the exact
live worker; it remains false in every current activation.

### Round 615 — Source Control Plane And Freshness SLOs

**Purpose:** turn source-ready direct/global ingestion into one operated, provenance-first fleet.

Deliver:

- source registry, enrollment proof, rights, owner, cadence, budgets, and risk;
- scheduler assignments, fencing, manifests, conditional fetch, backpressure, and dead-letter
  review;
- parser/volume/freshness/duplicate/source-health metrics;
- canonical raw/field-evidence retention policy;
- board/partner/curated/recruiter source labels; and
- managed release binding for direct and global discovery roles.

Exit gate: one canary source per provider family proves freshness and rollback without enabling a
customer cohort.

### Round 616 — Market Pulse And Evidence-First Job Detail

**Purpose:** productize Bluey's corpus advantage.

Deliver:

- versioned 24-hour and historical aggregate snapshots;
- domain/role/skill/company/location/workplace/salary/sponsorship/clearance/source views;
- query definitions, denominators, freshness watermarks, and methodology;
- authenticated Today view and public-safe aggregate views;
- consolidated job detail with source path, raw/canonical facts, match explanation, watch-outs, and
  original-source state; and
- saved searches, alerts, URL-stable filters, and digests.

Exit gate: every aggregate reconciles to its canonical cohort and does not expose personal or
restricted source data.

### Round 617 — Resume Modes, Themes, Opportunity Graph, And Upskilling

**Purpose:** match the competitor's product choice while retaining Bluey's claims integrity.

Deliver:

- multiple resume baselines;
- Polish/Expand/Rewrite, ATS, and C2C modes;
- accessible PDF/DOCX/TXT themes and deterministic render verification;
- claim diff, unsupported-claim blocker, and user authorization review;
- requirement/skill/opportunity graph with evidence and freshness; and
- gap recommendations that never become claims without attestation.

Exit gate: adversarial generation tests prove no unsupported claim, credential, duration, outcome,
or skill is introduced in any mode.

### Round 618 — C2C Requirement Ingestion And Reviewed Autopilot

**Purpose:** create the separate recruiter-distribution product.

Deliver:

- consented Gmail/Outlook or partner-channel ingestion;
- encrypted message normalization, requirement parsing, sender/domain evidence, dedupe, and
  retention;
- C2C Career Track policy, rate/authorization facts, truthful vendor resume, and draft;
- exact daily send reservation, schedule, recipient cooldown, and opt-out/bounce suppression;
- reviewed replies and calendar proposals using the existing communication authority; and
- provider receipt/side-effect-unknown reconciliation.

Exit gate: no sending until provider approval, legal/abuse review, clean canary, and flags are
separately authorized. Invisible open tracking is out of scope.

### Round 619 — Scoped Jobs Agent API

**Purpose:** expose Bluey's strongest read and preparation capabilities to trusted agents.

Deliver in two releases:

1. search, job detail, market, profile capability, usage, preparation, kit, provider capability,
   contact options, and intervention tools;
2. separately challenged outreach/calendar/submission approvals only after their underlying
   customer paths are live and proven.

Use OAuth/DCR or short-lived header grants, schema-generated docs, tool-level scopes, account and
Career Track binding, bounded PII, receipts, rate limits, revocation, and security conformance.

Exit gate: no generic tool returns credentials, cookies, raw OAuth tokens, passwords, or OTPs; read
tools cannot mint write authority.

### Round 620 — Contacts, Career Intelligence, And Outcome Operations

**Purpose:** complete the career-operations loop.

Deliver:

- licensed/provenanced hiring contacts with correction and suppression;
- company hiring/funding/layoff events in a separately labeled intelligence stream;
- tracker, inbox, recruiter brief, interview brief, follow-up, and outcome views;
- provider/employer/user-confirmed outcome hierarchy; and
- metrics for source freshness, match quality, preparation, execution, response, interview, and
  offer without collapsing distinct stages.

LinkedIn publishing and government-contract discovery remain independent P2 decisions. They need
official source/integration rights, user review, target-customer evidence, and their own privacy and
abuse analysis before implementation.

### Round 621 — Managed Cloud Canary And Launch Decision

**Purpose:** bind the finished worker set to one signed release and establish actual production
evidence.

Deliver only with external authority:

- exact clean build and stored immutable artifacts;
- registry/provenance/SBOM/read-only-root evidence;
- protected-environment signatures and approvals;
- live PostgreSQL/Temporal/network/provider credentials;
- direct/global/verifier/API/gateway/worker/dispatcher/cleanup/runner quorum;
- dark deployment, no-customer canaries, revocation and rollback drills;
- small explicit cohort, metric/alert/on-call evidence, and kill switches; and
- copy generated from the activated capabilities.

No previous round may switch a current release flag or claim launch.

## Acceptance Matrix

### Sources And Ingestion

1. An unenrolled, expired, rights-unknown, or revoked source cannot receive an assignment.
2. Lease expiry, generation change, or revocation prevents the old worker from publishing.
3. Exact run replay is idempotent; same identity with changed bytes creates a new observation.
4. Raw content/reference, parsed assertions, and canonical facts remain separately addressable.
5. Every displayed fact can name its observation, evidence, parser, confidence, and correction.
6. Board, curated, partner, direct-employer, and recruiter records never lose source class.
7. Duplicate merging is deterministic and emits a merge explanation.
8. Source lateness, volume collapse/spike, parse drift, error rate, and duplicate rate alert by
   provider/source, without auto-enabling another feed.
9. Deletion and retention remove restricted/account data without corrupting aggregate provenance.

### Taxonomy And Career Tracks

10. Server and portal agree on one taxonomy version or the write fails for explicit review.
11. Short skills and symbols match token/phrase boundaries and never raw substrings.
12. Aliases canonicalize consistently across profile, job, track, filter, resume, and aggregate.
13. City/state/country/metro/remote semantics are typed; raw text remains visible.
14. `track.locations` and exclusions are hard rules at every action boundary.
15. A taxonomy migration that widens or changes policy requires a visible user review receipt.
16. Unknown or ambiguous taxonomy values cannot silently broaden eligibility.

### Matching And Market

17. Every ineligible job has exact hard-rule reasons; a view filter cannot override them.
18. Rank features and weights are versioned, bounded, and explainable.
19. Feedback changes ranking but never eligibility, claims, or source truth.
20. Market counts reconcile to canonical job IDs for the named cohort and watermark.
21. Salary, sponsorship, clearance, and workplace aggregates distinguish explicit, inferred, and
    unknown values.
22. Hot, delayed, and historical cohorts have noncontradictory retention and display rules.

### Generation

23. Every generated claim maps to an attested candidate claim or is blocked.
24. Polish/Expand/Rewrite authority is enforced server-side, not only described in UI.
25. PDF, DOCX, and TXT outputs pass text, link, layout, accessibility, and ATS checks.
26. Generation retry does not remeter or switch input snapshots.
27. A materially changed job or profile invalidates the old kit before execution.

### Applications And Communications

28. Original-source verification is current at preparation and immediately before queue.
29. Unknown provider/site/field, CAPTCHA, login, sensitive answer, or mismatch stops for takeover.
30. Every filled value and selected option is read back before final review.
31. Final submission requires an exact, unexpired, job/kit/attempt-bound approval.
32. Request-start is durable before I/O; response loss becomes `side_effect_unknown`, never blind
    retry.
33. Submitted state requires provider/employer evidence or remains explicitly unconfirmed.
34. C2C messages use a separately approved recipient, content, attachment, purpose, cap, and window.
35. Reply, opt-out, bounce, complaint, cooldown, deletion, and account revocation each suppress new
    outreach.
36. Provider email/calendar writes remain off until live OAuth approval and canary evidence exist.

### Agent And Security

37. Read, prepare, communication, calendar, and submit scopes are mutually non-escalating.
38. Tokens are short-lived, audience-bound, header-only, rotatable, revocable, and redacted.
39. Credentials, cookies, raw OAuth material, passwords, and OTPs never cross the agent boundary.
40. Sensitive candidate facts are minimized per tool and never written to optional analytics.
41. Every write challenge names the destination, exact data/action, expiry, and resulting receipt.
42. Tool docs, portal state, plan copy, and runtime capability are generated from one registry.

### Operations And Launch

43. All new workers are exact signed Phase 611 successor roles with live fenced heartbeats.
44. Debug/test artifacts, mutable tags, missing provenance, or drift cannot activate.
45. A dark canary proves source, model, provider, browser, privacy, and rollback gates independently.
46. No flag, customer cohort, provider write, deployment, or marketing claim changes in the audit
    branch.
47. Hosted evidence, operational approval, and rollback authority remain external-only launch
    requirements.

## Measurement Contract

Bluey should publish and operate these definitions before marketing them:

| Metric | Required definition |
| --- | --- |
| Source coverage | Enrolled, legally usable, healthy sources by class/provider/region |
| Discovery latency | Original posting evidence time to first Bluey observation, with p50/p95/p99 |
| Freshness | Age of last successful original-source verification at display/action time |
| Canonical quality | Field precision/recall and unresolved/ambiguous rate on labeled fixtures |
| Duplicate quality | False merge and missed duplicate rate |
| Match quality | Hard-rule correctness plus ranked relevance on consented labels |
| Resume quality | Unsupported-claim rate, deterministic checks, user edits, and selection rate |
| Preparation | Time, success, intervention, stale-kit, and abandonment by provider |
| Submission | Confirmed, unknown, failed-before-effect, reconciled, and user-cancelled |
| Outreach | Sent, delivered, replied, bounced, opted out, complained, and meeting scheduled |
| Outcomes | Employer-confirmed acknowledgement, assessment, interview, offer, and closure |
| Reliability | Availability and latency by exact activated release and service role |

## Unknowns That Remain Unknown

Authenticated normal use closed the product-surface gap but did not—and cannot—prove:

- the complete employer/source registry, source contracts, licenses, or feed suppliers;
- exact crawl schedules per source, failure handling, or completeness;
- contact-enrichment and verification vendors;
- parsing, embedding, ranking, generation, or recommendation models and weights;
- database engines, internal queues, warehouses, or service topology beyond public signals;
- actual ATS form success, CAPTCHA/takeover behavior, or employer submission receipts;
- actual recruiter email delivery, reply, document-disclosure, complaint, or opt-out handling;
- live OAuth consent scopes and provider approval;
- performance, placement, hire, match, interview, uptime, or safety claims;
- authenticated data export/deletion proof; or
- production incident, rollback, abuse, and recovery behavior.

Bluey does not need those private implementation details. Every missing fact above has an owned
contract, test, metric, and external gate in the architecture proposed here.

## First-Party Source Index

Product and workflow:

- [GiraffyReach homepage](https://www.giraffyreach.com/)
- [Features](https://www.giraffyreach.com/features)
- [How It Works](https://www.giraffyreach.com/how-it-works)
- [Pricing](https://www.giraffyreach.com/pricing)
- [Live Jobs](https://www.giraffyreach.com/live-jobs)
- [C2C Autopilot](https://www.giraffyreach.com/autopilot)
- [Agent Connect marketing](https://www.giraffyreach.com/mcp)
- [Current Agent Connect documentation](https://app.giraffyreach.com/agent-connect)
- [Chrome extension](https://www.giraffyreach.com/extension)
- [Chrome Web Store listing](https://chromewebstore.google.com/detail/giraffyreach-auto-apply/anfocmglgjcodllpfboccehfdaamhjnk)
- [Opportunity Map](https://www.giraffyreach.com/opportunity-map)
- [Upskilling Engine](https://www.giraffyreach.com/upskilling-engine)
- [Changelog](https://www.giraffyreach.com/changelog)

Market methodology and current observations:

- [Market Pulse](https://app.giraffyreach.com/market)
- [Salary Board](https://app.giraffyreach.com/market/salary)
- [Sponsorship Tracker](https://app.giraffyreach.com/market/sponsorship)

Policy and company claims:

- [Privacy Policy](https://www.giraffyreach.com/privacy-policy)
- [Extension Privacy Policy](https://www.giraffyreach.com/extension/privacy)
- [Terms](https://www.giraffyreach.com/terms-of-service)
- [Acceptable Use Policy](https://www.giraffyreach.com/acceptable-use-policy)
- [Security](https://www.giraffyreach.com/security)
- [About](https://www.giraffyreach.com/about)
- [Creator Partner Program](https://www.giraffyreach.com/creator-partner)

The authenticated evidence was observed through the user's account on the `/candidate/*` routes
listed earlier. No secret URL, token, email, phone, name, profile value, or candidate document is
recorded in this document.

## Verification Performed

- Reviewed current local repository and Phase 611 authority without modifying `meeting-main`.
- Confirmed the replacement role/location handoff against current code and branch ancestry.
- Confirmed the stronger Round 574 implementation is not in current authority.
- Reviewed public first-party GiraffyReach product, market, pricing, policy, security, sitemap, and
  extension surfaces.
- Inspected normal authenticated user-visible routes for Jobs, all source cohorts, heatmap, market
  intelligence, Opportunity Map, C2C Autopilot, C2C Chat, Government Contracts, Agent Connect,
  Profile Builder, Resume Builder, LinkedIn workspace, tracker, and billing.
- Performed no purchase, signup, connection, upload, generation, send, schedule, application,
  feedback, rating, save, setting change, or submission.
- Accessed no SSD archive.

## Implementation-Agent Contract

Start with Round 613 only. Load `$bluey-ops`, use this Phase 612 branch and current Phase 611 tip as
authority, and verify the working tree before editing. Do not touch the dirty `meeting-main`
checkout, merge the stale Round 574 branch wholesale, enable a flag, deploy, contact a provider, or
claim customer readiness.

For each successor batch:

1. create the exact feature branch from the reviewed predecessor;
2. write a batch IMPL document and CHANGELOG entry;
3. implement one authority boundary at a time with paired SQLite/PostgreSQL behavior where
   applicable;
4. add adversarial unit, integration, migration, concurrency, security, and portal tests;
5. run formatting, strict lint/clippy, full Jobs tests/typecheck/build, privacy/provenance/schema
   gates, and `git diff --check`;
6. perform a line-by-line self-review and record source versus external-only evidence; and
7. keep every deployment/provider/write flag parked until separately authorized evidence exists.

## Decision

GiraffyReach is now understood well enough to build the complete functional surface independently.
Its strongest product advantages are freshness packaging, explicit source cohorts, approachable
market intelligence, C2C workflow focus, extensive resume controls, and agent/LinkedIn convenience.
Bluey's stronger foundation is provenance, server authority, review, immutable receipts, ambiguity
handling, and signed releases.

The correct plan is not a clone. It is to close Bluey's current taxonomy/location and managed
verification blockers first, then expose the existing corpus and safety substrate through a better
Today/Market/Job Detail product, followed by truthful resume modes, consented C2C, scoped agents,
and career intelligence. Managed-cloud launch remains closed until those exact source and runtime
authorities are proven.
