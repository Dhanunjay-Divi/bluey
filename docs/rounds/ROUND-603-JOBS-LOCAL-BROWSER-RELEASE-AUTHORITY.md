# Round 603 - Jobs Local Browser Release Authority

**Date:** 2026-08-05
**Branch:** `feat/phase-603-jobs-local-browser-release-authority`
**Status:** Source-complete locally; external native/hosting/device gates remain pending and distribution remains disabled

## Objective

Make a packaged local Bluey Browser release an explicit server authority before
the local distribution flag can ever be enabled. A generic download link,
Electron package version, client-reported string, or possession of a launch
ticket must not authorize an unsupported, downgraded, or revoked Browser build.

## Authority Model

- A root-authorized Ed25519 release key signs one canonical immutable build
  descriptor per target before packaging. The descriptor is channel-neutral:
  it binds only intrinsic build identity such as product and release IDs,
  Browser build ID, protocol version, source commit and epoch, target
  OS/architecture, application ID, Electron version, Playwright version,
  Chromium revision, issuance time, and signing-key ID. It does not bind a
  beta/stable channel, activation generation, or channel sequence, so one exact
  artifact can move from candidate verification to production without rebuild.
- The exact descriptor and detached signature are packaged as app resources.
  The Browser validates their strict shape and signature before it may claim a
  new local run. It sends base64url-encoded exact canonical descriptor bytes
  plus the detached signature, never a free-form version assertion or a parsed
  object that either Node or Rust could reserialize differently.
- A separately signed release manifest is created after packaging. It binds
  every platform descriptor digest to the exact immutable artifact URL, byte
  length, SHA-256, package kind, release notes, build/protocol identity, and
  source commit. The immutable manifest is also channel-neutral. It cannot
  contain its own artifact hash and therefore stays outside the artifact; the
  descriptor digest joins the installed build to the server-authorized artifact
  record without a self-referential archive.
- A separate signed channel activation selects an exact manifest digest for a
  channel and binds the trust-policy generation, monotonically increasing
  sequence, publication and expiry times, exact accepted runtime releases, and
  native verification/canary evidence. Candidate-to-stable promotion changes
  only this signed authority record and promotes the same stored bytes.
- PostgreSQL/SQLite store immutable signed descriptors, manifests, artifacts,
  signature sets, activations, rollbacks, and append-only revocations. Ordinary
  activation moves only forward. A downgrade requires a new higher-sequence
  signed rollback authority binding the exact current activation digest, exact
  target manifest digest, reason code, trust generation, compatibility/canary
  evidence, and signing key. Byte-identical replay is idempotent; a conflicting
  replay or revoked target fails closed.
- A root-signed trust policy assigns separate release, promotion, and incident
  roles, signature thresholds, generation bounds, and `active`, `retired`, or
  `revoked` key state. Root rotation requires the prior and successor root
  thresholds. Retired keys may verify historical immutable records but cannot
  authorize a later activation. An incident-role revocation is independently
  signed, audience-bound, append-only, exact, and irreversible; it may revoke a
  key, descriptor, manifest, release, or artifact digest. A higher root trust
  generation recovers safely even if a compromised delegated key emitted an
  artificially large channel sequence.
- Revocation blocks installation, activation, rollback targeting, new claims,
  and future pre-click final-submit authorization immediately. A claim verifies
  the exact signed descriptor, active manifest artifact, activation, trust
  policy, and absence of revocation inside the same transaction that consumes
  the one-time ticket, then freezes the release, manifest, build, protocol, and
  target binding onto the run and its server-issued capabilities.
- Existing result/resume and reconciliation authority remains usable after a
  later release change so an update cannot strand `click_started`,
  `side_effect_unknown`, or a trusted receipt. A revoked build cannot begin a
  new run or cross a future pre-click final-submit authorization boundary.
- This is cooperative software identity, not remote hardware attestation. Code
  signing/notarization, exact package verification, immutable public read-back,
  and physical-device install/protocol/update/rollback canaries complete the
  rollout evidence.

