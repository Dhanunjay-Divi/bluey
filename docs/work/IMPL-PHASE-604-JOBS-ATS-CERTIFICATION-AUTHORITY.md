# IMPL: PHASE-604 - Jobs ATS Certification Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the active
> Round 604 worktree, authoritative plan, current diff, and locally recorded
> verification evidence. No SSD/archive history, production service, live
> tenant, credential, signing key, cloud runner, or physical device was used.

## Scope

**Does:**

- Adds one offline-root-authorized trust domain with strict canonical signed
  trust policies, delegated role separation, immutable layout observations,
  evidence objects, manifests, activations, revocations, and quarantine
  commands.
- Adds paired SQLite/PostgreSQL authority state for immutable imports,
  monotonic heads, compare-and-swap transitions, revocation, quarantine,
  circuits, canary allowlists and reservations, application bindings, and
  recovery.
- Defines one exact Greenhouse/Lever application-target grammar in TypeScript
  and Rust, backed by shared positive and adversarial vectors. Unsupported or
  malformed providers remain on Review, Takeover, or Handoff paths.
- Resolves certification from server-owned posting, discovery, Track,
  provider, evidence, policy, activation, runtime, and rollout state, then
  freezes the exact result into approved Auto-submit packets.
- Integrates single-use Phase A binding and server-authorized Phase B consume
  into both local and cloud execution transactions before any durable
  irreversible marker or Submit activation.
- Preserves exact terminal receipt/result recovery without minting new submit
  authority, and invalidates unused certification when an intervention changes
  an employer-facing answer or document.
- Adds administrator-only import, activation, revocation, canary-allowlist,
  circuit, and status routes; a private bounded worker observation path; and
  redacted operations auditing. Runtime layout drift appends bounded quarantine
  evidence and opens/holds its circuit internally; reviewed closure remains an
  administrative operation.
- Adds strict portal decoding and truthful certification summaries for status,
  runner scope, verification time, expiry, safe reason, and next action.
- Extends schema-parity, CI-guard, canonical-vector, provider, Browser, runner,
  portal, integration, and recovery tests, and rebuilds the checked-in Jobs
  portal bundle.
- Documents the signed ceremony, shadow rehearsal, canary stop conditions,
  incident response, recovery, and external production boundary.

**Does NOT:**

- Certify Greenhouse, Lever, any tenant, any vacancy, or any unattended
  employer-facing submission path from source or synthetic evidence.
- Enable local Browser distribution, cloud Browser distribution, managed model
  generation, or mailbox synchronization.
- Create, import, expose, or use production root, delegated signing, provider,
  tenant, artifact-host, runner, object-store, or database credentials.
- Run an authorized sandbox/live tenant matrix, a live PostgreSQL migration or
  concurrency matrix, a cloud Browser/Temporal matrix, or a signed physical
  macOS/Windows runner matrix.
- Change Review-first provider behavior or allow certification alone to bypass
  Career Track Auto authorization and packet approval requirements.
- Claim production launch, provider approval, canary completion, distribution,
  or operational readiness from local source gates.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `infra/{sqlite,postgres}/server-runtime/*jobs_ats_certification_authority.sql` | Created | Paired immutable trust, evidence, manifest, activation, revocation, quarantine, circuit, rollout, reservation, binding, and recovery state. |
