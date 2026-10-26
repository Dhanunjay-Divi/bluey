# Jobs-only Phase 628 local continuation — handoff

Updated 2026-10-26. Local implementation, available code gates and staged
privacy checks completed. This is a Jobs-only candidate, not release approval.

## Authority and scope

- Checkout: `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`
- Branch: `feat/phase-628-jobs-source-verifier-release-v3`
- Continuation base: `529f49d6687372caee94e6e01e6a8b64f0698c5b`
- Original Phase 628 base: `fc7e535143a262689da8e99ce7638aee42f40407`
- Implementation commit: `9b2e2e08a18908f063d11bd1a9361f0276ec5f53`
- Documentation successor: `docs(jobs): record Phase 628 verified local handoff`.
  Read `git log -2` for both exact local commit IDs; neither was pushed.
- Read `bluey-ops`, this checkout's AGENTS.md, Round 628, IMPL and FIX-789/790.
  Current code/local handoff are authority. SSD archives are historical fallback
  only when a specific missing fact requires them; none was needed here.
- Never operate on `/Users/uno/Downloads/cue`: meeting/host/overlay work is
  excluded. Desktop/Pinky integration and future Jobs roadmap phases are excluded.

The continuation preserved the inherited dirty Phase 628 implementation. This
is one complete local batch, not permission to cherry-pick unrelated checkouts
or claim the whole Jobs product is production-ready.

## Implemented architecture

1. Exact worker-HMAC admission and claimed session proof authenticate boot
   registration/startup. A process owns stable boot/request IDs; the server
   owns monotonic generation and canonical response replay.
2. Current boot publishes degraded/startup N. The server provisionally installs
   ready N+1 inside a savepoint, executes the actual lease path, rolls everything
   back, then atomically commits the real probe result, ready head, heartbeat,
   audit and immutable response. Failed/unknown probes cannot fabricate idle.
3. Startup requires exactly degraded/startup. Generic heartbeat replay checks
   current boot before returning; old or late failures cannot lower readiness.
4. Every v3 verifier work request retains its process boot identity. Fresh
   readiness, boot head, heartbeat sequence binding and attempt boot identity
   must agree. A new process cannot adopt the previous process's attempt.
   Exact response-only terminal replay remains tied to its original attempt.
5. SQLite's leaseability probe uses the real lease clock at millisecond precision,
   avoiding false idle results from the managed timestamp's second truncation.
6. Prospective SQLite 065 / PostgreSQL 043 retain committed migration bytes,
   admit the exact ten-capability v3 shape and persist immutable heartbeat and
   attempt boot bindings. Attempt companions delete only with their authorized
   parent attempt, not by direct rebinding/deletion.
7. V1/v2 canonical fixtures/order remain separate. V3 has nine runtime roles,
   portal, fourteen ordered protocols and bounded identical-body retries.

## Evidence ledger

See IMPL for full commands, failed attempts and final results. Current observed
evidence includes five Jobs typechecks, 2,018 passing Jobs TypeScript tests plus
one explicit skip, 32 release-gate tests, schema parity (113 tables/91 indexes),
CI guard self-tests, local v3 contracts, 14 macOS native tests and the real N-API
addon smoke. The separate GPT-5.6-Sol static pass reports the assigned boot
blockers resolved; it is not human review or release approval.
The staged implementation privacy scan passed (2,793 tracked paths / 2,517
text files). Provenance passed (663 lock entries / 631 package versions /
14 commit-pinned repositories). All four non-portal TypeScript emits and the
portal typecheck/production bundle build passed in cleaned temporary output.
The portal's >500 kB chunk warning remains documented; no threshold was raised.

Rust final all-target ledger: PASS, exit 0. Library 1,693 + binary unit 1 +
integration 134 = 1,828 passing tests. The library took 2,786.26 s; the 113-test
HTTP suite took 545.97 s. Initial full library: 1,681 passed, ten failed; those
failures remain in FIX-790. Focused boot/auth/migration coverage passed 20/20;
real leaseable/rollback/immutable-attempt checks passed. The 32 changed source
and configuration file hashes matched before/after the final successful run.
Performance-analysis guidance was used to measure the slow export setup;
batching removed repeated full snapshot publication without removing its
102-row, source-positive or pagination-boundary assertions.

## Verification command patterns

All heavy Cargo commands use `scripts/run-bluey-tests.sh -- <command>` with
fresh task-owned Cargo/data/config/runtime/log/temp/SQLite paths, debug info
disabled, and inherited PostgreSQL URLs removed. Final test commands use:

```sh
cargo --config 'profile.test.package.curve25519-dalek.opt-level=3' \
  --config 'profile.test.package.ed25519-dalek.opt-level=3' \
  --config 'profile.test.package.sha2.opt-level=3' \
  test --offline --locked --manifest-path server/Cargo.toml --all-targets \
  --features integration-test-support,business-messaging-simulator-test-support
cargo clippy --offline --locked --manifest-path server/Cargo.toml --all-targets \
  --features integration-test-support,business-messaging-simulator-test-support -- -D warnings
```

Do not run these unwrapped or point them at any owner/production database.
Test-only dependency optimization changes no assertions, flags or release
artifact. The macOS smoke compiled runner JS and the real addon into the same
owned temporary tree, used the normal addon loader, and removed that tree.

## Pending external evidence and risks

- Dedicated non-production PostgreSQL runtime/concurrency: no configured URL;
  installed PostgreSQL 14/17 lack pgvector. Shared/default databases were not
  used. Early-return PostgreSQL tests are not runtime proof.
- Exact Linux Docker image/native smoke: local Docker CLI exists but its
  configured local daemon socket is absent. No remote context was selected.
- Full independent review of the entire batch remains required. The canonical
  REVIEW is still unapproved; scoped AI review is not a substitute.
- Hosted exact-tip CI, registry publication/read-back, signing, canaries and
  release approval remain separate gates and require authorization.
- Boot/heartbeat authority retention and fleet-scale database growth require
  capacity review before production enablement.
- No rollout/rollback/kill-switch rehearsal, provider or ATS canary is claimed.

## Explicitly untouched

No push, PR creation, merge, rebase, deploy, registry write, paid infrastructure,
production credential access or flag change. No ATS/provider call, application,
email, calendar event or competitor/private-session operation. No meeting,
desktop, host or overlay file was intentionally edited. Local verification
does not enable autonomous customer actions.

## Final source/cleanup manifest

The successful final Rust root was verified absent after exit 0:
`/private/tmp/bluey-test-runs-501/run.1793045008.18045.29205301450`.
Native and frontend qualification roots were also verified absent; the final
launcher self-test passed. Stopped/failed iterations were cleaned by the same
supervised launcher, never by broad cache/workspace deletion.
The tracked worktree is clean after the documentation commit. The sole
intentional untracked file is the inherited, unedited pending REVIEW listed
below; it was not self-approved or folded into the implementation commit.

The following 40 paths comprise the candidate relative to the continuation
base, including inherited Phase 628 changes. The canonical REVIEW is retained
unapproved, untracked and exactly unedited, as inherited. It is excluded from
the implementation commit rather than modifying its trailing-blank formatting.
Its pending checklist is not the observed verification ledger (use IMPL for
that). The other 39 paths comprise the committed implementation and handoff.

```text
.github/workflows/jobs-managed-cloud-release.yml
CHANGELOG.md
docs/rounds/ROUND-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md
docs/work/FIX-789-jobs-source-verifier-startup-readiness.md
docs/work/FIX-790-phase628-jobs-regression-fixtures.md
docs/work/HANDOFF-PHASE-628-LOCAL-20261026.md
docs/work/IMPL-PHASE-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md
docs/work/REVIEW-PHASE-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md
infra/postgres/server-runtime/040_jobs_original_source_verifier_startup_authority.sql
infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql
infra/sqlite/server-runtime/062_jobs_original_source_verifier_startup_authority.sql
infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql
jobs/OPERATIONS.md
jobs/automation/src/managed-cloud-runtime-client.ts
jobs/automation/src/worker-auth.ts
jobs/automation/tests/managed-cloud-runtime-client.test.ts
jobs/automation/tests/worker-auth.test.ts
jobs/scripts/check-jobs-schema-parity.mjs
jobs/scripts/ci-guards-self-test.mjs
jobs/scripts/managed-cloud-release-gate.mjs
jobs/scripts/managed-cloud-release-gate.test.mjs
jobs/workflows/Dockerfile
jobs/workflows/src/original-source-verification-api.ts
jobs/workflows/src/original-source-verification-runtime.ts
jobs/workflows/src/original-source-verifier.ts
jobs/workflows/tests/original-source-verification-runtime.test.ts
server/src/api/jobs.rs
server/src/api/jobs_managed_cloud_releases.rs
server/src/api/jobs_worker_auth.rs
server/src/db/jobs.rs
server/src/db/jobs/applications.rs
server/src/db/jobs/ats_certification_page.rs
server/src/db/jobs/managed_cloud_release_authority.rs
server/src/db/jobs/original_source_verification.rs
server/src/db/jobs/postgres_local_authority_tests.rs
server/src/db/jobs/source_verifier_boot.rs
server/src/db/jobs/tests.rs
server/src/db/jobs/workspace.rs
server/src/db/mod.rs
server/src/jobs_managed_cloud_runtime.rs
```
