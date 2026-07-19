# Round 550 — Integrated Jobs, Browser, discovery, spend, and cross-platform release

Status: release candidate verification in progress; not deployed or published.

Date: 2026-07-19

## Executive summary

This round converges the previously reviewed Bluey desktop, Jobs, Browser,
public-ATS discovery, provider-spend, PostgreSQL migration, and maintainability
work onto one release branch. Production remains unchanged while the exact
candidate is tested on macOS, a physical Windows 11 machine, PostgreSQL 18, and
a Linux server environment.

The release does not silently activate new automation. Production must retain:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

The current public terminal release is `0.1.103`; this source targets
`0.1.104`. Linux is a server verification target only, not a public terminal or
Bluey Browser artifact.

## Scope and sealed-source provenance

The integration history is preserved as reviewable commits above the last
observed `origin/main` commit `3c08c5f55e62db7d9dd00fdcf6a8347fed08be31`.
The release source commit is not sealed yet; the exact final commit must include
the `0.1.104` version metadata and this evidence scaffold with a clean
worktree. All final platform bundles, database drills, the source archive, and
deployment binaries must trace to that one commit. A stale branch, rebuilt
post-test artifact, or undocumented worktree delta is not a release input.

## Evidence-backed findings

### Public ATS discovery

- `jobs/automation/src/public-ats.ts:65` defines the shared discovery provider;
  its provider branches begin at lines 155-160 and cover Greenhouse, Lever,
  Ashby, SmartRecruiters, and Workday.
- `jobs/workflows/src/discovery-provider.ts:124-194` converts persisted Ashby,
  SmartRecruiters, and Workday source configurations into pinned public-ATS
  requests.
- `server/src/db/jobs/discovery.rs:123-195` admits only the five supported
  provider kinds and builds typed source configuration.
- `infra/postgres/server-runtime/009_jobs_discovery_board_owner.sql:6` creates
  the unique provider-board ownership boundary.
- The publication code preserves the last authoritative snapshot when a read
  is incomplete, invalid, or loses its current board ownership. Imported rows
  remain source evidence, not proof that a submission is eligible.

### Final-submit authority

- `server/src/api/jobs.rs:207-208` registers the scoped authorize-submit
  endpoint, and `server/src/api/jobs.rs:2840` begins its live authorization
  handler.
- `server/src/api/jobs.rs:2809-2835` issues a final-submit capability distinct
  from result and resume capabilities.
- `jobs/browser/src/authorized-final-submit.ts:64` sends only the scoped
  capability to that endpoint immediately before the irreversible boundary.
- `jobs/browser/src/run-controller.ts:216-223` records the durable marker and
  treats uncertain post-click state as `side_effect_unknown`.
- `jobs/browser/src/checkpoint-recovery.ts:54-102` restores uncertain work for
  reconciliation rather than automatic retry.

### Browser product shell and background truth

- `jobs/browser/src/browser-shell.ts:34-86` owns the controller, tray, and safe
  notification surfaces.
- `jobs/browser/src/controller-state.ts:45` states the local boundary as
  "Runs quietly while this computer is awake."
- `jobs/browser/src/renderer/controller.html:66-78` exposes the same truthful
  background wording in the sandboxed renderer.
- `jobs/browser/src/tray-controller.ts:12-56` creates the tray and reports
  background state without treating it as execution authority.
- `jobs/browser/src/local-checkpoint-store.ts:432-439` loads Electron
  `safeStorage` only when an explicit secure-store flag is enabled.
- `jobs/browser/tests/local-checkpoint-store.test.ts:130-173` proves the opt-out
  path makes zero `safeStorage` calls.

### Spend truth and recovery

- `server/src/api/router/provider_cost_guard.rs:59` reserves a durable cost
  hold before managed provider work; settlement paths begin at lines 183-245.
- `server/src/db/jobs_provider_cost_holds.rs:295-609` implements fail-closed
  reservation for SQLite and PostgreSQL, including the global cap and cutover
  baseline.
- `server/src/db/jobs_provider_cost_holds.rs:645-915` validates route/usage
  identity and settles without allowing ambiguous usage to erase exposure.
