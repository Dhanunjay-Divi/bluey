# Round 614B — Jobs Signed Job Integrity Authority

**Date:** 2026-08-30

**Branch:** `feat/phase-614b-jobs-signed-integrity-authority`

**Status:** Source implementation complete through FIX-766; observed local Rust, JavaScript,
native, release-containment, and PostgreSQL evidence green; independent source/security rereview
green; final documentation rereview green; external release evidence pending (🟡)

**Frozen non-doc source manifest:** 44 files at Git `HEAD`
`be925f89ccc5af2fb4b2ea41ba123c571fefadd6`; canonical SHA-256 of sorted per-file SHA-256 lines
`fd49f94a6c8e22c373a848f74956abb0af57b62c090d8b4f40c15856b46359aa`.

> **Codex preflight:** Load `$bluey-ops` before implementation or review and reconcile its operating
> memory against this branch, the final Phase 614 authority record, and the current source.

## Outcome

Phase 614B implemented an account-independent, independently signed job-integrity authority that
proves the employer identity and job-risk decision required by production-representative positive
Jobs routes. It composes with the exact current Phase 614 original-source receipt and does not infer
employer identity or scam clearance from provider presence, a hosted ATS domain, a mutable posting
projection, or a client assertion. The correction ledger now runs through FIX-766, including
managed/unmanaged FinalSubmit pairing, immutable historical replay metadata, migration-058
interruption recovery, a load-stable verifier clock margin, and feature-contained production-
positive integration fixtures.

This remains a source-authority batch, not a deployment batch. Current focused and aggregate Rust,
JavaScript, native-runner, release-containment, and fresh PostgreSQL 17.10 19-case evidence is
green. Independent source/security and final documentation rereviews found no remaining P0–P2.
Docker/hosted/production evidence is absent, and the PR chain is not merge-ready. Source verification, Browser
distribution, workflow dispatch, model generation, mailbox access, communications, provider
writes, and every production flag remain disabled.

## Predecessor Authority

Phase 614 established immutable original-source verification, exact provider/destination binding,
and fail-closed rechecks at preparation, queue, claim, and effect boundaries. It deliberately did
not mint independent employer-identity or scam-risk authority. Consequently:

1. a hosted-ATS snapshot or current original-source receipt supports source truth but remains
   Review-first without separate employer and risk authority;
2. the shared positive integration fixture at that checkpoint correctly stopped before Auto-submit
   because sponsorship and independent integrity authority were absent;
3. reservation and running-status transitions at that checkpoint rechecked source/discovery under
   PostgreSQL order `H -> M -> D`, but did not independently re-resolve ATS plus job integrity;
   later claim and effect gates prevented an external effect, while stale capacity could still be
   consumed; and
4. Phase 614 preserves the provider application domain as source evidence. It is not the canonical
   employer corporate domain.

Phase 614B closed those source-mechanism gaps and retained explicitly bounded evidence gaps in the
FIX ledger. Round 615 remains reserved for the Source Control Plane and Freshness SLOs: source
enrollment and rights, cadence, budgets, operated scheduling, dead letters, parser/volume/freshness
SLOs, retention, source labels, canaries, and direct/global discovery release binding.

## Scope

### In Scope

- one strict, account-independent `JobIntegrityAttestationV1` envelope;
- two separate `JobIntegrityAuthorizationV1` authorization blobs over the same canonical
  attestation bytes, signed by disjoint delegated roles `employer_identity` and `job_risk`;
- a separately delegated `revocation` role and monotonic signed trust-policy chain rooted in an
  offline trust anchor;
- strict canonical bytes, size, identifier, digest, domain, array, integer, and signature rules;
- paired SQLite/PostgreSQL immutable attestation, revocation, transition, and current-head
  authority using exactly seven integrity tables per dialect;
- exact replay, changed-identity conflict, fork/gap rejection, compare-and-swap head transition,
  policy/key/attestation/evidence expiry, and no-fallback semantics;
- one resolver that composes the exact current Phase 614 source subject/material/provider/
  destination with current policy, head, signatures, revocations, employer identity, and risk;
- a frozen application `job_integrity` receipt binding the exact authority used;
- current-authority comparison at preparation, approval, queue, reservation, running, claim,
  dispatch, and pre-Submit boundaries;
- PostgreSQL admission ordering that acquires `H -> M -> ATS -> D`, then integrity control and the
  exact integrity head with shared row locks before effect-capable mutation; and
- atomic prepared-application finalization that cannot commit `queued` before its exact
  `approved_execution` receipt is frozen;
