# Phase 620A Plan — Simulator-First Business Messaging Control Plane

> **Codex preflight:** `$bluey-ops` was loaded. The named Phase 614B worktree and current local
> handoff are authoritative. This is a design-only Phase 620A artifact. It changes no code,
> schema, provider account, credential, feature flag, release, or production state.

## Status

**Architecture verdict:** simulator-first only; no external effect is authorized.

**Independent re-review:** 🟢 design accepted on 2026-08-30 with no remaining P0-P2 finding after
the legal/eligibility, no-training, multi-item conflict, MFA linking, deterministic opt-out, flag,
and truthful provider-status corrections. This is not an implementation or launch verdict.

**Evidence date:** 2026-08-30 (America/New_York).

**Production posture:** every existing and proposed messaging write, ingress, dispatch,
reconciliation, and provider flag remains `0`. No WhatsApp Business Account (WABA), WhatsApp
business phone number, Meta app, Apple Messages for Business business, approved Messaging Service
Provider (MSP), Apple Business Register account, callback, template, token, or secret is established
by this phase. Bluey has no current evidence that its AI-first Jobs use case is eligible for live
WhatsApp Business Solution access, so the WhatsApp path is specifically ineligible and fail-closed
until the legal/provider gate below is satisfied.

This document uses the following evidence labels:

- **Provider fact** — stated by public official Meta/WhatsApp or Apple documentation linked below.
- **Current Bluey fact** — visible in the current local source tree.
- **Architecture inference** — an original Bluey design decision derived from provider facts and
  current Bluey invariants. It is not a claim about either provider's private implementation.
- **Future gate** — work that requires a separate implementation, security review, legal review,
  provider approval, verification, and explicit launch authority.

## Decision

Subject to separate, current provider-eligibility evidence, Bluey may eventually expose a
business-owned messaging control channel through:

1. the **WhatsApp Business Platform Cloud API**, using a Bluey business identity, WABA, business
   phone number, official API, and authenticated webhooks; and
2. **Apple Messages for Business**, using a registered Bluey business and an Apple-approved MSP,
   unless Bluey separately completes Apple's MSP qualification and approval process.

The WhatsApp Business Solution Terms marked “Last Modified: March 6, 2026” restrict AI Providers
from using the solution when AI is the primary rather than incidental functionality, subject to a
provider-controlled exception for WhatsApp users with European Economic Area or Brazil registered
phone numbers. Bluey must assume that its AI-first Jobs product is restricted. It cannot infer that
the exception applies from IP, locale, residence, or a self-declared region. No live WhatsApp work
may start until a separately reviewed, unexpired eligibility record proves the exact product use
case, account, endpoint, and provider-controlled recipient cohort are permitted under then-current
terms and any required Meta approval. Missing, ambiguous, expired, or conflicting evidence keeps
every WhatsApp flag `0`.

Those channels may communicate with the owning Bluey account only after exact linking and consent.
They must never be represented as the candidate's personal WhatsApp account or personal iMessage
identity. They do not authorize Bluey to contact a recruiter, employer, vendor, or other third party
on the candidate's behalf.

Phase 620A specifies only a deterministic, no-network simulator and its acceptance tests. It does
not send a message, subscribe a webhook, create a provider account, link a phone number, or accept
live provider data. No simulator code or test was implemented or run in this design phase.

## Scope

### This phase defines

- the provider and personal-account boundaries;
- a current, fail-closed provider-eligibility and legal-policy snapshot;
- a closed, deterministic owner-command grammar;
- account linking, authentication, consent, and revocation ceremonies;
- immutable ingress, intent, plan, approval, attempt, suppression, and receipt contracts;
- replay, idempotency, lease, fencing, and ambiguous-effect behavior;
- STOP, provider-close, operational-hold, and kill-switch precedence;
- provisional privacy, retention, deletion, and export boundaries;
- a threat model and security controls;
- all-zero release flags; and
- simulator acceptance tests that prove zero external effects.

### This phase does not

- use personal WhatsApp, WhatsApp Web, a QR-linked user session, browser automation, address-book
  scraping, unofficial libraries, or a user's personal WhatsApp credentials;
- read, automate, script, or scrape a personal Messages/iMessage database, UI, AppleScript surface,
  Accessibility surface, or device backup;
- send SMS, MMS, iMessage, WhatsApp, Apple Messages for Business, email, calendar, job application,
  recruiter outreach, C2C outreach, or social message;
- treat an iMessage extension as a background or server messaging API;
- create a WABA, Meta app, system user, business phone number, message template, Apple business,
  MSP relationship, invitation entitlement, or provider credential;
- ingest live webhooks, use a live callback, or make a provider API request;
- add code, migrations, secrets, infrastructure, UI, CHANGELOG entries, flags, jobs, workers, or
  production data;
- approve an application, submit an application, change a Career Track, or grant Auto-submit;
- reuse business-channel consent as consent for another channel, purpose, recipient, or action;
- infer a provider's private backend, delivery algorithm, or undocumented endpoint;
- claim launch readiness from simulator success; or
- treat the WhatsApp Third Party Agent Platform terms as Business Platform API access, onboarding,
  approval, or an eligibility exception for Bluey.

## Provider Boundary

| Surface | Officially supported shape | Bluey boundary |
| --- | --- | --- |
| WhatsApp Business Platform Cloud API | Business-to-user messaging through a WABA/business phone number, Graph API, access token, approved templates where required, and webhooks | Candidate communicates with **Bluey as a business**. It is not the candidate's personal WhatsApp identity. |
| WhatsApp Third Party Agent Platform | A separate WhatsApp-provided technical platform through which users may connect independent third-party agents under separate Agent Terms | No inferred API, onboarding path, approval, or Business Solution eligibility. Bluey has no authority to use it in Phase 620A. Its agent communications must not be represented as equivalent to end-to-end encrypted personal chats. |
| Personal WhatsApp / WhatsApp Web | Consumer/client product, outside the Cloud API business identity described above | No automation, session capture, QR linking, scraping, or unofficial API. Unsupported for Bluey autonomous effects. |
| Apple Messages for Business | Business-to-user channel routed through an approved MSP server-to-server REST integration | Candidate communicates with **Bluey as a registered business**. Bluey uses an approved MSP unless separately approved as an MSP. |
| Personal iMessage / Messages app | User-facing MessageUI composer and iMessage app-extension APIs with user-visible/user-interaction constraints | No server or unattended personal-iMessage channel. A future composer can be user-presented only; Phase 620A simulates it. |
| SMS/MMS composer | `MFMessageComposeViewController` presents Apple's composer and the person can edit, send, or cancel | It is not proof of delivery and not an autonomous effect API. Phase 620A does not present or send it. |

### WhatsApp Business Platform facts

The following are **provider facts**:

- The Meta terms distinguish the WhatsApp Business Solution and APIs from personal use. A WABA and
  linked Meta business/developer configuration are required for API access.
- A business may contact a person only after obtaining the person's phone number and opt-in
  permission for subsequent messages or calls. The business must honor block, discontinue, and
  opt-out requests, including requests received outside WhatsApp.
- Business-initiated conversations use approved message templates. Within 24 hours of the last user
  message, the business may reply without a template; outside that customer-service window it may
  send only approved templates.
- Automation within the 24-hour window must retain clear, direct escalation paths to a human or
  another support channel.
- WhatsApp Business Solution Data cannot be used to build or augment individual profiles, except
  for the content of message threads as allowed by the terms, and cannot be shared or repurposed
  outside the bounded service-provider relationship. Service providers require contractual and
  security safeguards.
- Official Cloud API documentation defines access tokens, permissions, Graph API message calls,
  templates, and webhooks. Bluey must use only those official surfaces.
- The current Business Solution Terms classify providers and developers of AI or machine-learning
  technology as AI Providers and restrict access where that technology is the primary functionality,
  with a provider-controlled exception for users whose registered phone number has an EEA or Brazil
  country code. Meta determines whether functionality is primary or incidental.
- When an AI Provider is retained as a Third Party Service Provider, Business Solution Data,
  including anonymous, aggregate, and derived forms, cannot be used to create, develop, train, or
  improve AI models, except for the terms' narrow exclusive-use fine-tuning provision.
- The Terms of Service for Use of Third Party Agents marked “Last Updated: August 25, 2026” govern a
  distinct user-to-agent platform. They do not document Business Solution API access or grant Bluey
  onboarding/eligibility. Those terms also state that communications with third-party agents are not
  end-to-end encrypted and that the Agent Provider receives, processes, and stores the messages in
  unencrypted form.

**Architecture inferences:**

