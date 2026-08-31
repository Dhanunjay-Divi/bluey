# Phase 620B-F Plan — Jobs Omnichannel Outreach And Agent Control Plane

> **Codex preflight:** `$bluey-ops` was loaded. The named Phase 614B worktree, current local
> handoff, current source, and Phase 620A plan are authoritative. This is a design-only artifact.
> It creates no code, schema, credential, provider account, OAuth grant, callback, message,
> application, public post, release, flag change, or production effect.

## Status

**Architecture verdict:** bounded, simulator-first successor design; no external effect is
authorized.

**Independent re-review:** 🟢 design accepted on 2026-08-30 with no remaining P0–P2 finding after
mailbox delegated-authority/recovery, hostile-content isolation, MCP session/client lifecycle, and
cross-channel routing/fallback/STOP corrections. This is not an implementation or launch verdict.

**Evidence date:** 2026-08-30 (America/New_York).

**Production posture:** every existing and proposed mailbox, agent, C2C, business-messaging,
LinkedIn, dispatch, reconciliation, submit, and posting flag remains `0`. Phase 620A remains
unchanged and is a prerequisite, not an implementation detail to absorb into this plan.

**Clean-room boundary:** this plan uses public/customer-visible Tsenta and GiraffyReach pages,
their public policy/security pages, public official provider documentation, current Bluey source,
and current Bluey authority documents. It does not rely on a competitor login, private session,
undocumented endpoint, network trace, credential, token, application submission, message, email,
post, proprietary code, UI asset, prompt, algorithm, data feed, or inferred private backend.

## Decision

Bluey may pursue the useful product goals visible in public competitor material—mailbox-aware
tracking, reviewed recruiter replies, owner-facing business messaging, C2C planning, safe agent
tools, and LinkedIn content preparation—only as original Bluey capabilities with stricter authority
and evidence boundaries.

The work is divided into five separately reviewed successors:

1. **Phase 620B — Mailbox Read And Draft Control Plane**
2. **Phase 620C — Scoped Agent And MCP Control Plane**
3. **Phase 620D — C2C Source, Planning, And Reviewed Outreach Control Plane**
4. **Phase 620E — Official Business Messaging Adapters**
5. **Phase 620F — LinkedIn Drafting And Approved Publishing Control Plane**

Each phase begins with deterministic fixtures and no-egress simulation. Each has its own data,
permission, privacy, abuse, provider, canary, and release gate. Read authority never implies draft
authority; draft authority never implies send; send never implies submit; submit never implies
social posting; and one provider's consent never grants another provider or purpose.

## Relationship To Phase 620A

Phase 620A remains the complete authority for the simulator-first owner-facing WhatsApp Business,
Apple Messages for Business, personal-iMessage prohibition, MessageUI boundary, channel linking,
closed command grammar, STOP/close precedence, and business-messaging effect contracts.

This plan does not edit, replace, widen, or reinterpret Phase 620A. In particular:

- `BusinessMessagingConnectionV1`, `BusinessMessagingConsentV1`, ingress envelope/item contracts,
  plans, approvals, dispatch attempts, effect receipts, and suppressions retain their Phase 620A
  meaning;
- WhatsApp remains legally and operationally fail-closed unless the exact current AI-provider,
  product-purpose, account, endpoint, and provider-controlled cohort eligibility is proven;
- Apple Messages for Business remains unavailable without a registered business and approved MSP
  relationship or separate Apple MSP approval;
- personal WhatsApp, WhatsApp Web, personal iMessage automation, and unattended SMS remain
  unsupported; and
- Phase 620A's owner channel does not authorize recruiter, employer, vendor, or C2C outreach.

Phase 620E may eventually implement provider adapters for Phase 620A's contracts. It may not change
their authority model merely to accommodate a provider SDK or marketing goal.

## Evidence Labels

This document uses exact evidence labels:

| Label | Meaning |
| --- | --- |
| **Public vendor claim** | A statement was present on a public first-party Tsenta or GiraffyReach surface on the evidence date. It is not independent proof that production behaves that way. |
| **Public contradiction** | Two current first-party surfaces materially differ or a claim is internally imprecise. |
| **Official provider fact** | A fact stated in current public Google, LinkedIn/Microsoft, Meta/WhatsApp, or Apple documentation. |
| **Current Bluey fact** | A fact visible in current local Bluey code or a current authority document. |
| **Architecture inference** | An original Bluey design choice derived from evidence and Bluey invariants. It is not a claim about a competitor's private implementation. |
| **Unknown** | Public evidence does not establish implementation, data rights, scope, retention, side effects, receipts, accuracy, or production behavior. |
| **Future gate** | Work requiring implementation, independent review, external authority, and explicit release approval. |

Visual similarity is not evidence of backend equivalence. A connected-looking tile is not OAuth
evidence, a tracker label is not provider evidence, an open pixel is not a human read, and a
successful HTTP response is not delivery.

## Clean-Room Method

The public-product pass reviewed only:

- Tsenta's homepage, messaging page, AI disclosure, privacy policy, terms, changelog, and MCP setup;
- GiraffyReach's homepage, C2C Autopilot, MCP/Agent Connect, privacy policy, terms, and security page;
- official Google Gmail scope documentation;
- official LinkedIn access and consumer API documentation;
- official WhatsApp Business Messaging Policy; and
- official Apple Messages for Business, Messages, and MessageUI documentation already pinned by
  Phase 620A.

No sign-in or authenticated competitor workflow was required for this pass. Search was used only
to locate public pages and to test whether GiraffyReach publicly documented WhatsApp, iMessage,
SMS, or C2C-chat product integrations. Absence from public results means only **not publicly
established**, not proof that no private or future implementation exists.

## Public Product Evidence

### Tsenta

#### Messaging

**Public vendor claims:**

- The public messaging page says users can apply to jobs through iMessage and also references
  WhatsApp.
- The homepage depicts an iMessage match alert, a user reply of `yes`, background application, and
  an application receipt.
- The AI disclosure describes iMessage/WhatsApp conversational signup and says a user replies
  `apply` to confirm each application.
- The AI disclosure advertises web, iMessage, email, and push notifications.

**Public contradiction:** `yes` and `apply` are both represented as the confirmation command. The
public command grammar is therefore not authoritative.

**Unknown:** public pages do not establish the sender identity, WhatsApp Business account,
WhatsApp provider, Apple business/MSP, phone-number link ceremony, consent capture, STOP/opt-out
grammar, template/window handling, channel delivery states, or channel-specific retention.

**Bluey decision:** adopt none of those undisclosed mechanics by inference. Phase 620A's exact
business identity, linking, consent, closed grammar, STOP, and truthful receipt model remains the
only proposed Bluey path.

#### Gmail And Recruiter Inbox

**Public vendor claims:**

- Gmail connection is optional.
- `gmail.readonly` is used to identify job-related mail, update the tracker, classify
  confirmations/recruiter replies/interviews/rejections, and read application OTPs.
- `gmail.compose` is used to save a suggested recruiter response as a draft for user review,
  editing, and sending.
- Tsenta says it does not autonomously send email from the user's account.
- Tokens are encrypted; disconnect stops sync and revokes access; matched messages already synced
  remain until individual or account deletion.
- Anthropic and OpenAI may process matched mail for classification and requested drafting under the
  policy described by Tsenta.

**Official provider fact:** Google's Gmail scope table states that `gmail.compose` permits both
draft management **and sending**. OAuth scope alone cannot enforce a draft-only product promise.

**Public contradiction:** the 2026 privacy policy first describes email as transient OTP-only
processing with no full-content storage or analysis, then later describes job-mail classification,
matched-message retention, tracker/inbox storage, and LLM processing. The older 2025 terms still
describe email access as OTP-only and say full contents are not read, stored, or analyzed.

**Bluey decision:** draft-only must be a server-enforced provider-method boundary, not copy, policy,
or scope naming. Bluey must align its UI, OAuth disclosure, privacy policy, terms, runtime methods,
logs, export, and deletion behavior before any live grant.

#### MCP And Application Receipts

**Public vendor claims:**

- MCP setup uses browser OAuth approval/deny.
- The client stores a short-lived OAuth token locally and refreshes it.
- The privacy policy says connector activity logs include tool, timestamp, and OAuth client ID and
  revocation invalidates access and refresh tokens.
- Public pages claim an agent can search, tailor, and submit applications.
- Application receipts may show submitted fields, open-ended answers, resume, cover letter, and
  confirmation details.
- A review-before-submit flow pauses for edit/approval, while auto-approve remains an available
  setting.

**Unknown:** public pages do not publish tool-level scopes, write-grant separation, step-up rules,
per-call content hashes, complete audit export, token audiences, retry/fencing behavior, or exact
effect receipts.

**Bluey decision:** MCP is only a policy facade over existing Bluey authority. It cannot create a
second source of truth, expose secrets, or convert a chat instruction into submit/send authority.

#### LinkedIn

**Public vendor claim:** LinkedIn profile/contact URLs appear as candidate or application data.

**Unknown:** no public evidence in the reviewed Tsenta surfaces establishes a connected LinkedIn
account, posting integration, recruiter messaging, or direct-message capability.

### GiraffyReach

#### C2C And Gmail

**Public vendor claims:**

- C2C Autopilot receives more than 3,000 requirements per day from recruiter channels and email
  blasts and refreshes around every five minutes.
- Users set title/skill criteria, weekday send windows, and tier-dependent caps; one public example
  describes 5 to 40 sends per day.
- The system tailors a resume and email and sends through the user's personal Gmail.
- Per-recruiter memory is intended to avoid duplicate contact.
- Replies land in Gmail and a push alert is shown.
- Server-side operation continues while the user's laptop is closed.

**Unknown:** public pages do not establish source licenses, source-specific terms, contributor
consent, exact parsing quality, permissible-contact basis, bounce/complaint handling, recipient
suppression propagation, or independent delivery rates.

#### Open Tracking And Auto Reply

**Public vendor claims:**

- Autopilot inserts invisible tracking beacons and records an email-open event.
- The same page says emails have no Giraffy footer or tracking footprint.
- Marketing copy says Auto Reply drafts responses from pre-approved rate, work-authorization, and
  interview-slot facts.
- Terms and privacy authorize the service to answer recruiter requests automatically when Auto
  Reply is enabled.

**Public contradictions:** an invisible tracking beacon is tracking even if it has no visible
footer. Public copy also does not consistently distinguish a saved draft from an automatically
sent reply.

**Bluey decision:** invisible open tracking is excluded from Phases 620B-F. If a later independent
phase ever proposes remote-resource telemetry, it must be opt-in, disclosed, privacy-reviewed, and
recorded as `remote_resource_requested`, never `human_read`.

#### Gmail Permissions, Retention, And User Controls

**Public vendor claims:**

- Automated outreach may compose/send and read only replies to threads the service created.
- Sent and reply copies are retained for the tracker; the rest of the mailbox is not stored.
- Gmail disconnect stops access immediately.
- Account deletion removes profile, resume, and outreach history within 30 days, while backup
  copies may remain up to 90 days.
- Security copy says OAuth refresh tokens, not passwords, are stored with separate-KMS encryption,
  minimum permissions, and logged token use by the outreach service.
- Disabling outreach stops future messages but cannot retract messages already sent.

**Unknown:** the reviewed public pages do not publish exact Gmail/Outlook scopes, a method-level
allowlist, provider grant revisions, revocation-race handling, or provider-side send reconciliation.

#### MCP / Agent Connect

**Public vendor claims:**

- A private revocable connector URL acts as a scoped access token.
- Actions are capped/logged and a 50-preparation daily ceiling is described.
- Public tools expose job search, profile/limits, preparation, tracker/form recipes and answers,
  Gmail OTP retrieval, progress, mark-applied, hiring contacts, outreach/open/reply intelligence,
  market intelligence, and resume-setting writes.
- A profile switch allows browser agents to submit; without it, the final click is handed to the
  user.
- Every field is claimed to be logged to the tracker first; CAPTCHAs and invented essay answers are
  said to be excluded.
- One tool description says job-site login credentials may be returned “when needed.”

**Unknown:** public pages do not establish standard OAuth for the connector URL, tool-by-tool
grants, step-up for secrets or submit, URL leak protections, revocation fencing, idempotency,
accepted/delivered states, or the public tool-count discrepancy.

**Bluey decision:** Bluey does not put bearer credentials in connector URLs and never returns a
password, cookie, raw OAuth token, provider refresh token, or generic OTP value to an agent.

#### LinkedIn

**Public vendor claims:**

- A paid tier advertises AI-written and AI-posted LinkedIn content.
- Privacy says a connected LinkedIn or Indeed account may provide a token and authorized profile
  data.
- Terms list LinkedIn as an example third-party account.
- MCP can return hiring-contact LinkedIn URLs.

**Official provider fact:** LinkedIn uses OAuth. Current open consumer permissions cover OIDC
profile/email and `w_member_social` for posting, commenting, and liking; most other products and
permissions require explicit LinkedIn approval. Those open permissions do not establish recruiter
DM authority.

**Unknown:** public GiraffyReach pages do not state requested LinkedIn scopes, provider product,
per-post review, audience choice, posting receipt, revocation behavior, or any DM/chat integration.

