# Round 550 — Integrated Jobs, Browser, discovery, spend, and cross-platform release

Status: exact seal deployed; publication-ready but not yet publicly released.

Date: 2026-07-19

## Executive summary

This round converges the previously reviewed Bluey desktop, Jobs, Browser,
public-ATS discovery, provider-spend, PostgreSQL migration, and maintainability
work onto one sealed source. Exact macOS, physical Windows 11, PostgreSQL 18,
Linux-server, Jobs, and independent audit gates passed, and that seal is now
running in the controlled production beta. Public desktop publication remains
separate and has not happened yet.

The release does not silently activate new automation. Production must retain:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

The current public terminal release remains `0.1.103`; the sealed source and
production servers target `0.1.104`. Linux is a server verification target
only, not a public terminal or Bluey Browser artifact. Bluey Browser packages
are test evidence only and are not part of the public terminal release.

## Scope and sealed-source provenance

The integration history is preserved as reviewable commits above the former
`origin/main` commit `3c08c5f55e62db7d9dd00fdcf6a8347fed08be31`. The code
and artifact seal is commit
`53c258cf843599f595a1e250d1872d191638d57a`, tree
`65dabc483bea3ea3694a28fc168d5b1e9dd247d0`, with build epoch
`1784475153`. The sealed source archive is 35,634,329 bytes with SHA-256
`63e6846bdda0191cb105b61a856e28f424e8d1950a0fd46351073f37aabeaeeb`.
Every final platform bundle, database drill, deployment binary, portal archive,
and independent audit traces to this identity.

The later commit that records deployment and publication evidence in this
document is documentation-only. It must not be confused with or substituted
for the code/artifact seal embedded in binaries and archives.

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

1. Exact macOS, physical Windows, Linux-server, Jobs/portal, PostgreSQL,
   artifact, and rollback gates passed. Public signed-manifest,
   installer/updater, and website publication verification remains pending as
   the final release step.
2. Preserve the three Jobs execution/generation flags at `0` after deploy and
   through public publication.
3. Main API and Jobs API moved atomically from the same sealed source across
   migrations 008, 009, and 010 under a full-ingress outage and paid-dispatch
   hold.
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

## Remaining unknowns and product residuals

- Production backup replication to R2/S3 remains unresolved: the current R2
  credential returns `AccessDenied`. The exact pre-cutover backup was instead
  verified both remotely and after transfer to macOS.
- The owner-approved Career Track policy remains P0: role-family-scoped relevant
  months, required/preferred separation, seniority/title guards, Track-bound
  resumes/evidence, hard engagement/authorization dimensions, immutable
  evidence revisions, dual scores, per-claim evidence IDs, and transactional
  eligibility/fact checks at claim and immediately before Submit.
- Broad discovery remains P1. External feeds may supply candidate leads only
  after schema/hash/provenance checks and canonical ATS reverification; they do
  not become application truth. Anti-bot, stealth, proxy-bypass, and visual-only
  Submit behavior remain out of scope.
- Bluey Browser passed exact macOS and physical Windows controller/tray/package
  tests, but distribution and all employer-facing execution remain disabled.
  Public distribution needs its own product approval after the P0 truth gates.
- Live provider answer quality and cost still require owner-controlled canary
  prompts before managed generation can be enabled.
- Mailbox/calendar OAuth and ATS-specific execution outside the currently
  certified provider paths require provider-specific validation.
- GitHub Actions supplied no runtime signal because an account budget prevented
  every job from starting. This was explicitly waived using the equivalent or
  stronger exact local and physical platform gates; it is not recorded as a
  green GitHub check.

## PostgreSQL backup, restore, and replay evidence

The exact-seal PostgreSQL drill used PostgreSQL 18.4 and pgvector 0.8.3. It
validated fresh migration, restored migration, embedded server replay,
operator replay, migrations 008, 009, and 010, the migration 009 unique index,
and 12 targeted runtime tests. A duplicate board owner was rejected. The
local-submit authority drill produced one claim winner and denied deleted
identity, revoked entitlement, and invalid binding paths.

The local backup is:

```text
/Users/uno/Downloads/bluey-release-backups/round549/bluey-postgres-20260719T074838Z.pgdump
24,840,952 bytes
SHA-256 64508a1e615aca00983a0f78fac6b614b0704f583e39eead5a26f3821659f5a4
```

This path is an earlier isolated-drill input, not the production deploy backup.
Immediately before mutation, production created:

```text
bluey-postgres-20260719T163508Z.pgdump
24,845,155 bytes
SHA-256 6cd7272a947a88e377cdf477ba182b29ddadde277b92f77c7a92840106b7022a
```

The production backup was validated remotely and again after transfer to
macOS. Database contents remain outside this documentation. R2 replication
remains unresolved because the current credential returns `AccessDenied`.

## macOS, Windows, and Linux-server verification

