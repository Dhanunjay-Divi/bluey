# IMPL — Phase 628 Jobs Source-Verifier Release V3

Updated: 2026-10-27. Jobs-only local candidate; canonical release REVIEW unapproved.
Continuation starts at `529f49d6687372caee94e6e01e6a8b64f0698c5b`, preserving
the original uncommitted Phase 628 changes and earlier Kiro commits.

## Architecture

- Rust/PostgreSQL or SQLite remains runtime/release/readiness authority.
- Managed runtime HMAC and actual claimed session proof authenticate boot
  registration/startup. Session secrets are hashed into request identity,
  never echoed into canonical responses.
- A process owns stable boot/request IDs; the server owns monotonic generation.
- Startup executes the real lease decision under a rollback savepoint,
  removes provisional changes, then commits actual probe result, boot ready
  head, heartbeat/audit and immutable canonical response in one transaction.
- Generic heartbeat requests bind boot ID/generation before replay handling.
  Stale generations and late failures cannot lower committed readiness.
- Operational requests retain the same boot identity. Current ready heartbeat,
  boot head and immutable attempt binding must agree before new work. Startup
  must cite an exact current `degraded/startup` predecessor. Exact read-only
  terminal replay preserves the original attempt's identity.
- TypeScript retries only transient failures, at most three identical-body
  deliveries. It never starts the worker without authority, falls back from
  partial boot support, accepts a client-supplied probe result, or sends a
  speculative degrading heartbeat after ambiguous startup.
- V3 binds nine runtime roles plus portal, fourteen wire protocols and latest
  SQLite 065 / PostgreSQL 043. Historical v1/v2 bytes and contract ordering
  remain unchanged. Existing legacy APIs remain separate from boot authority.
- Publication/promotion/rollback retain the existing exact-artifact boundaries;
  no hosted or production action is part of this local batch.

## Change groups

| Area | Change |
|---|---|
| Runtime client/lifecycle | Strict canonical identity parsing, session-bound boot requests, bounded stable retries, boot-scoped heartbeats |
| Worker authentication | Exact two boot paths, no prefix/suffix admission expansion |
| Server boot authority/API | Real runtime/session validation, current generation fencing, atomic server probe and ready transition |
| Heartbeat authority | Immutable per-sequence boot binding; validation before exact replay; boot-ready CAS/audit helpers |
| Migrations | Complete original 062/040 work, preserve committed 063/041 and 064/042, add prospective 065/043 |
| Release metadata | Ordered boot protocol; exact new heads; ten capabilities only for full discovery/source feature shape |
| Guards | Boot source-set digest, historical fixtures, mutation tests, five boot tables plus attempt companion and index included in parity |
| Tests | Real signed runtime fixtures replace fake-instance boot tests; duplicate startup concurrency, auth, loss, stale failure and populated migration replay |
| Jobs test lint | Remove unused helper/Copy clone and correct two existing test-only Clippy expressions |
| Fixture repairs | Real preference mutation, additive ATS corruption, exact general-channel query budget, migration prerequisites, repaired membership count and fresh authenticated positive-fixture liveness; see FIX-790 |
| Documentation | Round, FIX-789, operations and changelog updated; independent REVIEW not self-approved |

## October 27 independent-review continuation

The October 26 implementation/handoff commits remain local historical evidence.
Independent actual-diff review found two protocol/migration P2s, a contradictory
active operations runbook, and missing SQL/retry mutation coverage. Corrections
remain within Phase 628 (FIX-791 through FIX-794); no new roadmap batch or
external authority is introduced.

- Authenticated exact committed startup replay now precedes the current boot-head
  fence, while fresh publication and all operational work remain fenced.
- All transaction-owning SQLite boot scripts 063-065 use guarded rollback and
  prior-FK-mode restoration; committed script bytes are unchanged.
- Real populated failure injection covers each boot migration before its sole
  COMMIT; response replay after replacement snapshots authority tables and rejects
  stale new work, changed payloads and invalid session proof.
- The current runbook names one boot startup procedure. Retry tests verify each
  delivery's fresh nonce and recomputed valid HMAC over identical request bytes.
- SQL mutation guards now cover paired authority predicates, not only table shape.

