# FIX-601: Bind Certified ATS Submission to the Exact Request

> **Codex preflight:** Loaded `$bluey-ops` and verified the provider-authority,
> packet-checksum, irreversible-click, and reconciliation invariants before
> diagnosis and implementation.

## Issue

A certified adapter could identify the intended submit control but still rely on
page-realm form APIs and incomplete browser interception when proving the final
request. Page code could mutate form semantics, extra traffic could escape after
fill, and Chromium did not expose selected file bytes through intercepted
multipart request data. The result path also accepted unintended `3xx` statuses
and could mistake confirmation-looking text for success when multiple submit
controls made the returned form ambiguous.

## Root Cause

Round 598 separated certified adapters from generic submission, but the final
proof did not yet bind the effective form target, ordered successful controls,
hidden provider-job values, and exact outgoing PDF bytes as one immutable
authorization. Network rules also did not reduce the post-fill window to one
causal employer-side operation.

## Fix Summary

The browser now inspects the exact form from a fresh Chromium isolated world,
validates a narrow provider-specific hidden-field schema, and hashes selected
DOM files. Node independently verifies content-addressed PDFs from disk,
validates the browser-generated multipart shape, and inserts only file bodies
omitted by Chromium while retaining its boundary and headers. Durable server
authorization freezes the provider job, form target, ordered field and file
hashes, packet, documents, claims, attempt, and runner fence before the one
exact request may leave. All other post-fill traffic is blocked, and
confirmation must resolve to the same provider job. Browser, receipt, and
server verification share the same bounded success rule: any `2xx`, or exactly
`301`, `302`, `303`, `307`, or `308`. Both provider state machines reject a
returned unique or ambiguous submit form, or negative submission language,
before considering confirmation text.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/src/certified-submit-form.ts` | Captures successful controls and files in an isolated world. |
| `jobs/automation/src/effective-submit-target.ts` | Binds page, action, confirmation, and provider job. |
| `jobs/automation/src/final-submit-proof.ts` | Canonicalizes the immutable provider/document/job proof. |
| `jobs/automation/src/trusted-submit.ts` | Validates the exact browser multipart request and hydrates only omitted file bodies. |
| `jobs/automation/src/submission-confirmation.ts` | Rejects full-body negative submission outcomes before positive confirmation text. |
| `jobs/automation/src/playwright-page.ts` | Installs the post-fill guard and releases only the validated, boundary-preserving request. |
| `jobs/automation/src/providers/{greenhouse,lever}.ts` | Freezes provider state immediately before activation. |
| `server/src/db/jobs/execution_leases.rs` | Validates and atomically freezes the same proof server-side. |
| `server/src/api/jobs.rs` | Carries exact proof into cloud and local execution authority and receipts. |
| Automation and server tests | Cover mutation, spoofing, ordering, bytes, replay, and same-job confirmation. |

## Edge Cases Handled

- Page-realm `FormData` and input-prototype replacement.
- Same-host redirects or form actions for another provider job.
- Submitter overrides, method-override headers/fields/query values, and named
  submit controls.
- Unknown, duplicate, or mismatched hidden provider-job fields.
- Reordered, added, removed, renamed, or byte-changed form values and PDFs.
- Cross-type text/file field overlap and multipart boundary collisions.
- GET, HEAD, OPTIONS, WebSocket, beacon, asset, and unrelated redirect traffic
  after applicant data is filled.
- Confirmation-like text or a confirmation URL for another job.
- Unintended redirect statuses (`300`, `304`, `305`, `306`, and `309` through
  `399`) at the browser, receipt, and server boundaries.
- Confirmation-looking pages that still expose multiple visible submit controls.
- Positive confirmation fragments combined with full-body language saying the
  application was not or could not be submitted, including active voice,
  contractions, and “not yet” variants.
- Workflow receipt parsing that receives any status outside `2xx`, `301`, `302`,
  `303`, `307`, or `308` from a runner.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-automation
npm run typecheck --workspace @bluey/jobs-automation
npm run build --workspace @bluey/jobs-automation
npm run smoke --workspace @bluey/jobs-automation

cd ../server
cargo test final_submit -- --nocapture
cargo test --test integration_e2e jobs_cloud_ -- --nocapture
```

## Known Limitations

- The hidden-field allowlists intentionally reject unknown tenant DOM shapes and
  optional empty file controls. Greenhouse and Lever require owner-authorized
  tenant certification before unattended final submission can be enabled.
- Playwright/Chromium does not expose file bodies in intercepted multipart
  `postData`; the production path therefore inserts the separately verified
  bytes into the otherwise byte-preserved browser request before validating it
  again.