- production local-browser claim parity under the same complete prelock and integrity snapshot;
- a required, nonoptional managed-effect authority at the database boundary; and
- production-representative positive fixtures that use public authority import and preference
  paths, a real signed Phase 614 v2 source fixture, confirmed sponsorship, and dual-signed Phase
  614B integrity authority.

### Out of Scope

- Round 615 source operations, enrollment, rights, scheduling, canaries, and SLOs;
- a crawler, evidence-generation service, employer-domain discovery service, or risk-scoring
  pipeline;
- authenticated or private competitor access, scraping, UI/code copying, private-session analysis,
  or reconstruction of a competitor's proprietary implementation;
- ATS adapter changes, a new provider family, provider credentials, login, OAuth, cookies, CAPTCHA
  handling, anti-bot behavior, or employer/provider writes;
- portal UI, MCP, C2C contracts/chat, inbox, calendar, outreach, resume generation, or application
  workflow product expansion;
- a new operational-hold capability, activation table, application-binding table, or standalone
  signature table;
- production keys, deployment, flags, customer cohorts, or production-readiness claims;
- any inference that public customer-facing patterns from Tsenta, Giraffy, or similar services are
  integrity evidence. Those clean-room observations may inform later product-roadmap work only;
- Phase 620 business-messaging/MCP/inbox implementation, provider egress, personal WhatsApp, or
  unattended personal iMessage. The separate simulator-first and omnichannel plans are
  design-only and not part of this source manifest or acceptance.

## Implemented Defect And Evidence Ledger

The reviewed correction ledger is FIX-725 through FIX-766. It establishes source implementation,
not customer effect or release authority.

- **FIX-725–730:** reservation/running now revalidates ATS and integrity before capacity mutation;
  provider/application/corporate domains remain separate; prepared approval and queue are atomic
  under the full prelock; the production local-browser claim owns the canonical prelock and keeps
  `RunnerClaim` holds; the explicit managed API requires its tuple; and stored/customer authority
  projections preserve hard denials. Mapped reservation, ATS replacement, SmartRecruiters,
  composition, hold, queue-atomicity, PostgreSQL prelock, claim, profile (5/5), and workspace (4/4)
  regressions exist. FIX-729 is intentionally historical: FIX-762 later owns shared FinalSubmit
  managed/unmanaged classification.
- **FIX-731–738:** strict Ed25519 Rust/Node vectors, root/delegated role history, post-lock database
  time, blocking-safe public APIs, persisted role-scoped authorization-ID collision checks,
  negative-head/revocation evaluation, canonical sorted/URL/JSON bytes, and strict receipt
  projection are implemented. FIX-733 retains bounded contention gaps; FIX-734 lacks one live
  public-wrapper trust-policy case; FIX-735 retains a configured-PostgreSQL collision gap.
- **FIX-739–743:** paired schema, exact signed destination composition, managed-effect current
  integrity, workflow request-start, and workflow resume/order are implemented. The posture stays
  yellow: FIX-739 requires the final durable exact PostgreSQL catalog attachment; FIX-740 lacks one
  direct destination-drift FinalSubmit zero-mutation case; FIX-741 lacks the full behavioral drift
  matrix; FIX-742 lacks the revocation/expiry/destination request-start matrix; and FIX-743 lacks
  the stale-authority resume zero-mutation case.
- **FIX-744–752:** posting-refresh denial preservation, workspace/match/export representation,
  PostgreSQL lockable snapshots, SmartRecruiters cross-host binding, single post-fence database
  time, encrypted-posting source recheck, and execution-integrity/account-policy order are
  implemented. Their exact evidence posture remains governed by each FIX record; source completion
  does not erase a focused or configured-PostgreSQL rerun still marked pending there.
- **FIX-753–762:** the canonical global lock order and post-lock clock, managed-v2 signature
  audience, production-positive fixtures, database-clock FinalSubmit, composed Auto-submit
  eligibility, frozen ATS head, typed hold denials, signed Browser fixture, PostgreSQL 17 evidence,
  schema-v1 submitted-receipt authentication, and durable managed/unmanaged pairing are
  implemented. The final certified sweep passed 15/15, the final formerly failing set passed
  16/16, and the final PostgreSQL exact-name manifest passed 19/19. Independent source/security
  rereview found no remaining P0–P2; final documentation rereview is also green.
- **FIX-763:** exact replay now recovers the original immutable transition digest and revision from
  transition history after later head advancement. Direct SQLite and configured PostgreSQL 17.10
  lifecycle tests each passed 1/1.
- **FIX-764:** migration 058 now rolls back an interrupted migration, restores/verifies the incoming
  SQLite foreign-key setting, and runs `foreign_key_check`. Injected interruption coverage passed
  1/1.
