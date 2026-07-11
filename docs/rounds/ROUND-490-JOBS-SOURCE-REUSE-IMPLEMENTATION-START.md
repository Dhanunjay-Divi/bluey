# Round 490 - Jobs Source Reuse Implementation Start

Date: 2026-07-11
Branch: `codex/bluey-jobs-20260710`
Scope: turn the multi-agent source audit into the concrete first implementation plan for Bluey Jobs.

## Current reality

Bluey Jobs has a useful beta foundation: Jobs SPA, onboarding, persistence, metering, public ATS discovery shape, shared automation contracts, Answer Memory, interventions, Electron/Chromium controller, workflow boundaries, and receipt model.

The source audit confirms that the foundation is pointed in the right direction, but the product is not yet ready to claim broad unattended applying. The blockers are not UI polish; they are adapter certification, browser/profile isolation, durable cloud execution, submission ledgers, source provenance, and identity-aware application rules.

## What to build first

### 1. Identity-scoped Bluey Browser

Every application email needs its own isolated browser profile:

```text
Bluey account
  Application identity A
    Browser profile A
  Application identity B
    Browser profile B
```

Each application run must freeze `application_identity_id` and `browser_profile_id` in the receipt. This prevents Workday, Greenhouse, Lever, or direct employer cookies from one email affecting another identity.

Minimum implementation:

- `application_identities` table: email, display name, location, phone, calendar/mail integration, default Career Tracks, status.
- `browser_profiles` table: identity ID, profile type, local path reference or encrypted cloud snapshot reference, version, last used, deletion status.
- One local Chromium profile per identity.
- One encrypted cloud snapshot per identity/profile/version.
- Receipt includes identity/profile IDs and browser profile version.

### 2. Submission ledger

Current runners need a durable safety line around the submit click. The ledger should be separate from UI state:

```text
prepared
submit_started
submitted_confirmed
submitted_unknown
submit_rejected
receipt_captured
```

If a browser crashes after `submit_started`, Bluey must not retry blindly. It should ask the user to inspect the preserved browser or use email/portal evidence to classify the result.

### 3. Company and track collision policy

The user raised the SDE/Data Engineering issue correctly. Bluey needs a company application policy:

- Default: one active application per company per cooldown window.
- User option: allow multiple tracks at one company only when role family is clearly different.
- Always warn when a new application would use a different Career Track for the same company.
- Never allow the same canonical job to be submitted twice from different tracks.

Required data constraints:

- `canonical_companies`
- `canonical_jobs`
- `job_matches` scoped by `career_track_id`
- `applications` freeze canonical job, track, identity, packet, and resume version
- uniqueness on `(account_id, canonical_job_id)` for committed applications

### 4. Answer Memory

Answer Memory must be a first-class product surface, not a helper hidden in automation.

Scopes:

- Account answer: reusable everywhere.
- Career Track answer: applies to a role/location track.
- Company answer: overrides for one employer.
- Application answer: frozen for one exact job.

When automation is stuck on a question, the Intervention Inbox should let the user answer once and choose where to remember it. Future runs should show when an answer was reused and allow editing.

### 5. Recent-job discovery

Discovery must prefer fresh postings and record why old jobs were skipped.

Default policy:

- Prefer jobs posted within 14 days.
- Allow 30 days only when source confidence is high and no newer equivalent exists.
- Skip stale or expired posts.
- Normalize duplicates by employer, title, location, source URL, ATS ID, and canonical job URL.

### 6. Receipt-first application packet

Every completed packet and submitted application needs a durable receipt:

- JD snapshot and URL.
- Exact resume version.
- Cover letter if generated.
- Final application answers.
- Confirmed claims used.
- Identity used.
- Attachments and document hashes.
- Browser screenshots.
- Timestamp and outcome.
- Submission confirmation URL/text when available.

This is the customer trust object and the support/debug object.

## Source code to port first

### Discovery

Start from `career-ops` patterns because it is MIT and has the strongest provider/testing discipline:

- Provider contracts and public ATS normalization.
- Bounded pagination and defensive HTTP behavior.
- Provider fixtures as regression tests.

Bluey-specific changes:

