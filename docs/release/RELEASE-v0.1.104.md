# Bluey Release v0.1.104

Release candidate: 2026-07-19

Audience: controlled production beta

Status: verification in progress; not yet published

## Sealed Source

The release source commit is not sealed yet. The final commit must contain this
version bump and release record, have a clean worktree, and be identical to the
source archive used by every macOS, Windows, Linux-server, and deployment gate.
The exact commit and source-archive SHA-256 will be recorded here before
publication.

## Summary

`0.1.104` integrates the Jobs truth, discovery, browser-controller, submit
authority, provider-spend, PostgreSQL migration, and maintainability work that
was intentionally held out of `0.1.103`. It also avoids an unnecessary macOS
administrator prompt when an update already has correct public command links.

No item in this release note enables employer-facing execution by itself.
Managed Jobs generation, local Browser distribution, and cloud Browser
distribution remain separate production flags and must stay disabled until
their own canaries and operator approvals pass.

## Desktop

- The updater skips `sudo` only when every public command symlink already
  resolves to the fixed Bluey installation directory.
- Public packages target macOS Apple silicon, macOS Intel, macOS universal, and
  Windows x86-64. There is no public Linux desktop artifact in this release.
- Release verification runs with all four secure-store switches set to `0`:
  `BLUEY_USE_OS_KEYCHAIN`, `BLUEY_USE_SECURE_STORE`,
  `BLUEY_LEGACY_KEYRING_FALLBACK`, and `BLUEY_ALLOW_PLAINTEXT_TOKENS`.

## Bluey Jobs

- Verified public ATS discovery supports Greenhouse, Lever, Ashby,
  SmartRecruiters, and Workday through host-pinned, redirect-free, bounded
  readers in `jobs/automation/src/public-ats.ts`.
- Scheduled sources use canonical board ownership and publish only complete
  snapshots. A failed or partial read cannot erase the previous authoritative
  snapshot.
- Managed resume generation stays factual, lease-fenced, spend-bounded, and
  default-off. Deterministic evidence composition remains the fail-closed
  fallback.
- The owner-approved Career Track policy is not complete in this release:
  relevant experience is not yet role-family scoped, required and preferred
  experience are not separated, and profile fit is not persisted separately
  from tailored packet coverage. This is a P0 blocker before any unattended
  Jobs submission or Browser distribution, but not before the terminal/server
  release while all three Jobs flags remain `0`.
- Local Browser deliveries carry separate result, resume, and final-submit
  capabilities. Immediately before every irreversible click, the Browser asks
  the server to recheck current entitlement, verified identity, exact binding,
  and provider-final-review approval.
- A crash after the durable final-submit marker or click remains
  `side_effect_unknown` and is never automatically retried.

## Bluey Browser Controller

- The local renderer is sandboxed, Node-disabled, and protected by a strict
  Content Security Policy. Sensitive application packets and capabilities do
  not enter renderer or notification payloads.
- Background operation is explicit and reversible, with macOS menu-bar and
  Windows tray controls, pause/resume, intervention notifications, safe close
  to tray, and explicit Quit.
- Local copy says work continues only while the computer is online, awake, and
  unlocked. Only an entitled cloud runner may claim computer-off operation.
- Dedicated multi-resolution Browser icons are packaged for macOS, Windows,
  Linux build tooling, and monochrome tray use. Bluey Browser itself remains a
  separately gated, undistributed beta surface.
- Windows secure-store opt-out uses a random local file key protected by a
  current-user-only ACL. Electron `safeStorage`/DPAPI is not loaded unless an
  operator explicitly opts in.

## Provider Spend And PostgreSQL

- Every managed paid route reserves a durable projected provider hold before
  upstream I/O and settles with explicit `exact`, `estimated`, or `missing`
  usage provenance.
- Only exact, route-matching usage may reduce a projection. Missing,
  ambiguous, estimated, or route-mismatched usage preserves the conservative
  hold; an exact over-projection is persisted before the request fails closed.
- Migration 008 preserves an anonymous, bounded pre-cutover spend baseline so
  a provenance migration cannot make the global cap appear to reset to zero.
- Operator and embedded PostgreSQL migrations now share discoverable target
  markers. Physical migration 009 owns the discovery-board uniqueness
  boundary; migration 010 owns provider-usage provenance.
- Main API and Jobs API binaries must move together across this accounting
  boundary. Paid routes are disabled and drained before rollout or rollback.

## Maintainability

- The previous oversized Jobs persistence module is split into profile,
  discovery, eligibility, application, customer-data, local-runner,
  execution-lease, and workspace domains.