- A WhatsApp link proves control of the channel endpoint at that time; it does not by itself prove
  the person is the authenticated Bluey account owner. Bluey therefore needs a separate, expiring,
  authenticated account-link ceremony.
- Bluey adopts a stricter rule than the narrow exclusive-use fine-tuning exception: no WhatsApp
  Business Solution Data—including raw, anonymized, aggregate, derived, label, embedding, metric,
  feedback, or evaluation data—may create, develop, train, fine-tune, evaluate, benchmark, or
  generally improve any model or model-serving policy. User consent cannot override this rule.
- The distinct Third Party Agent Platform stays out of scope until official technical access,
  onboarding, eligibility, privacy, security, and regional requirements are separately evidenced.

### Apple Messages for Business facts

The following are **provider facts**:

- Apple Messages for Business is a business channel routed between the business and the user through
  an MSP platform. The MSP implements the server-to-server REST API.
- The REST API is limited to approved MSPs. Apple requires an MSP to register through Apple Business
  Register, complete review/onboarding, implement required features, and demonstrate the integration.
- Incoming messages are delivered to the MSP's `/message` endpoint. The MSP validates destination
  business identity, Authorization, content encoding, and payload before routing.
- Both directions require a signed JWT in the `Authorization: Bearer ...` header. Apple's current
  MSP documentation specifies HS256, MSP audience/issuer claims, `iat`, token regeneration at least
  hourly, rejection of tokens older than one hour, protected secret storage, and bounded old/new-key
  overlap during rotation.
- Apple's gateway accepts outgoing MSP messages at
  `POST https://mspgw.push.apple.com/v1/message`. A `200` response is needed for ordered sequencing;
  it must not be promoted into stronger delivery evidence than Apple actually returns.
- Apple requires support for receiving user-close events. After a user closes a conversation, the
  MSP and business must block live and automated agents from further sends. An attempted send may
  return `410 Gone` and is not delivered.
- Invitation messages require feature access and Apple approval, use Apple-managed templates, permit
  only limited customization, and require explicit phone-number opt-in. Unsolicited invitations and
  marketing campaigns are prohibited. Opt-out must be honored.
- Standard conversations use an Apple opaque user identifier. A phone number is used for an invitation
  before the user accepts; acceptance creates the opaque identifier for subsequent conversation use.

**Architecture inference:** Bluey is not an approved MSP merely because the public REST documentation
is available. Until an approved relationship is evidenced, the Apple adapter can only be a simulator.

### Personal iMessage and MessageUI facts

The following are **provider facts**:

- Apple's Messages framework creates iMessage app extensions that run in the Messages experience.
  It is not a general server API for a user's personal iMessage account.
- An iMessage app can insert a message into the input field for the user to send. Direct extension
  send APIs exist, but Apple documents that an iMessage app can send only while visible and in
  response to recent user interaction.
- `MFMessageComposeViewController` presents the standard SMS/MMS composition UI. The person may edit,
  send, or cancel. The composer result does not guarantee delivery; the Messages app owns the send.
- App extensions and their containing apps remain subject to App Store review, privacy, disclosure,
  and anti-spam requirements.

**Architecture inference:** Bluey must not advertise autonomous personal iMessage sending. A later
native phase may propose a user-presented composer, but it must record `presented`, `cancelled`, or
`user_selected_send`; it cannot manufacture a `delivered` receipt.

## Product Use-Case Boundary

The only Phase 620A conceptual recipient is the authenticated owner of the Bluey Jobs account.

Allowed future purposes, each separately consented:

- notify the owner that matches, interventions, or review plans are waiting;
- let the owner inspect bounded Bluey state;
- let the owner request reversible internal planning such as save, pass, or prepare;
- direct the owner to Bluey's authenticated web review for any approval or sensitive action; and
- let the owner pause or stop the channel immediately.

Not authorized by this channel:

- representing the candidate to a recruiter or employer;
- sending recruiter, C2C, staffing, referral, networking, or employer outreach;
- answering an employer question;
- submitting or withdrawing a job application;
- sending a resume, identity document, work-authorization fact, compensation expectation, SSN,
  date of birth, or other sensitive candidate data;
- accepting terms, contracts, offers, interviews, or calendar invitations;
- changing Auto-submit or provider write authority;
- using the conversation for cross-account training, ad targeting, profiling, or lead enrichment; or
- using WhatsApp Business Solution Data for any model training, fine-tuning, evaluation, benchmark,
  embedding corpus, reward signal, generalized analytics improvement, or model-serving improvement,
  even with user consent and even if the data is anonymous, aggregate, or derived.

Consent to receive a Bluey notification is never consent to an employer-facing effect.

## Architectural Invariants

1. **Business identity only.** Every live provider message identifies Bluey as the business sender.
2. **No personal-account emulation.** Personal WhatsApp and personal iMessage sessions are never an
   adapter implementation strategy.
3. **Authenticate before parse.** Preserve bounded raw bytes, authenticate the provider envelope,
   then decompress and parse with strict limits.
4. **Deny before grant.** STOP, provider close, account deletion, operational hold, revoked consent,
   and kill switches can deny an action without otherwise authenticated command authority.
5. **Closed grammar.** Free text and model output cannot become a command or effect authority.
6. **Immutable authority.** An edit creates a new plan revision and invalidates prior approval.
7. **Exact recipient.** Account, provider, business endpoint, provider subject, consent revision,
   connection revision, and intended recipient are hashed into every plan.
8. **Exact provider policy.** WhatsApp window/template state and Apple conversation/invitation state
   are inputs to the plan, not worker guesses.
9. **Provider acceptance is not delivery.** Record only the status supported by authenticated
   provider evidence.
10. **Ambiguous effect is terminal for automatic retry.** A timeout after request bytes may have
    crossed the boundary becomes `side_effect_unknown`; it is never blindly resent.
11. **Secrets stay server-side.** Tokens, app secrets, MSP secrets, and webhook verification secrets
    never enter the desktop, portal bundle, logs, plans, exports, or receipts.
12. **WhatsApp data never trains or improves models.** WhatsApp Business Solution Data, including
    anonymized, aggregate, derived, label, embedding, metric, feedback, and evaluation forms, is
    excluded by type and policy from every training, fine-tuning, evaluation, benchmark, pooled
    analytics, and general model or serving-policy improvement path. Consent cannot override this
    prohibition. Other channel data remains owning-account operational data unless a separate
    channel-specific authority permits a narrower use.
13. **Eligibility precedes availability.** A current provider/legal eligibility record, exact
    region/cohort evidence, and any required approval are authority inputs. Missing or stale evidence
    denies ingress, planning, approval, claim, and request start even if a feature flag is enabled.
14. **Database time is authority.** Expiry, window, lease, approval, and retention decisions use
    post-lock database time, not caller or worker clocks.
15. **Flags do not grant authority.** An enabled binary flag alone can never create a provider,
    consent, connection, release, tenant, or action grant.

## Control-Plane Topology

The following is an **architecture inference**, not an implemented topology:

```text
Official provider / in-process simulator
        |
        v
Webhook edge: raw-byte limit -> provider authentication -> envelope + logical-item inbox
        |
        v
Denial-only locale opt-out parser -> pending subject proof + original-session MFA linker
        |
        v
Consent/suppression guard -> closed command parser
        |
        v
Jobs authority reader -> immutable plan builder -> authenticated Bluey review
        |
        v
Global/provider/tenant/account/conversation/action holds
        |
        v
Fenced outbox lease -> provider adapter OR no-egress simulator
        |
        v
Immutable attempt receipt -> status webhook reconciliation -> user-visible evidence
```

### Components

| Component | Responsibility | Prohibited behavior |
| --- | --- | --- |
| Webhook edge | Bound bytes; retain exact digest; validate signature/JWT and endpoint identity; persist before ACK | Parsing untrusted body first, logging secrets/body, synchronous model calls |
| Ingress inbox | Deduplicate raw envelopes and independently identity/version every logical message, status, close, or other item; preserve immutable auth and conflict evidence | Rewriting an event, treating a multi-item envelope as one command, or claiming a late conflict can undo an earlier effect |
| Opt-out parser | Apply the exact locale/version denial table before account linking, command parsing, or any model path | Fuzzy/LLM classification, granting authority, or allowing an expired parser snapshot to revive consent |
| Account linker | Bind provider subject to one account only after channel proof plus confirmation from the originating authenticated Bluey session and MFA | Treating a challenge as a bearer grant; linking from phone, display name, chat assertion, or forwarded token alone |
| Consent ledger | Version purpose/category/channel consent and revocation | Boolean `connected`, inferred consent, cross-channel reuse |
| Suppression ledger | Block provider-native close, STOP, off-channel opt-out, account deletion | Automatic expiry or chat-only reactivation |
| Command parser | Parse strict ASCII grammar to typed intent | LLM execution, shell/tool syntax, arbitrary URLs, free-form arguments |
| Plan builder | Read current Jobs authority; produce immutable preview and policy decision | Approving or dispatching its own output |
| Review authority | Step-up authenticated approval of exact plan hash | Approval via possession of chat alone for sensitive/external effects |
| Outbox/lease service | Serialize dispatch with token, fence, operation key, and pre-effect recheck | Retry after ambiguous send, provider choice outside plan |
| Provider adapter | Map one exact plan version to documented provider request | Undocumented endpoints, personal accounts, payload field expansion |
| Reconciler | Apply authenticated provider status evidence monotonically | Converting absence to delivery; undoing STOP or submitted evidence |
| Retention worker | Execute bounded deletion and retain hash-only tombstone where required | Deleting authority before proving object/database purge |

