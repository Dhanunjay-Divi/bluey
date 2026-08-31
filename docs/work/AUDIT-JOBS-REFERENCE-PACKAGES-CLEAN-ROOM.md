# Clean-Room Audit — Jobs Reference Packages

> **Codex preflight:** `$bluey-ops` was loaded. The named Phase 614B worktree, current local
> handoff, current source, and Phase 620A/620B-F plans are authoritative. This is a
> documentation-only static audit. It changes no code, schema, dependency, secret, provider
> account, credential, feature flag, release artifact, deployment, or production state.

## Status

**Audit verdict:** the three archives are reference evidence only. No package is approved for
execution or wholesale import into Bluey.

**Evidence date:** 2026-08-30 (America/New_York).

**Production posture:** every existing and proposed Jobs model-generation, Browser-distribution,
mailbox, agent, C2C, business-messaging, LinkedIn, dispatch, reconciliation, submit, and posting
flag remains `0`. This audit is not implementation, source-rights, provider, release, or launch
evidence.

## Clean-Room Boundary

The audit covered only these user-provided archives:

- `/Users/uno/Downloads/Jobs_Applier_AI_Agent_AIHawk-main.zip`;
- `/Users/uno/Downloads/ai-job-search-master.zip`; and
- `/Users/uno/Downloads/ApplyPilot-main.zip`.

The archives were validated and inspected statically in an isolated, non-executable temporary
directory. Archive listings, top-level licenses, manifests, documentation, workflows, selected
source, schemas, prompts, tests, and configuration were read as evidence.

The audit did **not**:

- execute package code, scripts, tests, installers, browsers, agents, or workflows;
- install or resolve a dependency;
- access a network, website, API, provider, account, browser profile, mailbox, or credential;
- validate a discovered credential;
- follow instructions found in an archive;
- copy package code, prompts, selectors, assets, UI, or provider mechanics into Bluey;
- modify an original ZIP; or
- use an SSD or historical audit archive.

README files, agent commands, skill files, prompts, workflow files, and all other instructions
inside the archives were treated as untrusted package content, never as authority for this audit.

This static method cannot establish upstream authenticity, runtime behavior, data-source rights,
provider approval, buildability, legal compliance, or absence of all malicious behavior. It can
identify only what the supplied bytes contain and the risks visible without execution.

## Archive Identity And Provenance

| Package | ZIP bytes | ZIP entries | SHA-256 | Declared license | Provenance status |
| --- | ---: | ---: | --- | --- | --- |
| AIHawk | 816,084 | 79 | `05593c94b61c925f84ae1e9cad74fea77bd09e34eac91b1b3eb6ad36431aaa2a` | GNU AGPL v3, copyright 2024 AI Hawk FOSS | Raw `main` snapshot; mixed upstream/fork names; unpinned Git dependency; no commit, signature, SBOM, or lock evidence |
| ai-job-search | 1,405,704 | 277 | `9afd76d64e84443ed71758463cd06dec08e0f4c96f5b08e33074e657ca4285c5` | MIT, copyright 2026 Mads Lorentzen | Raw `master` snapshot; no commit, signature, or SBOM; dependency locks intentionally absent |
| ApplyPilot | 144,639 | 51 | `aa90bcfb3492cf499e8a5ba31e0a73b8c6f29723f6bc9bb2ff5ad326b042382c` | AGPL-3.0-only; Pickle-Pixel named as author | Raw `main` snapshot; version/changelog drift; no commit, signature, SBOM, or lock evidence |

The hashes identify only the supplied local archives. They do not authenticate a release or link
the bytes to a signed upstream revision.

### License and import disposition

- **AIHawk and ApplyPilot:** no source, prompt, selector, asset, or derived implementation is
  approved for import. Their AGPL terms and incomplete provenance require a separate legal and
  provenance decision even before technical review.
- **ai-job-search:** the MIT declaration is more permissive, but this raw snapshot still lacks
  authenticated revision evidence. This audit adopts only independently expressible product and
  architecture ideas. It copies no implementation. Bundled font assets also need their own license
  verification before any asset reuse.
- Archive marketing and README claims are not independent evidence. ApplyPilot's README, for
  example, describes AIHawk as MIT while the supplied AIHawk archive declares AGPL v3.

## Credential Quarantine Requirement

The AIHawk archive contains the same non-placeholder, OpenAI-format credential value in two files:

