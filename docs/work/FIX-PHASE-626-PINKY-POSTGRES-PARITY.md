# FIX-PHASE-626: Validate Pinky Store Semantics on PostgreSQL

> **Codex preflight:** `$bluey-ops` and the Pinky/Bluey integration runbook
> were loaded before implementation. This gate remains local-only and uses no
> existing database or provider credentials.

## Issue

The Phase 626 Pinky integration had comprehensive SQLite coverage, while its
PostgreSQL branches had only source review. The advisory-lock behavior for exact
request claims and concurrent Stop/Ask ordering therefore lacked execution
evidence against a real PostgreSQL engine.

## Root Cause

The Pinky store tests always created a SQLite pool. Existing opt-in PostgreSQL
tests elsewhere in Bluey apply the full server migration stack, including Jobs,
which is broader than this focused integration gate needs.

## Fix Summary

An env-gated store test now opens a real `DbPool::Postgres`, runs the production
Pinky schema initializer, and calls the real lifecycle, admission, claim,
cancellation, status, and delivery-fence functions. It covers:

- entitled session open, exact replay, access, and close;
- two simultaneous exact running claims with exactly one winner;
- idempotent Stop-before-Ask tombstones and rejected late admission; and
- repeated simultaneous cancel/admit races that always leave delivery fenced.

Each scenario uses a separate random Pinky subject and therefore a separate
ephemeral external-only account.

The local-only harness creates a private PostgreSQL 17.10 cluster with no TCP
listener, applies only the official `001_server_runtime_compat.sql` and
`002_usage_reservations.sql` migrations, and lets `store::initialize` create the
Pinky tables. Migration 001 supplies accounts, synthetic credit batches, and
the balance ledger. Migration 002 is required because the real delivery fence
checks managed usage reservations.

The harness strips inherited database/provider environment, uses a private
Unix socket, and owns its PostgreSQL data, Cargo target, application data,
temporary files, database, runtime, and log directories. Cleanup stops the
server and removes the recognized root on success, failure, or signals.
If both fast and immediate shutdown cannot be confirmed, cleanup fails closed
and retains the owned root instead of deleting files beneath a live server.
After the exact PostgreSQL test, the same queued harness and owned Cargo target
run all Pinky integration unit tests, all-target tests, and strict all-target
Clippy. Those broader gates omit the ephemeral marker and database URL, so the
PostgreSQL test skips instead of executing again and no database or provider
configuration leaks in.

## Files Modified

| File | Change |
|------|--------|
| `server/src/pinky_integration/store.rs` | Add the opt-in real-PostgreSQL store regression. |
| `scripts/run-pinky-integration-postgres-tests.sh` | Add the private local PostgreSQL 17.10 harness. |
| `docs/work/FIX-PHASE-626-PINKY-POSTGRES-PARITY.md` | Record scope, isolation, and the pending execution receipt. |

## Edge Cases Handled

- The test skips unless both the explicit ephemeral flag and database URL are
  present.
- PostgreSQL tools are addressed by their versioned Homebrew path so the older
  default `PATH` installation cannot be selected accidentally.
- Host authentication is rejected and `listen_addresses` is empty; local trust
  applies only inside the mode-0700 socket directory.
- Fixed advisory-lock identities are isolated inside a new temporary database.
- Test rows cascade from their independent external-only accounts; the entire
  cluster is deleted after the gate.
- Failure to confirm PostgreSQL shutdown is an explicit gate failure and never
  triggers unsafe data-directory removal.

## How to Test

```bash
bash -n scripts/run-pinky-integration-postgres-tests.sh
scripts/run-pinky-integration-postgres-tests.sh --self-test

# After the current Linux and Pinky queues are released:
scripts/run-pinky-integration-postgres-tests.sh
```

The final command self-acquires the shared `mac-heavy` lock before initializing
PostgreSQL or Cargo. A passing receipt must identify PostgreSQL 17.10, the exact
store test, the focused Pinky tests, all-target tests, strict all-target Clippy,
and successful cleanup with no owned root remaining.

## Known Limitations

- This receipt validates the final frozen source after the independently owned
  model-routing, pricing, and delegated presentation repairs.
- This focused gate validates the Pinky store. Managed provider accounting has
  separate PostgreSQL coverage and is not duplicated here.

## Execution Receipt