#### WhatsApp, iMessage, SMS, And C2C Chat

**Public evidence result:** no reviewed public GiraffyReach product page establishes WhatsApp,
iMessage, or SMS as a product messaging adapter. A vendor-owned blog identifies WhatsApp hotlist
networks as a C2C source category; that is not evidence of account integration, private-group
access, message ingestion, or delivery through WhatsApp.

Public recruiter interaction is Gmail/outreach/tracker oriented. Public “live chat” copy refers to
customer support, not an in-product candidate-to-recruiter C2C conversation system.

## Public Evidence That Must Stay Unknown

Public and normal customer-facing material cannot establish:

- either competitor's private database, queues, workers, cloud topology, schemas, prompts, models,
  source agreements, ranking weights, or failure recovery;
- completeness, legality, or contractual rights for every job/contact/C2C source;
- exact OAuth grants actually returned to a live tenant;
- exact token storage, refresh, rotation, revocation, incident, or operator-access behavior;
- actual deliverability, bounce, complaint, open, reply, interview, placement, or hire outcomes;
- exact application success or employer receipts;
- provider-specific idempotency and ambiguous-effect behavior;
- private WhatsApp, Apple, LinkedIn, Google, or Microsoft provider approvals; or
- authenticated export, deletion, rollback, abuse, or disaster-recovery evidence.

Bluey does not need these private facts. It needs explicit owned contracts, tests, evidence, and
release gates for its own implementation.

## Bluey Gap Table

| Dimension | Tsenta public evidence | GiraffyReach public evidence | Bluey target |
| --- | --- | --- | --- |
| WhatsApp | Owner-facing job commands and confirmation claimed; identity/consent/status undisclosed | No product-channel evidence | Phase 620A official business-owned channel only; closed owner commands; exact opt-in/STOP; no recruiter outreach |
| iMessage/SMS | Owner-facing iMessage command/receipt claimed; provider mechanics undisclosed | No evidence | Apple Messages for Business only after approval; otherwise user-presented MessageUI composer; no unattended personal iMessage/SMS |
| Gmail read | Job-mail classification, tracker, OTP | Reply-thread read for service-created outreach | Existing read-only inbox authority with exact grant revision, minimized projection, matched/unmatched review, deletion, and live provider gate |
| Gmail draft/send | Draft-only promise although `gmail.compose` permits send | Autonomous send and ambiguous draft/auto-reply language | Separate Bluey read/draft/send capabilities; method-level egress firewall; send remains a later independent release |
| Outlook | Older mailboxes reportedly use a processor; detail limited | Outreach connection claimed; scope detail absent | Provider-specific Microsoft Graph scopes, grant revision, delta/read behavior, and separate `Mail.Send` authority |
| LinkedIn | Profile/contact URL only | Posting claim; scope/approval/audit undisclosed | Contact URL and manual content export first; official OAuth only; exact per-post review later; no scraping/session automation/DM |
| MCP | OAuth and revocation/activity logging claimed | Broad bearer-URL tools, caps, and revoke claim | OAuth/DCR or equivalent reviewed flow; header-only audience-bound grants; read-only first; exact tool scopes and invocation receipts |
| Credentials/OTP | Public write behavior not fully documented | Tool may return OTP and job-site credentials | Secrets never cross the agent boundary; OTP is a one-attempt server capability, not an agent-visible string |
| C2C sources | Not publicly detailed | Recruiter channels/blasts claimed; rights undisclosed | Per-source rights/provenance/retention and sender evidence; no private group/feed ingestion without license/consent |
| C2C outreach | Not established | Gmail schedules/caps/dedupe and Auto Reply claimed | Planner/review simulator first; exact recipient purpose/cooldown/suppression; no send until provider/legal canary |
| Delivery/audit | Application receipts; channel delivery semantics absent | Tracker/open pixel; provider delivery semantics absent | Immutable plan/approval/request-start/effect receipt; accepted != delivered; pixel != read; unknown is terminal for blind retry |
| User controls | Review/auto-approve and Gmail revoke/delete | Schedules/caps/disable future outreach | Global off/review-only/limited; per-purpose grants; caps/windows/allowlists; pause/STOP/revoke/delete/export |
| Policy clarity | Material privacy/terms conflict | Auto-reply and tracking ambiguity | One generated capability matrix shared by UI, OAuth, privacy, terms, runtime, logs, and operations |

## Scope

### This plan defines

- the public evidence and unknown boundaries above;
- the relationship between current mailbox/communication authority and new channel-specific work;
- the Phase 620B-F delivery sequence and dependencies;
- conceptual PostgreSQL contracts and state machines;
- per-channel and per-purpose consent/capability rules;
- Gmail, Outlook, MCP, C2C, WhatsApp, Apple Messages, MessageUI, and LinkedIn boundaries;
- privacy, retention, deletion, export, abuse, suppression, and ambiguous-effect requirements;
- simulator, unit, database, concurrency, provider-sandbox, privacy, and end-to-end test matrices;
- an all-zero flag matrix; and
- external production gates and an implementation handoff.

### This plan does not

- modify Phase 620A or any current schema, type, enum, API, UI, provider adapter, operations file,
  generated bundle, feature flag, or release;
- grant OAuth, create a provider app, register a callback, create a connector, obtain a token, or
  store a credential;
- connect Gmail, Outlook, LinkedIn, WhatsApp, Apple Messages for Business, SMS, or iMessage;
- read an inbox, fetch an OTP, save a draft, send email, send a message, publish a post, or submit
  an application;
- ingest private recruiter emails, groups, Slack/WhatsApp hotlists, or contact data;
- add invisible open tracking or claim a remote-image request is a human read;
- copy competitor code, UI, assets, text, algorithms, prompts, schemas, or private data;
- use browser automation to operate LinkedIn, personal WhatsApp, WhatsApp Web, Gmail, iMessage, or
  another messaging product;
- treat user permission as provider approval, data-source rights, legal basis, or release authority;
- permit one phase's simulator evidence to satisfy another phase's live provider gate; or
- enable any external effect by default.

## Current Bluey Baseline

The following are **current Bluey facts**:

- `MailboxConnection` represents current Gmail/Outlook mailbox authority and capabilities.
- `JobsOAuthState` binds provider, PKCE verifier, account connection, purpose, requested scopes,
  requested capabilities, grant revision, return path, and expiry.
- `JobsProviderCredential` contains provider subject, access/refresh material, scopes,
  capabilities, monotonic grant revision/hash, and expiry inside the server boundary.
- `JobsProviderSyncState` owns cursor, schedule, errors, and database-backed lease state.
- `JobsProviderMessage` stores provider message identity, bounded content, correlation,
  classification, confidence, and processing state.
- `JobsCommunicationAction` and attempt/reconciliation evidence bind immutable content and authority
  hashes, exact application/connection/provider, approval/grant revisions, lease token/fence,
  provider operation key, request start, provider evidence, and conservative unknown outcomes.
- communication write fences and operational holds block new effects while preserving truthful
  handling of already-started work.
- current email/calendar provider grammar covers Gmail, Outlook email, Google Calendar, and Outlook
  Calendar. It does not authorize business messaging, C2C source ingestion, MCP, or LinkedIn.
- Phase 605 is source readiness, not provider or production evidence.
- `BLUEY_JOBS_MAILBOX_SYNC_ENABLED`,
  `BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED`,
  `BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED`, and
  `BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED` remain `0`.

### Reuse boundary

Successors should reuse Bluey's canonical hashing, immutable approval, grant revision, database
time, lease/fence, operational hold, request-start, reconciliation, deletion-drain, tenant, and
privacy principles. They must not reinterpret existing receipt rows or overload one serialized
type until an exact reviewed migration exists.

Specifically:

- `MailboxConnection` remains mailbox-specific; Phase 620A business connections remain separate.
- `JobsCommunicationAction` may continue to own exact Gmail/Outlook reply or calendar actions, but
  C2C source evidence, agent grants, and LinkedIn publication need distinct authority records.
- a shared UI timeline may project several domains, but it does not merge their underlying state
  machines or permissions.

## Architectural Invariants

1. **Phase 620A is preserved.** No successor weakens its business/personal channel boundary.
2. **Purpose is authority.** A provider grant is usable only for the reviewed connection, purpose,
   action class, recipient class, and data classes.
3. **Read, classify, OTP, draft, send, submit, and post are distinct.** Provider scope overlap does
   not collapse Bluey's capability model.
4. **Provider grants are facts, not consent.** Requested scopes, returned scopes, user consent,
   Bluey policy, and release flags are independently required.
5. **OAuth scope is not a method firewall.** Where a scope grants broader methods than the product
   exposes, server routing and credential custody mechanically deny the broader methods.
6. **No secrets cross the agent boundary.** Passwords, cookies, raw OAuth tokens, refresh tokens,
   connector bearer URLs, provider secrets, and generic OTP values are never agent output.
7. **The agent is not a source of truth.** It reads bounded projections and proposes immutable
   plans; server authority approves and executes.
8. **Exact content before effect.** Recipient/thread, body, subject, attachments, resume, facts,
   source, schedule, provider connection, and consent revision are hashed before approval.
9. **No silent plan edits.** Every edit creates a new plan revision and invalidates approval.
10. **Provider acceptance is not delivery.** Only authenticated provider evidence may advance an
    effect state.
11. **Remote resource load is not a human read.** Open pixels are excluded in these phases.
12. **Request-start ambiguity is terminal for automatic retry.** `side_effect_unknown` is not a
    retry queue.
13. **Suppression precedes convenience.** STOP, provider close, opt-out, bounce/complaint hold,
    disconnect, deletion, account hold, and kill switch are checked at every pre-effect boundary.
14. **C2C source rights precede ingestion.** A technically reachable feed or group is not an
    authorized source.
15. **Contact provenance is mandatory.** A contact needs source, method, time, permissible purpose,
    correction, suppression, and expiry; `verified=true` alone is invalid.
16. **Database time is authority.** Leases, scopes, grants, plans, approvals, caps, quiet hours,
    windows, and retention decisions use post-lock database time.
17. **PostgreSQL is live authority.** SQLite remains deterministic local/test parity, not a second
    production authority.
18. **Flags do not grant authority.** A `1` cannot substitute for provider approval, consent,
    exact grants, a signed release, a cohort, or a clear hold.
19. **Every phase is no-effects by default.** Simulator availability cannot accidentally start a
    live worker or route.
20. **Marketing follows receipts.** Bluey copy may claim only states proven by the deployed
    authority and provider evidence.
21. **Low-entropy lookup keys are keyed and domain separated.** Recipient, provider subject,
    email, phone, domain, conversation participant, and agent invocation-output lookup indexes use
    the current server-side `private_lookup_hash` keyed-HMAC boundary with an explicit
    domain/version; they never use plaintext, an unsalted digest, or a raw SHA-256 lookup key.

## Target Topology

This is an **architecture inference**, not an implemented service map:

```text
Official provider / licensed source / deterministic simulator
        |
        v
Provider edge: auth -> bounded envelope -> immutable ingress item
        |
        v
Connection + grant + consent + suppression + source-rights guards
        |
        v
Minimized classifier/correlator -> owner-visible evidence projection
        |
        +---------------------> read-only agent resource registry
        |
        v
Deterministic planner -> exact preview -> authenticated review/step-up
        |
        v
Global/provider/tenant/account/application/thread/action holds
        |
        v
Fenced provider-method adapter OR no-egress simulator
        |
        v
Attempt evidence -> provider reconciliation -> truthful receipt/timeline
```

The provider edge, planner, agent gateway, dispatch worker, and reconciler may be separate modules,
but they consume one PostgreSQL authority and one generated capability registry. A model call never
sits on the webhook acknowledgement path or inside an authority transaction.

## Phase 620B — Mailbox Read And Draft Control Plane

### Purpose

Complete a transparent mailbox outcome and recruiter-reply workflow without sending email.

### Initial product boundary

Allowed in deterministic simulation, then provider sandbox only after separate authority:

- connect one exact Gmail or Outlook mailbox through reviewed OAuth;
- read bounded job-related messages under current read-only authority;
- correlate a message to one application or place it in explicit unmatched/ambiguous review;
- classify confirmation, recruiter reply, interview invitation, assessment, rejection, offer,
  verification-required mail, or unsupported mail;
- notify the owner that a fresh application verification intervention needs attention without
  extracting, returning, or consuming an OTP;
- show a minimized, owner-only projection and exact evidence source;
- prepare a reply plan from verified candidate facts;
- save an approved draft through an exact draft-only adapter; and
- disconnect, revoke, delete, export, pause, and suppress.

Not allowed in Phase 620B:

- sending, scheduling, or auto-sending email;
- autonomous negotiation, compensation/rate agreement, work-authorization/legal representation,
  interview acceptance, or calendar write;
- scanning mail for unrelated purposes;
- extracting, returning, autofilling, or consuming an OTP;
- forwarding bodies or verification material to a generic agent;
- attaching or transporting a resume, cover letter, identity file, or any other document;
- using mailbox content for pooled training or advertising; or
- labeling a draft as sent.

Every Phase 620B plan and provider request has `attachment_refs=[]`. A nonempty attachment or
inline-document reference is a closed schema error before approval, lease, or provider I/O.

### Read OAuth/connect release boundary

