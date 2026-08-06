# REVIEW: PHASE-604 - Jobs ATS Certification Authority

> **Codex preflight:** Loaded `$bluey-ops` and independently reviewed the Round
> 604 source, plan, complete temporary-index snapshot, and recorded local
> evidence only. No SSD/archive history or external environment was used.

**Reviewed snapshot:** complete temporary-index snapshot based on `427e3e3d`
**Repository index:** empty; the review did not rely on the actual Git index
**Reviewer:** Codex independent source review
**Date:** 2026-08-06

## Per-Task Review

### Canonical trust, immutable evidence, and paired persistence

| Field | Value |
|-------|-------|
| Files | ATS authority module, paired migrations, administrator API, shared canonical vectors |
| Verdict | 🟢 accept |

**Findings:**

- Strict canonical decoding, exact byte replay, bounded signature sets, current
  persisted trust, and disjoint delegated roles prevent caller-selected trust
  or cross-role authority substitution.
- Evidence, observations, manifests, activations, revocations, quarantine
  commands, transitions, circuits, allowlists, reservations, and application
  bindings are immutable or monotonic as appropriate in both database dialects.
- The paired migrations, registration paths, triggers, constraints, and parity
  guard agree at 65 tables and 57 indexes.
- FIX-615, FIX-617, FIX-623, FIX-624, FIX-632, and FIX-634 close rooted-trust,
  lifecycle-continuity, paired-schema, expired-import, revocation-chain, and
  transition-replay defects without widening production authority.

### Exact provider target and evidence boundary

| Field | Value |
|-------|-------|
| Files | shared TypeScript/Rust target grammar, provider adapters, packet guards, evidence vectors |
| Verdict | 🟢 accept |

**Findings:**

- Greenhouse and Lever target derivation requires exact HTTPS authority and
  path grammar. Shared adversarial vectors reject credentials, ports,
  lookalikes, malformed or encoded paths, and query/fragment path manufacture.
- The signed manifest binds the complete runtime-by-check-by-evidence-class
  matrix, signed observations, immutable evidence objects, suite digest, and
  six zero-tolerance counters. Non-shadow authority requires authorized sandbox
  and authorized live evidence for every runtime.
- FIX-613, FIX-625, FIX-627, and FIX-635 close target-grammar drift,
  observation-privacy, synthetic-layout, and production-evidence gaps.
- FIX-638 makes `canaryEvidenceManifestSha256` an exact alias of the canonical
  certification-manifest digest. It cannot name arbitrary unimported bytes or
  imply a second evidence artifact.
- Unsupported providers, semantic-only paths, protected portals, and synthetic
  fixtures do not gain production certification from this source batch.

### Frozen admission, Phase A/B, intervention, receipt, and recovery

| Field | Value |
|-------|-------|
| Files | eligibility, applications, local runner, cloud leases, Browser/runner submit boundaries, receipts, customer-data intervention |
| Verdict | 🟢 accept |

**Findings:**

- Packet finalization re-resolves current server authority and freezes the
  exact target, packet, attempt, runtime, activation, manifest, and nonce into a
  single-use Phase A binding. Legacy or incomplete Auto packets fail to Review.
- Local and cloud Phase B execute inside the production transaction before any
  durable irreversible marker/checkpoint or employer Submit activation.
  Capacity reservation and binding consumption are atomic with that decision.
- FIX-614, FIX-626, FIX-629, FIX-630, FIX-631, FIX-633, and FIX-637 close marker
  ordering, intervention locking, runtime admission, cloud lifecycle, HTTP
  fixture, canary-capacity, and server-owned UTC-period defects.
- FIX-620 and FIX-639 classify transport loss, malformed success, HTTP
  500/502/503/504, and every other non-4xx Phase B failure as terminal
  `submit_outcome_unknown`. The page and recovery state remain preserved and no
  retry authority is created; bounded 4xx denial remains pre-marker.