- The previous oversized answer router is split into provider runtime, answer
  planning, behavioral grounding, domain intents, prompt contracts, Web
  search, streaming completion, and completion modules.
- The pre-split source was reconstructed byte-for-byte during the refactor
  audit before the new module layout was accepted.

## Public Platform Artifacts

| Platform | Status | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| macOS arm64 | Exact-candidate build pending | — | — |
| macOS x86_64 | Exact-candidate build pending | — | — |
| macOS universal | Exact-candidate build pending | — | — |
| Windows x86_64 | Exact-candidate build pending | — | — |
| Linux desktop | Not published | — | — |

An artifact row is replaced with exact bytes and SHA-256 only after its
packaged install and runtime smoke passes. Bluey Browser package hashes are
test evidence only and do not enter the public terminal manifest.

## Installed-Runtime And Linux-Server Verification

Exact-candidate installed-runtime results are pending. macOS must cover the
arm64, x86_64/Rosetta, and universal archives; Windows must run in an
interactive Windows 11 desktop. Each lane must prove version and executable
identity, secure-store opt-out, overlay/controller visibility, capture/privacy
state, clean lifecycle, and zero residual processes.

Linux verification covers the main API, Jobs API, worker packages, and Jobs
portal against the sealed source. It does not authorize or advertise a Linux
desktop package.

## PostgreSQL Recovery Proof

Pre-seal drills passed on PostgreSQL 18.4 with pgvector 0.8.3 for fresh and
restored databases, operator migration discovery, embedded replay, migration
009 uniqueness, and local submit authority. The exact sealed source must rerun
the focused migration and authority gates before deployment. The local backup
identity is recorded in Round 550; no customer rows are copied into evidence.

## Release Boundary

Publication remains blocked until all of these exact-candidate gates pass:

- Rust formatting, all-target tests, clippy with warnings denied, and release
  builds for the root workspace and server;
- Jobs automation, Browser, runner, workflows, and portal tests, typechecks,
  and production builds;
- PostgreSQL 18 fresh, restored, operator migration, replay, uniqueness,
  retention, and local-submit authority drills;
- packaged macOS install/start/update/stop and packaged Bluey Browser
  controller/tray verification;
- physical Windows terminal package/install/runtime and Bluey Browser
  package/controller/tray verification;
- Linux server/API/Jobs/workers/portal verification without claiming a Linux
  desktop package; and
- artifact checksum, source-commit, signed-manifest, rollback, and live health
  verification.

Exact commit, artifact sizes, hashes, and publication evidence will be added
only after those gates pass. Until then, `0.1.103` remains the public release.
The `0.1.104` website fallback must not be deployed before its immutable
artifacts and signed manifest are live.

## Security And Privacy

- No credentials, prompts, resumes, job descriptions, customer data, restored
  database rows, or Browser application packets belong in release artifacts or
  evidence.
- Public ATS imports are candidate evidence, not permission to fabricate facts
  or submit. Canonical source verification and current policy still control
  readiness.
- CAPTCHA, 2FA, assessment, missing-fact, authorization, legal, and uncertain
  submit states pause for visible user intervention.
- No source analysis or recovered third-party code changes Bluey's ownership,
  consent, security, or provenance requirements.

## Disabled Flags And Non-Claims

- `BLUEY_JOBS_MODEL_GENERATION_ENABLED=0`
- `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0`
- `BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0`
- Bluey Browser is tested but not publicly distributed.
- There is no public Linux desktop artifact.
- Mailbox/calendar OAuth and broad unattended ATS execution are not claimed.

## Deployment, Publication, And Rollback

Deployment identity, service stability, signed-manifest hash, signature hash,
installer hashes, updater smoke, and live artifact checks are pending. Rollout
must disable and drain paid dispatch, apply migrations 008 and 010, atomically
replace the main API and Jobs API, verify the new build with the three Jobs
flags still `0`, and only then restore independently approved paid routes.

Rollback is symmetric: disable and drain paid dispatch first, roll back both
server binaries together, and never run an older paid dispatcher against the
authority cutover merely because it boots. The previous `0.1.103` terminal
artifacts and previous approved Jobs binary remain immutable rollback inputs.

## Related Evidence

- `docs/rounds/ROUND-549-JOBS-TRUTH-SPEND-ATOMIC-ROLLOUT.md`
- `docs/rounds/ROUND-550-INTEGRATED-JOBS-BROWSER-DISCOVERY-SPEND-AND-CROSS-PLATFORM-RELEASE.md`
- `jobs/OPERATIONS.md`
- `jobs/browser/README.md`
- `infra/postgres/README.md`