### Relation to current Bluey code

The current source already has **current Bluey facts** that are useful precedents:

- canonical communication payload and authority hashes;
- immutable approval and grant revisions;
- leased dispatch and reconciliation attempts with fences;
- `side_effect_unknown`-style conservative behavior;
- communication write fences and operational holds; and
- the current `BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED`,
  `BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED`, and
  `BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED` release flags, all disabled unless explicitly
  enabled.

The existing communication provider grammar currently covers Gmail, Outlook email, Google Calendar,
and Outlook Calendar only. Phase 620A does **not** widen those enums or overload
`MailboxConnection`. A successor should first implement separate business-messaging contracts and
reuse the proven hashing/approval/lease principles. Unification requires a reviewed migration and
must not reinterpret existing receipts.

## Conceptual Immutable Contracts

All names in this section are **architecture inferences**. No table or type is created by this plan.

### `WhatsAppProviderEligibilityV1`

Required before any future live WhatsApp ingress, planning, approval, claim, or request start:

- exact Business Solution Terms, Business Terms, Messaging Policy, and documentation snapshot IDs;
- legal-review decision, reviewer identity, evidence hash, decision time, and mandatory expiry;
- product classification and evidence for whether AI is primary or incidental, without allowing
  Bluey to self-approve an ambiguous classification;
- exact WABA, business endpoint, provider grant, adapter release, and product-purpose hashes;
- provider-controlled eligible recipient cohort, including registered-number country-code evidence
  where a then-current regional exception is relied upon;
- any required written Meta/provider approval or confirmation and its immutable evidence hash;
- prohibited data-use projection, including the absolute Bluey no-training/no-improvement rule; and
- state: `eligible`, `denied`, `expired`, `terms_changed`, or `review_required`.

Only exact `eligible` authority may be consumed. IP geolocation, UI locale, residence, billing
country, a self-declaration, user consent, a WABA, an approved template, or a working access token is
not eligibility evidence. A terms or product-purpose change expires the record immediately. Phase
620A has no such record, so the current state is `review_required` and all live WhatsApp work is
denied.

### `BusinessMessagingLinkChallengeV1`

Required fields:

- challenge ID and a hash of the random secret, never the plaintext secret after presentation;
- originating account, authenticated Bluey session, MFA authority, provider, business endpoint,
  purpose set, return path, creation time, and database expiry;
- state: `awaiting_subject_proof`, `awaiting_owner_confirmation`, `consumed`, or `frozen`;
- at most one pending subject-proof hash, authenticated ingress-item hash, and proof time;
- masked subject and endpoint projection shown only to the originating authenticated session;
- owner-confirmation session/MFA revision and confirmation time; and
- freeze reason/evidence for replay, forwarding/leak suspicion, subject collision, mismatch, or
  expiry; expiry is a terminal frozen reason, not a reusable state.

The challenge is correlation material, not a bearer authorization. An authenticated provider event
can only attach one pending subject proof. It cannot create a connection or consent. The original
authenticated Bluey session must re-establish current MFA and confirm the exact masked subject and
endpoint before one transaction consumes the challenge and writes the connection plus consent
revision. A challenge observed from another account/session, a second subject, a forwarded or leaked
token, collision, replay, mismatch, or expiry freezes the challenge and every attached proof; it
never falls back to the first presenter.

### `BusinessMessagingConnectionV1`

Required fields:

- `connection_id`, `account_id`, `provider`;
- `provider_mode`: `whatsapp_business_platform` or `apple_messages_for_business`;
- `business_endpoint_id` and its canonical hash;
- encrypted provider subject mapping plus a log-safe subject hash;
- `link_revision`, `link_authority_sha256`, `linked_at_ms`;
- `consent_revision`, `consent_sha256`;
- `capabilities`, each from a closed allowlist;
- `status`: `pending_link`, `linked_read_only`, `revoked`, `stopped`, or `deleted`;
- `provider_grant_revision` and hash, without secret material;
- `created_at_ms`, `updated_at_ms`, and optional `revoked_at_ms`; and
- exact provider terms/policy snapshot identifiers reviewed for the release.

It must not store access tokens, app/MSP secrets, raw phone numbers in searchable plaintext, or a
generic `connected=true` authority.

### `BusinessMessagingConsentV1`

Required fields:

- account and connection IDs;
- provider and provider subject hash;
- exact purposes: `owner_commands`, `review_notifications`, and/or `intervention_notifications`;
- proactive categories approved by the owner;
- locale and disclosure revision;
- capture surface: authenticated web, provider inbound initiation, or approved invitation flow;
- evidence hash and immutable capture time;
- effective and optional expiry times;
- revocation time/source; and
- successor consent revision, if any.

One consent cannot authorize personal-account access, recruiter outreach, job submission, calendar
writes, email writes, attachments, marketing, a different provider subject, or any prohibited data
use. In particular, no consent revision can authorize training, fine-tuning, evaluation, benchmarking,
or general improvement from WhatsApp Business Solution Data.

### `BusinessMessagingIngressEnvelopeV1`

Required fields:

- provider, business endpoint, provider delivery/transport identity when officially supplied;
- exact raw body SHA-256 and bounded byte length;
- authenticated header projection and key/grant revision;
- authentication outcome and failure reason code;
- provider timestamp plus server received/committed times;
- content type/encoding and parser schema version;
- encrypted short-lived raw object reference, if retained;
- immutable envelope identity and ingestion revision; and
- declared logical-item count plus an exact ordered item-manifest hash produced only after provider
  authentication and strict schema validation.

The raw envelope identity is separate from every logical item carried inside it. A WhatsApp
envelope may contain multiple entries, changes, inbound messages, and status updates. Bluey stores
the authenticated envelope once and materializes each logical item independently; it never maps one
HTTP delivery to one command by assumption. When the provider supplies a documented transport
identity, envelope authority binds provider, endpoint, transport identity, and raw hash. Without one,
the fallback envelope identity binds provider, endpoint, and exact raw hash; logical-item identity
still detects conflicting content across separate deliveries. An exact replay returns the stored
envelope result. A documented transport identity reused with changed raw bytes creates an immutable
envelope conflict and the conflict semantics below apply to its items.

### `BusinessMessagingIngressItemV1`

Required fields:

- envelope ID/hash, provider, business endpoint, ordered item path/index, and item kind;
- provider-defined message/status/close identifier fields extracted by an exact adapter-schema
  version, plus a closed logical identity hash;
- exact canonical item hash, source raw-range/projection hash, and immutable item revision;
- provider subject/conversation/object hashes appropriate to the item kind;
- link, suppression, command, plan, attempt, or status target resolved after authentication;
- state: `stored`, `preclaim_conflict`, `claimed`, `terminal`, `late_conflict`, or `quarantined`;
- claim/request-start/terminal boundary evidence, if any; and
- conflict-set ID, first-seen item hash, later item hash, hold revision, and alert evidence, if any.

Logical identity is release- and provider-schema-specific; array position alone is never identity.
Exact identity plus exact canonical item hash is an idempotent replay. Exact identity with changed
canonical bytes has two fail-closed outcomes:

- **Pre-claim conflict:** if no item in the conflict set has been claimed or crossed a downstream
  authority boundary, quarantine every version and all derived unclaimed plans. Neither version may
  execute.
- **Late conflict:** if an earlier version was already claimed, reached request start, or became
  terminal, preserve that immutable historical result. Quarantine the later version, create a
  provider/endpoint/subject/action operational hold and security alert, and permit zero additional
  effect from the conflict. Never rewrite the earlier result or falsely claim that it did not run.

Status items use documented provider identifiers and transition evidence rather than borrowing only
an inbound message identity. For example, two documented transitions for one provider message object
are distinct logical items when the reviewed schema's status discriminator and evidence fields differ;
that normal progression is not an identity conflict. Reuse of one exact transition identity with
changed canonical bytes is a conflict. Multi-item persistence is atomic at the envelope/item-manifest
boundary: Bluey does not ACK an envelope while silently dropping one item.