- **FIX-765:** the long-horizon verifier fixture now provides explicit setup margin while retaining
  the unchanged production grant bound. Focused consumers and the final 1,586-test aggregate pass.
- **FIX-766:** the production-positive fixture is isolated behind an explicit test-support feature
  that is absent from ordinary/release artifacts. The explicit integration target passed 108/108;
  checks, strict Clippy, release containment, dependency/artifact scans, and CI guards are green.

No P0 was recorded. The remaining yellow items are explicit evidence or behavioral-matrix limits,
not permission to infer a green release verdict.

## Trust Model

The offline root signs only a monotonic delegated-policy chain. It does not sign job attestations.
Runtime authority begins from the configured
`BLUEY_JOBS_JOB_INTEGRITY_ROOT_TRUST_ANCHOR_JSON` and resolves a current policy plus current,
nonrevoked delegated keys.

One canonical attestation must be jointly authorized by two disjoint roles:

```text
employer_identity -> independently verifies the canonical employer identity/domain
job_risk           -> independently evaluates job-risk inputs under an exact signed policy
revocation         -> independently revokes delegated keys, policies, or attestations
```

The same key or delegated role assignment cannot satisfy both positive authorizations. A source
verifier, ATS adapter, discovery feed, browser runner, client, account, employer-facing workflow,
or mutable posting record cannot self-mint these roles. The attestation contains no account ID,
candidate identity, PII, Career Track policy, or Auto-submit decision.

## Canonical Authority Envelopes

### `JobIntegrityAttestationV1`

The canonical attestation uses version and audience
`bluey-jobs-job-integrity-attestation-v1` and binds:

```text
attestationId
policySha256
subjectSha256
sourceMaterialSha256
attestationGeneration
predecessorAttestationSha256
canonicalJobId

source {
  providerFamily
  providerRecordId
  target { host, tenant, job, variant }
  canonicalApplicationUrl
  applicationDomain
  atsTenantBindingSha256
}

employer {
  status
  canonicalEmployerId
  canonicalEmployerDomain
  verificationMethods
  evidence[]
}

risk {
  status
  signalCodes
  policySha256
  inputSha256
  engineReleaseSha256
  evidence[]
}

assessedAtMs
issuedAtMs
notBeforeMs
expiresAtMs
```

`applicationDomain` is the source-side application host. `canonicalEmployerDomain` is an
independently verified corporate domain. They are distinct facts even when their normalized values
happen to match.

### `JobIntegrityAuthorizationV1`

The employer and risk authorizations are separate canonical blobs. Each binds its exact role,
current policy, target attestation audience and SHA-256 digest, authorization ID, signed-at time,
and signature set. The two blobs authorize the same exact attestation bytes; neither may authorize
a projection, mutable JSON object, prior attestation, or alternate canonical encoding.

The public import packages are:

```text
JobIntegrityTrustPolicyPackageV1
JobIntegrityAttestationPackageV1
JobIntegrityRevocationPackageV1
```

No import becomes authority merely because it parses or carries a valid Ed25519 signature. Current
policy, role separation, subject identity, predecessor/head state, time bounds, revocation state,
and the complete composed source binding must also pass.

## Canonicalization Contract

Canonical bytes must:

- reject unknown fields rather than ignore them;
- end with one terminal newline;
- contain no floating-point numbers or unsafe integers;
- use lowercase 64-character SHA-256 hexadecimal values;
- use base64url without padding for binary signature material;
- sort and deduplicate every set-like array under its field-specific ordering;
- use normalized ASCII IDNA domains and exact HTTPS URL/host rules;
- remain at or below 64 KiB; and
- verify with Ed25519 under current delegated keys and threshold/role policy.

Semantically equivalent but byte-different input must normalize before signing or be rejected at
import. Exact authenticated replay is read-only and creates no mutation. Reusing an identity with
changed canonical bytes is a conflict.

## Integrity Semantics

Employer status is closed to:

```text
verified | unverified | mismatch
```

Risk status is closed to:

```text
clear | review_required | blocked
```

Positive `JobIntegrityCurrentAuthority` exists only when employer status is `verified`, risk status
is `clear`, all required evidence is present and current, and `signalCodes` is empty. Employer
`mismatch` requires risk status `blocked` plus its recognized closed mismatch signal.
`review_required` and `blocked` must carry at least one recognized signal. Unknown or malformed
status, evidence, or signal values fail closed.

A signed negative or review-required head remains the current denial until an authorized successor
advances the head. Expiry or revocation of a positive never falls back to an older positive.

## Relational Authority

