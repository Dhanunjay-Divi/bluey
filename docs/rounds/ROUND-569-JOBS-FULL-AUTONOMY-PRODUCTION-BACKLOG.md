# Round 569 - Jobs Full Autonomy Production Backlog

Date: 2026-07-24

## Objective

Define the remaining product, data, automation, infrastructure, security, and
operational work required for Bluey Jobs to behave like a reliable job-search
operator:

1. continuously find relevant jobs;
2. verify that each job is real, recent, open, and eligible;
3. generate a job-specific application packet;
4. submit through a certified runner in Auto mode without per-application
   approval;
5. pause only for missing facts, external verification, assessments, or an
   uncertain employer-facing side effect;
6. track replies, schedule interviews, preserve receipts, and maintain the
   user's application history.

This is a production backlog, not a claim that every item is live today.

## Definition Of Fully Autonomous

Bluey may describe a Career Track as autonomous only when all of the following
are true:

- discovery runs without the user pasting job links;
- source data is revalidated against the original employer or ATS;
- eligibility is decided by one server-authoritative policy;
- the application uses one frozen identity, one exact job snapshot, one exact
  resume version, and one exact answer set;
- Auto mode does not require packet approval for every job;
- every submitted claim is supported by an imported or user-confirmed fact;
- the selected runner has a certified adapter for the detected ATS version;
- application submission produces real confirmation evidence;
- retries cannot create duplicate employer submissions;
- incoming employer email is correlated to the exact application;
- Bluey can draft or send only the replies allowed by the user's communication
  policy;
- calendar actions use the correct identity and require unambiguous dates and
  time zones;
- billing and monthly allowances are idempotent;
- the user can pause, revoke, correct, export, and delete their data.

If any of these conditions is missing, Bluey must expose the exact degraded
mode: Review, Takeover, Handoff, or Needs input.

## Product Modes

### Review Mode

Bluey discovers, verifies, ranks, and prepares the complete Application Kit.
The user reviews the packet and starts submission or handoff.

### Auto Mode

The user approves a Career Track policy once. Bluey then prepares and submits
eligible applications without per-application approval. It pauses only when:

- a required fact is missing or conflicting;
- a legal, work-authorization, compensation, demographic, or identity answer
  is not covered by a confirmed policy;
- CAPTCHA, app/phone 2FA, or an assessment requires the user;
- the ATS version is uncertified;
- a submit side effect is uncertain;
- the job changed materially after preparation;
- the daily, company, allowance, or spend limit is reached.

Auto mode must never silently fall back to guessing.

## Current Production-Capable Foundation

The repository already contains substantial pieces of the target system:

- normalized Jobs workspace and Career Tracks;
- candidate profile, application identities, facts, Answer Memory, preferences,
  matches, applications, resume versions, interventions, receipts, and events;
- server-authoritative eligibility and packet guards;
- deterministic job-specific resume preparation;
- public ATS discovery infrastructure and source catalog;
- durable scheduled discovery workers;
- Greenhouse and Lever provider-specific state machines;
- generic typed adapter contracts;
- application leases, side-effect-unknown handling, evidence guards, and
  idempotent metering;
- local/cloud runner contracts and workflow boundaries;
- durable read-only Gmail and Outlook application-inbox synchronization;
- employer-message correlation, immutable evidence, and outcome-review
  interventions;
- responsive Jobs portal views for Matches, Applications, Resume, Browser, and
  Settings.

The following production flags remain intentionally disabled until their gates
are satisfied:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## P0 - Candidate Truth And Career Track Authority

### 1. Canonical profile extraction

Pending:

- complete PDF and DOCX extraction across the supplied resume fixtures;
- correctly separate company, title, location, dates, highlights, education,
  projects, certifications, and skills;
- preserve source spans and confidence for every extracted fact;
- show low-confidence fields in one compact correction step;
- support an explicit no-resume onboarding path;
- normalize role abbreviations to canonical full names while retaining aliases;
- add complete city, state, metro, country, and remote-location suggestions;
- normalize certification names without splitting one credential into several
  chips;
- derive years of relevant experience by non-overlapping role-family months.

Required acceptance:

- the same resume imports deterministically;
- company and title are never swapped;
- locations do not absorb titles;
- overlapping roles do not double-count experience;
- every extracted fact links back to its source document and source text;
- users can correct all extracted values before any Auto run.