### `BusinessMessagingCommandIntentV1`

Required fields:

- ingress envelope ID/hash and exact logical item ID/hash;
- account-link and consent revision/hash;
- normalized command and typed arguments;
- opt-out parser, command parser, locale-table, and grammar versions;
- command risk class;
- result: `informational`, `plan_created`, `step_up_required`, `denied`, `stopped`, or
  `unsupported`;
- immutable result hash and timestamp; and
- no model-generated authority.

### `BusinessMessagingPlanV1`

Required fields:

- `plan_id`, monotonic `plan_revision`, and immutable `plan_sha256`;
- owning account, connection, provider, business endpoint, provider subject, and consent revisions;
- exact current provider-eligibility/legal-policy revision and expiry where required;
- source ingress/intent hash;
- exact read-set revisions for referenced Jobs workspace/application/job/Track state;
- typed action and closed payload;
- user-visible preview hash;
- provider-policy projection:
  - WhatsApp customer-service-window boundary and approved template identity, or
  - Apple active-conversation/invitation eligibility and approved template identity;
- required review and step-up class;
- earliest/expiry times from database time;
- kill-switch/hold policy revision; and
- state: `simulated`, `awaiting_review`, `approved`, `cancelled`, `expired`, or `superseded`.

Any field change creates a new revision. Approval of revision `n` cannot authorize `n+1`.

### `BusinessMessagingApprovalReceiptV1`

Required fields:

- account/session/MFA authority hashes;
- exact plan ID, revision, and hash;
- exact provider grant, connection, consent, and release revisions;
- exact provider-eligibility/legal-policy revision where required;
- approval purpose and allowed effect class;
- database approval/expiry times; and
- immutable approval receipt hash.

In Phase 620A the only valid approval outcome is `simulated_no_effect`. A chat `APPROVE` command may
request a secure Bluey step-up page; the command itself cannot create this receipt for an external or
irreversible effect.

### `BusinessMessagingDispatchAttemptV1`

Required fields:

- action/plan/approval hashes;
- exact provider-eligibility/legal-policy revision where required;
- provider operation key;
- dispatch number, lease owner, opaque lease-token hash, and monotonic fence;
- request body hash with all credentials excluded;
- adapter/release/schema hashes;
- pre-effect authority recheck result;
- `request_not_started_at_ms` or `request_started_at_ms`;
- terminal state and provider object ID, when authenticated;
- exact response/status evidence hash; and
- completion/reconciliation database time.

### `BusinessMessagingEffectReceiptV1`

Allowed terminal assertions:

- `simulated_no_effect`;
- `cancelled_pre_effect`;
- `denied_pre_effect`;
- `provider_accepted`;
- `delivered` only from authenticated provider delivery evidence;
- `read` only from authenticated provider read evidence;
- `failed_pre_effect`;
- `failed_confirmed_no_effect`;
- `side_effect_unknown`;
- `stopped`; or
- `provider_closed`.

A provider HTTP success cannot be labeled `delivered` unless the official provider evidence supports
that exact assertion. A WhatsApp simulation may model only statuses in the pinned messages-webhook
schema. An Apple simulation stops at `provider_accepted` for a successful gateway response and
rejects `delivered`/`read` unless a later reviewed official Apple source defines exact authenticated
evidence for those states.

### `BusinessMessagingSuppressionV1`

Required fields:

- provider, business endpoint, provider subject hash, and optional account ID;
- source: `stop_command`, `provider_close`, `off_channel_opt_out`, `account_deletion`,
  `provider_410`, `abuse_hold`, or `operator_kill`;
- immutable evidence hash and database time;
- scope and reason code;
- state `active` or `superseded_by_reconsent`; and
- exact authenticated web re-consent revision, if superseded.

Suppression is checked before account linking, command planning, approval, claim, request creation,
request start, and reconciliation-driven retry.

## Denial-Only Locale Opt-Out Parser

Before account lookup/linking, the command grammar, translation, LLM/model code, analytics, or any
effect-capable tool, an authenticated inbound text item passes through a deterministic,
locale-versioned opt-out parser. This parser can only deny; it cannot connect, consent, plan,
approve, dispatch, or reactivate.

Its reviewed snapshot contains:

- exact parser version and provider-policy snapshot;
- supported BCP 47 locale plus fallback relationships;
- a closed exact phrase/keyword set for STOP, revoke, unsubscribe, and do-not-contact meanings;
- bounded deterministic Unicode normalization, whitespace/punctuation handling, and case folding for
  each locale, with canonical-input and canonical-output test vectors; and
- immutable reason code, locale decision, phrase-set version, and suppression evidence hash.

The universal ASCII keyword `STOP` is checked regardless of locale. A supported localized phrase is
recognized only by its reviewed deterministic table—never fuzzy matching, sentiment, translation,
an LLM, or an embedding. A missing/unsupported locale cannot create authority; `STOP` remains
available and other input proceeds only to the deny-safe command parser. Provider-native close,
off-channel opt-out, account deletion, and legal suppression bypass command parsing and directly
create equal or stronger denial. Parser/version expiry fails closed for proactive and write planning
until reviewed; it never makes an expired consent valid.

## Closed Command Grammar

After the denial-only opt-out parser, the command grammar is ASCII-only, case-insensitive at the
verb, at most 256 bytes, one line, and rejects control/format characters, confusables, quoting, URLs,
markdown, JSON, code blocks, attachments, and trailing unparsed text.

```abnf
command       = help / status / matches / show / save / pass / prepare
              / review / approve / cancel / pause / stop

help          = "HELP"
status        = "STATUS"
matches       = "MATCHES" [ SP limit ]
show          = "SHOW" SP job-ref
save          = "SAVE" SP job-ref
pass          = "PASS" SP job-ref SP pass-reason
prepare       = "PREPARE" SP job-ref
review        = "REVIEW" SP plan-ref
approve       = "APPROVE" SP plan-ref
cancel        = "CANCEL" SP plan-ref
pause         = "PAUSE"
stop          = "STOP"

limit         = %x31-35
job-ref       = "J-" 10*26base32-char
plan-ref      = "P-" 10*26base32-char
pass-reason   = "NOT_RELEVANT" / "LOCATION" / "COMPENSATION"
              / "SENIORITY" / "EMPLOYMENT_TYPE" / "OTHER_ROLE"
base32-char   = DIGIT / %x41-48 / %x4A-4E / %x50-54 / %x56-5A
```

### Command semantics

| Command | Future effect ceiling | Phase 620A result |
| --- | --- | --- |
| `HELP` | Return grammar and privacy/STOP notice | Deterministic simulated response |
| `STATUS` | Read owner-visible aggregate state | Synthetic read only |
| `MATCHES [1..5]` | Read bounded current matches | Synthetic read only |
| `SHOW J-...` | Read one owner-visible job summary | Synthetic read only |
| `SAVE J-...` | Propose reversible internal save | Immutable simulated plan only |
| `PASS J-... REASON` | Propose reversible candidate feedback | Immutable simulated plan only |
| `PREPARE J-...` | Propose application-kit preparation | Step-up plan only; no generation/write |
| `REVIEW P-...` | Display exact plan preview | Synthetic read only |
| `APPROVE P-...` | Request authenticated web step-up | Step-up-required receipt; no approval/effect |
| `CANCEL P-...` | Deny/supersede an unexecuted plan | Simulated cancellation only |
| `PAUSE` | Deny new channel plans for this account | Simulated suppression only |
| `STOP` | Immediately suppress the provider subject | Suppression receipt; no outbound response required |

Unknown input returns a bounded `unsupported` result and the `HELP` grammar. It cannot fall through
to an LLM, agent, MCP tool, shell, URL fetch, database mutation, or provider adapter. A model may
later propose explanatory copy only after both deterministic parsers have denied execution; the copy
cannot modify the typed intent. Provider-native close and the locale-versioned opt-out parser are the
only pre-grammar denial paths; neither can grant authority.

## Linking, Authentication, and Consent

### Common ceremony

1. The owner starts in an authenticated Bluey web/native session and completes required MFA.
2. Bluey displays the exact provider, business sender identity, purposes, message categories,
   retention caps, STOP behavior, privacy policy, and no-employer-outreach boundary.
3. Bluey creates a random, single-use link challenge bound to account, session, provider, business
   endpoint, purpose set, return path, and database expiry. The challenge is correlation material,
   not a bearer credential or connection grant.