Observed after correction: targeted automation runtime/auth tests PASS 15/15;
release-contract tests PASS 32/32; Rust boot/auth filter PASS 24/24 in 27.31 s,
including both new backend regressions; broader SQLite migration filter PASS
23/23 in 24.49 s. These filters overlap and are not 47 distinct tests.
Rust formatting and diff checks PASS.
Independent protocol static re-review found no remaining source blocker.
Strict all-target Clippy PASS (both test-support features, warnings denied).
The full all-target server command PASS, exit 0: 1,697 library + 1 binary unit +
134 integration tests = 1,832 passed / 0 failed. Library time 2,722.01 s;
HTTP integration 113/113 in 545.73 s. Both test-support features were enabled.
The exact launcher-owned root
`/private/tmp/bluey-test-runs-phase628-reviewed-20261027/run.1793079552.48662.4743250230`
was verified absent after exit, and its parent directory was empty. Both changed
Rust source SHA-256 values matched the pre-run values after cleanup.
Independent client/guard re-review ACCEPT with no scoped source blockers; its
own reruns passed parity (113/91), CI self-test and targeted client 11/11.
Staged privacy PASS (2,800 tracked paths / 2,524 text files). Final local
packaging also requires restaging and the privacy/diff preflight. Launcher
self-test PASS, including its
intentional active-process cleanup-refusal cases. The independent protocol
reviewer independently reran the two critical filters only after the parent
harness exited and cleaned up: boot 24/24 in 26.09 s; SQLite migration 23/23 in
24.41 s (overlapping filters). Its harness exited 0, preserved both source hashes
and removed the exact owned root:
`/private/tmp/bluey-test-runs-phase628-independent-postfix-20261027/run.1793083219.99359.7835160200`.
No concurrent heavy Cargo run was started. Both dated independent reports now
accept their reviewed source scopes; neither approves external/runtime release.
The 36 changed code/configuration files in the frozen continuation snapshot
remained byte-identical through final verification. The local correction commit
is identified in HANDOFF and `git log`; it must not be pushed by implication.

Parent-observed automation typecheck PASS and corrected retry/auth tests PASS
15/15. Schema parity PASS 113 tables / 91 indexes. A parent simplification of
the PostgreSQL function extractor initially failed parity because the real
scripts use both `$$ LANGUAGE plpgsql;` and `$fn$;` endings. It was corrected to
match the actual same dollar delimiter and optional language position; parity
then passed. Final CI self-test and independent re-review passed on
that corrected extractor. This guard-parser iteration did not change SQL or
the frozen Rust source under the full server harness.

### Local sample-data portal preview (not backend acceptance)

Built the unchanged portal with Vite into an isolated launcher-owned temporary
directory (2,301 modules, 5.31 s); the existing 502.51 kB minified chunk warning
remains. A temporary loopback-only static server at port 5187 serves the fixture
with no API/auth proxy, CSP `connect-src 'none'` and `form-action 'none'`, and
HTTP 405 for every write method. HEAD `/jobs/?preview=1` returned 200 with those
headers; POST `/api/jobs/workspace` returned 405. No feature flag or product
source changed to expose this preview.

Browser observations confirmed Overview -> Matches -> job details ->
Applications -> Overview, retaining `preview=1` and visible PREVIEW labeling.
The sample job showed its review-only uncertified automation boundary. The
browser tab was retained as a user-facing deliverable. Displayed account,
balance, connections, source health, applications and interviews are fixture
values, not live or verified customer activity. No real resume, account,
application, message, OAuth or provider operation was exercised.

Preview root:
`/private/tmp/bluey-test-runs-phase628-preview-20261027/run.1793080112.57557.614714310`.
It is intentionally active for the user's local test with a two-hour timeout;
its owned cleanup must be checked at shutdown, not falsely reported complete
while serving. This preview does not satisfy functional end-to-end or release
gates and is not a substitute for functional backend/runtime acceptance.

## October 26 observed verification ledger (prior committed candidate)

All Rust build/test commands run through `scripts/run-bluey-tests.sh` with
isolated Cargo target, SQLite/data/config/runtime/log/temp roots. Development
and test debug information is disabled to bound scratch growth. Inherited
production database URLs are removed by the launcher.

