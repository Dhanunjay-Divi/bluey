# Round 604 - Jobs ATS Certification Authority

**Date:** 2026-08-06

**Branch:** `feat/phase-604-jobs-ats-certification`

**Base commit:** `427e3e3dda1367d302608225dadbecbfe9fac904`

**Status:** Source-complete and locally verified; no ATS, provider, tenant, target,
adapter, runner, or unattended submission path is certified, enabled, launched,
or distributed by this document

## Objective

Create the server-owned authority that can truthfully promote one exact ATS
target, adapter layout, and runner build from Review first to certified Auto
submission without letting a source label, client, browser page, synthetic
fixture, or stale canary manufacture that authority.

This round must make certification:

- independently signed and immutable;
- exact to the provider target, adapter bundle, observed layout, and runner;
- revisioned, expiring, revocable, and protected by immediate circuit breakers;
- frozen into the Application Kit and rechecked before the irreversible action;
- recoverable after the irreversible marker without granting another click; and
- truthful in the Jobs portal and operations surface.

Discovery coverage, review-fill coverage, ATS certification, Career Track Auto
authorization, and Browser distribution remain separate capabilities. Passing
one never implies the others.

## Current Gap

At the base commit:

- Greenhouse `2026.07.1-beta.1` and Lever `2026.07.0-beta.1` have exact
  provider state machines and exact-submit proof, but both are explicitly
  `beta_review`, uncertified, and require a per-run final-review approval.
- Workday, Ashby, and SmartRecruiters use generic review-fill adapters. They do
  not have provider-specific final-submit authority.
- Server eligibility recognizes exact Greenhouse and Lever hosts only as
  `beta_review`. There is no durable path that can return `certified`.
- Final-submit validation hard-codes provider, adapter version, and submit
  control. It does not resolve an active tenant certification.
- No certification ID or digest is frozen into eligibility, the Application
  Kit admission, local/cloud run contracts, checkpoints, final-submit proof,
  receipt, or recovery authority.
- No database records, signed evidence format, activation head, revocation
  ledger, layout quarantine, circuit breaker, or operations ceremony exists
  for ATS certification.
- Existing provider fixtures are valuable synthetic safety evidence. They are
  not authorized tenant evidence and cannot justify production activation.

The internal names `CertifiedFinalSubmitAdapter`, `certifiedProviderJobKey`,
and `adapterCanFinalize` describe exact-submit mechanics. They are not proof
that Greenhouse or Lever is production-certified.

## Non-Goals

This round does not:

- certify or submit to a live employer tenant;
- create authorized test vacancies or provider accounts;
- enable local or cloud Browser distribution;
- deploy a runner, Temporal worker, database migration, portal, or service;
- change any production feature flag;
- implement provider-specific Workday, Ashby, or SmartRecruiters submission;
- allow semantic or visual automation to click Submit unattended;
- bypass CAPTCHA, 2FA, assessments, login walls, rate limits, or platform rules;
- treat a fixture, screenshot, page text, or pixel observation as submission
  authority;
- store field values, candidate answers, documents, cookies, OTPs, page bodies,
  screenshots, or other PII in certification observations;
- make Browser packaging evidence imply ATS certification, or ATS certification
  imply that a Browser artifact is distributed; or
- claim universal ATS coverage or public unattended submission.

## Authority Principles

1. **The server owns capability.** A customer request, discovery row, URL,
   source catalog entry, adapter result, or portal state cannot set
   `certified`.
2. **Authority is positive and exact.** Absence, ambiguity, expiry, drift,
   quarantine, revocation, or circuit-open state resolves to Review, Takeover,
   Handoff, or Blocked.
3. **Code and environment are part of certification.** An adapter upgrade,
   Browser release, Chromium build, cloud image, target layout, or suite change
   does not inherit an older certification.
4. **Synthetic evidence is shadow-only.** It may test contracts and propose a
   candidate manifest, but cannot satisfy a production activation.
5. **Promotion and testing are independent.** Evidence-signing keys cannot
   activate a manifest; promotion keys cannot manufacture evidence.
6. **Incidents stop future side effects, not recovery.** Revocation before the
   irreversible marker blocks Submit. Revocation after that marker permits only
   exact result/receipt recovery and reconciliation.
7. **History is append-only.** Signed objects, quarantine observations,
   activations, revocations, circuit transitions, and consumed run bindings are
   never rewritten or deleted as an operational shortcut.

## Signed And Immutable Authority Model

All signed objects use strict schema-one JSON with unknown fields rejected,
canonical UTF-8 bytes, deterministic key ordering, safe integers, canonical
base64url, bounded arrays and strings, and SHA-256 content identity. Duplicate
JSON keys, non-canonical encodings, invalid Unicode, future timestamps, expired
authority, key-role reuse, threshold shortfall, and byte-conflicting replay fail
closed.

Ed25519 private keys remain outside the repository, Browser packages, Jobs API
environment, workflow output, and evidence objects. The API receives only an
independently approved public root anchor and signed public trust policy.

