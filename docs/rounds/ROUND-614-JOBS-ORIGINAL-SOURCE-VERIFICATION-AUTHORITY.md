# Round 614 — Jobs Original-Source Verification Authority

**Date:** 2026-08-26

**Branch:** `feat/phase-614-jobs-original-source-verification-authority`

**Status:** Focused local lifecycle source frozen; aggregate and release evidence remain
conditional

> **Codex preflight:** Load `$bluey-ops` before implementation or review and reconcile its operating
> memory against this branch, the current source, and the newest Jobs authority documents.

## Outcome

Phase 614 is intended to close the local source contract for the `original_source_verifier` worker
that Round 611 reserved and Round 612 explicitly deferred. It must make a positive original-source decision
possible only from one exact, fenced, release-bound worker observation and one immutable receipt.

This is not a deployment batch. No current activation may claim source verification, no production
or provider-write flag may change, and no authenticated provider action is authorized. A successor
release contract may be exercised with local signed fixtures only after it contains the exact
verifier entrypoint, protocol, capability, runtime identity, and role quorum defined here.

## Predecessor Authority

This round reconciles four existing boundaries:

1. **Round 593 discovery evidence:** an external feed is a rankable lead, a hosted ATS snapshot may
   support a human Review-first packet, and unattended queueing requires current verified employer,
   risk, canonical-job, destination, and original-source evidence.
2. **Round 611 managed cloud:** the release schema reserves `original_source_verifier`, but the
   current v1 release and activation contracts require `sourceVerification=false` because no exact
   production entrypoint or pre-effect lease boundary exists.
3. **Round 612 successor definition:** Phase 614 owns the closed worker entrypoint, exact runtime
   role, fenced assignment and database-time heartbeat, provider-specific verification, immutable
   verification/material-change receipts, fail-closed preparation and queue rechecks, successor
   release fields, and revocation/quarantine/circuit/retry/ambiguity tests.
4. **Round 613 handoff:** public ATS acquisition remains classification input, not source authority;
   reviewable ambiguity may create a human-review packet, while approval, queueing, and final
   execution require current server-owned source authority.

Round 615 separately owns source enrollment and rights, cadence and budgets, operated scheduling,
dead-letter handling, parser/volume/freshness SLOs, broad evidence-retention policy, source labels,
and direct/global discovery release binding. Phase 614 must not absorb that control plane.

## Scope

### In Scope

- one closed `original_source_verifier` entrypoint and exact managed runtime role;
- a versioned successor managed-cloud release contract that can name that role only when the exact
  component, entrypoint, capability, protocol, runtime identity, and readiness requirement agree;
- paired SQLite/PostgreSQL verifier assignments, attempts, observations, immutable receipts, and a
  compare-and-swap current head;
- database-time assignment leases and heartbeats with generation and fencing tokens;
- anonymous, semantically read-only provider retrieval for an explicit signed provider set;
- provider-specific employer tenant, job identity, URL, path, destination, and response validation;
- immutable positive, negative, indeterminate, and material-change evidence;
- receipt-derived discovery projection and fail-closed rechecks at execution-capable preparation,
  queue admission, and the existing final effect-authorizing boundary;
- exact replay, concurrent-worker, stale-fence, response-loss, revocation, quarantine, circuit,
  bounded retry, and ambiguous-result behavior; and
- fresh local evidence across focused, full, privacy, provenance, schema, and release gates.

The intended initial provider families are the five current public hosted-ATS families:
Greenhouse, Lever, Ashby, SmartRecruiters, and Workday. The implemented set must be explicit in the
signed protocol. A family or variant absent from that set remains typed unsupported and can never
produce positive execution authority.

### Out of Scope

- source registry enrollment, legal-basis or rights administration, source owners, cadence, rate
  budgets, scheduling fleet, backpressure, dead-letter operations, and freshness/volume SLOs;
- direct-discovery or global-discovery managed release authority;
- a new feed, corpus import, market aggregate, portal product surface, or customer cohort;
- model generation, application form filling, final Submit, mailbox, calendar, outreach, contact
  enrichment, or any other employer/provider write;
