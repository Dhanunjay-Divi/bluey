# Round638 — Ordinary cloud-run identity and protocol-only runner qualification

Base:409498589b8d2e62b5ef54ebdee1372aeb8d005f.
Branch:feat/phase-638-jobs-runner-control-plane. Date:2026-10-28.
Preflight:bluey-ops, current AGENTS/local handoffs, security/SPARC and native team.
Status: D0.1 independently verified causal red79181; prerequisites FIX819/FIX821
and FIX826 locally qualified. D0.2 implemented/reviewed;89311 passed14 focused
Rust tests, including actual PG finalizer. FIX828 shared request-ID generator
passed selected TS/type/native validator checks. Real managed claims now200
(69688/65097), but harness mode/opaque-storage readbacks needed FIX829/FIX830.
Review-first lifecycle80446 is now locally qualified; auto-submit ATS, remaining
negative matrix/recovery and full source gates remain open.
No production, source-release or broader-goal acceptance.

Latest prepared negative matrix83098(review-first)/50372(auto) both terminal0 on
unchanged774-source e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3.
All24 exact cases per mode passed20-family preservation, honest freshness/binding,
retained valid heartbeat/release/fresh terminal replay and zero effects. Auto
preserved complete ATS row; both cleaned API/PG/root/PIDs/listeners.295 Node tests
pass. FIX836/FIX837 retain earlier two test-contract failures without rewriting them
as green. Full source, deferred cross-account/head/readiness and other recovery/
native-cloud/provider/receipt/economics/release gates remain OPEN.

Latest55819 passed the separate ordinary auto-submit prepared retry:4 exact Rust
regressions and real-client first200/repeat200 with newtoken/higherfence, immutable
ATS bytes/deadline, stale heartbeat+safe-finish409 unchanged20-family state, current
heartbeat/release/terminal replay and cleanup. FIX831 records source/artifacts;
sourceAcceptedfalse remains. Takeover/expired renewal, ordinary rotated-lease
PhaseB, remaining negatives/recovery/full source/native/cloud/cost gates stay open.

Latest80446 exited0 on exact source6b6fccb221db48534ee54dea6632666f9d4a83732805481f8b21c4e61ea70c6b:
review-first claim/heartbeat/safe release/fresh terminal replay, protected/effect
readback and cleanup passed.65 Node tests pass. This is not auto-submit ATS,
native/cloud execution, full source or production qualification.

## Objective and evidence boundary

Close the ordinary queue-to-runner control-plane seam before claiming an actual
runner/browser/receipt journey. Phase637's qualified source remains intact.
At the start of this phase, the ordinary route staged browser session
cloud-applicationID but created a distinct40-hex execution run ID. Workflow
materialization/activities/runner retained both, while target lookup and ATS
PhaseA substituted the run ID. Causal79181 confirmed the target404. FIX822 now
resolves exact durable START session authority; review-first80446 passed. The
auto-submit ATS branch still needs its own actual proof. Historical fixtures that
create session.id=run_id cannot establish that boundary.

Use the ordinary source/profile/packet/approval/run APIs and real
ExecutionLeaseClient against an owned loopback API/private PostgreSQL system.
Synthetic signed staging/general runtime/volume/readiness authorities are fixture
data, not measured cloud software, native storage or deployment evidence.
Do not widen the Linux/x86_64 contract, use production/shadow scope, start a VM,
run a browser, create an employer connection or manufacture submitted state.

The unchanged positive-source helper retains its historical synthetic
original_source_verifier production/us-east-1/general rows ONLY inside the
physically verified private PostgreSQL fixture. That is not production access,
an API-selected production scope, or new runner authority. All new runner and
admission records are staging/us-east-1/general; embedded runtime reporters stay
disabled because synthetic descriptors cannot qualify live executable measurement.

## Ordered work and acceptance

### D0.1 — Controlled fixture and actual red boundary

0. Prerequisite discovered during actual private-PG reproduction: the production
   PostgreSQL fleet-cutover unready-count query has one extra closing parenthesis.
   Source-derived READ ONLY parser probe62355 returned42601 (965bytes,
   SHA6bd923ad639c3b7e20a808f22d71acc15af01667ac47c5615c5396c84e6779fc).
   Fix only that PG syntax and qualify actual cutover lifecycle/replay/stale
   snapshot rejection before continuing. SQLite has an additional scalar outer
   subquery and is balanced; leave it and all authority predicates/locks unchanged.
   This is a confirmed availability prerequisite, NOT the run/session causal red.
   A second observed prerequisite in run96006 is a queue handler panic at
   workflow_commands.rs:2317: PG cloud_browser is INTEGER/INT4, but five consumers
   decode bool. Replace with one fallible named-column i32/nonzero decoder,
   preserving locks, optional-row false and typed storage errors; centralize the
   already-correct normal reader too. Prove actual migrated type, zero/nonzero
   semantics, incompatible-type error/no unwind and ordinary queue/real claim.
   No schema cast/migration or session-binding change belongs in this correction.
