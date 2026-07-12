# Round 489 - Jobs Product And Infrastructure Source Audit

Date: 2026-07-11
Workstream: Customer workflow, account model, billing, tenancy, security, scale,
and operations
Source roots:

- `/Users/uno/Downloads/job and AI browser`
- `/Users/uno/Downloads/cue-bluey-jobs/_refs/jobs-research`

## Executive Verdict

Bluey Jobs has a credible product shell and better domain boundaries than the
single-user bots in the source set. It is suitable for internal dogfood and a
small invited beta after the immediate correctness issues are fixed. It is not
yet suitable for the stated one-million-account or one-thousand-concurrent-
browser target.

The largest gap is not another dashboard feature. Product promises such as
daily limits, one-company policy, first-session matches, mailbox/calendar sync,
cloud continuation, packet allowances, and account deletion are not all backed
by durable enforcement. The infrastructure also lacks distributed browser
leases, complete tenant controls, provider-backed object storage, Jobs-specific
rate limits, and a deployable browser control plane.

## Customer Experience Findings

### 1. Make Onboarding Produce A Useful Result

The portal currently has a strong guided structure, but the first useful match
depends on production discovery that is not wired into a workflow. Completing
onboarding must atomically:

1. validate the baseline Career Profile;
2. create at least one Career Track;
3. persist one application identity;
4. start a durable discovery workflow;
5. show discovery progress and partial results;
6. route missing facts into one compact completion queue.

Useful MIT product references:

- `_refs/jobs-research/jobpilot/apps/web/src/components/features/onboarding/onboarding-wizard.tsx:33`
- `_refs/jobs-research/jobpilot/apps/web/src/components/features/onboarding/profile-checklist-card.tsx:23`
- `_refs/jobs-research/career-ops/DATA_CONTRACT.md:5`

Port the progressive checklist and contract ideas, not JobPilot's exact UI.
Bluey should preserve its compact dashboard and begin discovery before the user
finishes reading a success screen.

### 2. Enforce Application Lifecycle Server-Side

The customer needs one trustworthy timeline rather than separate match, browser,
and email stories. The canonical lifecycle should cover:

```text
matched -> preparing -> needs_confirmation -> awaiting_review
        -> queued -> running -> needs_input
        -> submitted -> screening -> interview -> offer
        -> rejected | withdrawn | failed
```

Every transition needs actor, source, timestamp, evidence, and idempotency key.
Mailbox updates may propose a transition, but a classifier must retain the
source message and confidence for review.

Useful MIT references:

- `_refs/jobs-research/jobpilot/apps/api/prisma/schema/application.prisma:1`
- `_refs/jobs-research/career-ops/modes/tracker.md:21`
- `_refs/jobs-research/career-ops/modes/followup.md:17`

### 3. Finish Answer Memory As A Product

Bluey's account, Career Track, and company precedence is the right model. The
current implementation still needs:

- normalized question fingerprints rather than exact-text lookup only;
- identity and jurisdiction scope for legal or demographic answers;
- `last_used_at`, `use_count`, change history, and revocation;
- a `Remember this answer` control in the Intervention Inbox;
- a preview of every application that reused an answer;
- forced reconfirmation when a time-sensitive answer expires.

Never reuse salary, sponsorship, work authorization, disability, veteran,
demographic, or legal attestations across identities or jurisdictions without
an explicit scope.

### 4. Support Multiple Application Emails Correctly

Multiple emails are application identities, not aliases on one browser session.
The hierarchy must be:

```text
Bluey account
  application identity
    mailbox connection
    calendar connection
    browser profile
    resume/contact defaults
    answer-memory overrides
```

Freeze `application_identity_id`, `browser_profile_id`, and contact details on
the application before packet generation. A run may not switch identities
after entering `queued`. This prevents one Workday or Greenhouse login from
contaminating another identity's application and keeps receipts auditable.

### 5. Replace Placeholder Integrations With Real Status

JobPilot's Gmail implementation is a useful MIT reference at
`_refs/jobs-research/jobpilot/apps/api/src/modules/email/gmail.provider.ts:30`.
Bluey currently represents Gmail, Outlook, and calendar intent more strongly
than the available production workers justify.

