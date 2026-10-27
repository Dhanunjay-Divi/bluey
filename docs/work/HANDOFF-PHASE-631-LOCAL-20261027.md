# Jobs-only Phase 631 local handoff

Date: 2026-10-27. Bounded local candidate-journey qualification complete;
the overall Jobs product goal remains active, not complete.

## Authority

- Checkout: `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`
- Branch: `feat/phase-631-jobs-candidate-journey-qualification`
- Base: `f4191d094ef1a0f5c3cfd60c86df465b5a8d8016`
- Closeout commit: resolve subject
  `test(jobs): qualify complete candidate onboarding and isolation` on this branch.
- Current code/local handoffs are authoritative. No SSD was used.
- Load `bluey-ops`, checkout AGENTS.md, Round 631, IMPL/REVIEW 631 and
  `docs/work/JOBS-V1-COMPLETION-LEDGER.md`.
- Preserve inherited untracked REVIEW-628. Never use the meeting-owned
  `/Users/uno/Downloads/cue`, desktop/overlay or Pinky checkout for Jobs work.

## New observed proof

Actual Chromium → current portal → real default-feature Rust Jobs/account APIs
→ private PostgreSQL: 5/5 ordered cases passed. Complete manual onboarding,
employment/role/location/W-2/C2C policy persistence and Track reload, two-account
read/write isolation, and honest empty/disabled Applications/Automation/Settings
states were exercised. Fixture JWTs are not signup/login/OAuth proof.

Report boundary: 6/6 tests, now run by normal Jobs `npm test`. Reports require
exact version/cases, no failures, two distinct clean/absent API PIDs, bounded
request counters and private file metadata. Launcher rehashes product and
harness source. CI guard, provenance, schema and launcher self-tests passed.

Runtime: terminal session 45745, exit 0; five cases, 32 Jobs/6 account requests;
API PIDs 371/441 code 0/signal null, required absent; PG clean shutdown.
Both failed and passing runtime parents are empty; source scope checks pass.
No preview service remains. IMPL retains the failed first attempt (unopened
dialog text assertion), which was a harness defect, not a product failure/pass.

Previous Phase 630 Jobs 2,039 tests/five workspace checks and Temporal 10/10,
and Phase 629 Rust 1,838/Clippy/Linux smoke remain dated baseline evidence.
No product Rust/SQL/UI/native/overlay/root web/workflow config changed here.
No fresh full regression or final-source release image is inferred.

## Reproduce

```sh
npm run test:nonprod-reports --prefix jobs
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
  with-machine-lock --machine mac-build -- \
  scripts/run-bluey-tests.sh -- node jobs/scripts/run-nonprod-postgres-tests.mjs \
  --pg-bin /opt/homebrew/opt/postgresql@17/bin -- bash -c 'set -euo pipefail
cargo build --offline --locked --manifest-path server/Cargo.toml --bins
node jobs/scripts/run-nonprod-browser-api-tests.mjs'
```

Never supply an existing/owner database URL. All heavy work uses the same
shared `mac-build` lock and exact canonical scratch cleanup.

## Next bounded work

Stitch packet preparation into this actual browser/API/PG journey. The existing
Rust `integration-test-support` positive-authority helper can seed signed
synthetic discovery/source/ATS/integrity via real authority lifecycles, not
eligibility SQL. A separate debug-only seed bridge is still needed. Normal
APIs may remain default-feature; both processes must use the same generated
32-byte Jobs data key and exact physically verified owned PG. Fixture signing
trust is persisted; no production/root-anchor credential is needed. Fresh DB
is required because fixed fixture authority IDs must not overwrite other trust.

Then use ordinary factual Review-first preparation and actual kit rendering.
Without a source-resume asset/current approved Track, approve must deny before
approved execution, attempt reservation or metering. A disabled distribution
flag alone is NOT sufficient to assume approve cannot queue. Positive approval,
actual runner execution and employer receipts are later separately bound tests.

## Remaining product/release holds

See the completion ledger: discovery breadth/freshness, object storage/model,
provider/OAuth/ATS canaries, billing and capacity, C2C, actual business messaging,
scoped MCP, extension/mobile parity, final-source Linux/release artifacts,
hosted exact-tip checks, rollout/in-flight rollback/kill switches remain.

No push, PR/merge, registry/signing publication, deploy, paid infrastructure,
production flag, real application/email/calendar or desktop/overlay action
occurred. Existing no-deploy/no-real-effects instructions remain in force.
Do not stop independent safe implementation merely because an external gate
needs separately scoped resources or authority.