- provider credentials, OAuth, login, session cookies, CAPTCHA solving, anti-bot evasion, browser
  fingerprinting, or authenticated provider actions;
- live provider canaries, hosted migrations, deployment, rollback, flag mutation, or production
  flag read-back; and
- a claim that a public provider without a stable snapshot token remained globally unchanged.

## Trust Model

The server owns assignments, lease time, fences, canonical job and employer identity, current
release/runtime authority, receipt canonicalization, risk bindings, head transitions, and every
execution decision. The worker may report bounded observations; it cannot self-assert employer
verification, scam clearance, source trust, freshness, or execution capability.

A board, feed, email, recruiter record, portal field, client boolean, public ATS cursor, or mutable
`JobDiscoveryEvidence` value can nominate or describe a job but cannot satisfy a verifier receipt.
Provider presence alone is not independent employer or scam-risk authority.

The verifier operates only on account-independent job/source facts. Account and Career Track
authority remain separate and are rechecked where an account action is authorized. A receipt must
not contain account-private claims, credentials, cookies, or candidate values.

## Authority Objects

Phase 614 uses distinct objects rather than overloading the mutable posting projection:

```text
OriginalSourceVerificationAssignment
OriginalSourceVerificationAttempt
OriginalSourceObservation
OriginalSourceVerificationReceipt
OriginalSourceVerificationHead
OriginalSourceVerificationDecision
JobDiscoveryEvidence                 derived compatibility projection only
```

An implementation may represent a material-change result in the common immutable receipt or in a
separate `OriginalSourceMaterialChangeReceipt`, but it must not rewrite the prior observation or
positive receipt.

### Assignment

An assignment binds at least:

```text
assignment_id, assignment_generation, canonical_job_id, employer_id,
original_url, provider_family, provider_target_digest, expected_identity_digest,
not_before, expires_at, attempt_budget, predecessor_assignment_digest
```

Lease authority additionally binds the opaque lease-token digest, monotonic fence, exact worker ID,
managed runtime instance, manifest, activation, release, protocol, acquired database time, expiry,
and heartbeat sequence.

### Observation

An immutable bounded observation records:

```text
observation_id, assignment_id, attempt_id, provider_family, provider_record_id,
requested_url, canonical_observed_url, observed_at, retrieval_status,
http_semantics_digest, redirect_chain_digest, headers_digest, content_digest,
bounded_raw_reference_or_none, parser_version, parser_digest, worker_runtime_identity
```

Raw storage must be bounded, encrypted where retained, and free of credentials or account-private
content. The broad raw/field-evidence retention program remains Round 615; Phase 614 records only
the minimum auditable observation required to verify its receipt.

### Receipt

Every accepted result is immutable and includes the Round 612 vocabulary:

```text
verification_id, canonical_job_id, employer_id, original_url, provider_family,
verified_at, expires_at, status, job_identity_digest, content_digest,
material_change_digest, source_risk, employer_risk, scam_risk,
execution_capability, verifier_release_id, receipt_digest
```

It also binds a version and audience, assignment/attempt/replay identities, assignment generation,
lease fence, observation ID, application domain, provider target/record identity, parser protocol,
activation/manifest/runtime identity, predecessor receipt, and head generation.

The server derives `receipt_digest` from canonical bytes. Supplied clocks, hashes, risk values,
status, and execution capability are untrusted input until recomputed or matched to current
server-owned authority. A receipt never becomes valid because it deserialized successfully.

### Current Head

One monotonic compare-and-swap head per canonical job and verification authority points to the
latest accepted receipt. The head is mutable only by append-only transition; receipts and
observations remain immutable. A newer nonpositive or indeterminate receipt prevents an older
positive receipt from authorizing a new effect even if the old bytes remain available for audit.

## State Machines

Assignment lifecycle and verification verdict are separate.

### Assignment Lifecycle

