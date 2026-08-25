# Round 612B — GiraffyReach Second Pass And Bluey Career Command Center

> **Codex preflight:** Load `$bluey-ops` before implementation or review. The current repository,
> local replacement handoff, and Round 612 are authoritative. Use the SSD archive only when one
> specifically identified historical fact is missing.

**Audit date:** 2026-08-25

**Status:** SECOND AUTHENTICATED PASS AND BOUNDED SYNTHETIC VALIDATION COMPLETE; READ-ONLY BLUEY
PRODUCT BOUNDARY DEFINED; NO DEPLOYMENT OR EXTERNAL-INTEGRATION AUTHORITY

## Purpose

Round 612 established the functional and source-system decomposition of GiraffyReach and the
current Bluey Jobs authority. This follow-up answers a narrower product question: what does a
normal signed-in user encounter when they move through the workflows, which interaction patterns
are worth adopting, and how should Bluey combine those patterns with its stronger provenance,
eligibility, consent, approval, and receipt model?

This is a clean-room product analysis. It records visible behavior and independently specifies a
Bluey experience. It does not recover or reuse competitor source code, assets, private APIs,
tokens, prompts, algorithms, or feeds. Similar user goals may produce familiar interaction
patterns, but Bluey owns its information architecture, copy, components, state model, and code.

The immediate implementation boundary is a read-only Career Command Center. It composes current
Bluey Jobs state into a calm daily operating view without adding any provider write, submission,
message, public post, credential, payment, or production authority. Round 613 canonical taxonomy
and Career Track enforcement remains mandatory before broader execution work.

## Authority And Evidence Labels

The second pass used, in order:

1. the current Phase 612 branch and Round 612 audit;
2. the local replacement handoff and current repository;
3. the already authenticated GiraffyReach browser session; and
4. browser-visible public documentation reached through normal product navigation.

The SSD archive was not accessed. No current fact required a historical fallback.

| Label | Meaning in this document |
| --- | --- |
| **Observed** | Visible in the signed-in UI or current public product documentation on the audit date. |
| **Claimed** | Product copy states the behavior, but the pass did not independently execute or verify it. |
| **Contradicted** | Current first-party surfaces give materially different behavior or limits. |
| **Unknown** | The UI does not establish the implementation, source right, retention, receipt, accuracy, or side effect. |
| **Bluey fact** | Proven by current Bluey code, tests, or current implementation-authority documents. |
| **Bluey decision** | An original product or architecture boundary selected for Bluey. |

Visual similarity is not evidence of backend parity. A connected-looking tile is not proof of a
valid OAuth grant, an `Applied` label is not proof of employer receipt, and an editable activity
calendar is not durable evidence of work performed.

## Bounded Test Actions And Actions Explicitly Not Taken

After the read-only pass, the user explicitly authorized a bounded functional test with synthetic
data. The audit created a one-page resume fixture that identified itself as synthetic and
instructed readers not to submit it to an employer. Through the normal UI, the audit uploaded and
parsed that fixture, kept the original after the recruiter-style audit, prepared one job-specific
resume without selecting or injecting missing skills, and observed the resulting tracker row. The
GiraffyReach account therefore retained one synthetic resume, one generated resume, one `Saved`
tracker item, and one consumed resume credit at the end of the pass.

The audit did **not**:

- grant Gmail, LinkedIn, or any other OAuth permission;
- choose an OAuth account or approve a requested scope;
- generate or rotate an MCP connector credential;
- accept the recruiter-audit rewrite or add an unverified skill;
- mark the prepared tracker item as applied or submit an application;
- select or execute a bulk-apply action;
- send an email, recruiter reply, chat instruction, or outreach message;
- create, schedule, publish, or auto-publish a LinkedIn post;
- enable C2C Autopilot, Auto Reply, or an automatic scheduler;
- attach or disclose identity, immigration, work-authorization, or rate documents;
- change a real candidate profile, resume policy, target, schedule, plan, billing method, or
  subscription;
- start checkout or make a payment;
- call undocumented endpoints, inspect cookies/storage/tokens, or bypass an access control;
- write to Gmail, LinkedIn, an employer, or any Bluey provider, production database, queue, or
  release environment;
- enable a Bluey production flag or deploy Bluey; or
- access the SSD archive.