### `AtsCertificationTrustPolicyV1`

The root-authorized policy defines:

- `schemaVersion: 1`;
- `policyId`, `sequence`, `predecessorSha256`, `issuedAtMs`, `notBeforeMs`, and
  `expiresAtMs`;
- disjoint key maps and thresholds for `manifest`, `evidence`, `activation`,
  `revocation`, and `layout_observation` roles;
- allowed provider families, evidence classes, suite namespaces, and maximum
  certification lifetime;
- maximum clock skew, manifest size, target count, observation count, and
  evidence-object count; and
- the previous policy digest for monotonic rotation.

No one key ID or public key may satisfy two roles in the same policy.

### `AtsCertificationManifestV1`

The manifest is independently threshold-signed by manifest keys. Its canonical
bytes bind:

- `schemaVersion: 1`, `manifestId`, `policySha256`, `sequence`,
  `predecessorManifestSha256`, `issuedAtMs`, `testedAtMs`, and `expiresAtMs`;
- one exact provider and canonical target selector;
- exact allowed provider hosts and provider page variant;
- `adapterKind`, `adapterVersion`, `adapterBundleSha256`, and exact final-submit
  control ID;
- `layoutSchemaVersion`, the complete allowed `layoutObservationSha256` set,
  and one aggregate `layoutSetSha256`;
- `suiteId`, `suiteVersion`, `suiteManifestSha256`, and required stable check
  IDs;
- exact source commit and authority-contract version;
- exact local and/or cloud execution targets;
- evidence class and immutable evidence-object metadata;
- the measured result summary, with zero-tolerance counters stored explicitly;
  and
- a bounded, non-secret reason and approval reference.

An execution target is exact:

```text
local:
  platform + architecture
  Browser release manifest SHA-256
  Browser artifact SHA-256
  Browser build descriptor SHA-256
  Chromium revision and executable SHA-256

cloud:
  platform + architecture
  runner build ID
  immutable container image digest
  automation bundle SHA-256
  Chromium revision and executable SHA-256
```

A manifest may contain both targets only when the complete required suite passed
independently on both. A local-only certification grants no cloud authority and
vice versa.

### `LayoutObservationV1`

Every layout used by a manifest is separately signed by an authorized
layout-observation key. It contains only PII-free structural evidence:

- `schemaVersion: 1`, provider, canonical target fingerprint, page variant,
  adapter version, runner target digest, observed time, and expiry;
- normalized control kinds, required/optional state, stable provider attribute
  hashes, bounded option-shape metadata, and conditional-control relationships;
- effective form method, encoding, target, provider-bound action identity, and
  submit-control identity;
- challenge categories, step count, and confirmation-state categories; and
- the observation and predecessor digests.

It must not contain candidate values, labels containing candidate data, full
URLs with query strings, page text, HTML, screenshots, documents, selectors
that embed user values, cookies, tokens, credentials, or employer secrets.
Canonical target identifiers are stored separately under server authority; the
observation uses their opaque fingerprint.

An observation from a synthetic fixture is marked `synthetic` and remains
shadow-only. Production activation requires the policy-defined authorized
sandbox and authorized-live evidence classes.

### `AtsCertificationActivationV1`

Activation is independently threshold-signed by activation keys. It contains:

- `schemaVersion: 1`, `activationId`, `policySha256`, `manifestSha256`,
  `targetKey`, `runnerTargetKey`, `sequence`, `predecessorActivationSha256`,
  `issuedAtMs`, `effectiveAtMs`, and `expiresAtMs`;
- exact mode `auto_submit`;
- the required initial canary window, account cap, concurrency cap, and daily
  side-effect cap;
- the immutable canary-evidence commitment, which must exactly equal the
  canonical certification-manifest digest already binding the independently
  signed evidence/layout objects, complete check/runtime matrix, suite, and
  zero-tolerance results; and
- a bounded approval reference.

Activation cannot widen a manifest, replace its target, extend its expiry,
change its runner, or weaken required checks. A compare-and-swap apply operation
moves one target/runner head only when the expected head revision and transition
digest still match.

### `AtsCertificationRevocationV1`

Revocation is independently threshold-signed by incident keys. It contains:

- `schemaVersion: 1`, `revocationId`, `policySha256`, monotonically increasing
  sequence, `issuedAtMs`, scope, affected digests/keys, and bounded reason;
- predecessor-bound scope for an activation, adapter bundle, Browser release
  manifest, evidence object, layout observation, certification manifest,
  policy, runner build, runner image, exact runtime tuple, certification scope,
  provider target, or trust key; and
- an explicit `blocksNewIrreversibleActions: true` invariant.

Revocation is append-only and irreversible. Recovery authority for an already
marked run is evaluated from the run's frozen binding, not from current
permission to begin another side effect.

## Exact Persistence Records

SQLite and PostgreSQL receive paired migrations with the same constraints and
indexes. Canonical signed bytes remain the authority; relational columns are
indexed projections used for locking and lookup.

### Trust and signature records