```text
pending | retry_wait | idle
  -> leased(fence, generation, database-time expiry)

leased
  -> idle             accepted terminal evidence; immutable receipt/event records the result
  -> retry_wait       typed transient failure, expired-lease retry, or persisted hold backoff
  -> quarantined      integrity, identity, parser, source-trust, or changed-replay failure

pending | leased | retry_wait | idle
  -> superseded       typed release/runtime/source/subject authority loss or replacement
  -> cancelled        explicit canonical-job/account cancellation
```

These are the exact stored assignment states:
`pending`, `leased`, `retry_wait`, `idle`, `quarantined`, `superseded`, and `cancelled`.
`observing` and `publish_pending` are worker protocol phases, not database assignment states.
Expiration is an immutable `lease_expired` event followed by fenced retry/reclaim or persisted
operational-hold backoff; there is no stored `expired` assignment state. Release/runtime/source/subject revocation
maps to typed `superseded`, `cancelled`, or `quarantined` transitions rather than a stored `revoked`
state.

Required invariants:

- lease issue, heartbeat, expiry, retry time, and publication use database time;
- no network I/O occurs while a database lock is held;
- assignment generation and fence advance monotonically when authority is reacquired;
- one lease call examines at most one deterministically ordered normal scan window of 32
  candidates; persisted hold backoff or typed supersession provides cross-call fairness rather than
  an unbounded in-call keyset walk;
- up to eight due assignments in operational-hold backoff are rechecked separately so held work
  cannot consume the normal candidate budget;
- first publication rechecks the exact lease, assignment, runtime, release, provider target,
  canonical job, employer, revocation, circuit, quarantine, and relevant operational holds;
- a stale, expired, revoked, wrong-role, wrong-release, or wrong-target worker cannot create a new
  receipt or advance the head;
- authenticated byte-identical response-loss replay returns the already committed immutable result,
  including after later authority loss, without reminting freshness or advancing the head; and
- the same replay identity with changed canonical bytes conflicts and is quarantined. A later
  observation requires a new server-issued attempt identity.

### Verification Verdict

Canonical receipt statuses are:

```text
positive:
  verified_open

hard non-authority:
  closed
  redirected_to_unknown
  identity_mismatch
  materially_changed
  source_untrusted
  expired

indeterminate or retryable, also non-authority:
  unknown
  unreachable
  rate_limited
  auth_required
  captcha_required
  parse_ambiguous
  provider_unavailable
```

Only `verified_open` may carry positive execution capability. It requires the exact canonical job,
employer, source URL, provider target, provider record, application destination, job identity, and
content observation plus current server-owned source, employer, and scam-risk bindings. The worker
cannot turn a provider response into `employer_risk=clear` or `scam_risk=clear` by declaration.

Every other status fails closed for preparation that could progress to execution, queue admission,
and a new employer-facing effect. An indeterminate result may schedule a bounded retry, but it is
never evidence that a job is open or closed. Receipt history remains available without reviving an
older positive decision.

## Provider Retrieval Contract

Each supported provider has a closed protocol containing:

- exact HTTPS host and port policy, tenant/board identifiers, path grammar, and job identifier;
- the exact semantically read-only method and bounded request body, if any;
- no Authorization header, cookie jar, account session, browser profile, credential, or OAuth token;
- `redirect: error` or an equally strict provider-specific redirect policy whose entire chain is
  validated and digested;
- bounded DNS resolution, timeout, attempts, response bytes, decompression, content type, encoding,
  nesting, arrays, strings, and parser work;
- exact required fields and typed handling of missing, conflicting, duplicated, or malformed rows;
- a canonical application destination and employer/provider-tenant binding; and
- explicit typed results for closed, missing, redirected, unsupported, auth/CAPTCHA, rate-limited,
  unreachable, ambiguous, or changed content.

Workday or another provider may use an anonymous non-GET query endpoint only when the signed
provider protocol proves it is a read operation with an exact bounded body. HTTP method alone does
not authorize a provider effect. No provider endpoint that mutates state is in scope.

The public ATS cursor-v2 acquisition contract from FIX-710 remains separate. Its prefix, overlap,
total, history, and checksum evidence supports bounded discovery traversal but is neither a
signature nor a verifier receipt. Where a provider has no stable snapshot token, content may change
between observations; Phase 614 binds only the exact bytes it observed and revalidates at action
boundaries. It must not claim arbitrary provider snapshot stability.