- FIX-628 expands exact certified-receipt proof. Result/resume recovery reads
  only the frozen terminal authority and cannot mint another binding,
  reservation, marker, or Submit action.

### Quarantine, circuit restoration, and audit isolation

| Field | Value |
|-------|-------|
| Files | runtime drift authority, circuit lifecycle, revocation/quarantine API, operations audit |
| Verdict | 🟢 accept |

**Findings:**

- FIX-619 and FIX-622 make unknown runtime layouts commit bounded append-only
  quarantine evidence and open/hold the exact circuit without committing an
  employer-facing marker or an unbounded outcome.
- FIX-621 requires acknowledged redacted operations auditing before mutation
  success is returned; immutable replay is the recovery path after response
  loss or audit failure.
- FIX-636 proves successor identity and ordering. FIX-640 additionally
  revalidates current trust, manifest, activation, canary allowlist, every bound
  runtime, revocation, quarantine, and expiry at server-controlled
  `recorded_at_ms` sampled after the SQLite write transaction or PostgreSQL
  authority lock.
- FIX-641 permits automatic `newer_activation` closure only for the exact
  predecessor activation circuit. Provider, target, adapter, and runtime
  circuits require the explicit reviewed-close path, preventing shadow,
  canary, cross-channel, or cross-target scope widening.

### Administrator/worker isolation and portal truth

| Field | Value |
|-------|-------|
| Files | ATS administrator API, worker auth, portal decoder/component/views, checked-in bundle |
| Verdict | 🟢 accept |

**Findings:**

- Authority import/application, revocation, quarantine, circuit transitions,
  allowlists, and status remain administrator operations. The authenticated
  worker route is private and bounded to signed structural observations;
  customer routes cannot mutate certification state.
- FIX-616 makes the portal decoder exact and fail closed. It intersects the
  server-authored certification scope with current runner availability and
  rejects missing, unknown, malformed, expired, revoked, suspended, or drifted
  projections.
- The portal production bundle contains 2,290 modules across 27 files, no
  source maps, and aggregate SHA-256
  `00dc3c66468e894ca96c1217d8e9a8edde22b5ce4bb1c5ca843dcc6c1ac493e0`.
  Its chunk-size advisory is nonfailing and does not alter the source verdict.

### Operations, protected flags, and launch truth

| Field | Value |
|-------|-------|
| Files | `jobs/OPERATIONS.md`, runner environment example, CHANGELOG, Round 604 and FIX-613 through FIX-641 |
| Verdict | 🟢 source accepted; external activation blocked |

**Findings:**

- The runbook separates shadow rehearsal, signed import, head application,
  canary monitoring, circuit opening, revocation, recovery, and rollback.
- FIX-618 makes PostgreSQL runtime migration discovery explicit. The optional
  PostgreSQL tests compile and self-skip because no authorized
  `BLUEY_TEST_POSTGRES_URL` was provided; they are not represented as live
  database evidence.
- No provider, tenant, runner, device, canary, unattended path, distribution,
  or production environment is described as certified or launched.
- All protected production flags remain `0`; changing them requires a separate
  reviewed and authorized production change.

## Cross-Task Findings

| Acceptance criteria | Independent review result |
|---------------------|---------------------------|
| AC1-AC9 | 🟢 Strict canonical trust, role separation, immutable evidence, exact target/provider boundaries, monotonic lifecycle, expiry, revocation, and replay are source-complete. |
| AC10-AC11 | 🟢 Bounded append-only drift evidence, automatic opening/holding, exact CAS replay, post-lock authority revalidation, and reviewed-versus-automatic close authority are source-complete. |
| AC12-AC23 | 🟢 Shared resolution, frozen admission, exact runtime, Phase A/B ordering, canary accounting, intervention invalidation, terminal receipts, and non-widening recovery are source-complete. |
| AC24-AC25 | 🟢 Administrator/worker isolation, acknowledged redacted audit, strict portal decoding, and truthful runner-scope intersection are source-complete. |
| AC26 | 🟢 Paired source migrations and parity guards pass. Live PostgreSQL execution, concurrency, failover, and recovery remain external gates. |
| AC27-AC28 | 🟢 The complete stable-check/evidence-class matrix and all six zero-tolerance counters have passing source enforcement and adversarial coverage. |
| AC29 | 🟢 Protected flags remain disabled; no source or documentation change authorizes enablement. |
| AC30 | 🟢 All wording is source-only and explicitly parks provider, tenant, runner, device, canary, distribution, and production authority. |