Mailbox connection is a live provider effect even when its resulting capability is read-only. A
separate `BLUEY_JOBS_MAILBOX_READ_OAUTH_ENABLED` flag, default `0`, gates all Google and Microsoft
read-OAuth lifecycle boundaries independently from mailbox sync and communication-write flags.

A future `MailboxOAuthProviderReleaseV1` must bind:

- provider, exact authorization/token/revocation hosts, OAuth client ID hash, redirect URI, scopes,
  authorization purpose, delegated authorization-code grant class, exact mailbox subject, PKCE
  policy, state schema, callback schema, and token parser;
- signed binary/release and provider-method-policy hashes;
- exact tenant/cohort/account eligibility and start/end database times;
- privacy, provider-policy, restricted-scope/security-assessment, and operator approvals;
- monotonic activation/revocation generation and kill-switch revision; and
- state `staged`, `active`, `revoked`, `expired`, `terms_changed`, or `review_required`.

OAuth start, callback, token exchange, credential commit, refresh claim, refresh request start, and
refresh CAS commit each recheck, under the required lock order and post-lock database time:

1. the read-OAuth flag is `1` for that exact service role;
2. the signed provider release is active and unexpired;
3. the authenticated account is in the exact approved cohort;
4. the purpose/scopes/redirect/provider match the release and one-time state;
5. the account/connection is not held, draining, disconnected, deleted, or grant-revoked; and
6. the activation/revocation generation has not changed since claim.

The start route cannot mint state while disabled. The callback cannot exchange a code or activate a
connection merely because state was minted before disable. The refresh worker cannot claim, call,
or commit while disabled. Disabling the flag or revoking the release increments the durable
revocation generation, fences pending states and refresh leases, and prevents new usable authority.

Consumer mailbox connections accept only delegated authorization-code authority for the exact
authenticated owner and exact mailbox subject. Client-credentials/application-role grants,
service-account or domain-wide delegation, tenant/admin-wide authority, and shared-mailbox access
are rejected before credential commit. Supporting any such enterprise authority requires a
separate product, privacy, tenant-administration, impersonation, consent, audit, and launch design;
it cannot be inferred from a broadly scoped Google or Microsoft token.

If disable/revoke wins before token request start, no provider request occurs. If a token exchange
or refresh was durably request-started first, Bluey records the result truthfully but may commit it
only as quarantined/draining credential evidence with zero usable capability, then performs a
separately authorized provider revocation/cleanup path. It must never activate the connection,
resume sync, or silently discard an ambiguous provider-side token issuance. A disabled callback
returns a minimized retry-later/disabled result without leaking whether another account or cohort
would be eligible.

### Gmail scope and method boundary

The preferred read grant remains `openid`, `email`, and `gmail.readonly`. Draft creation may require
`gmail.compose`, which Google documents as both draft and send authority. Therefore:

1. Bluey records requested and provider-returned scopes separately.
2. A new `mail_draft` consent/grant revision is distinct from `mail_read`.
3. The credential is available only to a provider adapter whose dispatch registry contains the
   reviewed draft-create/update methods.
4. Gmail `messages.send` and `drafts.send` are absent from the draft adapter's callable surface.
5. The network/egress policy rejects unregistered Gmail method/path combinations before provider
   I/O.
6. Tests inject a credential with the broad provider scope and prove send methods remain
   structurally unreachable.
7. A future send phase must use a new Bluey capability, approval, adapter release, flag, and canary;
   it cannot reuse a Phase 620B draft receipt.

Both `gmail.readonly` and `gmail.compose` are Google restricted scopes. Before live OAuth, Bluey
must complete the current Google Workspace API User Data and Developer Policy and Google API
Services User Data Policy/Limited Use review, OAuth verification, scope justification, data-use and
human-access disclosures, approved subprocessor/transfer model, deletion/export behavior, and any
required restricted-scope security assessment. A working client ID or consent screen is not this
authority. Missing, stale, rejected, or scope-expanded assessment evidence keeps read OAuth, sync,
draft, and every downstream capability disabled.

### Outlook scope and method boundary

Read and draft behavior must use current reviewed Microsoft Graph scopes and exact returned grants.
`Mail.Send` is never requested or consumed in Phase 620B. If server-side draft persistence needs
`Mail.ReadWrite`, the consent screen must disclose that scope's breadth and a signed Microsoft
method policy must allow only the exact reviewed draft create/update/read-back operations. The
denylist must mechanically reject `sendMail`, message `send`, forward, reply/reply-all send,
attachment upload, mailbox-rule/settings mutation, and every unregistered method/path before
provider I/O. The exact current Microsoft Graph permissions reference and mail-method documentation
must be pinned into the provider release and re-reviewed when changed. Google and Microsoft grants,
method policies, receipts, and canaries remain provider-specific; success on one provider does not
certify the other.

### Mailbox subscription, cursor, and reconciliation authority

Bluey must not represent a mailbox as current from timestamp polling or a successful webhook
alone. Each provider uses an immutable `MailboxSyncCursorAuthorityV1` bound to:

- account, exact mailbox subject, provider, connection/grant/revocation generations, delegated
  permission class, provider release, adapter and parser revisions;
- subscribed resource, provider subscription/watch identity hash, notification-authentication
  mode/key generation, creation/renewal/expiry database times, and a monotonic sync epoch;
- provider cursor type and encrypted value, last contiguous provider checkpoint, last successful
  reconciliation database time, and cursor CAS generation;
- exact cohort, sync flag, holds, retention authority, and kill-switch revision; and
- health `initial_sync`, `current`, `notification_delayed`, `cursor_gap`, `full_resync_required`,
  `reconciling`, `reauthorization_required`, `held`, `revoked`, or `expired`.

For Gmail, the adapter authenticates the exact Pub/Sub notification path, renews `watch` before its
provider expiry, advances only through contiguous `historyId` results, and performs periodic
reconciliation because notifications may be delayed or dropped. A missing/expired history range,
including the provider's history-expired `404`, transitions to `full_resync_required`; it never
skips to the latest cursor. Full sync records a new epoch and reconciles additions, changes, and
deletions before health can return to `current`.

For Microsoft Graph, the adapter authenticates notification validation and delivery, binds the
subscription/client-state or encrypted-resource-data authority, renews before expiry, persists the
exact delta cursor, and handles lifecycle notifications. Invalid or expired delta state, including
`410 Gone`/`syncStateNotFound`, transitions to a new full-resync epoch with the same reconciliation
requirements.

Watch/subscription creation and renewal, notification acceptance, sync claim, provider request
start, cursor commit, and full-resync commit each recheck the exact release, flag, cohort, delegated
grant, mailbox subject, revocation generation, and holds. Notifications are acknowledged on a
bounded non-model path, deduplicated by provider evidence, and cannot directly mutate an
application. Cursor updates use monotonic compare-and-swap; an old worker cannot overwrite a newer
checkpoint. The UI shows last contiguous sync time, subscription expiry, and `stale`, `gap`, or
`reconciling` state explicitly; it never labels a connection “synced” while continuity is unknown.

### Hostile mailbox-content boundary

Every mailbox object is untrusted data. The provider release pins exact maximum raw/decoded sizes,
MIME nesting depth, part count, header count/length, recipient count, subject/body length, charset
allowlist, decode budget, and parser/sanitizer revisions. Malformed, recursive, oversized,
ambiguous, or unsupported content is quarantined before model or UI use.

- Plain text is preferred. HTML is converted through a reviewed isolated sanitizer/text extractor;
  raw HTML is never inserted into a Bluey or portal DOM.
- Scripts, styles, forms, SVG, active content, event handlers, embedded objects, remote fonts,
  images, CSS, tracking pixels, and other remote resources are removed and never fetched.
- `javascript:`, `data:`, `file:`, custom, credential-bearing, noncanonical, and otherwise
  unapproved URL schemes are rejected. Visible links remain untrusted text until a separate
  user-present navigation policy validates them.
- `From`, `Reply-To`, `Return-Path`, provider message/thread IDs, and available SPF, DKIM, DMARC,
  ARC, alignment, and provider-warning evidence are projected separately. Mismatch or missing
  authentication remains visible and non-authoritative; it cannot establish recruiter identity.
- Message content cannot issue instructions, select tools, grant authority, alter prompts, fetch a
  URL, approve an action, or override saved candidate facts. Classifiers receive a closed,
  minimized schema with explicit untrusted-data delimiters and adversarial prompt-injection tests.
- Attachments and inline documents remain unavailable in Phase 620B even if their MIME metadata is
  visible; no parser, renderer, model, or draft path opens them.

### Verification notification now; OTP consumption later

An OTP is not mailbox read output and Phase 620B has no live OTP consume capability. The read path
may classify a bounded message as `verification_intervention_possible` and notify the owner through
the authenticated Bluey review surface. It does not extract, display, return, autofill, or consume
the code and does not expose a provider query that searches broadly for one.

A later separately flagged and reviewed subphase may propose `OtpConsumeAuthorityV1` only when it
binds all of the following at claim and immediately before use:

- fresh authenticated owner intervention and current MFA/session authority;
- exact application, application revision, Career Track, verified application identity, and
  current application-evidence/integrity revisions;
- exact active local or managed runner attempt, lease owner, opaque lease-token hash, monotonic
  fence, signed runner release, and provider/ATS certification release;
- exact provider connection/grant and mailbox message/evidence identity;
- exact ATS challenge type, challenge/transaction identifier hash, employer origin and canonical
  host, expected sender/domain, and one reviewed verification purpose;
- database-time expiry bounded by the owner intervention, ATS challenge, runner lease, application
  attempt, and provider evidence, taking the earliest boundary;
- one server-side consumer, one use result, and no transfer to the model, agent, portal list,
  desktop log, runner log, export, or normal trace; and
- deletion or irreversible redaction of the code after the bounded attempt, subject only to an
  approved keyed lookup/audit record that cannot recover the value.

Any missing, stale, mismatched, superseded, submitted, request-started, unknown, or uncertified
authority denies consumption. The later subphase requires its own flag, provider sandbox, ATS
fixture, runner concurrency matrix, privacy review, and protected launch; it cannot be enabled as
part of Phase 620B mailbox read or draft.

### Draft planning

The reply plan binds:

- exact source thread/message and participant projections;
- application, employer, job, Career Track, identity, resume, and candidate-fact revisions;
- purpose and permitted topic taxonomy;
- subject, body, recipients, and reply threading fields, with `attachment_refs=[]`;
- model/provider/prompt-policy revision where generation is used;
- unsupported-claim scan and user-visible fact citations;
- connection, grant, consent, release, cap, hold, and suppression revisions;
- preview hash, expiry, and required step-up; and
- a draft-only effect class.

Rate, compensation, work authorization, immigration, location, availability, and interview-slot
facts must be exact saved candidate facts and shown before approval. A missing or changed fact
invalidates the plan.

### Phase 620B exit gate

- deterministic no-network matrix passes;
- provider method allowlist proves send is unreachable despite broad Gmail scope;
- PostgreSQL/SQLite parity and PostgreSQL concurrency/race matrix pass;
- live OAuth applications, redirect URIs, consent copy, security, privacy, retention, deletion, and
  support are separately approved;
- authorized Gmail and Outlook sandbox matrices pass for grant, refresh, revoke, disconnect,
  delegated-grant enforcement, subscription/watch creation and renewal, notification
  authentication, cursor continuity, gap/full-resync reconciliation, hostile MIME/HTML handling,
  draft create/update/read-back, outage, throttle, ambiguous request, and deletion;
- exact-tip CI and independent review are green; and
- all production sync, draft, dispatch, and reconciliation flags remain `0` until a distinct launch
  decision.

## Phase 620C — Scoped Agent And MCP Control Plane

### Purpose

Expose bounded Bluey Jobs read and preparation resources to trusted agents without exposing
credentials or turning an LLM instruction into external-effect authority.

### Release 1 capabilities

The first release is read-only or reversible preparation:

- `jobs.search`;
- `jobs.get` with source/freshness/provenance projection;
- `tracks.list` and `tracks.get_capabilities`;
- `applications.list` and `applications.get_receipt`;
- `usage.get`;
- `application.preview_preparation_requirements`, a pure bounded read that reports current
  readiness, missing review, and the authorities a future preparation would require without
  persisting a plan, reserving capacity, running generation, or mutating application state;
- `communication.list_review_items` with minimized bodies;
- `communication.get_draft_plan` without provider credentials; and
- `agent.get_capability_registry` generated from the deployed registry.

No Phase 620C Release 1 tool may send, submit, post, change provider settings, change resume policy,
fetch a password, return an OTP, obtain a raw token, create a provider grant, create a preparation
plan, reserve a generation/application unit, or mutate a Jobs record. Any persisted plan request is
a separately flagged write subphase under `BLUEY_JOBS_AGENT_GATEWAY_WRITE_ENABLED=0`.

### Agent authorization

A future implementation must use reviewed OAuth authorization-code flow with PKCE and, where the
MCP client ecosystem requires it, reviewed dynamic client registration. If a non-DCR client is
supported, it still receives a header-only, short-lived, audience-bound grant; bearer secrets never
appear in connector URLs, query strings, fragments, command history, logs, referrers, screenshots,
or support exports.

### Pinned MCP authorization profile