`jobs_ats_certification_signature_sets`

- `sha256` primary key;
- `object_kind`, `object_sha256`, `policy_sha256`, `role`, and `threshold`;
- canonical signature-set bytes;
- `imported_at_ms`.

`jobs_ats_certification_signatures`

- signature-set digest plus `key_id` composite primary key;
- canonical signature bytes and verified public-key digest;
- unique object/key use.

`jobs_ats_certification_trust_policies`

- policy digest primary key, ID, sequence, predecessor digest, validity window,
  canonical bytes, signature-set digest, and import time;
- unique ID/sequence and predecessor transition.

`jobs_ats_certification_trust_keys`

- policy digest plus role plus key ID primary key;
- public key, public-key digest, validity, and revocation projection.

### Certification records

`jobs_ats_certification_manifests`

- manifest digest primary key;
- manifest ID, policy digest, sequence, predecessor, provider, target key,
  runner target key, adapter version and bundle digest, layout-set digest,
  suite ID/version/digest, source commit, evidence class, validity window,
  canonical bytes, signature-set digest, and import time;
- unique provider/target/runner/sequence and immutable byte replay.

`jobs_ats_certification_layout_observations`

- observation digest primary key;
- provider, target fingerprint, runner target key, adapter version, page
  variant, evidence class, observed/expiry times, predecessor digest, canonical
  bytes, signature-set digest, and import time.

`jobs_ats_certification_evidence_objects`

- manifest digest plus stable evidence ID primary key;
- check ID, evidence class, immutable object key, SHA-256, size, media type,
  captured time, and redaction-schema version;
- no raw evidence body in the authority row.

`jobs_ats_certification_activations`

- activation digest primary key;
- activation ID, policy and manifest digests, target and runner keys, sequence,
  predecessor, the canary-evidence commitment equal to the exact canonical
  certification-manifest digest, bounded rollout limits, validity window,
  canonical bytes, signature-set digest, and import time.

`jobs_ats_certification_activation_heads`

- target key plus runner target key primary key;
- head revision, activation digest, transition digest, and updated time;
- changed only by compare-and-swap activation or an exact higher-sequence
  replacement.

`jobs_ats_certification_revocations`

- revocation digest primary key;
- policy digest, sequence, scope, subject digest/key, reason reference, issued
  time, canonical bytes, signature-set digest, and import time;
- append-only unique sequence.

### Drift, circuit, and run records

`jobs_ats_layout_quarantine`

- quarantine ID primary key;
- provider, target/runner keys, observed layout digest, active layout-set digest,
  run/application opaque fingerprints, typed reason, first/last observed time,
  occurrence count, and resolution manifest digest;
- append-only observation history; resolution never deletes the row.

`jobs_ats_certification_circuit_events`

- event ID primary key;
- scope, subject key, transition `opened|held|closed`, typed trigger, window,
  counters, authority reference, and event time;
- current state is derived from ordered events. Only a reviewed close event or
  a newer signed activation may restore authority.

`jobs_ats_certification_canary_reservations`

- reservation ID primary key;
- activation, manifest, target, runner, account/application/run/attempt opaque
  bindings, period key, status, fence, reserved/consumed/released times;
- uniqueness prevents concurrent runs from exceeding the signed rollout cap.

`jobs_application_ats_certification_bindings`

- binding ID primary key;
- account, application, run, attempt, browser session/profile, packet checksum,
  Auto authorization ID/revision/fingerprint, provider target, manifest,
  activation, layout set, adapter bundle, runner target, release/image/browser
  digests, nonce hash, expiry, phase, fence, and timestamps;
- one active preflight binding per application attempt;
- terminal bindings remain immutable recovery evidence.

Raw nonce/capability values are never stored. Lookup hashes and authenticated
encrypted payloads follow the existing local/cloud capability boundaries.

## Exact Target Matching

The server derives the target from the current canonical job URL, canonical
discovery source, and fresh original-source evidence. The customer and runner
may present evidence, but neither chooses the target key.

| Provider | Exact target authority | Current submission state |
| --- | --- | --- |
| Greenhouse | normalized board token, official host family, exact job ID, and `public` or `embedded` variant | Provider state machine; candidate for future signed certification |
| Lever | normalized site, exact regional host, exact posting UUID, and hosted-form variant | Provider state machine; candidate for future signed certification |
| Workday | validated `tenant~instance~site`, exact tenant host, requisition ID, and page-flow variant | Review-fill only; shadow evidence only |
| Ashby | normalized board name, official host, exact posting ID, and page variant | Review-fill only; shadow evidence only |
| SmartRecruiters | normalized company identifier, official host, exact posting ID, and page variant | Review-fill only; shadow evidence only |
| Semantic/unknown | no certifiable target | Review or Takeover only |
| LinkedIn/Indeed/protected portals | explicit policy handoff | Handoff only |

No arbitrary regular expression, caller hostname, source-name suffix, redirect,
subdomain lookalike, tenant wildcard, or layout similarity grants authority.
Provider redirects must remain inside the manifest's exact job identity and
allowed host set. A target/layout mismatch is quarantined before any
irreversible marker.

