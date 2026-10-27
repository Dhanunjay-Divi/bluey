# IMPL: Phase 630 — Jobs runtime qualification

Preflight: `bluey-ops` and Round 630. Base `edeb6a76`.
Status: bounded local runtime qualification complete and separately reviewed.
No production release approval.

## Scope

Real local Temporal/gateway/workflow qualification with synthetic activities,
credential isolation, exact owned cleanup and a measured local baseline. No
deployment, external send, registry write, production flag or host/overlay work.

## Files

- `jobs/scripts/run-nonprod-temporal-tests.mjs`: reviewed pinned tool download,
  environment admission and canonical-launcher child supervision.
- `jobs/workflows/nonprod/temporal.integration.mjs`: actual local service and
  SDK integration; no production API, runner or employer action.
- `jobs/scripts/run-nonprod-browser-api-tests.mjs` and
  `jobs/portal/nonprod/browser-api.integration.mjs`: private portal/Chromium,
  real API/PG fixture, persistence and process-cleanup proof.
- `jobs/workflows/tests/workflow-bundle.test.ts`: real SDK bundle regression.
- Automation package export, workflow import and cleanup adapter/tests:
  FIX-802 / FIX-803; no Rust/SQL/native/product UI implementation changed.
- Round/IMPL/REVIEW/handoff and changelog: observed evidence and limits.

## Baseline and checks

- Phase 629 remains the unchanged-product baseline: PostgreSQL 30/30, Rust
  1,838/1,838, Jobs 2,025, strict Clippy and Linux runner smoke passed. Those
  are prior evidence, not rerun counts for this round.
- Canonical launcher `--self-test` passed on October 27. Its deliberately
  failing cleanup/process-group scenario emitted a retained-status diagnostic;
  the self-test aggregate exited zero, including residue checks.
- Structural code graph returned zero indexed projects; focused file searches
  were used without creating an index or changing repository metadata.
- The Temporal CLI is not globally installed. Public official release metadata
  identifies stable v1.9.1, published 2026-09-14. The Darwin arm64 archive is
  43,195,714 bytes with SHA-256
  `41e0425378fcb4fb5766340b97435e20fe47bbff2d7bf644ec2d51f7662b7c56`.