The first implementation must pin the exact MCP Authorization specification dated 2025-06-18 and
its content hash in the signed capability release. “Latest MCP” is not a release input. A later MCP
revision requires compatibility review, new fixtures, and a new activation.

The pinned profile requires:

- OAuth Protected Resource Metadata under RFC 9728, with the resource server's exact HTTPS metadata
  URL and authorization-server list bound to the signed release;
- authorization-server discovery through validated RFC 8414 metadata, or an explicitly pinned OIDC
  discovery profile where the release permits it;
- RFC 8707 resource indicators in authorization and token requests and exact resource/audience
  validation at the Bluey MCP resource server;
- PKCE for authorization-code clients and exact redirect URI matching;
- a reviewed client-registration mode: pre-registered client, RFC 7591 Dynamic Client Registration,
  or the exact Client ID Metadata Document profile named by the pinned MCP specification;
- short-lived access tokens, refresh-family rotation, grant/revocation generation, and no token in a
  URI; and
- a generated capability/scope registry whose version is included in protected-resource metadata,
  authorization requests, grants, and invocation receipts.

The Bluey MCP server rejects token passthrough. It neither accepts a Google, Microsoft, LinkedIn,
WhatsApp, browser-session, or another resource server's token as its own nor forwards the Bluey MCP
access token to an upstream provider. Every downstream provider call uses a separately stored,
server-owned provider credential and the underlying provider/action authority. Token exchange,
on-behalf-of, or delegation is unsupported unless a later phase defines and reviews one exact
standard flow.

### Discovery, registration, redirect, and SSRF boundary

- Protected-resource and authorization-server metadata are fetched only from exact HTTPS origins
  derived and validated under the pinned specification and signed Bluey allowlist.
- Metadata URLs, issuer, authorization/token/registration endpoints, JWKS locations, and client-ID
  metadata URLs reject userinfo, fragments, noncanonical ports, IP literals unless explicitly
  approved, private/link-local/loopback/multicast/reserved addresses, DNS rebinding, mixed-script or
  noncanonical hosts, oversized documents, invalid content types, excessive redirects, and
  cross-origin redirects not allowed by the pinned profile.
- Client ID Metadata Document mode requires the `client_id` to be the exact canonical HTTPS
  metadata-document URL, a bounded signed/cached document, an exact redirect-URI set, and a stable
  metadata hash. A fetched document cannot introduce a token endpoint, registration endpoint, or
  broader scope authority.
- DCR calls only the exact registration endpoint from validated authorization-server metadata.
  Registration requests use an exact metadata schema and redirect allowlist; responses are bound to
  the returned client ID, metadata hash, issuer, registration generation, and expiry. DCR never
  self-grants a scope or trusts a client-supplied software statement without a separately pinned
  trust root.
- Production redirect URIs are exact HTTPS values with no wildcard, userinfo, fragment, or open
  redirect. Loopback redirects are allowed only for an explicit local-client class under the pinned
  MCP profile and are never accepted for web clients.

### MCP 401 and 403 behavior

- A missing, malformed, expired, wrong-issuer, wrong-audience/resource, revoked, or otherwise invalid
  token returns `401 Unauthorized` with a standards-compliant `WWW-Authenticate: Bearer` challenge
  and the exact RFC 9728 `resource_metadata` reference where required. The challenge contains no
  tenant, token, body, or existence-sensitive detail.
- A valid token that lacks the required tool scope returns `403 Forbidden` with
  `insufficient_scope` and only the closed scope information permitted by the registry. It does not
  return `401`, trigger refresh loops, or disclose whether a cross-account resource exists.
- Authorization-server, metadata, or client-registration failures are not converted into tool
  results. They are bounded OAuth errors with privacy-safe correlation IDs.

### MCP HTTP session and resumption authority

An MCP transport session ID is a routing hint, never authentication or authorization. Every initial,
continued, and resumed HTTP request revalidates the bearer grant, exact resource/audience, account,
client, tool scope, capability-registry generation, revocation generation, holds, and expiry.

`AgentTransportSessionV1` uses a cryptographically random opaque ID with short database-time expiry
and rotation. It binds account, client ID/trust class, grant/family revision, RFC 8707 resource,
protected-resource metadata hash, capability-registry revision, transport mode, origin policy,
creation/last-use/expiry times, and a monotonic session generation. A session ID presented without
valid current authorization yields the same privacy-safe denial as no session; it cannot recover
prior results or reveal whether the session exists.

Resumable event IDs, stream cursors, and cross-node queues bind the same tuple plus invocation ID,
canonical event hash, sequence, bounded retention, and queue generation. They are tenant-isolated,
encrypted where sensitive, rate/size bounded, and compare-and-swap advanced. Revoke, disconnect,
grant rotation, registry change, account hold/deletion, expiry, or transport-origin change closes
the stream and prevents replay. Nodes never accept a client-chosen queue, account, event range, or
session binding. Stolen IDs, forged/cross-account resumes, cross-node injection, cursor rollback,
revoke-during-stream, and reconnect-after-expiry are mandatory negative tests.

The signed release pins both the 2025-06-18 MCP authorization profile and an immutable content hash
for the reviewed session-hijacking/security-practices artifact. A mutable documentation redirect is
not release evidence; a later security artifact or transport revision requires a new review and
activation.

`AgentClientGrantV1` binds:

- account, client ID, redirect identities, client metadata hash, and client trust class;
- exact MCP specification, protected-resource metadata, authorization-server issuer/metadata,
  registration mode/generation, tool scopes, RFC 8707 resource, and token audience;
- Career Track and optional application/workspace bounds;
- data-class ceiling and PII projection policy;
- access-token lifetime, refresh family, rotation generation, and revocation generation;
- terms/disclosure/capability-registry revisions;
- creation, last-use, expiry, revoke, and delete database times; and
- no provider/mailbox/browser credential material.

Every tool call creates an `AgentInvocationReceiptV1` with client, grant revision, tool version,
canonical input/output integrity hashes, policy decision, bounded resource IDs, time, and result.
If output lookup or deduplication is required, its index is a domain-separated keyed HMAC under
`private_lookup_hash`, never the raw output, an unspecified hash, or the integrity SHA. Bodies and
secrets are excluded from normal logs.

### Write expansion

No write tool is part of the initial Phase 620C implementation. A later expansion may expose a
plan request, but an external effect requires:

1. a live, proven customer path for the same action;
2. a separately granted tool scope;
3. an exact immutable plan created by server authority;
4. authenticated Bluey web step-up with current MFA;
5. approval bound to content hash, provider, recipient, expiry, and effect class;
6. current underlying provider grant and release authority;
7. all holds/suppressions clear at claim and request start; and
8. the underlying provider receipt returned by reference, not fabricated by the agent.

An agent cannot approve its own plan, upgrade its own scopes, or treat conversation history as
durable authorization.

### Phase 620C exit gate

- schema-generated tool registry and public docs agree exactly;
- unknown/deprecated tools and extra fields fail closed;
- RFC 9728 resource metadata, RFC 8414 authorization-server discovery, RFC 8707 resource/audience,
  PKCE, CIMD/DCR, redirect, SSRF/DNS-rebinding, token-passthrough, tenant/resource isolation, scope
  downgrade, revocation, rotation, replay, HTTP session/resumption, cross-node queue isolation, and
  `401`/`403` tests pass;
- active-client/session inventory and revoke-one/revoke-all behavior are proven under concurrency;
- prompt injection and confused-deputy tests prove no hidden tool or scope escalation;
- secrets/PII/privacy scans pass;
- load/rate/cap and audit-export tests pass;
- no tool returns credentials, cookies, raw OAuth material, passwords, or OTPs; and
- read-only agent flags remain `0` until a distinct reviewed launch.

## Phase 620D — C2C Source, Planning, And Reviewed Outreach Control Plane

### Purpose

Build a separate, truthful recruiter-distribution product for W2, 1099, and C2C opportunities.
This phase does not reinterpret an email send as a job application or treat a hotlist as an
employer ATS source.

### Source-rights registry

Every source needs an immutable `C2CSourceRightsRevisionV1` containing:

- source owner/provider, source class, acquisition method, endpoint/feed/group identity hash, and
  jurisdiction;
- contract/license/permission evidence and exact permitted use;
- whether Bluey may ingest, retain, display, match, contact, derive, or redistribute;
- contributor/sender notice and consent requirements;
- data categories, recipient/contact fields, and prohibited fields;
- retention, correction, deletion, audit, and downstream-provider obligations;
- terms snapshot, reviewer, decision, effective/expiry time, and evidence hash; and
- state `approved`, `denied`, `expired`, `terms_changed`, or `review_required`.

Missing or stale rights authority denies ingestion. Technical reachability, public indexing,
membership in a private group, a forwarded email, or competitor use is not permission.

### Requirement model

A normalized requirement preserves:

- source record and immutable body/object hash;
- sender identity/domain and verification method/time;
- staffing vendor, implementation partner, end client, and uncertainty separately;
- engagement type: full-time, W2 contract, 1099, C2C, contract-to-hire, or unknown;
- pay/rate unit, range, currency, duration, location, work mode, travel, clearance, authorization,
  sponsorship, corporation/vendor requirements, and every unknown explicitly;
- canonical role/skills/location plus raw source values and confidence;
- duplicate/chain-of-vendors relationships without destructive merge;
- observed, published, received, verified, expires, and last-confirmed times; and
- source rights, parser, taxonomy, and original-source verification revisions.

Unknown hard eligibility facts fail closed for autonomous selection. A recruiter email is not an
employer-controlled posting unless exact evidence establishes that relationship.

### C2C planning

The first implementation produces a simulated outbox only:

- exact eligible Career Track and contractor/employer facts;
- exact requirement and source-rights revision;
- exact recruiter/vendor recipient with provenance, purpose, and contact basis;
- recipient cooldown, per-domain and per-recipient dedupe, daily reservation, quiet hours, and
  schedule window;
- tailored resume and message diff grounded only in verified candidate claims, represented as a
  Bluey-local proposed document reference that cannot enter a provider request;
- visible employer/vendor chain and uncertainty;
- user approval with the exact content and Bluey-local proposed-document hash and expiry; provider
  attachment references remain empty;
- suppression checks for opt-out, bounce, complaint, prior contact, provider, domain, source, and
  account; and
- simulated attempt/effect receipt only.

Every Phase 620D outreach provider projection has `attachment_refs=[]`. Preparing, previewing, or
approving a local resume does not authorize its transport.

### Separate C2C document transport gate

A later document-transport subphase requires
`BLUEY_JOBS_C2C_DOCUMENT_TRANSPORT_ENABLED=0` to be independently reviewed and changed. Its
`C2CDocumentTransportAuthorityV1` must bind:

- exact C2C plan, recipient, purpose, requirement, source-rights, contact-purpose, Career Track,
  application identity, and consent revisions;
- exact immutable resume/document revision, plaintext content hash, encrypted object identity,
  filename, media type, byte length, page count, and attachment order;
- malware/content-type/package validation, PDF/DOCX reopen/read-back, sensitive-data disclosure,
  unsupported-claim, and recipient-appropriateness evidence;
- exact provider method/release, provider attachment limits, cap/spend reservation, approval hash,
  database expiry, and suppression/hold revisions; and
- provider upload/object/message evidence with request-start and ambiguous-effect handling.

The subphase has its own provider sandbox, privacy/legal review, document lifecycle/deletion,
malware, corruption, duplicate-upload, timeout, unknown-effect, and recipient-disclosure matrices.
It cannot be enabled by C2C planning or outreach flags and cannot reuse a local preview hash as a
provider upload receipt.

### C2C replies and “chat”

Bluey may project an email thread into an owner-visible C2C conversation view, but the projection
does not create a new chat transport. Every item retains the provider thread/message evidence and
direction. A free-form C2C assistant may propose a reply plan; it cannot send or negotiate.

The initial safe fact taxonomy may include only exact saved values for:

- work authorization category;
- preferred engagement type;
- location/work-mode preference;
- available start date or reviewed availability window;
- target rate/range when explicitly stored for that Career Track; and
- interview availability as a proposal, not a calendar commitment.

Client identity, pay agreement, exclusivity, representation rights, immigration/legal advice,
background-check consent, contract terms, sensitive documents, and final interview scheduling
always require fresh authenticated human review and may remain entirely unsupported.

### Phase 620D exit gate

- each enabled source has current rights authority and a source-specific conformance suite;
- fixture and provider-sandbox ingestion proves immutable provenance, dedupe, correction, deletion,
  expiry, and sender/domain evidence;
- compliance approves recipient purpose, opt-out, suppression, CAN-SPAM/CASL/GDPR and applicable
  state/jurisdiction handling;
- cap, reservation, cooldown, duplicate, bounce, complaint, STOP, revoke, and race tests pass;
- unsupported claims and document disclosures fail closed;
- simulated outbox proves zero provider calls; and
- ingestion, planning, outreach, and document-transport flags remain `0`; live sending or document
  attachment requires its own exact provider canary and launch authority.

## Phase 620E — Official Business Messaging Adapters

### Purpose

Implement, without weakening, the Phase 620A simulator contracts for eligible official business
channels.

### WhatsApp Business Platform

