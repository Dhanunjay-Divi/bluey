# REVIEW: Phase 638 Jobs runner control plane

Date: 2026-10-28 (America/New_York)
Base: `409498589b8d2e62b5ef54ebdee1372aeb8d005f`
Reviewer role: independent read-only design/source review
Verdict: 🟡 Current-source Rust gate passed; API preservation, human hold and release gates remain

## Scope reviewed

- Current `AGENTS.md` and the `bluey-ops` preflight.
- `docs/rounds/ROUND-638-JOBS-RUNNER-CONTROL-PLANE.md` and
  `docs/work/IMPL-PHASE-638-JOBS-RUNNER-CONTROL-PLANE.md`.
- Ordinary cloud queueing, workflow-command materialization and execution-target/ATS Phase A
  identity flow in `server/src/api/jobs.rs`, `server/src/db/jobs/workflow_commands.rs` and
  `server/src/db/jobs/execution_leases.rs`.
- Worker authentication and real runner client behavior in
  `server/src/api/jobs_worker_auth.rs`, `server/src/api/jobs_runner_volumes.rs`,
  `jobs/runner/src/execution-lease.ts`, `leased-run.ts` and the runner server call site.
- Existing integration-feature fixture shape and positive managed release/runtime/volume helpers.

No product, test, fixture or plan source was changed by this reviewer. No build, test, service,
browser, VM, network, provider or external-account action was run. Phase637 evidence is historical
context only and is not counted as Phase638 execution evidence.

## Source diagnosis

The causal red now observed in session79181 is explained by current source:

1. `queue_application_run` derives a40-hex execution run ID and stages the browser session as
   `cloud-{application_id}`.
2. The durable start command, materialization response, workflow activity and runner request retain
   the distinct `runId` and `browserSessionId` values.
3. `sqlite_execution_target` and `postgres_execution_target` query `jobs_browser_sessions` by the
   run ID, and `validate_execution_target_payloads` requires `session.id == run_id`.
4. ATS Phase A construction independently assigns `browser_session_id` from `context.run_id`.

This should fail closed at actual lease admission; it is not evidence of an unsafe effect or a
successful cloud runner. Existing tests that construct `session.id = run_id` cannot qualify the
ordinary route.

## Resolved plan finding

- **Resolved P1 — the proposed wrong-worker acceptance could falsely claim a lifecycle worker
  binding that current heartbeat/finish APIs do not enforce.** The claim handler binds worker ID
  to the runner-volume authority and rejects managed claims from the debug worker. In contrast,
  heartbeat and finish handlers do not pass `JobsWorkerIdentity` into their database boundaries;
  they authorize account/application/run plus lease token/fence and retained volume identity. The
  HMAC signing key is shared, so a test that only proves a mismatched worker is denied at claim
  does not prove heartbeat or terminal finish is worker-ID-bound. Make the negative matrix name
  its exact operation: wrong worker/proof must deny **claim**. For heartbeat and finish, prove fresh
  worker-auth signatures/nonces and wrong token/fence denial. If Phase638 intends worker-ID binding
  on those later operations, that is a separate production authority change requiring explicit
  design and regressions; it must not be inferred from the claim test. Round638 D0.3 now scopes
  wrong-worker denial specifically to claim and names signed transport plus retained token/fence/
  volume as heartbeat/finish authority. IMPL638 records the same boundary without promising a new
  production binding.

No P0-P3 plan finding remains at this checkpoint. The original ambiguity was an evidence-definition
issue, not a demonstrated cross-worker exploit: later operations still require the opaque lease
token/fence and signed worker channel.

## Fixture proposal sanity checkpoint

The proposed five-file, debug/integration-only fixture shape is provisionally clear, subject to the
following implementation checks. These are prerequisites for a valid red/green observation, not
runtime evidence:

- Keep `BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED` false in the actual debug Jobs API. Scope
  selection and customer admission do not require the embedded runtime reporters. Enabling them
  would instead require three exact grant IDs/tokens, the measured
  `/usr/local/bin/bluey-jobs-api` executable and the release measurement rooted at `/`; the
  synthetic Linux/x64 fixture must not be passed off as that live measurement. The seven bootstrap
  role records may establish database readiness, but their session tokens must remain in the
  mode-`0600` bridge and must not be injected as a substitute runtime identity.
- Start the API before or immediately around bootstrap and fail closed if the post-queue readback
  does not find all seven exact role heartbeats still current. The fixture head uses a bounded
  heartbeat TTL, so compilation or an unbounded startup delay between bootstrap and queueing would
  otherwise turn the test into an unrelated readiness failure. Do not silently refresh by
  installing a replacement head.
- Keep the worker HMAC signing key distinct from the generated runner-volume Ed25519 seed. The
  former authorizes the signed HTTP transport; the latter proves the exact retained volume. Neither
  belongs in stdout, diagnostics, argv or the non-secret report.
- A 503 from the owned loopback observer is a sound way to exercise the normal dispatcher without
  starting Temporal: current code records request-start evidence and completes the command as
  `delivery_unknown`. The ordinary effect claimant then requires
  `first_request_started_at_ms IS NULL`, while reconciliation remains separately disabled. Runtime
  evidence must nevertheless observe exactly one POST and wait beyond the five-second retry base
  and at least one further dispatcher poll before asserting no second effect request. Hash or bound
  observer diagnostics rather than echoing the command body or authorization header.
- The post-queue helper may tolerate the race from `pending` through `delivering` to
  `delivery_unknown`, but its accepted terminal snapshot must require the exact request-start event,
  `first_request_started_at_ms`, command/request/workflow IDs and immutable memo. It remains
  read-only and must prove the absence of a `session.id = run_id` alias; only the later real
  `ExecutionLeaseClient` is allowed to become the first lease claimant.

With those checks, the fixed staging/us-east-1/general v2 synthetic authority is accurately
described as fixture authority, not native runtime measurement, and the observer proves only a
local gateway delivery ambiguity. It provides no Temporal acceptance or workflow-run proof.

## Current implementation review (pre-runtime snapshot)

Reviewed the newly written runner adapter, reproduction child, inner launcher, report-negative
tests, guarded Rust example and fixture module. The present implementation correctly keeps the
actual debug API on staging/us-east-1/general with
`BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED=0`; uses the ordinary account, source, packet, approval,
queue and signed worker HTTP paths; checks the physical private PostgreSQL data directory before
fixture mutation; makes post-queue inspection a PostgreSQL read-only transaction; and requires the
exact `delivery_unknown` request-start command with no run-ID session alias or prior lease.

One authority-labeling distinction must remain explicit. The unchanged positive-source helper
used to create the signed posting installs its historical synthetic
production/us-east-1/general original-source-verifier authority inside the physically verified
private database. The new runner/admission head is separately fixed to
staging/us-east-1/general, and the actual debug API never selects production. This is reuse of a
test composition, not observation of a production verifier, production runtime or external
resource.

### Resolved implementation findings

- **Resolved P1 — cleanup failure could be reported as successful.** The original boolean guard
  remembered only that cleanup had started. The frozen child now installs a single-flight cleanup
  promise before execution and every success/error caller awaits that same promise; a rejection is
  sticky. Pure tests cover repeated success and repeated failure. Actual process, listener and port
  cleanup is still a runtime gate.
- **Resolved P2 — observer receipt raced durable dispatcher completion.** The child now polls the
  exact durable request to `delivery_unknown` under the bounded operation deadline before invoking
  the read-only post-queue helper. Intermediate state is limited to
  `pending`/`claimed`/`delivering`; the helper remains strict about one request-start attempt and
  does not publish state.
- **Resolved P2 — `noMutation` overstated the measured snapshot.** The report now names
  `protectedClaimStateUnchanged` and carries the exact fourteen-table family list, including the
  workflow command, managed binding and request-start authority. It explicitly excludes the
  expected worker-auth replay reservation instead of claiming that no system state changed.
- **Resolved P2 — bootstrap-only readiness did not prove all seven roles remained current at the
  claim boundary.** Reusing the internal readiness resolver inside the fixture's read-only
  transaction was correctly rejected because its PostgreSQL path takes `FOR SHARE` locks. The
  frozen driver instead repeats the ordinary cloud queue API immediately before claim. That handler
  re-evaluates production fleet and managed-cloud customer admission, including the current head
  and all required role heartbeats. The driver requires an exact replay of the same run, session,
  command and request and an unchanged fourteen-family snapshot. It does not use the workspace GET,
  whose handler schedules background discovery work, and it adds no fixture trust query.

Parent-reported lightweight validation on the frozen implementation is Node syntax plus 27
report/cleanup tests passed with zero skips and a clean scoped diff check. These checks exercise
report rejection and cleanup promise semantics only; they are not PostgreSQL, API, causal-red,
Rust-compilation or actual cleanup evidence.

Parent also reports the repeated prospective-index whitespace/privacy scan passed over all sixteen
owned paths (2,898 tracked paths and 2,622 text files), with the real index unchanged and scratch
removed. This is mechanical evidence only and does not advance the runtime verdict.

### First recipe attempt and narrow compile correction

Parent-reported recipe session74589 exited101 during Rust compilation with E0308 before any API,
dispatcher or claim execution: the fixture passed the stored workflow-binding representation to
the gateway validator, which requires the resolved binding. PostgreSQL stopped cleanly, the exact
root and parent were removed/empty, and the admitted772-file source manifest was unchanged. This is
a dated compile failure, not causal-red evidence.

The narrow correction is statically sound. Post-queue now passes the stored row plus the exact
request command/account/application/run/workflow IDs and fixed staging scope through the existing
`require_exact_managed_cloud_workflow_binding`, then supplies that resolved binding to the existing
gateway validator. The helper verifies subject HMACs, binding digest and release memo; it performs
no write and preserves the repeatable-read/read-only transaction. A successful rerun remains
required.

Parent-reported session23761 then compiled the API and fixture successfully but failed before API
startup while importing the bundled runner adapter: the automation root barrel eagerly initialized
PDF.js and required `DOMMatrix`. PostgreSQL stopped, the exact root/parent were removed/empty and
the 772-file source manifest remained unchanged. This is another setup failure, not causal-red
evidence.

The test-controller-only import correction is narrowly valid. `execution-lease.ts` has one runtime
root import, `assertFinalSubmitProof`; the bundler now requires that exact importer and verifies the
production automation index still re-exports `./final-submit-proof.js`, then resolves the root to
that actual production module. Subpath imports retain their reviewed source mappings. No mock or
product source is substituted. The new `--preflight-client` mode builds and imports this exact
private bundle, rechecks source hashes and exits with `sourceAccepted:false` before Cargo, without
requiring compiled binaries. Its runtime pass remains unverified.

Parent-reported session73022 passed that preflight and Rust compilation, then reached the ordinary
profile/source/onboarding flow. Bootstrap failed with the closed `bootstrap_lifecycle` label before
claim. The controller's esbuild service remained alive after its assertion; the parent identified
and terminated only the exact owned controller/process group, after which PostgreSQL stopped and
the root/parent/PIDs were absent. The recorded source manifest is explicitly pre-run only; no
post-run hash equality is inferred. This is a fixture diagnostic failure, not causal-red evidence.

The two narrow harness corrections are statically clear. The bundling call now invokes esbuild's
service stop in `finally`, covering success and throw. Child cleanup still makes the primary fixture
failure abort the run, but accepts an already-reaped failed helper as proof of process exit; a
successful causal report independently requires both fixture outcomes and the API to be code0,
signal-null and absent. Rust bootstrap operations now add closed stage contexts, and the example
maps only an exact allowlist to fixed diagnostic codes; unknown/private error chains remain the
generic `bootstrap_lifecycle`. The mapper test confirms private text is not emitted. Actual cleanup
and the newly exposed bootstrap stage remain pending rerun.

Parent-reported session72929 exited1 automatically after preflight and Rust compilation when
bootstrap returned `bootstrap_volume_fleet`; packet, queue and claim were not reached. Cleanup was
reported true, PostgreSQL exited, the root/parent were removed/empty, the before/after 772-file
manifest matched and HEAD/status were unchanged. Read-only comparison did not establish a fleet
guard or value defect, so no lifecycle check was changed.

The follow-up diagnostic-only delta is statically clear. `prepare_fleet` retains the same call
order, values and final assertions; it only attaches fixed contexts to initial status, inventory
begin/ready/readback, cutover begin/ready and final readback. The example's allowlist checks these
inner labels before the outer volume-fleet label and still maps unknown/private chains to the
generic code. No production or fixture authority is relaxed. The exact inner-stage runtime result
remains pending.

Parent-reported session62571 narrowed the next actual failure to
`bootstrap_fleet_cutover_begin`; automatic cleanup, PostgreSQL/root removal and the equal 772-file
manifest were observed. Static inspection identifies a serious PostgreSQL-only query defect in
`record_runner_volume_fleet_cutover_postgres`: the unready-volume predicate opens three enclosing
parentheses (`AND (`, outer `NOT EXISTS (` and nested `NOT EXISTS (`) but ends with four. The SQLite
query has the same four closings correctly because it has an additional scalar
`(SELECT COUNT(*) ...)` wrapper. Therefore the PostgreSQL text has one extra closing parenthesis.

- **Resolved P1 — PostgreSQL fleet cutover was syntactically unavailable.** Parent-reported private-PG
  probe62355 extracted the exact965-byte production PostgreSQL unready-count query (SHA-256
  `6bd923ad639c3b7e20a808f22d71acc15af01667ac47c5615c5396c84e6779fc`) and received SQLSTATE42601
  from read-only `EXPLAIN` on PostgreSQL17.10; no schema or source was changed and PostgreSQL exited
  cleanly. Together with session62571 failing at cutover begin, this confirms a prerequisite
  availability/release blocker, not an authority bypass. The minimal correction is PostgreSQL-only
  removal of the extra closing parenthesis. Keep the balanced SQLite query and every predicate/input
  unchanged, and qualify actual PostgreSQL lifecycle, exact replay and stale-snapshot rejection.

### FIX819 static and partial-query review

The production correction now on disk removes exactly the one extra PostgreSQL closing
parenthesis. The SQLite query is byte-unchanged. No predicate, input, lock or authority behavior is
otherwise modified; the corrected PostgreSQL predicate is structurally balanced.

The new configured-PostgreSQL regression is appropriately closed: it requires the configured URL
when PostgreSQL tests are mandatory, serializes with the existing boot lock, requires a fresh empty
database rather than resetting append-only authority, executes reconciling then ready through the
production function, checks typed full-state equality for both exact replays, advances inventory
through the existing authorized lifecycle, and requires the old accepted request to return
`Conflict` with the advanced state unchanged. The empty-fleet regression proves parser/lifecycle
and replay behavior; the ordinary one-volume fixture remains necessary for the causal-red rerun.