## Material Change

The provider protocol defines which normalized fields participate in `job_identity_digest` and
which changes are execution-material. At minimum, company/employer, provider job ID, canonical and
application URLs, title/role identity, location/workplace, employment type, and closed/open state
must not change silently.

A material change appends a new receipt with `materially_changed`, identifies the changed field set
without exposing account-private values, advances the head, and blocks old kits and new execution.
Nonmaterial content changes may produce a new `verified_open` receipt only after complete current
validation. Neither path rewrites prior evidence.

## Preparation, Queue, And Effect Rechecks

Phase 614 preserves two distinct preparation meanings:

1. **Review-first packet preparation:** a current hosted-ATS snapshot may create a packet for human
   review when its canonical key and destination match. It grants no queue, Auto-submit, or final
   effect authority.
2. **Execution-capable preparation:** requires the current positive verifier receipt/head and all
   existing employer, risk, canonical taxonomy, Career Track, identity, resume, ATS, hold, and
   entitlement authority.

Queue admission rechecks the same current receipt/head inside the authoritative transaction. Final
execution continues to recheck the receipt/head and frozen job evidence at the existing
effect-authorizing boundary. Receipt expiry, a newer attempt/head, material change, source or
employer drift, Track drift, hold, circuit, quarantine, account deletion, or release/runtime loss
denies new effects without blocking receipt persistence or ambiguity reconciliation.

## Managed Release Successor

The Phase 611 v1 contract remains valid and must continue rejecting source verification. Phase 614
adds a versioned successor rather than weakening v1 validation.

A successor manifest may set structural `sourceVerification=true` in local signed test fixtures
only when all of the following agree:

- the exact `jobs-workflows` artifact and closed `original-source-verifier.js` entrypoint;
- `original_source_verifier` capability and runtime role;
- versioned original-source assignment, heartbeat, observation, and receipt protocol digests;
- exact runtime measurement and grant/claim identity;
- activation requirement and fresh compatible runtime heartbeat/capacity;
- current nonrevoked manifest, activation, transition, cohort, and release; and
- direct discovery and global discovery remain false.

This structural fixture is not a current activation, production flag, deployed worker, or customer
authority. The checked-in release configuration and every current activation keep
`sourceVerification=false` until the external release process separately proves the exact stored
artifact, approvals, runtime, canary, and rollback evidence.

## Threat Model

Phase 614 must fail closed against:

- a feed, board, email, recruiter record, client, or worker attempting to self-mint source truth;
- URL userinfo, scheme, port, suffix-host, Unicode/IDN, shortener, cross-tenant, path, or job-ID
  confusion;
- localhost, private, link-local, metadata-service, DNS-rebinding, or redirect-based SSRF;
- oversized, compressed, deeply nested, malformed, partial, duplicated, conflicting, or hostile
  provider responses;
- content mutation between retrieval, parsing, and receipt publication;
- a provider feed without stable snapshot tokens being overstated as a global snapshot;
- replay with changed bytes, response loss, concurrent workers, stale fences, clock skew, or a
  worker from an old or revoked release;
- forged receipt fields, caller-supplied freshness, rollback of a head, or cross-job/employer
  receipt reuse;
- circuit/quarantine/revocation or operational-hold changes during a fetch;
- logging or retaining credentials, cookies, account identity, candidate data, sensitive URLs, or
  unbounded raw bodies; and
- login, CAPTCHA, access challenge, terms/rights uncertainty, or an authenticated endpoint. These
  stop verification; they are never bypassed.

## Acceptance Matrix