- `data_folder/secrets.yaml`; and
- `data_folder_example/secrets.yaml`.

The value is intentionally omitted from this document. It was neither displayed nor tested
against a provider. It must be treated as compromised regardless of whether it is now active.

Required handling outside this documentation change:

1. revoke or rotate the credential through the owning provider account;
2. quarantine the original archive and every copy until a redacted replacement is available;
3. do not commit, attach, log, paste, index, or import either affected file;
4. review the owning provider's audit and usage records for unexpected activity; and
5. preserve only a secret-free incident record containing the archive hash, file paths, discovery
   time, rotation evidence, and owner acknowledgement.

The static credential-pattern pass found no other obvious API-token or private-key shapes. That is
not proof that no other secret or sensitive personal data is present.

## Package Findings

### AIHawk

The supplied package is primarily a local resume and cover-letter generator rather than a working
job-search and application platform.

- Its interactive entry point offers base-resume PDF generation, tailored-resume generation from a
  job URL, and tailored-cover-letter generation from a job URL.
- Provider/job-board plugins are described as removed. Application automation imports are disabled
  or absent, and the included local application saver references missing pieces.
- It contains no usable job-discovery source, submit receipt, mailbox synchronization, application
  outcome workflow, C2C/W2/1099 classification, WhatsApp/iMessage integration, or C2C chat.
- Applicant YAML can contain identity, contact, work history, education, salary, demographic,
  work-authorization, sponsorship, location, and preference data.
- LLM request/response logging can retain complete resume, job, and token-usage content in local
  output files.
- Browser setup disables the sandbox, ignores certificate errors, disables web security, permits
  local-file access, and downloads a driver at runtime.
- One provider adapter disables all listed Gemini harm-category blocks.
- Public-facing package copy promotes anti-bot and CAPTCHA-evasion behavior.
- No test files were supplied, and important dependency/module boundaries are incomplete or
  unpinned.

**Disposition:** quarantine for credential handling; do not execute or import.

### ai-job-search

The supplied package is a local agent/command workflow, not a hosted service and not an autonomous
final-submit engine.

- Commands cover setup, discovery, ranking, application-document preparation, outcome recording,
  Gmail sync, Notion sync, offline HTML reporting, interview preparation, and skill/portal
  extension.
- Application preparation archives the exact posting, evaluates fit, drafts LaTeX resume and cover
  documents, invokes an independent reviewer, renders PDFs, visually checks output, and checks the
  PDF text layer for ATS readability.
- The application flow asks before proceeding and records a `drafted` tracker state. It does not
  submit the employer application.
- The tracker is a local CSV plus append-only application/outcome folders. It is useful as a
  prototype representation, not production authority.
- Gmail sync is read-only, cites source messages, proposes status changes, and requests batch
  approval before local writes. Follow-up email is drafted, not sent.
- Full email bodies may still enter agent/model context. Skipped or unmatched messages can be
  marked processed permanently, which can hide later false negatives.
- Personal profile data is written into tracked methodology files. A public fork can expose it even
  when generated output paths are gitignored.
- Permission guards and CI are comparatively disciplined, but instruction-level controls are not a
  sandbox and dependency resolution is not locked.

**Disposition:** retain as a clean-room design and evaluation reference only. Do not copy code,
prompts, skills, portal clients, or assets.

### ApplyPilot

The supplied package is a local Python CLI using SQLite, browser workers, LLM-generated documents,
and broad automated application behavior.

- Its pipeline is discovery, enrichment, scoring, tailoring, cover-letter generation, PDF
  generation, and a separate apply stage.
- SQLite stores job, score, document, worker, attempt, status, and dashboard data.
- Job discovery combines JobSpy, many Workday employer endpoints, and direct-site Playwright/LLM
  extractors.
- The resume prompts permit related tools absent from the source resume, the reviewer permits
  limited “stretches,” and the last failed review can still be persisted as approved with a warning.
- LLM-generated resume content is inserted into browser-rendered HTML without HTML escaping. A
  malicious posting or model response could create active content or network requests during PDF
  generation.
- The apply launcher dynamically resolves unpinned MCP packages, invokes an agent with permission
  bypass enabled, and permits email submission, OTP reading, account creation, CAPTCHA solving,
  and final application submission.
- Browser-worker setup clones broad portions of a real Chrome profile, including authenticated
  state, into persistent worker directories.