- `server/src/db/usage_reservations.rs:188-217` derives expiry decisions from
  database transaction time, not caller time.
- `server/src/db/mod.rs:1431-1463` defines the anonymous cutover baseline and
  durable provider holds; lines 1523-1564 apply the replay-safe cutover.
- `infra/postgres/server-runtime/010_provider_usage_provenance.sql:6-22` adds
  and validates provider usage provenance independently of the earlier schema.

### Maintainability

- `server/src/db/jobs.rs` is now a small module boundary over focused files in
  `server/src/db/jobs/` instead of a 12,000-line persistence unit.
- `server/src/api/router.rs` is now a small module boundary over focused files
  in `server/src/api/router/` instead of a 12,000-line answer-routing unit.
- Before accepting each split, the prior source was reconstructed from the new
  modules and compared byte-for-byte. The Jobs reconstruction SHA-256 was
  `addf949d0b64abe064bcb2dee6745bab451953ca64e3a0c406ad0c31c198c35d`;
  the answer-router reconstruction SHA-256 was
  `afd6625c4eedc21cf371c553427e74d6bcfa6e9f4b6cc871322c019534b2db5e`.

## Cross-application feature matrix

| Capability | Bluey `0.1.104` source | Release boundary |
| --- | --- | --- |
| Continuous, consent-first work context | Implemented in desktop | User-controlled capture; no covert claim |
| Local meeting detection | Implemented | Detection never starts recording |
| Project/session memory | Implemented | Owner-private and bounded |
| Public ATS discovery | Five typed providers | Candidate evidence; source verification required |
| Broad job-board aggregation | Partial | External feeds remain future candidate-only inputs |
| Resume tailoring | Evidence-bound, deterministic fallback | Managed generation remains production-off |
| Profile fit versus packet coverage | Missing | P0 before any unattended submission |
| Local application runner | Implemented behind distribution gate | Production flag remains off |
| Cloud application runner | Architecture only | Production flag remains off |
| Background controller/tray | Implemented for macOS/Windows | Explicit local opt-in; awake/online/unlocked only |
| CAPTCHA/2FA/assessment handling | Visible intervention | Never bypassed |
| Final-submit safety | Live server reauthorization | Denial before marker/click; unknown is no-retry |
| Email/calendar outcomes | Not production-enabled | OAuth/provider certification required |
| Global provider spend cap | Implemented | Positive configured cap required for paid routes |
| Linux desktop distribution | Missing by policy | No public artifact or claim |

## Bluey code references

The primary implementation seams are:

- discovery: `jobs/automation/src/public-ats.ts`,
  `jobs/workflows/src/discovery-provider.ts`, and
  `server/src/db/jobs/discovery.rs`;
- browser execution: `jobs/browser/src/run-controller.ts`,
  `jobs/browser/src/authorized-final-submit.ts`, and
  `server/src/api/jobs.rs`;
- controller UX: `jobs/browser/src/browser-shell.ts`,
  `jobs/browser/src/controller-state.ts`, and
  `jobs/browser/src/renderer/`;
- spend authority: `server/src/api/router/provider_cost_guard.rs`,
  `server/src/db/jobs_provider_cost_holds.rs`, and
  `server/src/db/usage_reservations.rs`;
- migrations: `infra/postgres/server-runtime/008_jobs_generation_allowance.sql`,
  `009_jobs_discovery_board_owner.sql`, and
  `010_provider_usage_provenance.sql`.

## Career Track and tailoring policy audit

The audit at source commit `40eb10026d478c68036b336e828399036970da8a`
found that the release remains fail-closed but does not yet implement the
owner's complete policy:

| Rule | Status | Evidence |
| --- | --- | --- |
| Non-overlapping relevant months | Partial | `server/src/db/jobs/eligibility.rs:735` merges overlaps globally, not per Track/role family |
| Default -1/+2-year window | Implemented numerically | `server/src/db/jobs/eligibility.rs:766`; `server/src/db/jobs/tests.rs:141` |
| Required vs preferred experience | Contradicted | `server/src/db/jobs/eligibility.rs:786` takes one maximum from all prose |
| Independent seniority guard | Partial | `server/src/db/jobs/eligibility.rs:815`; explicit years suppress the Staff floor |
| Track role/resume isolation | Partial | `server/src/db/jobs/applications.rs:213`; account-global profile is loaded |
| Employment/engagement/auth hard filters | Partial | `server/src/db/jobs/eligibility.rs:309-634`; W2/C2C/1099 and work-authorization eligibility are absent |
| Immutable evidence graph | Partial | `server/src/db/jobs/profile_postings.rs:209-458`; facts remain mutable/deletable |
| Evidence-backed synthesis | Partial | `server/src/api/jobs_resume_generation.rs:1102-1384` safely reorders exact evidence but cannot synthesize validated wording |
| Claim-to-evidence IDs | Partial | `server/src/api/jobs_resume_generation.rs:54`; IDs are transient positions, not persisted per claim |
| Separate base-fit/packet-coverage scores | Missing | `server/src/db/jobs.rs:325`; only `match_score` exists |
| Live current-policy recheck before Submit | Partial | `server/src/db/jobs/local_runner.rs:300-547`; binding/identity are checked, current eligibility is not |

The seven acceptance examples are not all satisfied. In particular, a two-year
candidate can incorrectly pass a Staff posting that explicitly says four
years; general software months are not isolated from data-engineering months;
EKS evidence cannot yet support a validated Kubernetes rewrite; and a 70% base
fit versus 90% packet coverage cannot be represented. Snowflake without
evidence is safely excluded, and the current extractive generator cannot invent
a client, employer, title, or unsupported skill.

This does not block `0.1.104` while ATS capability never becomes `certified`,
`can_auto_submit` remains false (`server/src/db/jobs/eligibility.rs:450`), and
the model/local/cloud flags remain `0`. It is a strict P0 before enabling any
unattended Browser distribution or submission. The smallest follow-up is a
versioned eligibility decision with Track-scoped relevant months, structured
required/preferred requirements, role/seniority/engagement/auth dimensions,
immutable evidence revisions, dual scores, per-claim evidence IDs, and a
transactional current-policy recheck at claim and immediately before Submit.

## Recommended P0/P1/P2 changes

### P0 — release and truth gates

1. Finish the exact macOS, physical Windows, Linux server, Jobs/portal,
   PostgreSQL, artifact, installer, updater, and rollback verification listed
   in `docs/release/RELEASE-v0.1.104.md`.
2. Preserve the three Jobs execution/generation flags at `0` during deploy.
3. Prove main API and Jobs API use the same migration/accounting source and
   move atomically across migrations 008 and 010.
4. Implement the audited Career Track/experience/tailoring gaps. Do not enable
   automatic submission while profile eligibility and tailored packet coverage
   are conflated or current eligibility is not rechecked before Submit.

### P1 — broad discovery without false authority

1. Add a source catalog and candidate landing zone for licensed/public feeds.
   Reverify every top candidate against the canonical employer or ATS page.
2. Keep candidate, live-verified, closed, and invalid states explicit; preserve
   source hash, observed time, and closure evidence.
3. Extend typed discovery adapters by provider rather than creating bespoke
   scrapers per staffing company.
4. Add source-health metrics for freshness, checksum/schema drift, row deltas,
   reverification rate, and zero-row regression.

### P2 — provider certification and outcomes

1. Certify execution per ATS and version with fixtures, canaries, drift
   monitoring, durable leases, intervention takeover, and complete receipts.
2. Add email/calendar outcome ingestion only after provider OAuth review,
   least-privilege scopes, deletion coverage, and account-level consent.
3. Benchmark screenshot grounding only as a no-submit fallback. Typed DOM and
   semantic DOM remain ahead of visual grounding; visual grounding alone never
   authorizes final Submit.

## Reuse and provenance notes

The implementation is Bluey-owned source. Reference applications, recovered
packages, public repositories, and external feeds informed architecture and
test cases, but do not become application truth or waive provenance review.
No minified third-party bundle should be pasted into the shipping repository.
External job data must retain its source, observation time, canonical URL, and
license/usage boundary even when a repository's code license is permissive.

