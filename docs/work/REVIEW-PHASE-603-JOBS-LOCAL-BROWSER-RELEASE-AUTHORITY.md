# REVIEW: PHASE-603 - Jobs Local Browser Release Authority

> **Codex preflight:** Loaded `$bluey-ops`, reconciled it against current bytes,
> and reviewed only the isolated Round 603 worktree. No SSD/archive lookup was
> needed.

**Commit range:** `066592c0..staged working tree`
**Reviewer:** Codex self-review with independent client/package, server-authority, and final adversarial audits
**Date:** 2026-08-06

## Per-Task Review

### Cross-language trust and immutable server registry

| Field | Value |
|-------|-------|
| Files | paired migrations, shared Node fixture/validator, Rust trust/registry modules, administrator API |
| Verdict | 🟢 accept |

**Findings:**

- Canonical bytes, unique audiences, exact roles, thresholds, generations,
  expiry, artifact origin, immutable URLs, native targets, and Chromium bounds
  agree between Node and Rust; unknown fields and noncanonical values fail.
- Independent review found that fresh successor state could reuse a predecessor
  artifact origin and that delegated incident semantics could interfere with
  root recovery. `FIX-611` requires the latest origin for fresh decisions,
  preserves exact historical replay, and prevents delegated revocation from
  targeting independently anchored root material.
- Activation is monotonic, rollback is signed compare-and-swap, revocation is
  append-only, immutable identities reject conflicting replay, and paired
  schema parity covers all registry tables and indexes.

### Native package, workflow, and no-rebuild authority

| Field | Value |
|-------|-------|
| Files | Electron Builder config, target scripts, release CI gate, protected workflow, package tests |
| Verdict | 🟢 accept as source contract |

**Findings:**

- Credential-free preparation and protected native packaging are separated;
  candidate code is not executed on the signing runner after the trusted
  boundary, and signing inputs are step-scoped and excluded from evidence.
- Bounded tar validation rejects traversal, hardlinks, special files,
  write-through symlinks, ancestor and Unicode/case collisions, and oversized
  input. Real-ASAR inspection rejects links, unpacked required members, source,
  tests, fixtures, maps, declarations, environment files, keys, stale authority,
  and extra Chromium trees.
- The workflow requires matching macOS ZIP/DMG metadata and content. Its Windows
  contract requires outer/inner Authenticode and timestamps plus locked protocol
  construction, but correctly does not claim those unrun native checks or
  installer, registry, AUMID, or protocol-launch execution as local evidence.
- The workflow binds canonical external canary evidence; local source tests do
  not manufacture or claim physical-device evidence.

### Local claim, capability, submit fence, and recovery

| Field | Value |
|-------|-------|
| Files | local runner DB/API, capability codec, packaged authority loader, protocol and recovery clients/tests |
| Verdict | 🟢 accept |

**Findings:**

- Claim validates the account assignment, active manifest, accepted server
  release, complete exact artifact set, descriptor proof, and revocation inside
  the ticket transaction before any ticket or application mutation.
- Release-bound v2 capabilities freeze operation and release identity. Later
  revocation blocks a future pre-click submit without stranding result/resume,
  `click_started`, `side_effect_unknown`, or exact submitted-receipt recovery.
- Independent review found a debug raw-ticket submit path and a stale plan
  fixture. The raw and v1 paths now share the recovery-only operation/status
  fence, submit returns 404, and the matrix seeds genuine signed release
  authority while preserving disabled-distribution behavior.
- The final panic audit replaced a production predecessor `expect` with an
  explicit trust-rotation error and added a missing-predecessor regression.

### Portal target selection and checked-in bundle

| Field | Value |
|-------|-------|
| Files | release metadata validator, access mapper, Browser view/types/styles/tests, generated `web/jobs/` |
| Verdict | 🟢 accept |

**Findings:**

- Only server-owned snake-case metadata from the exact policy origin and
  immutable release path may expose one target installer. Disabled, expired,
  revoked, malformed, incomplete, foreign-origin, unsupported, or ambiguous
  state exposes no URL.
- Unknown macOS architecture requires an explicit Apple-silicon or Intel
  choice; Windows arm64 and unknown systems remain unsupported.
- Full portal tests and the production bundle pass. A future interaction test
  should exercise the actual selector click rather than only helpers and static
  render state.

## Cross-Task Findings

- Acceptance criteria 1-6 are covered by shared canonical vectors, rotation,
  origin, replay, revocation, monotonic activation, and rollback matrices.
- Criteria 7-9 are covered by packaged authority, ticket non-consumption,
  release-bound capability, raw/v1 submit rejection, and recovery tests.
- Criterion 10 is covered by exact portal metadata and target tests.
- Criteria 11-12 are covered as source/package/workflow contracts only; native
  credentials, immutable hosting, and physical devices remain external gates.
- Criteria 13-14 are covered by focused/full test matrices, paired schemas,
  source policy checks, and all local/cloud/model/mailbox flags remaining `0`.
- No production service, channel, database, artifact host, credential, live
  ticket, tenant, physical device, or meeting-owned checkout was changed.

## Build & Test Verification

```text
Jobs tests                 1,216 passed
  automation                 530
  browser                    192
  runner                     249
  workflows                   76
  portal                     169
Focused authority/package     29 passed
Release gate                   9 passed
Focused portal                47 passed
Jobs typecheck/build            5 workspaces passed

Server library             1,013 passed
HTTP integration             100 passed
Auxiliary server               6 passed
Focused trust                  7 passed
Server fmt/check/Clippy        passed

Schema parity                 37 tables / 41 indexes
CI guard self-tests           passed
Provenance/license            passed
Workflow/YAML/diff            passed
Final staged privacy           2,403 paths / 2,130 text files passed
```

The local portal build retains one non-blocking Vite chunk-size advisory. No
credential-backed native job, public artifact read-back, or physical-device
canary is included in these results.

The final index contains only the 86 reviewed Round 603 paths. Its privacy scan,
staged diff check, schema/provenance/license guards, and source checks pass.

## Overall Verdict

🟢 **ACCEPT** - Ready to commit as source; production distribution remains
disabled and external release gates remain mandatory.

## Follow-ups for Next Batch

- Complete credentialed native packaging, immutable hosting/read-back, and the
  physical macOS/Windows certification matrix before changing any distribution
  flag.
- Return installed-app update/feed/install/recovery authority as a separate
  capability; the macOS ZIP alone is not update readiness.
- Address the packaged-file TOCTOU hardening nit, native-marker mutation
  coverage, real portal selector interaction, and August 2027 fixture rotation.
- Continue the next independent Jobs production capability while external
  release interventions remain parked.