### 2. Application identities and subprofiles

One Bluey account may contain multiple user-owned application identities. Each
identity freezes:

- legal/preferred name;
- application email;
- phone number;
- address or location;
- work authorization and sponsorship answers;
- portfolio and profile URLs;
- Career Tracks allowed to use the identity;
- an isolated browser profile.

Bluey must not create fake identities or email accounts. Users may add multiple
identities only when they control those addresses and phone numbers.

Company collision policy:

- one active application per canonical company is the default;
- a second role at the same company requires an explicit account policy;
- separate emails do not bypass the company rule;
- parent/subsidiary relationships are considered;
- withdrawal, rejection, cooldown, and materially different role exceptions are
  stored as explicit policy decisions.

### 3. Career Track policy

Each Career Track must own:

- canonical role family and aliases;
- allowed seniority range;
- required and preferred experience range;
- default experience window of current relevant years minus one through plus
  two, with an independent title/seniority guard;
- locations, relocation, remote, hybrid, and travel policies;
- salary floor and compensation preferences;
- employment and engagement types;
- work authorization and sponsorship policy;
- excluded companies, titles, industries, and staffing firms;
- identity and base resume;
- Review or Auto mode;
- daily volume and company cooldown;
- claim-review and communication policies.

Applications-per-day and Auto thresholds should use safe Bluey defaults. They
may be exposed under advanced controls, not presented as unexplained onboarding
requirements.

### 4. Employment and engagement taxonomy

Normalize source-specific labels into:

- permanent full-time;
- permanent part-time;
- W2 contract;
- C2C contract;
- 1099 independent contract;
- contract-to-hire;
- temporary;
- internship;
- co-op;
- apprenticeship;
- fellowship;
- new graduate;
- returnship;
- seasonal;
- clinical or shift-based engagement where applicable.

Source labels remain preserved for evidence. A source that cannot be mapped
confidently becomes Needs review and cannot enter Auto mode.

## P0 - Discovery And Job Truth

### 5. Continuous source ingestion

Bluey needs a production global staging pipeline:

```text
source connector
  -> bounded fetch
  -> raw source snapshot
  -> normalization
  -> provenance and license check
  -> canonical deduplication
  -> original-source revalidation
  -> searchable global index
  -> per-account hard filters
  -> per-Track ranking
```

Required source families:

- Greenhouse;
- Lever;
- Ashby;
- SmartRecruiters;
- Workday;
- employer career sites;
- approved staffing-company feeds;
- licensed aggregators;
- rights-reviewed curated feeds for internships, new-grad, and remote roles.

GitHub lists and third-party datasets are candidate leads only. Before a lead
appears as Ready, Bluey must open the original source and confirm title,
company, location, job ID, open state, posting date, employment type, and
canonical application URL.

### 6. Portal and staffing-source catalog

Create a provider registry for the supplied staffing firms and job sites.
Registry entries must record:

- canonical company/provider;
- domains and ATS family;
- discovery method;
- data-rights status;
- authentication requirements;
- application capability;
- current certification version;
- rate limits and source health;
- last successful verification;
- kill-switch status.

A company list is not equivalent to an application adapter. Most companies use
an underlying ATS that should be handled by the ATS-family adapter.

### 7. Aggregators and protected portals

LinkedIn, Indeed, ZipRecruiter, Dice, CareerBuilder, and similar sources require
a connector-specific product boundary:

- use licensed feeds, approved APIs, user-authorized browser sessions, email job
  alerts, or normal visible-browser interaction;
- never treat an aggregator listing as proof the employer is still accepting
  applications;
- deduplicate the aggregator listing against the employer/ATS canonical job;
- prefer submission on the employer's original application surface;
- use Takeover or Handoff when an unattended path is not certified.

The product can feel unlimited by continuing the workflow across Review,
Takeover, and Handoff, but it must not label those paths unattended Auto-submit.

### 8. Job freshness, fraud, and canonicalization

Pending:

- provider-specific closure and removal detection;
- canonical employer and parent-company IDs;
- normalized title, location, remote status, and compensation;
- posting-age policy per source;
- repost and duplicate detection;
- suspicious-domain and impersonated-employer detection;
- staffing-firm versus end-client attribution;
- source-health and stale-index monitoring.

