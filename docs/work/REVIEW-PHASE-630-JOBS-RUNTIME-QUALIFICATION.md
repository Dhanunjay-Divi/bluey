# REVIEW: Phase 630 Jobs runtime qualification

Date: 2026-10-27 (America/New_York)
Base: `edeb6a767c357909953d1e18261265f546ec6908`
Reviewer role: independent, read-only source and evidence review
Verdict: 🟢 accept for this bounded local Phase 630 batch; not production or release approval

## Scope reviewed

- `jobs/scripts/run-nonprod-temporal-tests.mjs`, complete new launcher.
- `jobs/scripts/nonprod-temporal-boundary.test.mjs`, complete report-boundary suite.
- `jobs/workflows/nonprod/temporal.integration.mjs`, current complete worker-owned draft.
- `jobs/workflows/tests/workflow-bundle.test.ts`, complete real-SDK bundle regression.
- `jobs/automation/package.json` and `jobs/workflows/src/workflows.ts`, complete FIX-802 diff.
- `jobs/workflows/src/gateway-cleanup-service.ts` and its complete focused test diff, FIX-803.
- `jobs/scripts/run-nonprod-browser-api-tests.mjs`, complete browser/API launcher.
- `jobs/portal/nonprod/browser-api.integration.mjs`, complete browser/API integration child.
- Production gateway, cleanup service, workflow and failure-converter call paths used by the
  integration file.
- `docs/rounds/ROUND-630-JOBS-RUNTIME-QUALIFICATION.md` and the current implementation record.

No product or test source was edited by this reviewer. No service, Cargo command, download,
provider, production account, application, email, registry, signing or deployment action was run.

## Resolved finding

### P1 validation blocker — ignored ambient `dist` was not bound to current production source

The initial runtime child imported the gateway, cleanup, workflow and failure-converter
implementations from the ignored ambient `jobs/workflows/dist`. File existence did not bind those
bytes to the reviewed production source, so a stale prior build could have produced a false-green
Temporal result.

The correction removes that ambient-product-code dependency. The launcher now copies current
`automation` and `workflows` package metadata, TypeScript configuration and source into the owned
runtime root, rejects source symlinks, bounds the copy to 2,000 files and 32 MiB, records each source
hash, and compiles both packages directly with the installed TypeScript compiler without running npm
scripts. It rehashes every original source file after compilation and records the source-manifest and
current lockfile digests. The child requires the exact owned workspace path, exact owned `@bluey`
package links and the required owned compiled workflow files, then dynamically imports and configures
the production gateway, cleanup service, workflow and failure converter only from that owned output.

I also inspected the current installed package topology used for external dependency resolution:
no non-`@bluey` top-level package is a symlink outside `jobs/node_modules`; the repository workspace
`@bluey` links are excluded and replaced by the exact owned automation/workflows links. This closes
the reported current-source identity defect. At that checkpoint the broad production automation
barrel was intentionally left visible to the actual runtime rather than bypassed; the run then
correctly exposed the separate workflow bundle defect recorded below.

Locations:

- `.gitignore:11`
- `jobs/scripts/run-nonprod-temporal-tests.mjs:61-121`
- `jobs/scripts/run-nonprod-temporal-tests.mjs:205-207`
- `jobs/workflows/nonprod/temporal.integration.mjs:139-181`
- `jobs/workflows/nonprod/temporal.integration.mjs:195-224`

## Positive static observations

- The launcher accepts no endpoint, executable or credential argument. It requires Darwin arm64,
  rejects inherited Temporal/database/Node-hook configuration, verifies the canonical private test
  root and marker, and passes the child an explicit credential-free environment with a private home.
- The official CLI archive is pinned to version 1.9.1, exact byte length and SHA-256, streamed with
  an upper bound into a new mode-0600 file, restricted to expected release hosts, and extracted as
  one named file into the owned root. The executable is not globally installed.
- The runtime binds Temporal and the gateway to loopback ephemeral ports. Synthetic opaque
  activities are supplied directly to the Worker; no production activity implementation, Jobs API,
  runner, provider or employer action is imported or invoked.
- The integration covers pre-creation authentication/private-input rejection, reconcile-only
  absence, fresh and concurrent start identity, changed-digest conflict, intervention Update
  binding and exact replay, persistent server/worker restart, terminal uncertainty, non-retryable
  failure-history privacy, exact cleanup and 20 local latency samples.
- Child and server output is byte-bounded. SDK logs are replaced by a counting sink. Runtime
  failures report only an opaque stage identifier. The child attempts every owned cleanup handle,
  requires two clean Temporal exits, writes its report only after cleanup, and fails closed on
  interruption.
- The outer report parser requires exact top-level, server and latency keys; a positive bounded test
  count; no failures; clean zero/null server exits; dead reported PIDs; at least 20 finite ordered
  latency samples; and no extra fields.
- The current-source correction binds product JavaScript to a fresh owned compile. External npm
  packages remain the current local installation identified by the recorded lockfile hash rather
  than a fresh package installation inside the runtime root; this is a bounded local qualification,
  not a hermetic dependency-reproducibility proof.

## Resolved runtime blocker

