# FIX-638: ATS canary evidence commitment was not bound

> **Codex preflight:** Loaded `$bluey-ops` and verified this defect against the
> current Round 604 worktree only. No SSD/archive or external environment was
> used.

## Issue

A signed canary activation accepted any canonical 64-character digest in
`canaryEvidenceManifestSha256`. No second canary-evidence object type or import
path existed, so the value could name bytes that were never imported or
verified.

## Root Cause

The activation validator and paired schema constrained the field's shape but
did not bind it to the certification manifest that already commits to the
independently signed evidence/layout objects, complete runner/check matrix,
suite digest, zero-tolerance counters, scope, target, and validity window.

## Fix Summary

- Defined the canonical certification-manifest digest as the canary evidence
  commitment; this is not a second artifact.
- Required exact equality in the signed activation validator and both database
  dialect constraints.
- Corrected the shared canonical vector and added a negative validation case
  for an arbitrary otherwise-valid digest.
- Clarified the Round 604 and operations contracts.

## Production Impact

No canary, provider, runner, feature flag, signing key, or deployment was
enabled.