| Boundary                   | Required Phase 614 behavior                                                                                                                                                                                      |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Managed release v1         | Continues to reject verifier activation and preserves all Phase 611 fixtures.                                                                                                                                    |
| Managed release successor  | Accepts the verifier only with the exact entrypoint, protocol, capability, runtime identity, role quorum, and false direct/global discovery.                                                                     |
| Assignment                 | One fenced winner; database-time lease and heartbeat; stale generation, fence, role, runtime, release, or revocation cannot publish.                                                                             |
| Provider                   | Every supported family has exact positive, closed, mismatch, redirect, malformed, auth/CAPTCHA, rate-limit, timeout, and hostile-URL fixtures.                                                                   |
| Observation                | Exact bounded bytes/metadata are immutable, digest-bound, credential-free, and connected to one assignment and attempt.                                                                                          |
| Receipt                    | Canonical server digest, immutable history, exact replay, material-change preservation, and monotonic CAS head.                                                                                                  |
| Projection                 | Positive posting evidence derives only from the current receipt/head; imports and constructors cannot self-assert verified employer/scam/source authority.                                                       |
| Review-first               | Hosted ATS evidence may prepare for human review but cannot queue or authorize an effect.                                                                                                                        |
| Queue                      | Current positive receipt/head and all existing Track, identity, resume, risk, ATS, hold, entitlement, deletion, and policy authority are rechecked transactionally.                                              |
| Reservation/run transition | Current implementation takes `H -> M -> D` and rechecks source/discovery. Later claim/effect gates prevent an external effect, but stale capacity may be consumed; ATS/integrity composition remains Phase 614B. |
| Final effect               | Frozen job evidence equals the current validated receipt/head; drift denies new effects while receipt/reconciliation paths remain available.                                                                     |
| Retry                      | Typed and bounded; no retry remints freshness, crosses a stale fence, or converts ambiguity into success.                                                                                                        |
| Circuit/quarantine         | New leases/publications stop under current authority; tuning, canaries, alerts, and operated SLOs remain Round 615/external.                                                                                     |
| Privacy                    | No secrets, account-private values, authenticated provider state, or unbounded raw provider content enters receipts, logs, or public status.                                                                     |
| Release state              | No deployment, cohort, current activation, provider write, or production flag changes in this batch.                                                                                                             |

## Required Local Evidence

Implementation is not complete until fresh evidence records all of the following after the final
diff. Historical Phase 613 counts are baseline context, not Phase 614 pass counts.

- focused provider-verifier, worker runtime/API, worker-auth, discovery-quality, and eligibility
  suites;
- paired SQLite/PostgreSQL migration, fresh-schema, replay, concurrency, revocation, deletion/hold,
  circuit, quarantine, and schema-parity tests;
- a disposable PostgreSQL run through the normal migration path twice;
- managed-release tests proving unchanged v1 rejection and exact successor acceptance;
- closed archive/inventory/runtime measurement and verifier-entrypoint import smoke without live
  provider access;
- all five Jobs workspace tests, strict typechecks, and production builds;
- `cargo fmt --all --check`, all-target check, strict Clippy, and full Rust all-target tests;
- whole-diff privacy, provenance/SBOM, schema-parity, CI-guard, release, Browser, native-storage,
  account-deletion, and scoped diff checks; and
- a line-by-line review of every changed file, including generated or release-authority surfaces.

Provider behavior in local tests uses fixtures or a host-pinned local mock. Tests must assert zero
credentials, zero cookies, zero authenticated actions, zero form writes, and zero external network
effects.

### Implementation Checkpoint — 2026-08-26

The current Phase 614 worktree implements the contract with paired SQLite 057/PostgreSQL 035
migrations, a private Rust assignment/heartbeat/terminal API, an exact managed workflow entrypoint,
anonymous provider-specific TypeScript retrieval, immutable evidence and compare-and-swap head
authority, receipt-derived compatibility projection, and transactional action-boundary rechecks.
The successor release contract is represented only by signed local fixtures; Phase 611 v1 and every
current activation continue to reject source verification.

Observed focused checkpoints on the current implementation include:

```text
Provider verifier fixture/adversarial matrix         27 / 27
Post-freeze automation Vitest                        708 passed / 1 skipped; 38 files / 1 skipped
Post-freeze workflows Vitest                         300 / 300; 13 files
Post-freeze automation/workflows typecheck           passed
First fresh full Jobs aggregate                     1,883 passed / 1 failed / 1 skipped
  Runner                                             307 / 308; volume-purge 1,000ms sentinel only
Immediate isolated + repeated volume-purge          1 / 1; then 20 / 20
Subsequent runner aggregates                        3 / 3 at 308 / 308
Second fresh full Jobs aggregate                    1,884 passed / 1 skipped
  Files                                              146 passed / 1 skipped
  Automation                                         708 passed / 1 skipped; 38 files / 1 skipped
  Browser                                            219 / 219; 34 files
  Runner                                             308 / 308; 33 files
  Workflows                                          300 / 300; 13 files
  Portal                                             349 / 349; 28 files
All five Jobs workspace typechecks/builds            passed after final parser hardening
Portal production build                              2,299 modules; existing >500 kB warning
Disposable-index privacy gate                       2,648 tracked paths / 2,373 text files
Dependency/provenance                               663 lock / 631 versions / 1 override / 14 pins
Browser release CI gate + workflow contract          10 / 10; contract passed
Managed-cloud workflow contract                      passed
Browser account-deletion pending-flow gate           3 / 3
Jobs CI guard self-tests                             passed
Native runner storage tests                          14 / 14 (1 lib + 13 integration)
Native runner fmt/strict Clippy/release build        passed
Darwin native-addon smoke                            passed
Rust original-source authority suite                 25 / 25 in four owner/reviewer runs; PG URL case self-skipped
Public SQLite lifecycle/replay subset                 6 / 6 owner and reviewer
Typed assignment expiry/revocation                    1 / 1 (4.82s final source)
Held-prefix fairness                                  1 / 1 (3.73s final source)
Verifier heartbeat/terminal PG lock order            focused static regression passed
Jobs operations readiness regression                 1 / 1 (0.00s final source)
Application state-machine regression                 1 / 1 (2.33s final source)
Provider-source Review-first regressions              2 / 2 (0.07s, 2.84s final source)
Submitted verified-runner finalization denial         1 / 1 (2.51s final source)
Certified fixture-scope subset                        6 passed / 8 expected authority denials
Cloud/local intervention diagnostics                  0 / 2; deeper shared-authority blockers
Protected-admission PostgreSQL lock order             1 / 1
Managed-cloud release v1/v2 gate                     17 / 17
Schema parity                                        95 tables / 79 indexes per dialect
Execution-lease regressions                          13 / 13
Local-run regressions                                 4 / 4
Runner-plan review-first matrix                       2 / 2
Final-source Rust check                               passed (34.06s)
Final-source strict Clippy                            passed (49.41s)
Pre-final Rust library baseline                      1,416 / 1,450; 34 failed
Integration E2E                                      86 / 108; 22 shared approval denials
```

These are local-source checkpoints, not a production verdict. The replacement 25-test Rust suite invokes
the public SQLite lease, heartbeat, completion, and failure APIs. Its 6-test public subset covers
lease/reclaim and bounded held-prefix recovery; heartbeat and exact replay; positive completion;
changed-byte quarantine; exact terminal replay after later runtime revocation without reminting;
denial of a fresh request ID; failure and exact replay; reclaimed-lease stale heartbeat/complete/fail
fences; and hold/source/runtime publication denial. The focused PostgreSQL source regression pins
heartbeat, first terminal publication, and changed-byte replay quarantine to
`H -> M -> D -> assignment`; exact replay remains read-only. It also proves that managed heartbeat
expiry and runtime-grant revocation return typed managed-authority unavailability, supersede the
assignment, and mint no attempt or receipt. No live PostgreSQL behavior is inferred.
Independent review found and closed raw JSON/member and response-metadata parser boundaries: a
bounded pre-scan rejects duplicate decoded keys before normal parsing, including escaped/nested
variants, and exact raw
octets are decoded with fatal UTF-8 and hashed before semantic parsing. Content type is exactly
`application/json` with only optional UTF-8 charset, content encoding is absent or `identity`, and
bounded/over-limit header fingerprints are digest-bound. A streamed over-limit body binds the exact
bounded `max+1` prefix so distinct oversized bodies retain distinct content digests. The fresh
focused provider suite passed 27/27 with automation typecheck green. The parser also rejects
recognized-but-malformed scalar
aliases and nested provider records rather than collapsing them to absent; the added matrix spans
IDs, workplace/timestamps, Lever list/salary shapes, SmartRecruiters job-ad sections, and Ashby
`isListed` across all five families.