### P1 qualification blocker — real Temporal Worker bundling failed before every case

The first fresh canonical runtime reached the owned Temporal server and then failed at
`worker_gateway_startup`: `Worker.create` reported `Webpack finished with errors`. No qualification
case ran. This is a real-path failure, not evidence against the source-binding correction: the
launcher first copied and compiled the current 70-file production source snapshot, recorded its
digest, admitted the exact CLI, and only then reached the SDK Worker bundle.

The current failure output intentionally suppresses SDK details and therefore does not yet prove
which module violates the workflow bundling boundary. The production workflow imports the broad
automation barrel, which reaches Node/PDF/browser dependencies and is a plausible cause, but that is
only a hypothesis until a bounded non-private Webpack module diagnostic identifies the exact import.

The direct real-SDK bundle regression then reproduced `UnhandledSchemeError` for `node:crypto`,
`node:buffer`, `node:dns/promises`, `node:fs/promises`, `node:fs`, `node:https` and `node:net`,
confirming the automation barrel as the cause. FIX-802 exposes the already-existing deterministic
`approved-execution` module through an explicit package subpath and moves only the two workflow
runtime helper imports to it. `SubmissionReceipt` remains an explicit type-only root import and is
erased. The checksum, validation and snapshot implementations are unchanged. The new regression
invokes the real `bundleWorkflowCode` boundary with the production workflow module and failure
converter, does not ignore modules or replace Node globals, and checks that both workflow protocols
remain in the produced bundle.

Parent-reported post-fix evidence is one passing real bundle test plus a passing workflows typecheck
and fresh automation build. The final fresh real-service qualification also passed all ten cases,
so this P1 is resolved both at the bundle boundary and through the exercised service path.

### Resolved runtime interoperability defect — visibility omits the first-run chain identifier

The post-FIX-802 real run passed the first eight cases and reached exact cleanup, but both cleanup
attempts returned 409. Bounded diagnostics showed the exact-run Describe identity, memo encoding and
bytes, type, time and first-run identifier all matched the request. The corresponding visibility row
matched workflow ID, run ID, type, status, memo and time, but its decoded `firstRunId` was an empty
string. The cleanup service correctly treated that missing chain authority as an identity conflict.

The proposed correction is acceptable only as an adapter normalization: for a missing/null/empty
visibility `firstRunId`, load exact workflow+run Describe and copy only a valid described first-run
identifier after exact workflow/run/type validation. Never overwrite a nonempty visibility value,
never infer from the caller or latest run, reject an oversized page before any hydration calls, and
map every stale NotFound/error/mismatch to unavailable/pending rather than absence. The original
visibility status, time and memo and all service mutation/absence fences must remain unchanged.
Hydration for a maximum-size page also needs bounded concurrency or equivalent deadline evidence;
100 sequential RPCs under the existing request deadline would create an avoidable cleanup outage.

FIX-803 implements those constraints only in the raw visibility adapter. It validates all
missing-root lookup identities first, hydrates ordered batches of at most eight exact workflow/run
Describes inside the caller's deadline context, copies only the valid first-run field, preserves the
visibility memo/status/time, and leaves every nonempty root untouched. A stale NotFound or mismatch
throws into the existing unavailable/pending path. Real-protobuf, AsyncLocalStorage deadline,
foreign identity, stale row, oversize/malformed, maximum-concurrency/order and failed-batch tests
cover the boundary; the focused cleanup suite passed 63/63 and workflow typecheck passed.

The first real post-fix attempt advanced cleanup from 409 to accepted/pending 202, proving that the
adapter corrected the transport mismatch, then correctly failed its old 25-second observation
window. The harness now polls every five seconds for at most 90 seconds, still bounded by the global
deadline. A timeout remains failure. The final run observed pending to `absence_observed` after
35,104 ms, then independently required Describe NotFound, exact-run history NotFound and exhausted
exact visibility. Production cleanup deadlines and all authority/mutation/absence fences are
unchanged.

## Browser/API static review

No pre-run P0/P1 was found in the current two-file browser/API harness.

- The launcher requires the private canonical-root layout and exact private Cargo target, rejects an
  inherited owner database URL, constructs a credential-free build/install environment, confines
  Vite output and a fresh Playwright browser download to the owned root, and passes only the
  generated loopback PostgreSQL authority to the child. Portal source is symlink-rejected and hashed
  before the build, then rehashed before a positive result is accepted. Both private Rust binaries
  are admitted from the exact owned target and their SHA-256 values are reported.
- The child validates the physical PostgreSQL 17 data directory, loopback address, port and fresh
  database before schema mutation. Its psql processes use `-X` and a private home. The two Rust
  services receive random fixture-only JWT and data keys, loopback binds, PostgreSQL backend, beta
  access enabled, and model/browser/mailbox/archive/workflow/communication/OAuth/reconciliation
  effects explicitly disabled.
- The browser permits requests only to the owned loopback proxy. The UI cannot reach onboarding
  unless the real beta/workspace and account requests succeed. The step transition awaits the real
  profile and preference writes, the reload re-reads the workspace, and a final authenticated
  production workspace request checks the saved synthetic name and empty matches against the
  PostgreSQL-backed API. No desktop overlay, login flow, provider, ATS, employer, resume upload,
  workflow dispatch or cloud-browser behavior is claimed.
