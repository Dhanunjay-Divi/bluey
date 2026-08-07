# FIX-658: Jobs CI runner disk bound

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The exact-SHA Jobs CI rerun passed every step through server unit tests but the
GitHub-hosted Ubuntu runner exhausted its ephemeral disk as the final server
integration test began. The runner process then failed while writing its own
diagnostic log, so no source assertion failed.

## Root Cause

The combined Jobs lane retained the JavaScript dependency tree, native-storage
Cargo target, multi-stage Playwright/Rust Docker build cache, and debug-heavy
incremental server artifacts until the end of the job. Each gate was valid in
isolation, but their retained artifacts exceeded the hosted runner's disk budget
before the last test binary could finish.

## Fix Summary

Disable incremental and debug-heavy dev/test artifacts in the combined CI job.
After the managed-runner image is built and smoked, prune its ephemeral Docker
state, clean the already-verified native-storage Cargo target, and remove the
already-verified Jobs `node_modules` tree before starting server Rust checks.
The server check, unit-test, and integration-test commands remain unchanged.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/jobs-ci.yml` | Bound Cargo artifact growth and reclaim verified ephemeral build state before server tests. |
| `CHANGELOG.md`, Round 605, IMPL/REVIEW docs | Record the runner-capacity failure, scope, and fresh exact-SHA requirement. |

## Edge Cases Handled

- Cleanup runs only after the portal freshness, native storage, and managed
  runner image gates have passed.
- Cargo registry/source caches remain available for the server build; only the
  separate native-storage target is cleaned.
- The ephemeral Docker image/cache and workspace dependency tree are no longer
  needed by any later step.
- Server formatting, API check, unit tests, and integration tests still execute
  as separate fail-closed steps.

## How to Test

```bash
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 \
  .github/workflows/jobs-ci.yml
node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/privacy-gate.mjs
```

The cleanup command itself is intentionally exercised only by the fresh
GitHub-hosted exact-SHA Jobs run because local Docker state is user-owned.

## Known Limitations

- Hosted runner capacity remains external infrastructure. The final exact-SHA
  Jobs run must pass before merge; this source change alone is not completion
  evidence.