## Artifact Contract

- Package once, inspect and smoke that exact application directory, then hash
  and preserve the exact installer/archive. Do not rebuild between verification
  and promotion.
- Release targets are explicit macOS arm64, macOS x64, and Windows x64 packages.
  Bluey Browser does not claim a universal macOS package: the current Playwright
  installation supplies one target-architecture Chromium bundle, so a universal
  Electron shell would not prove a universal browser runtime.
- `app.asar` must contain the compiled Browser entry and compiled automation
  dependency. It must not contain Bluey source, tests, fixtures, maps, local
  environment files, credentials, stale descriptors, or extra Chromium
  revisions.
- The application ID, product name, protocol registration, descriptor bytes,
  Electron version, and matching Playwright Chromium revision must agree with
  the signed descriptor and release manifest.
- Artifact URLs are immutable HTTPS paths. Mutable aliases may point users to a
  manifest, but the signed manifest never points to a mutable installer name.
- Every signed object uses deterministic canonical UTF-8 bytes and an explicit,
  unique signature audience. Verification checks the exact bytes and digest
  before parsing, rejects noncanonical encoding and unknown fields, and never
  permits a signature from one descriptor, manifest, activation, rollback,
  revocation, or trust-policy audience to authorize another.
- Electron Builder updater metadata is never independent authority. If retained
  for installer compatibility, it is an auxiliary artifact pinned by the
  canonical manifest; Browser download and update decisions trust only the
  signed trust policy, activation, manifest, descriptor, and exact artifact.

## Acceptance Criteria

1. Node and Rust share exact canonical descriptor, manifest, activation,
   rollback, revocation, and trust-policy vectors. Tampering, unknown or
   wrong-role keys, noncanonical encoding, unknown fields, and signature replay
   across audiences fail closed.
2. A channel-neutral descriptor binds exact intrinsic build/protocol, source,
   target, application, Electron, Playwright, and Chromium identity. A
   channel-neutral manifest binds that descriptor digest to the immutable URL,
   byte length, SHA-256, package kind, release notes, and native evidence for
   each exact artifact.
3. A separately signed activation binds channel, trust generation, monotonic
   sequence, exact manifest and signature-set digests, accepted runtime release
   IDs, expiry, and canary evidence. Promotion never rebuilds or modifies the
   candidate artifact or its immutable manifest.
4. Immutable descriptor, manifest, and signature-set replay succeeds. A
   same-identity conflict, duplicate target artifact, descriptor reuse under
   conflicting bytes, trust-generation regression, or missing/extra artifact
   fails closed.
5. Root and delegated key rotation preserves historical verification while
   preventing retired keys from authorizing newer state. Append-only signed
   revocation is exact and idempotent, cannot be reversed, and prevents a
   revoked key, release, manifest, or artifact from being newly activated or
   selected as a rollback target.
6. Ordinary activation is monotonic. Downgrade requires an exact signed
   compare-and-swap rollback authority with a higher sequence and current/target
   digests; replay is byte-identical and cannot authorize a different target.
7. The Browser loads and verifies its packaged descriptor before claim and sends
   the exact base64url descriptor bytes and detached signature. Development mode
   requires an explicit test-only descriptor path and never becomes production
   authority.
8. The local claim verifies the exact active, accepted, and non-revoked release
   inside the ticket-claim transaction and rejects before consuming the ticket
   or mutating application, reservation, session, or event state. Unknown newer
   builds are not accepted through semantic-version or `>=` comparison.
9. The claimed run and server-issued capability freeze exact release, manifest,
   build, protocol, and target identity. A later release or revocation blocks a
   future pre-click final-submit authorization but does not strand result,
   resume, `click_started`, `side_effect_unknown`, or trusted-receipt
   reconciliation for an already claimed run.
10. The portal obtains authoritative platform/channel availability and
    immutable download metadata from the server. It never falls back to generic
    `/download`, and a disabled flag, expired activation, missing active
    release, revocation, or unsupported target exposes no URL.