| Command / scope | Observed result |
|---|---|
| `bash scripts/run-bluey-tests.sh --self-test` | PASS; intentional cleanup-negative fixtures are expected, final exit 0 |
| `npm --prefix jobs/<package> run typecheck`, automation/workflows/portal/browser/runner | PASS 5/5 |
| Same five `npm ... test` suites | automation 797 passed/1 skipped; workflows 307; portal 387; browser 219; runner 308 (2,018 passed total) |
| `node --test jobs/scripts/managed-cloud-release-gate.test.mjs` | PASS 32/32 |
| `node jobs/scripts/ci-guards-self-test.mjs` | PASS |
| `node jobs/scripts/check-jobs-schema-parity.mjs` | PASS 113 tables / 91 indexes, including five boot tables and immutable attempt companion |
| Isolated `cargo check --locked --manifest-path server/Cargo.toml --lib` | PASS |
| Isolated `cargo test --offline --locked --manifest-path server/Cargo.toml --lib source_verifier_boot -- --test-threads=2` | PASS 17/17 before the additional populated migration-replay test |
| Initial strict all-target Clippy | PASS before review fixes; historical iteration evidence only |
| Final strict all-target Clippy, both test-support features | PASS after boot/clock/batched FIFO fixture corrections |
| Initial full server library | 1,681 passed / 10 failed in 2,738.69 s; stopped before later targets; not a full gate pass |
| Final all-target server command, both simulator/integration test features | PASS exit 0: 1,693 library + 1 binary unit + 134 integration tests (1,828 total); library 2,786.26 s; 113-case HTTP suite 545.97 s |
| Rust formatting / diff guard | PASS, rechecked after corrections |
| Local v3 release-contract CLI | PASS after clock/fixture corrections in launcher-owned temporary output |
| JS emit / portal bundle | PASS temporary automation/workflows/runner/browser TypeScript emits; portal typecheck + Vite production build (2,301 modules, 4.39 s). Vite reports a >500 kB chunk warning (largest minified JS 502.51 kB); no warning threshold was raised |
| Darwin native addon | PASS strict Clippy, 14 tests (1 unit + 13 integration), release build and real N-API loader/read/write/inventory/removal smoke; temporary qualification artifact only |
| Provenance/license inventory | PASS: 663 lock entries, 631 package versions, one audited metadata override, 14 commit-pinned source repositories |
| Staged privacy scan | PASS before local commit; pending inherited REVIEW is left untracked and unedited |

The initial unoptimized verification command was:

```sh
bash scripts/run-bluey-tests.sh -- env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 bash -c 'cargo clippy --offline --locked --manifest-path server/Cargo.toml --all-targets --features integration-test-support,business-messaging-simulator-test-support -- -D warnings && cargo test --offline --locked --manifest-path server/Cargo.toml --all-targets --features integration-test-support,business-messaging-simulator-test-support'
```

The successful final rerun uses Cargo test-package optimization level 3 for
`curve25519-dalek`, `ed25519-dalek`, and `sha2`. This changes no assertions,
timeouts, production configuration or release artifact. The launcher still
owns and removes the entire fresh Cargo/test root. Focused boot, leaseable,
packet, ATS-page, migration and source-positive-export tests precede strict
Clippy during diagnostics. The final successful command ran strict Clippy
followed directly by the complete all-target suite in a fresh isolated invocation.
The exact test half of that command was:

```sh
cargo --config 'profile.test.package.curve25519-dalek.opt-level=3' \
  --config 'profile.test.package.ed25519-dalek.opt-level=3' \
  --config 'profile.test.package.sha2.opt-level=3' \
  test --offline --locked --manifest-path server/Cargo.toml --all-targets \
  --features integration-test-support,business-messaging-simulator-test-support
```

All 32 changed code/configuration files were hashed before the successful
run and matched byte-for-byte afterward. The final root
`/private/tmp/bluey-test-runs-501/run.1793045008.18045.29205301450`
was verified absent after exit 0. The launcher self-test also passed, including
its intentional cleanup-refusal fixtures. Native and frontend qualification
roots were independently checked absent.

Initial failed runs are not omitted:

- Offline check/test initially lacked the public locked dependency cache.
  Locked public crates were fetched by normal Cargo execution; no production
  account credentials or external mutations were used.
- The first real boot suite produced 8 passes / 9 failures, all from the
  old manifest capability CHECK before startup. The prospective migration
  correction enabled the subsequent 17/17 pass.