These limits preserve the user's connected accounts and the evidentiary value of the audit. An
OAuth grant, external send, publication, employer submission, payment, or credential generation
would be a new external side effect and requires its own exact authority at action time. Removing
the retained synthetic account artifacts is also a separate account mutation and was not inferred
from authority to create the fixture.

## Executive Interaction Finding

GiraffyReach's strongest product choice is not a novel widget. It keeps many job-search activities
inside one signed-in workspace:

```text
readiness -> discover -> inspect -> prepare -> communicate -> track -> improve
```

The product repeatedly uses a few understandable patterns:

- a task-oriented Command Center instead of dropping the user into a raw feed;
- readiness cards that make setup dependencies visible;
- a dense job list beside a persistent job-detail pane;
- source, freshness, scope, and action badges close to the job;
- contextual settings near the action that depends on them;
- explicit selected-item state before a bulk action;
- guided tours for unfamiliar products; and
- desktop navigation plus a compact bottom navigation on narrow screens.

Bluey can adopt that interaction grammar with original components. Bluey should improve it by
showing source provenance, canonical-versus-raw facts, eligibility reasons, consent scope,
preparation-versus-send-versus-submit state, and immutable receipts. The goal is not a mirror; it
is a more trustworthy career operating system.

## Second-Pass Coverage Map

| Area | Interaction inspected | Evidence result | Mutation boundary |
| --- | --- | --- | --- |
| Command Center | Onboarding/readiness, counters, activity streak, gap/opportunity entries | Observed | No setup action completed; no streak changed |
| Jobs Today | Search modes, filters, split pane, selection state, Apply Tools, resume settings, one preparation flow | Observed | One resume generated; no bulk action, email, or employer submit |
| Master Profile | Section tabs, synthetic resume import, parser stages, audit, parsed fields | Observed | Synthetic fixture retained; AI rewrite declined |
| Application Tracker | Prepared-resume row, `Saved` status, download/view/mark-applied controls | Observed | `Mark as Applied` not selected |
| LinkedIn workspace | Five-step tour, manual writer, scheduler confirmation, attribution | Observed | No OAuth grant, draft save, schedule, or post |
| C2C | Autopilot prerequisites/schedule/caps/targeting, Chat prompts/voice, Auto Reply fields/documents | Observed/claimed | No chat turn, Gmail grant, schedule save, send, or attachment |
| MCP / Agent Connect | Public tool registry, connector model, submission descriptions | Observed/contradicted | No connector generated, copied, rotated, or used |
| Referral / roadmap | Navigation and product-program entry points | Observed | No referral sent or roadmap request submitted |
| Billing | Plan comparison, trial/entitlement presentation, checkout entry | Observed | No checkout, payment, or subscription change |
| Mobile | Narrow-screen shell, bottom navigation, desktop-only messaging | Observed | Navigation only |

## Detailed Interaction Findings

### 1. Command Center: Readiness Before Activity

**Observed:** the signed-in landing view presents onboarding as product readiness. Gmail,
LinkedIn, and resume setup appear as distinct first-session actions rather than one opaque setup
percentage. The page also groups plan/credit state, matched and saved jobs, generated resumes,
screening-match average, applied jobs, interviews, market intelligence, skill gaps, Opportunity
Map, and recent activity.

**Observed:** the activity streak calendar exposes per-day active-state controls. A user can
change the visual state of an individual date. The UI therefore treats the streak as editable
product state, not strictly as a derivation from verified activity.

**Unknown:** the pass did not establish how readiness is calculated, whether an integration tile
expires when its grant is revoked, whether counts reconcile to durable events, or whether streak
edits preserve an audit history.

**Bluey decision:** use an explicit readiness ledger. Each item should have a state such as
`not_started`, `review_required`, `ready`, `degraded`, `expired`, or `unavailable`, with evidence,
last-checked time, and a next action. Daily activity is derived from immutable application,
preparation, review, outreach, interview, and outcome events. A user correction is allowed, but it
creates a correction event; it does not rewrite the underlying evidence.

### 2. Jobs Today: Dense Discovery With Context In Place

**Observed:** the desktop Jobs experience uses a split pane. The left side remains a searchable,
filterable list while the right side shows the selected job's source link, parsed scope, job
description, reach/intelligence context, save action, resume action, and outreach entry points.
This reduces route switching while the user compares opportunities.