Official references: [Temporal CLI](https://docs.temporal.io/cli),
[dev server flags](https://github.com/temporalio/documentation/blob/main/docs/cli/command-reference/server.mdx),
[release](https://github.com/temporalio/cli/releases/tag/v1.9.1).

## Open release holds

Local service proof does not imply hosted CI, registry/read-back, signed final
artifacts, hosted PostgreSQL/Temporal/TLS, provider/OAuth/ATS certification, live
runner capacity, or rollout/in-flight rollback approval. Push, external writes
and separately credentialed tests need explicit authorization and resource scope.

## First actual runtime result — failed, not qualification

The canonical launcher source snapshot copied and freshly compiled 70 current
automation/workflows source/config files; digest
`66b95937c7ab9fc2bc03cb5b0c0943a1c114651c4a827e1ceeeffb2ff28deacf`.
The lock digest stayed `8b1f25a86b0e33953f9e32cfee818b97744518443750cd9c8a95c9d43db7ff35`.
The actual official CLI version/size/hash passed admission and its private
service started. The SDK then rejected the production workflow bundle during
`worker_gateway_startup`: Webpack finished with errors. Zero of the ten cases
ran, and the wrapper exited 1. No readiness, application or provider action
is inferred from that failure.

The exact owned root was removed and its run parent was verified empty.
Bounded, redacted SDK build-error lines were then added to the test harness
because a generic error message alone was insufficient root-cause evidence.
The next run is diagnostic; no module ignore-list or other runtime safety
check has been weakened.

The second service attempt reproduced the same failure without useful SDK
module detail. A direct real-SDK bundle regression then reproduced the exact
forbidden Node imports without starting a service. FIX-802 changes only the
package export/import boundary to an existing deterministic module. The same
direct bundle regression subsequently passed 1/1, and workflow typecheck
passed. Both protocols and the production failure converter are included;
there is no ignored-module, custom-global or determinism bypass.

After FIX-802, the actual fresh-source runtime passed eight cases: gateway
authentication/private-input rejection, reconcile-only absence, fresh start,
concurrent exact start/digest fencing, intervention Update/replay, persistent
service/worker restart, uncertain terminal finalization, and real activity
failure-history redaction. It then stopped at exact cleanup (409 identity
conflict); the ten-case batch failed and its private root was removed.

A bounded diagnostic rerun reproduced that cleanup failure. Real Describe
returned matching workflow/run/first-run, type, status, timestamp and exact
opaque memo bytes. Visibility returned the same workflow/run/type/status/time
and exact memo bytes, but its first-run field did not match. A narrow adapter
correction is under review: hydrate only a missing field by an exact-run
authoritative Describe, preserving rejection of any non-empty contradictory
identity and all existing mutation/absence fences. No cleanup pass is claimed.

## Browser / API / PostgreSQL qualification

The additional story is fixture-JWT authentication → actual portal → standalone
Jobs API plus the shared account endpoint → owned PostgreSQL → reload/read-back.
The shared `bluey-server` binary supplies only the existing account/auth HTTP
dependency inside this disposable fixture; no desktop/overlay process starts.
Both binaries bind loopback, receive generated private keys, and keep provider,
mail, calendar, browser and workflow effects disabled. Only the isolated beta
admission master is enabled for the one synthetic enrolled account.

The first attempt compiled both unchanged Rust binaries in 82 seconds and
built the current portal with dotenv/config discovery disabled and private
output (not `web/jobs`). Chromium downloaded into that root. Child admission
then rejected the portal output directory's default mode before any API start
or seed. The outer launcher now creates that directory as mode 0700 before
the build; no admission fence changed. PostgreSQL shut down and the exact root
was verified removed. That attempt did not prove browser functionality.

The second attempt stopped at actual Jobs API startup, before seed or browser
requests. The test had omitted the mandatory runner-volume purge signing policy;
the real binary correctly failed closed. The fixture now generates a fresh
in-memory Ed25519 pair, verifies canonical DER forms and passes the exact four
policy values only to the two isolated API children. No key is logged/written
or borrowed from production; no startup validator or production policy changed.

The third browser attempt PASSED, wrapper exit 0. Both actual debug Rust binaries
built from unchanged source in 79 seconds. Real Chromium loaded the portal,
used fixture-JWT authentication, persisted the manual profile step through the
actual profile/preferences APIs, reloaded into the saved Work history step and
read the same name plus zero matches from PostgreSQL-backed workspace data.
Observed requests: seven Jobs API and two shared account API; zero page errors
or HTTP 5xx. This is not signup/login, resume-file/R2, mailbox or employer proof.
Both API exits were code 0 / signal null; the parent verified their PIDs absent.
PostgreSQL shut down with status 0 and the canonical root was removed. The exact
run parent was empty and no executable process retained its owned path.

Passing Jobs API binary SHA-256:
`650a62b75b656242b7f393b2eaf8cdfcc43b22eb0930766a86747d3e0424838c`.
Passing shared-account API binary SHA-256:
`f41b010c29da8294cafeb268a90deede8c5b05e0a00533067433fd1085eef198`.
These are nonproduction debug fixture builds, not stored production artifacts.

## Cleanup timing and final Temporal rerun

FIX-803's focused cleanup unit suite passed 63/63. Typecheck exposed literal
widening in the new lookup descriptor; a readonly const descriptor preserved
the validated workflow-type union and typecheck then passed. The subsequent
real run confirmed `firstRunEmpty:true` in raw visibility and advanced cleanup
from 409 to accepted/pending 202, but its initial 25-second harness deadline
expired. That attempt failed and its owned root was removed.

The [provider API contract](https://github.com/temporalio/api/blob/master/temporal/api/workflowservice/v1/service.proto)
defines deletion as asynchronous. The actual Rust cleanup dispatcher defaults
to five-second polling and a 30-second confirmation interval. The integration
test now polls every five seconds with a bounded 90-second observation window,
without changing production deadlines or absence checks. Closed receipt
transitions and bounded RPC status codes are observable; a timeout still fails.
Final service qualification PASSED all ten cases, wrapper exit 0. Actual
embedded Temporal server version was 1.32.0 (CLI 1.9.1 / SDK 1.20.2). The raw
visibility field was confirmed empty, so the real test exercised FIX-803 rather
than a present identity. Cleanup transitioned from pending/history-delete to
real absence after 35,104 ms. Describe, exact-run history and exhausted exact
visibility all proved absence; expected gRPC NotFound codes were observed.
The longer harness window changed no production fence or timeout policy.

Both owned Temporal instances shut down with code 0 / signal null; the outer
launcher verified both PIDs absent. The exact root was removed and its parent
was empty, with no executable retaining that root path. The source manifest
digest was `483423c1ae97f7320df534acfb392b1bddd78fc6ed8954be4ddf27e022a5a974`,
70 files; current source and lock matched again after runtime.

Twenty sequential fresh synthetic workflows completed without errors: p50
148.15 ms, p95 153.19 ms, maximum 153.88 ms. This is a local, warm-worker
gateway-to-workflow result baseline with fake activities, not hosted capacity,
real application latency or a production SLO.

## Final quality and review

Full five-workspace quality passed against the frozen correction, wrapper exit
0: 804 automation + 219 Browser + 308 runner + 321 workflows + 387 portal =
2,039 tests, zero failures or skips. All five typechecks and builds passed;
fresh installed dependency audit found zero vulnerabilities. Owned configured
Chromium was verified and the real intercepted exact-submit guard passed,
not skipped. Release tests passed 32/32, schema parity 113 tables / 91 indexes,
and CI guard self-tests passed. Report-boundary Node tests passed 2/2.
Rust/SQL/native/host/overlay sources remain exactly unchanged from `edeb6a76`;
the earlier full 1,838-test server/Clippy gate is baseline evidence, not a new
Rust-suite rerun in this test-only TypeScript round.

All 13 Phase 630 scratch parents were empty after cleanup and no executable
retained an owned root path. Host free space was about 39 GiB. Expected PDF
fixture warnings and the existing 503.99 kB portal chunk warning remain visible;
no assertion, warning threshold or release fence was increased.

The final canonical launcher self-test, report-boundary tests (2/2), and
provenance/license inventory (663 lock entries / 631 versions / 14 pinned
references) passed. Indexed privacy passed 2,824 paths / 2,548 text files;
final documentation is restaged and rescanned before commit. Independent AI
review accepted only this bounded local batch with no open finding.

The Phase 629 Linux image remains historical smoke evidence, not an exact
artifact for these changed workflow bytes. Final source-qualified Linux images
must still be built/signed/read back through the approved release path before
promotion. No unpushed local build, fixture key or dev Temporal service closes
hosted CI, TLS, provider, bucket/OAuth/ATS, fleet capacity or rollout gates.