Each database dialect adds exactly these seven tables:

```text
jobs_job_integrity_trust_policies
jobs_job_integrity_trust_keys
jobs_job_integrity_attestations
jobs_job_integrity_revocations
jobs_job_integrity_head_transitions
jobs_job_integrity_heads
jobs_job_integrity_control
```

Attestation, policy version, key, revocation, and head-transition rows are immutable. Foreign-key
deletion is restrictive. Policy generation, revocation sequence, attestation generation, and head
revision advance monotonically. The control singleton serializes trust-policy, revocation, and head
publication authority without introducing a new operational-hold capability.

There is no activation table, application-binding table, or separate signature table. The exact
authorization blobs and their digests are retained with the immutable attestation authority.

### Publication And Replay

- A first import resolves the exact current policy, disjoint role keys, signatures, time bounds,
  subject/source material, and predecessor/head state before one compare-and-swap publication.
- Exact authenticated replay returns the original immutable transition digest and revision from
  transition history without advancing a sequence, refreshing time, or appending a transition,
  including after a later head advance.
- Reuse of an ID or digest identity with changed bytes conflicts with zero authoritative mutation.
- A missing predecessor, generation gap, fork, stale expected head, or compare-and-swap loss fails
  with zero head mutation.
- Trust-policy and revocation writers acquire the integrity control row exclusively.
- Attestation publication acquires integrity control exclusively, then updates the exact head under
  its compare-and-swap contract. This makes a representation-wide shared control lock freeze both
  existing head advancement and insertion of a previously missing head.
- Database time determines currentness and transition time.

## Resolver And Freshness

The resolver must compare the attestation to the exact current Phase 614:

```text
canonical job and employer subject
source-material SHA-256
provider family and provider record ID
provider target host/tenant/job/variant
canonical application URL and application domain
ATS tenant binding
```

It then resolves the current integrity policy and head, both role authorizations, relevant
revocations, employer identity/domain, risk policy/input/engine, and all evidence bounds. Positive
expiry is the minimum of current source, attestation, policy, delegated-key, and integrity-evidence
expiry. Any mismatch, absence, expiry, revocation, fork, stale head, negative current head, or
unsupported semantic value returns typed `ReviewRequired` or `Blocked` authority, never a prior
positive.

## Application Receipt And Action Boundaries

The frozen application receipt adds `job_integrity` containing at least:

```text
subjectSha256
sourceMaterialSha256
attestationSha256
attestationGeneration
headRevision
headTransitionSha256
policySha256
employerIdentityAuthorizationSha256
jobRiskAuthorizationSha256
canonicalEmployerId
canonicalEmployerDomain
riskPolicySha256
expiresAtMs
```

Preparation, approval, queue, reservation, running, claim, dispatch, and pre-Submit must recompute
and compare the exact current composed authority. A receipt is audit evidence, not self-validating
authority. Any drift denies the transition with zero effect-capable mutation.

Prepared-kit finalization freezes `approved_execution` and transitions to `queued` atomically under
this same authority snapshot. An API follow-up write may not repair a queued orphan after the
database transaction commits.

The production local-browser claim route must own the complete prelock before claim/runner
mutation. Existing `RunnerClaim` hold enforcement remains mandatory. A standalone helper can
receive semantic parity but cannot substitute for route evidence. The explicit managed execution
API requires the complete managed-authority tuple. At the shared FinalSubmit boundary, durable
workflow state proves whether an application is managed: tuple absence is accepted only for a
proven unmanaged application, while managed omission or mismatch fails before mutation.

For PostgreSQL effect-capable readers, the canonical order is:

```text
H -> M -> ATS -> D -> integrity control FOR SHARE -> exact integrity head FOR SHARE
```

Policy, revocation, and attestation publishers take integrity control exclusively. A single-job
effect reader takes it shared before its exact head. A multi-posting representation holds one
shared control publication fence after `H -> M -> ATS -> D`, samples database time once, and then
resolves all exact heads without reacquiring the control or policy locks. SQLite must preserve the
same logical snapshot and zero-mutation semantics in one authoritative transaction.

SQLite migration 058 also preserves host state on failure: it rolls back an open transaction,
restores and verifies the incoming `foreign_keys` value, and executes `foreign_key_check` before
returning the error.

## Positive Fixture Contract

Production-representative positive tests must not use direct SQL or a shared helper that fabricates
positive integrity. They must:

1. save sponsorship as `not_required` through the public candidate-preference path;
2. import a real signed Phase 614 v2 original-source authority through its public path;
3. import the trust policy and dual-signed Phase 614B attestation through the public package path;
4. resolve the resulting current authority through the same public resolver used by production;
5. create approval only after the composed current authority succeeds; and
6. prove revocation, expiry, source drift, head replacement, and sponsorship drift deny later
   boundaries with zero mutation.

Negative tests may construct malformed bytes locally but must not bypass the public verifier or
database transition paths when asserting accepted authority.

## Threat Model

Phase 614B must fail closed against:

- a source/provider presence assertion masquerading as employer verification or scam clearance;
- conflation of a shared hosted-ATS domain with an employer corporate domain;
- a key delegated to both positive roles, wrong-role signatures, insufficient threshold, stale
  policy, revoked key, revoked attestation, or forged root/delegation bytes;
- unknown fields, alternate JSON encodings, duplicate/set-order ambiguity, unsafe numbers,
  overlarge envelopes, Unicode/IDNA domain confusion, and signature malleability;
- cross-job, cross-employer, cross-provider, cross-tenant, cross-destination, or cross-source-material
  replay;
- exact-ID replay with changed bytes, predecessor gaps, forks, stale compare-and-swap heads, or
  rollback to an older positive;
- caller-supplied clocks, expiry extension, stale evidence, or a signed negative being ignored in
  favor of historical positive evidence;
- revocation or authority drift between preparation, approval, reservation, claim, dispatch, and
  pre-Submit;
- stale reservations consuming runner capacity after ATS or integrity authority changes;
- prepared-kit finalization committing a queued row without an atomic approval receipt;
- outer/inner local-claim helpers composing `H -> ATS -> D -> M` or reacquiring authority after
  account/application locks;
- an internal managed-effect caller using optional/absent authority to skip admission;
- mutable posting JSON widening operational-hold employer scope before signed-domain resolution;
- account/PII/Auto-submit fields entering an account-independent attestation; and
- logs, errors, or receipts disclosing unbounded evidence, private candidate data, secret key
  material, or sensitive source content.

## Acceptance Matrix

| Boundary                 | Required Phase 614B behavior                                                                                             | Evidence |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------ | -------- |
| Root/delegation          | Offline root authorizes only a monotonic delegated-policy chain                                                          | Implemented; mapped Rust coverage |
| Role separation          | Disjoint current `employer_identity` and `job_risk` authorizations cover the same canonical attestation                  | Implemented; strict Rust/Node vectors green in observed gates |
| Revocation               | Independent `revocation` role; monotonic revocation; no fallback to old positive                                         | Implemented; some boundary matrices remain yellow |
| Canonical bytes          | Strict v1 schema, newline, integer, digest, base64url, set, domain, size, and Ed25519 rules                              | Implemented; vector/guard evidence green |
| Employer identity        | Canonical employer ID/domain comes from signed independent evidence, never ATS-provider presence                         | Implemented; SmartRecruiters/composition/hold mappings present |
| Risk                     | `clear` requires complete evidence and zero signals; review/blocked/mismatch remain typed denials                        | Implemented; focused evidence green to date |
| Storage                  | Exactly seven parity tables; immutable rows; restrictive deletion; monotonic policy/revocation/head                      | Implemented; 102-table/86-index parity green; final catalog attachment pending |
| Replay/head              | Exact replay is read-only and returns historical transition metadata; changed identity, gap, fork, and stale CAS cause zero mutation | Implemented; SQLite and configured PG lifecycle 1/1 each |
| Resolver                 | Exact current Phase 614 subject/material/provider/destination is composed with policy, signatures, head, and revocations | Implemented; focused/certified evidence green |
| Freshness                | Database time after locks; positive expiry is the minimum of every source/integrity authority bound                      | Implemented; bounded contention matrix remains yellow |
| Application receipt      | Frozen `job_integrity` binding is complete and compared at every later boundary                                          | Implemented; schema-v1 focused evidence green |
| Reservation/running      | ATS plus integrity rechecked before capacity mutation; stale authority cannot consume a new slot                         | Implemented; mapped regressions and PG manifest green |
| Application finalization | Complete prelock; atomic approval-plus-queue                                                                             | Implemented; mapped queue/PG prelock coverage |
| Local Browser claim      | Actual API/browser-release route prelocks complete authority before mutation; holds and helper parity preserved          | Implemented; final certified sweep green |
| Managed effect           | Explicit managed tuple required; shared FinalSubmit accepts absence only for durable unmanaged state                     | Implemented; focused pairing green; full behavioral PG matrix remains bounded |
| Hold employer scope      | Caller-resolved signed corporate domain after common prelocks; no mutable posting alias or internal relock               | Implemented; typed denial and composition coverage |
| PostgreSQL order         | `H -> M -> ATS -> D -> integrity control -> exact head` for effect-capable reads                                         | Implemented; final-source PG 19/19 green |
| Positive fixtures        | Public sponsorship, Phase 614 v2 source, and dual-signed integrity imports; no direct-SQL positive helper                | Implemented; certified sweep 15/15 green |
| Migration recovery       | Migration 058 failure restores autocommit/foreign keys and validates referential integrity                              | Implemented; injected interruption 1/1 green |
| Privacy                  | No account, candidate, PII, Auto-submit, secrets, or unbounded evidence in attestations/logs                             | Guard green: 2,648 paths / 2,373 text files |
| Release state            | No deploy, flag, current activation, provider write, or customer effect                                                  | Preserved; release remains yellow |