4. The provider subject proves channel participation through an authenticated inbound provider event
   or approved provider account-link surface. This creates only one pending subject proof bound to
   the challenge and exact endpoint; it does not consume the challenge or write consent.
5. Bluey returns to the **same originating authenticated Bluey session**, displays the exact masked
   subject and business endpoint, and requires fresh MFA confirmation of that pairing.
6. Only then does one post-lock database transaction recheck expiry, session, MFA, subject, endpoint,
   provider eligibility, suppression, and consent disclosure; atomically consume challenge/proof;
   and write one connection plus consent revision.
7. Reuse, expiry, provider mismatch, endpoint mismatch, cross-account/session presentation, a second
   subject, forwarded/leaked-token suspicion, collision, or an active suppression freezes the
   challenge and all attached proofs. Recovery starts with a new authenticated ceremony; it never
   selects the first or latest presenter.
8. Secrets remain in the server secret manager. The portal receives only masked display state and
   hashes.

A phone number, display name, Apple opaque ID, inbound text, forwarded message, screenshot, or
possession of a Bluey account or link challenge alone is insufficient to link the other side.

### WhatsApp linking

- The user initiates a message to the published Bluey business number or enters an official,
  consented provider flow from Bluey's authenticated surface.
- Bluey binds the authenticated webhook sender identity only as a pending subject proof; the
  originating Bluey session and fresh MFA must confirm it before a connection exists.
- Bluey records whether proactive notification categories were expressly opted in.
- No personal WhatsApp session, QR code, device pairing, chat backup, or contact-list permission is
  requested.
- WABA/business phone/App/system-user setup is an operator/provider release task, never a user-account
  linking shortcut.

### Apple Messages for Business linking

- Bluey first proves a registered business and approved MSP route.
- The normal user-started conversation supplies an Apple opaque subject. Bluey links it through an
  Apple-supported OAuth2 authentication message or Bluey landing page with state, nonce, PKCE, exact
  redirect allowlist, and one-time consumption.
- Invitation linking is unavailable until Apple separately approves invitation access and the exact
  user has provided explicit phone-number opt-in.
- Phone number and opaque ID are different identifiers. Bluey does not merge them without authenticated
  provider evidence and the account-link ceremony.

### Personal Apple surfaces

- A future MessageUI composer requires an on-device user gesture, presents Apple's standard UI, and
  records only the delegate result.
- A future iMessage extension requires App Store review and visible/recent interaction. It cannot be
  used by the server dispatcher.
- Neither surface shares consent or receipts with Apple Messages for Business.

## Provider Authentication Boundary

### WhatsApp Business Platform adapter

Before any parse or ACK, a successor implementation must:

- refuse route/worker startup unless the exact current `WhatsAppProviderEligibilityV1`, legal-policy
  snapshot, grant, endpoint, adapter release, and recipient cohort are eligible and unexpired;
- enforce TLS, host/path allowlists, WAF/rate/size limits, and exact raw-body capture;
- complete official webhook verification using a server-only verify token;
- validate the official webhook signature over the exact raw bytes with the Meta app secret;
- bind the event to an allowlisted WABA/business phone endpoint and provider grant revision;
- reject unknown object types and payload shapes;
- after raw-byte authentication, enumerate every documented entry/change/message/status/close-style
  logical item using an exact schema version; atomically commit the envelope, ordered item manifest,
  and every item identity/hash before returning success;
- apply pre-claim versus late-conflict semantics independently to each logical item, never assuming
  one webhook body contains one event or that array position is stable identity;
- use only documented Graph API versions, message endpoints, permissions, and templates; and
- pin adapter behavior to a reviewed provider documentation/terms snapshot and release hash.

Access tokens and app secrets must be independently rotatable. No token is accepted from the chat
body, portal client, URL query, worker log, or simulator fixture.

### Apple Messages for Business adapter

Before any parse or ACK, a successor implementation must:

- accept traffic only through the approved MSP integration endpoint;
- verify the Bearer JWT using the decoded MSP Messaging API secret;
- require the documented algorithm, audience/issuer direction, and fresh `iat`;
- bind `Destination-Id` to the exact registered business and tenant;
- enforce content type, GZIP bounds, decompression ratio, JSON schema, and message type;
- rotate keys with explicit old/new overlap and revocation evidence;
- use the documented Apple MSP gateway only through the approved MSP context; and
- treat `close` and `410 Gone` as immediate suppression evidence.

Bluey must not become an MSP by implementation assertion. Apple approval evidence is a release gate.

## Planning and Review

The plan builder reads current Jobs authority but cannot mutate it. It must include:

- current account/connection/consent/suppression revisions;
- current provider eligibility, terms/policy snapshot, approved cohort, and data-use prohibition;
- current Career Track and profile-evidence revisions for referenced jobs;
- current original-source and signed job-integrity projection;
- application state and any intervention/reconciliation hold;
- exact user-visible data fields proposed for the response;
- provider-policy state at database time;
- whether a template is required and its approved immutable identity;
- whether authenticated web step-up is required; and
- expiry short enough that provider window and Jobs authority cannot silently drift.

No plan may include a resume body, identity document, compensation expectation, immigration/work
authorization answer, SSN, date of birth, account credential, provider token, or employer-facing
answer. The owner receives a short summary and a secure link to Bluey for sensitive review.

`APPROVE` in chat does not authorize a job submit, recruiter message, calendar write, email send,
provider invitation, or Auto-submit. Those require their existing separate authority and, where
applicable, authenticated web step-up.

## Dispatch, Replay, and Idempotency

### Inbound

- Raw-envelope identity and logical-item identity are separate. One authenticated webhook envelope
  can carry multiple inbound messages, statuses, closes, or other documented items.
- Same envelope transport identity plus the same raw hash is an idempotent envelope replay. Each
  logical item also deduplicates on its provider-schema-specific identity and canonical item hash.
- Same logical identity with changed bytes discovered before claim quarantines every conflicting
  version and its unclaimed plans; neither version executes.
- If changed bytes arrive after an earlier item was claimed, crossed request start, or became
  terminal, the earlier immutable result remains truthful. The later item is quarantined, an
  operational hold/security alert is created, and the conflict can produce zero additional effect.
- Events and items may arrive duplicated, batched, and out of order. Provider status transitions use
  their own identities and an explicit partial order, not envelope or array order.
- ACK follows atomic durable envelope/item-manifest commit, not command execution. A partially
  persisted multi-item envelope is not acknowledged.

### Outbound

The operation key is a canonical SHA-256 over at least:

```text
account + connection revision + consent revision + provider + business endpoint
+ provider subject hash + plan ID/revision/hash + action kind + template/window projection
+ exact payload hash + adapter/release hash
```

- A database unique constraint permits one logical operation.
- A dispatch attempt uses an opaque lease token and monotonic fence.
- Claim revalidates the exact plan, approval, provider grant, consent, suppression, flags, holds,
  provider-policy window, template, and release at post-lock database time.
- Request start is durably marked immediately before the adapter crosses the external boundary.
- A confirmed pre-request failure may be retried with a new fenced attempt.
- A timeout, disconnect, or crash after request start becomes `side_effect_unknown` unless official,
  authenticated provider evidence proves the outcome.
- Provider APIs are not assumed idempotent unless current official documentation and a provider
  conformance test prove the exact behavior.
- Reconciliation can add evidence to an attempt; it cannot create a new send authority.
- A delivery/read webhook may advance status. It cannot change payload, recipient, or authority.
- STOP or close received before request start cancels the attempt. If received after request start,
  it suppresses every future action and reconciliation records the in-flight uncertainty.

## STOP, Close, and Kill Switches

### STOP semantics

- Universal `STOP` and reviewed localized opt-outs are deterministically recognized before account
  lookup/linking, the command grammar, LLM/model code, analytics, and every other command.
- It creates or replays an active suppression for the exact provider/business endpoint/subject.
- It cancels all unstarted plans and leases in the same transaction where feasible.
- It requires no outbound confirmation if a confirmation could violate provider policy or race the
  suppression.
- Repeated STOP is idempotent.
- `START`, `RESUME`, a new chat, or an inbound job command cannot reactivate the channel.
- Reactivation requires authenticated Bluey web consent, MFA where required, a new consent revision,
  and provider-policy eligibility. Provider-native close may additionally require a valid provider
  re-entry point.
- Off-channel opt-out and account deletion create the same or stronger suppression.
- Consent expiry or revocation denies proactive/write work independently of whether the latest
  inbound text is a command. Neither a parser snapshot update nor a new inbound conversation can
  revive expired/revoked consent.

### Apple close

An authenticated Apple `close` event is denial authority even when the account link is missing or
broken. It blocks automated and live-agent sends for that provider subject. A `410 Gone` response is
recorded as provider-close evidence and suppresses future attempts; it is not a retry signal.