Parent-reported probe90815 exercised the corrected source-derived964-byte query against fresh
private PostgreSQL temporary tables across eight empty/missing/current/newer-generation/tombstone/
status/destroyed cases; all eight passed and PostgreSQL/root cleanup was verified. This supports
predicate semantics but does not execute `record_runner_volume_fleet_cutover_postgres` or qualify
the new Rust test. At that checkpoint, the P1 remained a runtime hold until those gates passed.

Parent-reported focused session2776 subsequently exited0 with eight tests passed and zero
failed/ignored/skipped: one actual-PostgreSQL two-phase lifecycle/replay/stale-successor regression,
two helper pure tests, four example guard/redaction tests and one SQLite parity case. The configured
PostgreSQL case completed in0.97s and SQLite in1.67s after a2m32s test-binary build. PostgreSQL
exited, the exact root/parent were removed/empty and free space was38GiB. The772-file manifest
`4acfd0ed62eaa25eb52381b890f975bb4cf230d5cc464f5aabcc3caa7a6c8efc`, HEAD`40949858` and scoped
status matched before/after.

This resolves the parser/function/replay portion of FIX819 on an empty fleet. Parent-reported
session81361 subsequently traversed the ordinary one-volume bootstrap/fleet path and completed
factual preparation and approval. It failed later during queueing with the old generic
`closed_stage_failure`, before the intended claim. PostgreSQL and the owned root cleaned up
automatically, and the772-file manifest
`4acfd0ed62eaa25eb52381b890f975bb4cf230d5cc464f5aabcc3caa7a6c8efc`, HEAD and scoped status
matched. This closes the nonempty FIX819 prerequisite. It does not identify the queue cause,
observe the run/session claim defect or qualify the broader phase.

### FIX820 redacted diagnostic review

The narrow Node-only diagnostic change is statically clear. Queue phases now have distinct fixed
labels. Each ordinary request records only its internally supplied HTTP method, numeric status and
elapsed milliseconds around the complete bounded fetch, body read and JSON parse. Failure
classification exposes an allowlisted error type and system code,
and a numeric line only when the stack points back to this exact owned script; arbitrary native
error names, codes and messages remain closed. Assertion text is exposed only when Node marks it as
`ERR_ASSERTION` at that owned call site and the first line is a bounded alphanumeric/underscore
label. `FixtureGateError` is private to the module and is raised only with owned fixed labels.

The optional PostgreSQL diagnostic is also bounded and read-only: at most32 rows from the already
physically verified private database, containing only PID, server state, wait-event type/event and
numeric blocker PIDs. It does not select query text, bind values, client addresses, connection
strings or application data, and a diagnostic-query failure is swallowed before the independent
sticky cleanup is awaited. The original failure remains the terminal outcome, request timeout is
unchanged, and none of these fields can satisfy the causal-red report contract.

Parent reports Node syntax/scoped-diff checks and30 report/cleanup/privacy tests passed with zero
skips (evidence marker`052e42`). Those were local helper checks, not an independently rerun queue,
PostgreSQL or causal-red result. At that checkpoint, the queue cause remained unobserved pending a
fresh private run with bounded metadata.

Parent-reported run84038 then reached queue and failed during the local fetch after244ms with an
allowlisted `TypeError`, null HTTP status, eighteen private PostgreSQL activities all idle on
`ClientRead`, and no blockers. It was not the15-second timeout. PostgreSQL and the owned root were
cleaned automatically, the772-file manifest
`1daef5...`, HEAD and scoped status matched, and free space was38GiB. This evidence narrows the
failure boundary but does not establish whether the socket, API process or an underlying panic was
causal, and it still did not reach claim.

The follow-up FIX820 diagnostic delta is also statically clear. A nested transport cause is emitted
only when its code is in the fixed Undici/system allowlist; its message and other cause fields are
never copied. The API stderr tail remains private and bounded. Its public fingerprint contains only
a Rust source basename and numeric line parsed from a panic marker, one fixed coarse category and a
SHA-256 of the tail; it excludes directories, panic text and log content. Absence of a panic marker
is explicit. The category is a heuristic over the bounded tail and therefore must remain
diagnostic-only, never root-cause or acceptance evidence. Cleanup and the primary failure outcome
remain independent of fingerprint generation.

Parent reports the expanded32 redaction/report tests passed with zero skips (evidence marker
`589971`) and the latest syntax/scoped-diff check passed (`77601c`). These were not independently
rerun and did not themselves qualify the API, PostgreSQL or causal red.

Parent-reported run96006 reproduced the queue failure after251ms as `TypeError` with nested
`UND_ERR_SOCKET`, null HTTP status and a redacted API panic at
`workflow_commands.rs:2317`, category`postgres_row_type`. Eighteen private PostgreSQL activities
were idle with no blockers; PostgreSQL/root cleanup completed and the772-file manifest
`e59d0f78...`, HEAD and scoped status matched. The production source at that line decodes
`jobs_entitlements.cloud_browser` as PostgreSQL `bool`, while the authoritative schema declares it
`INTEGER` and the existing ordinary entitlement reader correctly uses `i32 != 0`.

- **Resolved P1 — PostgreSQL cloud-browser entitlement reads panicked on the authoritative INT4 schema.**
  The observed queue line is not the only affected consumer. The same incorrect Boolean decode is
  present in workflow resume (`workflow_commands.rs:3099`), managed-cloud effect admission
  (`managed_cloud_release_authority.rs:10440`) and both execution-lease PostgreSQL paths
  (`execution_leases.rs:1913` and`:3934`). Correcting only the observed line would move the next
  availability failure downstream. Use one fallible shared PostgreSQL row decoder based on
  `try_get::<_, i32>` and preserve the established nonzero-enabled semantics. Optional missing
  entitlement rows remain disabled; an actual type mismatch must return the caller's storage or
  unavailable error rather than panic. Keep every existing `FOR UPDATE`, fleet/current-authority
  ordering and account fence unchanged. Do not migrate/cast the column to Boolean and do not add a
  permissive fallback.

  The minimum regression boundary is actual PostgreSQL decoding of0 as false,1 and another nonzero
  integer as true, and a deliberately wrong Boolean type as an error without unwind. The ordinary
  API reproduction must then traverse queue and the real claim path, so that managed-cloud and
  lease consumers are exercised instead of merely proving the first call site. Resume and terminal
  replay behavior remain covered by their focused PostgreSQL paths; no lifecycle rule is weakened
  by this type correction.

### FIX821 frozen implementation review

The four-module correction is statically sound and complete for the identified defect. A private
named-column helper in `applications.rs` calls
`Row::try_get::<_, i32>("cloud_browser")` and maps every nonzero value to enabled. The existing
ordinary entitlement reader now uses that helper, as do both workflow-command consumers,
managed-cloud effect admission, execution-lease claim and irreversible-submission start. Optional
missing entitlement rows still resolve to disabled through `transpose()?.unwrap_or(false)`;
decode failure remains a typed propagated storage/unavailable error instead of an unwind.

The diff does not change query text, `FOR UPDATE` placement, fleet/current-authority ordering,
account fences or module dependencies. Managed-cloud error mapping remains explicit. No Boolean
migration, SQL cast or fallback was introduced. The second lease consumer is the irreversible
submission start path, not heartbeat or finish, so this correction does not change terminal replay
or worker/session binding.

The new configured-PostgreSQL regression prepares the actual migrated entitlement column and
requires PostgreSQL INT4 metadata. It then checks0 as false and1,2 and-1 as true. Boolean, INT8 and
a missing named column each must return `Err` inside `catch_unwind`, proving both fail-closed typing
and absence of `Row::get` panic. It requires the configured URL when mandatory and serializes on
the existing PostgreSQL test lock. Scoped formatting/diff checks are parent-reported only; the test
and corrected ordinary queue/claim lifecycle have not yet run. The P1 therefore remains open for
runtime qualification, not for another source-shape correction.

Parent-reported focused session37153 subsequently exited0 with nine Rust tests passed and zero
failed, ignored or skipped: one actual-PostgreSQL entitlement decoder case, one actual-PostgreSQL
fleet-cutover lifecycle case, two fixture tests, four guarded-example tests and one SQLite parity
case. The decoder completed in0.95s, cutover in0.42s and SQLite in2.10s after a2m32s test build.
The772-file manifest
`8758dc34013ed91b323dc72c87bc8b4ba0370b47fe14d395455257455fa401d3`, HEAD`40949858` and scoped
status matched before/after; PostgreSQL/root cleanup completed, the parent was empty and free space
was38GiB. This qualified the shared decoder and focused PostgreSQL consumers. At that checkpoint,
FIX821 still required the ordinary queue and real claim to traverse the corrected production path.

Session12156 is retained only as interrupted-run history: a documentation EOF-whitespace check
exited2 and the parent accidentally continued, then terminated the owned launcher. The aggregate
ended143, PostgreSQL/root cleanup completed and no tests from that run are counted. Parent reports
the document correction and prelaunch assertions subsequently passed (`414715`/`072e24`). This is
mechanical setup evidence, not product qualification.

### Actual causal-red result and resolver scope

Parent-reported session79181 exited0 and reached the intended boundary. The real ordinary profile,
source, Track, factual prepare, approval, cloud queue and exact replays passed. The read-only
post-queue helper validated the actual run, session, command, request, workflow and release memo.
The real signed `ExecutionLeaseClient` issued exactly one claim and received the exact404
`Execution lease target not found.` response whose hash was checked by the strict report. The run
ID and browser-session ID were distinct, no run-ID session alias existed, all fourteen protected
database families were unchanged, and a third queue replay rechecked production all-role
readiness immediately before claim. Exactly one owned loopback gateway request returned503 and no
retry occurred during at least11seconds; Temporal/effect acceptance remained zero.

The strict report also required all three child processes to exit code0 with absent PIDs and exact
ports refusing connections. PostgreSQL exited, the root/parent were removed/empty, free space was
38GiB, and the772-file manifest
`8758dc34013ed91b323dc72c87bc8b4ba0370b47fe14d395455257455fa401d3`, HEAD`40949858` and scoped
status matched. The parent reports API binary prefix`9e8a9beb...` and fixture prefix`08a8afa5...`.
This closes the D0.1 causal-red reproduction and FIX821 ordinary-path prerequisite. It does not
accept a production correction or qualify a positive lease lifecycle.

The production correction should be bound to the outer
`ManagedExecutionLeaseClaimContext`, not imposed unconditionally on every low-level helper. The
worker HTTP endpoint always calls `claim_managed_execution_lease_for_runner_volume_authorized` and
therefore supplies that outer context, even when its optional managed-release input is absent.
Debug/direct test helpers pass no context and commonly use their historical `session.id = run_id`
fixture shape. Within the real-worker context, however, exact durable START resolution must be
mandatory: select one command by account/application/run and exact request/binding when provided,
decode its START material, and pass its `browser_session_id` explicitly to execution-target and ATS
Phase A validation. Do not query `run_id OR browser_session_id`, choose a current session, or accept
a caller alias.

The same context-bound session belongs in the first irreversible transition. The existing
`click_started` replay branch must continue to return from frozen stored receipt/ATS/managed-cloud
authority before any current START lookup, so retained terminal replay survives command/head
advance. Heartbeat and finish remain token/fence operations and should not acquire this new lookup.
Context-free debug helpers retain their legacy contract. Any terminal/reconciliation successor
must use frozen stored session authority rather than re-requiring a current active command.

### D0.2 resolver lock-order review

- **Resolved P1 design constraint — do not lock the START command before a managed RESUME request.**
  Existing PostgreSQL managed authority validation locks the exact request command and, for a
  RESUME, then locks the corresponding START command/binding. A new resolver that first takes the
  START row `FOR SHARE` and later invokes that authority path can deadlock with request-start:
  claim holds START and waits for RESUME while request-start holds RESUME for update and waits for
  START. Preserve the existing request-to-START order. After managed-admission/ATS/fleet,
  discovery/account fence and cleanup prelocks, validate and lock the exact request through the
  existing authority helper, then decode/recheck the unique START material under the established
  START lock. An unlocked preliminary read is not authority; if retained for discovery it requires
  byte-exact post-lock recheck. Delaying execution-target validation until after the established
  request/START authority is simpler.

  For outer worker context with optional input absent, there is no RESUME request lock: taking the
  unique START row `FOR SHARE` after the account fence and cleanup check follows the existing
  higher-to-child lock order. SQLite's immediate transaction serializes writers, but should enforce
  the same exact-one and frozen-field semantics. The eligible START states remain `delivering`,
  `delivery_unknown` or `accepted`, with both `first_request_started_at_ms` and its matching
  `request_started` event. Use a bounded two-row query and reject zero or duplicate matches.

  Active consumers are claim in both databases, SQLite managed-effect authorization, and the first
  irreversible transition in both databases when the real-worker outer context is present. The
  PostgreSQL managed-effect path already performs its own exact effect resolution and has no target
  lookup. Heartbeat/finish stay unchanged. The existing `click_started` retained-replay branches
  must remain before the resolver, and ATS Phase A must receive the resolved session ID rather than
  `run_id`. The PostgreSQL helper now invokes the existing exact managed request authority with row
  locking before querying the unique START row `FOR SHARE`, preserving request-to-START order.
  With optional managed input absent it takes only the START child lock after account/cleanup
  fences. The design finding is resolved statically; concurrency/runtime proof remains pending.

### D0.2 integration review in progress

The visible integration correctly makes the session ID explicit in execution-target validation,
stores it in the resolved target, propagates it into SQLite and PostgreSQL ATS Phase A, invokes the
resolver whenever the outer real-worker context exists even when its optional managed input is
absent, and leaves context-free debug helpers on `run_id`. SQLite managed-effect authorization and
both first irreversible-transition branches use the new active resolver. Both SQLite and
PostgreSQL retained `click_started` replay branches still return before active lookup; heartbeat and
finish are unchanged.

- **Resolved P1 implementation finding — two PostgreSQL active child locks still used `run_id`.** After
  the corrected execution-target lookup, claim still locks `jobs_browser_sessions` with
  `id = run_id` at the current `execution_leases.rs:2001-2006`; first irreversible transition does
  the same at`:4078-4083`. On an ordinary cloud queue these rows do not exist, so either path will
  reproduce the causal red despite the resolver. Each lock must use the already validated
  `execution_target.browser_session_id`. The parent corrected both binds to that exact validated
  field without an `OR`, alias or second lookup. SQLite has no analogous later child-row lock.

The new resolver helper is now present and statically clear. Both database implementations repeat
the active-account fence and cleanup-generation denial, select at most two START rows, reject zero
or duplicates, require protocol v2, active `delivering`/`delivery_unknown`/`accepted` state,
`first_request_started_at_ms` and an actual request-start event. START material must bind the exact
account, application, run, cloud runner, session, identity, derived profile and packet fields, with
its result request equal to the durable command request. Managed START or RESUME input is validated
through the existing exact workflow binding/release authority; the current request must chain to
the same START workflow/session/identity/profile. PostgreSQL locks request authority before the
START row, avoiding the reviewed inversion. SQLite uses its immediate transaction with identical
cardinality and field semantics.