**Observed:** the search field has four explicit modes:

- `Title`;
- `Skill`;
- `Location`; and
- `Company`.

The mode is visible before a query is entered. This is clearer than asking users to guess which
fields a global text box searches.

**Observed:** selection is a first-class state. Apply Tools exposes `Selected / 0 jobs` before any
items are selected, and the bulk path makes Gmail a prerequisite. Source-specific views also
advertise bounded selection, including a visible limit of up to 100 on Exclusive Contracts.

**Unknown:** the pass did not execute bulk selection or establish whether selected state survives
filter, sort, pagination, session refresh, source refresh, or job deduplication. It also did not
prove that the displayed Gmail dependency maps to the minimum OAuth scopes or to an exact send
receipt.

**Observed:** Resume Settings are reachable near the job workflow. Visible controls include change
authority, career stage, themes, output format, ATS-Max, C2C/vendor mode, summary, skills,
experience, education, certifications, output detail, and writing style. This proximity is useful:
the user can understand the preparation policy before requesting a tailored document.

**Observed:** a bounded preparation test used Title search to select one public software role. The
detail pane showed a direct employer ATS link, parsed pay/work-mode/sponsorship/clearance data, one
strong skill, and a missing-skill set. `Tailor Resume` opened a Profile Gap Matcher with separate
controls to select skills, use only selected skills, inject all missing skills, or skip injection.
The audit chose `Skip & Optimize`; it did not select or inject any missing skill. The product then
replaced the preparation action with a `Download Resume` link and incremented the visible resume
counter by one.

**Unknown:** the generated document could not be independently inspected in this pass, so the
audit does not claim that its prose remained fully evidence-bound. The visible global policy was
`Expand`, which states that it may add a few bullets for authorized skills. Skipping the missing
skills establishes only that none were selected in the gap dialog; it is not proof that every
generated sentence was source-faithful.

**Bluey decision:** retain the split-pane and explicit search-mode ideas, but add an evidence rail:

- source class and original-source verification state;
- observed, verified, and stale timestamps;
- raw value beside canonical role, company, and location when confidence is limited;
- hard-rule eligibility with exact Career Track reasons;
- claim-diff and candidate attestation status before document generation;
- separate `prepared`, `approved`, `sent`, `submitted`, and `side_effect_unknown` states; and
- a stable selection manifest with canonical job identity, source revision, policy revision, and
  expiry.

A view filter or selected checkbox cannot grant application or communication authority.

### 3. Master Profile: Sectioned Truth And Bounded Import

**Observed:** the profile is organized into focused tabs/steps for resume import, personal and
preference facts, work history, education and certifications, projects, and skills/categories.
The separation keeps a large candidate record manageable and lets the product disclose where a
resume section gets its facts.

**Observed:** the import surface accepts PDF or DOCX and displays a 5 MB maximum. It offers a
resume scan/import path as an alternative to manual entry. The bounded test uploaded a clearly
labeled synthetic PDF through the normal file chooser. The UI reported completion of storage,
personal-info, work-experience, education/certification, project, and skill stages, then marked the
resume synced and parsed successfully.

**Observed:** the synthetic fixture produced a professional summary, total-experience value, two
work-history records, a bachelor's-level education record, and seven skills. Parsing was not
lossless: the education field of study and location were not populated, and the Projects section
was empty. The recruiter-style audit assigned an overall 7.3/10 and offered a one-click rewrite;
the audit selected `Cancel & Keep Original`.

**Unknown:** one synthetic fixture establishes workflow shape, not parser quality. Malware
handling, encryption, retention, deletion, model-provider disclosure, field confidence,
field-level provenance, overwrite behavior, recovery, and behavior across real-world layouts
remain unknown.

**Bluey decision:** preserve the tabbed profile pattern, but make the claims ledger authoritative.
An imported field is a proposal with file hash, page/section evidence, parser/model version,
confidence, sensitivity, and review state. Import must show whether it will merge, replace, or
create a second baseline before upload. The 5 MB competitor limit is an observation, not a Bluey
requirement.

### 4. Application Tracker: Prepared Is Saved, Not Applied