No job may queue after its source snapshot expires. Eligibility and job-open
state must be rechecked transactionally before submission.

## P0 - Eligibility, Ranking, And Tailoring

### 9. One eligibility decision

The same server-owned decision must be used by:

- Matches;
- packet preparation;
- Auto eligibility;
- queue reservation;
- runner claim;
- final pre-submit check;
- receipt.

It must enforce:

- location and workplace policy;
- salary floor;
- employment and engagement type;
- sponsorship and work authorization;
- relevant experience and seniority;
- excluded companies and titles;
- application identity;
- one-active-application/company rule;
- daily limit and allowance;
- job freshness and open state;
- ATS capability and certification;
- confirmed-fact and claim policy.

### 10. Separate fit scores

Do not present one misleading percentage. Store and display:

- `base_profile_fit`: fit before changing the resume;
- `tailored_packet_coverage`: how well the final packet covers supported job
  requirements;
- `hard_filter_status`: pass or explicit reasons;
- `missing_requirements`: requirements that cannot be supported;
- `confidence`: source and extraction confidence.

Tailoring may improve coverage but may not rewrite the candidate's actual years
of experience or seniority.

### 11. Managed resume generation

Production resume generation must:

- preserve the selected template and visual hierarchy;
- preserve real employers, clients, dates, education, credentials, and project
  ownership;
- reorder and rewrite supported achievements for the job;
- select the most relevant supported skills;
- remove low-value content to stay within the chosen length;
- generate evidence-linked bullet points;
- create a unique resume version for every canonical job;
- create a real diff against the base resume;
- validate ATS text extraction from the exported PDF/DOCX;
- freeze model, prompt version, evidence revision, checksum, and cost.

Enhance mode may propose stronger wording, inferred skills, or metric
placeholders. Any newly asserted fact must remain excluded from Auto submission
until the user confirms it. It may never invent employers, clients, dates,
degrees, credentials, years, or responsibilities unsupported by evidence.

Before enabling managed generation:

- configure the production model provider;
- enforce per-packet token and cost limits;
- reserve spend before generation;
- test timeout and provider-retry idempotency;
- run the supplied resume corpus through extraction, generation, export, and
  text-recovery regression tests;
- add quality evaluation for factuality, requirement coverage, readability,
  duplication, and layout.

### 12. Application Kit activation

Every prepared job needs a complete Application Kit:

- exact job snapshot;
- exact application identity;
- tailored resume and extractable export;
- optional cover letter;
- final answers;
- claim/evidence map;
- match and eligibility explanation;
- missing requirements;
- resume diff;
- application email;
- runner capability;
- expected metering;
- pause conditions.

Review users approve the kit. Auto users approve the Career Track policy once,
then Bluey applies automatically when the kit passes every guard.

## P0 - Cloud Runner And Submission Reliability

### 13. Cloud-first runner architecture

The local runner is optional for the launch product. A cloud-first design
reduces installation friction and allows execution while the user's computer is
off.

Required cloud path:

```text
approved application packet
  -> transactional attempt reservation
  -> durable workflow
  -> isolated browser allocation
  -> identity-scoped encrypted browser profile
  -> certified ATS adapter
  -> fill and validate
  -> final eligibility/fact/job recheck
  -> submit once
  -> evidence capture
  -> receipt persistence
  -> browser/profile release or intervention hold
```

Required infrastructure:

- Temporal or equivalent durable orchestration;
- isolated Chromium containers;
- durable PostgreSQL execution leases;
- Valkey/Redis rate limits and live allocation state;
- R2/S3 evidence, document, screenshot, and encrypted-profile storage;
- KMS-managed encryption;
- signed short-lived runner capabilities;
- deny-by-default network egress;
- secure, expiring takeover links;
- queue and browser-pool autoscaling;
- regional capacity and recovery.

### 14. ATS certification program

Provider-specific state machines are required for:

1. Greenhouse;
2. Lever;
3. Workday;
4. Ashby;
5. SmartRecruiters;
6. additional ATS families by observed demand.

Each certification covers:

- URL and tenant detection;
- closed-job detection;
- authentication state;
- resume and attachment upload;
- known fields;
- dynamic and custom questions;
- validation errors;
- CAPTCHA/2FA/assessment detection;
- one unique submit control;
- confirmation evidence;
- crash-after-submit reconciliation;
- provider-version drift.

