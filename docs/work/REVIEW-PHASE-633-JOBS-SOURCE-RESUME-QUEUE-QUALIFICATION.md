# REVIEW: Phase 633 Jobs source resume and queue qualification

Date: 2026-10-27 (America/New_York)
Base: `73bf9c3745b6beb6e3c571f3e03b23984b18313f`
Reviewer role: independent source and evidence review
Verdict: 🟢 accept for this bounded local Phase 633 batch; not production or release approval

## Scope reviewed

- `jobs/scripts/nonprod-object-store.mjs` and its complete focused test file.
- `jobs/portal/nonprod/browser-api.integration.mjs`, complete Phase 633 child diff and the
  unchanged production API/object-storage contracts it exercises.
- `jobs/scripts/nonprod-browser-api-report.mjs` and its complete focused test diff.
- `jobs/scripts/run-nonprod-browser-api-tests.mjs`, complete launcher diff.
- `server/examples/jobs-nonprod-browser-fixture.rs`, complete entitlement-option diff and relevant
  existing root/database/account/Track guards.
- `jobs/package.json`, normal-test wiring only.
- `docs/rounds/ROUND-633-JOBS-SOURCE-RESUME-QUEUE-QUALIFICATION.md` and the current implementation
  record.

No product or test source was edited by this reviewer. No Cargo command, service, browser,
PostgreSQL cluster, provider, production account, application, email, deployment or external action
was run.

## Resolved validation findings

### P1 — source upload could pass after changing unrelated candidate data

The initial child checked only the candidate name after omitting the upload profile. That did not
prove that the server preserved the rest of the manually entered candidate or preferences.

The final child snapshots the profile and preferences before upload, removes only the six exact
server-owned source/timestamp fields from both profile copies, and deep-compares everything else.
It separately requires the new source asset, SHA-256, name, media type and template status.

Location: `jobs/portal/nonprod/browser-api.integration.mjs:533-544`.

### P1 — a stale or non-factual second packet could qualify

The initial reprepare checks did not bind the provenance mode, did not reload the old packet after
the source change, and could accept a duplicate application alongside the expected ID.

The final child captures the real Matches-button POST, bounds its response, requires the exact
fixture job/factual/Review-first request, factual resume and provenance, a fresh resume ID and exact
current source authority. It reloads and deep-compares both the old immutable resume and the fresh
resume, preserves employment and skills, and requires exactly one application bound to the fixture
job before and after approval.

Location: `jobs/portal/nonprod/browser-api.integration.mjs:551-567,576-580`.

### P1 — queue success could pass without proving an included, non-debit packet

Counting one metering row and one used packet did not establish that the row was included, that its
amount was zero, or that the account balance itself stayed unchanged.

The final child requires the production approval response to report `newly_metered`, `included`, a
zero amount and exactly one used allowance. Its read-only PostgreSQL snapshots require one new
reservation, metering row and included row; a zero aggregate amount; unchanged balance and balance
event count; no workflow command; and workspace usage equal to the database. A repeated approval
must return 409 and leave the complete snapshot unchanged.

Location: `jobs/portal/nonprod/browser-api.integration.mjs:218-226,559-585`.

### P1 — synthetic positive fixture omitted typed engagement authority

The second runtime reached the fresh factual and immutable packet checks but correctly failed
`queue_local_allowed`. The helper supplied only `full_time`; mentioning W2 in description prose is
not positive typed engagement evidence.

The corrected helper uses the production discovery representation `full_time w2`. The shared
test-only positive-authority adapter splits only one exact nonempty canonical employment/engagement
pair before passing both fields through the normal discovery validator. Its tests cover all 32
canonical pairs and preserve plain, mixed, malformed, space-altered, unknown and negated input for
normal validation. When the positive queue option is requested, the helper additionally requires
both normal category checks to have passed. The browser independently requires current policy,
role, location, employment, engagement, freshness, source-health and ATS checks, no hard failures
and active ATS status before accepting local queue authority.

Locations: `server/src/db/jobs/production_positive_authority_fixture.rs:28-43,122-150,394-439`;
`server/examples/jobs-nonprod-browser-fixture.rs:390-394,426-441`;
`jobs/portal/nonprod/browser-api.integration.mjs:565-578`.

## Object protocol fixture review

- The fixture binds only to an ephemeral loopback port and generates its own bucket, prefix, access
  key and secret. The API child receives only those generated values; it does not inherit owner,
  AWS or R2 credentials.
- It independently reconstructs the SigV4 canonical request and HMAC chain. It requires unique
  host/date/payload-hash/authorization headers, the exact credential scope and region, bounded clock
  skew, exact content length and hash, and rejects transfer encoding, trailers and extra `x-amz-*`
  headers.
- The only admitted key is the exact private synthetic account namespace followed by a UUID,
  SHA-256 and `.txt` suffix. Traversal, encoded separators, query strings, absolute-form requests,
  foreign bucket/prefix/account, a second object and changed retry bytes are denied.
- Bodies are bounded to 1 MiB. Pending and stored byte buffers are cleared, sockets are destroyed,
  and close is repeatable. Diagnostics do not expose paths, credentials, headers or bytes.
- The separate test-side signer does not import a verifier shortcut. Focused negatives cover
  signature, credential, date, header, body, namespace, method, content type, size, second-object and
  missing-object failures, plus listener/byte cleanup.

This is narrow HTTP/SigV4 contract evidence. It is not S3, MinIO, R2, TLS, hosted-bucket or durable
storage qualification.

## Guarded helper and launcher review

- `queue_entitlement` is a strict optional boolean and defaults off, preserving the prior request
  contract. Unknown fields and string/plan substitutes are rejected.