- Initial strict Clippy exposed three test-only findings: Copy clone,
  constant assertion and immediately invoked closure. These were corrected
  without warning suppressions or production behavior changes.
- The first full library run's ten failures are recorded in FIX-790; none is
  omitted from the ledger. A new regression helper initially failed compilation
  by treating a fallible row decoder as a tuple; `.map` now propagates the
  decoder error, and that temporary run was cleaned before restarting.
- Post-review focused boot/auth/migration tests passed 20/20. The true leaseable
  fixture then exposed a second-truncated SQLite clock returning false idle;
  millisecond probe timing corrected it, and the real leaseable/claim rollback
  test passed. Packet-focused 14/14, ATS-page 14/14, failure-injection 2/2 and
  pagination registration 1/1 also passed in the diagnostic continuation.
- The old source-positive export diagnostic was intentionally stopped after a
  read-only one-second macOS process sample showed repeated full snapshot
  publication dominating fixture setup (quadratic work over 102 jobs). No
  export pass is claimed from that stopped run. It exited 143 and its exact
  temporary root was verified absent. The fixture now saves all verified
  imports, publishes one final 102-job snapshot, then installs every real
  source/ATS/integrity authority. No production TTL or final assertion changed.
- The first batched fixture pass correctly rejected a job mismatch: workspace
  ranking is not verifier FIFO order. The fixture now reads the real scheduler
  order and publishes all 102 authorities in that order, preserving the exact
  leased-job comparison rather than accepting an unrelated lease.
- The corrected 102-job export then passed in 60.98 s. A subsequent early
  full-library pass was stopped deliberately to make shared fixture reads
  PostgreSQL-aware (heartbeat refresh and FIFO selection had used SQLite-only
  connections). Its exact root was verified absent; it is not counted as a
  completed all-target pass. Final-source all-target verification is separate.

## Separate static review

A bounded GPT-5.6-Sol read-only pass found genuine operational-generation and
invalid-startup-predecessor blockers. Re-review of the corrected code reports
both resolved and no open blocker in that assigned scope. It also caught and
resolved the PostgreSQL attempt-cascade guard mismatch. This is a separate AI
static review, not human review, runtime PostgreSQL evidence, or full release
approval. The canonical independent REVIEW remains unapproved.

## Environmental evidence and release holds

- PostgreSQL full runtime/concurrency: PENDING. No dedicated non-production
  URL is configured, and neither installed PostgreSQL 14 nor 17 has pgvector.
  The shared default server and production credentials were not used.
- Exact Linux Docker build/native smoke: PENDING. Docker CLI is installed,
  context is local `unix:///var/run/docker.sock`, but that socket is absent
  and no daemon is running. No remote context or new infrastructure was used.
- Darwin native addon: qualification passed as recorded above; Linux exact
  image/addon qualification is still independently pending.
- Exact-tip hosted CI, registry publication/read-back, signing, deployment,
  canaries and production flags: not authorized or attempted.
- REVIEW remains the independent reviewer's responsibility.

## Deviations and remaining risks

- New 065/043, rather than editing 064/042: the latter are already committed
  and could have been applied. Their bytes are preserved.
- The prospective successor also repairs the proven v3 metadata CHECK gap.
- Historical Kiro fake-instance test counts are not runtime-auth proof and
  are not reused as current evidence.
- Boot authority/binding retention is immutable; fleet-scale retention and
  database capacity need release review before enabling production.
- The portal build's large-chunk warning is retained as a separate frontend
  performance follow-up; this boot protocol batch changes no portal component
  or bundle threshold and makes no live latency claim.
- No claim is made about live auto-apply, job-source coverage, browser runners
  or mailbox integrations being launched by this startup-only batch.

## Completion state

The October 27 corrections passed parent-owned full server/Clippy, independent
critical backend execution, and focused client/release/schema gates. Local
correction packaging does not authorize a new roadmap or integration batch.
See HANDOFF for the local commit ledger.
This is not a release approval: PostgreSQL runtime, Linux Docker/native, hosted
exact-tip CI and protected publication/rollout gates remain pending. No push,
PR, merge, rebase, deploy, image
publication, provider access, application or message. The meeting checkout,
desktop overlay and all production flags/accounts remain untouched.