| `infra/{sqlite,postgres}/server-runtime/*jobs_browser_release_runtime_components.sql` | Created | Persist exact local release component identity used by certification. |
| `infra/{sqlite,postgres}/server-runtime/*jobs_runner_process_runtime_authority.sql` | Created | Persist exact cloud process, image, Browser, and one-use runtime authority. |
| `server/src/db/jobs/ats_certification_authority.rs` | Created | Strict codecs, signature verification, lifecycle, resolver, Phase A/B, quarantine, circuits, canary limits, and recovery. |
| `server/src/jobs_ats_target.rs` | Created | Canonical Rust Greenhouse/Lever target parser using shared vectors. |
| `server/src/api/jobs_ats_certifications.rs` | Created | Administrator authority registry/status routes and bounded worker observation route. |
| `server/src/db/{mod.rs,jobs.rs}`, `server/src/{lib.rs,api/mod.rs}` | Modified | Register paired migrations, modules, and routes. |
| `server/src/db/jobs/{browser_release_authority,browser_release_registry,browser_release_trust}.rs` | Modified | Bind current local Browser component identity into ATS runner authority. |
| `server/src/db/jobs/{runner_volume_purge,execution_authority,execution_leases}.rs` | Modified | Bind cloud process/image authority, accept either server-authorized runner at the shared lifecycle boundary, and preserve exact local/cloud Phase A/B claims and recovery. |
| `server/src/db/jobs/{eligibility,applications,local_runner,customer_data}.rs` | Modified | Apply the shared resolver, freeze packet authority, claim locally, and invalidate changed packets. |
| `server/src/api/{jobs,jobs_local_capability,jobs_runner_volumes,jobs_worker_auth}.rs` | Modified | Carry exact certification proofs, authorize submit, validate terminal receipts, and isolate workers. |
| `server/src/db/jobs/{tests,postgres_local_authority_tests}.rs`, `server/tests/integration_e2e.rs` | Modified | Cover lifecycle, runtime, isolation, packet, receipt, intervention, and dialect boundaries. |
| `jobs/automation/src/ats-target.ts` | Created | Canonical TypeScript provider target grammar. |
| `jobs/automation/src/{adapters,approved-execution,contracts,execute,final-submit-proof,index,packet-guards,policy,provider-job-key,receipts,source-catalog}.ts` | Modified | Require exact target, frozen certification, packet, submit, confirmation, and receipt authority. |
| `jobs/automation/src/providers/{greenhouse,lever}.ts` | Modified | Preserve exact versioned provider state machines through confirmation and recovery. |
| `jobs/automation/tests/**` | Created/modified | Add canonical authority, target, packet, provider, receipt, and adversarial vectors. |
| `jobs/browser/src/{authorized-final-submit,local-capabilities,release-authority,run-controller}.ts` | Modified | Bind local runtime authority and authorize Phase B before marker/click. |
| `jobs/browser/tests/**` | Created/modified | Cover runtime reachability, capability binding, denial ordering, ambiguity, and recovery. |
| `jobs/runner/src/{certified-final-submit,execution-lease,resume-policy,runner-volume-client,server}.ts` | Created/modified | Bind cloud runtime authority and enforce one-use Phase B before checkpoint/click. |
| `jobs/runner/tests/**` | Created/modified | Cover process authority, runtime mismatch, denial ordering, ambiguity, and recovery. |
| `jobs/portal/src/{types,App.tsx,styles.css}` | Modified | Decode and present server-authored certification without exposing internal authority. |
| `jobs/portal/src/{components/AtsCertificationSummary.tsx,lib/ats-certification.ts}` | Created | Add bounded certification validation, runner intersection, and safe presentation. |
| `jobs/portal/src/{lib,views}/**` | Modified | Apply the same fail-closed summary to matches, applications, and preview actions. |
| `jobs/scripts/{browser-release-ci-gate,check-jobs-schema-parity,ci-guards-self-test}.mjs` | Modified | Extend release/runtime binding, paired-schema, and policy guards. |
| `web/jobs/` | Rebuilt | Preserve the rebuilt production portal bundle; 27 files, no source maps, stable aggregate hash, and generated-diff hygiene were verified. |
| `jobs/OPERATIONS.md`, `ops/bluey-jobs-runner.env.example` | Modified | Document disabled-by-default authority, ceremony, runtime identity, monitoring, and recovery. |
| `CHANGELOG.md`, Round 604, `FIX-613` through `FIX-641`, IMPL/REVIEW docs | Created/modified | Record source scope, defect corrections, evidence, limitations, and handoff. |

## Build & Test

The following checkpoint results remain useful diagnostic evidence, but the
final report is based on a temporary-index snapshot of the complete working
tree rooted at `427e3e3d`; the actual Git index remains empty.