1. Reuse production-positive source/ATS/integrity publication and existing
   managed-release/runner-volume lifecycle functions. Any new bridge is hidden,
   debug/integration-feature-only, guarded by physical private root/marker/PG
   ownership, generated synthetic account/keys and strict bounded request files.
   No HTTP seed endpoint, trust SQL, production key or flag default change.
2. Provision global fleet/volume readiness BEFORE ordinary queueing; retain full
   runtime descriptors and a generated private test-volume signing key. Split
   pre-queue provisioning from post-queue run-specific proof construction.
   Do not predict/rewrite the route's run ID or seed a session alias for it.
3. Save accurate synthetic candidate/source data, prepare/approve through normal
   APIs, then POST /api/jobs/applications/:id/runs with cloud. Read actual session,
   run, reservation and durable command material. Repeating the ordinary queue
   must not create another command/reservation/session.
4. Preserve unchanged workflow IDs through ordinary command materialization or
   existing exact lifecycle helpers. Bind request-start, release memo, worker,
   runtime and volume through production canonicalization. The real client's
   claim must reach the intended target lookup after all prerequisites pass.
5. Record the actual failure/status and read-only state snapshots. An unrelated
   signing/readiness/eligibility failure is setup evidence, not the intended red.
   No production binding fix until this red is independently reviewed.

### D0.2 — Exact active-admission binding, only after red

1. Preserve distinct execution run and reusable browser-session identities.
   Resolve the session from exact durable account/application/run command
   authority, not caller aliases, broad OR lookup or whichever session is current.
2. Apply the same binding to payload validation and ATS PhaseA. Reject missing,
   corrupt, cross-account/application/run/profile or stale authority unchanged.
3. Keep heartbeat/finish/submitted receipt replay on their existing retained
   token/fence/volume/frozen receipt authorities. Do not make terminal replay
   depend on a currently materializable command, current session or policy head.
4. Add SQLite/actual-PG regressions and session-reuse, command-terminalization,
   policy-rotation negatives. No schema/history rewrite without separate review.

### D0.3 — Positive protocol-only lifecycle and strict negatives

The review-first baseline80446 is a bounded pass with zero ATS bindings. It is
not a substitute for auto-submit. The next required private case uses an explicit
separate auto-submit scenario and preserves that baseline's v1 behavior:

- Use a wholly synthetic profile, uploaded source and signed posting that all
  truthfully name Rust, TypeScript and PostgreSQL. The resulting server score80
  meets the unchanged default threshold80. Do not lower production policy or add
  ungrounded facts to a real candidate resume.
- Save/review all profile/source/preferences/Track inputs before ordinary
  POST /api/jobs/tracks/:id/auto-submit. Prepare factual auto_submit through the
  normal applications API; require queued/schema3 track_auto_submit admission.
  Do not call review-first /approve or seed an authorization row.
- Reuse the existing synthetic certified cloud target and seven-role staging
  authority. Add only separate closed bootstrap/post_queue operations and a
  minimized typed read-only authority summary; no arbitrary decrypt/export helper,
  session alias, trusted-row rewrite or native runtime claim.
- Prove actual first managed claim with exactly one PhaseA binding tied to the
  distinct durable session, account/app/run/attempt/profile, packet, authorization
  and runtime, with preflight/fence0 and no PhaseB/consumption.
- Independent source review predicts an exact prepared repeat currently409s:
  the claim proposes a fresh token hash/expiry, but the immutable existing PhaseA
  replay requires its original nonce/expiry. First obtain a clearly labeled causal
  red, with fresh valid proof, current readiness replay and unchanged protected
  claim state. Stop heartbeat during the comparison, then use the original lease
  for safe release and fresh terminal replay. Never count this red as a green
  rotation result or weaken ATS equality to make the test pass.
- Review an explicit ATS/lease stability or renewal design only after evidence.
  The required recovered/repeated claim contract below remains in scope.

Actual76811 confirmed this intended red. The bounded arena decision is recorded
in DECISION-PHASE-638-PREPARED-ATS-RETRY.md: retain immutable PhaseA nonce/expiry
only for an exact same-owner unexpired prepared managed-cloud retry, while all
current authority checks run and the live token/fence rotate. Generic/local paths
stay strict; takeover/expired renewal remain denied, not qualified by this change.
This is selected implementation direction, not a green runtime result.

1. Real signed managed claim creates exactly one prepared lease with exact
   account/application/run/profile/command/ATS/runtime/volume binding.
2. Observe an actual server heartbeat update, same token/fence and prepared phase.
3. Safe release occurs before any irreversible marker. Prove terminal replay by
   a FRESH signed HTTP request/nonce with exact token/fence/outcome; calling
   ActiveExecutionLease.finish twice only reuses a cached Promise and is not proof.
