# FIX-657: Browser release dispatch input limit

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The Browser release authority workflow failed at definition validation with
zero jobs because it declared 29 manual-dispatch inputs while GitHub permits at
most 25.

## Root Cause

The workflow accumulated five independent promotion-time activation,
signature, digest, and canary inputs without enforcing GitHub's top-level
dispatch-input limit. Its contract test validated security boundaries but did
not count the workflow inputs, and the invalid workflow therefore could not run
its own contract job.

## Fix Summary

Replace the five promotion-only fields with one bounded JSON envelope, reducing
the workflow to the exact approved 25-input set. The trusted release gate
accepts only the five known envelope keys, preserves each authority schema's
signed key order, verifies canonical base64 JSON and both supplied SHA-256
digests, and materializes read-only authority files in a private directory. The
48 KiB envelope budget leaves room beneath GitHub's total dispatch-payload
limit. Run the real materialized authorities through promotion tests from the
independent Jobs CI workflow and reject extra, missing, inline, or noncanonical
input definitions.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/jobs-browser-release.yml` | Consolidate promotion authority into one validated input. |
| `.github/workflows/jobs-ci.yml` | Run Browser workflow tests and validation independently. |
| `jobs/scripts/browser-release-ci-gate.mjs` | Count dispatch inputs and safely materialize the exact authority envelope. |
| `jobs/scripts/browser-release-ci-gate.test.mjs` | Cover the input limit, exact materialization, unknown fields, digest mismatch, and noncanonical JSON. |

## Edge Cases Handled

- Missing, extra, duplicated, or non-minified envelope fields fail closed.
- Noncanonical base64 or decoded JSON cannot become release authority.
- Digest mismatches are rejected before any authority file is written.
- Signed activation, signature-set, and canary schema order is preserved rather
  than rewritten into generic alphabetical JSON.
- A malformed release workflow is caught by Jobs CI even when GitHub cannot
  schedule the release workflow itself.

## How to Test

```bash
node --test jobs/scripts/browser-release-ci-gate.test.mjs
node jobs/scripts/browser-release-ci-gate.mjs workflow \
  --file .github/workflows/jobs-browser-release.yml
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 \
  .github/workflows/jobs-browser-release.yml .github/workflows/jobs-ci.yml
```

## Known Limitations

- Candidate signing, offline authorization, physical-device canaries, immutable
  hosting, and promotion still require their protected external authorities.