All five active consumers are wired: claim in both databases, SQLite managed-effect authorization,
and first irreversible transition in both databases. Outer worker context with input absent still
requires START authority; context-free debug helpers retain `run_id`. The two later PostgreSQL
session `FOR UPDATE` locks now use `execution_target.browser_session_id`. ATS Phase A uses the same
resolved ID, while retained `click_started` replay returns before resolution and heartbeat/finish
remain untouched. Pure helper tests cover active states, inactive/unstarted rejection, frozen-field
drift and exact-one cardinality. Compilation, actual SQL, concurrency and lifecycle execution are
not yet evidenced.

### Prepared-lifecycle bridge review

The frozen adapter and protocol-only mode are statically coherent in their positive path. The
private bundle uses the production `ExecutionLeaseClient`, production worker-auth signing and
runner-volume claim proof. Its fetch adapter permits only claim plus, in prepared mode, heartbeat
and finish for the exact queued run; finish is restricted to `released`. The driver reads back the
lease token hash/fence, exact retained volume binding and ATS binding to the distinct durable
browser session, observes a real heartbeat extension with the same token/fence, releases without a
local final-submit attempt, then sends a separately signed finish request with a fresh nonce rather
than calling the cached `ActiveExecutionLease.finish()` promise again. The strict report modes are
mutually exclusive and continue to carry `sourceAccepted:false`.

The two evidence defects found in the first prepared-mode snapshot are resolved in the current
bytes:

- **Resolved P2 — the no-effect report could pass unobserved protected-family mutations.** The
  prepared report now starts `externalEffects` as null and derives it from the observed submitted,
  evidence and `click_started` counts. Across claim through release it compares exact hashes for
  the nine protected families that must remain immutable: application, browser session, metering,
  balance ledger, resume, evidence, workflow command, managed binding and request-start authority.
  The exclusions are the expected lease/ATS/volume/residency creation and the attempt reservation,
  whose timestamp may refresh. A separate attempt projection preserves every other current column,
  including status, reservation time and canonical employer binding. The strict report requires
  the exact immutable-family list, both unchanged results and the zero-valued effect object; report
  tests reject omitted families, fabricated effects and billing/attempt mutation flags.
- **Resolved P2 — the SQLite exact-scope regression was confounded by a missing request-start
  event.** The regression now has a dedicated unstarted-command rejection. A separate started
  fixture first resolves its valid distinct session, then independently rejects foreign account,
  application and run values. These paths no longer share the missing-event failure condition.

The parent also adapted the pre-existing managed final-submit fixture to create its START command
through the canonical builder with the actual frozen approved packet, identity and derived profile,
then retain the fixture's deliberate legacy `session_id == run_id` shape. Existing managed binding,
request-start and lease authority flow is unchanged. This keeps context-free compatibility tests
valid without weakening the real-worker resolver or introducing an alias in the ordinary runtime
fixture.

No runtime/build evidence exists for the helper bridge or prepared mode at this checkpoint. The
readiness/negative matrix, concurrency and retained recovery remain explicitly outside this
positive protocol slice. Parent-reported lightweight evidence is 54 Node report/cleanup tests
passing with zero skips, including the new family-omission and mutation rejection cases. This is
not independently rerun PostgreSQL, Rust, API or lifecycle evidence.

Parent-reported session23220 compiled for2m33s, passed eight pure tests and then failed all three
new SQLite resolver regressions before command admission. The canonical START builder correctly
rejected their inherited `cloud-run-{suffix}` IDs because those values were shorter than the
production opaque-identifier minimum. PostgreSQL and the exact root cleaned up, the parent was
empty, free space was38GiB, and the774-file manifest
`4520f7d...` matched before/after. This is a fixture failure and provides no resolver or prepared
lifecycle qualification.

The test-only correction is narrowly valid. `resolver_command` now prefixes only its use of the
existing execution-lease fixture with `session-authority-`, making the generated run ID satisfy the
same20-to-128-byte ASCII opaque-identifier predicate enforced by production, and asserts that
predicate before constructing START. Application, frozen packet, distinct session, request-start
and resolver inputs are otherwise unchanged. The next run remains required.

Parent-reported focused session1656 then compiled for2m32s, passed nine tests and failed two SQLite
cases when the dispatcher returned no command. The exact774-file source manifest
`21265467...` matched before/after; PostgreSQL/root cleanup completed, the parent was empty and free
space was38GiB. The cause was another fixture prerequisite: the dispatcher correctly selects only
commands admitted with managed-cloud authority, while the new regression admitted its START with
that requirement disabled. This is not resolver evidence.

The FIX824 test correction is statically clear. The regression now installs the existing signed
seven-role staging fixture after the positive source fixture, derives the exact source-fixture
authority clock from its immutable production head, advances the ordinary empty-fleet inventory
and two-phase cutover through existing test helpers, admits START with managed authority required,
binds the exact managed workflow authority in the same transaction, then claims the exact command
and records a real managed request-start result. The helper visibility change is confined to the
existing `cfg(test)` module, and all three environment-sensitive tests are serialized while holding
the staging environment guard. No production admission predicate, readiness gate or dispatcher
selection is bypassed. The client diagnostic addition records only fixed POST method, numeric/null
status and elapsed time around the actual fetch; it neither changes timeout/status behavior nor
exposes request material. A rerun remains required.

Parent-reported run84098 compiled for2m43s and all thirteen focused Rust tests passed, including
the new resolver tests and two existing managed cases. The ordinary private-PostgreSQL API claim
then returned HTTP500 after1,283ms. There was no panic fingerprint; eighteen PostgreSQL activities
were idle with no blockers. Cleanup was automatic, PostgreSQL/root exited cleanly, free space was
38GiB and the774-file manifest `3e1d860d...` matched before/after. The focused tests do not make the
ordinary PostgreSQL resolver green; its real API transaction failed before a lease was observed.

The managed-command locking-clause ordering hypothesis is now ruled out. Parent-reported isolated
probe86871 ran both `ORDER BY ... FOR SHARE LIMIT 2` and `ORDER BY ... LIMIT 2 FOR SHARE` against a
synthetic temporary table on private PostgreSQL17.10; both parsed and executed,2/2, and the
transaction was rolled back before clean PostgreSQL/root cleanup. No production SQL-order change is
justified. Parent-reported ordinary claim run50082341 still returned HTTP500; its bounded private-log
diagnostic reported an error with category `other` and no explicitly logged SQLSTATE. That result
does not identify a cause and remains neither resolver nor prepared-lifecycle evidence.

The FIX825 diagnostic is statically privacy-safe. On failure only, it reads the owned private
PostgreSQL log through the existing no-follow, owner/mode/link/inode-stability file helper. Its
public result is limited to an error-present Boolean, an explicitly logged five-character SQLSTATE
or null, one fixed category and a SHA-256 of the bounded private log. It never emits SQL, statement
text, identifiers, values, paths, connection details or native messages; unknown lines remain
`other`, and startup-only logs remain not detected. Parent reports57 Node tests passed, including
three new privacy/false-positive cases. This classifier is diagnostic-only and cannot satisfy a
qualification report.

The follow-up private-PostgreSQL verbosity instrumentation is also statically clear. It first proves
the exact data directory, loopback address/port, database and `bluey_jobs_test` identity, then changes
only that disposable cluster role to `log_error_verbosity = 'verbose'`. The helper's `sql` function
spawns a fresh pinned `psql` process for every invocation, so the subsequent `SHOW` is a meaningful
check that a new connection inherits the role setting before either API or fixture starts. The
cluster log remains below the mode-`0700` owned root and is read through the existing bounded,
mode-`0600`, no-follow/inode-stability path; only the closed category, an explicitly logged
five-character SQLSTATE or null, and a digest may leave the private diagnostic. The setting neither
changes production logging nor authorizes a source correction. A SQLSTATE from the next run may
narrow diagnosis, but it is not independently sufficient for a green gate.

### FIX826 PostgreSQL finalizer review

Parent-reported ordinary run45305 reproduced the real-client claim HTTP500 on private
PostgreSQL17.10 with verbose SQLSTATE `42804`, no API panic and no blocker. The exact774-file source
manifest `34dee63f...`, HEAD and scoped status matched before/after, and PostgreSQL/root cleaned up.
Source and schema then identify the concrete leaf: the PostgreSQL runner-volume finalizer inserted
Boolean `FALSE` into `jobs_runner_account_subjects.legacy_unresolved`, while migration024 defines
that column as `SMALLINT` constrained to0/1. The PostgreSQL row decoder reads `i16`, and the ordinary
subject writer already converts Boolean state to `i16`; this is an availability/type defect, not an
authority bypass.

Parent-reported prepare-only probe31410 copied the exact bounded subject-table DDL into private
temporary tables and prepared the uniquely extracted production INSERT. The original form failed
with exact `42804`; on a fresh connection, replacing only `FALSE` with `0::SMALLINT` prepared and
rolled back successfully. It created no permanent account or row, PostgreSQL/root cleaned up, and
manifest `bfa6c5db...` matched before/after. The one-token production correction at the finalizer is
statically correct: it matches the authoritative type and preserves every schema, lock, predicate,
transaction, conflict and false-state semantic. FIX826 accurately states that this causal probe is
not an actual finalizer or lifecycle pass. The pending configured-PostgreSQL regression must begin
without an account-subject row, execute the real prepare/finalize path, read back `i16=0`/decoded
false, prove exact residency/binding replay and retain rollback-on-rejection coverage. Ordinary
claim/heartbeat/release runtime and broader gates remain pending.

The new configured-PostgreSQL regression is statically fit for that bounded gate. It requires the
explicit PostgreSQL test flag when no URL is present, serializes through the existing boot-test
lock, migrates and replays migrations, and rejects a non-fresh fleet/cohort before composing state
through existing account, beta, signed posting, approval, lease, volume, attestation and cutover
helpers. It asserts that no subject exists before the finalizer; the preliminary legacy debug lease
is explicitly labeled and is not misrepresented as managed HTTP qualification. The test then calls
the production PostgreSQL prepare/finalize helpers, rejects a mismatched request without any of the
three protected row families, reads `legacy_unresolved` as exact `i16=0`, and rolls the successful
three-family mutation back to an empty snapshot. Finally it exercises the public production wrapper,
checks the joined subject/residency/lease/binding fields, decodes false through the public reader and
requires an exact replay with an unchanged canonical row snapshot. Its random proposed subject on
replay does not weaken identity: the existing account-subject authority is re-read and must win.
Static inspection finds no test bypass or false-green path. Parent-reported focused session89311
then passed14/14 cases with zero skips; the new actual-PostgreSQL case completed in4.34s after a
2m45s compile. Source manifest
`0f33d8b9ea6a71d01613c969ed7d5d1fc961ef0da956f9231287872f1569bdf3` matched before/after and
cleanup completed. This closes the bounded FIX826 regression gate, not ordinary API claim,
heartbeat, release or terminal-replay qualification.

### Resolved P1 source defect; execution pending: runner-volume authority request IDs

Parent-reported ordinary private-API session61106 built in1m23s and then returned HTTP400 from the
real client claim after2ms. There was no PostgreSQL error, panic or blocker; cleanup and PostgreSQL
shutdown completed. The response body and generated authority request ID were not retained, so
that particular run's exact400 branch is not yet proven.

Independent source inspection nevertheless finds a deterministic intermittent contract defect.
Both the production generator at `jobs/runner/src/runner-volume-client.ts:1594` and the
nonproduction fixture at `jobs/runner/nonprod/control-plane-client.mjs:60` generate the authority
`requestId` as an unprefixed
`randomBytes(24).toString("base64url")`. Rust's `require_runner_identifier` accepts the same
alphabet after the first byte but requires the first byte to be ASCII alphanumeric at
`server/src/db/jobs/runner_volume_purge.rs:1447`; `validate_unsigned` applies it at line1705.
Base64url can start with `-` or `_`, so2 of64 possible first characters (1/32 requests) are rejected
before database lookup and map to HTTP400
`Invalid runner-volume request.` This can affect every operation using the shared production
`createAuthorityProof`, not only claim, and is a real availability/reliability defect even if the
61106 response proves to be a different400 branch.

Do not relax the server's general identifier validator. Generate new proof request IDs through one
shared production helper with a fixed alphanumeric domain prefix while retaining all192 random
bits, and have the fixture call that same helper. A deterministic pure test should cover every
possible leading entropy byte and prove that all generated values satisfy the Rust grammar and
length bound; a zero-database Rust test should retain explicit rejection of raw leading `-`/`_`
and acceptance of the prefixed forms. Existing accepted proof IDs and signatures need no rewrite.
The next ordinary run must distinguish `invalid_volume_request` from the other HTTP400 constants
before attributing session61106 to this defect.

The new HTTP response diagnostic is suitable for that bounded distinction. It compares the
bounded response body only against exact public API constants and emits only numeric status, a
fixed category and the body's SHA-256; unknown or extended text remains `unclassified`, and no body
or prefix is reported. Parent reports58 Node tests passed with zero skips. This instrumentation is
not a product correction or runtime qualification.

The FIX828 implementation resolves the generator defect statically without weakening the server.
`createRunnerVolumeAuthorityRequestId` validates an exact24-byte `Uint8Array`, prefixes the full
32-character base64url encoding with `rva-`, and is used by both the production shared
`createAuthorityProof` path and the private fixture. The36-character result is within the Rust
bound, begins with an ASCII letter and retains every one of the192 random bits. Canonical signing
still covers the complete generated request ID; existing accepted IDs, replay consumption and
database uniqueness are unchanged.

The TypeScript regression exhausts all256 possible leading entropy bytes, observes the expected
eight raw vectors whose first character is `-`/`_`, round-trips the full entropy suffix and requires
every prefixed value to match the Rust grammar. It also rejects representative non-24-byte inputs.
The zero-database Rust regression independently applies the real validator to the same256 vectors,
retains rejection of each unsafe raw form and accepts every prefixed form. Static review finds no
source blocker in this correction. The new TypeScript/Rust regressions, typecheck and ordinary
private lifecycle rerun are not yet executable evidence; session61106 remains unattributed until a
closed response category is actually captured.

P3 documentation cleanup remains before closeout: the current FIX828 record still describes the
implementation/files/tests as pending/planned even though the patch is present. That historical
record should be updated after the specialist freezes the exact source and the new regressions run;
it must not relabel the pre-diagnostic61106 failure as proven evidence for this cause.

