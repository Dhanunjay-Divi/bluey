# FIX-781: Jobs dependency security blocked the V1 release candidate

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this bounded correction against the
> exact Phase 622 branch. No host, provider, application, mailbox, deployment, production flag,
> or cohort state was changed.

## Issue

The Phase 614 review retained three production dependency advisories as an explicit release
blocker. A fresh audit on 2026-08-31 confirmed two high-severity and one moderate-severity finding
across 252 production dependencies. The full 673-dependency development graph additionally had
four high-severity build-tool findings.

The direct high-severity finding was especially relevant: both the portal résumé importer and the
automation document validator accepted user-supplied PDFs through affected `pdfjs-dist` 5.7.284.

## Root Cause

- `automation/package.json` and `portal/package.json` allowed PDF.js 5.x and the shared lockfile
  resolved 5.7.284, which is inside `GHSA-hq66-cqwq-w95j`'s affected range.
- The root overrides intentionally pinned DOMPurify 3.4.12 and PostCSS 8.5.25. New advisories made
  DOMPurify 3.4.12 and PostCSS's locked Nano ID 3.3.16 unsafe.
- Locked browser build dependencies retained affected brace-expansion, JS-YAML, and Undici patch
  versions.
- Jobs CI installed with `--no-audit` and had no later advisory gate, so a newly disclosed issue
  could remain visible only in a manual audit.

Official advisory records:

- <https://github.com/mozilla/pdf.js/security/advisories/GHSA-hq66-cqwq-w95j>
- <https://github.com/cure53/DOMPurify/security/advisories/GHSA-55q2-fjhq-7xh7>
- <https://github.com/advisories/GHSA-2v37-7h3g-55p8>

## Fix Summary

- Pin both PDF consumers to the first patched PDF.js release, 6.2.108.
- Keep Bluey on the non-rendering text-extraction surface; neither consumer instantiates PDF.js
  viewer, annotation, nor scripting layers.
- Follow the PDF.js 6 lifecycle by destroying the loading task on success and on every bounded
  rejection path.
- Move DOMPurify and PostCSS overrides to patched releases and refresh every affected transitive
  lock entry within its existing compatible dependency range.
- Declare the PDF.js-required Node floor, which current Node 22 CI and Node 24 managed runtime
  satisfy. The managed-cloud candidate independently rejects a configured build image below
  Node 22.13 before auditing or building it.
- Add a moderate-or-higher dependency audit after the locked install in Jobs CI, the repository
  release workflow, Browser candidate preparation and isolated packaging, and the managed-cloud
  candidate build. Their contract tests prevent silent removal or post-build reordering.
- Rebuild the checked-in portal bytes from the corrected locked graph and update PDF.js notices.

## Files Modified

| File | Change |
|------|--------|
| `jobs/package.json` | Declare Node floor, patched overrides, and the dependency-audit command |
| `jobs/package-lock.json` | Lock patched production and build dependency versions with integrity |
| `jobs/automation/package.json` | Pin PDF.js 6.2.108 |
| `jobs/automation/src/documents.ts` | Use PDF.js 6 loading-task cleanup for bounded PDF validation |
| `jobs/automation/tests/documents.test.ts` | Use the same corrected cleanup lifecycle in test extraction |
| `jobs/portal/package.json` | Pin PDF.js 6.2.108 |
| `jobs/portal/src/lib/documents/import.ts` | Retain text-only import and destroy the loading task |
| `.github/workflows/jobs-ci.yml` | Audit the installed exact dependency graph |
| `.github/workflows/release.yml` | Re-audit immediately in the release check |
| `.github/workflows/jobs-browser-release.yml` | Audit candidate and trusted-tool graphs before packaging credentials |
| `.github/workflows/jobs-managed-cloud-release.yml` | Audit the read-only exact graph before artifact builds |
| `jobs/scripts/browser-release-ci-gate.mjs` | Bind Browser audit order into the release contract |
| `jobs/scripts/managed-cloud-release-gate.mjs` | Bind managed-cloud audit order into the release contract |
| `jobs/scripts/ci-guards-self-test.mjs` | Make every release-path audit position a structural invariant |
| `jobs/THIRD_PARTY_NOTICES.md` and `jobs/automation/THIRD_PARTY_NOTICES.md` | Record PDF.js 6.2.108 |
| `web/jobs/` | Rebuild the checked-in portal from the patched dependency graph |
| `CHANGELOG.md` | Record the security correction without claiming deployment |

## Verification

```bash
npm ci --prefix jobs --ignore-scripts
npm audit --prefix jobs --omit=dev --audit-level=moderate
npm audit --prefix jobs --audit-level=moderate
npm run --prefix jobs build --workspace @bluey/jobs-automation
npm exec --prefix jobs vitest run automation/tests/documents.test.ts
npm run --prefix jobs typecheck --workspace @bluey/jobs-portal
npm exec --prefix jobs vitest run portal/src/lib/documents.test.ts
npm run --prefix jobs build --workspace @bluey/jobs-portal
node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/check-provenance-licenses.mjs
npm exec --prefix jobs node --test scripts/browser-release-ci-gate.test.mjs
npm exec --prefix jobs node --test scripts/managed-cloud-release-gate.test.mjs
git diff --check
git diff --exit-code -- web/jobs
```

An additional local Chrome smoke opened the preview-only résumé route, uploaded a synthetic
3,228-byte PDF, extracted its text through the production portal bundle, and presented the import
review with no page error. It used no real résumé, account, provider, or external write.

The final exact commit must also pass the hosted Jobs CI/privacy workflow. A zero-advisory result
is time-bound registry evidence, so every later release reruns the gate rather than treating this
document as permanent proof.

## Known Limitations

- The dependency fix does not enable discovery, runner, provider, mailbox, communication,
  application, or production effects.
- The existing portal bundle-size warning remains a separately tracked performance follow-up; the
  build succeeds and this fix does not waive that warning.