- Tenant-aware provider config.
- SSRF protection for top-level requests and subresources.
- Distributed throttling through Redis/Valkey.
- OpenSearch indexing.
- `canonical_jobs` and `job_matches` split.

### Form intelligence

Use `job-apply-plugin`, `easy-job-application-filler-extension`, and `ai-job-agent` as reference material for:

- Field aliases.
- Common ATS questions.
- Document upload handling.
- Answer-bank memory.
- Confirmation gates.

Bluey-specific changes:

- No guessing required facts.
- Track/company/account answer precedence.
- Application-specific frozen answers.
- Typed interventions for unknown required questions.
- Stop before final submit if confidence is below threshold.

### Product workflow

Use AIHawk, ApplyPilot, ai-job-agent, and proficiently skills for:

- Complete profile breadth.
- Exclusions and daily volume.
- Per-job artifact bundle.
- Review-first default.
- Resume diff and unique resume per job.

Bluey-specific changes:

- PostgreSQL records instead of YAML files.
- Square/Bluey balance/subscription authority.
- Temporal workflows.
- Local and cloud browser parity.
- Provenance records for facts and resume changes.

### Research and matching

Use the AI/browser archive audit only as a clean-room pattern source:

- Source ledger.
- Citations tied to source chunks.
- Query planning.
- Provider normalization.
- Replayable search.

Bluey-specific changes:

- No private endpoint automation.
- No credentials in research fetchers.
- RAG sources support ranking and explanations, not unconfirmed resume facts.

## What not to ship from source repos

- Any raw cloned profile, cookies, credentials, screenshots containing user secrets, or job-site session state.
- Selenium selectors copied from LinkedIn-focused tools.
- Local YAML/JSON as canonical production state.
- CAPTCHA-solving or private API bypasses.
- Generic apply bots that lack receipts, metering, and tenant isolation.
- Any dependency bundle without generated notices.

## Backend services needed

Minimum private-beta stack:

- Existing Bluey identity, balance, and subscription authority.
- `bluey-jobs-api` for profile, facts, tracks, jobs, matches, applications, identities, browser sessions, interventions, receipts, and entitlements.
- PostgreSQL for canonical data and ledgers.
- Redis/Valkey for leases, live run state, rate limits, and browser pool coordination.
- R2/S3 for resumes, cover letters, screenshots, receipts, browser profile snapshots, and source snapshots.
- KMS or equivalent envelope encryption for sensitive profile fields and browser snapshots.
- Temporal for discovery, ranking, packet generation, browser runs, interventions, email/calendar sync, reminders, and outcome tracking.
- OpenSearch for searchable job corpus and source-backed matching.
- Observability: traces, run events, structured logs with no raw resume/source/body leakage.

## Public beta gates

Do not open broad public automation until these gates pass:

1. Greenhouse and Lever submit end-to-end in fixtures, sandbox/live test tenants, and preserved-browser recovery tests.
2. Workday, Ashby, and SmartRecruiters pass the same suite.
3. Browser profiles are identity-scoped, encrypted, removable, and absent from logs.
4. Submission ledger handles crash-after-click without duplicate submission.
5. Multiple Career Tracks cannot duplicate the same job or silently conflict at the same company.
6. Gmail/Outlook and calendar integrations are real or hidden behind an honest beta label.
7. Dependency notices and source-provenance manifest are generated in CI.
8. Overages and subscription allowances are idempotent.
9. SSRF, rate limits, tenant isolation, and secret redaction pass focused tests.
10. Receipts are complete enough that a user can prove what Bluey submitted.

## Product stance

The product should feel like this:

- Import resume.
- Complete baseline profile once.
- See useful matches immediately.
- Pick Career Tracks and application identities.
- Let Bluey tailor each packet uniquely.
- Review when needed.
- Submit locally or in cloud.
- Intervene only for missing facts, site challenges, or user-sensitive questions.
- Track every application, reply, follow-up, and interview from the receipt.

That is the version that can compete: not a forked bot, not a Chrome extension, and not a generic resume spammer. Bluey should be the calm command center with a dedicated browser and a durable audit trail underneath.