Parent-reported session69688 then passed the new256-vector Rust regression after a2m38s compile,
built the API in2.84s and obtained HTTP200 from the actual managed claim in1,291ms. The harness
failed only at `exact_prepared_bindings`: it expected one ATS Phase-A row. There was no PostgreSQL
error or panic, cleanup completed, PostgreSQL/root exited and source manifest
`9e928d8c3a34d16bd78d5740254d1d99869735ceccec98bd1c94e1e72f00e4e8` matched before/after. This
proves the random-ID correction got through the ordinary claim transport and database transaction,
but the run is not yet a prepared-lifecycle pass.

The failing expectation is a harness defect, not a production admission gap. The ordinary packet
is explicitly created and approved as `review_first`; production's SQLite and PostgreSQL
`application_requires_ats_phase_a` predicates create Phase A only for `auto_submit` with frozen ATS
certification. Forcing or seeding ATS authority would test the wrong policy and weaken the causal
chain. Persisted application readback must instead prove exact `review_first`, and the prepared
report validator must require that submission mode and zero ATS bindings.

The replacement should expose explicit observed counts rather than hiding them behind only
`exactBindings`: exactly one lease, exactly one matching volume binding, zero run/session alias and
zero ATS rows for the fresh account/application/run. The ATS zero query must omit phase, fence,
consumed, profile and session filters (and preferably assert account-, application- and run-scoped
totals independently), so an incorrectly shaped ATS row cannot evade the check. Add negative report
tests for altered submission mode, nonzero ATS count and each incorrect lease/volume/alias count.
This bounded review-first lifecycle must not be described as auto-submit ATS qualification; the
existing D0.2/D0.3 auto-submit ATS regressions and a corresponding runtime path remain separate
pending gates.

The corrected implementation reads `submission_mode` from persisted application JSON,
requires independent zero ATS totals for the account, application and run without narrowing
filters, and makes every count part of the strict report shape with negative tests. The P2 volume
false-green is resolved: `volumeBindingCount` is now the unfiltered total for the run, while
`exactVolumeBindingCount` independently requires the expected volume, epoch and process-instance
binding. Both must equal one, both are serialized into the strict report, and a mismatched sole row
has dedicated negative coverage. The review-first harness correction is statically clear for the
next private rerun. Parent reports65 Node tests passed with zero skips; those pure tests are not a
prepared-lifecycle runtime result.

Parent-reported private run65097 again obtained an actual managed HTTP200 claim, in1,256ms, then
failed because the harness attempted `application_json::jsonb` in `psql`. PostgreSQL correctly
reported `22P02 invalid_input`: this column contains encrypted application storage, not plaintext
JSON. There was no API panic, cleanup completed, PostgreSQL/root exited, and source manifest
`a7a3035d746dc2f7c473d494a8f97ff81a40b22ae018b7751293369d4abd08b3` matched after the run. This
is another harness-evidence failure, not a production data or crypto defect.

The proposed correction is statically sound. The existing strict post-queue Rust fixture already
opens a repeatable-read, read-only transaction, selects the exact account/application row and calls
the production `parse_application_json` path with the real job/application binding. Require that
decrypted application to have exact `review_first`, then add only that closed string to the strict
`RunnerControlPlanePostQueueResult`; do not export encrypted bytes, decrypted application content
or key material. The Node driver must accept the new exact field, use it as the claim report's mode
and remove the invalid SQL cast.

To bind the pre-claim mode proof to the post-claim result, immediately after HTTP200 require the
full `PREPARED_IMMUTABLE_TABLES` snapshot to equal `immutableBefore` before evaluating the allowed
lease/volume counts. Because this snapshot includes `jobs_applications`, it proves the encrypted
application row did not change across claim; the Rust fixture proves its pre-claim decrypted mode.
Retain the existing final immutable snapshot as a second lifecycle-wide check. This change neither
mutates PostgreSQL nor alters encryption/serialization. FIX830 should record65097 as a failed
harness assumption and must not call it a product failure or prepared-lifecycle pass.

The landed FIX830 implementation matches that design. `submission_mode` is added to the strict
post-queue result only after production parsing has bound the exact account/application/job row and
an `anyhow::ensure!` has required exact `review_first`, queued state, run ID and no submission. The
serialized value is therefore a closed observed mode, not arbitrary decrypted content. Node's
exact-shape check requires the field and its exact value, the invalid JSONB cast is gone, and the
claim report uses the fixture observation rather than request echo. Immediately after HTTP200 the
driver compares all nine immutable families, including encrypted `jobs_applications`, with the
pre-claim snapshot before evaluating allowed lease/volume mutations. The final lifecycle snapshot
remains. Static review finds no crypto, root-guard, database-write or privacy regression. Parent
reports65 Node tests with zero skips and fixture rustfmt check passing; actual corrected private
lifecycle execution remains pending.

### Parent-reported prepared-lifecycle execution

Session80446 completed with exit0 on the reviewed source. The private API and fixture built in
1m32s; the actual managed claim returned HTTP200 and the strict report emitted
`runner_control_plane_prepared_lifecycle` while retaining `sourceAccepted:false`. The report proved
persisted `review_first`, one exact lease, one total and exact volume binding, zero ATS bindings at
account/application/run scope, no run/session alias, an observed heartbeat with unchanged token and
fence, safe HTTP204 release, and a fresh signed HTTP204 terminal replay that left all14 protected
families unchanged. Across the prepared lifecycle all nine immutable families and the attempt
binding remained unchanged, with zero submitted/evidence/irreversible effects.

The owned gateway received exactly one request and returned503; no second request appeared during
the at-least11-second observation window. This is only local request-start/delivery-unknown
behavior, not Temporal or provider execution. All three child processes exited0 and were absent,
their loopback listeners refused connections, PostgreSQL shut down with exit0, the exact root and
parent were empty, and free space was38GiB. The774-file manifest
`6b6fccb221db48534ee54dea6632666f9d4a83732805481f8b21c4e61ea70c6b`, HEAD and scoped status
matched before/after. The admitted API and fixture hashes were respectively
`a4126a85eee633faae25e0219f0a429422833e86593f225fbef48c6e34bfa867` and
`83e43de57ad232ab4e1892d8c21adf323e80c4e36534f810db6c212ec67d90b8`.

This closes the bounded ordinary review-first claim/heartbeat/release/replay gate. It does not prove
auto-submit ATS Phase A, native managed-cloud execution, browser/Temporal/provider behavior,
submission effects, production readiness or source acceptance. The stale/cross-binding/reuse,
command terminalization and policy-rotation matrix, retained checkpoint recovery seam and full
regression/privacy/self-test gates remain open.

### Auto-submit prepared-repeat causal-red static review

I reviewed the frozen auto-submit additions line by line in the guarded Rust fixture/example and
the Node reproduction child, validator tests and inner launcher. I found no new P0-P3 blocker to
starting the bounded private causal-red run. This is static clearance only; the parent-reported102
Node report tests are not PostgreSQL/API evidence and were not independently rerun by this reviewer.

The fixture keeps review-first and auto-submit file families and operation names disjoint. Auto
bootstrap still requires a fresh synthetic account, verified identity/source and ordinary Track;
the driver authorizes Auto-submit through the ordinary Track endpoint, prepares the factual packet
as `auto_submit`, requires schema-v3 `track_auto_submit` admission and never calls the manual
approval endpoint. The historical positive-source helper's synthetic production-scope verifier
rows remain private fixture composition; the runner/release admission selected by the API remains
fixed staging/us-east-1/general and is not native or production runtime evidence.

The post-queue and first-claim inspector use database-enforced repeatable-read/read-only
transactions. They bind the exact durable command, request-start, distinct run/session/profile,
application, frozen packet/authorization, prepared lease token hash/fence, one Phase-A row,
trusted process runtime and runner volume. The inspector decodes the production frozen ATS
authority and checks the stored projection field by field, requires preflight/fence0, a future
expiry and absent Phase-B/consumption/invalidation fields, and emits only closed synthetic IDs,
digests, counts and booleans. It neither creates nor refreshes authority and does not export the
lease token, frozen certification bytes, candidate content or signing seed.

The reproduction exercises two actual `ExecutionLeaseClient.claim` calls. After the first HTTP200
claim and exact ATS inspection it observes and stops the normal heartbeat, takes the full protected
snapshot, and repeats the ordinary queue endpoint to re-evaluate current fleet/release readiness
without changing the durable command. The second claim retains every semantic claim field while
using a fresh runner-volume request ID/proof and fresh worker-auth nonce. Qualification requires
the production client classification `lease_unavailable`, HTTP409 and the exact public response
hash, plus byte-stable snapshots of all twenty named claim/ATS/runtime/volume/effect families and
the full ATS row. The intentional worker-auth nonce reservation is the only named exclusion.

After the expected denial, the driver sends a separately signed fresh-nonce heartbeat using the
original token/fence and requires the original prepared lease expiry to advance. It then uses that
same retained lease for safe `released` finish and a separately signed terminal replay, with the
released snapshot unchanged. Candidate/billing/workflow state, attempt bindings, submitted/evidence
state, submission capacity, canary reservations and irreversible receipts must remain zero or
unchanged. The strict report requires four clean, absent child processes (API plus bootstrap,
post-queue and inspector), closed loopback listeners, one503 gateway request with no retry for at
least11 seconds, `sourceAccepted:false` and `temporalAccepted:false`.

The twenty-family snapshot deliberately does not qualify policy rotation: current Auto-submit
authorization and shared ATS heads are revalidated by the ordinary queue/claim paths but are not
claimed as an exercised rotation negative. That matrix, session reuse/terminalization, checkpoint
recovery and all full source gates remain open. Before changing production behavior, the private
run must still prove the first claim/ATS row, the exact second-claim409, unchanged protected state,
retained heartbeat/release/replay, clean process/listener/PostgreSQL/root teardown, and unchanged
admitted source manifest. A different status, setup denial or in-memory rejection is not the
required causal red and must not be accepted or repaired by weakening ATS equality.

### Auto-submit prepared-repeat green harness and source-only fix review

The frozen Node green mode is statically clear. It preserves the review-first and causal-red
scenarios, routes `autoSubmit` through the same ordinary profile/source/Track/authorization/packet/
queue path, and requires a second real claim to return HTTP200 with a fresh volume proof and
worker-auth nonce, a different live token and a strictly higher fence. It compares the complete
stored ATS row with the production-decoded first-claim inspection, keeps all nine immutable
families, the attempt binding and account subjects unchanged, and requires one lease, one exact
volume binding and two runtime-history rows with exactly one row for the new fence/runtime. Fresh
signed stale-token heartbeat and safe-finish requests must each return409 while all twenty named
families remain unchanged. The new active token/fence must heartbeat successfully, then release
with HTTP204 and accept a fresh signed terminal replay without changing the released snapshot.
The strict report exposes none of the tokens, frozen rows, private response bodies or key material.
It requires four clean child exits and retains `sourceAccepted:false` and
`temporalAccepted:false`. Parent reports133 Node validator tests passed; I did not rerun them.

The frozen production Rust fix is also statically clear at the source-only level. Hashes reviewed:
`execution_leases.rs` `feb116da1c6ec580fa5c73877113b0cde1d565afb553991b2bff0ed5d30558dd`
and `ats_certification_authority.rs`
`9c352654bd9d90658fc1f8e081f8d068d944725d1c0a20daecbac58551829b2d`.
`PreparedManagedCloudAtsRetry` is private evidence derived from the already read/locked lease; it is
available only for a managed-context claim whose exact account/application/profile lease is
prepared, positive-fence, unexpired and owned by the authenticated and volume worker. A missing
lease or generic context stays on the original strict creator, while an incompatible existing
lease fails closed. SQLite retains `IMMEDIATE` serialization. PostgreSQL retains the ATS prelock,
then locks the lease, resolves final database time and current execution authority, and performs
the retained replay before the lease CAS rotates token/fence.

The retained helper requires exactly one preflight, fence-zero, unconsumed and unexpired ATS row
for the exact account/application/run/session/profile and cloud runtime. It restores only the
stored nonce and requested expiry, then re-reads current application/posting/authorization/
attempt/certification state, compares the frozen admission/runtime projection and passes the
reconstructed request through the ordinary exact raw-replay matcher. Consequently it cannot
create a missing ATS row, extend its TTL, accept revoked Auto-submit authority, or tolerate packet,
target, runtime or scope drift. Generic creation, Phase B and retained checkpoint recovery are
unchanged. The four named Rust regressions cover same-owner/live eligibility, incompatible stored
state/runtime, generic nonce/deadline strictness, missing-row denial, revoked authorization,
packet drift and exact immutable replay; compilation and backend execution were still pending at
this snapshot.

This source-only checkpoint was not source acceptance. At that point actual SQLite and configured-
PostgreSQL execution and the ordinary green API run were still pending; the subsequent bounded
result is recorded below. Preservation of the causal-red and review-first runs, the wider negative/
policy/terminalization matrix, the open checkpoint-recovery seam and full gates remain required.
No Phase B, native cloud, browser, Temporal, provider or production behavior was qualified by the
static review.

### Parent-reported auto-submit prepared-repeat execution

Parent reports session55819 completed with terminal exit0 on the frozen source. All four exact Rust
tests passed individually with zero ignored: the test build took2m59s and the fixture cases took
2.18s,2.13s and1.91s. The private API build took2.78s. The ordinary auto-submit path emitted the
strict `auto_prepared_repeat` success event with HTTP200, `sourceAccepted:false`, exactly one owned
gateway POST and `cleanup:true`. The wrapper separately reported269 source files unchanged, and
the private PostgreSQL wrapper reported attempted shutdown and clean server exit.

The strict report validated the first prepared lease and frozen Phase-A authority; a second real
claim with a new token and higher fence; exact ATS-row preservation; unchanged immutable,
attempt-binding and account-subject state; one live lease, one exact volume binding and two runtime
history generations; signed stale-token heartbeat and safe-finish HTTP409 responses without a
change to the twenty protected families; a successful current-token heartbeat; safe HTTP204
release; fresh signed terminal replay; four clean child exits; and zero submission/evidence/
capacity/canary/irreversible effects. This is a prepared-only result and does not exercise an
ordinary live irreversible Phase-B transition.

Parent reports the774-file source manifest
`a970b9c46405079358550f3363d8568f9e3fc0df62b67a2198efb0840b8801fe`, HEAD and scoped-status
digest matched before/after. The admitted API and fixture SHA-256 values were respectively
`06ce3f5b2f8fe7b5617d3f7bfc2734572175acb044872ae12067bc05058df040` and
`0d2ebc6f8b0dc9cb1ae9f07c77dd752e853d445d0df863f65c2933363416e7e4`.
The exact root was absent, its parent empty, free space was38GiB and the real index was empty after
the run. Parent also reports the prospective privacy scan passed over2,923 tracked paths and2,647
text files with scratch tree `1c90d3ea9a1281243f13bbfc1240d8d880a293b2`. I did not independently
rerun or load this runtime log; these counts are parent-reported evidence.

