# FIX-20261126: Keep favicon traffic out of the synthetic redirect fixture

> **Codex preflight:** Load `$bluey-ops` and compare exact hosted failures
> against the current test and candidate inputs.

## Issue and root cause

[Linux CI](https://github.com/Dhanunjay-Divi/bluey/actions/runs/45775583148/job/139758348877)
at `5ee7bca6` stopped in the Chromium fixtures for 307 and 308. In both
cases the final network census showed only a zero-byte GET to
`/favicon.ico`, not application data or a request to the synthetic
redirect destination. The documents had no explicit favicon.

## Fix summary

Use a valid inline icon on the fixture form and the local positive-control
document. Keep the final zero-request assertion and all existing
positive, transport and payload checks intact. Do not allowlist favicon
traffic or change the application/browser guard code.

| File | Change |
| --- | --- |
| `jobs/automation/tests/playwright-submit-redirect.integration.test.ts` | Inline icon for the two synthetic documents |
| `CHANGELOG.md` | Record the fixture correction |

## How to test

Check TypeScript syntax and the exact test-only delta. On the authorized
Linux runner, run this fixture with the pinned Chromium build and then
rerun the exact Jobs workflow. No hosted retry is authorized by this fix.

Local TypeScript parsing and exact archive comparison passed (`0d2b3d`).
Removing only the inline icon and two fixture HTML edits reconstructs
the exact original test bytes. The check decoded the 1×1 GIF and verified
the unchanged zero-request assertion. Independent review agreed.
No browser was executed by these local checks.

## Known limitations

This does not qualify the fixture in a real browser or prove the hosted
Linux workflow, managed-runner Docker smoke or external ATS compatibility.