**Observed:** generating the bounded job-specific resume created one Application Tracker result.
The row showed the role, company, location, generation date, a `Saved` status, and controls for job
description, resume download, original job view, and `Mark as Applied`.

**Observed:** preparation alone did not produce an `Applied` label. The separate mark-applied
control remained available and was not selected. No employer site was opened for form completion,
no email was sent, and no employer receipt was created.

**Bluey decision:** retain that semantic separation and make it stronger. A generated document is
`prepared`; a local bookmark or tracker row is `saved`; a candidate assertion is
`user_marked_applied`; and a verified employer/provider result is `submitted`. The UI must show
the evidence source and cannot promote one state into another based on a download or button click.

### 5. LinkedIn Workspace: Guide, Draft, Confirm, Publish

**Observed:** a five-step tour explains the workflow in this order:

1. `Connect your LinkedIn`;
2. `Write your first post`;
3. `A small credit (optional)`;
4. `Post it`; and
5. `Turn on auto-posting (optional)`.

This sequence is effective because the optional attribution and automation choices are disclosed
instead of hidden inside the editor.

**Observed:** the manual ghostwriter exposes topic/idea input, brand tone and content-category
choices, AI optimization, a feed-style preview, and an editable draft. The workspace also offers
profile/headline/About/skill/company optimization, AI headshots, matched-job skill ideas, and
interview/offer win-post drafting.

**Observed:** the automatic scheduler is presented as a distinct mode. Enabling it triggers a
confirmation explaining that the product may create and publish daily posts using configured
topics or recent-job skills. The audit stopped at that boundary.

**Observed:** GiraffyReach attribution is optional and controlled by a visible toggle. Immediate
posting and scheduled posting are separate choices.

**Unknown:** no OAuth grant was made, so the pass does not establish LinkedIn permission scopes,
token lifetime, refresh/revocation behavior, posting API, retry/idempotency model, moderation,
image rights, analytics accuracy, or receipt semantics.

**Bluey decision:** any future LinkedIn capability requires an official integration and separates
five authorities: profile read, profile suggestion, draft generation, schedule creation, and
publication. Every public post is representational. It needs final content preview, target
account, scheduled time, attribution choice, attachment hashes, and an action-time approval or an
explicitly bounded recurring grant. Provider result and `side_effect_unknown` must be durable.
LinkedIn publishing remains P2 and is not part of Phase 612B.

### 6. C2C: A Separate Recruiter-Communication Product

**Observed:** C2C setup exposes three prerequisites:

- Gmail connection;
- Resume Settings completion; and
- phone verification.

**Observed:** C2C Chat is a separate conversational surface. It describes itself as able to find
C2C jobs, apply with tailored resumes, and tune Autopilot in plain English, while stating that
sends ask first. Visible starter prompts cover a daily brief, applying to ten Java jobs, finding
jobs with no applicants, following up with quiet vendors, rate guidance, hot titles, and a general
capability explanation. Voice input is present beside the text composer.

**Claimed:** the send-confirmation promise and the effects implied by those starter prompts were
not executed. No prompt was sent, so the audit did not establish tool routing, confirmation
granularity, resume/job selection, C2C recipient identity, action receipts, or whether a chat turn
can cross into Autopilot, email, tracker, or application side effects.

**Observed:** targeting and schedule are explicit. The UI offers weekday send slots in local time,
a count per slot, a displayed maximum of 25 per hour, and a 50-per-day product counter. It asks for
target titles and locations, remains paused until targeting exists, shows recent runs, states that
the same company will not receive a duplicate application, and says the automation stops when the
plan lapses.

**Claimed:** those counters, dedupe rules, plan stop, send cadence, matching quality, and reply
handling were visible in product copy but were not executed during the audit.

**Observed:** the Auto Reply manager includes materially sensitive candidate facts: legal name,
email, phone, authorization and validity, current location, relocation, experience, availability,
education, rate, LinkedIn, Teams/Skype, C2C employer and account-manager contacts, last-four SSN,
date of birth, and freeform notes. Optional document categories include Green Card, OPT, H1B, H4
EAD, driver's license, I-94, travel history, and custom uploads. The UI can permit visa-status
documents to be attached when a vendor asks, and describes settings as auto-saving.