4. At CLAIM, wrong account/application/run/profile/worker/proof/runtime/volume/
   readiness denies without extra authority/evidence/metering. Owner/web bearer
   cannot substitute for authenticated worker/runtime authority. Heartbeat and
   finish currently use signed worker transport plus retained token/fence/volume
   authority, not worker-ID binding. Their negative cases cover fresh HTTP auth,
   account/application/run and token/fence/volume, not a stronger worker-ID claim.
   Adding later-operation worker binding would require a separate reviewed change.
5. Prepared claim repeats can rotate token/increment fence; preserve that contract
   and reject stale token/fence. Do not demand byte-identical prepared grants.
6. No click/submit/email/provider call, workflow engine execution or native runtime
   claim follows. Counters/immutable state readback must make the boundary visible.

### D0.3 continuation — explicit prepared negative matrix

Use separate --prepared-protocol-negatives and --auto-submit-protocol-negatives
private runs; both validators receive expected mode from invocation, not report.
Each ordinary admitted first claim is followed by observed heartbeat, then stopped
heartbeat before exact20-family snapshots around EACH denial. Require ordered
24-case catalog, one actual HTTP response each, exact status/public-body SHA and
actual client classification where real client is used. No generic any4xx pass.

Cases: bearer without worker signature; changed signed body; owner mismatch;
identical worker-nonce replay of that rejected request; top-level proof volume and
process mismatch; proof version/signature/payload defects; consumed proof replay;
fresh-proof wrong application/run/profile; fresh-proof runtime grant/hash/instance;
fully consistent alternate-worker HMAC/owner/proof tuple; heartbeat and safe finish
wrong token/fence; real checkpoint recovery wrong token/fence/owner. Only the exact
six private adapter fault selectors are allowed; unknown selectors reject before
key handling. Preserve production client serialization/canonicalization and keys.

Record nonce/proof freshness honestly: original consumed proof and worker nonce
replays are not fresh; handler/middleware-only negatives do not qualify the proof
layer. Same-account target cases are not cross-account isolation. Defer a second
ordinary admitted account and readiness/policy-head rotation to distinct fixtures;
do not seed trusted aliases or call an upstream denial a downstream pass.

Every denial must preserve20 complete table-family snapshots. Original valid lease
must then heartbeat200 with unchanged token/fence/scope and extended expiry, safe
release204 and fresh terminal replay204. Auto requires ordinary Track consent,
production-decoded first ATS authority, COMPLETE unchanged ATS row, zero capacity/
canary/irreversible receipts; review-first requires zeroATS. Require nine immutable
families, stable attempt, zeroeffects,3/4 clean children and complete private cleanup.
This is protocol evidence, not browser/native/cloud/provider or receipt evidence.

### D0.4 — Qualification and closeout

Review every owned diff; run complete prospective-index whitespace/privacy
including new files BEFORE heavy source freeze. Use shared mac-build lock and
canonical isolated launcher, private HOME/Cargo/PG/data/output and cleanup on all
outcomes. Parent owns all runtime execution; workers do not start builds/services.
Qualify focused ordinary-path regression, actual private-PG cases and real-client
lifecycle before full Jobs/server gates. Record actual counts, source/HEAD/status,
process/group/listener/root cleanup and launcher self-test. Failures remain dated
failures. Local source checkpoint only after review/gates; no release claim.

## Ownership and coordination

- Parent: plan/IMPL/FIX records, protocol Node driver/controller/report, execution,
  source freeze/integration, and any production execution-binding fix after red.
- Existing fixture specialist: minimal debug/test-only lifecycle adapters, hidden
  wrapper and guarded fixture example/Cargo support only, after proposing the
  private JSON handoff contract. No production execution_leases/workflow behavior
  changes, no migration, no tests/builds/services/commits. Do not move entire test
  modules into production or change existing unit fixtures to mask the red.
- Existing independent reviewer: design/source/authority review; REVIEW638 only.

Workers are not alone; preserve other edits and inherited untracked REVIEW628.
Native agents only; no unreviewed framework CLI/hooks/daemon/provider installs.
The code graph refused this checkout because its server is project-bound to a
different root. Use current direct source; do not change that guard or index the
meeting checkout as a substitute. Ruflo task:task-1793204589019-9to79y.

Current Jobs checkout remains
/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831.
Never modify/start meeting-owned /Users/uno/Downloads/cue, desktop or Pinky.
No push/PR mutation/merge/rebase/retarget/deploy, signing/publication, paid
resource, production flag, owner account, real application/email/calendar,
competitor probing or VM start. SSD only for a specific missing historical fact;
none needed. Broader browser/receipt, Linux/x86_64, source/OAuth/model/monthly
billing, MCP/C2C/messaging/client parity and release conditions remain unfinished.