## Lifecycle

### 1. Shadow collection

Run synthetic fixtures and authorized PII-free observation tooling. Import
signed `LayoutObservationV1` records as `synthetic`, `authorized_sandbox`, or
`authorized_live`. Synthetic observations exercise matching, drift, and
privacy validators but cannot enter a production manifest.

### 2. Candidate manifest

Assemble one `AtsCertificationManifestV1` for one provider target and one exact
runner target. Every required check ID and immutable evidence object must be
present. The API verifies canonical bytes, threshold signatures, evidence
class, suite completeness, object metadata, layout set, build bindings, and
validity without executing candidate code or contacting the provider.

### 3. Independent activation

Import a separately signed `AtsCertificationActivationV1`. Read current target
status, then apply it with the exact expected head revision and transition
digest. Keep Browser distribution flags off during rehearsal. Activation makes
the target certifiable; it does not make an unavailable runner available.

### 4. Canary rollout

When external launch authority exists, assign only the approved canary accounts
and enforce the signed activation-wide total/distinct-account limits, live
concurrency limit, and one server-owned UTC daily side-effect limit.
Every irreversible run reserves exact canary capacity transactionally. Canary
failure, evidence failure, unexpected layout, false confirmation risk, or
side-effect uncertainty can open the target circuit immediately.

### 5. Renewal and drift

Certification expires; it does not renew from recent success alone. A changed
layout, adapter bundle, Browser release, cloud image, Chromium build, suite, or
target variant requires a new observation set, manifest, and activation.
Unchanged canonical replay returns the same digest. Conflicting replay fails.

### 6. Incident response

Open the circuit first, then import the signed revocation when appropriate.
New preflights and pre-click transitions fail closed immediately. Preserve all
in-flight recovery, evidence, receipts, and quarantine records. Restoration of
provider, target, adapter, and runtime circuits requires an explicitly audited
`reviewed_close`. Only an activation-scoped circuit whose subject is the exact
predecessor activation may close through `newer_activation`, and only while
applying that exact successor. Its complete signed head, runtime, and canary
authority must remain current, non-revoked, non-quarantined, and unexpired after
database serialization.

## Two-Phase Execution And Recovery

### Phase A - Preflight and single-use binding

Before a certified Auto run may become employer-facing, the server atomically:

1. re-evaluates the canonical job, original-source evidence, hard filters,
   confirmed facts, Track Auto authorization, identity, source resume, packet,
   allowance, attempt, runner distribution, and exact release/image binding;
2. derives the provider target and resolves one current activation head;
3. validates the manifest, layouts, adapter bundle, runner target, expiry,
   revocations, quarantine state, and circuit state;
4. freezes every authority digest into one expiring, single-use application
   certification binding;
5. binds the local capabilities or cloud lease/checkpoint to that binding and
   its random nonce hash; and
6. returns no general certification credential to the portal or page.

Any mismatch leaves the application in Review, Takeover, or a typed blocked
state. It never silently downgrades an Auto run into an employer-facing click.

### Phase B - Atomic consume immediately before Submit

Immediately before the durable irreversible marker, the runner submits its
single-use binding capability plus the exact provider proof and current
PII-free layout digest. In one database transaction the server:

1. locks the application, attempt, execution lease/local ticket, binding,
   activation head, circuit, and canary-capacity rows;
2. verifies the binding is unexpired, unused, and exact to the run;
3. repeats current authority, revocation, target, adapter, layout, runner,
   Track, identity, packet, claim, document, discovery, and eligibility checks;
4. rejects and quarantines any new layout digest;
5. reserves signed canary capacity;
6. consumes the one-use binding and advances its fence exactly once; and
7. returns the operation-scoped authorization that permits the runner to write
   its durable irreversible marker and activate exactly one submit control.

Only a successful Phase B response permits the marker or click. A bounded HTTP
4xx authorization denial is an explicit pre-marker denial and writes neither.
HTTP 5xx, transport loss, timeout, malformed success, or any response that may
conceal a committed Phase B transaction writes no local marker or click, becomes
terminal `side_effect_unknown`, and is never retried.

### Recovery after the irreversible marker

After the marker, current certification is no longer consulted as permission
to report what already may have happened. The exact frozen binding may only:

- upload or replay the bound trusted result and evidence;
- complete the immutable submitted receipt when provider evidence is valid;
- resume a recovery-only capability that cannot reach Submit; or
- enter the existing owner-confirmed-not-submitted reconciliation path.

An expiry, circuit opening, head replacement, or revocation after the marker
must not reject the exact trusted receipt or force a retry. It blocks all new
irreversible actions. Submitted state remains irreversible, and owner
reconciliation cannot overwrite a trusted submitted receipt.

## Provider Certification And Evidence Matrix

Every stable check ID is versioned by the suite manifest. The result is stored
per provider target, adapter version, and runner target rather than blended
across providers.