- Profile configuration can retain a reusable job-site password and applicant PII in plaintext.
- A dry run can be recorded as applied. Normal success is inferred from model text rather than an
  employer-controlled readback and trusted receipt.
- A timeout after submit can be recorded as failure and retried, creating duplicate irreversible
  effects. There is no safe `side_effect_unknown`, lease fencing, crash reconciliation, or trusted
  finalization boundary.
- A port-cleanup path can terminate any listener on a chosen debugging port without first proving
  worker ownership.
- The supplied CI names a test directory that is absent from the archive.

**Disposition:** do not execute or import. Use only as negative architecture evidence and as a
source of simulator failure cases.

## Feature And Adoption Matrix

| Capability | AIHawk | ai-job-search | ApplyPilot | Bluey disposition |
| --- | --- | --- | --- | --- |
| Job discovery | No working providers in supplied snapshot | Six portal adapters with normalized output | JobSpy, Workday, and direct-site adapters | Ideas only; every live source needs immutable rights, parser, taxonomy, and original-source authority |
| Match/rank | Preference YAML, no complete discovery pipeline | Fit ranking and configurable methodology | LLM score and explanation | Keep server-authoritative Track taxonomy and eligibility; model rank is never hard-filter authority |
| Resume/cover | Base and tailored PDFs | Grounded draft, independent review, LaTeX/PDF QA | Generated documents with unsafe claim tolerance | Adopt reviewer and document-QA concepts; retain immutable Bluey claim IDs, source resume revision, and unsupported-claim denial |
| Final application submit | Not present | Explicitly not performed | Automated browser/agent submit | Reject imported submit mechanics; use only exact-version certified Bluey ATS state machines |
| Tracker/outcomes | Incomplete local saver | CSV plus application/outcome archives | SQLite plus terminal/HTML dashboards | Dashboard ideas only; PostgreSQL and immutable receipts remain live authority |
| Gmail/mailbox | No integration | Read-only classification proposals and follow-up drafts | Gmail send, OTP, signup, and email apply | Phase 620B read/draft boundaries only; no inherited send, OTP, or account-creation authority |
| MCP/agent access | No scoped server | Agent commands and optional connectors | Unpinned MCP packages with permission bypass | Phase 620C pinned, audience-bound, least-capability, read-only-first control plane only |
| C2C/W2/1099 | Not implemented | Not implemented | Not implemented | No package evidence; Phase 620D remains a separate source-rights and reviewed-planning phase |
| WhatsApp/iMessage/C2C chat | Not implemented | Not implemented | Not implemented | No package evidence; Phase 620A/620E official business-channel rules remain unchanged |
| Submit receipt/recovery | None | Posting/application archive, but no submit | Model-reported status only | Preserve Bluey request-start evidence, readback, trusted receipts, fencing, and terminal ambiguity |
| User-facing reporting | Minimal local output | Offline self-contained HTML report | Terminal and offline HTML dashboards | Reuse information architecture only; implement original Bluey UI against authoritative APIs |

## Job-Source Matrix

| Source or mechanism | Package evidence | Static finding | Bluey adoption decision |
| --- | --- | --- | --- |
| LinkedIn guest-job endpoints | ai-job-search | Public unauthenticated endpoints; package documentation itself warns automated use violates LinkedIn terms | Reject as a Bluey source and regression fixture; no guest scraping, session automation, or inferred permission |
| Freehire REST API | ai-job-search | Public/configurable endpoint and normalized adapter | External due diligence only; require current terms, rights, retention, stability, and original-source verification before a future proposal |
| Jobbank RSS | ai-job-search | RSS discovery plus HTML/JSON-LD detail | External due diligence only; RSS visibility is not production ingestion authority |
| Jobdanmark public API | ai-job-search | Search/category/location APIs plus detail pages | External due diligence only; verify legal/source authority and contract behavior independently |
| Jobindex HTML | ai-job-search | Search HTML/embedded data plus detail pages | Reject generic scraping; reconsider only through an authorized, versioned provider contract |
| Jobnet government API | ai-job-search | Public government job API adapter | Candidate for independent rights/terms review; no automatic adoption from package behavior |
| JobSpy boards | ApplyPilot | Indeed, LinkedIn, Glassdoor, ZipRecruiter, and Google Jobs through a third-party scraper | Reject as production source authority; each board requires its own approved source mechanism and terms evidence |
| Workday CXS endpoints | ApplyPilot | Forty-eight employer configurations using an explicitly undocumented interface | Reject undocumented generic ingestion; use exact certified ATS/provider integration only |
| Direct-site Playwright/CSS/API interception | ApplyPilot | Thirty configured sites, including LLM-generated selectors; five blocked and one manual ATS classification | Reject as a generic source strategy; retain only manual quarantine and exact provider certification concepts |
| AIHawk job providers | AIHawk | Removed or absent from supplied snapshot | No source evidence to adopt |

