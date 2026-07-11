# Round 474 - Signed 0.1.98 Consolidated Deploy

Date: 2026-07-10
Branch: `codex/bluey-web-ui-parallel-20260704`
Release version: `0.1.98`
Backup task id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Promote the reviewed work from Rounds 458 through 473 as one traceable, signed
Bluey desktop/web/API release without GitHub Actions.

## Release Invariants

- Private evaluation output and Python bytecode stay outside git/artifacts.
- Provider keys, the release private key, customer content, and raw private
  evaluation answers stay outside git and public artifacts.
- Production data and the current API binary are backed up before replacement.
- Capture-visible/dev-marker binaries are rejected.
- Windows is not claimed without a Windows/MSVC-built executable.
- A failed release artifact remains immutable and is superseded by a new version.

## Scope

- account connection, legal acceptance, trial, and signed-out behavior
- transcript final flush, deduplication, captions, and listen auto-stop
- humanized role-aware answers and 50-question routing regressions
- bounded provider fallback and detailed internal phase diagnostics
- durable session sync/audit records and stable ownership
- macOS/Windows process aliases and installer cleanup
- Auto/Quick/Thorough, context readiness, source chips, recovery, and workbench
  version preservation
- static web/account/session-history cleanup

## Pre-Deploy Verification

- release hygiene passed across 83 release-facing files
- scoped staged secret scan passed without printing candidate material
- root workspace library tests: 646 passed, 0 failed, 5 ignored
- root workspace all-target tests passed
- server library tests: 296 passed, 0 failed
- server integration tests: 54 passed, 0 failed
- strict Clippy passed for both Rust workspaces
- formatting, JavaScript syntax, Swift typecheck, Windows C syntax, touched shell
  syntax, and staged whitespace checks passed

The integration gate caught and fixed a real sync defect: `/sync/batch` accepted
a safe legacy session ID while lookup required a UUID. Validation now accepts
bounded alphanumeric/hyphen/underscore IDs and rejects traversal, slash,
whitespace, and control-character shapes.

## 0.1.97 Gate Rejection

Commit `ba90b46f904abb0bb02771b51e9e9acb2720a66c` built and published a signed
`0.1.97` macOS archive. The live verifier rejected it because the Makefile
omitted `termb`, `hostovb`, and `adriverb`, although runtime and installers
already required those identities. The signed `latest.json` was immediately
rolled back to verified `0.1.96`. The `0.1.97` archive was not modified.

The corrective `0.1.98` package adds all identity copies on macOS and equivalent
Windows package aliases. Final source commit, artifact SHA256, backup paths, API
binary SHA256, health identity, MIME/signature checks, service state, and log
scan are recorded after the corrected promotion.

## Deployment Status

Corrective signed promotion in progress.