- Exact macOS arm64, x86_64/Rosetta, and universal native/Rosetta terminal
  artifacts passed installed-runtime, updater, lifecycle, secure-store-off,
  and zero-residual checks. The exact Browser controller/tray/package lifecycle
  also passed; Browser remains test-only.
- Physical Windows 11 passed the exact terminal install/runtime/updater lane,
  automation 139/139, Browser 95/95, runner 50/50, sandbox/CSP/Node isolation,
  125%/200% layout, current-user key ACL, close-to-tray, relaunch, and clean
  explicit Quit.
- Exact Linux passed the main API, Jobs API, workers, Jobs portal, server
  788-test full gate, and runner 50/50. No result in this lane creates a Linux
  desktop claim.
- The independent exact diff/security/release audit found no blocker.
- GitHub Actions ran zero job steps because its account budget prevented every
  job from starting. The explicit infrastructure waiver relies on the
  equivalent or stronger exact gates above; the zero-step jobs are neither
  green checks nor product-test failures.

## Artifact, source, and binary hashes

| Input | Bytes | SHA-256 |
| --- | ---: | --- |
| Source archive | 35,634,329 | `63e6846bdda0191cb105b61a856e28f424e8d1950a0fd46351073f37aabeaeeb` |
| macOS arm64 terminal | 22,022,647 | `dfddcc30b78cd2819777ca8c8551a43443fc513bafdd630b14cbecb476892b95` |
| macOS x86_64 terminal | 23,391,529 | `bd28eb0fe683b90c070776f50e70c563cd5fa9c5cca241004d81b80de390caa4` |
| macOS universal terminal | 45,418,523 | `b842b6e31a52a749ce3011bf1717a4a0e5f321a0dd7c531bc94167ca8e6cc12c` |
| Windows x86_64 terminal | 22,617,970 | `3887013440ed1cd5c55a6656fae6ef36b6bb1dc42a0625c752e21268b4670a23` |
| Linux main API | 27,327,944 | `7704fb8d279d1d64af15646f19d14a657ed36d5144fcf89d98ac0e8bad593888` |
| Linux Jobs API | 21,364,968 | `687fc4834a08a53465bde64cb55336f931ae26acf3819139e221edb30a07bb2e` |
| Jobs portal archive | 1,279,010 | `488b0f08f15f479b291f517901f99cd3a25562634051475ad8b67274263fbf3f` |

All rows above are exact seal inputs. The local signed publication packet also
verifies before upload:

| Publication input | Bytes | SHA-256 |
| --- | ---: | --- |
| `latest.json` | 1,376 | `b97a6090d93d69c14455d63c9ff354de997f7a2abb99c8417cc81a3cf4f4f716` |
| `latest.json.sig` | 88 | `9b70ccbf7894979d818e53fb20087682d234ef83b2a895929d8dc7dcc9f1c3b9` |
| `install.sh` | 24,466 | `ced25c22f7d8cf58439bcd08e0563cb6f81d83a6ec93a3b443fdecaff1b74e57` |
| `install.ps1` | 20,778 | `74de690c5eebf6a03cac6aafb2e00ab96b410fab9db919ecfc55ef1e111481b3` |

The Ed25519 signature verifies locally. These are pre-publication identities,
not a claim that the packet is live. Browser package hashes remain non-public
test evidence and do not enter the terminal manifest.

Archive reproduction has a precise boundary: rerunning packaging from the same
final built outputs produced byte-identical archives, while independent clean
Swift helper relinks changed Apple `LC_UUID`/ad-hoc `CDHash` metadata. The Rust
CLI/daemon and picker `Info.plist` remained byte-identical. Therefore the
hash-pinned artifacts above are authoritative, but this round does not claim
fully byte-identical clean Swift relinks. Deterministic Swift linker metadata,
including evaluation of `-no_uuid`, remains a P1 packaging follow-up.

## Deployment and publication evidence

Production deployed exact seal `53c258cf843599f595a1e250d1872d191638d57a`
during a full public-ingress outage. A verified remote and macOS backup was
taken first, paid dispatch was held closed, all measured aggregates were `0`,
migrations 008, 009, and 010 plus all four data markers were verified, and the
main API, Jobs API, and Jobs portal moved as one maintenance unit.

Post-rollout identity:

| Surface | Live evidence |
| --- | --- |
| Main API | PID `2217136`; binary SHA-256 `7704fb8d279d1d64af15646f19d14a657ed36d5144fcf89d98ac0e8bad593888` |
| Jobs API | PID `2217137`; binary SHA-256 `687fc4834a08a53465bde64cb55336f931ae26acf3819139e221edb30a07bb2e` |
| Ingress / portal | Caddy PID `2217438`; `index.html` SHA-256 `309c555e40ed568a84c79e681e93756758392d3cf0762e9fc10e32fc1cb08ca3` |
| Service stability | Main and Jobs `NRestarts=0`; public health reports exact seal |
| Spend / execution | 1,000 cents per 24 hours; model/local/cloud Jobs flags all `0`; all measured aggregates `0` |