No live work may begin unless Phase 620A's exact `WhatsAppProviderEligibilityV1` is current and
`eligible`. User opt-in, a WABA, a business phone, an approved template, or a token cannot replace
that product/legal eligibility record.

If eligible in a future separately authorized release, the adapter must use only official
WhatsApp Business Platform APIs and authenticated webhooks. It must enforce:

- Bluey business identity, not candidate personal identity;
- exact business endpoint, WABA, grant, template, locale, category, and adapter release;
- explicit owner opt-in naming Bluey and exact message purposes/categories;
- approved templates for business-initiated conversations and the exact 24-hour service window;
- STOP/block/discontinue requests from any supported channel;
- clear human/support escalation;
- authenticated multi-item webhook envelope/item identity and conflict semantics;
- provider-specific accepted/delivered/read states only from authenticated evidence;
- absolute exclusion of WhatsApp Business Solution Data from model training, fine-tuning,
  evaluation, benchmark, embedding corpora, reward signals, and generalized serving improvement;
  and
- no recruiter/C2C outreach without an independent recipient opt-in and product/legal authority,
  which is not proposed by this phase.

### Apple Messages for Business

The adapter remains simulator-only until Bluey has a registered business and approved MSP
relationship or separate MSP qualification. It must use Phase 620A's JWT, key rotation, destination
business, opaque subject, close, invitation, and `410 Gone` rules. A gateway `200` proves at most
`provider_accepted` unless current official evidence supports a stronger state.

### Personal iMessage And SMS

No background personal-iMessage or SMS adapter is permitted. A later native-only phase may present
Apple's MessageUI composer in direct response to a user gesture. The user can edit, send, or cancel.
Bluey may record `presented`, `cancelled`, or `user_selected_send`; it cannot manufacture
`provider_accepted`, `delivered`, or `read` without separate provider evidence.

### Phase 620E exit gate

All Phase 620A simulator tests remain green, plus current provider terms/eligibility, registered
business/provider artifacts, webhook security, key/token rotation, template/window, STOP/close,
privacy, deletion, incident, sandbox, rate/spend, support, canary, rollback, and kill-switch gates.
Every live flag remains `0` until a protected exact-release launch decision.

## Phase 620F — LinkedIn Drafting And Approved Publishing Control Plane

### Purpose

Offer evidence-grounded professional content preparation without scraping LinkedIn or automating a
user's browser/session.

### Release 1: draft and manual export

The initial implementation requires no LinkedIn OAuth and performs no provider write. It may:

- generate a draft from Bluey-owned, consented market/source aggregates and verified candidate
  facts;
- show citations/provenance, unsupported-claim checks, audience suggestion, and a complete diff;
- let the user edit, copy, or export the text manually; and
- record only Bluey draft/review events, never a LinkedIn publication receipt.

It may not promise engagement, invent experience, disclose confidential employer/source data, or
use application/mailbox content beyond its exact consented purpose.

### Future official posting

Only an official, approved LinkedIn application/product may post. A future
`LinkedInPublicationPlanV1` binds:

- account and official LinkedIn provider subject;
- returned OAuth scopes and monotonic grant revision/hash;
- exact content, media, link, audience, alt text, and visibility hashes;
- source/provenance and candidate-claim revisions;
- disclosure and content-policy scans;
- per-post authenticated approval, MFA step-up, and short expiry;
- exact `w_member_social` or then-current official capability and adapter release;
- provider object identity and response evidence; and
- edit/delete/correction and revoke behavior supported by the official API.

No standing auto-post setting may bypass per-post review in Phase 620F. LinkedIn DMs, connection
requests, recruiter scraping, contact enrichment, browser/session automation, cookie use, and
unapproved Talent/Sales/Marketing APIs remain out of scope.

### Phase 620F exit gate

- public/manual draft flow passes provenance, privacy, unsupported-claim, export, and accessibility
  tests without LinkedIn OAuth;
- any later posting phase has explicit LinkedIn product approval, exact scopes, redirect URIs,
  provider terms, rate limits, content/abuse review, token security, revoke/delete behavior, and
  authorized sandbox/canary evidence;
- per-post approval and provider receipt tests pass; and
- LinkedIn OAuth and posting flags remain `0` until a separate protected launch.

## Conceptual Data Model

All names below are **architecture inferences**. This plan creates no table, migration, or type.

### Private lookup hashing

Content and authority integrity hashes remain canonical SHA-256 where current Bluey contracts
require them. Searchable indexes over low-entropy or guessable values are different: they must use
domain-separated keyed HMAC consistent with the current `private_lookup_hash` boundary.

Required lookup domains include at least:

- `jobs/omnichannel/provider-subject/v1`;
- `jobs/omnichannel/recipient/v1`;
- `jobs/omnichannel/email/v1`;
- `jobs/omnichannel/phone/v1`;
- `jobs/omnichannel/domain/v1`;
- `jobs/omnichannel/thread-participant/v1`; and
- `jobs/agent/invocation-output/v1`.

The stored lookup value binds tenant/account scope where required, canonical input bytes,
lookup-domain/version, and active key revision. Plain domains, emails, phone numbers, provider
subjects, recipients, or small agent outputs never become unique/index columns or unsalted hashes.
Key rotation uses an explicit bounded dual-read/single-write generation, collision-safe uniqueness,
audit, and completion proof; it does not silently rewrite immutable content/authority receipts.
Logs, exports, URLs, and client payloads do not expose lookup HMACs or key revisions that would
create a correlation handle.

### Domain ownership

| Domain | Current/reused authority | New conceptual authority | Must not be overloaded |
| --- | --- | --- | --- |
| Gmail/Outlook read | `MailboxConnection`, provider credential/sync/message | consent/purpose revision and data-use projection | Business-messaging connection |
| Gmail/Outlook draft/send | `JobsCommunicationAction` and exact attempt evidence | method-policy release and draft-specific approval | Read-only mailbox grant |
| WhatsApp/Apple business | Phase 620A contracts | Phase 620E exact provider adapter release | `MailboxConnection` |
| MCP/agent | Jobs APIs and capability readers | client grant, tool registry, invocation receipt | Provider credential or browser cookie |
| C2C source | Discovery/source authority precedents | source-rights revision, requirement evidence, contact-purpose authority | Employer ATS job identity |
| C2C outreach | Communication hashing/lease principles | C2C plan, reservation, cooldown, suppression projection | Application submission receipt |
| LinkedIn | Contact URL/provenance only | draft/publication plan and official provider grant | Generic integration tile or browser session |

### `OmnichannelConsentRevisionV1`

Required fields:

- account, connection/agent client, provider/channel, provider subject hash, and business endpoint
  where applicable;
- exact purposes, action classes, data classes, recipient classes, and allowed source classes;
- requested scopes, returned scopes, derived Bluey capabilities, and method-policy release hash;
- capture surface, disclosure/privacy/terms revisions, locale, evidence hash, and actor/MFA revision;
- effective, expiry, revoke, and delete database times;
- predecessor/successor revision and reason; and
- state `pending`, `active`, `reduced`, `revoked`, `expired`, `terms_changed`, or
  `review_required`.

It cannot contain `all_messages`, `all_actions`, `all_recipients`, or a generic `connected=true`
authority. A capability absent from the closed allowlist is denied.

### `ProviderMethodPolicyV1`

Required fields:

- provider and adapter release;
- OAuth scope set and returned-grant projection;
- exact allowed HTTP method, canonical provider host, path template, request schema, and response
  evidence parser;
- denied method/path set required by the product boundary;
- allowed effect class, idempotency behavior, and ambiguous-effect rule;
- security/legal/provider review hashes and expiry; and
- signed activation/revocation authority.

This contract enforces draft-only behavior where provider scopes are broader than Bluey's product.

### `CommunicationThreadLinkV1`

Required fields:

- account, connection, provider, provider thread/message identity hashes;
- application, job, employer/vendor, contact, and Career Track revisions;
- correlation evidence features, score, runner-up separation, classifier version, and review state;
- subject/participant minimized projection;
- owner correction event rather than destructive rewrite; and
- state `matched`, `ambiguous`, `unmatched`, `corrected`, `revoked`, or `deleted`.

### `OmnichannelActionPlanV1`

Required fields:

- plan ID, monotonic revision, canonical plan hash, owner, and effect class;
- source ingress/thread/requirement/post and exact evidence revisions;
- provider connection, grant, consent, method policy, adapter release, and capability registry;
- recipient/subject/thread/audience, purpose, content, attachment, and data-class hashes; attachment
  references are empty unless an exact separately released transport authority permits them;
- Career Track, candidate fact, resume, application, job, source, and contact revisions;
- cap/reservation/cooldown/schedule/quiet-hour and suppression revisions;
- user-visible preview/diff/provenance hash;
- required step-up, earliest/expiry times, hold policy, and database decision time; and
- state `simulated`, `awaiting_review`, `approved`, `cancelled`, `expired`, or `superseded`.

Plan approval never creates a provider attempt by itself.

### `OmnichannelApprovalReceiptV1`

Required fields:

- exact plan ID/revision/hash and allowed effect class;
- account, authenticated session, MFA, actor, and approval-purpose hashes;
- connection, consent, grant, method-policy, adapter, source-rights, and capability revisions;
- exact recipient/audience/content/attachment hashes;
- database approval and expiry times; and
- immutable receipt hash.

An approval for `draft_create` cannot authorize `email_send`; an approval for `linkedin_post`
cannot authorize a comment or DM; an owner-channel reply cannot authorize recruiter outreach.

### `OmnichannelDispatchAttemptV1`

Required fields:

- plan/approval/action hashes, provider operation key, dispatch number, lease owner, token hash,
  monotonic fence, and exact worker/release identity;
- provider connection/grant/method-policy and current suppression/hold revisions;
- request method/host/path/body hashes with credentials excluded;
- durable `request_not_started_at_ms` or `request_started_at_ms` from database time;
- provider response/object/status evidence hashes;
- terminal/reconciliation state and database times; and
- no plaintext token, body, recipient, OTP, or sensitive attachment in log projections.

### `OmnichannelEffectReceiptV1`

Allowed assertions are provider/action-specific:

- `simulated_no_effect`;
- `cancelled_pre_effect`;
- `denied_pre_effect`;
- `draft_created` only after exact provider draft read-back;
- `provider_accepted`;
- `delivered` only from authenticated delivery evidence;
- `read` only from authenticated read evidence;
- `user_selected_send` for MessageUI without provider delivery proof;
- `failed_pre_effect`;
- `failed_confirmed_no_effect`;
- `side_effect_unknown`;
- `bounced` or `complained` only from authenticated provider evidence;
- `stopped`, `provider_closed`, `revoked`, or `deleted`; and
- `published` only from exact official LinkedIn object evidence.

These are not a single linear enum for every provider. The canonical evidence vocabulary maps each
provider-specific state to only the strongest supported assertion.

### `OmnichannelSuppressionV1`

Required fields:

- provider/channel, business endpoint or mailbox connection, subject/recipient/domain/source/account
  hashes, and optional application/contact scope;
- source `stop`, `provider_close`, `off_channel_opt_out`, `unsubscribe`, `bounce`, `complaint`,
  `disconnect`, `account_delete`, `source_rights_revoke`, `abuse_hold`, or `operator_kill`;
- evidence hash, database time, scope, reason, and state;
- predecessor/successor and exact authenticated re-consent authority where reactivation is legal;
  and
- no automatic expiry for STOP, complaint, provider close, or account deletion.

### Agent and C2C companion contracts

- `AgentClientGrantV1` and `AgentInvocationReceiptV1` use the Phase 620C boundaries above.
- `C2CSourceRightsRevisionV1` and normalized requirement evidence use Phase 620D boundaries.
- `ContactPurposeAuthorityV1` binds source, permissible purpose, freshness, correction,
  suppression, expiry, and data categories for one contact.
- `C2CDocumentTransportAuthorityV1` is the only contract that may make C2C `attachment_refs`
  nonempty; a preparation or outreach approval cannot mint it.
- `DailyEffectReservationV1` atomically reserves a named unit by account/provider/purpose/date and
  cannot be shared between resume preparations, applications, emails, messages, or posts.
- `DataLineageRestrictionV1` prevents mailbox, WhatsApp, contact, source, and candidate data from
  entering prohibited analytics, training, advertising, or cross-account paths.

## State Machines

### Provider connection

```text
unconfigured -> oauth_pending -> read_only
                         \-> reauthorization_required
read_only -> draft_review_required -> draft_capable
draft_capable -> send_review_required -> send_capable   (future phase only)
any active -> draining -> revoked -> deleted
any active -> terms_changed / grant_reduced / held
```

Returned grant reduction immediately removes derived capabilities. It cannot preserve an older
write capability while showing a degraded connection.

### Mail ingress and correlation

```text
received -> authenticated -> stored -> classified
                         \-> quarantined
classified -> matched | ambiguous | unmatched | unsupported
matched -> review_required -> owner_confirmed/corrected
```

Authentication and immutable storage precede model classification. Ambiguous correlation never
silently attaches to the highest score.

### Mail subscription and cursor health

```text
unconfigured -> initial_sync -> current
current -> notification_delayed -> current
current -> cursor_gap/full_resync_required -> reconciling -> current
any active -> held / reauthorization_required / revoked / expired
```