Technically reachable content is not an authorized source. A future source proposal must satisfy the
Phase 620D rights registry and the current Bluey source, content-identity, normalization, taxonomy,
freshness, and original-source verification boundaries.

## Rejected Mechanics

The following mechanics must not enter Bluey from these packages:

1. importing AGPL or provenance-ambiguous code, prompts, selectors, or assets by convenience;
2. using, testing, preserving in source control, or propagating the exposed AIHawk credential;
3. guest-endpoint scraping, undocumented Workday calls, API interception, generic HTML scraping,
   browser-header masquerading, or LLM-generated production selectors without source authority;
4. CAPTCHA solving, anti-bot evasion, stealth claims, or automation intended to defeat provider
   controls;
5. disabling the browser sandbox, TLS verification, or web security;
6. cloning a user's browser profile, cookies, local storage, or authenticated sessions;
7. storing reusable passwords, provider tokens, full applicant PII, mailbox content, or prompts in
   plaintext configuration or logs;
8. resolving `latest`, unpinned Git, driver, MCP, browser, or agent dependencies at runtime;
9. permission-bypass agent execution or denylist-only provider tooling;
10. giving an agent Gmail send, OTP, signup, password, token, or final-submit capability through a
    broad prompt;
11. trusting model text, page appearance, a dry-run marker, or an HTTP response as an employer
    submission receipt;
12. retrying after an irreversible request may have crossed the provider boundary;
13. accepting unsupported resume claims, “minor stretches,” or reviewer failure as approval;
14. rendering unescaped posting/model content in an active browser context;
15. treating CSV, local folders, or SQLite worker state as production application authority; and
16. treating archive tests, CI names, README claims, user permission, or technical accessibility as
    provider, source-rights, security, or launch evidence.

## Bluey-Safe Design Ideas

The following ideas are independently implementable in Bluey's existing authority model. They are
requirements and simulator concepts, not permission to copy package implementation.

### Source and discovery

- Define one normalized output contract per source and test required fields, provenance, pagination,
  timeouts, bounded retries, degraded states, and deterministic deduplication.
- Preserve the exact acquired posting, normalized plaintext identity, source mechanism, parser
  revision, taxonomy revision, source-rights revision, and original-source verification revision.
- Represent blocked, manual-only, rights-unknown, stale, and structurally invalid sources as typed
  fail-closed states instead of silently dropping or accepting them.
- Separate discovery coverage from eligibility and final-submit authority.

### Resume and application quality

- Keep a drafter and independent reviewer as separate roles with exact claim citations.
- Archive the exact posting and compare every employer-facing claim against immutable verified
  profile evidence and the current source resume revision.
- Add rendered-page visual checks, PDF/DOCX reopen checks, ATS text-layer checks, and deterministic
  document-diff evidence before an application kit can be approved.
- Treat unsupported claims, malformed active content, missing text layers, and layout loss as
  blocking failures.

### Tracker and outcome learning

- Offer an owner-readable funnel/dashboard over authoritative application events, source health,
  review state, interventions, receipts, and truthful outcomes.
- Preserve append-only outcome observations and their evidence so ranking quality can be evaluated
  without rewriting historical applications or training across accounts without consent.
- Distinguish `drafted`, `approved`, `request_started`, `provider_accepted`, `submitted`,
  `side_effect_unknown`, and owner-confirmed outcomes.

### Mailbox and reviewed replies

- Use the Phase 620B minimized message projection, exact connection/grant revision, source message
  identity, correlation evidence, idempotence, and batch owner approval pattern.
- Make matched, unmatched, skipped, duplicate, and uncertain messages reviewable; never permanently
  suppress an uncertain message merely because one classifier skipped it.