## Required Local Evidence

Observed local evidence against the current implementation is:

- Jobs guards green: privacy scanned 2,648 tracked paths / 2,373 text files; schema parity reported
  102 tables / 86 indexes; provenance reported 663 lock entries / 631 unique package versions / one
  override / 14 commit-pinned repositories.
- Browser release gate 10/10 and managed release gate 17/17 passed.
- Jobs workspaces passed 1,896 tests / one skip: automation 720/1, browser 219, runner 308,
  workflows 300, portal 349. Typecheck and production build were green; the build emitted only
  Vite chunk-size warnings. Account-delete was 3/3 and the checked-in portal bundle was fresh.
- Native runner storage formatting/check/strict-Clippy/tests/release evidence was green with 14
  tests (one unit and 13 integration); exact CI-like private-root addon smoke passed. Docker was
  unavailable, and the expected `/tmp` `unsafe_entry` rejection is not a source failure.
- The final formerly failing Rust set passed 16/16 in 20.74 seconds. The final certified sweep
  passed 15/15 in 41.82 seconds. `job_integrity_authority_tests` passed 18/18, with three configured
  PostgreSQL branches self-skipped in that unconfigured filter.
- FIX-763 exact lifecycle passed 1/1 on SQLite and 1/1 on configured PostgreSQL 17.10. FIX-764
  injected migration interruption passed 1/1. Managed/unmanaged pairing passed 2/2 with its PG
  branch skipped in that unconfigured filter; legitimate unmanaged full-boundary passed 1/1;
  `irreversible_submit` passed 3/3; and the managed omission/wrong-worker no-mutation boundary
  passed 1/1. Stripe/Auto Reload lock order passed static 1/1 and final configured PG `r8` 1/1.

The first 19-test PostgreSQL manifest is **SUPERSEDED**: it ran
`2026-08-30T05:37:39Z–05:38:16Z`, summed wall 37.10 seconds, with binary SHA-256
`2aab625238a10ca16d164c819be7bb78c7750a5f2d1f1804e1938021d93fa7c8` before the final source
digest.

The final frozen-source manifest passed 19/19 sequentially on PostgreSQL 17.10 Homebrew aarch64
against a newly created `bluey_phase614b_pg17_r8` at
`2026-08-30T09:32:42Z–09:33:44Z`, summed observed real time 30.49 seconds. Binary
`server/target/debug/deps/bluey_server-0ad0add04d272872` had SHA-256
`1eb9fb796a5789c53dfbacd7d47fc90f4347ddbbc15c50b99d61d6bd6e947f0b`; every exact test
reported one passed / zero failed / 1,585 filtered out. The IMPL record enumerates all 19 names.

The explicit support-feature integration target passed 108/108 with zero
failed/ignored/measured/filtered in 543.78 seconds; binary SHA-256 is
`4cc05b9abe006f8dc23bdf79ba2bc56323f579fa09e755f409ce842838e96b10`. The feature-off library
aggregate passed 1,586/1,586 with zero failed/ignored/measured/filtered in 2,178.55 seconds using
the binary and digest recorded for `r8` above. All remaining Rust targets passed. Locked default
and support-feature checks passed; both strict-Clippy profiles passed with `-D warnings` and zero
warnings.

Default locked release builds passed without warnings. `bluey-server` SHA-256 is
`4cc6937745138957f91946981fbf5206f7f03f2bb672e783bc3ebb2952f3550f`; `bluey-jobs-api`
SHA-256 is `4c662fcfcaa4e9b54adb12a27e09e1a2b937b4a3ec6bdb15779231c8e26ec30a`. The support feature
is absent from ordinary dependencies/aggregates, deliberately fails a release check at
`compile_error!`, and left no feature/fixture identifiers in either release binary.