### Kill-switch scopes

| Scope | Example authority | Effect |
| --- | --- | --- |
| Global | Business messaging operational hold | Deny every plan/claim/send |
| Provider | WhatsApp or Apple provider hold | Deny that provider only |
| Business endpoint | WABA phone or Apple business ID hold | Drain one endpoint |
| Tenant/account | Account deletion, abuse, billing, security hold | Deny one owner |
| Conversation | STOP/close/provider subject suppression | Deny one subject |
| Action kind | Proactive notification, command response, invitation | Deny selected effects |
| Adapter release | Revoked/expired adapter manifest | Deny claims for that release |

Every scope is checked during plan, approval, claim, immediately before request start, and retry or
reconciliation decisions. A kill switch can deny or release safe pre-effect work. It cannot erase a
known provider effect, mark an unknown effect unsent, or manufacture delivery.

## Privacy, Retention, and Deletion

### Data minimization

- Store provider subject mappings encrypted; use keyed, domain-separated hashes for indexes/logs.
- Never log message bodies, phone numbers, opaque IDs, access tokens, app/MSP secrets, auth headers,
  one-time link tokens, resume content, or secure-review URLs.
- Do not ingest attachments in Phase 620A. A future attachment phase needs MIME allowlists, size and
  decompression limits, malware scanning, content-disposition isolation, encrypted object storage,
  and separate user consent.
- Do not copy chat content into profile facts, career evidence, training datasets, analytics events,
  or support tickets by default. WhatsApp Business Solution Data and every anonymous, aggregate,
  derived, label, embedding, metric, feedback, or evaluation form are absolutely excluded from model
  creation, development, training, fine-tuning, evaluation, benchmarking, and general improvement;
  consent cannot override this policy.
- Use short provider-safe responses and authenticated Bluey links for sensitive detail.
- Exports omit credentials and internal auth metadata. They label provider evidence separately from
  Bluey inference.

### Provisional maximum retention classes

These are **architecture inferences**, not production policy approval:

| Class | Maximum | Notes |
| --- | --- | --- |
| Simulator synthetic raw input | Test lifetime, at most 24 hours | In-process or isolated test store only; no real identifiers |
| CI simulator artifacts | 14 days | Hashes/statuses only; no message body or secret |
| Future live raw webhook object | 7 days after durable normalization | Encrypted, tightly scoped break-glass access |
| Future normalized owner command/body | 30 days after terminal state | Shorter on unlink, account deletion, or user request |
| Future attachment | Disabled; proposed maximum 7 days | Separate phase and consent required |
| Sanitized attempt/authority receipt | 400 days | Hashes, reason codes, revisions; no body/token/phone |
| Active STOP/close suppression | Until authenticated re-consent or deletion/legal disposition | Minimal provider/business/subject hash and evidence hash only |
| Provider credential | Only while grant is active | Secret manager; delete/revoke on disconnect |

Legal, privacy, security, provider-contract, regional, and customer-support owners must approve final
periods before live data. If law or provider terms require a shorter period, the shorter period wins.

### Deletion

- Disconnect revokes provider grants, activates a write fence, drains safe pre-effect leases, and
  deletes provider credentials after revocation evidence.
- Account deletion fences ingress correlation and all writes before removing connection, consent,
  messages, objects, plans, and receipts according to the account-deletion authority.
- Raw/object deletion requires exact object identity and read-back/tombstone evidence; a failed purge
  remains pending rather than falsely reported complete.
- Hash-only suppression may be retained only as needed to honor opt-out or law. It cannot be used for
  analytics, matching, or re-identification.
- Backups and replicas must receive deletion propagation under a documented maximum age.

## Threat Model

| Threat | Required control |
| --- | --- |
| Forged webhook | Raw-byte provider signature/JWT verification before parse; endpoint allowlist |
| Replay or duplicate delivery | Separate envelope and logical-item identities/hashes; idempotent stored result per layer |
| Multi-item webhook truncation/conflation | Authenticate raw bytes, atomically persist exact ordered item manifest, identity every item independently before ACK |
| Logical item ID with changed bytes before claim | Quarantine every conflicting version and derived unclaimed plan; zero effect; security alert |
| Logical item ID with changed bytes after claim/effect | Preserve prior immutable outcome; quarantine later item; endpoint/subject/action hold and alert; zero additional effect |
| Cross-tenant confused deputy | Bind account, provider, business endpoint, subject, connection, consent, and plan in every hash |
| Stolen/forwarded/leaked link challenge | Channel event creates pending proof only; original session plus fresh MFA confirms masked subject/endpoint; mismatch/collision/expiry freezes challenge |
| Phone recycling or SIM takeover | Chat possession alone cannot link or re-consent; original authenticated session confirmation, MFA, and warnings |
| Apple opaque-ID/phone conflation | Separate identifier types; merge only on authenticated provider evidence and link revision |
| Prompt injection in a message | Closed deterministic grammar; unknown text cannot reach effect tools |
| Unicode/control/log injection | ASCII command grammar, bounded canonicalization, structured redacted logs |
| Malicious link or open redirect | No arbitrary URL arguments; exact redirect allowlist, PKCE, state, nonce, single use |
| STOP race with queued send | Suppression row and pending cancellation serialized; recheck immediately before request start |
| Provider credential compromise | Server secret manager/HSM, least privilege, rotation, revisioned grants, endpoint kill switch |
| Compromised MSP/service provider | Contract/DPA review, tenant isolation, minimal data, signed ingress, audited operator access |
| Worker credential theft | Short leases, opaque token hash, monotonic fence, mTLS/workload identity in a future phase |
| Template/window bypass | Server-owned provider-policy projection, exact template revision, database time, adapter checks |
| Ambiguous network result | `side_effect_unknown`; no blind retry; authenticated reconciliation only |
| Out-of-order status webhooks | Monotonic partial-order state machine and immutable event receipts |
| Media SSRF/malware/decompression bomb | Attachments disabled; separate gated pipeline if later approved |
| PII exfiltration in replies | Closed response schemas, sensitive-field denylist, review link instead of content |
| WhatsApp data reaches a model-improvement path | Type/policy deny raw, anonymous, aggregate, and derived Business Solution Data; consent cannot override; scan manifests and lineage |
| Other-channel cross-account model training | Owning-account-only operational purpose unless a separate channel-specific reviewed authority exists |
| Insider/operator misuse | Least privilege, dual control for provider/kill changes, immutable access audit, no body in routine tools |
| Retention or deletion drift | Retention class on every object, database-time sweeper, purge evidence, alert on overdue work |
| Feature-flag bypass | Multi-layer flag, release, provider, tenant, consent, hold, and exact plan checks |
| Provider terms/AI eligibility change | Versioned legal/provider eligibility record; exact cohort and purpose binding; route/worker startup and every effect boundary fail closed on expiry/change |

## Release Flags: All Zero

No variable is added or changed by this document. A successor may propose the following names, but
their required Phase 620A and production values are `0`:

| Flag | Required value | Purpose |
| --- | --- | --- |
| `BLUEY_JOBS_BUSINESS_MESSAGING_SIMULATOR_ENABLED` | `0` | No simulator in production process |
| `BLUEY_JOBS_BUSINESS_MESSAGING_INGRESS_ENABLED` | `0` | No live callback ingestion |
| `BLUEY_JOBS_BUSINESS_MESSAGING_PLANNING_ENABLED` | `0` | No account plans from live provider input |
| `BLUEY_JOBS_WHATSAPP_BUSINESS_PLATFORM_ENABLED` | `0` | No WhatsApp provider path |
| `BLUEY_JOBS_APPLE_MESSAGES_FOR_BUSINESS_ENABLED` | `0` | No Apple business provider path |
| `BLUEY_JOBS_BUSINESS_MESSAGING_DISPATCH_ENABLED` | `0` | No provider write |
| `BLUEY_JOBS_BUSINESS_MESSAGING_RECONCILIATION_ENABLED` | `0` | No live provider status worker |
| `BLUEY_JOBS_BUSINESS_MESSAGING_PROACTIVE_ENABLED` | `0` | No proactive template/invitation |
| `BLUEY_JOBS_PERSONAL_MESSAGE_COMPOSER_ENABLED` | `0` | No MessageUI/iMessage-extension surface |
| `BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED` | `0` | Existing provider OAuth write authority stays off |
| `BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED` | `0` | Existing general communication dispatch stays off |
| `BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED` | `0` | Existing general reconciliation stays off |
| `BLUEY_JOBS_MAILBOX_SYNC_ENABLED` | `0` | Existing mailbox sync stays off |