Required integration behavior:

- OAuth connection with least-privilege scopes;
- incremental mailbox cursor and replay protection;
- sender/domain and recipient-identity matching;
- provider message reference retained, message body minimized;
- calendar event proposal before creation or modification;
- disconnection, token revocation, and full data deletion;
- explicit `Not connected`, `Syncing`, `Action needed`, and `Connected` states.

Until each worker exists, the UI should say `Coming later` rather than render a
button that only creates a pending row or mailto link.

### 6. Make Receipts The Center Of Trust

Each committed packet needs one immutable receipt containing:

- canonical job and captured description;
- Career Track and application identity;
- exact resume and cover letter object versions;
- final answers and reused Answer Memory references;
- selected claims and provenance;
- browser events and interventions;
- employer-visible filename validation;
- submission confirmation, screenshot, and timestamp;
- billing meter event and outcome state.

Users should be able to download this receipt from Applications. The server
must not describe a locally materialized file as uploaded until employer-page
evidence confirms it.

### 7. Remove Product/Charging Ambiguity

The plan says metering occurs once when a final packet is downloaded, approved,
queued, or submitted. Customer copy elsewhere describes completed applications.
Those are different products.

Recommended rule:

- Included allowance and overage are consumed when a unique, usable packet is
  committed for a canonical job.
- Regeneration, browser retries, interventions, and handoffs reuse that meter.
- If generation fails before the packet is usable, no allowance is consumed.
- A receipt always shows the meter event.

Instrument real costs before finalizing margins. Cloud-browser minutes,
document generation, retrieval/ranking, LLM tokens, storage, email sync, and
support burden must be visible per application. Current Cloud pricing should
remain beta until high-percentile cost and failure rates are measured.

## Infrastructure Findings

### 1. Fix The PostgreSQL Migration Gap

The SQLite schema includes `jobs_local_run_tickets`, while the PostgreSQL
migration set does not. The API can therefore appear healthy in local tests and
fail when issuing a production local-browser launch ticket.

Required release gate:

- add the PostgreSQL table, indexes, expiry cleanup, and account-scoped foreign
  keys;
- run the complete Jobs database suite against PostgreSQL in CI;
- reject schema startup when the expected Jobs migration version is absent.

### 2. Replace Process Memory With Durable Coordination

The cloud runner uses process-local Maps for active sessions and identity locks
at `jobs/runner/src/server.ts:48-49`. This does not survive a restart and does
not coordinate replicas.

Required services:

| Need | System of record |
| --- | --- |
| Workflow state, retries, timers | Temporal |
| Browser leases, heartbeats, rate limits | Valkey/Redis |
| Canonical product state and ledgers | PostgreSQL |
| Job retrieval | OpenSearch fed by a Postgres outbox |
| Documents, screenshots, receipts, profiles | R2/S3 |
| Data-encryption keys | KMS/HSM envelope encryption |
| Metrics, traces, structured logs | OpenTelemetry backend |

Do not add another queue for workflow ownership. Temporal should remain the
durable orchestrator; Valkey should provide short-lived leases and live state.

### 3. Close The SSRF And Browser-Egress Boundary

`jobs/automation/src/network.ts:4-117` validates navigation targets, but a page
may still request private subresources or open private WebSockets. Browser
containers need both application and network enforcement:

- route every browser request through exact-host and resolved-IP validation;
- block loopback, RFC1918, link-local, metadata, Unix sockets, and DNS rebinding;
- deny private egress at the container/VPC layer;
- isolate browser containers from databases, Temporal, KMS, and internal APIs;
- allow only a narrow runner control channel using a workload identity.

### 4. Add Tenant Defense In Depth

Account predicates in application queries are necessary but insufficient for a
high-scale sensitive product. Add:

- PostgreSQL row-level security or an equally testable tenant access layer;
- composite account-scoped foreign keys for cross-record references;
- object-store prefixes plus signed account-bound object access;
- account and identity AAD on encrypted records;
- cross-tenant negative tests for every Jobs API;
- paginated workspace APIs rather than loading all collections at once.

### 5. Protect Secrets And Workflow Histories