- Before the option can mutate anything, the debug-only feature-gated helper validates the exact
  owned root and marker, private request, generated data key, loopback PostgreSQL URL and live
  physical database identity, verified synthetic `@example.test` account, saved candidate,
  completed onboarding, and exact owned active Track with an application identity.
- The option revalidates the root/database immediately before calling the normal
  `set_entitlement_plan("pro")` function. It requires local enabled, cloud disabled and zero prior
  usage; it does not insert an eligibility row, call a payment provider or grant production billing
  authority.
- The launcher hashes the object fixture in addition to all previously admitted child/report,
  portal and helper sources. Its environment remains generated, private and rooted in the canonical
  launcher; the exact v4 report requires nine ordered cases, two clean absent API PIDs, one clean
  absent helper PID, bounded positive request counters and the object counters `put=1`, `get>=2`,
  `denied=0`, `objects=1`.

## Reviewer-observed checks

- `node --check` passed for the object fixture/test, report module/test, launcher and browser child.
- `node --test jobs/scripts/nonprod-browser-api-report.test.mjs
  jobs/scripts/nonprod-temporal-boundary.test.mjs jobs/scripts/nonprod-object-store.test.mjs` passed
  15/15 with zero failures, skips or cancellations.
- `git diff --check 73bf9c37 -- jobs server/examples/jobs-nonprod-browser-fixture.rs` passed.

These lightweight checks do not compile Rust or execute the production object client, PostgreSQL,
APIs or Chromium.

## Historical runtime attempt

Parent-reported evidence below was not independently reproduced and is not a positive
qualification:

- The first frozen run passed the helper's five focused guards and the narrow warnings-denied
  Clippy gate. The browser journey passed cases one through eight, then failed closed with
  `generic_failure` during `review_first_reprepare`, before any queue success was recorded.
- The retained cases, helper/lint results and failed run do not qualify the batch. The next run must
  use a fresh private root and re-establish the complete nine-case result and cleanup.
- The corrected selector now requires the exact synthetic employer row instead of the first row.
  Added stage names are fixed non-sensitive identifiers. Assertion diagnostics expose only an
  explicit lowercase/underscore assertion label; messages containing values or arbitrary text
  collapse to `generic_failure`.
- Runtime 2 reached and passed source upload/readback, current Track review, fresh factual packet
  generation and both immutable resume readbacks, then failed `queue_local_allowed` before
  approval. Review established that the synthetic fixture lacked typed W2 engagement evidence; it
  did not establish a production eligibility defect or a positive queue result. The exact Run 2
  root was removed and its private parent was empty according to the parent report.

## Successful bounded runtime evidence

Parent-reported Run 3 evidence below was not independently reproduced:

- Frozen session 89002 exited zero. Actual Chromium, both private APIs and PostgreSQL completed the
  exact version-4 journey with nine ordered cases, nine passes and no failures. The report counted
  66 Jobs and 12 account requests.
- The loopback object fixture observed exactly one production-client PUT, two GETs, zero denied
  normal-journey requests and one object. The journey passed current Track review, same-job fresh
  factual reprepare and immutable old/fresh readback before the included queue approval.
- Both API processes and the feature-gated helper exited with code zero and null signal. The outer
  launcher observed PIDs 61612, 61642 and 61698 absent and accepted every frozen source hash.
- PostgreSQL reported status zero, clean shutdown and server exit. The exact run root
  `/private/tmp/bluey-test-runs-phase633-source-queue-20261027/run.1793137620.56061.21073240390`
  was absent, its private parent was empty and host free space was 39 GiB after cleanup.
- The helper guards passed 5/5. The test-only positive-authority module passed 3/3, including the
  two new category regressions and its existing public-lifecycle test; the focused candidate
  category unit passed 1/1. Strict warnings-denied Clippy passed for the narrow feature-gated
  example. This is not a fresh all-target Clippy or full Rust-suite claim.

## Aggregate regression and closeout evidence

Parent-reported evidence below was not independently reproduced:

- Canonical session 36519 exited zero. The report/Temporal boundary tests passed 8/8 and object
  tests passed 7/7. Workspace tests passed automation 803, Browser 219, runner 308, workflows 321
  and portal 387: 2,053 passes total, with one existing configured-Chromium integration skip. All
  five workspace typechecks passed. The exact root was removed and its parent was empty.
- The skipped exact-submit/beacon guard was then run separately with pinned Chromium installed only
  in a fresh owned scratch root. Session 25836 exited zero with 1/1 passed and no skip. The test used
  intercepted synthetic HTML/POST responses, denied the delayed beacon and aborted every other
  network route; it did not contact an employer. Across the two runs 2,054 cases actually executed
  and passed, without rewriting the first run's one skip.
- Launcher self-test session 53627 exited zero, including its intentional negative cleanup,
  signal and recovery cases. Both closeout parents were empty.
- The parent-reported privacy scan passed 2,843 tracked paths and 2,567 text files, including all
  newly staged files; cached diff checking passed. This reviewer remained read-only for product and
  test source and edited only this independent review record.

## Current verdict

🟢 **ACCEPT FOR THE BOUNDED LOCAL PHASE 633 BATCH** — no remaining P0-P3 finding in the
reviewed source, closeout documents or supplied local evidence. This does not approve production,
deployment or release, and does not prove hosted storage/TLS, a real subscription or balance
charge, runner execution, provider/ATS behavior, employer submission, real receipt,
mailbox/calendar action or production capacity. No fresh full Rust suite, all-target Clippy,
release artifact, hosted canary or external-provider test is claimed.