An earlier final-parser Jobs aggregate rebuilt automation first and passed 1,884 tests with one
explicit Playwright skip across 146 passing files and one skipped file. All five workspace
typechecks and production builds then passed; the portal processed 2,299 modules and emitted only
its existing greater-than-500-kB chunk advisory. A later frozen-source validation preserved one P2
load-sensitive harness observation: the first fresh full Jobs aggregate passed 1,883, failed one,
and skipped one because the runner volume-purge test hit its explicit 1,000ms `purge deadlocked`
sentinel. It passed immediately in isolation, then 20/20 isolated repetitions and three subsequent
runner aggregates at 308/308. The second fresh full Jobs aggregate passed 1,884 with one skip, and
all five typechecks/builds remained green. Source did not change. A bounded five-second sentinel is
a successor CI-hardening follow-up, while the initial timeout remains in this audit record.

On final intended source, global Rust fmt, server
all-target check, strict Clippy, scoped diff, disposable-index privacy, dependency/provenance, CI
self-test, and release/workflow guards passed. The full Rust all-target test aggregate remains
required. The
integration E2E run is explicitly non-green: 22 tests reached the same
shared setup approval and received fail-closed `409` with `Confirm the sponsorship answer before
Auto-submit.` A future production-representative positive fixture must supply confirmed sponsorship
and the separately reviewed **Phase 614B — Signed Job Integrity Authority**; the gate is not waived.
The pre-final Rust library baseline passed 1,416 of 1,450 and failed 34 in 2,277.55s. Its readiness,
held-prefix/typed-authority, application-state, and managed-prelock failures are now exact-green
under FIX-720, FIX-719/721, FIX-722, and FIX-723. FIX-724 exact-greens two further stale
source/category expectations. A 14-case certified-fixture triage subset now has no `ScopeMismatch`:
six pass and eight reach the intentional Phase 614B employer-identity/current-authority denial.
That focused result does not establish a revised full-library count, and the full all-target gate
remains non-green; it is not collapsed into the separate 86/108 integration result.
Live PostgreSQL, the exact Docker/Linux image, exact-tip CI, and every hosted/runtime/deployment gate below
remain unproven. The Darwin native runner/addon path is locally green, but the Docker/Linux image is
still unavailable: the Docker command is absent and the host has only 4.7 GiB free.

The full Rust library rerun also exposed a stale readiness-test cardinality after
`OriginalSourceVerification` became the eighth concrete operational capability. FIX-720 repins the
count and explicitly checks that the new row inherits one global hold, no native blocker, and one
combined blocker. The focused regression passed 1/1. That is a test-fixture correction, not a
production readiness change; the full-library aggregate remains non-green for separately recorded
authority-fixture gaps.

The same aggregate exposed one older application state-machine test that still expected direct
Auto-submit queueing without the transactional `approved_execution` snapshot required by FIX-718.
FIX-722 now expects that queue attempt to fail and proves the application remains
`awaiting_review`/`review_first`, including after an invalid mode mutation. It does not add a queue
bypass or make the known integration aggregate green. The final-source focused regression passed
1/1 in 2.33s.

FIX-723 makes the combined managed-cloud `H -> exclusive M -> ATS -> fleet` prelock the first
protected operation for claim and final Submit. Unmanaged paths retain
`H -> shared M -> ATS`, and both branches take account `D` afterward. The focused static regression
passed 1/1; final fmt/check/strict Clippy and scoped diff passed, while live PostgreSQL contention
and the full test aggregate are not yet final.

FIX-724 replaces two overstated certified-fixture labels with
`provider_verified_original_source` and repins two historical tests to Review-first preparation
without queue authority. The exact tests passed 2/2. The certified subset's remaining eight
Phase 614B denials stay visible and are not promoted to passes.