Only a contiguous provider cursor plus completed reconciliation reaches `current`. Notification
receipt alone cannot advance health or an application timeline.

### Draft effect

```text
planned -> awaiting_review -> approved -> leased
leased -> request_started -> draft_created
                         \-> failed_pre_effect
                         \-> side_effect_unknown
draft_created -> owner_edited/provider_changed -> superseded observation
```

A draft receipt never advances to `sent` without a separate send plan, approval, attempt, and
provider evidence.

### External send or publication

```text
planned -> awaiting_step_up -> approved -> leased -> request_started
request_started -> provider_accepted -> delivered/read      (only where evidenced)
                -> published                                (exact social object evidence)
                -> bounced/complained/provider_closed
                -> side_effect_unknown
```

No automatic retry follows `request_started` unless exact provider reconciliation first proves no
effect and creates fresh authority.

### Agent grant

```text
registered -> authorization_pending -> active_read_only
active_read_only -> scope_reduced | expired | revoked | deleted
active_read_only -> write_review_required                  (future only)
write_review_required -> active_scoped_write               (separate release only)
```

An invocation begun under a revoked generation cannot create a new plan or cross request start.

### C2C requirement

```text
observed -> rights_verified -> parsed -> canonicalized -> eligible_for_review
        \-> rights_denied/expired
parsed -> ambiguous/quarantined
eligible_for_review -> expired/superseded/contact_suppressed
```

Rights revocation blocks new use and triggers the approved deletion/retention policy without
rewriting historical receipts.

## Consent And Permission Model

Every permission decision is the intersection of:

```text
account entitlement
AND provider/product eligibility
AND source rights where applicable
AND provider-returned grant
AND Bluey purpose consent
AND channel/recipient/action/data-class capability
AND signed adapter/capability release
AND tenant/account/application/thread holds clear
AND suppression clear
AND current immutable plan and approval
AND release flag and cohort
```

If any term is absent, stale, ambiguous, expired, or conflicting, the result is denial.

### Closed capability vocabulary

| Capability | Data/effect | May imply |
| --- | --- | --- |
| `mail.read_job_metadata` | Bounded headers/metadata | Nothing else |
| `mail.read_job_content` | Bounded matched content | Classification only for consented purpose |
| `mail.classify_verification_intervention` | Classify and notify the owner | No OTP extraction, display, autofill, or consume |
| `runner.consume_application_otp` | Later, separately flagged exact runner intervention | Fresh owner action and exact lease/fence/challenge authority; no code output |
| `mail.create_draft` | One exact reviewed draft | No send |
| `mail.send_reviewed` | One exact reviewed message | No auto-reply, submit, or calendar write |
| `agent.read_jobs` | Bounded job projections | No preparation/write |
| `agent.preview_preparation_requirements` | Pure readiness/authority preview | No plan, reservation, generation, or write |
| `c2c.ingest_source` | One rights-approved source | No contact/send |
| `c2c.plan_outreach` | Simulated exact plan | No provider write |
| `business.owner_commands` | Closed owner command grammar | No recruiter/employer action |
| `linkedin.prepare_draft` | Bluey-local content draft | No OAuth/post |
| `linkedin.publish_reviewed` | One exact reviewed official post | No DM/comment/connection request |

Capabilities are versioned and provider-specific. Unknown strings are denied and never passed
through as provider scopes.

## Channel-Specific Controls

### Gmail

- Request the narrowest current official scopes and disclose their actual breadth.
- Keep refresh tokens server-side, encrypted, rotation-CAS protected, and unavailable to models.
- Use exact Gmail message/thread IDs only inside encrypted authority; return minimized projections.
- Separate mailbox query/filter policy from OAuth authority; a query is not a security boundary.
- Use a signed provider-method registry so draft-only cannot invoke send methods.
- Reconcile provider draft/message identity before claiming success.
- Disconnect/revoke creates a write fence, stops new sync/plan/claim, and drains truthful unknowns.

### Outlook / Microsoft Graph

- Keep Google and Microsoft connections, scopes, cursors, adapters, and receipts separate.
- Use delta/read behavior only under exact current provider contracts.
- Do not request `Mail.Send` for read or draft-only phases.
- Treat provider-returned grant reduction, tenant-policy denial, and invalid delta state as explicit
  review/reauthorization conditions, not silent fallback.

### MCP / Agents

- Use OAuth/DCR or an independently reviewed equivalent; no long-lived bearer connector URL.
- Bind token audience, client, account, resource, scopes, registry revision, and expiry.
- Generate docs from the deployed capability registry and reject undocumented tools/fields.
- Require server-side pagination, caps, rate limits, content bounds, and tenant checks.
- Never put provider credentials, browser sessions, raw mailbox bodies, passwords, or OTPs in tool
  output or tool arguments.
- Treat agent output as untrusted proposal content and run the same validation as portal input.

### C2C

- Keep employer postings, staffing requirements, email blasts, and private groups as distinct source
  classes.
- Require current source rights before ingestion and current contact-purpose authority before
  planning.
- Show vendor chain, end-client uncertainty, engagement type, rate unit, work authorization, and
  source freshness.
- Reserve daily units atomically and separately from ATS applications.
- Enforce per-recipient/domain/source cooldown, duplicate, unsubscribe, bounce, complaint, and STOP
  suppressions.
- Require exact resume/document diff and prohibit unsupported skill/experience injection.
- No invisible open tracking in Phases 620B-F.

### WhatsApp Business Platform

- Phase 620A is authoritative.
- Use Bluey's business identity only and official APIs only.
- Exact AI-provider eligibility precedes every live boundary.
- Explicit opt-in, template/window, STOP, escalation, webhook authentication, data-use restriction,
  and provider-specific receipts are mandatory.
- C2C recruiter outreach is not an allowed Phase 620E purpose.

### Apple Messages for Business / iMessage / SMS

- Phase 620A is authoritative for official business messaging.
- Approved MSP/business authority is required before live Apple work.
- Personal iMessage and SMS remain user-presented only through official MessageUI if a later native
  phase is approved.
- User interaction is not provider delivery evidence.

### LinkedIn

- Use official OAuth and approved LinkedIn products only.
- Draft/manual export needs no LinkedIn account.
- A future post uses exact per-post approval, content/audience hash, short expiry, and official
  provider receipt.
- No scraping, cookie/session reuse, browser automation, DMs, connection requests, or unapproved
  Talent/Sales/Marketing capabilities.

## User Controls And Product Grammar

Bluey should expose a single understandable control center without merging authority:

- global mode: `off`, `review_only`, or `limited_effects`;
- per-channel connection and health;
- per-purpose consent with returned provider scopes and plain-language data use;
- per-action controls for read, draft, send, submit, and post;
- recipient classes and allow/deny lists;
- daily caps by named unit, quiet hours, schedules, and expiry;
- source/contact provenance and reason for recommendation;
- exact preview, diff, attachments, facts, and data sent to provider;
- `why this action`, authority expiry, and required step-up;
- pause, STOP, disconnect, revoke, delete, correction, and export;
- immutable timeline with provider-specific status and evidence strength; and
- explicit `unknown` states with support/reconciliation path.

Owner notifications additionally require a saved intent category, primary route, fallback route,
fallback delay, quiet-hours/emergency policy, expiry, and cross-channel deduplication key. One event
cannot fan out repeatedly merely because web, email, WhatsApp Business, and Apple business channels
are connected. The control explains whether STOP suppresses one channel, one notification category,
or all optional notifications; safety/legal receipts may remain available only inside the
authenticated product where required.

The agent-control view lists every active MCP client with client identity/trust class, exact redirect
URI set, tool scopes, Career Track/application bounds, data-class/PII projection ceiling, grant
creation and last-use times, refresh-family identity/rotation generation, token expiry, active
session identities/count/expiry, registry revision, and connection health. The owner can revoke one
client, one refresh family/session, or all agent access, with immediate generation fencing and an
immutable receipt.

Controls may share a UI shell, but each operation links to its own immutable receipt. “Autopilot” is
not a permission value.

## Privacy, Retention, Deletion, And Export

### Data minimization

- Raw provider bodies are authenticated, encrypted, bounded, and retained only for an approved
  short evidence window.
- Normalized projections contain only fields needed for the exact job-search purpose.
- Tokens, authorization headers, connector secrets, raw phone numbers, provider IDs, OTP values,
  message bodies, resumes, and sensitive candidate facts do not enter normal logs or traces.
- Models receive minimized inputs only after authority and data-use checks. Provider data prohibited
  from training/evaluation remains structurally excluded.
- Analytics uses an approved event dictionary and pseudonymous identifiers; no mailbox body,
  candidate fact, contact detail, or WhatsApp Business Solution Data enters ad-tech.

### Retention authority

Before implementation, privacy/legal must approve exact maximum periods for:

- OAuth state and failed link state;
- encrypted raw mailbox/business-message/C2C source objects;
- normalized matched and unmatched messages;
- OTP material and hash-only consumption evidence;
- drafts, sent messages, provider receipts, and unknown-effect evidence;
- agent grants and invocation receipts;
- source-rights, contact provenance, and suppressions;
- LinkedIn drafts and publication evidence; and
- backups, tombstones, security evidence, and billing/legal records.

The plan intentionally does not copy competitor retention numbers into Bluey's policy. Until exact
periods are approved, live ingestion is denied.

### Disconnect and deletion

- Disconnect fences new sync, planning, approval, claim, request creation, and request start.
- An already-started unknown effect remains available only for bounded truthful reconciliation.
- Account deletion drains unresolved irreversible attempts, revokes provider grants, cancels
  unstarted plans, deletes provider connections and content under approved policy, and verifies
  object/database deletion.
- Source-rights revocation and provider erasure requests propagate through derived projections.
- A hash-only tombstone may remain only when approved and must not contain recoverable content.
- Suppression needed to prevent future unwanted contact is retained only under an approved legal
  basis and cannot be repurposed.

### Export

Export includes user-visible connections, scopes/capabilities, consent history, source/contact
provenance, plans, approvals, attempts, effects, suppressions, agent grants/invocations, and
corrections. It excludes tokens, secret headers, secure URLs, internal credentials, raw key
material, and cross-account identifiers. Facts, vendor claims, inferences, and unknowns remain
clearly labeled.

## Threat Model

| Threat | Required control |
| --- | --- |
| Broad OAuth scope used for an unapproved method | Signed provider-method allowlist, separate capability, exact adapter/egress route, denial tests |
| Connector URL leaked through logs/history/referrer | No bearer URL; header-only short-lived audience-bound token; redaction and rotation |
| Agent prompt injection requests secrets or submit | Tool-level scopes, typed inputs, server authority, no secret tools, web step-up, output minimization |
| OAuth callback replay or cross-account state | One-time PKCE state bound to account/provider/purpose/redirect/grant revision and database expiry |
| Read-OAuth disabled during start/callback/refresh | Recheck flag, signed release, cohort, and activation generation at every boundary; fence in-flight work and quarantine only minimized evidence |
| Refresh race resurrects old grant | Monotonic grant/revocation generation, transaction CAS, post-lock capability derivation |
| Discovery or dynamic registration reaches attacker-controlled/internal host | Signed HTTPS metadata/registration allowlist, canonical host and redirect validation, DNS-rebinding/SSRF denial, bounded documents, no redirect following |
| MCP or provider token is passed through to another resource | Exact RFC 8707 audience validation and mechanical token-passthrough denial |
| Mail classifier attaches wrong application | Minimum score and runner-up separation, ambiguous review, immutable correction event |
| OTP leaks to model/agent/log | Attempt-bound server consumer, structural redaction, one-time use, TTL, privacy scans |
| Draft-only path sends email | Provider-method registry omits send, egress deny, broad-scope adversarial test |
| Duplicate C2C contact | Atomic recipient/domain/source reservation, cooldown, idempotency key, suppression recheck |
| Low-entropy recipient, subject, domain, participant, or invocation-output index is enumerable | Domain-separated, versioned keyed HMAC under `private_lookup_hash`; never plaintext, unsalted, or raw integrity hashes |
| C2C preview silently transports a resume | `attachment_refs=[]` before provider I/O; separate document-transport flag, authority, validation, approval, and receipt |
| Private hotlist ingested without rights | Source-rights authority required before fetch/store/parse; denied/expired sources cannot claim |
| Contact data used for another purpose | Contact-purpose authority and data-class check on every plan/claim/request start |
| STOP/disconnect races dispatch | Serialize suppression/write fence; recheck immediately before durable request start |
| Timeout after provider request start | `side_effect_unknown`; no blind retry; provider-specific reconciliation only |
| Pixel load mislabeled as human read | Open tracking excluded; type rejects `read` without authenticated provider evidence |
| LinkedIn browser session automated | No browser/session adapter; official OAuth/provider-method policy only |
| WhatsApp personal session emulated | Phase 620A personal-channel rejection and provider-mode type boundary |
| Provider terms/scopes change | Versioned terms/method/eligibility authority with expiry; routes and claims fail closed |
| Tenant leak through IDs or agent pagination | Account-bound resources, opaque IDs, generic denials, signed cursors, cross-tenant tests |
| Deletion races unknown effect | Durable drain fence; bounded reconciliation; verified DB/object purge |

