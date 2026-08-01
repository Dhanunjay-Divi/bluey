# FIX-030: Actions disk exhaustion and duplicate feature runs

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The latest-SHA server test job exhausted its hosted runner disk while linking,
and every feature-branch update launched duplicate push and pull-request copies
of all three Bluey CI workflows.

## Root Cause

The observability test job retained the workspace target tree while compiling a
second independent `server/target` tree. GitHub reported zero megabytes free
immediately before `rust-lld` terminated with signal 7. The CI, Jobs privacy,
and observability workflows also selected feature branches under both `push`
and `pull_request` events.

## Fix Summary

Run workspace and server tests as separate jobs on clean hosted runners with
scope-specific Rust caches. Preserve the legacy `tests` check as an aggregator
that requires both isolated jobs. Keep push verification on `main` and use the
mandatory, base-unrestricted pull-request event for feature branches,
eliminating duplicate gates without weakening stacked-branch coverage.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/ci.yml` | Run the matrix once per feature update through the pull request. |
| `.github/workflows/jobs-ci.yml` | Run the Jobs privacy gate once per feature update. |
| `.github/workflows/observability-policy.yml` | Isolate workspace/server artifacts and deduplicate triggers. |
| `CHANGELOG.md` | Record the CI reliability and capacity correction. |

## Edge Cases Handled

- Direct pushes to `main` still run every gate after merge.
- Pull requests to `main` or another feature branch retain the same macOS,
  Ubuntu, Windows, Jobs privacy, observability, workspace-test, and server-test
  coverage.
- The existing `tests` job name remains a required-check-compatible aggregate
  and cannot pass unless both isolated test jobs pass.
- Workspace and server caches use distinct keys and target roots.

## How to Test

```bash
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.7 \
  .github/workflows/ci.yml .github/workflows/jobs-ci.yml \
  .github/workflows/observability-policy.yml
cargo test --all-targets
cargo test --manifest-path server/Cargo.toml
git diff --check
```

After push, verify exactly one pull-request run per workflow and require every
latest-SHA job to pass before merge.

## Known Limitations

- Feature branches without an open pull request rely on the required local
  preflight; opening the mandatory pull request starts all hosted gates.