| Check ID | Required evidence | Synthetic | Authorized sandbox | Authorized live canary |
| --- | --- | ---: | ---: | ---: |
| `ATS-TARGET-001` | Exact provider, tenant, host, job, redirect, and closed-job detection | Yes | Yes | Yes |
| `ATS-LAYOUT-001` | Signed PII-free layout observation and exact allowed-layout match | Yes | Yes | Yes |
| `ATS-AUTH-001` | Signed-in, signed-out, expired-session, and login/takeover behavior | Yes | Yes | Yes |
| `ATS-FIELDS-001` | Known text/select/radio/checkbox values accepted and read back | Yes | Yes | Yes |
| `ATS-DYNAMIC-001` | Custom, conditional, multi-select, legal, consent, and EEO controls | Yes | Yes | Yes |
| `ATS-DOCS-001` | Exact content-addressed PDF upload, filename, bytes, and read-back | Yes | Yes | Yes |
| `ATS-CHALLENGE-001` | CAPTCHA, 2FA, and assessment pause with no bypass | Yes | Yes | Yes |
| `ATS-SUBMIT-001` | Exactly one provider-scoped submit control and exact request proof | Yes | Yes | Yes |
| `ATS-CONFIRM-001` | Explicit provider-bound positive confirmation | Yes | Yes | Yes |
| `ATS-NEGATIVE-001` | Duplicate, validation, returned form, ambiguous copy, and negative language veto | Yes | Yes | Yes |
| `ATS-FAULT-001` | Crash/response loss before and after activation, with zero duplicate click | Yes | Yes | Yes |
| `ATS-REPLAY-001` | Duplicate workflow delivery, two-runner race, and exact receipt replay | Yes | Yes | Yes |
| `ATS-DRIFT-001` | Unknown layout quarantine, circuit open, expiry, and revocation | Yes | Yes | Yes |
| `ATS-PRIVACY-001` | No candidate values, paths, tokens, cookies, OTPs, or page bodies in evidence/logs | Yes | Yes | Yes |
| `ATS-RECEIPT-001` | Complete typed receipt with exact certification, documents, evidence, and metering | Yes | Yes | Yes |
| `ATS-RUNNER-001` | Exact local release or cloud image/browser binding | Yes | Yes | Yes |

Production activation requires every check in the policy-defined suite, not
only a successful happy-path submission. Zero-tolerance counters are:

- hard-filter violations;
- unsupported factual claims;
- duplicate employer submit activations;
- false Submitted states;
- incomplete or mismatched receipts; and
- PII-bearing certification observations or telemetry.

Provider status for this round:

| Provider | Source implementation | Certification framework result |
| --- | --- | --- |
| Greenhouse | Exact provider state machine and exact-submit proof | Eligible for signed shadow/sandbox manifests; remains uncertified without external live evidence |
| Lever | Exact provider state machine and exact-submit proof | Eligible for signed shadow/sandbox manifests; remains uncertified without external live evidence |
| Workday | Generic review-fill | Cannot activate Auto; collect shadow observations only |
| Ashby | Generic review-fill | Cannot activate Auto; collect shadow observations only |
| SmartRecruiters | Generic review-fill | Cannot activate Auto; collect shadow observations only |
| Semantic/unknown/protected | Review, Takeover, or Handoff | Never certifiable through this framework |

## API Contract

All administrative writes require the existing administrator authentication,
strict JSON with unknown fields rejected, bounded request sizes, audit events,
and generic not-found behavior where appropriate.

| Route | Request authority | Purpose |
| --- | --- | --- |
| `POST /admin/jobs/ats-certifications/trust-policies` | canonical policy and root signature set | Import or byte-replay trust policy |
| `POST /admin/jobs/ats-certifications/layout-observations` | canonical observation and observation signature set | Import PII-free signed structural observation |
| `POST /admin/jobs/ats-certifications/manifests` | aggregate of an independently signed canonical manifest and independently signed evidence envelopes | Atomically import a complete certification candidate and immutable evidence metadata |
| `POST /admin/jobs/ats-certifications/activations` | canonical activation and independent signature set | Import promotion authority without moving a head |
| `POST /admin/jobs/ats-certifications/activations/apply` | activation digest, expected head revision, expected transition digest | Compare-and-swap one exact target/runner head |
| `POST /admin/jobs/ats-certifications/canary-allowlists` | bounded account set, validity window, and approval reference | Import or byte-replay the exact server-owned canary allowlist |
| `POST /admin/jobs/ats-certifications/canary-allowlists/revoke` | allowlist digest and revocation reference | Irreversibly stop new authority from the allowlist |
| `POST /admin/jobs/ats-certifications/revocations` | canonical revocation and incident signature set | Append irreversible revocation |
| `POST /admin/jobs/ats-certifications/circuits` | exact scope, transition, trigger, authority reference | Open, hold, or reviewed-close a circuit |
| `GET /admin/jobs/ats-certifications/targets/:target_key/status` | none beyond admin auth | Read signed head, expiry, revocation, quarantine, circuit, and rollout state |