Independent source/security and final documentation rereviews are green with no remaining P0–P2.
The yellow evidence limits in FIX-733 through FIX-735 and FIX-739 through FIX-743 remain binding
even where broader focused or PostgreSQL evidence is green.

## Clean-Room Public Product Findings

Public customer-facing observations are non-authoritative roadmap input. No private competitor
session, undocumented endpoint, proprietary UI/code, or provider-side implementation was used.

- Bluey's public `/jobs/` presents review-first discovery, application kits, and invited-beta
  local/cloud runners. It does not prove the unpushed Phase 614B source.
- Giraffy's public surfaces describe discovery/matching, resumes, contacts, alerts, market data,
  tracking, manual/extension submission, C2C Autopilot, Agent Connect/MCP, opportunity maps,
  upskilling, salary, and sponsorship. Public source classes include employer/ATS pages,
  boards/direct pipelines, curated or hidden leads, and recruiter email/network requirements.
  These are vendor claims and were not independently verified.
- Tsenta publicly describes find/prep/apply/track across 50,000 career pages and 19 ATS families.
  Its `/messaging` page presents Text to Apply and WhatsApp entry points; `/mcp` describes OAuth and
  a Streamable-HTTP-style `https://api.autojobs.me/api/v1/mcp` endpoint with Claude, Cursor, and
  Codex setup. The endpoint was not probed.
- Existing public notes for LazyApply, Sonara, AIApply, LoopCV, Simplify, ApplyCot, and comparable
  services remain clean-room roadmap inputs, not Phase 614B evidence.

The permitted product lesson is a unified career command center with visible source provenance,
resume-first activation, reviewable packets, tracking, and separately authorized C2C, messaging,
and MCP services. It is not permission to copy private logic, code, or UI.

## Phase 620 Separation

`PLAN-PHASE-620A-JOBS-SIMULATOR-FIRST-BUSINESS-MESSAGING-CONTROL-PLANE.md` is a separate,
design-only plan. Its independent green review applies to design quality only, not implementation
or launch. It is excluded from this round's 44-file manifest and acceptance, keeps every flag at
`0` and all egress disabled, separates WhatsApp Business Platform from Third Party Agent Platform,
does not support personal WhatsApp or unattended personal iMessage, and treats Apple Messages for
Business separately. Its simulator is no-egress; WhatsApp eligibility is fail-closed, WhatsApp
data is excluded from training/improvement, and OAuth write authority remains `0`. Its SHA-256 is
`3076d531f4de783df256c7431ad88a072e5779ffcc8f68e83ad556df89aff701`.

`PLAN-PHASE-620B-F-JOBS-OMNICHANNEL-OUTREACH-AND-AGENT-CONTROL-PLANE.md` adds delegated-only
Gmail/Microsoft mailbox authority and recovery, hostile-content isolation, MCP session/resumption
binding, active-client inventory, and cross-channel routing/fallback/deduplication/STOP semantics.
Independent rereview found no remaining P0–P2 at SHA-256
`968fd808b2da9f1730d2d188844c0811b3c5a3c4e88be15ef2cb850e3092bd12`.

The user-supplied ZIPs were assessed separately in
`AUDIT-JOBS-REFERENCE-PACKAGES-CLEAN-ROOM.md`, SHA-256
`4243690318e57ce2b313dace663a3c79668070d07fa2b72c24f335e4fb89273c`; no package code was
imported.

## PR #26–#32 Read-Only Audit

All seven PRs are OPEN, DRAFT, reported MERGEABLE, and have no review, review decision, or review
request. The audit performed no checkout, fetch, edit, comment, approval, retarget, rebase, merge,
close, push, or deploy.

