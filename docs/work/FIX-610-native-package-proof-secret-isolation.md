# FIX-610: Native package proof and signing isolation

> **Codex preflight:** Loaded `$bluey-ops` and verified this fix in the isolated
> Round 603 worktree without using signing credentials or producing, publishing,
> or executing a production installer.

## Issue

The Browser packaging path did not prove that one exact native package contained
the approved runtime, build authority, Chromium revision, application identity,
and protocol construction inputs, nor that those exact bytes survived native
verification and later promotion without a rebuild.

## Root Cause

The prior generic packaging command mixed targets, treated `app.asar` as an
opaque blob, lacked an immutable evidence set, and had no protected workflow
boundary separating credential-free candidate preparation from trusted signing
tooling. An unvalidated prepared archive or stale package output could therefore
enter a credential-bearing job without the required fail-closed proof.

## Fix Summary

Packaging is now explicit for macOS arm64, macOS x64, and Windows x64. A
credential-free job builds only the candidate JS and matching Chromium bytes.
The protected job uses workflow-revision-pinned trusted tooling, a bounded
pre-extraction tar validator, script-disabled candidate dependency installation,
step-scoped signing inputs, and exact-output sealing. Post-package inspection
opens the real ASAR header and required runtime members, rejects links, source,
tests, fixtures, maps, declarations, credentials, stale authority, and extra
Chromium trees, and records a canonical content inventory. Native verification
binds macOS signature/notarization evidence or Windows Authenticode/timestamp
evidence to the same seal. Authorization and promotion operate only on stored
bytes and independently produced signatures.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/jobs-browser-release.yml` | Add credential-free preparation, isolated native packaging, exact inspection, offline authorization, and no-rebuild promotion. |
| `jobs/browser/scripts/release-package-contract.mjs` | Add target, source, secret, Chromium, tar extraction, builder, and output contracts. |
| `jobs/browser/scripts/package-release.mjs` | Package one prepared target with trusted configuration and sanitized child environments. |
| `jobs/scripts/browser-release-ci-gate.mjs` | Seal, inspect, inventory, assemble, authorize, promote, and validate exact evidence. |
| `jobs/browser/electron-builder.yml` | Limit packaged application files and define exact native targets/resources/protocol metadata. |
| `jobs/browser/scripts/install-chromium.mjs` | Require one explicit supported target and matching Chromium architecture. |
| `jobs/browser/package.json`, `jobs/package-lock.json` | Add explicit release entry points and locked ASAR/tar inspection dependencies. |
| `jobs/browser/tests/release-package-contract.test.ts`, `jobs/scripts/browser-release-ci-gate.test.mjs` | Cover positive contracts and archive, ASAR, package, signature, replay, and mutation failures. |
| `.gitignore` | Exclude only generated local release-authority output. |

## Edge Cases Handled

- Prepared archives reject traversal, hard links, escaping or write-through
  symlinks, path collisions, unsafe ancestors, special files, and boundedness
  violations before extraction.
- ASAR inspection is platform-neutral and rejects link members, unpacked
  required entries, case/Unicode collisions, and hidden test-shaped outputs.
- Exactly three target parts and five native artifacts must be present; stale,
  missing, duplicate, or package-kind-mismatched outputs fail closed.
- macOS DMG and ZIP records must describe identical application content for a
  target; promotion cannot rebuild or replace candidate bytes.
- Signing and root/release/promotion private material is never stored in the
  repository or uploaded as candidate evidence.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-browser -- --run tests/release-package-contract.test.ts
node --test scripts/browser-release-ci-gate.test.mjs
node scripts/browser-release-ci-gate.mjs workflow \
  --file ../.github/workflows/jobs-browser-release.yml
```

## Known Limitations

- Apple Developer ID/notarization, Windows Authenticode/timestamp, immutable
  public hosting, and physical install/protocol/upgrade/rollback canaries require
  approved external credentials or environments and remain unclaimed.
- Windows protocol registration is a locked construction/runtime contract until
  the exact signed installer completes its physical protocol canary; it is not
  represented as a locally executed installer result.