Runner layout observations use a separate private, worker-authenticated route or
existing authenticated runner result channel. Public customers cannot submit
observations, evidence, activations, circuit changes, or certification labels.

Existing queue, local claim, cloud lease, and final-submit authorization routes
must return or accept only their bounded operation-scoped certification binding.
No reusable manifest or signing material is placed in a custom-protocol URL.

## Portal Behavior

The portal consumes only a bounded server summary:

- `capability`;
- provider label;
- adapter version;
- certified runner kinds;
- certification status `active|review_only|expired|suspended|revoked|drifted`;
- last verified and expiry times;
- truthful reason and next action; and
- canary availability without internal counts or account identities.

`Certified` appears only when the exact current job target and selected runner
resolve to an active authority. Review mode remains available and does not
become Auto merely because a provider is certified. A changed layout or open
circuit changes the action to Review/Takeover before a run begins.

The portal must not expose target keys, employer test-tenant identifiers,
evidence-object paths, internal check output, selectors, signature material,
rollout account IDs, or circuit thresholds.

## Operations Contract

The operations runbook must document this order:

1. load the `bluey-ops` preflight and resolve the exact source commit;
2. verify the public root anchor and current trust-policy status;
3. import and inspect authorized PII-free observations;
4. import the complete manifest and immutable evidence metadata;
5. import the independently signed activation;
6. read target status and apply with exact compare-and-swap fields;
7. keep Browser distribution disabled during rehearsal;
8. configure only approved canary accounts, runner, daily cap, concurrency cap,
   and support owner;
9. enable only the separately approved runner distribution path;
10. monitor every canary receipt, unknown outcome, layout, circuit, and capacity
    reservation; and
11. open the circuit and preserve recovery evidence before incident revocation.

Required dashboards are split by provider target, adapter version, manifest,
runner target, and activation:

- preflight allow/deny reasons;
- layout drift and quarantine rate;
- fill/read-back and document failure rate;
- challenge/intervention rate;
- submit activation and confirmation rate;
- `side_effect_unknown` rate and reconciliation age;
- receipt completeness and evidence failure;
- duplicate-prevention conflicts;
- canary reservation use; and
- circuit/revocation state.

An activation is never deleted to roll back. Apply a new higher-sequence signed
activation to an already certified immutable manifest, or append revocation.
Database rollback must not resurrect revoked authority or erase quarantine,
circuit, binding, receipt, or reconciliation history.

## Acceptance Criteria

### AC1 - Canonical signed objects

Trust policy, evidence, manifest, activation, revocation, and layout observation reject
unknown fields, duplicate keys, non-canonical bytes, invalid encodings,
threshold shortfall, wrong role, key reuse, invalid predecessor, future time,
and expiry. Byte-identical replay returns the same digest; conflicting replay
fails.

### AC2 - Independent authority roles

Manifest signatures cannot activate, activation signatures cannot create
evidence, observation signatures cannot promote, and only incident authority
can revoke.

### AC3 - Complete immutable manifest binding

The manifest binds exact provider target, adapter version and bundle, layout
set, suite, source commit, evidence class, and local release or cloud
image/browser identity. Changing one byte requires a new manifest.

### AC4 - Synthetic evidence is shadow-only

Synthetic fixtures and observations pass validator tests but cannot satisfy a
production activation or make eligibility `certified`.

### AC5 - PII-free observations

Observation validation rejects values, answers, page bodies, HTML, screenshots,
documents, cookies, tokens, OTPs, credential-bearing URLs, or unbounded labels.

### AC6 - Exact target derivation

The server derives the provider target from canonical URL, discovery source,
and fresh original-source evidence. Spoofed hosts, tenant mismatches, redirects,
wildcards, source suffixes, and client capability fields cannot match.

### AC7 - Provider boundary

Only exact Greenhouse and Lever provider state machines are structurally
eligible for a certification manifest in this batch. Generic Workday, Ashby,
SmartRecruiters, semantic, and protected-portal paths cannot activate Auto.

### AC8 - Monotonic lifecycle

Manifest and activation sequences, predecessors, head revisions, and transition
digests are enforced transactionally in both database dialects. Stale
compare-and-swap and concurrent head movement fail without partial mutation.

### AC9 - Expiry and revocation

Expired, suspended, or revoked policy, key, manifest, activation, layout,
adapter, target, runner release, or image blocks new certified preflight and
Phase B authorization.

### AC10 - Append-only quarantine

An unknown current layout cannot use similarity or prior success to proceed. It
creates bounded append-only quarantine evidence and falls back before the
irreversible marker.

### AC11 - Circuit breakers

Typed layout drift, evidence failure, confirmation ambiguity, false-state risk,
or configured error threshold can open a provider/target/adapter/runner circuit
immediately. A client or runner cannot close it. Provider, target, adapter, and
runtime circuits require `reviewed_close`; only an activation-scoped circuit for
the exact predecessor activation may close through `newer_activation` while its
exact successor is applied and remains fully current after serialization.