Certification is versioned. Unknown or changed variants fall back to Review or
Takeover.

### 15. Semantic and visual fallback

Playwright remains the deterministic execution base. Semantic or visual tools
such as Stagehand or OmniParser may help identify controls on unknown forms,
but they are not submission authority.

Allowed fallback behavior:

- identify labels, inputs, upload controls, and page structure;
- propose field mappings;
- fill low-risk fields in a preserved browser;
- request user review for unknown questions;
- collect adapter-development telemetry without PII.

Not allowed for unattended submission:

- clicking a visually guessed Submit control;
- bypassing CAPTCHA or platform access controls;
- treating pixels as confirmation evidence;
- retrying after an uncertain side effect.

Repeated successful Review runs may produce a new tested adapter candidate.
Only reviewed fixtures, canaries, and explicit certification may promote it to
Auto.

### 16. Browser profile and login handling

Browser profiles are scoped by:

```text
Bluey account
  -> application identity
    -> portal or ATS account
      -> encrypted browser profile
```

Pending:

- cloud profile encryption and rotation;
- account/identity/profile binding on every run;
- secure login takeover;
- session-expiry detection;
- user-visible connected-site inventory;
- revoke/delete controls;
- provider-specific cookie retention;
- no raw passwords in Bluey storage or logs.

Bluey may preserve user-authorized sessions. It must not manufacture accounts or
emails for job sites.

### 17. Irreversible-side-effect safety

Submission requires:

- one durable attempt reservation;
- one run ID and idempotency key;
- one certified submit control;
- evidence recorded before and after the action;
- no automatic retry after network loss following a click;
- side-effect-unknown reconciliation;
- duplicate-employer and duplicate-job protection;
- real confirmation URL, text, or provider receipt;
- immutable final receipt.

## P0 - Intervention And Human Takeover

### 18. Intervention Inbox

Required intervention types:

- missing required fact;
- unknown application question;
- legal or authorization question;
- sensitive demographic question;
- CAPTCHA;
- app/phone 2FA;
- email verification;
- assessment;
- login expired;
- browser takeover;
- uncertain submission result;
- employer message requiring a reply.

Each intervention preserves the exact browser and application state when
possible. The user resolves it on web or mobile and the workflow resumes with
the same run and idempotency key.

Email OTP may be detected from a connected inbox and offered for one-click user
approval. Bluey must not expose OTPs in logs or reuse them across runs.

### 19. Answer Memory

Answers are stored with precedence:

```text
company-specific
  -> Career Track
    -> account
```

Every saved answer stores:

- normalized question key;
- original question;
- answer;
- identity and Track scope;
- source;
- confirmation state;
- effective and optional expiry dates;
- last-used timestamp;
- change history.

Users can edit, disable, or delete remembered answers. Auto mode may use only
confirmed, non-expired answers.

## P1 - Email, Replies, Calendar, And Outcomes

### 20. Application inbox

Round 568 implements durable read-only Gmail and Outlook synchronization,
correlation, encrypted evidence, and review interventions.

Production enablement still requires:

- approved Google and Microsoft OAuth applications;
- exact production redirect URIs;
- live token rotation, revocation, and provider-outage tests;
- retention and deletion policy;
- backlog, ambiguity, and reauthorization monitoring.

### 21. Reviewed outbound replies

Add send scopes only after read-only inbox canaries pass.

Communication policy:

- always draft: recruiter questions, assessments, compensation, authorization,
  legal terms, offers, rejections, and ambiguous requests;
- optionally auto-send: simple acknowledgement, receipt confirmation, or
  availability from a user-approved schedule;
- never auto-send a new factual claim or a different resume without generating
  and freezing a new Application Kit.

Every sent message stores:

- application and identity;
- triggering provider message;
- final subject and body;
- attachments and hashes;
- user policy or explicit approval;
- provider send ID;
- timestamp and delivery state.

If a recruiter requests a resume for a different role, Bluey creates a new
canonical job and packet rather than reusing an unrelated resume.

### 22. Calendar automation

Pending:

- Google Calendar and Outlook Calendar OAuth;
- availability and time-zone preferences;
- detection of proposed interview slots;
- conflict checking;
- reviewed or policy-approved reply;
- event creation with company, role, participants, source message, and
  application receipt;
- update/cancel handling;
- duplicate-event prevention;
- disconnect and delete controls.