## Security and privacy findings

- Browser packets, credentials, answers, resumes, and capabilities do not enter
  controller renderer or OS notification text.
- Browser navigation and ATS reads remain host-pinned, redirect-free, bounded,
  and fail closed.
- The release test environment disables Keychain, Credential Manager/DPAPI,
  legacy keyring fallback, and plaintext tokens. Private file keys retain
  owner-only permissions or current-user-only Windows ACLs.
- Provider spend truth survives account deletion only as bounded anonymous or
  pseudonymous operational evidence; it is not customer billing or analytics.
- Employer-facing side effects require current server authority, and uncertain
  final-submit state cannot enter an automatic retry loop.

## Unknowns requiring runtime validation

- Exact `0.1.104` Windows terminal package and updater behavior on the physical
  Windows 11 machine.
- Exact frozen-candidate Browser close-to-tray, relaunch, and explicit-Quit
  lifecycle on macOS and Windows.
- Linux build/runtime behavior for the server, Jobs API, workers, and portal;
  this does not create a Linux desktop release.
- Production backup replication to R2/S3; the last observed R2 credential
  returned `AccessDenied`, while local and off-host PostgreSQL backups existed.
- Live provider answer quality and cost under owner-controlled canary prompts.
- Mailbox/calendar OAuth and ATS-specific execution outside the currently
  certified provider paths.

## PostgreSQL backup, restore, and replay evidence

The pre-seal PostgreSQL drill used PostgreSQL 18.4 and pgvector 0.8.3. It
validated fresh migration, restored migration, embedded server replay,
operator replay, and the exact migration 009 unique index. A duplicate board
owner was rejected. The local-submit authority drill produced one claim winner
and denied deleted identity, revoked entitlement, and invalid binding paths.

The local backup is:

```text
/Users/uno/Downloads/bluey-release-backups/round549/bluey-postgres-20260719T074838Z.pgdump
24,840,952 bytes
SHA-256 64508a1e615aca00983a0f78fac6b614b0704f583e39eead5a26f3821659f5a4
```

This path is local evidence, not a deploy input. The final gate must prove the
backup can restore and keep database contents out of documentation. R2
replication remains unresolved because the last credential check returned
`AccessDenied`.

## macOS, Windows, and Linux-server verification

- macOS pre-seal Browser package and controller checks passed, but the final
  terminal and Browser packages must be rebuilt from the sealed commit.
- Physical Windows 11 passed the pre-seal Rust suites and the Browser's 24
  files / 90 tests, package, sandbox/CSP/Node isolation, 125%/200% layout, and
  current-user key ACL. Final terminal and Browser packages remain required.
- Linux must build and run the main API, Jobs API, workers, and portal from the
  sealed source. No result in that lane creates a Linux desktop claim.

## Artifact, source, and binary hashes

Pending candidate seal and exact-platform verification. This section will
record the source archive, four terminal artifacts, deployed main API binary,
deployed Jobs API binary, signed manifest, detached signature, and installer
hashes. Browser package hashes remain non-public test evidence.

## Deployment and publication evidence

No deployment, restart, feature-flag change, release upload, manifest change,
or public website update has occurred in this round. The last read-only
production observation showed the approved Jobs commit
`f3a0a04360febb36363c27f869e954f2d61f32e0`, binary SHA-256
`07e12cb5d5668c4c8cd24129a9fbccc3f512e873d2400867b0fde55be288ac96`,
active with zero restarts and all three Jobs generation/distribution flags at
`0`. These values must be reverified immediately before rollout.

The website contains a `0.1.104` fallback and therefore must not be deployed
before the immutable `0.1.104` artifacts and signed manifest are live.

## Feature flags and explicit non-claims

- Managed Jobs generation is not enabled.
- Local or cloud Bluey Browser distribution is not enabled.
- Bluey Browser is not a public `0.1.104` terminal artifact.
- Linux server verification is not Linux desktop distribution.
- Imported or aggregated job rows are not canonical employment truth.
- LinkedIn, Indeed, ZipRecruiter, Dice, mailbox/calendar automation, and broad
  unattended ATS execution are not claimed as certified production paths.