## Release Flags — Every Value Is `0`

Existing flags remain unchanged:

| Existing flag | Required value |
| --- | --- |
| `BLUEY_JOBS_MAILBOX_SYNC_ENABLED` | `0` |
| `BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED` | `0` |
| `BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED` | `0` |
| `BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED` | `0` |

All Phase 620A proposed flags remain `0`:

| Phase 620A proposed flag | Required value |
| --- | --- |
| `BLUEY_JOBS_BUSINESS_MESSAGING_SIMULATOR_ENABLED` | `0` |
| `BLUEY_JOBS_BUSINESS_MESSAGING_INGRESS_ENABLED` | `0` |
| `BLUEY_JOBS_BUSINESS_MESSAGING_PLANNING_ENABLED` | `0` |
| `BLUEY_JOBS_WHATSAPP_BUSINESS_PLATFORM_ENABLED` | `0` |
| `BLUEY_JOBS_APPLE_MESSAGES_FOR_BUSINESS_ENABLED` | `0` |
| `BLUEY_JOBS_BUSINESS_MESSAGING_DISPATCH_ENABLED` | `0` |
| `BLUEY_JOBS_BUSINESS_MESSAGING_RECONCILIATION_ENABLED` | `0` |
| `BLUEY_JOBS_BUSINESS_MESSAGING_PROACTIVE_ENABLED` | `0` |
| `BLUEY_JOBS_PERSONAL_MESSAGE_COMPOSER_ENABLED` | `0` |

The following names are **proposed design placeholders**, not implemented environment variables:

| Proposed flag | Required value | Boundary |
| --- | --- | --- |
| `BLUEY_JOBS_MAILBOX_READ_OAUTH_ENABLED` | `0` | No read-OAuth start, callback exchange/commit, or refresh |
| `BLUEY_JOBS_MAILBOX_DRAFT_SIMULATOR_ENABLED` | `0` | No simulator in a production process |
| `BLUEY_JOBS_MAILBOX_DRAFT_WRITE_ENABLED` | `0` | No provider draft creation |
| `BLUEY_JOBS_RUNNER_OTP_INTERVENTION_CONSUME_ENABLED` | `0` | No extraction or runner consumption; later subphase only |
| `BLUEY_JOBS_AGENT_GATEWAY_ENABLED` | `0` | No MCP/agent route |
| `BLUEY_JOBS_AGENT_GATEWAY_WRITE_ENABLED` | `0` | No agent-requested write plan/effect |
| `BLUEY_JOBS_C2C_SOURCE_INGRESS_ENABLED` | `0` | No external C2C source ingestion |
| `BLUEY_JOBS_C2C_PLANNING_ENABLED` | `0` | No live C2C plan creation |
| `BLUEY_JOBS_C2C_OUTREACH_ENABLED` | `0` | No recruiter/vendor send |
| `BLUEY_JOBS_C2C_DOCUMENT_TRANSPORT_ENABLED` | `0` | No resume/document attachment or upload |
| `BLUEY_JOBS_C2C_AUTO_REPLY_ENABLED` | `0` | No autonomous reply |
| `BLUEY_JOBS_LINKEDIN_OAUTH_ENABLED` | `0` | No LinkedIn provider grant |
| `BLUEY_JOBS_LINKEDIN_PUBLISH_ENABLED` | `0` | No LinkedIn post |

Even a future `1` is insufficient without the exact signed release, provider/source authority,
connection, grant, consent, plan, approval, cohort, cap, clear holds, and provider-specific gate.

## Test Plan

All tests below are **specified, not run** by this design phase.

### Cross-cutting simulator and authority tests

| ID | Scenario | Required result |
| --- | --- | --- |
| 620BF-GATE-001 | Production-default configuration | Every existing/proposed effect flag resolves to `0`; no route/worker starts |
| 620BF-GATE-002 | Simulator transport | Zero DNS/socket/HTTP/browser/device/provider attempts |
| 620BF-GATE-003 | Real-looking endpoint, token, phone, email, provider ID, or credential in fixture | Rejected before plan creation |
| 620BF-GATE-004 | Flag enabled without signed release/connection/grant/consent/cohort | Denied at route, plan, claim, and request start |
| 620BF-GATE-005 | Adapter/method registry version changes after approval | Approval invalidated; no effect |
| 620BF-GATE-006 | Concurrent STOP/revoke/hold and claim | Denial wins before request start |
| 620BF-GATE-007 | Timeout after durable request start | `side_effect_unknown`; zero automatic retry |
| 620BF-GATE-008 | Provider success without exact evidence | At most provider-supported weaker state; no fabricated delivery/read/published |
| 620BF-GATE-009 | Account deletion races pending/started actions | Pending cancelled; started drained/reconciled; verified purge |
| 620BF-GATE-010 | Logs/traces/export scan | No tokens, credentials, OTPs, raw bodies, resumes, contact values, or secure URLs |
| 620BF-LOOKUP-001 | Recipient/subject/email/phone/domain or small invocation output is indexed | Domain-separated keyed HMAC via `private_lookup_hash`; no plaintext/unsalted/SHA lookup index |
| 620BF-LOOKUP-002 | Lookup-key rotation and concurrent duplicate insert | Bounded dual-read/single-write generation, collision-safe uniqueness, no receipt rewrite or cross-tenant correlation |
| 620BF-ROUTE-001 | Same owner event reaches primary and fallback route concurrently | One category-scoped dedupe winner; no repeated notification; truthful per-channel receipt |
| 620BF-ROUTE-002 | STOP arrives on one channel | Exact configured channel/category/global suppression applies immediately and is visible; no ambiguous fan-out |

### Phase 620B mailbox tests

| ID | Scenario | Required result |
| --- | --- | --- |
| 620B-OAUTH-001 | PKCE success with exact returned read scopes | One read-only grant revision |
| 620B-OAUTH-002 | Replay, wrong provider/account/purpose/redirect, expired state | Fail closed; no credential overwrite |
| 620B-OAUTH-003 | Refresh rotation races disconnect or grant reduction | Monotonic winner; old capability cannot resurrect |
| 620B-OAUTH-004 | Read-OAuth flag/release/cohort absent at start or callback | No state, code exchange, credential commit, connection activation, or existence oracle |
| 620B-OAUTH-005 | Flag/release disabled after state mint but before callback/token request | Durable fence; no exchange or activation |
| 620B-OAUTH-006 | Disable wins after token/refresh request start | Result recorded as quarantined/draining with zero capability; bounded revoke/cleanup; no sync activation |
| 620B-OAUTH-007 | Refresh claim/request/CAS crosses release or cohort generation | Stale lease cannot call or commit; no old grant resurrection |
| 620B-OAUTH-008 | Client-credentials, application-role, service-account/domain-wide, tenant/admin-wide, or shared-mailbox grant reaches consumer flow | Rejected before credential commit; zero derived capability |
| 620B-GOOGLE-001 | Restricted Google scope lacks current Limited Use/policy/verification/security-assessment authority | OAuth start/callback/refresh and downstream routes remain disabled |
| 620B-MS-001 | `Mail.ReadWrite` grant attempts send/forward/reply-send/attachment/settings method | Signed method denylist rejects before provider I/O |
| 620B-MAIL-001 | Exact provider message replay | One immutable message result |
| 620B-MAIL-002 | Same provider identity with changed canonical content | Quarantine/conflict; no silent rewrite |
| 620B-MAIL-003 | Ambiguous application correlation | `ambiguous`; owner review; no application mutation |
| 620B-MAIL-004 | Unrelated mailbox message | Unsupported/unmatched minimized handling; no model or retention expansion |
| 620B-SYNC-001 | Gmail notification is delayed/dropped or watch renewal races revoke | Periodic reconciliation or revoke wins; health is not falsely `current` |
| 620B-SYNC-002 | Gmail history cursor is expired/returns `404` | `full_resync_required`; new epoch reconciles before `current`; no cursor skip |
| 620B-SYNC-003 | Graph subscription renewal/lifecycle notification races disconnect | Monotonic generation winner; no stale subscription or sync resurrection |
| 620B-SYNC-004 | Graph delta returns `410`/`syncStateNotFound` | Full-resync epoch and reconciliation; no silent missing interval |
| 620B-SYNC-005 | Old worker commits a cursor after a newer worker | Cursor CAS rejects rollback; immutable evidence retained |
| 620B-MAIL-005 | Oversized/deep/malformed MIME or unsupported charset | Quarantined before model/UI; bounded parser resource use |
| 620B-MAIL-006 | HTML contains script/style/form/SVG/pixel/remote resource or dangerous URL scheme | Sanitized text only; zero DOM insertion or remote fetch |
| 620B-MAIL-007 | Body instructs agent to reveal data, call a tool, follow a URL, or approve/send | Treated as untrusted data; no authority/tool/network effect |
| 620B-MAIL-008 | `From`/`Reply-To` mismatch or SPF/DKIM/DMARC/ARC warning | Evidence shown separately; recruiter identity remains unverified |
| 620B-DRAFT-001 | Gmail credential has `gmail.compose`; adapter attempts `messages.send` | Structural method-policy rejection before network |
| 620B-DRAFT-002 | Approved draft create with exact content | Provider draft read-back; `draft_created`, never `sent` |
| 620B-DRAFT-003 | Draft content/recipient/thread/fact changes after approval | New plan revision; prior approval invalid |
| 620B-DRAFT-004 | Timeout after draft-create request start | `side_effect_unknown`; lookup/reconciliation only, no duplicate draft |
| 620B-DRAFT-005 | Draft plan/provider projection contains an attachment or inline document | Closed rejection before approval/lease/provider I/O |
| 620B-OTP-001 | Verification-like mail arrives in Phase 620B | Classify/notify owner only; no code extraction, display, output, autofill, or consume |
| 620B-OTP-002 | Any Phase 620B route/tool requests an OTP value or consumption | Unsupported denial; no broad mailbox search or runner resume |
| 620B-OTP-LATER-001 | Later consume authority lacks fresh owner intervention, exact runner lease/fence, ATS/release/challenge/origin, or current application/evidence | Fail closed; zero code extraction/use |
| 620B-OTP-LATER-002 | Exact later authority races lease expiry, submit/request-start, evidence change, revoke, or replay | Earliest bound wins; no consumption; code never enters model/agent/portal/log/export |

### Phase 620C agent tests

| ID | Scenario | Required result |
| --- | --- | --- |
| 620C-AUTH-001 | OAuth/DCR client with exact audience and scopes | One active read-only grant |
| 620C-AUTH-002 | Bearer token in URL/query/fragment | Rejected; no connector created |
| 620C-AUTH-003 | Revoked/rotated grant races tool call | Old generation cannot read or plan |
| 620C-AUTH-004 | Protected-resource metadata/AS discovery has wrong origin, issuer, redirect, private IP, DNS rebinding, oversized body, or redirect chain | Bounded SSRF-safe rejection |
| 620C-AUTH-005 | Token lacks exact RFC 8707 resource/audience or is issued for another provider/resource | `401`; no token passthrough or upstream forwarding |
| 620C-AUTH-006 | CIMD/DCR metadata changes, self-grants scope, or introduces unapproved redirect/endpoint | Registration frozen/denied; no grant |
| 620C-AUTH-007 | Missing/invalid token versus valid token missing scope | Standards-safe `401` challenge versus `403 insufficient_scope`; no existence leak or refresh loop |
| 620C-SESSION-001 | Stolen session ID without a current exact bearer grant | Privacy-safe denial; no session/resource existence oracle |
| 620C-SESSION-002 | Session resume crosses account/client/grant/resource/registry or origin | Rejected; no cross-tenant event or result exposure |
| 620C-SESSION-003 | Revoke/rotate/hold/delete occurs during a stream | Generation fence closes stream; queued/resumed work cannot continue |
| 620C-SESSION-004 | Forged/rolled-back event cursor or cross-node queue injection | Sequence/binding/CAS denial; no replay or event substitution |
| 620C-SESSION-005 | Expired session reconnects with otherwise valid grant | New bounded session required; old event range unavailable |
| 620C-CLIENT-001 | Client/grant/session is created, refresh family rotates, expires, or owner targets revoke-one/revoke-all | Inventory exactly reflects data/PII ceiling, creation/use/expiry, refresh/session generations and active counts; exact fencing/receipt; no unrelated-client mutation |
| 620C-TOOL-001 | Unknown tool or extra/unbounded field | Schema denial |
| 620C-TOOL-002 | Read scope invokes preparation/write tool | Scope denial; no plan/effect |
| 620C-TOOL-003 | Prompt asks for token/password/cookie/OTP/raw body | No such resource/tool; privacy-safe denial |
| 620C-TOOL-004 | Cross-account/application/Track identifier | Generic denial without existence oracle |
| 620C-TOOL-005 | Agent tries to approve its own plan | `step_up_required`; no approval/effect |
| 620C-TOOL-006 | Read-only preparation preview is invoked | Pure bounded response; zero plan row, reservation, generation, or application mutation |
| 620C-DOC-001 | Registry and generated docs comparison | Exact tool/count/schema agreement |

### Phase 620D C2C tests

