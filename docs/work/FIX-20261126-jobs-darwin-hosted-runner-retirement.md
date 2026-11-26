# FIX-20261126: Restore Jobs CI on a supported Darwin runner

> **Codex preflight:** Load `$bluey-ops` and reconcile the current exact
> feature branch, workflow and hosted CI logs before acting.

## Issue and root cause

[Jobs CI run 45775583148](https://github.com/Dhanunjay-Divi/bluey/actions/runs/45775583148)
attempt 1 ran on `5ee7bca6`. The
[Darwin job](https://github.com/Dhanunjay-Divi/bluey/actions/runs/45775583148/job/139758348537)
was cancelled before a single step; its check annotation reports that
`macos-14-arm64-us-default` is not supported. GitHub
[retired the macOS 14 image on November 2, 2026](https://github.blog/changelog/2026-10-01-github-actions-macos-14-runner-image-retirement/).
This is a runner provisioning failure, not a native addon test result.

## Fix summary

Use the fixed, supported [standard `macos-15` ARM64 runner](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job)
for the Jobs native-storage lane. Check actual Darwin/ARM64 before
the build. Preserve the job name, exact checkout check, Rust/Node versions,
tests, native smoke and all Linux behavior. No hosted rerun by this change.

| File | Change |
| --- | --- |
| `.github/workflows/jobs-ci.yml` | Pin macOS 15 and check the native host platform |
| `CHANGELOG.md` | Record the Jobs CI repair |

## How to test

Parse the workflow and compare it with the exact failed-head definition.
Assert the native platform check passes on Darwin ARM64 and rejects a
different OS or architecture. Run the existing Jobs CI guard self-test.
Then bind one fresh authorized hosted run to the reviewed successor SHA;
the current failure cannot be converted into a pass.

Local result: exact parsed YAML comparison `460c0f` matched failed-head
workflow after accounting for only the Darwin runner label and one
verification step. Real Darwin ARM64 passed the new command; synthetic
Darwin/x86_64 and Linux/ARM64 refused. The existing Jobs CI guard suite
passed (`92225/c39646`). Independent scope review found no blocking issue.
The current workflow's Darwin lane was not rerun; the fix is not published.

## Known limitations

This does not change the separate desktop and optional Jobs Browser
packaging workflows, which also contain historical macOS 14 pins.
Host selection is not proof that the hosted build or native smoke will pass.