**Unknown:** the pass did not establish the actual recipient population, mailbox scopes, per-field
encryption, retention, regional processing, recipient verification, opt-out/bounce suppression,
attachment review, audit trail, or the precise meaning of "application" in a recruiter-email
flow.

**Bluey decision:** C2C is never merged with employer ATS submission. A future Bluey C2C command
must bind one consented mailbox/channel, source message, recipient, purpose, rate/engagement facts,
content, attachments, schedule window, cap, cooldown, and user approval. Last-four SSN and date of
birth remain outside generic automation. Identity and immigration documents always require exact
document, recipient, purpose, content hash, expiry, and action-time approval. Phase 612B sends
nothing and connects no mailbox. A Bluey conversational command must render a typed, inspectable
plan before any write; natural language is input, not authority.

### 7. MCP / Agent Connect: Useful Tool Shape, Unsafe Credential Shape

**Observed:** current public Agent Connect documentation enumerates these 17 tools:

1. `get_candidate_profile`;
2. `get_limits`;
3. `search_jobs`;
4. `get_job`;
5. `prepare_application`;
6. `get_applications`;
7. `get_outreach_intel`;
8. `get_hiring_contacts`;
9. `get_resume_settings`;
10. `update_resume_settings`;
11. `get_verification_code`;
12. `mark_applied`;
13. `get_market_intelligence`;
14. `get_form_recipe`;
15. `get_application_answers`;
16. `classify_form_fields`; and
17. `report_application_progress`.

They fall into understandable product categories: candidate capability and limits, job discovery
and detail, preparation/history, outreach/contacts, resume settings, verification, market data,
form assistance, answer retrieval, field classification, and progress reporting.

**Observed:** the authenticated page presents Streamable HTTP, a connector URL, and a separate
Bearer-header example. It warns that anyone with the connector URL can act as the user and offers
generate, rotate, and revoke guidance. No credential was generated or copied during this pass.

**Contradicted:** current first-party descriptions disagree about irreversible behavior. One FAQ
answer says an agent can submit when the candidate profile's `submit_allowed` value is enabled. A
later FAQ answer says the preview does not actually submit and only prepares. The Chrome extension
documentation separately says the extension always leaves final Submit to the user. Public
surfaces also variously advertise 11, 13, or 17 tools while the current docs enumerate 17.

**Unknown:** the audit did not establish the deployed registry, exact tool schemas, scope checks,
credential lifetime, per-tool authorization, sensitive-field minimization, or whether any tool can
cause a provider side effect.

**Bluey decision:** generate agent documentation from the deployed registry, use OAuth/DCR or
short-lived audience-bound header grants, keep credentials out of URLs, scope each tool and Career
Track, and expose read/preparation before challenged writes. Bluey never returns a password,
session cookie, mailbox token, raw OAuth token, or OTP to a generic agent. Phase 612B exposes no
public MCP tool and generates no credential.

### 8. Referral And Roadmap Surfaces

**Observed:** the signed-in product includes referral and roadmap/product-feedback entry points.
They extend the workspace beyond the core job loop and provide clear places for invitation and
future-product interest.

**Unknown:** the audit did not send a referral, submit a feature request, vote, or verify reward
eligibility, payout, abuse controls, publication state, release commitment, or delivery date.

**Bluey decision:** referrals require their own fraud, attribution, privacy, reward, and disclosure
contract. A roadmap can communicate `exploring`, `planned`, `in_progress`, `available`, and
`retired`, but it must not imply release authority or a date that operations have not approved.
Neither surface is required for Phase 612B.

### 9. Billing: Entitlements Near The Product

**Observed:** Billing presents the user's trial/plan state and the current $19.99, $39.99, and
$69.99 monthly plan comparison. The visible entitlements cover feed freshness, resume volume,
tracking, one-click/application assistance, C2C, contacts, and LinkedIn features.

**Unknown:** no checkout was opened, so payment-provider fields, taxes, refunds, proration,
cancellation, renewal, failed-payment handling, entitlement timing, and receipt behavior were not
verified. Public limits elsewhere remain internally inconsistent.

**Bluey decision:** product surfaces may explain why an action is unavailable, but only the
server-authoritative entitlement/reservation contract grants a unit. Counters for resumes,
preparations, emails, posts, and employer submissions stay distinct. Phase 612B may display current
read-only plan/capability state; it does not add checkout or change billing.