The first released harness attempt exited before schema creation or Cargo. Its
owned root inherited macOS's long `/var/folders/...` temporary path, making the
PostgreSQL Unix socket pathname exceed the platform's 103-byte limit. PostgreSQL
shut down, the guarded root was removed, and a process-residue check was clean.
The harness now uses a mode-0700 `/tmp/bluey-pinky-postgres-tests.*` root so the
private socket stays below that operating-system limit. Real test evidence is
still pending the corrected rerun.

The corrected-path rerun started PostgreSQL 17.10, applied migrations 001/002,
and reached the Bluey test-binary compilation. It then exited `101` before any
test ran because concurrent routing work had three stale two-argument calls to
the newly three-argument `openai_compatible_thinking_for` helper. That code is
owned by the routing worker and is outside this PostgreSQL change. The harness
fast-stopped PostgreSQL, removed `/tmp/bluey-pinky-postgres-tests.vZwYra`, and a
process-residue check was clean. Real parity evidence remains pending a rerun
after the routing compile repair.

A second corrected-path rerun started PostgreSQL 17.10, applied migrations
001/002, and again reached the final Bluey test-binary compilation. It exited
`101` before any test ran because `server/src/routing/dispatcher.rs:4028`
references `DEEPSEEK_FAST_MODEL`, while the defined constant is
`DEEPSEEK_FLASH_MODEL`. This routing source is independently owned and was not
changed by the PostgreSQL work. The harness fast-stopped PostgreSQL, removed
`/tmp/bluey-pinky-postgres-tests.VBkQHj`, and left no owned database root. The
exact PostgreSQL test, focused Pinky tests, all-target tests, and strict
all-target Clippy remain pending after that compile repair.

The next released rerun used PostgreSQL 17.10 and the private root
`/tmp/bluey-pinky-postgres-tests.M6Di4e`. The exact real-PostgreSQL store test
passed (`1 passed`, `842 filtered out`), exercising lifecycle, the single
running claimant, Stop-before-Ask, and concurrent cancel/admit through the
production store. The focused Pinky suite then passed (`35 passed`, `808`
filtered out).

The added all-target gate passed all `843` library tests, the ConnectInfo test
(`1`), context migration test (`1`), and GDPR cleanup tests (`2`). Its
`integration_e2e` binary passed `76` of `78` tests and failed two routing tests:

- `router_complete_reports_upstream_error_after_capacity_skip` expected two
  upstream requests but observed a third `gpt-6-sol` fallback; and
- `router_complete_stream_openai_error_frame_is_retryable` expected `502` but
  observed `429`.

This run predates the separately required default-off model route and prompt
repairs. The fail-fast harness exited `101`, so strict all-target Clippy and
later test binaries did not run. PostgreSQL fast-stopped, the owned root was
removed, and the owned PostgreSQL PIDs were absent after cleanup. PostgreSQL
parity and focused Pinky evidence are passing; current-source all-target tests
and Clippy remain a follow-up gate after the independent repairs.

The final frozen-source rerun used PostgreSQL 17.10 and private root
`/tmp/bluey-pinky-postgres-tests.cuLcPb`. It exited `0` with every gate passing:

- exact real-PostgreSQL Pinky store transaction test: `1 passed`, `847`
  filtered out;
- focused Pinky integration suite: `35 passed`, `813` filtered out;
- all-target tests: `932 passed` total, comprising `848` library tests,
  ConnectInfo `1`, context migration `1`, GDPR cleanup `2`, integration E2E
  `78`, Jobs plan matrix `1`, and usage-reservation schema `1`; and
- strict all-target Clippy with `-D warnings`: passed.

The two routing tests that failed in the pre-repair run both passed in this
final run. PostgreSQL fast-stopped, the owned root was removed, and no owned
PostgreSQL process remained after cleanup.

After the delegated Short policy was tightened, the complete harness was run
again against that exact source. PostgreSQL 17.10 used private root
`/tmp/bluey-pinky-postgres-tests.cbdTN2`, and the harness exited `0`:

- exact real-PostgreSQL store test: `1 passed`, `847` filtered out;
- focused Pinky tests: `35 passed`, `813` filtered out;
- all-target tests: `932 passed` with the same `848 + 1 + 1 + 2 + 78 + 1 + 1`
  breakdown; and
- strict all-target Clippy with `-D warnings`: passed.

The source tests verify the revised Short policy construction and token budget;
they do not prove a hard provider word cap. A new live Haiku 5.5 trial remains
the separate behavioral gate. PostgreSQL fast-stopped, the owned root was
removed, and no owned process remained after cleanup.