- Keep read, classify, propose, create-draft, send, consume-OTP, and account-action capabilities
  separate. A broad OAuth scope must be narrowed by a provider-method allowlist and egress firewall.
- Exclude mailbox data from pooled training and define retention, export, disconnect, deletion, and
  revocation-race behavior before any live provider grant.

### Worker and simulator observability

- Reuse pipeline-stage, worker-capacity, duration, failure-category, source-health, and cost views as
  original Bluey observability requirements.
- Turn every ApplyPilot failure mode into a deterministic negative fixture: dry-run false success,
  request-start timeout, duplicate retry, stale worker, profile leakage, untrusted redirect,
  active-content PDF input, model-fabricated receipt, and crash after click.
- Keep simulator success explicitly separate from provider, native-runtime, data-rights, canary, and
  production evidence.

## Relationship To Current Bluey Plans

| Bluey phase or authority | Clean-room conclusion |
| --- | --- |
| Current Jobs/Phase 614B integrity authority | The packages do not replace signed source, identity, evidence, eligibility, destination, ATS-head, execution, or receipt authority. ApplyPilot's failures strengthen the need for those exact bindings. |
| Managed runner and final submit | Preserve lease/fence authority, immediate employer-controlled readback, exact request-start evidence, trusted finalization, and terminal `side_effect_unknown`. No package supplies an acceptable substitute. |
| Phase 620A business messaging | None of the archives implements WhatsApp Business Platform, Apple Messages for Business, safe personal iMessage, or an owner command channel. All provider and eligibility gates remain unchanged and all flags remain `0`. |
| Phase 620B mailbox read/draft | ai-job-search supports the value of source-cited status proposals and explicit approval, but Bluey must add exact grant revisions, minimized projections, provider-method enforcement, unmatched review, retention, deletion, and revocation fencing. ApplyPilot's Gmail mechanics are rejected. |
| Phase 620C scoped agent/MCP | ApplyPilot is negative evidence for unpinned packages, token/password exposure, and permission bypass. Bluey remains read-only-first with pinned specifications, audience-bound grants, exact tool scopes, no token passthrough, and invocation receipts. |
| Phase 620D C2C | No archive implements C2C, W2, or 1099 classification or rights-safe recruiter outreach. Portal adapter patterns may inform fixtures only; source rights, contact provenance, engagement taxonomy, suppression, and review remain prerequisites. |
| Phase 620E official adapters | No archive adds provider evidence. Personal WhatsApp, WhatsApp Web, personal iMessage automation, browser-session capture, and unattended SMS remain prohibited. |
| Phase 620F LinkedIn | LinkedIn guest search is rejected. No package proves official posting or messaging authority. Manual drafting/export remains the first safe boundary; any future write requires official OAuth/provider approval and exact per-effect review. |

## Security And Malware-Like Behavior Review

All three ZIPs passed archive-integrity validation. The static listing found no path traversal,
absolute extraction targets, symlinks, bundled native executables, or obvious covert persistence or
obfuscation. No package code was executed, so this is not a malware-clearance result.

Observed behavior requiring quarantine or rejection includes:

- AIHawk's exposed credential, unsafe Chrome flags, runtime driver/dependency acquisition, broad PII
  logging, and anti-bot posture;
- ai-job-search's potential full-mail-body/model exposure, public-fork PII risk, unmatched-message
  suppression, and non-reproducible dependency resolution; and
- ApplyPilot's profile/session cloning, plaintext password/PII storage, dynamic packages,
  permission bypass, Gmail/OTP/account actions, CAPTCHA solver, unescaped active HTML, arbitrary
  navigation risk, port termination, and unsafe submit/retry state machine.

These are overtly unsafe automation and privacy mechanics; the static pass found no separate covert
malware payload. The distinction does not make any package safe to execute.

## Final Decision

1. Preserve the three SHA-256 values as the only identity for this evidence set.
2. Revoke/rotate and quarantine the AIHawk credential-bearing archive without exposing the token.
3. Import no code, prompt, selector, asset, dependency choice, or provider mechanic from any package.
4. Use ai-job-search only as a clean-room source of workflow and evaluation ideas.
5. Use ApplyPilot primarily as a negative-test catalog for Bluey's source, document, worker,
   mailbox, browser, and irreversible-effect safety gates.
6. Keep every production flag at `0`; this audit grants no implementation, provider, source,
   release, deploy, or autonomous-operation authority.