### AC12 - One certification resolver

Matches, preparation, queueing, local claim, cloud lease, runner start, and
pre-click authorization use the same server-owned resolution semantics.

### AC13 - Frozen application binding

The exact manifest, activation, layout set, adapter bundle, runner target,
packet checksum, identity, Track Auto authorization, attempt, and job target are
covered by one immutable application certification binding.

### AC14 - Exact runner identity

A local artifact, platform, architecture, Browser manifest, Chromium binary, or
cloud image/build mismatch prevents certified execution. Local certification
does not authorize cloud execution and vice versa.

### AC15 - Single-use Phase A

One application attempt receives at most one live expiring preflight binding.
Nonce replay, cross-run use, cross-account use, stale packet use, and two-runner
claim races fail closed.

### AC16 - Atomic Phase B

Binding consume, complete authority recheck, layout match, canary reservation,
and fence advance commit atomically before the irreversible marker can be
written.

### AC17 - No marker on denial

A bounded HTTP 4xx authorization, validation, expiry, revocation, circuit,
capacity, or compare-and-swap denial is an explicit pre-marker rejection and
produces zero durable marker writes and zero submit-control activations.

### AC18 - No blind Phase B retry

HTTP 5xx, transport loss, timeout, malformed success, process loss, or any other
response that may conceal a committed Phase B transaction becomes terminal
`side_effect_unknown`. No runner, workflow replay, resume request, or new
certification binding may activate Submit again automatically.

### AC19 - Revocation-safe recovery

Revocation before the marker blocks Submit. Revocation after the marker blocks
new action but accepts the exact frozen trusted receipt/result and recovery-only
capabilities.

### AC20 - Review behavior remains distinct

`beta_review` retains packet review and provider final approval. Certified
Review mode still requires packet approval. Only a current `track_auto_submit`
admission plus exact active certification may omit per-application final review.

### AC21 - Intervention changes require reapproval

An employer-facing answer or document change invalidates the packet and
certification binding and returns to review. A run at or beyond the marker stays
in reconciliation.

### AC22 - Exact receipt authority

A submitted receipt records and validates manifest, activation, layout,
adapter, runner, binding, target, request, documents, screenshots, confirmation,
attempt, and metering evidence. Missing or mismatched certification evidence is
not Submitted.

### AC23 - Recovery does not widen authority

Result/resume recovery cannot mint a new preflight, change the target or packet,
write another marker, or consume another canary reservation.

### AC24 - Admin and worker isolation

Only administrators import/apply signed authority and transition circuits. Only
authenticated workers submit observations through the private bounded path.
Customer endpoints cannot mutate certification state.

### AC25 - Truthful portal summary

The portal shows server-authored status, exact runner scope, verification time,
expiry, reason, and next path. Missing new fields fail closed during mixed-version
rollout.

### AC26 - Dialect and multi-process parity

SQLite and PostgreSQL have equivalent tables, constraints, indexes, replay,
locking, activation, revocation, quarantine, circuit, reservation, binding, and
recovery behavior.

### AC27 - Complete evidence suite

Every policy-required stable check ID is present for the exact provider and
runner target. Happy-path evidence alone cannot activate.

### AC28 - Zero-tolerance safety results

The certification evidence reports zero hard-filter violations, unsupported
claims, duplicate submits, false Submitted states, incomplete receipts, and
PII-bearing observations.

### AC29 - Protected flags remain disabled