One global AES/HMAC key without key IDs, rotation, or account AAD is not an
adequate production browser-profile design. Sensitive resume text and answers
also enter Temporal histories unless a payload codec is configured.

Required controls:

- per-account or per-identity data-encryption keys;
- KMS-wrapped keys and versioned ciphertext envelopes;
- encrypted Temporal payload codec for sensitive fields;
- secret and PII redaction before logs or traces;
- local browser secrets in Electron `safeStorage` or OS keychain;
- key rotation and cryptographic deletion tests.

Useful MIT references:

- `_refs/jobs-research/jobpilot/apps/api/src/common/crypto/crypto.service.ts:8`
- `_refs/jobs-research/job-hunter-team/desktop/auth/keyring-storage.js:112`

### 6. Disable Generic Unattended Submission

A generic semantic adapter must not submit an unmatched HTTPS page merely
because it contains form-like controls. Production policy should require:

- a versioned provider adapter, or a versioned direct-employer allowlist;
- verified job/application classification;
- review-first for all fallback forms;
- no submit action from semantic fallback until a provider confidence and
  evidence policy passes;
- exact restricted-domain policy for LinkedIn, Indeed, and ZipRecruiter unless
  Bluey holds a documented automation agreement.

### 7. Add Operational Gates

Required before invited cloud beta:

- Jobs CI covering TypeScript, Electron, Rust, PostgreSQL, migrations, fixtures,
  and generated dependency notices;
- OpenTelemetry traces spanning API, Temporal, runner, browser, object writes,
  and meter event;
- SLOs for discovery freshness, queue age, browser allocation, intervention
  recovery, submit-unknown rate, duplicate-submit rate, and receipt completion;
- admission limits per account, identity, employer, ATS host, and region;
- regional browser-pool draining and disaster recovery drills;
- support tooling that never exposes raw cookies, passwords, OTPs, or sensitive
  answer values.

## Source Reuse Decisions

| Source pattern | Decision | Bluey use |
| --- | --- | --- |
| JobPilot onboarding, lifecycle, Gmail, analytics | Port/adapt MIT components and tests selectively | Progressive onboarding, tracker schema, mailbox worker, outcome analytics. |
| Career Ops contracts, tracker, follow-up cadence, stats, cooldowns | Port/adapt MIT logic | Server-enforced product policy and lifecycle reminders. |
| JobPilot crypto and origin policies | Adapt | Envelope primitives and takeover origin checks, with Bluey tenancy added. |
| Job Hunter keyring and redacting logger | Adapt | Local profile secrets and secret-free logs. |
| AIHawk single-user YAML configuration | Schema reference only | Import into validated account/track/identity records; never production state. |
| Legacy Selenium/browser-extension login reuse | Reject | Conflicts with dedicated, identity-scoped Bluey Browser. |
| Source projects' in-memory/subprocess queues | Reject | Do not replace Temporal and durable ledgers. |

## Rollout Gates

### Internal Dogfood

- PostgreSQL schema parity;
- server-enforced daily/company/track rules;
- receipt download;
- one real discovery workflow;
- provider fixtures and no generic unattended submit;
- no duplicate click after submit uncertainty.

### Invited Local Beta

- packaged identity-scoped browser;
- at least two provider-certified adapters;
- durable interventions and takeover;
- real account export/deletion;
- measured per-packet costs.

### Invited Cloud Beta

- distributed browser leases and fenced ownership;
- versioned encrypted profile snapshots;
- cloud takeover gateway;
- Jobs-specific rate limits, observability, and regional isolation;
- Gmail/Outlook integrations display only capabilities that really work.

### General Availability

- five provider certifications with live canaries;
- audited tenant isolation and encryption;
- disaster recovery and profile-deletion verification;
- measured margin and capacity headroom;
- legal review of data use, provider automation agreements, and customer terms.

## Product Consequence

Bluey should compete on trust, continuity, and evidence rather than claiming the
widest brittle site coverage. The winning experience is: finish one profile,
get fresh matches, approve a genuinely job-specific packet, let one isolated
browser run it, intervene only when needed, and retain an exact receipt. Every
backend and infrastructure change above exists to make that simple promise
true under retries, multiple identities, multiple tracks, and many users.