11. Automated package inspection proves the exact `app.asar`, descriptor,
    app ID and protocol construction inputs, one matching headed Chromium
    bundle, excluded-file, hash, size, and target contracts. macOS post-signing
    inspection additionally proves the bundle ID and protocol declaration in
    both package forms. Windows verification covers exact Authenticode identity
    and timestamp evidence when approved credentials are available; actual
    registry registration remains part of the physical protocol canary.
12. Release jobs explicitly build macOS arm64, macOS x64, and Windows x64 once,
    verify and upload those same outputs and manifest as one immutable evidence
    set, and reject stale or extra output. Missing credentials or native
    verification cannot produce a production-ready label; no universal macOS
    Browser artifact is claimed.
13. Tests cover positive claims, exact-byte tampering, role and audience replay,
    key rotation/retirement/revocation, compromised-sequence recovery,
    unsupported or downgraded builds, ticket non-consumption, rollback
    compare-and-swap, submit-versus-reconciliation behavior, portal availability,
    package mutation, and cross-language vectors.
14. Local/cloud Browser distribution flags remain `0`; no source-only, unsigned,
    unnotarized, unverified, or physically uncertified artifact is described as
    distributed, promoted, notarized, or certified.

## Verification Evidence

| Acceptance criteria | Current evidence |
|---------------------|------------------|
| 1-6 | Shared Node/Rust vectors, 18 focused authority tests, 7 focused Rust trust tests, immutable replay/conflict tests, root/delegated rotation and recovery, latest-origin tests, activation/rollback/revocation matrices, and 37-table/41-index dialect parity pass. |
| 7-9 | Packaged descriptor and local protocol tests, ticket non-consumption tests, genuine signed plan matrix, release-bound v2 capabilities, raw/v1 recovery-only regressions, submitted-receipt replay, and pre-click revocation fencing pass. |
| 10 | 47 focused Browser portal tests, all 169 portal tests, strict typecheck, and the production portal build pass; unsupported or ambiguous targets expose no installer URL. |
| 11-12 | 11 package-contract tests, 9 release-gate tests, workflow validation, YAML parsing, exact-target/native-evidence contracts, and no-rebuild evidence assembly pass. These are construction and policy proofs, not credentialed package or physical-device execution. |
| 13 | All 1,216 Jobs tests and all 1,119 server tests pass, including 1,013 library, 100 HTTP integration, and 6 auxiliary server tests. |
| 14 | Operations/env policy and source checks keep local/cloud Browser distribution, model generation, and mailbox sync at `0`; no deployment or live-state mutation occurred. |

Strict Jobs typecheck and build pass for all five workspaces. Server formatting,
`cargo check --tests`, and Clippy with warnings denied pass. Schema, CI-guard,
provenance/license, generated portal, workflow, YAML, and diff checks pass. The
portal build retains one non-blocking Vite chunk-size advisory.

Not executed or claimed: Apple signing/notarization/stapling/Gatekeeper,
Windows Authenticode/timestamp packaging, immutable artifact-host upload and
read-back, administrator registry import/application, physical macOS or Windows
install/protocol/upgrade/rollback canaries, any feature-flag change, or any
production/live-tenant action.

## Production Boundary

This round does not access Apple Developer ID, notarization, Authenticode,
timestamp, artifact-host, or production database credentials. It does not
publish an installer, activate an update channel, mutate a live ticket, or run
physical macOS/Windows canaries. Those external gates remain parked while all
independent source, packaging, protocol, server, portal, and CI work continues.

Native self-update is explicitly deferred to a later production batch. Round
603 does not ship an installed-app update feed, polling runtime, downloader,
platform installer, restart coordinator, or durable update journal. The macOS
ZIP classified as an updater artifact is only an exact manifest-bound release
artifact in this round; it is not evidence that automatic update or rollback is
implemented. Self-update must return with authenticated signed feed authority,
sealed exact-byte installation, platform code-signing verification, durable
recovery, and physical macOS and Windows update/rollback canaries.