| ID | Scenario | Required result |
| --- | --- | --- |
| 620D-RIGHTS-001 | Source lacks/loses/changes rights authority | No fetch/store/parse/new use; hold and lifecycle action |
| 620D-RIGHTS-002 | Private WhatsApp/Slack/group URL without contract/consent | Rejected as unauthorized source |
| 620D-REQ-001 | Same requirement across vendor/email/public source | Preserve provenance and relationship; bounded dedupe without false merge |
| 620D-REQ-002 | Engagement/rate/work-auth fact unknown | Remains unknown; hard autonomous eligibility denied |
| 620D-PLAN-001 | Exact recipient contacted previously for same requirement | Cooldown/dedupe suppression; no plan/effect |
| 620D-PLAN-002 | Daily cap race across workers | One atomic named-unit reservation; cap never exceeded |
| 620D-PLAN-003 | Resume/message introduces unsupported skill/fact | Plan denied before approval |
| 620D-PLAN-004 | STOP/bounce/complaint arrives during lease | Suppression wins before request start; no future contact |
| 620D-DOC-001 | Simulated outreach plan contains provider `attachment_refs` | Closed rejection; local proposed document remains nontransportable |
| 620D-DOC-002 | Document upload attempted without separate flag/authority/provider release | Denied before read/upload; zero provider call |
| 620D-CHAT-001 | Email thread projected as C2C chat | Every item retains provider evidence; no invented transport/delivery state |
| 620D-E2E-001 | Full C2C fixture flow | Rights -> requirement -> match -> review -> simulated receipt; zero send |

### Phase 620E business messaging tests

Phase 620E inherits every Phase 620A test. Additional tests must prove the exact signed adapter
release, provider sandbox, business identity, eligibility, grants, templates/windows, webhook/JWT
authentication, status evidence, STOP/close, no-training lineage, support escalation, rate/spend
caps, canary, and rollback behavior without weakening Phase 620A.

### Phase 620F LinkedIn tests

| ID | Scenario | Required result |
| --- | --- | --- |
| 620F-DRAFT-001 | Local draft/manual export | No LinkedIn OAuth or provider call; provenance and unsupported-claim scan shown |
| 620F-DRAFT-002 | Draft uses mailbox/private source outside consent | Data-lineage denial |
| 620F-AUTH-001 | Returned scope lacks exact official post capability | No publication plan/claim |
| 620F-POST-001 | Content/audience/media changes after approval | New revision; prior approval invalid |
| 620F-POST-002 | Per-post step-up absent or expired | Denied; no provider call |
| 620F-POST-003 | Provider returns exact object evidence | `published` for that object only; no engagement claim |
| 620F-POST-004 | Browser/session/cookie/DM/connection-request adapter attempted | Closed unsupported rejection |

### Database and concurrency matrix

Every phase that adds persistence must prove on SQLite fixtures and PostgreSQL 17:

- paired schema/migration parity, constraints, foreign keys, indexes, and migration recovery;
- tenant/account/provider/source uniqueness and collision behavior;
- post-lock database-time expiry/window/cap decisions;
- lock-order matrix covering connection, grant, consent, source rights, application, plan,
  suppression, reservation, action, attempt, and deletion/drain rows;
- concurrent grant rotation/revoke, plan edit/approval, cap reservation, claim, request start,
  suppression, reconciliation, and deletion;
- immutable evidence and transition replay;
- exact account export and delete fan-out; and
- fail-closed rollback with zero partial external authority.

## Release Gates

Source-complete or simulator-green is not launch authority. Each live phase requires:

1. exact current provider/source terms and legal/privacy/data-rights approval;
2. approved provider applications, products, scopes, redirect URIs, endpoints, and credentials;
   read-OAuth/connect separately requires its exact signed release, cohort, start/callback/token
   exchange/credential-commit/refresh gates, and tested in-flight-disable fencing;
3. secret-manager custody, rotation, revoke, incident, operator-access, and audit evidence;
4. reviewed schema/migrations and live PostgreSQL 17 concurrency/recovery evidence;
5. provider sandbox matrices including outage, timeout, throttle, duplicate, revoke, delete, and
   ambiguous-effect reconciliation;
6. UI consent, scope disclosure, review, accessibility, correction, pause, STOP, disconnect,
   deletion, export, and truthful state review;
7. privacy scans, data lineage, retention jobs, deletion read-back, and subprocessor disclosure;
8. source/contact rights and abuse/compliance review where outreach is involved;
9. signed adapter/capability release, registry/read-back, SBOM, provenance, and rollback authority;
10. internal/employee canaries using controlled recipients and no uninvolved third party;
11. cohort, rate, spend, daily cap, complaint, bounce, support, and kill-switch thresholds;
12. monitoring and alerts for backlog, grant health, suppression, unknown effects, provider errors,
    abuse, deletion, and reconciliation;
13. kill-switch and rollback rehearsal, including an effect that becomes unknown during revoke;
14. exact-tip CI/privacy/security evidence and independent no-P0-P2 review; and
15. explicit protected production authorization to change one exact flag/cohort from `0`, if ever.

No phase may batch-enable all channels. Gmail read, Gmail draft, agent read, C2C ingress, C2C send,
business messaging, LinkedIn OAuth, and LinkedIn post are separate release decisions.

## Metrics And Truthful Product States

Before marketing, define:

| Metric | Required denominator/evidence |
| --- | --- |
| Mail correlation | Matched, ambiguous, unmatched, corrected, and false-link rate on reviewed labels |
| Draft quality | Fact-grounding, unsupported-claim, user edit, approval, abandonment, and draft-create evidence |
| Send outcome | Request-started, accepted, delivered where evidenced, bounced, complained, unknown, and reconciled |
| C2C source quality | Rights-approved sources, freshness, duplicates, sender/domain confidence, expired and quarantined records |
| C2C outreach | Reviewed plans, sent, bounced, opted out, complained, replied, meeting proposed, and exact caps |
| Agent reliability | Calls by client/tool/scope/version, denials, latency, rate limits, revokes, and privacy incidents |
| Business messaging | Linked owners, consent categories, STOP/close, accepted/delivered/read by exact provider evidence |
| LinkedIn | Drafts, edits, manual exports, approved posts, provider failures, revokes; never inferred impressions |
| Safety | Unsupported claims, duplicate contacts, suppression breaches, secret/PII leaks, unknown effects, deletion SLA |

Do not combine `prepared`, `drafted`, `sent`, `submitted`, `published`, `delivered`, `read`,
`replied`, `interviewed`, or `hired` into one automation counter.

## Public And Standards Sources

All competitor links are public first-party pages accessed 2026-08-30. Their claims may change and
must be re-read before implementation. No competitor source is provider or production evidence.

### Tsenta

- [Homepage](https://tsenta.com/)
- [Messaging](https://tsenta.com/messaging)
- [AI Disclosure](https://tsenta.com/ai-disclosure)
- [Privacy Policy](https://tsenta.com/privacy)
- [Terms of Service](https://tsenta.com/terms)
- [Changelog](https://tsenta.com/changelog)
- [MCP setup](https://tsenta.com/mcp)

### GiraffyReach

- [Homepage](https://www.giraffyreach.com/)
- [C2C Autopilot](https://www.giraffyreach.com/autopilot)
- [Agent Connect / MCP](https://www.giraffyreach.com/mcp)
- [Privacy Policy](https://www.giraffyreach.com/privacy-policy)
- [Terms of Service](https://www.giraffyreach.com/terms-of-service)
- [Security](https://www.giraffyreach.com/security)
- [Vendor-owned C2C comparison mentioning WhatsApp hotlists](https://blogs.giraffyreach.com/best-platforms-for-c2c-contract-jobs-in-2026-honest-comparison)

### Official providers

- [Google Gmail OAuth scopes](https://developers.google.com/workspace/gmail/api/auth/scopes)
- [Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy)
- [Google Workspace API User Data and Developer Policy](https://developers.google.com/workspace/workspace-api-user-data-developer-policy)
- [Gmail push notifications](https://developers.google.com/workspace/gmail/api/guides/push)
- [Gmail synchronization and history recovery](https://developers.google.com/workspace/gmail/api/guides/sync)
- [Google domain-wide delegation](https://developers.google.com/identity/protocols/oauth2/service-account)
- [Microsoft Graph permissions reference](https://learn.microsoft.com/en-us/graph/permissions-reference)
- [Microsoft delegated and application permission types](https://learn.microsoft.com/en-us/graph/permissions-overview)
- [Microsoft Graph change-notification webhooks](https://learn.microsoft.com/en-us/graph/change-notifications-delivery-webhooks)
- [Microsoft Graph message delta](https://learn.microsoft.com/en-us/graph/delta-query-messages)
- [Microsoft Graph delta recovery](https://learn.microsoft.com/en-us/graph/delta-query-overview)
- [Microsoft Graph lifecycle notifications](https://learn.microsoft.com/en-us/graph/change-notifications-lifecycle-events)
- [Microsoft Graph create message draft](https://learn.microsoft.com/en-us/graph/api/user-post-messages)
- [Microsoft Graph send mail](https://learn.microsoft.com/en-us/graph/api/user-sendmail)
- [Microsoft Graph send message](https://learn.microsoft.com/en-us/graph/api/message-send)
- [LinkedIn API access and permissions](https://learn.microsoft.com/en-us/linkedin/shared/authentication/getting-access)
- [LinkedIn Consumer Solutions](https://learn.microsoft.com/en-us/linkedin/consumer/)
- [WhatsApp Business Messaging Policy](https://business.whatsapp.com/policy)
- [WhatsApp Cloud API overview](https://developers.facebook.com/docs/whatsapp/cloud-api/overview)
- [Apple Messages for Business REST API](https://register.apple.com/resources/messages/msp-rest-api/)
- [Apple Messages for Business MSP onboarding](https://register.apple.com/resources/messages/msp-onboarding/)
- [Apple Messages framework](https://developer.apple.com/documentation/messages/)
- [Apple MessageUI](https://developer.apple.com/documentation/messageui)

Phase 620A's pinned provider/legal source list remains authoritative for its exact WhatsApp and
Apple claims.

### MCP and OAuth standards

- [MCP 2025-06-18 Authorization](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)
- [MCP 2025-06-18 Security Best Practices](https://modelcontextprotocol.io/specification/2025-06-18/basic/security_best_practices)
- [Current MCP security guidance: session hijacking threat reference](https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices#session-hijacking)
- [RFC 9728: OAuth 2.0 Protected Resource Metadata](https://www.rfc-editor.org/rfc/rfc9728.html)
- [RFC 8414: OAuth 2.0 Authorization Server Metadata](https://www.rfc-editor.org/rfc/rfc8414.html)
- [RFC 8707: Resource Indicators for OAuth 2.0](https://www.rfc-editor.org/rfc/rfc8707.html)
- [RFC 7591: OAuth 2.0 Dynamic Client Registration](https://www.rfc-editor.org/rfc/rfc7591.html)
- [OWASP cross-site scripting prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html)

Implementation must pin the exact MCP specification version and content hash in the signed release;
these links are design references, not permission to follow mutable discovery or registration data.

## Implementation Handoff

The implementation successor must:

1. keep Phase 620A unchanged and implement Phases 620B-F as separately reviewed batches;
2. start each phase with conceptual-contract review and deterministic no-egress fixtures;
3. preserve current mailbox/communication receipts and use explicit companion authority rather
   than reinterpreting serialized history;
4. make read, verification classification, later runner-bound OTP consumption, draft, send,
   submit, and post distinct in types, database constraints, UI, OAuth disclosures, flags, tests,
   and operations;
5. implement a signed provider-method firewall before requesting any broad provider scope used for
   a narrower Bluey capability;
6. keep all provider credentials server-side and every secret/OTP out of agent, portal, logs, and
   exports;
7. require current source/contact rights before any C2C ingestion or planning;
8. preserve exact plan, approval, lease, request-start, unknown-effect, suppression, and deletion
   invariants across every provider;
9. implement read-OAuth/connect as an independently flagged lifecycle, with exact signed release,
   cohort, start/callback/token/commit/refresh rechecks, and in-flight-disable fencing;
10. keep Phase 620C read-only preparation as a pure preview; persisted preparation plans remain a
    later separately flagged write capability;
11. keep Phase 620B attachment references empty, and require the separate C2C document-transport
    authority for any later upload;
12. use domain-separated, versioned keyed HMAC under `private_lookup_hash` for every low-entropy
    recipient/subject/domain/participant/invocation-output lookup index;
13. generate the agent/public capability documentation from the deployed registry;
14. leave invisible open tracking, recruiter WhatsApp, personal iMessage automation, LinkedIn
    browser automation, and agent-visible credentials unsupported;
15. keep every existing and proposed flag at `0`; and
16. stop for explicit authority before provider setup, OAuth, callback, source access, external
    draft/message/email/post/application, sandbox with real accounts, deploy, or production change.

## Final Architecture Verdict

Bluey can exceed the useful public competitor workflows without copying their implementation by
making authority and evidence visible: provider scopes versus Bluey capabilities, read versus
draft versus send, exact source/contact provenance, per-purpose consent, immutable content
approvals, provider-specific receipts, truthful unknowns, immediate suppression, and complete
deletion/export controls.

This plan authorizes design work only. It leaves Phase 620A intact, every effect disabled, and every
live provider/source/production decision unresolved pending its own reviewed phase.