This closes the bounded prepared auto-submit repeat gate only. The historical causal red was not
rerun on these exact bytes, the review-first lifecycle still needs a current-source preservation
run, and ordinary live Phase B remains unqualified even though a private SQLite context test passed
after the ATS retry. Session-reuse/terminalization/policy-rotation negatives, retained checkpoint
recovery, focused/full regressions, final privacy/self-test and source acceptance remain open. No
native cloud, browser, Temporal, employer/provider, production or release claim follows.

### Unit-economics benchmark review

The dated `jobs/UNIT_ECONOMICS.md` update is arithmetically and evidentially bounded. Parent reports
the public Tsenta page advertised `$19/600`, `$39/1,500` and `$99/4,500` on2026-10-28; those compute
to `$0.031667`, `$0.026` and `$0.022` per included application at full utilization. The note clearly
labels these as customer prices rather than competitor costs or verified submissions, distinguishes
unused allowance and comparable terms, and correctly observes that a `$0.05` target would not beat
those full-use prices. It also labels Bluey's `$0.125` cloud-packet budget as historical and
unmeasured, separates packets from receipt-verified employer submissions, includes failures/retries,
idle capacity, fixed/shared cost and quality/latency in the required measurement plan, and makes no
price, flag, competitive-advantage or release claim. This is a planning benchmark only, not Phase638
runtime evidence or an economics acceptance gate.

- **Open P1 qualification seam — retained checkpoint reconciliation still assumes
  `session.id = run_id`.** The separate checkpoint-recovery path selects browser sessions by run ID
  in both databases and `checkpoint_reconciliation_plan` repeats that equality. Ordinary cloud
  sessions use the distinct frozen START session ID, so released/side-effect-unknown checkpoint
  recovery remains unqualified even if active claim, heartbeat and release become green. Do not
  attach this recovery to current START/head authority: its eventual correction must derive the
  session from frozen lease/command/receipt authority and preserve historical terminal replay.
  This is an availability/recovery hold, not evidence of an unsafe effect, and is intentionally
  separate from the bounded active D0.2 correction.

  Further inspection supersedes my earlier ATS-only recommendation. Current managed leases already
  persist an all-null-or-complete twelve-field authority, including the immutable historical START
  command/binding, request command/request ID, runtime/worker projection and a canonical authority
  digest bound to the exact run, fence and token. PostgreSQL migration033 gives those fields
  command, binding and runtime foreign keys; SQLite insert/update guards require the same complete
  shape and exact command/binding/runtime rows. Command hard deletion is account-deletion-only and
  removes the dependent lease, so a surviving complete managed lease has the historical material
  needed to resolve its frozen START session without a new column or migration.

  The correction must classify those fields before compatibility handling: partial authority is
  invalid; complete authority must resolve the exact stored execution command and request chain,
  verify the canonical binding and lease digest, and cross-check frozen application/profile/session
  identities. The existing `require_unmanaged_cloud_execution_*_tx` classifier already recognizes
  command, binding and request-start managed history without live-head/time checks; reuse it for the
  all-null legacy branch instead of inventing a binding-only test. If it finds managed history, an
  all-null lease must fail closed rather than guess `run_id`. Do not invoke the outer live effect
  resolver, consult the current command/head or use `run_id OR session_id`.

  The frozen session ID is inside the encrypted historical START payload, not in the twelve lease
  columns. Normal command parsing already has the Jobs data key, so this does not require a schema
  migration or new authority. It does make old-key retention/rotation a required recovery
  regression: losing the ability to decrypt a surviving START must fail closed, not fall back to
  the run ID.

  PostgreSQL lock ordering remains a design gate: existing managed effect code documents admission
  locks, account, command/binding, then exact lease. Recovery must not lock historical command rows
  after already locking the lease if a concurrent path takes the documented order. Immutable
  discovery plus ordered lock/recheck is preferable. Mutating recovery must additionally require
  `application.run_id` to match and select/update only the resolved old session ID, never a newer
  run's session.

  There is also a concrete backend-parity defect in the current ordering: SQLite returns a trusted
  submitted checkpoint before selecting a browser session, while PostgreSQL selects the session by
  run ID first. Thus an ordinary distinct-session submitted reconciliation can fail only on
  PostgreSQL before reaching the trusted-submitted replay. Move that PostgreSQL early return ahead
  of session resolution. For non-submitted reconciliation, update
  `exact_checkpoint_recovery_replay`, `checkpoint_reconciliation_plan`, SQLite's session
  select/update and PostgreSQL's session select/update together. The PostgreSQL submitted return
  must remain after exact lease owner/token/fence, authoritative database-time volume, application
  and evidence checks; it needs no historical command/session dependency because it mutates
  nothing. Non-submitted recovery still needs the historical resolver and ordered recheck above.

### Parent-reported checkpoint-session causal red

Parent reports session29416 completed the intended causal-red harness with terminal exit0. The
ordinary review-first path claimed a real prepared lease and observed its heartbeat, then sent an
exact checkpoint-v2 `prepared` reconciliation using the original owner/token/fence. Production
returned the exact public HTTP404 target-not-found response while all twenty protected families
remained unchanged. A separately signed retained heartbeat still returned200, safe release204 and
fresh terminal replay204; ATS/effect/capacity/canary/receipt counts remained zero. The report also
required three clean child exits, one owned gateway POST, refused listeners and
`sourceAccepted:false`.

Parent reports PostgreSQL and the owned root cleaned up, free space was38GiB, and the774-file
manifest `79f252d90da5b58ea027f379d4727941ad2284f0b6ea7e7e54778f99fd042463`, HEAD and scoped status
matched before/after. I did not independently rerun or load this runtime log. This establishes the
checkpoint/session availability red on that admitted source; it does not qualify a correction.

### Retained checkpoint correction and green harness static review

The frozen FIX832 Rust correction is statically clear at the source-only level. The historical
resolver classifies the stored lease authority before compatibility handling: partial managed
metadata is rejected; all-null metadata is accepted only through the existing unmanaged-cloud
classifier; and complete twelve-field metadata must match the immutable historical START command,
request chain, managed binding, request-start event and canonical lease authority for the exact
run/token/fence. It does not consult a current command head or time-dependent admission state.
The plain MVCC history reads are bounded to immutable rows pinned by the surviving lease, while the
mutating path still rechecks database time, the exact lease/volume/application authority and the
resolved historical session before capacity or state changes. PostgreSQL now reaches the trusted
submitted replay after the exact lease, database-time volume, application and evidence checks but
before historical command/session resolution; this preserves the read-only terminal replay without
making new recovery depend on live command material. The SQLite and PostgreSQL compatibility paths,
strict partial-metadata denial and complete historical authority are covered by the frozen focused
regressions, but those tests had not executed at this review checkpoint.

The frozen Node green harness is also statically clear after FIX833. The earlier P2 false-green is
closed: `assertCheckpointPreparedRecoveryReport` requires an independently supplied expected mode
from the closed set `review_first`/`auto_submit` and compares it with the report before selecting any
mode-specific checks. The driver derives that argument from its exact allowlisted scenario; the
outer controller independently derives it from its own explicit command-line mode. Cross-mode in
both directions, missing and unknown expected modes are negative-tested, so an untrusted report can
no longer choose the weaker review-first validator for an Auto-submit invocation.

The review-first and Auto-submit green variants respectively require six and seven clean child
records. Both require one real checkpoint-v2 prepared reconciliation and one separately signed exact
replay to return200, canonical post-recovery inspection before/after/replay, seven immutable-family
and sixteen stable-family equality, stable lease authority, unchanged attempt binding and a released
terminal finish plus a fresh signed finish replay with unchanged state. The inspector exposes only
closed lifecycle values and SHA-256 summaries; the driver compares the stable application/session/
recovery hashes, full stable-family snapshots and the lease JSON excluding only its expected
lifecycle timestamps/phase. Auto-submit additionally preserves the production-decoded frozen ATS
row and requires zero capacity, canary and irreversible authority. Both retain zero submission,
evidence and click-started effects, one gateway delivery, refused listeners, exact child exits and
`sourceAccepted:false`/`temporalAccepted:false`.

Parent reports186 Node validator tests passed with zero skips after FIX833. I reviewed the frozen
call graph and validator/test source but did not run those tests or a PostgreSQL/API lifecycle. This
is static clearance to execute the bounded green gate, not evidence that recovery succeeds on either
backend. The actual review-first and Auto-submit checkpoint-green runs, the focused Rust/SQLite and
configured-PostgreSQL regressions, preservation cases, wider negative matrix and full source gates
remain required. No Phase B, native cloud, browser, Temporal, provider, production or release
qualification follows.

### FIX834 test-fixture correction

Parent reports the first correction gate, session6759, stopped with terminal exit101 after the
first13 pure-test filter produced9 passes and4 failures. All four failures occurred in the shared
historical fixture's first material validation; the subsequent five database, three fixture, two
example and API gates were not reached. PostgreSQL and the exact root cleaned up, the parent was
empty, free space was38GiB, and the774-file manifest
`f7226ec3ea1ef4c7d2c5778449e465f26fa5b3e33d508623943daa4193d07027`, HEAD and scoped status
matched before/after. This is failed fixture setup, not recovery evidence.

The frozen test-only FIX834 correction is statically clear. `historical_fixture` now assigns the
canonical request ID to the START command, envelope and `result_request_id` before its first
historical-material validation. The RESUME branch assigns its new request ID to the command and
envelope and constructs its material with that same ID. A post-branch material validation now
exercises both START and RESUME forms after the packet/admission transport has been frozen. The
later redundant START assignment was removed. The binding, request-start, lease and canonical
authority builders continue to consume those exact command objects; no production validator,
fallback, authority field or expected denial changed. I found no second request/material setup
mismatch in this shared fixture. The reviewed file SHA-256 is
`b6852a50a42e728da11752786be221a7aa0b2d4b8d4725bb0233be150829a423`.

This clears only the corrected fixture for rerun. It does not turn the9/13 partial result into a
pass or qualify the FIX832 recovery behavior; the full13 pure cases and all downstream backend,
fixture, example and dual-mode API gates remain unverified on these bytes.

### FIX835 PostgreSQL historical-marker correction

Parent reports retry77890 passed all23 focused cases—13 pure, five SQLite/database, three fixture
and two example tests, with zero ignored—then the real private-PostgreSQL recovery returned HTTP500
after18ms. The private diagnostic classified SQLSTATE42883/operator type, with no API panic or
sampled blocker. Source, HEAD and scoped status matched before/after, and PostgreSQL/root cleanup
completed. This qualifies the focused cases on that admitted pre-FIX835 source, not the ordinary
recovery path.

The source/schema diagnosis and one-line production correction are statically clear. PostgreSQL
migration033 defines `managed_cloud_authority_required` as `BOOLEAN NOT NULL DEFAULT FALSE`, while
the historical SELECT had copied SQLite's integer expression. PostgreSQL now projects
`managed_cloud_authority_required IS TRUE`, which has type BOOLEAN and feeds the existing
`try_get::<_, bool>` call. SQLite continues to use `managed_cloud_authority_required=1` for its
integer-backed column. No WHERE predicate, join, lock, historical event check, authority digest or
fallback changed.

Parent reports private prepare-only probe17989 reproduced SQLSTATE42883 for the exact original
source-derived query and prepared the otherwise identical corrected query. Its true/false/null
cases returned true/false/false, all work was transaction-local and rolled back, and PostgreSQL/root
cleanup completed. This is direct operator/type evidence, not full resolver execution. The added
Node regression reads the authoritative migration and function bodies, requires the PostgreSQL
BOOLEAN declaration and `IS TRUE`, forbids PostgreSQL `=1`, and preserves the SQLite `=1`
expression. Parent reports187 Node tests passed with zero skips; I did not rerun them.

No P0-P3 issue remains in this bounded FIX835 delta. It was statically clear for the build-only
ordinary private-PostgreSQL review-first and Auto-submit recovery reruns. The earlier500 is not
relabeled green from the parser probe or focused23-case pass; the separate successful runtime
results are recorded below.

### Parent-reported dual-mode checkpoint recovery execution

Parent reports both closed FIX835 checkpoint-recovery scenarios completed with terminal exit0 on
the same admitted source. Review-first session86077 built in1m27s; Auto-submit session20772 built in
1m24s. Each strict report was bound to its explicit invocation mode and validated a real prepared
claim and observed heartbeat; three typed read-only inspections before, after and after replay; one
real checkpoint-v2 recovery HTTP200; one separately signed exact recovery replay HTTP200; and an
unchanged exact twenty-family snapshot around the replay. Both required sixteen stable-family and
seven immutable-family equality, stable lease authority and attempt binding, frozen application and
session hashes, terminal finish HTTP204, a fresh signed finish replay HTTP204 and zero submitted,
evidence, click-started, capacity, canary or irreversible effects.

The review-first report required six clean child records and zero ATS authority. The Auto-submit
report required seven clean child records, ordinary consent/Phase-A authority and byte-identical
preservation of the complete ATS row. Parent reports the API, PostgreSQL and exact roots cleaned up
for both runs, both parents were empty, free space was38GiB and the real index remained empty. The
774-file source manifest
`d66a712f17e2cbced73fc1b2006836e771cc8abe7c5e5c2db2760861d932cb5d`, HEAD and scoped-status
digest matched before/after both runs. The review-first API/fixture SHA-256 values were
`2cd9dc12646c878dca93119a26afb42e555638d44081b436f2919bb63a7b8cb8` and
`f33428b266eb5f4756ade7b63c81f6c63dbdb31e42049bffeb0e770739c617b0`; the Auto-submit values
were `80b6c0ce238cac8f25894c22cc01cfa57a18ac5ddd23d780cdc0a7ead28655d3` and
`b8ecbcba8c870aad3d6c52cfde5f97e3439a7bb889167e9b4b09897ea0518340`.

I did not independently rerun or load these runtime logs. Parent also reports187 Node validator
tests passed. The earlier23 focused Rust cases passed before the one-expression FIX835 source
change, so they are not represented as a current-byte full focused pass. These two runs qualify only
the bounded prepared checkpoint-recovery protocol in review-first and Auto-submit modes. They do not
qualify unsafe/unknown/submitted actual-PostgreSQL recovery, key rotation, negative concurrency/head-
change/session-reuse cases, an ordinary irreversible Phase-B transition, native cloud execution,
provider receipts, production cost or release behavior.

## Required fixture contract before implementation

The proposed two-stage JSON bridge is acceptable only if its frozen contract retains all of these
properties:

1. Both request and result use an exact schema version, reject unknown fields, bound file count and
   bytes, require ordinary regular files at verified owned paths, and are created/read with mode
   `0600` below one mode-`0700` canonical root. Secrets and candidate payloads never enter stdout,
   stderr, reports, command arguments or source files.
