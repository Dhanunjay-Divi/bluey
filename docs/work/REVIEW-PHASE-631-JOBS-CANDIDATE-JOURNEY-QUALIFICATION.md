# REVIEW: Phase 631 Jobs candidate journey qualification

Date: 2026-10-27 (America/New_York)
Base: `f4191d094ef1a0f5c3cfd60c86df465b5a8d8016`
Reviewer role: independent source and evidence review
Verdict: 🟢 accept for this bounded local Phase 631 batch; not production or release approval

## Scope reviewed

- `jobs/scripts/nonprod-browser-api-report.mjs`, complete report boundary.
- `jobs/scripts/nonprod-browser-api-report.test.mjs`, complete focused boundary suite.
- `jobs/scripts/run-nonprod-browser-api-tests.mjs`, complete launcher diff.
- `jobs/portal/nonprod/browser-api.integration.mjs`, complete candidate-journey child diff.
- `jobs/package.json`, complete report-test wiring diff.
- Portal workspace types and the Applications, Automation and Settings views needed to validate the
  child assertions against the actual UI contract.
- `docs/rounds/ROUND-631-JOBS-CANDIDATE-JOURNEY-QUALIFICATION.md`, the implementation record and the
  completion ledger.

No product or test source was edited by this reviewer. No Cargo command, service, browser, database,
provider, production account, application, email, deployment or external action was run.

## Resolved validation findings

### P1 — account-isolation case could pass partial cross-tenant reads or writes

The initial child checked that the second account had a different name and no Tracks/applications,
then wrote that account's profile and verified only the first account's name and Track role. A defect
that leaked the first account's unique employment or preferences, or changed another first-account
profile/preference/Track field, would still have advanced `auth_isolation`.

The current child now checks the second account has no first-account employment, desired roles,
desired locations, Tracks or applications; reads back its own distinct profile write; and compares
the first account's complete profile, preferences, Tracks and applications with a pre-write deep
snapshot. This closes both read and write false-green paths for the exercised candidate authority.

Location: `jobs/portal/nonprod/browser-api.integration.mjs:355-370`.

### P1 — effect-availability case did not prove absent effects stayed absent

The initial case only opened three routes, required no page/HTTP 5xx errors and counted one locked
runner card. A fabricated application/run/authorization or a misleading completed state elsewhere
in the views would still have passed.

The current child requires empty application, intervention, browser-session and auto-submit families
before navigation, no source resume, false cloud distribution/availability and false auto-submit
availability. It then requires the Applications empty state, the Automation locked card with the
server-supplied reason and no start/active-run affordance, and Settings' missing-source,
missing-policy and disabled auto-submit state. A fresh API read after all navigation must still show
the four effect families empty.

The first attempted correction incorrectly required `No applications ready` on the base Automation
view. That text exists only inside the queue dialog, which a locked account cannot open. The first
runtime failed closed at `view_automation` after four cases and cleaned its private PostgreSQL/root.
The assertion was removed in favor of the reachable locked-state contract above; this was a harness
defect, not a product defect or a positive qualification result.

Location: `jobs/portal/nonprod/browser-api.integration.mjs:371-400`.

## Static observations

- `passCase` validates the exact next expected case before incrementing. Duplicate, reordered or
  extra calls fail; the result file is written only after all five cases and cleanup complete.
- The report parser requires exact top-level and nested fields, exact version/case order, five
  passes, no failures, two distinct clean exits, bounded safe request counts and absent reported
  PIDs. Its tests reject partial, duplicate, reordered, fabricated and extra-field reports, signal
  death, nonzero exits, invalid PID identity, malformed counts and ambiguous liveness.
- The launcher requires the canonical private root, private Cargo target and loopback generated
  PostgreSQL URL. It hashes current portal source and package/lock metadata, confines Vite output and
  Chromium to the owned root, admits the two private binaries by hash, validates a private mode-0600
  report, and rehashes source before accepting a result.
- The child verifies the PostgreSQL 17 physical data directory, address, port and database, uses
  fixture-only random JWT/data/purge-signing material, binds both APIs and the browser proxy to
  loopback, blocks external browser traffic, and disables browser, mailbox, archive, workflow,
  communication and OAuth effects.
- Both synthetic accounts are explicitly enrolled in the private public-beta fixture. The primary
  browser path exercises all six onboarding screens, Review-first selection, a real reload, exact
  synthetic employment/preferences and a persisted Track through the actual API.
- Normal `npm test` now runs both nonproduction report-boundary files before the existing workspace
  build/tests; no workflow configuration is changed.

## Reviewer-observed checks

- `node --check` passed for the report module/test, launcher and integration child.
- `node --test jobs/scripts/nonprod-browser-api-report.test.mjs jobs/scripts/nonprod-temporal-boundary.test.mjs`
  passed 6/6 with zero failures, skips or cancellations.
- `git diff --check f4191d09 -- jobs` passed on the corrected source.

These are lightweight source/report checks only. They do not execute PostgreSQL, Rust APIs or a real
browser and do not independently reproduce parent runtime evidence.

## Runtime evidence status and limits

Parent-reported historical attempt, not independently reproduced:

- Candidate runtime 1 passed the first four cases, then failed closed at `view_automation` because
  the harness looked for dialog-only text while the account was correctly locked. PostgreSQL stopped
  and the exact root was removed. It is not a five-case pass.

Parent-reported final evidence, not independently reproduced:

- Candidate runtime 2 exited zero on the final frozen child in session 45745. It built fresh default
  Rust binaries, used PostgreSQL 17.10 with vector 0.8.3, and passed the exact five named cases. The
  version-2 report declared five tests, five passes and no failures, with 32 Jobs and six account
  requests.
- Both API processes reported exit code zero and null signal. The outer report validator observed
  both reported PIDs absent. The launcher accepted unchanged source/harness hashes.
- The PostgreSQL wrapper reported zero status, clean shutdown and server exit. The exact successful
  root and the earlier failed-attempt root were both absent; listings of both private parent
  directories contained no run residue. Host free space was 39 GiB after cleanup.
- `git diff f4191d09 --exit-code -- server crates native infra web .github` exited zero. Phase 631
  changes no product backend, UI, SQL, native, infrastructure or CI workflow source; the executable
  scope is the qualification harness, report boundary/tests and root Jobs npm-test wiring.

This round cannot prove signup/login/OAuth, hosted TLS, provider/ATS behavior, resume/object storage,
real employer actions, mailbox delivery, runner execution, capacity, production signing, desktop or
overlay integration, production data or release readiness.

## Current verdict

🟢 **ACCEPT FOR THE BOUNDED LOCAL BATCH** — no remaining P0-P3 finding in the reviewed scope.
The corrected real browser/API/PostgreSQL qualification and exact cleanup passed according to the
parent-reported terminal evidence above. This is not production, deployment or release approval and
does not expand the explicitly listed proof limits.