Bluey must not infer an interview from keywords alone. The event needs a matched
application and unambiguous scheduling evidence.

### 23. Outcome state model

Separate:

- application execution state;
- job availability;
- employer outcome.

Employer outcomes include:

- awaiting response;
- recruiter contact;
- assessment;
- interview;
- rejected;
- offer;
- withdrawn;
- unknown.

Mailbox classifiers may propose an outcome. The user or an evidence-backed
provider contract confirms it. Outcome corrections remain auditable.

### 24. Follow-ups

Pending:

- configurable follow-up windows;
- no-follow-up company and role policies;
- draft generation;
- user-approved auto-send policy;
- reply detection that cancels pending follow-ups;
- reminders for assessments and interview preparation;
- outcome analytics after enough comparable applications exist.

## P1 - Billing, Entitlements, And Unit Economics

### 25. Entitlement enforcement

Free, Pro, and Cloud limits must be enforced server-side at packet commit and
runner reservation. UI-only plan controls are insufficient.

Meter once per unique canonical job when its final packet is downloaded,
approved, queued, or submitted. Regeneration, retries, takeovers, and workflow
replay do not double-charge.

### 26. Overage and insufficient balance

Pending:

- transactional allowance reservation;
- atomic Bluey-balance overage reservation;
- release on pre-commit failure;
- finalize once on packet commitment;
- pause before generation or execution when funds are insufficient;
- direct Add balance recovery;
- receipt-level billing evidence;
- refund and dispute tooling.

### 27. Unit economics

Measure per completed application:

- discovery/provider cost;
- ranking and generation tokens;
- document rendering;
- cloud browser minutes;
- R2/S3 storage and egress;
- email/calendar API operations;
- intervention/support rate;
- retries and failed runs;
- payment fees.

Pricing must preserve target contribution margin at p50, p90, and abuse-case
usage. Do not promise unlimited cloud execution without a fair-use and
concurrency policy backed by measured costs.

## P1 - Evidence, Audit, Privacy, And Security

### 28. Complete application receipt

Every committed/submitted application preserves:

- canonical job and source snapshot;
- eligibility decision and policy version;
- identity;
- Career Track;
- base profile evidence revision;
- resume and cover letter;
- final answers;
- claim/evidence map;
- browser/adapter version;
- screenshots and uploaded evidence hashes;
- submit and confirmation evidence;
- metering event;
- timestamps and final state.

No synthetic confirmation text is permitted.

### 29. Tenant and secret protection

Required:

- account scope on every query;
- generic not-found behavior for cross-tenant IDs;
- bounded pagination and response minimization;
- body-size and file-type limits;
- malware scanning;
- signed object URLs;
- encrypted provider credentials and browser profiles;
- PII-free logs;
- short-lived worker credentials with nonce/replay protection;
- service and staff access audit.

### 30. Data controls

Users need:

- Jobs export;
- provider disconnect;
- browser-profile deletion;
- message and document retention controls;
- account and Career Track deletion;
- backup deletion policy;
- correction of extracted facts and outcomes.

Terms and Privacy must accurately describe job automation, third-party
providers, email/calendar access, document retention, model use, and data
deletion.

## P1 - Reliability And Operations

### 31. Observability

Dashboards by source, adapter version, and runner:

- discovery freshness and closure lag;
- normalization and deduplication rate;
- eligibility rejection reasons;
- generation latency, cost, and factuality failures;
- queue depth and lease age;
- browser allocation and crash rate;
- interventions;
- submit success;
- side-effect-unknown rate;
- receipt completeness;
- mailbox sync and correlation ambiguity;
- billing reservation and reconciliation.

### 32. Kill switches

Support immediate disablement by:

- source;
- ATS family and adapter version;
- employer/domain;
- account;
- Career Track;
- region;
- local or cloud runner;
- email/calendar provider;
- model provider.

### 33. Recovery testing

Required fault tests:

- worker death before and after submit;
- duplicate workflow delivery;
- browser crash;
- database failover;
- object-store outage;
- provider timeout and rate limit;
- token expiration;
- mailbox cursor replay;
- two workers claiming one application;
- deploy during active intervention;
- restore from backup.

## P1 - User Experience

### 34. Onboarding

Onboarding is complete only when the user has:

- imported or entered a profile;
- corrected low-confidence facts;
- added one verified identity;
- selected role family and engagement types;
- selected locations and authorization policy;
- reviewed Answer Memory defaults;
- created one Career Track;
- seen a sample Application Kit;
- chosen Review or Auto mode;
- connected inbox optionally;
- understood the first next action.

The portal should show useful matches immediately after source availability,
not an empty dashboard.

### 35. Today view

Provide one prioritized action list:

- applications Bluey can submit now;
- interventions blocking work;
- packets awaiting review;
- recruiter replies;
- assessments;
- interviews and preparation;
- stale or failed runs;
- balance or plan issue.

### 36. Trust progression

Recommend Review mode for the first several applications. After the user
approves consistent packets and answers, offer Auto mode for that Career Track.
This is a recommendation, not a hidden block.

## P2 - Expansion

- certify additional ATS families by observed job volume;
- approved staffing-provider connectors;
- multilingual profile and application support;
- region-specific authorization and demographic policies;
- interview Coach handoff and account-scoped workspaces;
- advanced outcome analytics and resume experiments;
- recruiter CRM and networking workflows;
- mobile-first interventions;
- local runner distribution if customer demand justifies it.

## Implementation Order

### Gate A - Truthful application packets

1. Complete resume extraction and correction.
2. Freeze Career Track, identity, engagement, and experience policies.
3. Finish managed resume generation with evidence and spend guards.
4. Prove PDF/DOCX export and job-specific uniqueness.

### Gate B - Continuous jobs

1. Connect scheduled sources to the global staging pipeline.
2. Add original-source revalidation and canonical index.
3. Rank all eligible matches per Career Track.
4. Add source health, stale removal, and fraud checks.

### Gate C - First autonomous submissions

1. Provision cloud browser pool and encrypted profiles.
2. Certify Greenhouse and Lever with authorized test vacancies.
3. Pass crash-after-submit and duplicate-delivery tests.
4. Enable Cloud runner only for certified canary accounts.

### Gate D - Communication loop

1. Productionize read-only Gmail/Outlook sync.
2. Add reviewed reply drafts.
3. Add policy-limited send.
4. Add reviewed calendar scheduling.
5. Confirm evidence-backed outcomes.

### Gate E - Broad coverage

1. Certify Workday, Ashby, and SmartRecruiters.
2. Add high-volume ATS families.
3. Add approved aggregator and staffing connectors.
4. Promote semantic observations only through the certification pipeline.

## Required Owner Inputs

Engineering cannot complete the external production gates without:

- approved Google OAuth application;
- approved Microsoft OAuth application;
- production redirect URIs;
- cloud browser and Temporal credentials;
- working R2/S3 credentials and retention policy;
- KMS/encryption configuration;
- production model provider credentials and spend budget;
- authorized Greenhouse and Lever test vacancies;
- test accounts for Free, Pro, and Cloud;
- billing caps and overage policy;
- communication auto-send policy;
- calendar scheduling defaults;
- support and incident owner;
- provider/data-source contracts where required.

Secrets must be installed directly in the production secret store, never
committed or pasted into round documentation.

## Go/No-Go Gates

Bluey may enable unattended Auto mode only after:

- zero hard-filter violations across the acceptance corpus;
- zero unsupported factual claims in submitted documents;
- zero duplicate submissions under fault injection;
- zero false Submitted states;
- 100 percent complete typed receipts;
- no Auto runs on unknown or uncertified ATS variants;
- stale and closed jobs cannot reserve a run;
- crash-after-submit never retries blindly;
- all runner actions are identity/account scoped;
- billing remains exactly once under replay;
- inbox and calendar data are encrypted and deletable;
- no raw PII, cookies, tokens, answers, resumes, or OTPs appear in telemetry;
- account deletion and backup restoration pass;
- adapter success, intervention, and recovery rates meet the launch threshold;
- an independent production canary passes on each enabled adapter version.

## Launch Truth

The nearest credible autonomous release is:

- continuous verified discovery;
- managed job-specific Application Kits;
- Auto mode for certified Greenhouse and Lever variants;
- cloud execution;
- preserved interventions and takeover;
- durable receipts;
- read-only employer-update tracking;
- reviewed reply drafts.

Everything else should remain Review, Takeover, or Handoff until its own
certification and production gates pass. This preserves the user's experience
without hiding failures or claiming automation that is not actually running.