Source implementation and rehearsal do not change:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_MAILBOX_SYNC_ENABLED=0
```

### AC30 - No launch claim from source evidence

Passing local source gates may describe the certification framework as
source-complete and locally verified. It may not describe any ATS, provider,
tenant, target, adapter, runner, or unattended submission path as certified,
enabled, launched, or distributed.

## Verification Matrix

The results below record the temporary-index snapshot of the complete working
tree based on `427e3e3d`. The actual Git index remained empty. A narrow test is
evidence only for the boundary it exercises. Privacy and diff hygiene passed
again after the documentation closure.

| Gate | Required command or evidence | Result |
| --- | --- | --- |
| Canonical authority vectors | Focused Node and Rust parity tests for policy, manifest, activation, revocation, and observation bytes | Passed as part of the focused ATS authority run: 58/58 tests |
| Certification database unit tests | Focused SQLite lifecycle, replay, target, circuit, binding, and recovery tests | Passed as part of the focused ATS authority run: 58/58 tests |
| PostgreSQL authority tests | Real PostgreSQL transaction, advisory-lock, concurrent activation, preflight, consume, and recovery tests | Compiled; runtime cases self-skipped without `BLUEY_TEST_POSTGRES_URL`; live PostgreSQL execution remains parked |
| Automation tests | `npm test --workspace @bluey/jobs-automation` | Passed: 644 tests in 35 files |
| Browser tests | `npm test --workspace @bluey/jobs-browser` | Passed: 219 tests in 34 files |
| Runner tests | `npm test --workspace @bluey/jobs-runner` | Passed: 269 tests in 32 files |
| Workflow tests | `npm test --workspace @bluey/jobs-workflows` | Passed: 76 tests in 7 files |
| Portal tests | `npm test --workspace @bluey/jobs-portal` | Passed: 194 tests in 16 files |
| Full Jobs suite | `npm test` from `jobs/` | Passed: 1,402 tests in 124 files |
| TypeScript | Typecheck all five Jobs workspaces | Passed: all five workspace typechecks |
| Production builds | Build all five Jobs workspaces | Passed: all five workspace builds; portal transformed 2,290 modules and emitted 27 files with no source maps; aggregate SHA-256 `00dc3c66468e894ca96c1217d8e9a8edde22b5ce4bb1c5ca843dcc6c1ac493e0`; non-failing 502.51 kB advisory |
| Server tests | `cargo test --manifest-path server/Cargo.toml` | Passed: 1,106 library plus 107 other/integration tests, 1,213 total; focused ATS authority 58/58 |
| Server compile | `cargo check --manifest-path server/Cargo.toml --tests` | Passed |
| Strict Clippy | `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings` | Passed with warnings denied |
| Rust formatting | `cargo fmt --manifest-path server/Cargo.toml --all -- --check` | Passed |
| Schema parity | `node jobs/scripts/check-jobs-schema-parity.mjs` | Passed: 65 tables and 57 indexes |
| Privacy | `node jobs/scripts/privacy-gate.mjs` on the temporary-index snapshot | Passed: 2,456 paths and 2,183 text files, including the post-documentation confirmation |
| Provenance/license | `node jobs/scripts/check-provenance-licenses.mjs` | Passed: 663 dependency entries, 631 versions, 1 override, and 14 commit-pinned repositories |
| Client/server boundary | `node scripts/check-bluey-jobs-client-boundary.mjs` | Passed |
| Workflow/CI guards | Existing workflow validators and CI guard self-tests | Passed: CI guards; Browser release gate 9/9; workflow contract |
| Diff hygiene | `git diff --check` and temporary-index diff review | Passed for the final temporary-index snapshot after documentation closure |
| Synthetic shadow matrix | All stable check IDs on Greenhouse/Lever fixtures, with zero activation authority | Passed: complete stable-check fixtures remained shadow-only with zero activation authority |
| Authorized sandbox matrix | Exact target and runner evidence | External authorization required |
| Authorized live matrix | Two or three approved vacancies per enabled provider, independent canary evidence | External authorization required |
| Physical local runner matrix | Exact signed macOS/Windows release and clean-device canaries | External credentials/devices required |
| Cloud runner matrix | Exact image, Browser, Temporal, PostgreSQL, R2/S3, crash, and capacity canaries | External infrastructure required |

## Required Documentation Artifacts

- `docs/rounds/ROUND-604-JOBS-ATS-CERTIFICATION-AUTHORITY.md` - this
  authoritative plan and final outcome record;
- `docs/work/IMPL-PHASE-604-JOBS-ATS-CERTIFICATION-AUTHORITY.md` - exact files,
  implementation decisions, verification results, deviations, and parked gates;
- `docs/work/REVIEW-PHASE-604-JOBS-ATS-CERTIFICATION-AUTHORITY.md` - line-by-line
  self-review and verdict;
- one `docs/work/FIX-<number>-<slug>.md` per defect fixed during implementation;
- `CHANGELOG.md` under `Unreleased` with source-readiness wording only; and
- `jobs/OPERATIONS.md` with the signed import, activation, canary, circuit,
  revocation, recovery, and rollback ceremony.

Do not edit the frozen `docs/reviews/` directory.

## External Production Boundary

The following can be completed from source without external authority:

- paired schema and database authority;
- strict signed-object and PII-free observation validators;
- shadow evidence manifests;
- server certification resolution and lifecycle APIs;
- exact Application Kit, local/cloud, pre-click, receipt, and recovery binding;
- layout quarantine and circuit breakers;
- portal truth and operations ceremony;
- synthetic/adversarial tests and full local gates.

The following remain parked until separately authorized or available:

- independently managed signing keys and approved public root anchor;
- authorized Greenhouse and Lever sandbox/live tenants and two or three test
  vacancies per provider;
- owner-approved canary accounts, plans, daily limit, concurrency cap, and named
  support/incident owner;
- credentialed native packaging, immutable public artifact read-back, and
  physical macOS arm64/x64 and Windows x64 canaries;
- authenticated Temporal and isolated cloud Browser pool;
- live PostgreSQL migration/concurrency and failover tests;
- real R2/S3 evidence fault, retention, deletion, and read-back tests;
- production network egress, takeover, monitoring, and regional recovery;
- provider/data-rights approvals where required; and
- an explicit production change authorizing any distribution flag.

No source change, fixture, synthetic observation, or document can manufacture
those approvals. Until the exact external matrix passes, Greenhouse and Lever
remain `Beta - Review first`; Workday, Ashby, SmartRecruiters, semantic forms,
and protected portals remain Review, Takeover, or Handoff according to current
server policy.