## Rollback procedure

1. Disable every managed paid dispatcher and drain in-flight provider work.
2. Keep the three Jobs generation/distribution flags at `0`.
3. Restore the previous approved main API and Jobs API binaries together; do
   not mix an older paid dispatcher with the authority-cutover schema.
4. Verify database migrations remain additive and the prior binaries do not
   dispatch paid work before they are re-enabled.
5. Restore the previous signed desktop manifest only by its immutable release
   inputs; never overwrite versioned artifacts.
6. Verify health, binary hashes, source identity, and service restart counts.

## Final acceptance checklist

- [ ] Candidate commit sealed and worktree clean.
- [ ] Root and server fmt, clippy, all-target tests, and release builds pass.
- [ ] All Jobs packages pass tests, typechecks, and production builds.
- [ ] PostgreSQL fresh/restore/operator/replay/authority gates pass on the
      sealed commit and the backup hash is recorded in full.
- [ ] macOS arm64, x86_64, and universal terminal artifacts pass installed
      runtime and updater checks with secure stores disabled.
- [ ] macOS Bluey Browser package passes controller, tray, close, relaunch,
      intervention, and explicit-Quit checks.
- [ ] Physical Windows 11 terminal and Browser packages pass equivalent gates.
- [ ] Linux server/API/Jobs/workers/portal gate passes with no desktop claim.
- [ ] Source and artifact reproducibility/hashes are recorded.
- [ ] Independent diff/security/release audit has no blocker.
- [ ] Origin main has not advanced incompatibly; integrated work reaches main.
- [ ] Production backup and rollback inputs are verified.
- [ ] Main API and Jobs API deploy atomically with all Jobs flags at `0`.
- [ ] Immutable desktop artifacts and signed manifest publish before the
      `0.1.104` website fallback.
- [ ] Live health, versions, hashes, flags, and `NRestarts` are verified.
- [ ] Round 549 and this round are updated with exact rollout evidence.
- [ ] The continuation Codex receives the final main/deploy handoff.

## Concrete implementation handoff

The next agent must start from the final main commit recorded here after
publication, verify a clean worktree and live flags, and must not resurrect a
stale branch. Its first Jobs work item is the owner-approved Career Track and
tailoring policy: non-overlapping relevant months, a default -1/+2-year window,
required-versus-preferred experience, seniority and role-family guards, hard
employment/authorization filters, immutable evidence provenance, separate
profile-fit and tailored-packet-coverage scores, and server-owned auto-submit
eligibility. That work needs schema, API, scoring, portal, and acceptance tests
before any execution flag changes.

## Verification ledger

Completed before candidate seal:

- Jobs workspace: 371 tests across automation, Browser, runner, workflows, and
  portal, plus typecheck/build gates.
- Browser after Windows secure-store, package-pruning, and close-copy fixes: 25
  files / 95 tests on macOS and physical Windows.
- Server library: 706 tests in serial, with the spend-retention boundary made
  robust against SQLite timestamp rounding and repeated ten times.
- PostgreSQL 18.4 + pgvector 0.8.3: fresh and restored migration drills,
  operator replay, migration 009 uniqueness, and local submit authority.
- Physical Windows Browser package smoke from the pre-version-bump tree,
  including sandbox/CSP/Node isolation, 125% and 200% layout, current-user key
  ACL, zero secure-store calls, real WM_CLOSE hide/show when background is on,
  and clean Quit when it is off.

Still required on the sealed source commit:

- final full parallel Rust/server and Jobs suites;
- final macOS terminal and Browser packages and runtime smokes;
- final physical Windows terminal and Browser packages and runtime smokes;
- final Linux server/API/Jobs/workers/portal gate;
- exact artifact hashes, reproducibility checks, signed manifest, rollout,
  rollback readiness, and live health/NRestarts verification.

No deployment or publication is authorized until every required item above is
green and this document is updated with the exact source and artifact hashes.