Even a future `1` is insufficient without a signed reviewed release, provider registration and
grant, exact tenant cohort, active consent, clear holds, and approved action authority.

## No-Egress Simulator

The simulator is an **architecture inference** and the only permitted Phase 620A implementation
target for a successor.

Requirements:

- in-process provider transports only; no HTTP client, DNS, socket, browser, device, SMS, or callback;
- synthetic provider endpoints and subjects that cannot resolve to real recipients;
- deterministic clock, randomness, provider event IDs, key revisions, and failure schedule;
- fixture-only signing keys clearly rejected by production configuration;
- provider adapters compiled or injected against a transport whose network method fails the test if
  invoked;
- isolated database transaction or disposable test database, never a production connection;
- no secret-manager lookup, OAuth, WABA/MSP registration, template creation, or webhook subscription;
- exact captured request projections with secrets structurally impossible;
- final proof asserting zero network attempts, zero provider credentials, zero live endpoints, zero
  production writes, and every release flag `0`; and
- test cleanup that removes synthetic raw inputs within 24 hours.

The simulator may return only provider-specific states justified by the pinned official schema:

- a synthetic WhatsApp adapter may emit the documented status transitions included in the reviewed
  webhook schema, including `delivered` or `read` where that schema supplies the corresponding
  authenticated status evidence; and
- the Apple adapter may emit `provider_accepted` from a scripted successful gateway response, but
  it must not emit `delivered` or `read` unless a later reviewed official Apple source defines exact
  authenticated evidence for that stronger state.

Both adapters may emit `close`, `410`, pre-effect failure, and ambiguous timeout only where the
provider-specific plan permits those conditions. Every result is labeled simulated and can never
satisfy a production provider receipt or release gate. A generic cross-provider simulator status is
invalid.

## Simulator Acceptance Tests

All tests below are **specified, not run** by this documentation phase.

| ID | Scenario | Required result |
| --- | --- | --- |
| 620A-SIM-001 | Production-default configuration, including `BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED` | Every existing/proposed flag resolves to `0`; workers and routes do not start |
| 620A-SIM-002 | Simulator test transport | Zero DNS/socket/HTTP/browser/device attempts; network method is unreachable |
| 620A-SIM-003 | Real-looking provider endpoint or credential in fixture | Fixture rejected before plan creation |
| 620A-SIM-004 | Personal WhatsApp provider name/session/QR input | Closed rejection `personal_whatsapp_unsupported` |
| 620A-SIM-005 | Personal iMessage unattended-send plan | Closed rejection `personal_imessage_background_unsupported` |
| 620A-SIM-006 | MessageUI composer projection | Result cannot exceed `simulated_user_presented`; never `sent` or `delivered` |
| 620A-SIM-007 | Generic provider adapter attempts to emit `delivered`/`read` | Type/policy rejection; statuses must come from an exact provider-specific schema |
| 620A-GRAM-001 | Every valid command and boundary ID length | One typed intent with exact canonical bytes |
| 620A-GRAM-002 | Unknown verb, extra token, URL, JSON, markdown, multiline, control or Unicode confusable | `unsupported`; no plan/effect adapter call |
| 620A-GRAM-003 | LLM/prompt-injection text | Deterministic denial/help only; no tool invocation |
| 620A-GRAM-004 | `MATCHES 0`, `6`, negative, decimal, or free-form limit | Rejected, no partial parse |
| 620A-GRAM-005 | Invalid or cross-account job/plan reference | Generic not-found/denied without existence oracle |
| 620A-LINK-001 | Valid original-session/MFA challenge, authenticated subject proof, and exact masked-subject confirmation | One atomic connection/consent revision |
| 620A-LINK-002 | Expired, replayed, cross-account, cross-provider, or endpoint-mismatched challenge | Fail closed; no connection |
| 620A-LINK-003 | Phone/display name/chat assertion only | Insufficient link authority |
| 620A-LINK-004 | Active STOP/close suppression during linking | Link/re-consent denied pending authenticated web ceremony |
| 620A-LINK-005 | Attacker steals/forwards/leaks a challenge, proves a different subject, and lacks the originating session/MFA confirmation | Pending proof cannot bind; mismatch/rejection/expiry freezes challenge and proofs; neither subject gets a connection |
| 620A-LINK-006 | Valid subject proof without return to original session and fresh MFA | `awaiting_owner_confirmation`; zero connection/consent authority |
| 620A-LINK-007 | Challenge or MFA expires after subject proof but before confirmation | Challenge/proof freeze; no fallback or silent extension |
| 620A-LINK-008 | Same challenge receives a second subject, session, or endpoint proof | Collision freeze and alert; neither presenter is selected |
| 620A-WA-001 | Valid synthetic WhatsApp signature over exact raw multi-item bytes | Durable authenticated envelope, exact ordered item manifest, and all items before ACK |
| 620A-WA-002 | Missing/bad signature or body changed after signature | Rejected before JSON parse |
| 620A-WA-003 | Same envelope/raw hash and same logical item identities/hashes | Idempotent replay of stored envelope and item results |
| 620A-WA-004 | Same logical item identity with changed bytes before any claim | Quarantine all conflicting versions and derived plans; zero effect |
| 620A-WA-005 | User inbound at 24-hour boundary | Database-time decision exactly follows reviewed provider rule |
| 620A-WA-006 | Outbound outside window without exact approved template | Denied before lease |
| 620A-WA-007 | Template ID/category/language/revision mismatch | Approval invalidated; no dispatch |
| 620A-WA-008 | Missing proactive category opt-in | Proactive plan denied |
| 620A-WA-009 | Same logical item identity changes after the first version was claimed or terminal | Preserve prior immutable result; later item quarantined; hold/alert; zero additional effect |
| 620A-WA-010 | One envelope contains two messages plus delivered/read status items | Every item independently identified and applied once; no array-position identity or status conflation |
| 620A-WA-011 | Missing/stale/ambiguous AI-provider eligibility, terms snapshot, cohort evidence, or required approval | Route/worker/plan/claim/request-start denied; all WhatsApp flags remain `0` |
| 620A-WA-012 | IP, locale, residence, billing country, consent, WABA, token, or template offered as regional eligibility | Rejected as insufficient evidence |
| 620A-WA-013 | Third Party Agent Terms/Platform offered as Cloud API onboarding or eligibility evidence | Rejected; separate product surface with no inferred access |
| 620A-WA-014 | Any raw/anonymous/aggregate/derived WhatsApp Business Solution Data offered to training, fine-tuning, evaluation, benchmark, embedding corpus, reward, analytics-improvement, or model-serving-improvement path | Type/policy rejection irrespective of consent; lineage alert; zero write |
| 620A-APPLE-001 | Valid synthetic MSP JWT direction, algorithm, audience/issuer, and fresh `iat` | Authenticated envelope |
| 620A-APPLE-002 | Missing token, wrong algorithm/claim/secret, future `iat`, or older than one hour | Rejected before body parse |
| 620A-APPLE-003 | Old/new key overlap and old-key revocation | Only bounded overlap accepted; revoked key rejected |
| 620A-APPLE-004 | Destination business ID mismatch | Cross-tenant rejection |
| 620A-APPLE-005 | GZIP bomb, invalid JSON, unsupported message type | Bounded rejection, no plan |
| 620A-APPLE-006 | Invitation without feature approval, explicit opt-in, or exact Apple template | Denied before lease |
| 620A-APPLE-007 | Authenticated `close` event | Active suppression; all pending unstarted work cancelled |
| 620A-APPLE-008 | Simulated `410 Gone` | `provider_closed`; no retry; suppression active |
| 620A-APPLE-009 | Simulated successful Apple gateway response | At most `provider_accepted`; no `delivered`/`read` without later pinned official evidence |
| 620A-APPLE-010 | Apple simulator emits fabricated `delivered` or `read` | Type/policy rejection; no receipt advancement |
| 620A-PLAN-001 | Exact replay of command | One intent/plan operation; stored result returned |
| 620A-PLAN-002 | Plan payload, recipient, consent, job authority, template, or adapter changes | New revision; prior approval invalid |
| 620A-PLAN-003 | Chat `APPROVE P-...` | `step_up_required`; zero approval and provider effect |
| 620A-PLAN-004 | Web approval for wrong/expired plan hash | Denied; no lease |
| 620A-PLAN-005 | Current Jobs/source/integrity authority changes before claim | Plan superseded or review required |
| 620A-DISP-001 | Two workers claim one action | One lease/fence wins; one provider operation key |
| 620A-DISP-002 | Crash before durable request-start marker | Safe pre-effect retry only |
| 620A-DISP-003 | Timeout/crash after request-start marker | `side_effect_unknown`; no automatic resend |
| 620A-DISP-004 | Duplicate/out-of-order provider-specific status items, including pinned WhatsApp delivered/read fixtures | Monotonic status; immutable evidence; no regression or cross-provider status inference |
| 620A-DISP-005 | Provider 2xx without delivery webhook | At most `provider_accepted` |
| 620A-STOP-001 | STOP before link, after link, repeated, mixed case | One active suppression; idempotent denial |
| 620A-STOP-002 | STOP races approved/unstarted lease | Suppression and cancellation win before request start |
| 620A-STOP-003 | STOP races started request | Future sends blocked; started attempt remains reconciled/unknown |
| 620A-STOP-004 | `START`, `RESUME`, new command, or new conversation after STOP | Cannot reactivate |
| 620A-STOP-005 | Global/provider/endpoint/account/conversation/action/release kill switch | Denied at plan, approval, claim, and pre-request recheck |
| 620A-STOP-006 | Every reviewed localized opt-out phrase and canonicalization vector | Deterministic suppression with exact locale/parser/policy version before linking/LLM |
| 620A-STOP-007 | Mixed case/approved punctuation or whitespace variant in the exact locale table | Same canonical suppression; no command/model path |
| 620A-STOP-008 | Unsupported locale, expired parser snapshot, or ambiguous free text | No authority; universal `STOP` still works; proactive/write planning stays fail-closed where snapshot is required |
| 620A-STOP-009 | Revoked or expired consent followed by a valid non-STOP command/new conversation | Consent remains denied; no plan, reactivation, or external effect |
| 620A-STOP-010 | Opt-out races link proof, owner confirmation, plan claim, or request start | Suppression wins every pre-effect boundary; started effect remains truthful/unknown and no future send occurs |
| 620A-PRIV-001 | Structured log and tracing scan | No body, number, opaque ID, token, secret, auth header, secure URL, or resume data |
| 620A-PRIV-002 | Attachment or media event | Metadata-only denial; no download/fetch |
| 620A-PRIV-003 | Message text offered to profile/training/analytics path | Type/policy rejection |
| 620A-PRIV-004 | Retention boundary and account deletion | Exact due objects purged; suppression handled by approved policy; evidence retained only as allowed |
| 620A-PRIV-005 | Export | No credentials/auth headers/internal secrets; facts and inferences labeled |
| 620A-PRIV-006 | User consents to model improvement using WhatsApp data | Consent cannot create authority; absolute Business Solution Data exclusion remains active |
| 620A-E2E-001 | Full synthetic WhatsApp command flow | Auth -> link -> consent -> intent -> plan -> simulated receipt; zero external effect |
| 620A-E2E-002 | Full synthetic Apple user-started flow | JWT -> opaque subject link -> plan -> simulated receipt; zero external effect |
| 620A-E2E-003 | End-of-suite side-effect audit | Zero real messages, callbacks, provider objects, credentials, subscriptions, flags, or production writes |

