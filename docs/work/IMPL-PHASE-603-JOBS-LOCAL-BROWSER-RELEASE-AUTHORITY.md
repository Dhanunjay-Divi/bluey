# IMPL: PHASE-603 - Jobs Local Browser Release Authority

> **Codex preflight:** Loaded `$bluey-ops`, reconciled it against the isolated
> Round 603 worktree and current repository docs, and did not use the SSD
> archive. The canonical meeting checkout, credentials, production services,
> release hosting, live tenants, and physical devices remained untouched.

## Scope

**Does:**

- Defines strict, canonical, cross-language Ed25519 authority for root trust
  policies, immutable build descriptors and manifests, threshold signature
  sets, channel activations, compare-and-swap rollbacks, and append-only
  revocations.
- Stores immutable release state in paired SQLite/PostgreSQL schemas and exposes
  eight administrator registry routes for import, movement, assignment, status,
  rollback, and incident response.
- Verifies the account's exact active, compatible, non-revoked Browser release
  inside the local ticket-claim transaction before consuming the ticket, then
  freezes that identity into operation-scoped capabilities.
- Keeps result/resume and submitted-receipt reconciliation usable after a later
  release change while fencing legacy/raw submit and every future pre-click
  submission against current release authority.
- Loads and verifies packaged Browser authority, restricts development authority
  to unpackaged loopback use, and carries exact canonical descriptor bytes and a
  request-bound nonce through claim.
- Adds target-explicit native packaging for macOS arm64, macOS x64, and Windows
  x64; bounded safe extraction; real-ASAR inspection; exact Chromium, native
  metadata, signer, timestamp, notarization, staple, inventory, and no-rebuild
  evidence contracts.
- Replaces generic portal downloads with server-owned immutable release
  metadata and an explicit Apple-silicon/Intel choice when browser architecture
  cannot be known safely.
- Documents workflow handoffs, public/private authority boundaries, immutable
  host read-back, registry order, rollback, revocation, and external gates.

**Does NOT:**

- Enable local or cloud Browser distribution, managed model generation, mailbox
  sync, or employer-facing production execution.
- Access Apple, notarization, Authenticode, timestamp, artifact-host, root,
  release, promotion, incident, production database, or live-tenant credentials.
- Produce, publish, install, promote, or activate a production package; mutate a
  live release channel or ticket; or run physical macOS/Windows canaries.
- Claim that Windows registry/AUMID/protocol launch was executed from source
  construction proof, or that canonical external canary evidence was generated
  by this local checkout.
- Implement an installed-app update feed, downloader, platform installer,
  restart coordinator, durable update journal, or automatic rollback.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `.github/workflows/jobs-browser-release.yml` | Created | Contract, credential-free preparation, protected native packaging, stored-byte authorization, and no-rebuild promotion. |
| `.gitignore` | Modified | Exclude generated local release-authority output only. |
| `infra/{sqlite,postgres}/server-runtime/*jobs_browser_release_authority.sql` | Created | Paired immutable trust, release, artifact, activation, rollback, revocation, channel, and assignment storage. |
| `server/src/db/jobs/browser_release_{authority,trust,registry}.rs` | Created | Strict codecs, signature verification, trust rotation, registry transitions, selection, replay, and revocation fencing. |
| `server/src/db/jobs.rs`, `server/src/db/mod.rs` | Modified | Export authority modules and register paired migrations. |
| `server/src/db/jobs/{local_runner,customer_data}.rs` | Modified | Freeze exact release identity on claim and include release state in lifecycle handling. |
| `server/src/api/jobs_browser_releases.rs` | Created | Administrator trust/manifest/activation/rollback/revocation/assignment/status API. |
| `server/src/api/jobs_local_capability.rs` | Modified | Add release-bound v2 capabilities and recovery-only v1 verification. |
| `server/src/api/jobs.rs`, `server/src/api/mod.rs` | Modified | Integrate availability, claim, submit fencing, recovery, and routes. |
| `server/src/db/jobs/tests.rs`, `server/tests/{integration_e2e,jobs_runner_plan_matrix}.rs` | Modified | Cover exact authority, entitlement, claim, ticket non-consumption, legacy/raw recovery, revocation, and current plan behavior. |
| `jobs/browser/src/{release-authority,packaged-release}.ts` | Created | Canonical Node authority and safe packaged descriptor loading. |
| `jobs/browser/src/{app-lifecycle,checkpoint-recovery,local-capabilities,local-protocol-handler,local-run-contracts,run-controller}.ts` | Modified | Require packaged proof, carry frozen capabilities, and preserve bounded recovery. |
| `jobs/browser/electron-builder.yml`, `jobs/browser/scripts/{install-chromium,package-release,release-package-contract}.mjs` | Created/modified | Exact target, packaging, archive, ASAR, signer, Chromium, protocol, and output contracts. |
| `jobs/browser/{package.json,fixtures,tests}`, `jobs/package-lock.json` | Created/modified | Locked inspection dependencies, shared vectors, packaged fixtures, and negative matrices. |
| `jobs/scripts/{browser-release-ci-gate,browser-release-ci-gate.test,check-jobs-schema-parity,ci-guards-self-test}.mjs` | Created/modified | Seal and verify evidence plus paired-schema and policy self-tests. |
| `jobs/portal/src/lib/{browser-release,runner-access}.ts` | Created/modified | Validate authoritative release availability and exact immutable target URLs. |
| `jobs/portal/src/{types,data/preview,styles.css}`, `jobs/portal/src/views/BrowserView.tsx` | Modified | Add exact release types, previews, selector state, and evidence presentation. |
| `jobs/portal/src/lib/runner-access.test.ts`, `jobs/portal/src/views/BrowserView.test.tsx` | Created/modified | Cover disabled, malformed, unsupported, and explicit-target behavior. |
| `web/jobs/` | Rebuilt | Check in the verified production portal bundle. |
| `jobs/OPERATIONS.md`, `ops/bluey-jobs.env.example` | Modified | Add the complete disabled-by-default release and registry runbook. |
| `CHANGELOG.md`, Round 603, `FIX-609` through `FIX-612`, IMPL/REVIEW docs | Created/modified | Record scope, audit fixes, evidence, limitations, and handoff. |