### 10. Mobile: Useful Shell, Incomplete Product

**Observed:** at a narrow viewport the app exposes a compact bottom navigation for primary product
areas. Some complex product surfaces instead show a desktop-only limitation rather than a fully
responsive workflow.

This is a pragmatic release boundary, but it leaves setup, inspection, or recovery paths
inconsistent when a user is away from a desktop.

**Bluey decision:** use a bottom navigation only for the few highest-frequency destinations and
preserve a visible read/review path on mobile. A complex editor may remain desktop-optimized, but
mobile must still show state, evidence, pending approvals, exact blockers, and a safe deep link to
continue on desktop. No irreversible control should disappear without explaining its current
state.

## What Bluey Should Adopt

Bluey may independently implement these high-level interaction patterns:

| Useful pattern | Original Bluey form | Trust improvement |
| --- | --- | --- |
| Task-oriented home | Career Command Center with readiness, focus, pipeline, and evidence | Each tile has authority state and last-checked evidence |
| Explicit setup dependencies | Integration/document/profile readiness ledger | OAuth scopes, expiry, and revocation are visible |
| Split-pane jobs | Searchable list plus evidence-first detail | Original-source and raw/canonical facts stay in view |
| Search modes | Title, skill, location, and company selectors | Server canonicalization and query revision are explicit |
| Visible selected state | Stable review manifest before a bulk operation | Selection cannot bypass eligibility or approval |
| Contextual resume settings | Preparation policy drawer near the job | Claim diff and candidate attestations block invention |
| Guided integration tour | Explain read, draft, schedule, and publish separately | No broad "connected" state hides representational writes |
| Tracker/status chips | Career outcomes and intervention states | Provider/employer/user evidence hierarchy is preserved |
| Desktop plus bottom nav | Responsive command center and review surfaces | Mobile never hides pending approval or failure evidence |

Bluey will not reuse competitor branding, copy, icons, CSS, screenshots, DOM structure, private
content, or implementation code. Familiar controls such as tabs, cards, split panes, filters, and
confirmation dialogs are reimplemented within Bluey's own design system and accessibility rules.

## Bluey Career Command Center Contract

Phase 612B is deliberately read-only. It can compose existing authorized Jobs data into the
following original information architecture:

```text
Career Command Center
├── Readiness
│   ├── Career Track
│   ├── candidate claims / resume baseline
│   ├── source and original-verification capability
│   └── communication / execution availability
├── Today's focus
│   ├── recent active-Track matches not currently passed; eligibility not implied
│   ├── stale or changed-source warnings
│   └── interventions requiring the user's decision
├── Pipeline
│   ├── discovered
│   ├── eligible
│   ├── prepared
│   ├── approved
│   ├── submitted / sent
│   └── interview / offer / closed
├── Source health
│   ├── source class
│   ├── freshness watermark
│   ├── original-source verification
│   └── degraded / unknown state
└── Recent evidence
    ├── preparation receipts
    ├── provider receipts
    ├── ambiguity / side-effect-unknown
    └── user corrections
```

### Read-Only Means Read-Only

Within this batch:

- a card may navigate to an already authorized local page;
- a filter may change only the displayed local view;
- a disclosure may explain a capability, blocker, or next reviewed batch;
- a demonstration state must be clearly labeled and must not masquerade as production data;
- no control can send, post, submit, purchase, upload, grant, rotate, approve, or enqueue;
- no new server mutation endpoint or provider command is introduced;
- no flag is enabled and no current production gate is weakened; and
- no UI count becomes authority for eligibility, entitlement, or execution.

### State Semantics

The Command Center must never collapse unlike events into a single success number:

| State | Required meaning |
| --- | --- |
| Discovered | A source observation exists; no eligibility or freshness promise implied |
| Eligible | Current server Career Track hard rules passed at a named policy revision |
| Prepared | A candidate-bound kit exists; no send or submission implied |
| Approved | The user approved an exact version for an exact target and action |
| Sent | A communication provider returned durable evidence |
| Submitted | The employer/provider returned durable submission evidence |
| Side effect unknown | A timeout or ambiguity prevents a safe retry or success claim |
| Interview / offer | Provider, employer, or user-confirmed outcome with source label |