```text
Focused local Phase B ordering       18 tests passed
Focused cloud Phase B ordering       27 tests passed
Rooted-policy authority checkpoint    9/9 tests passed
Rooted-policy migration checks         2 passed
Rooted-policy server library check     passed
Portal checkpoint                    178 tests passed; typecheck/build passed
Portal focused final guard             60 tests passed; typecheck passed
PostgreSQL migration discovery          1 test passed

Final Jobs workspace tests            1,402 tests / 124 files passed
  automation                            644 tests / 35 files passed
  browser                               219 tests / 34 files passed
  runner                                269 tests / 32 files passed
  workflows                              76 tests / 7 files passed
  portal                                194 tests / 16 files passed
Final Jobs typechecks/builds          all five workspaces passed
Final portal bundle                   2,290 modules; 27 files; no source maps
Portal aggregate SHA-256              00dc3c66468e894ca96c1217d8e9a8edde22b5ce4bb1c5ca843dcc6c1ac493e0
Final ATS authority module              58 tests passed
Final server Rust                     1,213 tests passed
  library                             1,106 tests passed
  integration/other                     107 tests passed
Final server fmt/check/strict Clippy  passed; all targets; warnings denied
Final schema parity                    65 tables / 57 indexes passed
Final dependency provenance           663 entries / 631 versions / 1 override
Final source provenance                14 commit-pinned repositories passed
Final client/server boundary          passed
Final CI guard self-tests             passed
Final Browser release guard             9/9 tests + workflow contract passed
Final temporary-index privacy gate    2,456 paths / 2,183 text files passed
Final temporary-index diff hygiene    passed; actual Git index empty
```

The production portal build retained one non-failing Vite size advisory for
`index-oWaD1Ibx.js` (502.51 kB minified, 131.20 kB gzip). The checked-in bundle
remained deterministic at the exact aggregate digest above.

The PostgreSQL authority, CAS, Phase B, and canary concurrency regressions
compiled but self-skipped because `BLUEY_TEST_POSTGRES_URL` was not set. No
result is represented as live PostgreSQL evidence.

The final report must preserve exact command output rather than extrapolating
from focused checkpoints. Live PostgreSQL, authorized tenant, cloud runner,
physical-device, artifact-signing, and independent signing-ceremony evidence
was not run and is explicitly parked.

## Deviations from Plan

| Deviation or refinement | Rationale |
|-------------------------|-----------|
| The canonical provider grammar is implemented once per language and governed by shared vectors. | Exact target authority must not drift among policy, adapter, receipt, local, cloud, and server paths. |
| Root authorization is a persisted, offline-root-signed delegated policy rather than caller-supplied anchors on ordinary imports. | Every lifecycle object must remain in one server-authorized trust domain. |
| Auto-submit packet authority advances to schema three and the certified receipt projection to schema four. | Mixed-version packets and receipts must fail closed instead of silently omitting certification. |
| Phase B response loss is classified as `submit_outcome_unknown`, while a bounded 4xx authorization denial remains a safe pre-marker failure. | A committed server transaction cannot be treated as retryable merely because its response was lost. |
| Exact local runtime components and cloud process grants use dedicated paired migrations. | Certification must bind the real Browser/Chromium/image/process identity, not a caller assertion. |

No refinement widens provider support, enables a distribution flag, or converts
synthetic evidence into production authority.

## Known Follow-ups

- Provision independently managed signing keys and an approved public root
  anchor through a separate authorized ceremony.
- Run authorized Greenhouse and Lever sandbox/live matrices against approved
  tenants and two or three approved test vacancies per provider.
- Apply and exercise the migrations against live PostgreSQL, including
  multi-process compare-and-swap, advisory-lock, concurrency, recovery, and
  failover cases.
- Run exact signed macOS arm64/x64 and Windows x64 Browser packages on clean
  physical devices with immutable artifact read-back and approved signing.
- Run the authenticated Temporal, isolated cloud Browser, image, PostgreSQL,
  R2/S3 fault, crash, recovery, capacity, retention, deletion, and read-back
  matrix.
- Obtain owner-approved canary accounts, plans, daily and concurrency limits,
  named support/incident ownership, provider/data-rights approvals, and a
  separate production change before altering any protected flag.

## Review Checklist (for reviewer)

- [x] This document describes the current Round 604 source scope without a
  provider, tenant, canary, distribution, or launch claim
- [x] SSD/archive history, production services, credentials, live tenants,
  cloud infrastructure, and physical devices are excluded
- [x] `docs/reviews/` remains frozen
- [x] Exact final Jobs and Rust results are inserted from the final
  temporary-index snapshot run
- [x] Every source-testable AC1-AC30 boundary has final passing evidence
- [x] Final schema, privacy, provenance, boundary, workflow, generated UI, and
  diff gates pass
- [x] Final line-by-line and independent adversarial reviews are complete