## Build & Test

```text
Jobs tests                       1,216 passed
  automation                       530
  browser                          192
  runner                           249
  workflows                         76
  portal                           169
Focused Browser authority/package   29 passed (18 authority + 11 package)
Release workflow gate                 9 passed
Focused Browser portal               47 passed
Jobs strict typecheck/build            5 workspaces passed

Server tests                     1,119 passed
  library                         1,013
  HTTP integration                  100
  auxiliary targets                   6
Focused trust rotation                7 passed
Server fmt/check/strict Clippy        passed

Schema parity                         37 tables, 41 indexes
CI guard self-tests                   passed
Dependency inventory                  663 lock entries / 631 versions
Source provenance                     14 commit-pinned repositories
Workflow contract and YAML            passed
Generated portal and diff checks      passed
Final staged privacy                   2,403 paths / 2,130 text files passed
```

The portal production build retains one existing Vite advisory for a minified
chunk slightly above 500 kB; the build succeeds. Credential-backed native
packaging, immutable public read-back, and physical canaries were not run and
are not represented by the passing source gates.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Build descriptors and manifests are channel-neutral; activation owns channel movement. | Candidate-to-stable promotion must preserve the exact stored package bytes and immutable manifest. |
| Root recovery rejects delegated revocation of root material. | Incident authority cannot be allowed to remove the independently anchored authority needed to rotate compromised delegated roles. |
| Legacy v1 and debug raw-ticket authority are recovery-only. | Compatibility may settle existing results or receipts but must never recreate submit authority. |
| Unknown macOS architecture requires an explicit portal choice. | Mainstream browsers report `MacIntel` on both Intel and Apple silicon; guessing would expose the wrong signed installer. |
| Windows source verification stops at construction proof. | Registry, AUMID, installer behavior, and protocol launch require a clean credential-free Windows device. |

## Known Follow-ups

- Run the protected native jobs with approved signer identities and credentials,
  copy the exact five artifacts to immutable HTTPS paths, and verify complete
  public read-back before any server activation.
- Run physical macOS arm64/x64 and Windows x64 clean-install, launch, protocol,
  upgrade, rollback, portal-download, and revocation canaries; Windows must also
  prove installer/AUMID and registry registration.
- Implement installed-app update authority, sealed installation, restart and
  crash recovery, and physical update/rollback canaries in a separate batch.
- Harden packaged descriptor loading against the remaining lstat/read TOCTOU
  defense-in-depth gap while retaining the server's exact manifest authority.
- Add direct deletion mutations for every native evidence marker and a real
  portal click/state-transition test; rotate the shared trust fixture before its
  August 2027 expiry.

## Review Checklist (for reviewer)

- [x] Files match the integrated Round 603 scope described above
- [x] No unrelated meeting-owned checkout, production, credential, or live data changes are included
- [x] Tests cover all source-testable acceptance criteria and audit regressions
- [x] Rust, TypeScript, workflow, schema, generated UI, and policy gates pass
- [x] External native, hosting, tenant, and physical-device evidence is explicitly unclaimed
- [x] Work follows `AGENTS.md`; no `docs/reviews/` file was edited