Readiness follows the same discipline. `Ready` means current typed evidence supports the check;
`Needs action` means the user can address a missing input; `Reported` means the workspace carries a
reference but this view lacks the authoritative read-back needed to promote it; and `Optional`
keeps a convenience such as mailbox correlation outside the required-check denominator.
`Unavailable` or `Review required` remains visible instead of being converted to optimistic green.

## Phase 613 Remains A Launch Blocker

Phase 612B does not resolve the current role, skill, location, or Career Track authority gap. Round
613 still must deliver:

- token-safe skill matching so `Go` cannot match `Golang` by raw substring;
- canonical role families and aliases from one server library;
- typed country/subdivision/city/metro/remote normalization;
- server canonicalization on Career Track create, update, and import;
- exact `track.locations` enforcement at match, prepare, approve, queue, and submit;
- policy-version migration/readback for existing tracks; and
- exhaustive boundary, alias, location, and policy-widening tests.

No visual polish, search mode, readiness tile, or selected-job count can compensate for that
missing authority. Phase 612B must not be used as evidence that broader matching or execution is
production-ready.

## Acceptance Criteria

### Audit Record

1. Observed, claimed, contradicted, unknown, Bluey fact, and Bluey decision are not conflated.
2. The exact Jobs search modes, LinkedIn tour steps, and current 17 MCP tools are recorded.
3. Gmail/LinkedIn, bounded synthetic upload/preparation, posting, C2C, employer submission,
   payment, and credential boundaries are explicit.
4. Sensitive Auto Reply fields/documents are documented without recording a user's values.
5. No secret, token, real email address, real phone number, resume body, or personal identifier
   appears.

### Command Center

6. The surface is original Bluey design and copy, not a pixel or asset mirror.
7. Readiness, focus, pipeline, source health, and recent evidence have clear empty/loading/error or
   unavailable states.
8. `Prepared`, `sent`, and `submitted` remain distinct.
9. Every source/freshness/eligibility value is either backed by current data or visibly labeled as
   demonstration/unavailable.
10. A selected item or navigation action cannot create write authority.
11. Mobile retains state, blockers, and pending-review visibility even where an editor is
    desktop-optimized.
12. Accessibility, keyboard focus, contrast, reduced motion, and responsive checks cover the new
    surface.

### Operational Boundary

13. The bounded synthetic GiraffyReach account writes are recorded exactly and do not imply
    authority for OAuth, communication, posting, employer submission, or payment.
14. No Bluey provider, OAuth, payment, production database, queue, or external account write is
    introduced.
15. No deploy, production flag, credential generation, or release activation occurs.
16. Round 613 remains an explicit prerequisite for canonical matching and execution.

## Unknowns Preserved For Later Reviewed Batches

- exact Gmail and LinkedIn OAuth scopes, token lifetimes, refresh, and revocation behavior;
- parser behavior beyond the single synthetic fixture, retention, deletion, model routing, and
  overwrite semantics;
- exact generated-resume prose and whether every rewrite remained evidence-bound;
- bulk-selection persistence, deduplication, approval, and provider receipt behavior;
- C2C recipient sourcing, consent, delivery, reply, bounce, opt-out, and abuse controls;
- LinkedIn publication API, moderation, idempotency, and post receipt behavior;
- Agent Connect deployed schemas, actual write capability, and scope enforcement;
- referral qualification, rewards, fraud controls, and privacy terms;
- billing taxes, proration, renewal, cancellation, refunds, and failed-payment behavior; and
- the server behavior behind mobile desktop-only surfaces.

Unknown means unknown. No implementation agent should fill one of these gaps by assumption or by
probing a private endpoint.

## Decision

The second pass confirms that GiraffyReach packages career operations well: setup is visible, job
inspection stays in context, settings are close to the action, unfamiliar features have tours,
and complex automation shows prerequisites. Bluey should adopt those product lessons with an
original interface.

Bluey's differentiator is the truth underneath the interface. The Career Command Center must make
source evidence, canonicalization status, Career Track eligibility, claim provenance, exact
consent, approval scope, and durable receipts easier to understand than a competitor does. Phase
612B begins that work as a read-only composition layer. It grants no external or production
authority, and Phase 613 taxonomy remains the next mandatory foundation.