## Future Production Gates

Simulator success is necessary but insufficient. Live work requires separate reviewed phases for:

1. legal/privacy review of the exact candidate use case, the WhatsApp AI Provider restriction,
   primary-versus-incidental classification, provider-controlled regional eligibility, any required
   written approval, Business Solution Data no-training lineage controls, provider terms,
   DPA/controller/processor roles, retention, deletion, and support escalation;
2. security review of webhook edge, secret storage, rotation, tenant isolation, WAF, mTLS/workload
   identity, operator access, and incident response;
3. a registered, verified Bluey WhatsApp business, WABA, business phone, Meta app, system user,
   least-privilege scopes, approved templates, webhook subscription, and sandbox conformance;
4. a registered Apple Messages for Business identity and approved MSP contract/integration, or a
   completed and evidenced Apple MSP onboarding and approval;
5. exact provider policy/window/template conformance tests against provider sandboxes;
6. data schema, migrations, PostgreSQL locking, object-store lifecycle, export, and deletion review;
7. portal/native linking, consent, STOP, privacy, secure-review, and accessibility review;
8. signed adapter release authority and rollback/revocation procedures;
9. synthetic, internal, and employee canaries with no third-party recipients;
10. cohort limits, rate/spend caps, abuse monitoring, human escalation, support runbook, and drills;
11. kill-switch and rollback rehearsal, including STOP during in-flight ambiguity;
12. exact-tip CI/privacy/security evidence, independent review, and explicit launch authorization; and
13. a separately authorized production phase that changes flags from `0`, if ever approved.

The WhatsApp gate is not merely a paperwork follow-up. Unless the live release has an exact,
unexpired `WhatsAppProviderEligibilityV1` for the intended cohort and purpose, implementation and
provider-account readiness cannot authorize route startup or a live message. Third Party Agent
Platform availability cannot substitute for this gate.

Recruiter/employer/C2C outreach, personal messaging, job submission, attachments, proactive marketing,
and personal iMessage composer work remain separate product and authority decisions. None is implied by
owner-channel launch.

## Official Provider Sources

All sources below are public official-provider documentation, accessed 2026-08-30. Provider terms and
documentation can change; a production phase must re-read and version the then-current sources.

### Meta / WhatsApp

- [WhatsApp Cloud API overview](https://developers.facebook.com/docs/whatsapp/cloud-api/overview)
- [WhatsApp Cloud API get started](https://developers.facebook.com/docs/whatsapp/cloud-api/get-started)
- [WhatsApp Cloud API webhooks](https://developers.facebook.com/docs/whatsapp/cloud-api/webhooks)
- [WhatsApp Cloud API send messages](https://developers.facebook.com/docs/whatsapp/cloud-api/guides/send-messages)
- [WhatsApp Business Messaging Policy](https://business.whatsapp.com/policy)
- [WhatsApp Business Terms of Service](https://www.whatsapp.com/legal/business-terms?lang=en)
- [WhatsApp Business Solution Terms](https://www.whatsapp.com/legal/business-solution-terms)
- [WhatsApp Terms of Service for Use of Third Party Agents](https://www.whatsapp.com/legal/third-party-agents-terms)
- [Meta Terms for WhatsApp Business](https://www.whatsapp.com/legal/meta-terms-whatsapp-business?lang=en)
- [WhatsApp messages webhook reference](https://developers.facebook.com/documentation/business-messaging/whatsapp/webhooks/reference/messages)

### Apple

- [Apple Messages for Business REST API overview](https://register.apple.com/resources/messages/msp-rest-api/)
- [Apple Messages for Business MSP onboarding](https://register.apple.com/resources/messages/msp-onboarding/)
- [Apple Messages for Business messages received](https://register.apple.com/resources/messages/msp-rest-api/messages-received)
- [Apple Messages for Business messages sent](https://register.apple.com/resources/messages/msp-rest-api/messages-sent)
- [Apple Messages for Business invitation messages](https://register.apple.com/resources/messages/msp-rest-api/business-updates)
- [Apple Business Chat Accounts Terms of Use](https://register.apple.com/tou/bca/latest/en)
- [Apple Messages for Business authentication message](https://register.apple.com/resources/messages/msp-rest-api/type-interactive#authentication-message)
- [Apple Messages framework](https://developer.apple.com/documentation/messages/)
- [Apple `MSConversation` message insertion](https://developer.apple.com/documentation/messages/msconversation/insert%28_%3Acompletionhandler%3A%29-3g248)
- [Apple iMessage extension visibility limit](https://developer.apple.com/documentation/messages/msmessageerrorcode/sendwhilenotvisible)
- [Apple iMessage extension recent-interaction limit](https://developer.apple.com/documentation/messages/msmessageerrorcode/sendwithoutrecentinteraction)
- [Apple `MFMessageComposeViewController`](https://developer.apple.com/documentation/messageui/mfmessagecomposeviewcontroller)
- [Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)

## Handoff

The implementation successor must:

1. treat this as a simulator architecture, not provider or production authority;
2. preserve the business-versus-personal channel distinction in types, UI, docs, and tests;
3. implement no network transport until simulator tests and independent review pass;
4. retain current communication authority, hashing, lease, hold, and ambiguous-effect invariants;
5. keep every listed flag at `0` and all production flags unchanged;
6. implement raw-envelope and logical-item identity separately, with truthful pre-claim and late
   conflict semantics for batched provider messages/statuses;
7. require original-session MFA confirmation after pending channel proof, and run the deterministic
   locale opt-out parser before linking, command parsing, or model code;
8. make WhatsApp Business Solution Data structurally unavailable to every training/evaluation/general
   improvement path regardless of consent;
9. record every architecture inference as a Bluey decision, not a provider claim; and
10. stop for explicit authority before any account registration, credential, callback, sandbox,
   external message, deploy, flag, or production action.