No immutable desktop artifact, signed manifest, detached signature, or public
installer has been uploaded yet. The website terminal fallback has not been
updated. Publication must upload and verify the artifacts and signed manifest
first, smoke the public installers/updater, and only then update the website.
Consequently `0.1.103` remains the public terminal release even though the
controlled production services run the `0.1.104` seal.

## Feature flags and explicit non-claims

- Managed Jobs generation is not enabled.
- Local or cloud Bluey Browser distribution is not enabled.
- Bluey Browser is not a public `0.1.104` terminal artifact.
- Linux server verification is not Linux desktop distribution.
- Imported or aggregated job rows are not canonical employment truth.
- LinkedIn, Indeed, ZipRecruiter, Dice, mailbox/calendar automation, and broad
  unattended ATS execution are not claimed as certified production paths.

## Rollback procedure

1. Stop Caddy and both APIs, verify every in-flight aggregate is `0`, and keep
   the three Jobs generation/distribution flags at `0`.
2. Before migration 008 commits, restore both saved binaries/configurations
   together and retain the original positive spend cap.
3. After migration 008 commits, never boot the old binaries against the
   migrated authority schema. A true old-version rollback requires restoring
   the verified pre-cutover PostgreSQL dump with `pg_restore --clean
   --if-exists --single-transaction`, then restoring both saved binaries and
   configurations while ingress remains stopped.
4. Verify the migration ledger is back at 001-007, both old binary hashes are
   exact, temporary zero-cap holds are absent, and the original positive cap is
   effective before restarting Caddy.
5. Once public ingress has reopened, treat the database rollback window as
   closed because a restore would discard post-cutover writes; fix forward.
6. Restore the prior Jobs portal entrypoint only from the paired rollback
   packet and retain immutable hashed assets.
7. Restore a previous signed desktop manifest only from immutable release
   inputs; never overwrite versioned artifacts.

## Final acceptance checklist

- [x] Code/artifact commit sealed and its build worktree clean.
- [x] Root and server fmt, clippy, all-target tests, and release builds pass.
- [x] All Jobs packages pass tests, typechecks, and production builds.
- [x] PostgreSQL fresh/restore/operator/replay/authority gates pass on the
      sealed commit and the backup hash is recorded in full.
- [x] macOS arm64, x86_64, and universal terminal artifacts pass installed
      runtime and updater checks with secure stores disabled.
- [x] macOS Bluey Browser package passes controller, tray, close, relaunch,
      intervention, and explicit-Quit checks.
- [x] Physical Windows 11 terminal and Browser packages pass equivalent gates.
- [x] Linux server/API/Jobs/workers/portal gate passes with no desktop claim.
- [x] Source and artifact identities plus bounded archive-repack evidence and
      the independent-clean-Swift-relink limitation are recorded.
- [x] Independent diff/security/release audit has no blocker.
- [x] Exact integrated seal reaches `origin/main`.
- [x] Production backup and rollback inputs are verified.
- [x] Main API and Jobs API deploy atomically with all Jobs flags at `0`.
- [ ] Immutable desktop artifacts and signed manifest publish before the
      `0.1.104` website fallback.
- [x] Live health, versions, hashes, flags, and `NRestarts` are verified.
- [x] Round 549 and this round are updated with exact pre-publication rollout
      evidence.
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

Completed on the exact code/artifact seal:

- root and server formatting, all-target tests, clippy with warnings denied,
  and release builds; server full gate 788 tests, including 706 library tests;
- all Jobs package suites, typechecks, and builds; physical Windows automation
  139/139, Browser 95/95, and runner 50/50; exact macOS and Linux runner 50/50;
- exact macOS terminal and Browser installed-runtime/package lifecycle gates;
- exact physical Windows 11 terminal and Browser installed-runtime/package
  lifecycle gates;
- exact Linux main API, Jobs API, workers, and portal build/runtime gates;
- PostgreSQL 18.4 + pgvector 0.8.3 fresh and restored migration drills,
  operator and embedded replay, migration 009 uniqueness, local-submit
  authority, and 12 targeted runtime tests;
- artifact/source identity checks, independent diff/security/release audit,
  pre-mutation backup verification, atomic production rollout, and post-rollout
  health/hash/flag/restart verification.

GitHub Actions is covered only by the documented infrastructure waiver: every
job had zero steps because the account Actions budget prevented it from
starting. No GitHub job is represented as green.

Still required for public release: sign and publish the immutable desktop
manifest and artifacts, verify the public installers and updater, update the
website terminal fallback afterward, record the resulting hashes/evidence, and
send the continuation handoff. Until then, deployment is live but `0.1.104`
remains unpublished.
