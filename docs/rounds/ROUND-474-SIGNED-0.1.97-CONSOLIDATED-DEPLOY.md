# Round 474 - Signed 0.1.97 Consolidated Deploy

Date: 2026-07-10
Branch: `codex/bluey-web-ui-parallel-20260704`
Release version: `0.1.97`
Backup task id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Promote the reviewed local work from Rounds 458 through 473 as one traceable,
signed Bluey desktop/web/API release without GitHub Actions.

## Release Invariants

- Private `tmp/` evaluation evidence and Python bytecode are excluded.
- No provider key, release private key, customer prompt, resume text, or raw
  private evaluation answer enters git or a public artifact.
- Build one immutable desktop artifact from the release source commit and
  publish exactly that artifact.
- Back up production data and the current API binary before replacement.
- Do not publish capture-visible/dev-marker binaries.
- Do not claim a Windows artifact unless it is produced and verified by the
  Windows/MSVC release builder.

## Scope

- account connection, legal acceptance, trial and signed-out behavior
- transcript final flush, deduplication, captions, and listen auto-stop
- humanized role-aware answers and 50-question routing regressions
- bounded provider fallback and detailed internal phase diagnostics
- durable session sync/audit records and stable ownership
- macOS/Windows process aliases and installer cleanup
- Auto/Quick/Thorough controls, context readiness, source chips, recovery, and
  workbench version preservation
- static web copy and account/session-history cleanup

## Pre-Deploy Verification

Passed before the release commit:

- release hygiene scan passed across 83 release-facing files; warnings were
  limited to intentional dev-flag mentions in local visual-smoke scripts and
  security documentation
- scoped source/new-file secret-pattern scan passed without printing candidate
  material
- root workspace library tests: 646 passed, 0 failed, 5 ignored
- root workspace all-target tests passed; hardware/network-only tests remained
  intentionally ignored
- server library tests: 296 passed, 0 failed
- server integration tests: 54 passed, 0 failed
- strict Clippy passed for all targets in both Rust workspaces
- Rust formatting, JavaScript syntax, Swift typecheck, Windows C syntax, touched
  shell syntax, and `git diff --check` passed

The integration gate initially exposed three failures. Two were stale test
expectations after the intended provider short-wait/pre-output fallback changes.
The genuine defect was that `/sync/batch` accepted a safe legacy session ID but
session lookup required a UUID. Session validation now accepts bounded
alphanumeric/hyphen/underscore IDs while rejecting slash, traversal, whitespace,
and control-character shapes. The complete integration suite passed after the
fix.

The final document will also record release id, artifact SHA256, backup paths,
API binary SHA256, public health identity, signature and MIME verification,
service restart state, and post-deploy log scan.

## Deployment Status

Pending owner-approved signed promotion in this round.
