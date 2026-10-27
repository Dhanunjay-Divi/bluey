# FIX-797: Jobs workspace dependency advisory corrections

> Codex preflight: `bluey-ops`, security review and Round 629.

## Issue and Root Cause

The Phase 629 dependency audit found `source-map-js` 1.2.1 and Vite 6.4.3
advisories in the Jobs workspace/build dependency graph. They are not in the
managed runner's selected production dependency tree after workspace pruning;
that distinction does not exempt the build environment from the existing
moderate-or-higher dependency audit gate.

The subsequent **full workspace** audit found 13 package findings (one critical,
nine high, three moderate), including the existing DOMPurify and fast-uri
security pins, the isolated Jobs Browser's Electron, workflow gRPC, document XML
parsing, and compatible transitive/tooling dependencies. The initial selective
runner audit was not a complete workspace audit and is not represented as one.

The reviewed
[source-map-js advisory](https://github.com/advisories/GHSA-68fv-2mgg-jv7q)
identifies 1.2.2 as the patched version. The official
[Vite 6.4.5 changelog](https://github.com/vitejs/vite/blob/v6.4.5/packages/vite/CHANGELOG.md)
records the 6.4.4 launch-editor/access-check fixes and 6.4.5 filesystem check
correction. This batch makes no claim that Bluey's static production portal
exposes a Vite development server or indexed source-map endpoint.

## Fix Summary / Files

- `jobs/portal/package.json`: raise the existing Vite 6.x floor to `^6.4.5`,
  retaining the same major and existing React/Tailwind integration.
- `jobs/package.json`: pin the transitive `source-map-js` override to 1.2.2,
  and advance existing DOMPurify / fast-uri pins to 3.4.17 / 3.1.9, following
  the existing explicit security-override pattern.
- All five Jobs workspaces: raise the Vitest floor to `^4.1.12`.
- `jobs/browser/package.json`: patch only the isolated Jobs Browser's Electron
  dependency within major 43 to 43.8.3. No Bluey desktop/host/overlay dependency
  or code is touched. Update the current-source packaging contract test to
  require that exact version; historical signed fixtures remain unchanged.
- `jobs/package-lock.json`: mechanically regenerate the exact dependency
  resolution with package scripts disabled. Target only the audited packages
  and their required same-major dependency closure; no `audit fix --force`.

The installed npm 11.3.0 crashed while updating the workspace graph (`edgesOut`)
and did not reliably refresh overridden lock entries. Generated the candidate
with npm 11.21.1 installed only in a private disposable tooling directory,
without package scripts, global installation or owner credential/config access.
The resulting full metadata audit passed with zero findings across all 673
dependency records before copying the generated lockfile back. That is lockfile
evidence, not yet an installed-package or application behavior proof.

Resolved audited packages: gRPC 1.14.6; xmldom 0.8.15; baseline-browser-mapping
2.11.33; brace-expansion 1.1.21 / 2.1.7 / 5.0.12 (existing major lines);
browserslist 4.29.5; http-cache-semantics 4.3.0; undici 6.29.1 / 7.30.1;
source-map-js 1.2.2; Vitest family 4.1.12; Vite 6.4.5; Electron 43.8.3;
DOMPurify 3.4.17; fast-uri 3.1.9; CSV 7.0.3 (separately explained in FIX-796).

## Edge Cases and Verification

No portal component, UI behavior, server address, proxy, production bundle
threshold, submission authority or feature flag is changed. Run the whole Jobs
dependency audit, workspace typechecks/tests and a clean temporary portal build;
rebuild/smoke the Dockerfile against the corrected lockfile. Passing results are
recorded only after execution in the Phase 629 IMPL. Final parent execution
passed the installed full audit (zero findings), all five workspace typechecks
and builds, all 2,025 tests and the corrected Linux Docker/native smoke. The
provenance/license checker also passed: 663 registry lock entries, 631 unique
package versions and 14 commit-pinned reference repositories. This inventory's
definition differs from npm's 673 dependency records; they are not test counts.

## Known Limitations

An audit is a dated registry observation, not proof against future advisories.
The existing CI audit remains mandatory. Local tests and this dependency patch
do not authorize registry publication, production deployment or user actions.