The line-by-line pass covered FIX-613 through FIX-641 against the complete
temporary-index snapshot. No unresolved correctness, security, privacy,
fail-closed, schema-parity, receipt/recovery, API-isolation, portal-truth, or
launch-claim finding remains in the reviewed source.

## Build & Test Verification

```text
Complete temporary-index snapshot based on 427e3e3d

Jobs workspace                       1,402 tests / 124 files passed
  automation                           644 tests / 35 files
  browser                              219 tests / 34 files
  runner                               269 tests / 32 files
  workflows                             76 tests /  7 files
  portal                               194 tests / 16 files
Package typecheck gates                 5/5 passed
Package build gates                     5/5 passed

Portal production bundle             2,290 modules / 27 files
Portal source maps                    none
Portal aggregate SHA-256              00dc3c66468e894ca96c1217d8e9a8edde22b5ce4bb1c5ca843dcc6c1ac493e0
Portal chunk advisory                 nonfailing advisory only

Rust formatting                      passed
Rust cargo check                      passed
Rust strict Clippy                    passed
ATS authority module                    58 tests passed
Server library                       1,106 tests passed
Other server targets                   107 tests passed
Server total                         1,213 tests passed
Optional PostgreSQL cases             compiled; self-skipped without BLUEY_TEST_POSTGRES_URL

Schema parity                           65 tables / 57 indexes passed
Dependency provenance                  663 lock entries / 631 versions
Approved provenance override             1 verified
Pinned repository provenance            14 repositories verified
Client/server boundary guard           passed
CI guard self-tests                    passed
Browser release gate                     9/9 passed
Workflow guards                        passed
Temporary privacy scan               2,456 paths / 2,183 text records passed
Diff hygiene                           passed
```

The privacy and diff-hygiene checks above passed again after this review record
was closed. That confirmatory repeat adds no production evidence and does not
broaden this verdict.

No live PostgreSQL service, authorized provider sandbox/live tenant,
independent signing ceremony or credential custody, cloud Browser/Temporal
fleet, object-store fault matrix, immutable artifact host, physical signed
device, approved canary, or production deployment is represented by these
results.

## Overall Verdict

🟢 **ACCEPT—SOURCE COMPLETE** - The complete temporary-index snapshot based on
`427e3e3d` passes independent line-by-line source review and the recorded local
verification gates. The actual repository index remains empty.

This verdict is source-only. It is not a certification, launch, distribution,
canary, deployment, or protected-flag authorization for any provider, tenant,
runner, device, fleet, or unattended submission path. Any later source change
outside the confirmatory review-file hygiene pass reopens the affected review
scope.

## Follow-ups for Next Batch

- Provision independently governed signing keys, credentials, root trust, and
  audit custody through an authorized ceremony.
- Run approved Greenhouse and Lever sandbox and live-provider matrices against
  authorized tenants, vacancies, and data-rights terms.
- Apply and exercise the migrations on live PostgreSQL, including
  multi-process advisory-lock races, failover, recovery, and reconciliation.
- Run signed clean-device Browser packages and authenticated isolated cloud
  runner/Temporal/object-store fleets with immutable artifact read-back and
  crash/fault recovery.
- Obtain named canary accounts, bounded plans, incident ownership, provider and
  data-rights approval, and reviewed production deployment authority.
- Keep every protected flag at `0` until those external gates pass and a
  separate authorized production change explicitly enables it.