2. The helper verifies the physical private PostgreSQL cluster identity before every mutation:
   database, data directory, loopback address/port, current user and marker/root ownership must
   match the canonical harness. An arbitrary URL or an owner/default database is rejected.
3. Pre-queue bootstrap installs only existing signed staging/general managed release, runtime,
   fleet and volume prerequisites. It contains no predicted run ID, browser-session ID, session
   alias, application row or terminal state. Generated worker and test-volume private keys remain
   private fixture material; release/runtime descriptors remain explicitly synthetic.
4. Candidate/source/Track/packet/approval and `POST .../runs` proceed through ordinary APIs. The
   returned run ID, `cloud-{application_id}` session ID, workflow ID/request ID/digest and material
   are read back before the post-queue request is created. Queue replay must preserve one command,
   one reservation and one session.
5. Post-queue proof construction accepts those actual immutable identifiers, re-reads their exact
   durable rows and may add only required run-specific runtime/volume authority. It must reject a
   mismatch rather than rewrite the command, application, reservation or session, and must prove
   that no `session.id = run_id` alias exists.
6. The wrapper remains hidden behind `integration-test-support` plus debug assertions and is
   release-guarded. There is no HTTP seed route, production fixture key, generic trust SQL,
   production/shadow scope, flag-default change or fixture-data path callable by normal APIs.

## Fix-shape and replay constraints

After a causal red, active claim/admission should derive the browser session through the exact
durable account/application/run command authority and validate its frozen start payload. It must
not select the current session, accept caller aliases or use a broad `run OR session` lookup. The
same resolved session belongs in payload validation and ATS Phase A; missing, stale, cross-account,
cross-application, cross-run and profile mismatches remain closed.

Do not move this active-command/session dependency into heartbeat or finish. Prepared claim replay
may rotate the token and increment the fence. A release replay requires a second actual signed HTTP
request with a new nonce and exact original token/fence/outcome; calling
`ActiveExecutionLease.finish()` twice only returns its cached promise. Submitted-receipt replay and
retained terminal lease paths must continue to use their frozen authorities even when active
command materialization or policy heads have advanced.

## Qualification gates and limits

The real review-first `ExecutionLeaseClient` claim, observed heartbeat mutation, safe release and
fresh exact terminal replay were proven by session80446 on its admitted source; parent reports
session63105 preserved that review-first lifecycle on the later auto-repeat source with clean API,
PostgreSQL, process and root teardown. Session55819 proves the ordinary prepared auto-submit first
claim and same-owner unexpired repeat with stable Phase A, rotated live token/fence, stale-token
denials and safe release/replay. Session29416 separately proves the checkpoint/session causal red,
while parent-reported sessions86077 and20772 prove the bounded review-first and Auto-submit
checkpoint correction on their admitted bytes. Before source acceptance, preserve the FIX831 repeat
positive on current final bytes while completing SQLite plus actual private-PG regressions for
unsafe/unknown/submitted recovery, key rotation, concurrency, head change, session reuse, stale/cross-
binding authority, command terminalization and policy rotation, with immutable command/session/
reservation/attempt, authority, evidence and metering snapshots around every negative. The
auto-submit negative matrix and ordinary live Phase B remain separate. No click, submission,
browser, Temporal workflow, employer/provider call or native storage/runtime measurement is part
of the completed protocol-only proof.

The bounded green runs supplied source identity and cleanup evidence for their own roots. Full
current-source focused/Jobs/server gates and launcher self-test remain unverified. The prospective privacy scan is
recorded above. No Phase638 source acceptance, production readiness, release approval or broader
Jobs completion is granted by this review.

## Current disposition

### Prepared-protocol negative matrix static checkpoint

I independently reviewed the frozen 24-case matrix and its six closed adapter faults before heavy
execution. The initial static pass missed one P2 harness-catalog error. Parent-reported session55887
reached the tenth case and then failed closed: `proof_consumed_replay` returned the production
contract's 409 `Execution lease is not available.` (SHA-256
`b0c0ade598bbb038c6314b2174ca12113b874d7fc692d5a669fd5777dce2ece5`) rather than the catalog's
expected 401. This was not an authorization bypass: the read-only proof verifier intentionally does
not consult consumption history, the claim transaction's duplicate authority-use insert returns
`RunnerVolumePurgeError::Unauthorized`, `execution_lease_from_runner_volume_error` maps that to
`ExecutionLeaseError::Conflict`, and the API maps conflict to the generic 409 public response. The
catalog and independent test fixture now expect that exact 409; no production mapping or guard was
changed. Session55887 does not qualify the first nine cases or the matrix as a whole. Parent reports
its API/fixture/PG/root cleanup completed, and its 774-file manifest
`59b955b1434baf9739f97b137306e43a09d5535b947ddc36d92590758eb237de` matched before/after.

A second P2 catalog error surfaced in parent-reported review-first session40249. The run reached
`wrong_run` and correctly returned 409 `Execution lease is not available.` with client code
`lease_unavailable`, rather than the catalog's 404 target-missing/request-failed expectation. A
fresh proof for the changed run does not reach a simple lease-row lookup: active managed-session
resolution calls `postgres_managed_cloud_execution_command_authority`, whose exact query includes
both the changed run and original workflow request. Zero exact rows is
`ManagedCloudRegistryError::IdentityConflict`, then `ExecutionLeaseError::Conflict`, then the
generic 409 public response. FIX837 changes only that catalog entry and its independent both-mode
regression; no production predicate or mapping changed. Session40249 is not matrix qualification.
Parent reports clean API/PostgreSQL/root teardown and an unchanged 774-file manifest
`10e54a5d4efe4649378ab47b083c9e51ea5cfb713c878064e21cb4ef0ba7050c`.

I re-traced every remaining case after FIX837. `wrong_application` still returns 404 because the
application discovery query precedes managed-session resolution. `wrong_profile` resolves the
frozen session and target, then fails the explicit supplied-profile equality check as conflict.
Runtime grant/hash reach the trusted process-runtime comparison; runtime instance reaches the
current managed-runner lookup/readiness check; all map conflict to 409 and claim-client
`lease_unavailable`. The alternate worker passes signed transport and proof verification, then is
denied 401 by the explicit verified volume-worker comparison in the handler. Heartbeat and finish
select the exact account/application/run lease and map bad token/fence to 409. Checkpoint recovery
validates owner/token/fence with `validate_checkpoint_lease_access`, likewise mapping each mismatch
to 409 while the client reports `request_failed`. I found no remaining static status/code ambiguity
in those listed paths. This does not substitute for executing either full 24-case mode.

No P0-P3 source finding remains after that correction. The matrix pins an exact ordered
operation/status/public-body-hash/client-code contract; uses a fresh worker nonce and volume-proof
request ID except for the two deliberate replay cases; checks the intended changed field while
holding all other claim or later-operation fields fixed; and compares all 20 protected database
families before and after every case. The claim adapter has no arbitrary mutation callback: its
only selectors are bad signature, altered signed payload, runtime grant/hash/instance mismatch and
an alternate authenticated worker whose proof is still signed by the owned volume key. Production
ordering supports the expected denials: owner mismatch and proof/body-shape failures occur at the
API boundary, consumed proof fails transactionally, target/profile/runtime cases reach their typed
lease paths, and the alternate worker reaches the volume-worker mismatch rather than an unsigned
transport shortcut. Heartbeat and finish deliberately test only token/fence; this review does not
invent a worker-ID binding those endpoints do not have.

The retained tail requires a fresh signed heartbeat on the original token/fence, a safe released
finish and a second fresh signed terminal replay, with unchanged immutable/attempt state and zero
effect-authority counts. The controller allowlists only claim, heartbeat, finish and checkpoint
recovery paths, so no irreversible/effect endpoint can satisfy the matrix. The wrapper binds
review-first versus Auto-submit to its invocation rather than the report, hashes the admitted
source, uses private root/home/database inputs, and validates the closed report again. Its frozen
SHA-256 was parent-reported as
`4fd2944021d7f045d47cea06129bb4089a55195f63a6aa691bb8f02f52d3d706`; the report-test SHA-256 was
updated to `39395be6ab38302bbbb56c17373a4333e2f51b9aa61b2cdab69e3cc51df9894d` by the focused FIX836 and
FIX837 regressions. The former mutates the consumed-proof entry back to the old 401 plus
cryptographic-error body; the latter mutates wrong-run back to the old 404, body and client code.
Both modes must reject both stale contracts, and the existing exact-field comparison rejects every
other status/hash as well. Parent reports 295/295 Node contract tests passed with zero skips (run
reference `f54abf`); I did not rerun them. Parent subsequently reports the final prospective-index
whitespace/privacy command exited 0 over 2,932 tracked paths and 2,656 text files with tree
`a738c3d8c51cb661783784ca67dd458a45ce80f9`, after the bounded source and documentation changes.
The 774-file source manifest remained
`e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3` with the same HEAD/scoped
status. I did not rerun that scan. Its tree necessarily predates this review-only factual edit, so
the parent's planned post-edit rescan remains the final mechanical checkpoint; this is not a source
or runtime hold. Parent also reports the owner-facing Asana readback keeps protocol, evidence and
cost work incomplete and task `1219999301138331` at risk; this is consistent with the yellow scope
and does not convert the bounded matrix pass into broader completion.
Cross-account admission, readiness rotation and policy-head rotation remain explicitly deferred;
the current-byte FIX831 repeat path, full regression gates and every external/release hold below
also remain open.

Parent subsequently reports both corrected private-PostgreSQL modes passed on the exact same
774-file source manifest
`e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3`, HEAD `40949858` and scoped-
status digest `6754505e` before/after. Review-first session83098 and Auto-submit session50372 each
completed all 24 ordered cases with exact status/body/client semantics and a per-denial unchanged
20-family snapshot, then retained heartbeat 200, safe release 204 and a fresh terminal replay 204.
Both also preserved the nine immutable families and attempt binding and observed zero effect state.
Review-first proved zero ATS with three clean child exits. Auto-submit additionally used ordinary
Track consent and a factual packet, decoded the initial production ATS binding, preserved the whole
ATS row and had four clean child exits. Parent reports inner and outer validators passed, all API,
fixture and wrapper processes exited, listeners refused connections, PostgreSQL shut down, exact
roots and parents were empty, the real index was empty and 38 GiB remained free. The API/fixture
SHA-256 values were
`b6ad4c4eb1d6158824b410a3af91c3c2ab2d0ebea07526adc507495bfd9f53db` /
`3d465f52d38d167b9b86abadf61365025425feb948b9421d2210e12183a0110f` for review-first and
`3786d1bb35e72bde261d0ebaa08ad2843c800a5e7195f2fc0489af51dc76dcc1` /
`a580f896e4b472384218c01b32605e58db27c6dc78b2289be230c9465ca6c675` for Auto-submit. Launcher
self-test session92494 also exited 0; its intentional cleanup-refusal messages are expected negative
evidence. I did not independently rerun or load these logs.

This qualifies only the bounded current-source prepared negative matrix in review-first and
Auto-submit modes. It does not qualify the deferred cross-account/readiness/policy-head cases,
irreversible Phase B, broader recovery/concurrency/session reuse, native cloud/provider behavior,
full current-source regressions or release acceptance.

The reproduction harness findings, FIX819 PostgreSQL prerequisite, FIX821 decoder, FIX826
finalizer, FIX828 request-ID generator and D0.1 causal red are resolved at their bounded
local-evidence levels. Session79181 confirms the original production run/session mismatch without
an alias or protected-state mutation; session80446 confirms the corrected ordinary review-first
prepared lifecycle through real signed transport and private PostgreSQL; parent reports session63105
preserved it on the later auto-repeat source; session55819 confirms the bounded prepared auto-submit
repeat; and session29416 confirms the retained checkpoint/session404 with unchanged protected state
and a still-usable original lease. Parent-reported sessions86077 and20772 qualify the corrected
prepared checkpoint recovery in both closed modes with exact replay and unchanged authority/effect
state. None grants source acceptance or broader recovery, Phase-B or broader runtime qualification.

Parent-reported sessions83098 and50372 now additionally qualify the bounded prepared negative
matrix in both modes on manifest `e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3`.
They do not alter the overall yellow disposition or satisfy the deferred and full-source gates.
The earlier FIX831 preservation run33250 passed on manifest `d66a712...` after FIX835, but it is not
an exact-current-source preservation run for manifest `e9591654...` and is not promoted here.

The next admissible evidence is current-byte preservation of the FIX831 prepared-repeat path;
unsafe/unknown/submitted, key-rotation, concurrency/head-change/session-reuse and other negative
recovery cases; ordinary live Phase B; then focused/full Jobs/server regressions and strict Clippy.
Native cloud execution, provider receipts, production cost, release approval and broader Jobs
completion remain unqualified.

### Current-Rust gate recipe static review

I reviewed the complete, unexecuted
`docs/work/COMMAND-PHASE-638-CURRENT-RUST-GATES.md` recipe. The first draft had a P1 execution
blocker: it supplied 33 selectors to `run-nonprod-postgres-tests.mjs`, whose explicit maximum is
32, so it would have failed before PostgreSQL startup. The corrected recipe uses the
module-qualified
`db::jobs::runner_volume_purge_tests::postgres_runner_volume_f` prefix. Current source contains
exactly the intended fleet-cutover and finalizer tests under that prefix; the selector list is now
32 unique entries while the required discovered-case summary remains exactly 41. The wrapper cap
was not raised.

No P0-P3 static recipe finding remains. The command pins the 774-file manifest, HEAD, scoped status
and the two prospective migration hashes; requires 32 GiB free; uses a private HOME/Cargo cache and
a closed environment; and keeps both support features plus the three established crypto
optimization overrides. Formatting and strict all-target Clippy fail first. Configured PostgreSQL
tests run through the existing wrapper in distinct fresh databases, and the recipe requires the
41/41 structured summary plus stopped-server PID-file absence before constructing a SQLite-only
environment with all PostgreSQL URL/required-test variables absent. Focused SQLite regressions then
precede the default-parallel all-target suite. Cargo exit status is authoritative; the supplemental
parser requires at least 11 successful target summaries, zero failed and ignored tests, and a total
not below the prior 1,872-pass floor. A terminal source/HEAD/status comparison prevents accepting
mutated bytes.

This is static clearance to execute the recipe, not execution evidence. The separate current-byte
Auto-submit prepared-repeat preservation must use the separately documented fresh canonical root;
it is not performed or implied by this command. The parent-reported read-only divergence audit and
the heavy gate were still pending when this note was written. Even a clean run would qualify only
the stated local Rust/configured-PostgreSQL scope; it cannot prove browser, Temporal, employer,
native-cloud, provider, hosted, production or release behavior.