| PR | Exact base → head | Merge/check summary | Linkage/blocker |
| --- | --- | --- | --- |
| [#26 — `feat(jobs): add ATS certification authority`](https://github.com/Dhanunjay-Divi/bluey/pull/26) | `feat/phase-603-jobs-local-browser-release-authority` `427e3e3dda1367d302608225dadbecbfe9fac904` → `feat/phase-604-jobs-ats-certification` `8845566bcb4a2c8c56a3c525e196676273b3433c` | CLEAN; 0 checks | phase-603 base has no discovered PR |
| [#27 — `feat(jobs): add reviewed communication execution authority`](https://github.com/Dhanunjay-Divi/bluey/pull/27) | `8845566bcb4a2c8c56a3c525e196676273b3433c` → `feat/phase-605-jobs-communication-execution` `936fba2419e093eb6dd9d27a764d3c4bc8c6fb25` | CLEAN; 10 success / 5 skipped | Draft/unreviewed |
| [#28 — `feat(jobs): add launch safety control plane`](https://github.com/Dhanunjay-Divi/bluey/pull/28) | `936fba2419e093eb6dd9d27a764d3c4bc8c6fb25` → `feat/phase-606-jobs-launch-safety` `b1ed19024b2a24488771e2cef6328bb655274fbb` | CLEAN; 9 success | Draft/unreviewed |
| [#29 — `feat(jobs): make web automation cloud-first`](https://github.com/Dhanunjay-Divi/bluey/pull/29) | `b1ed19024b2a24488771e2cef6328bb655274fbb` → `feat/phase-608-jobs-cloud-first-web` `792506b0281d29aaedaa1e87be284432b2353bb6` | CLEAN; 9 success | Draft/unreviewed |
| [#30 — `feat(jobs): add durable workflow command authority`](https://github.com/Dhanunjay-Divi/bluey/pull/30) | `792506b0281d29aaedaa1e87be284432b2353bb6` → `feat/phase-609-jobs-workflow-command-outbox` `2e2a18a919b03a500e56713e4c0be2aa081f80ab` | CLEAN; 9 success | Diverged: ahead 3 / behind 1; merge base `7d96fb68738e8069c36db9b9d9e7ff079f1fa0ab`; explicit lineage decision required |
| [#31 — `feat(jobs): add durable workflow cleanup authority`](https://github.com/Dhanunjay-Divi/bluey/pull/31) | `2e2a18a919b03a500e56713e4c0be2aa081f80ab` → `feat/phase-610-jobs-workflow-cleanup-authority` `89d6b820ae46a42557c8ce76d9d632465a67b99c` | CLEAN; 9 success | Depends on #30 lineage |
| [#32 — `feat(jobs): add managed cloud launch authority`](https://github.com/Dhanunjay-Divi/bluey/pull/32) | `89d6b820ae46a42557c8ce76d9d632465a67b99c` → `feat/phase-611-jobs-managed-cloud-launch-authority` `423fba5c45ff20f00631d650b28f5a277a97985d` | UNSTABLE; 8 success / 1 failure | Managed-runner image build/smoke failed; steps 23–27 skipped; local `f50103e8` fix absent remotely |

The stack is not merge-ready. The phase-603 anchor is 14 commits ahead / zero behind `main`
`755e7d71c5ec15ea7079f7dce5a32f02b7b1fcab` without a discovered PR; #30 has exact ancestry
divergence; #32 is failed and lacks the handoff fix; and every PR is draft and unreviewed. #32's
remote tip is 31 commits ahead / zero behind `main`.

## External-Only Evidence

Exact-tip hosted CI, hosted PostgreSQL failure injection, signed production-key
custody/rotation/revocation, immutable artifact read-back, Docker/Linux runtime evidence, live
provider canaries, monitoring/on-call, rollback rehearsal, customer cohorts, production-flag
read-back, and deployment remain external-only and pending. The observed isolated local
PostgreSQL 17.10 evidence cannot satisfy them.

## Parked Flags And Effects

All production and provider-write flags remain `0`; every current activation keeps source
verification false, and `directDiscovery` and `globalDiscovery` remain false. Phase 620 egress and
OAuth-write authority remain `0`. This round authorizes no live provider access, application,
email, message, deployment, key creation, customer cohort, or production mutation.

## Implementation-Agent Contract

1. Work only on `feat/phase-614b-jobs-signed-integrity-authority` in the authoritative worktree and
   preserve unrelated work and the frozen 44-file non-doc source manifest.
2. Load `$bluey-ops`, verify branch/status, and read the final Phase 614 record plus this Round,
   IMPL, REVIEW, and FIX-725 through FIX-766 before any further source change or review.
3. Do not reopen implemented authority decisions or add a shortcut positive constructor. New
   findings require a bounded FIX/phase and refreshed frozen-source evidence.
4. Keep Round 615 and Phase 620 separate; clean-room competitor observations remain product
   input, never integrity or release evidence.
5. Record only commands actually run and exact results. Do not infer a passing aggregate from
   focused filters or an interrupted run.
6. Do not push, merge, rebase, retarget, deploy, enable flags, create production keys, access
   private competitor sessions, or perform an external write without separate authority.
7. Preserve the frozen source; any later source change requires a refreshed freeze and review.

## Decision

The Phase 614B source implementation is complete through FIX-766 and the observed local evidence is
green; independent source/security rereview found no remaining P0–P2. The release verdict remains
🟡: bounded yellow FIX evidence, PR-chain resolution, hosted/Docker/runtime evidence, and all
external release gates are not complete. Every flag and external effect remains at zero; this
round grants no merge, deployment, provider-write, customer, or production execution authority.