- A positive result requires both Rust binaries to exit with code zero and no signal, both recorded
  PIDs to be absent, positive Jobs/account request counts, no child-reported failure and unchanged
  portal source. The canonical launcher and PostgreSQL wrapper retain process-group/root cleanup
  authority for interrupted or failed runs.

The first browser attempt failed closed on private portal-directory mode before service/seed; the
second failed closed at Jobs API startup because the test omitted the mandatory runner-volume purge
signing policy. The final fixture uses a fresh in-memory Ed25519 seed/public key pair with exact DER
and length checks and passes the four mandatory test-only policy values only to the isolated child
processes. No key file or production signing key is created, and no validator is weakened.

The third browser run passed the real path: current private Vite output, fresh Chromium, both Rust
APIs, fixture JWT, private PostgreSQL, manual profile save, reload into Work history and matching
workspace readback. It observed seven Jobs and two account requests with zero page errors or HTTP
5xx. Both Rust binaries exited zero/null and their PIDs were absent; PostgreSQL stopped cleanly and
the exact root and parent were empty. This does not prove signup/login, object storage, provider/ATS,
mailbox, employer action, desktop/overlay or production-browser behavior.

## Evidence status and limits

Reviewer-observed checks:

- `node --check` passes for the launcher and runtime integration files.
- `node --check` passes for both browser/API harness files; their current source also passes
  `git diff --check`.
- `node --test jobs/scripts/nonprod-temporal-boundary.test.mjs` passes 2/2 with zero failures,
  skips or cancellations.

Parent-reported evidence, not independently reproduced by this reviewer:

- The CLI version, archive length and SHA-256 were checked against official public release metadata.
- The first actual runtime used a current 70-file source snapshot, admitted CLI 1.9.1 and started the
  owned server, then exited 1 at `worker_gateway_startup` with `Webpack finished with errors`.
  It passed 0/10 cases. The canonical run root was removed and its private parent was empty afterward.
- A bounded direct real-SDK regression reproduced the seven disallowed Node module schemes before
  FIX-802. After the correction, the real bundle test passed 1/1, workflows typecheck passed and a
  fresh automation build completed successfully in parent session 28351.
- The corrected real runtime passed 8/10 cases, then both exact-cleanup attempts returned 409. The
  Describe/visibility comparison isolated an empty visibility `firstRunId`.
- FIX-803 focused cleanup tests passed 63/63 and workflows typecheck passed. The final Temporal run
  exited zero with 10/10 cases on server 1.32.0, SDK 1.20.2 and CLI 1.9.1. Wrong-digest cleanup stayed
  denied; exact cleanup reached proven absence after 35,104 ms. Both server exits were zero/null and
  their PIDs were absent. The 70-file source digest was
  `483423c1ae97f7320df534acfb392b1bddd78fc6ed8954be4ddf27e022a5a974`; source and lock were rechecked.
- The local warm-worker synthetic latency sample had 20/20 completions and zero errors: p50 148.15
  ms, p95 153.19 ms, maximum 153.88 ms. It is not a hosted or production SLO.
- The third browser/API run exited zero after the real manual-profile save/reload/readback path, with
  seven Jobs and two account requests, zero page errors/HTTP 5xx, two clean zero/null API exits and
  dead PIDs. PostgreSQL and canonical-root cleanup completed successfully.

Final parent-reported aggregate evidence, not independently reproduced by this reviewer:

- A fresh `npm ci` installed the unchanged lock and the installed-tree audit reported zero findings.
- All 2,039 tests passed with no skips: automation 804, browser 219, runner 308, workflows 321 and
  portal 387. This includes the real workflow-bundle regression and 63 cleanup tests.
- All five workspace typechecks and builds passed. The private configured Chromium exact-submit
  guard passed rather than skipping. Release checks passed 32/32; schema verification found the
  expected 113 tables and 91 indexes; CI guards passed.
- All four harness syntax checks passed. Rust/crates/native/infra/web/GitHub workflow sources were
  byte-unchanged from `edeb6a76`, so the Phase 629 1,838-test Rust/strict-Clippy result remains prior
  baseline evidence and was not rerun or relabeled as Phase 630 evidence.
- All 13 Phase 630 run parents were empty, no executable command retained an owned root and the host
  had 39 GiB free at closeout.

Reviewer-observed final source checks: current `node --check` passed the four JavaScript harness
files; tracked production/test changes passed `git diff --check`; and each new Phase 630 source,
test and documentation file passed an individual no-index `git diff --check`.

The bounded local acceptance covers the reviewed current-source compile, real local Temporal path,
synthetic activities, private browser/API/PostgreSQL fixture and aggregate Jobs checks only. Hosted
Temporal/TLS, hosted PostgreSQL, signup/login, object storage, production identities, live runner
capacity, provider/ATS behavior, employer effects, final signed artifacts, physical cross-platform
qualification, deployment and rollback remain outside this batch. This document is not merge,
release, deployment or production-flag approval.