### Offline Jobs gate recipe static review

I also reviewed the complete, unexecuted
`docs/work/COMMAND-PHASE-638-OFFLINE-JOBS-GATES.md` controller. No P0-P3 static finding remains.
The two mandatory browser files are present and passed as module-relative `--exclude` arguments to
workspace scripts whose test command is `vitest run`; every other workspace remains selected. The
three Node suites and five Vitest workspaces must each exit zero and produce one unambiguous parsed
summary. Node suites require zero failures and skips; Vitest summaries reject reported failures or
skips; and the aggregate must meet the stated 2,083-pass offline floor. The event correctly reports
the two named exclusions separately rather than calling them skips or a full browser pass.

The command uses a private HOME, npm configuration/cache, XDG directory, empty Playwright root and
canonical owned process group. Timeouts and output bounds terminate that exact group. It checks the
empty browser directory, process command paths and open files before, after and in `finally`. The
four ignored non-portal `dist` writes are admitted only after `git check-ignore`; five typechecks
run, the four non-portal builds run under the exclusive machine lock, and the actual portal Vite
configuration writes only to a private output with `envDir:false`. Terminal HEAD, scoped and full
worktree status, all 774 source hashes and tracked `web/jobs` hashes must match, and `git diff
--check` must pass.

This is static clearance only; the parent-owned Rust gate was still live and this command had not
started. “Offline” here describes the closed, credential-free test selection, not kernel-enforced
network isolation. The gate deliberately omits both real-browser fixtures, Playwright installation,
`npm ci`, `npm audit`, Temporal, VM/Linux, external providers and publication. The committed/index
privacy script does not replace the separate dirty-diff privacy review. A future clean execution
therefore qualifies only the bounded offline Node/test/typecheck/build/static-contract scope and
does not close the browser, dependency-install/audit, hosted, production or release holds.

### C2C/MCP and inherited PR-chain audit review

I reviewed `AUDIT-PHASE-638-C2C-MCP-READINESS.md` and
`AUDIT-PHASE-638-PR-CHAIN-26-32.md` against the cited current source and local Git objects. No
P0-P3 factual or scope finding remains. The first audit accurately separates implemented
employment/engagement taxonomy, Track filters and REST seams from an absent inspected MCP
transport/tool registry and from unimplemented C2C vendor, rights, outreach, negotiation and
receipt journeys. It explicitly labels Phase620/622 contracts as design, Phase633 as W2/local-queue
evidence rather than a C2C receipt, and the proposed read-only MCP slice as a successor rather than
completion. It does not activate a fixture, provider, worker, browser or delegated authority.

For the PR-chain audit, I independently confirmed from local Git objects that #26–#28 and #30–#32
are ancestors of current HEAD, that `f50103e8` is an ancestor of #32's recorded tip, and that #29's
recorded tip and `2e2a18a9` have the same stable patch ID
`19d38204bfa1c9033d46a2fb4357153ca1dee404`, subject and four-file stat. The current provider-grant
clock/CAS implementation and named stale-refresh regression remain present. I did not refresh the
volatile remote PR/check state; OPEN/DRAFT/head/check conclusions are the audit assignment's dated
metadata observations. The document correctly confines those results to the old exact heads,
treats #26's missing rollup as missing evidence, and does not authorize merge, retarget, push,
promotion or release.

Neither audit is source implementation or executable qualification. The parent-owned Rust gate was
still live when reviewed, and the owner program card remained incomplete/at-risk. Updating the
direct known card with these scoped audit notes does not substitute for a prohibited bulk-project
export or close the remaining program, external or release gates.

### Current-Rust gate failure and FIX838 design review

The parent reports current-Rust session15847 exited 1: the library target completed 1,763 passes,
one failure and zero ignored tests in 2,584.32 seconds, then Cargo exited101 before later binaries
and integration targets ran. The exact panic was `tests.rs:20737:10`:
`unmanaged authenticated worker may start with an absent managed tuple: NotFound`. The 774-file
`e9591654...` manifest, HEAD and scoped status matched afterward; PostgreSQL and the exact root were
removed, the parent/index were empty and no runtime remained. I did not rerun or independently load
the full log. This is a real failed current-source gate, not full-suite qualification.

The source cause is deterministic. The public worker claim wrapper and authorized irreversible
submit wrapper represent an omitted managed tuple as an outer authenticated context containing
`input:None`. Four selectors—SQLite and PostgreSQL claim and SQLite and PostgreSQL final submit—
tested only the outer context and therefore invoked the exact durable-START session resolver. A
genuinely unmanaged aligned-ID lease has no such START, so the resolver returns `NotFound`. The
direct legacy claim fixture uses outer `None` and did not expose the public worker-claim variant.

The bounded FIX838 design is statically sound. For outer context plus inner managed input, retain
the exact durable-START resolver. For authenticated inner `None`, first require the existing strict
unmanaged classifier, which rejects both complete and partial managed authority, and only then use
the aligned legacy run ID. The final-submit branches already classify before session selection. The
claim branches must add that classification before fallback and retain their later managed resolver/
classifier recheck. SQLite's immediate transaction makes the classification stable. PostgreSQL's
first check adds no lock class; its later in-transaction recheck rejects a concurrently visible
managed history before commit, rolling back any prepared writes. Outer-`None` context-free helpers,
the session resolver's direct `None` tests, the nonoptional managed-effect resolver, and
click-started terminal replay ordering must remain unchanged. This design closes the real claim and
submit compatibility gap without accepting omitted authority on a managed or partially managed
run. Implementation and current-source regression evidence remain pending.

I subsequently reviewed the frozen four-selector implementation in
`execution_leases.rs`. It matches that design with no additional production change: both claim
backends distinguish outer-`None`, authenticated inner-`None` and inner-`Some`; both final-submit
backends select durable START only for inner-`Some` after their pre-existing all-none unmanaged
classification. The new claim classifiers run inside the existing SQLite immediate/PostgreSQL
transactions, map through the established managed-cloud error boundary and are retained again in
the later claim resolver. Any denial rolls back prepared residency/runtime/ATS/lease work. The
nonoptional managed-effect call, direct session-helper semantics and terminal replay branch are
unchanged. No P0-P3 source finding remains in this frozen production delta; focused SQLite and
actual-PostgreSQL regressions plus the successor full gate remain pending.

The first frozen FIX838 test batch correctly exercises authenticated unmanaged claim plus final
submit and exact replay in SQLite and configured PostgreSQL, proves absent workflow/managed history
and null stored managed fields, and checks that replay changes none of the captured lease, ATS,
capacity, attempt, workflow, volume, session or application state. The existing SQLite and new
configured-PostgreSQL managed final-submit tests preserve omission/wrong-worker denial with an exact
snapshot. Fixture defaults remain unchanged, all environment readers now share the serial guard,
and configured PostgreSQL requires a fresh database without resetting trust tables.

Resolved P2 — the frozen `tests.rs` correction (SHA-256
`bf0a534f3b9408ed57f3e90bed1b70b71bbf703b6887c73107bde463dc1a4004`) now makes the shared managed
fixture install a complete START/binding/request-start authority, attempt the authenticated managed
claim with omitted input, require typed `Conflict`, and compare an exact snapshot before allowing
the correct `Some(&managed_cloud)` claim to succeed. That fixture is exercised by both the SQLite
managed test and the required-fresh configured-PostgreSQL managed test. The snapshot covers 16
lease/ATS/capacity/attempt/workflow/volume/runtime/session families plus decoded applications. Its
whole-table reads are restricted to opaque volume/runtime history tables that intentionally have no
`account_id`; every caller uses an isolated SQLite database or the helper's fresh-private-PostgreSQL
precondition. Remaining families use an account predicate. I found no schema-invalid filter,
authority-consumption gap or changed helper default in this correction. This is static clearance,
not an executed current-source result.

I also reviewed the prepared, unexecuted
`COMMAND-PHASE-638-FIX838-RUST-GATES.md`. Its selector transformation preserves all 41 inherited
PostgreSQL cases: three canonical selectors become one prefix without changing case count, and
three local-authority selectors become one qualified prefix to make room for the two FIX838 cases.
The resulting 32 unique selectors must discover and pass exactly 43 nonoverlapping fresh-database
cases. The five upfront SQLite regressions cover the original failure, the authenticated-unmanaged
wrapper, managed omission plus valid managed claim/final-submit behavior, and both session-authority
classifiers. Formatting and both-feature strict all-target Clippy run first. The recipe requires the
PostgreSQL wrapper to exit zero, report 43/43 and remove its PID file before constructing a later
SQLite-only environment with no PostgreSQL URL/require flag or provider credential. Source count,
manifest, migration bytes, HEAD and scoped status are pinned before and after; the final all-target
floor is only a shrink guard and actual target counts remain required evidence. I found no P0-P3
command defect. All of these runtime gates remain unverified until the command executes and cleanup
is independently observed.

That static test clearance did not survive execution. Parent-observed successor run89202 passed
formatting, strict all-target Clippy (1m59s), compilation (2m35s) and the original unmanaged-start
regression, but the new SQLite authenticated-wrapper test failed before its intended assertion:
`authenticated_worker_submit_snapshot` queried `jobs_managed_cloud_workflow_bindings` with
`WHERE account_id = 'acct-jobs'`, although the table deliberately stores only
`account_id_hmac_sha256`. PostgreSQL and later all-target stages were not reached. The command
terminated nonzero and canonical cleanup was reported; this is failed-fixture evidence, not FIX838
qualification. My earlier statement that every remaining family exposed an account predicate was
incorrect. A corrected snapshot must avoid schema-dependent account filters, remain limited to
isolated SQLite or required-fresh private PostgreSQL callers, and rerun all current-source gates.

FIX839 now corrects only that test helper. Frozen `tests.rs` SHA-256
`d00abbdb4cd67d606ee328a0977329b56a07e2fcaf573b69473205efb8b6f9f0` snapshots each of the same 16
closed-list table families without assuming an account column; decoded applications remain scoped
to `acct-jobs`. Every named table exists in both current migration families. SQLite serializes each
typed cell explicitly and PostgreSQL uses `to_jsonb(snapshot_row)`; rows are sorted before same-
backend comparison, so representation differences cannot produce cross-backend false equality.
All snapshot callers are serial tests over an isolated SQLite database or the helper's required-
fresh private PostgreSQL database. The additional managed-fixture callers also use isolated SQLite
databases, so whole-family comparison cannot absorb concurrent unrelated writes. I found no further
P0-P3 issue in the frozen correction. This restores static coverage intent only; run89202 remains a
failed run and the corrected current source has not yet executed.

The run89202 identity record also requires care: the worker began the FIX839 edit after terminal but
before the parent captured the post-run source hash, so the transient post-run manifest is not
evidence that failed-run bytes changed during execution. Parent's read-only reconstruction of the
774-entry manifest with only `tests.rs` restored to the admitted
`bf0a534f3b9408ed57f3e90bed1b70b71bbf703b6887c73107bde463dc1a4004` reproduced the admitted
`83ea8ce0d6c1061f3b6ccefcd10d3924f2f7d24f9c9818a0b1ae660a1fdd647a`; no PostgreSQL process had
started, and root/parent/index cleanup was reported. Future qualification must capture the terminal
post-run identity before dispatching any correction.

Parent-observed FIX839 successor run62014 then confirmed the source correction but exposed a second
command-only defect. Formatting and strict all-target Clippy passed in1m57s, the test binary built
in2m35s, and the first three exact regressions passed: the original failure (4.01s), the new SQLite
authenticated-unmanaged positive (3.45s), and managed-boundary omission denial plus valid claim
(5.53s). The fourth selector discovered zero tests because the command used the file-derived module
name `execution_session_authority_tests`; `jobs.rs` actually declares the path-backed module as
`execution_session_authority_db_tests`. Both session selectors therefore require the
`db::jobs::execution_session_authority_db_tests::` prefix. The exact-count guard stopped before
PostgreSQL or full tests. Parent captured matching 774-source manifest
`d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46`, HEAD and scoped status before
and after terminal, then observed root/parent/index cleanup with no PostgreSQL start.
Run62014 is useful corrected-fixture evidence for those three cases, but remains a nonzero command
failure and cannot qualify FIX838 or the full gate.

I reviewed the prepared FIX840 successor command. Both session filters now use the declared
`db::jobs::execution_session_authority_db_tests::` module path and match the two exact function
names in the path-backed source file. Before executing any of the five regressions, the command runs
the same offline/locked, both-feature, crypto-profile library target with each filter and
`-- --list`; it parses only complete Rust test-name lines, requires exact singleton equality with
the requested filter, and requires five unique names. A missing, stale, prefix-overlapping or
renamed test therefore fails before mutation-capable test execution. The existing 4MiB stdout bound,
signal propagation and nonzero-exit handling remain intact. The only other executable changes are
the private parent/event labels and the current frozen 774-source manifest pin
`d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46`; the previously reviewed
43-case PostgreSQL accounting and later no-PostgreSQL full gate are unchanged. I found no P0-P3
defect in FIX840. This is static command clearance only; no FIX840 result or final cleanup has yet
been observed.

Parent-observed FIX840 run62964 terminated nonzero with a now well-bounded fixture failure. All five
FIX838 focused cases and seven inherited preflights passed. The actual-PostgreSQL wrapper discovered
43 cases and reported 41 passed, with only the two new configured-PostgreSQL cases failing at
`AdminActorRequired`; the later 23 focused/full stages were not reached. The fixture passed candidate
account `acct-jobs` as the actor to `update_public_beta_cohort_audited`, but the production API
correctly requires an existing `accounts.is_admin` actor. This is not evidence against FIX838 and
must not be fixed by elevating the candidate or weakening the audited guard. Parent captured final
774-source manifest
`d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46`, HEAD and scoped status equal
before dispatching any correction. The PostgreSQL wrapper reported nonzero with clean server exit;
root/parent/index cleanup was observed.

The safe FIX841 shape is test-only: create a separate synthetic administrator through
`Account::create_with_admin(..., true)`, use only that ID for the audited cohort transition, and
keep `acct-jobs` explicitly non-admin before and after while verifying/enrolling only the candidate.
No direct role SQL, temporary candidate elevation or production guard change is justified. Frozen
source and current-source execution remain pending.

I reviewed the frozen FIX841 implementation in `tests.rs` (SHA-256
`bc24abd478512f85c9c5c3e8040d24a519906e4843c66fd618445db5fc303a82`; 774-source manifest
`50f7a7f78cc2a96d5664580833f5546ced067993e5d4ac146062ea61f2dc0df4`). It follows that boundary:
the fixed candidate account is verified and asserted non-admin; a separate UUID-named synthetic
account is created with the ordinary `Account::create_with_admin` API and verified; only that admin
ID performs the audited cohort update; only `acct-jobs` is evaluated/enrolled and receives the
identity/Track fixture; and the candidate is asserted non-admin again after setup. The production
admin guard requires an existing admin row with no deletion intent, so this uses its real contract
without role SQL or candidate elevation. The fresh-private-database precondition still runs before
either account is created. No other helper, production guard or trust table changed. I found no
P0-P3 issue in FIX841. Current-source PostgreSQL and full-gate execution remain pending.