The related execution-lease/local-run fixtures were cleaned up to bind provider-verified evidence,
retain the real approved-execution envelope/checksum, and persist a test-only preapproved queued
row without asking the public queue gate to fabricate Phase 614B authority. Their latest focused
intervention run is still 0/2: cloud stops at claim with a shared entitlement/execution-authority
`Conflict`; local reaches the running update and then fails the canonical match-score threshold.
This is diagnostic evidence for the deeper shared positive-authority/fixture blocker, not a green
Phase 614 result.

FIX-721 restores the typed operational classification for managed heartbeat expiry and runtime
grant revocation during assignment-authority rechecks. Those paths already failed closed; the fix
maps only registry `Unavailable`/`Revoked` to managed runtime unavailability while preserving all
other registry failures as storage/integrity errors.

The heavy 32-candidate and authority-expiry regressions use a test-only long-horizon signed runtime
fixture plus explicit aging beyond the signed TTL and the managed database clock's rounding margin.
This removes wall-clock-load flakiness without changing a production TTL, activation, or runtime
policy.

Phase 614 deliberately does not mint independent employer-identity or scam-risk clearance. A
current original-source receipt can support accurate source state, but without those separate
authorities the product remains Review-first: approval, queueing, and employer-facing effects fail
closed with zero mutation. Phase 614B, not Round 615, owns the missing signed employer-identity and
scam-risk authority needed for production-representative route-positive evidence; Round 615 remains
reserved for the Source Control Plane and Freshness SLOs.

## External-Only Evidence

Local source acceptance cannot prove:

- exact-tip hosted CI;
- the exact Docker/Linux image and verifier entrypoint in its final runtime filesystem;
- immutable registry read-back, threshold signatures, or protected-environment approvals;
- hosted PostgreSQL migration, replica, lock, and network-fault behavior;
- real runtime heartbeat capacity, task-queue behavior, or revocation propagation;
- legally approved anonymous live-provider canaries, rate budgets, robots/terms review, or provider
  behavior outside local fixtures;
- read-only root filesystem and runtime digest attestation;
- dark deployment, kill switch, rollback rehearsal, customer cohort approval, or monitoring/on-call;
  or
- production configuration or flag read-back.

These remain mandatory before any current activation or customer authority may set source
verification true. They are not valid local-green claims in the Phase 614 IMPL or REVIEW document.

## Parked Flags And Effects

The release configuration remains parked:

```text
BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED=0
BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
```

Model generation, mailbox synchronization, communication OAuth writes, communication dispatch,
communication reconciliation, and every other provider-write flag also remain `0`. No production
flag read-back is claimed. Local test fixtures may construct signed successor objects but may not
mutate a current activation, contact a provider, deploy, or produce an employer-facing effect.

## Implementation-Agent Contract

1. Work only on `feat/phase-614-jobs-original-source-verification-authority` from the reviewed Phase
   613 tip and preserve unrelated work.
2. Load `$bluey-ops`, verify branch/status, and read Round 611, Round 612, Round 613, Round 593,
   FIX-710, this Round, and the work templates before editing.
3. Implement one authority boundary at a time, with paired SQLite/PostgreSQL behavior wherever data
   or transaction semantics are involved.
4. Use `docs/work/TEMPLATE-FIX.md` for each reviewed defect; FIX-712 and FIX-713 begin the batch.
5. Do not weaken the Phase 611 v1 release contract. Add an explicit versioned successor.
6. Do not use live or authenticated provider access. Provider evidence must be local and mocked.
7. Record only commands actually run and their exact results. Never copy Phase 613 counts forward as
   Phase 614 evidence.
8. Keep all flags false, perform no deployment or provider action, and make no customer-readiness
   claim.
9. Complete a line-by-line review and leave the final verdict conditional on all external-only gates.

## Decision

Phase 614 is implemented locally under this contract and remains in conditional review. Source
verification is not currently activated, deployed, or production-proven merely because the source,
focused tests, or a signed local fixture can represent the successor role.