I independently reviewed the prepared post-full API preservation recipe
`COMMAND-PHASE-638-CURRENT-API-RETRY-PRESERVATION.md` (SHA-256
`ab427aa5fd67fb18c2472f481590985bdd41aa0e22b3fdebb48194c28cfe90ac`). It is correctly conditional
on the parent-owned Rust gate reaching terminal zero with final source/HEAD/status equality,
PostgreSQL/PID cleanup and one passing, nonignored occurrence of each of the four current-source
FIX831 regressions. It does not substitute an old partial run or compile those regressions again.

The command uses the canonical private launcher, shared Mac lock and private PostgreSQL wrapper,
then runs only the production-client preflight, offline/locked integration-support API/example
build, and `--auto-submit-prepared-repeat`. The nested zsh/Node quoting and literal heredoc are
well-formed. Before wrapper cleanup, the builtin-only metadata reader resolves the canonical owned
root, opens the known report with `O_NOFOLLOW`, checks regular-file/single-link/current-UID/mode0600,
bounds it to1–16KiB, pins device/inode/path and unchanged size/mtime through the read, parses JSON,
and requires `sourceAccepted:false`. Its stdout contains only report basename, byte count, SHA-256
and the false acceptance marker; no private report field is printed. The strict protocol validator,
four clean child exits, no-effects assertions, PostgreSQL clean shutdown/PID absence and independent
774-source final equality/root cleanup remain mandatory. I found no P0-P3 recipe defect. This is
static clearance only: the live Rust gate, prospective privacy check and API preservation execution
must all finish independently before this contributes runtime evidence.

Parent-observed run56416 has reached a positive but nonterminal checkpoint on frozen 774-source
manifest `50f7a7f78cc2a96d5664580833f5546ced067993e5d4ac146062ea61f2dc0df4`, HEAD40949858 and scoped
status6754505e. Formatting and strict both-feature all-target Clippy passed in2m00s. Exact discovery
found all five singleton FIX838 filters; those five tests, seven preflights and all 23 post-PostgreSQL
focused cases passed (35 focused cases total). The fresh private PostgreSQL matrix reported43/43,
including the authenticated-unmanaged positive and complete-managed-omission case; wrapper status
was zero, server exit was clean and PID-file absence was asserted before later stages. The mandatory
no-PostgreSQL all-target stage has only started and is compiling target binaries. There is no full
test result, terminal source equality or launcher cleanup yet. The verdict therefore remains yellow,
and the prepared API preservation recipe must not start until run56416 reaches terminal zero and all
documented final checks pass.

Run56416 subsequently terminated nonzero in the library stage:1766 passed, one failed, zero ignored
in2882.19s; later all-target binaries/integrations were not reached. The exact panic at
`tests.rs:883` was `cloud final-submit admission performs work before prelock`. Source inspection
shows a stale lexical boundary, not a production lock inversion: FIX838 introduced
`if let Some((Some(input), _)) = managed_cloud_context {` first in the SQLite final-submit branch,
while the guard used an unscoped `.find()` and therefore scanned intervening SQLite statements
before locating the PostgreSQL managed prelock. In the actual PostgreSQL branch the same marker is
followed immediately by `lock_managed_cloud_workflow_admission_postgres_tx`; the unmanaged branch
still acquires operational hold, registry and ATS locks before beta/account authority and the new
plain-read classifier. Parent captured exact 774-source manifest
`50f7a7f78cc2a96d5664580833f5546ced067993e5d4ac146062ea61f2dc0df4`, HEAD and status equality
after terminal, with PostgreSQL already stopped and root/parent/index cleanup complete,
before authorizing edits. The 35 focused and 43 actual-PostgreSQL passes remain dated partial
evidence only. FIX842 should scope the existing strict lexical assertion to the already extracted
PostgreSQL branch; no production change or weakened lock-order assertion is justified.

I reviewed frozen FIX842 `tests.rs` SHA-256
`cca8f7aa8fa16ae98a84c5a7e5a23d638375e7802f0ee04a4ac684905cf7bb72` (774-source manifest
`3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1`). The only lock-guard
correction moves the existing `cloud_submit_postgres` extraction before the managed-prelock call and
passes that PostgreSQL-only substring to the unchanged assertion helper. The helper still requires
the managed prelock to follow its branch marker without a preceding semicolon, `lock_`, query,
execute, `FOR UPDATE` or `FOR SHARE`; the separate durable-replay-before-hold check uses the same
PostgreSQL substring. Claim ordering and every other protected-admission assertion are unchanged.
This removes only the accidental SQLite-prefix scan and does not weaken the intended PostgreSQL
lock-order contract. Production and FIX841 setup bytes are untouched. I found no P0-P3 issue in
FIX842; current-source focused/full execution remains pending.

I reviewed `COMMAND-PHASE-638-FIX842-RUST-GATES.md` (SHA-256
`3d27d395daf71d54b7d3a703d6680250e739160b4fac50eaaa1680cee1969a5c`). It pins the frozen
774-source manifest `3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1` and prepends the exact
formerly failing lock-order guard to the existing five-test array. The unchanged singleton
discovery loop therefore proves all six full names before execution, and the execution loop runs
that guard first. Seven preflights, 43 fresh-database PostgreSQL cases through the same 32 selectors,
23 later focused cases and the no-PostgreSQL all-target floor remain unchanged; the 36-focused total
is exactly6+7+23 and the existing guard adds no test to the full-suite baseline. No filter,
assertion, feature, crypto profile, shutdown check or source/cleanup condition was removed. The
remaining executable changes are the source pin and private parent/outer labels. I found no P0-P3
command defect. The obsolete 50f7 API preservation recipe remains blocked and must be repinned and
re-reviewed only after a successful current-source full gate.

## November 1 current-source Rust re-verification

Parent-reported session52148 terminated zero on the exact frozen 774-source manifest
`3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1`, HEAD40949858 and scoped
status6754505e. Formatting and strict both-feature all-target Clippy passed. The all-target summary
reported1916 passes, zero failures and zero ignored across
`[1767,0,1,14,1,1,2,113,2,1,5,9]`; all four exact FIX831 regressions passed. The external bounded
journal contained the required nine events and SHA-256
`61ab2f12353a85110fc96714514ccb9051b1dc2dfd609cd7357865597cebd2d5`. Its final evidence required
the exact runtime root absent, parent and real Git index empty, launcher PID absent, and unchanged
source manifest, HEAD and scoped status; the parent independently rechecked those identities before
authorizing later source edits. I did not independently rerun this executable evidence. The older
session76069 remains outcome-unknown and is not relabeled by this pass.

This resolves the current-source full-Rust/strict-lint gate only. It does not qualify the repinned
current-source API retry journey, the still-prepared human-hold browser mode, real login or
cross-restart persistence, hosted/native cloud, provider receipts, production flags, release
artifacts, employer effects, cost, latency or capacity. The bounded Phase638 verdict therefore
remains yellow rather than production or release acceptance.

## November 1 saved-account positive qualification

I read the generated current-run journals directly. Parent-operated default session33796 terminated
zero with the unchanged v4 ordered9/9 contract. Its journal SHA-256 is
`35130f465cd2157faaf8d928a45a4d0876a8ebeeb90d1f0699ad8785852bae54`; it records clean API/helper
exits, object-store counters `put=1/get=2/denied=0/objects=1`, PostgreSQL status zero with shutdown
and server exit, and final root/parent/index/PID/listener cleanup. Parent-operated headed session93177
also terminated zero with the separate v1 ordered8/8 contract. Its journal SHA-256 is
`308be50b1d0cbed1c0ced7a22b4ad7b6ac634e21174743a28d30f3716793df08`.

The headed run records an80,266ms hold and one synthetic prepared `review_first` application. The
profile, application and resume remained stable through actual UI reload and API/opaque-PostgreSQL
readback; unauthenticated access returned401 and the isolated account had zero applications. It also
records zero submission, browser-session, workflow-command, run-event and irreversible-receipt
effects; zero page/server errors; one intercepted body/auth-free nonlocal canary; zero spontaneous
nonorigin, popup, download or WebSocket events; four refused listeners; and zero private Chromium
process/open-file inventory after closure. PostgreSQL and launcher exited cleanly, and the exact
runtime root was absent with its parent and real index empty. Both journals finish on the same
774-source manifest `9b1ee47e1deeb796af1d771513f6c2dd60362f832f1f4c5711bd8d9b2990c764`,
status932ee, HEAD40949858 and `sourceAccepted:false`/`productionAccepted:false`.

The parent reports that its CUA interaction used only the owned synthetic fixture: Match → Refresh
Jobs → synthetic job → Factual/Review First → Prepare → toolbar reload showing Awaiting Review →
Finish. The focused Prepare control remained visible. The retained prepare and reload screenshots
exist in the isolated visualization folder with SHA-256 values
`2685a396e4b039cb38e834304d33ab56039d91639635d94e524485b3a4e58165` and
`6fa52518324d6149205c69af34a4477843c8e5fcefdc82025018f8ebcf68bf1e`; I verified their file
presence and hashes, not the interaction independently.

This is bounded local synthetic saved-account persistence across refresh only. It is not owner
approval, password/OAuth login, cross-restart durability, real application/submission, production,
provider, employer, hosted-cloud or release proof. Startup and ready-state cooperative signal
negative qualifications are still pending; no negative acceptance is inferred from these positive
runs. The overall Phase638 verdict remains yellow.

## November 1 saved-account interruption qualification

I directly read both generated signal journals. Startup session5512 completed its controller gate
with journal SHA-256
`9430914ea7fec1f5c2da18ab0955b55a77d68c6e9bdf5671f41f0a9d1b002594` and the exact ordered nine
events. It interrupted the wrapper during `start_jobs_api`, required the wrapper itself to end
nonzero, and recorded no pass report, no surviving captured descendant or Chromium handle, clean
PostgreSQL shutdown/server exit and refusal, and absent root/empty parent/index/PIDs. Ready-state
session10800 likewise completed with journal SHA-256
`f7c017633ee6886e4bed568576d8a4dc163d8d77676c5ae32c4939b461bb31fb` and17 ordered events. It
observed the exact `human_browser_ready` boundary before SIGTERM, then proved the same expected
nonzero wrapper, absent reports/processes/handles and complete PostgreSQL/root cleanup.

Both negative journals finish on the same 774-source manifest
`9b1ee47e1deeb796af1d771513f6c2dd60362f832f1f4c5711bd8d9b2990c764`, status932ee and HEAD40949858
as the default and headed positive runs, with `sourceAccepted:false` and
`productionAccepted:false`. The parent additionally reports a fresh read-only validation of all
four private journal sequences (19/24/9/17 events), strict revalidation of both retained positive
reports and their SHA/size, and final source, index, PostgreSQL, PID, listener, root and parent
checks. Four malformed mode invocations were rejected by the exact argument assertion before
resource startup. I did not independently rerun those validators or CLI negatives.

These results close the bounded local startup and ready-state cooperative SIGTERM qualification.
They do not change the documented synthetic-only persistence boundary or prove password login,
cross-restart durability, real provider/employer/application effects, hosted/native cloud,
production deployment, release artifacts or full autonomy. The November2 human guide still says
the signal checks are pending; that sentence is now stale and should be corrected by its owner, but
it does not invalidate the journal evidence above. The overall Phase638 verdict remains yellow.

The current November2 human guide was subsequently corrected by its owner to record both passing
signal-negative sessions and their exact journal hashes. The stale-guide observation in the prior
paragraph is therefore historical and no longer an open documentation finding.

## November 1 current-source offline Jobs regression

I directly read the generated eleven-event journal
`EVIDENCE-PHASE-638-SAVED-REGRESSION-20261101.jsonl`; its SHA-256 is
`b443fa8ef4f0d1ea9e1cab857e5a11203eb1fed897ea69637a5112dc586c9b11`. Parent-operated session56666
terminated zero. The journal records Node counts306/7/17 and Vitest counts806/219/310/321/397,
for2383 tests with zero extra skips. All five typechecks, four non-portal builds and the private
portal build passed; tracked portal output remained unchanged. Browser inventory at before, after
and finally recorded zero files, processes and open files.

The journal begins and ends on the same 774-source manifest
`9b1ee47e1deeb796af1d771513f6c2dd60362f832f1f4c5711bd8d9b2990c764`, scoped status932ee and
HEAD40949858, with a clean real index. It records launcher exit0/null signal, absent exact root,
empty parent, absent owner/group and `sourceAccepted:false`/`productionAccepted:false`. The two
mandatory configured-browser files were deliberately excluded and `fullBrowserGate` is explicitly
`unverified`; this offline result does not replace their separately recorded browser evidence.
Parent additionally reports fresh launcher self-test44139 terminal zero, including its intentional
negative cleanup diagnostics, and a four-file hash-restoration calculation showing the other770
inputs are byte-identical to the earlier3de70 source. That calculation is provenance evidence, not
new Rust or browser execution, and I did not independently rerun either command.

## Successor browser-to-receipt boundary decision

I reviewed `DECISION-PHASE-638-BROWSER-RECEIPT-BOUNDARY.md` at SHA-256
`8a6b96f05bfd9d544b4376ac406423889f2f1cf55a650d6d9c2c71822559cb8b`. It accurately records the
selected bounded design: retain the production network guard, use only a test-private Route facade
that maps an approved continuation to exact fixture fulfillment, and deny all malformed, private,
non-target, redirected, unrecognized and WebSocket traffic before fulfillment. It does not claim
that this proves production DNS, namespace egress, TLS or employer transport.

The decision correctly rejects changing production `continue()` semantics merely to compose a
fixture and rejects presenting an extracted executor with invented storage as full-path proof. It
requires normal measured runner startup, the real lease and managed-v2 native volume/profile
residency before browser execution, then ordinary API/PostgreSQL receipt finalization. It also keeps
the artifacts distinct: explicit-review final-submit proof schema3 and review receipt schema1, with
no auto-submit/schema4 inference. The local VM resource decision and representative platform/runtime
qualification remain pending; the old synthetic Linux descriptor and ARM64 construction evidence
are not relabeled as measured Linux/x86_64 execution. I found no P0-P3 records or design defect in
this decision note.

These additions close only the bounded current-source offline Jobs regression and static successor
decision review. The overall Phase638 verdict remains yellow: joined normal runner-to-browser-to-
receipt execution, measured managed runtime/storage, provider/employer transport, production flags,
deployment, release artifacts and the broader autonomy objective remain unqualified.
