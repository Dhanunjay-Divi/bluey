# Jobs-only Phase 630 local handoff

Date: 2026-10-27. Bounded local qualification complete, not production release approval.

## Authority and boundaries

- Checkout: `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`
- Branch: `feat/phase-630-jobs-runtime-qualification`
- Base: `edeb6a767c357909953d1e18261265f546ec6908`
- Local fixes: `4e056453` (pure workflow imports), `3aaf92e2` (exact visibility
  root hydration). The qualification successor is identified by commit subject
  `test(jobs): record real workflow and browser qualification`; resolve its hash
  from this branch's log rather than treating this document as self-attestation.
- Current source and local handoffs are authority. SSD is only a specific
  missing historical-detail fallback; none was used.
- Load `bluey-ops`, checkout AGENTS.md, Round 630, IMPL/REVIEW 630 and FIX-802/803.
- Preserve the inherited untracked canonical Phase 628 REVIEW unchanged.
  Never work on the meeting-owned `/Users/uno/Downloads/cue` checkout or change
  the desktop host/overlay/Pinky for this Jobs batch.

## What changed

Two real service defects were exposed beyond mocked tests:

1. The Temporal workflow imported the broad automation barrel, pulling forbidden
   Node/network/PDF/browser modules into its deterministic bundle. Export/import
   the existing pure checksum module directly; checksum code is unchanged.
   A real SDK bundle test now covers both workflow protocols.
2. Actual visibility rows omit `firstRunId` while exact Describe supplies it.
   Hydrate only missing values through an exact workflow/run lookup, in batches
   of eight under the existing deadline. Never replace a nonempty identity or
   borrow a caller/current-run root. Status/time/memo and all destructive/absence
   fences remain unchanged. Real protobuf, concurrency and negative tests cover it.

New nonproduction harnesses bind fresh current source, private credentials,
owned services, actual process completion and root cleanup. Browser authentication
is a fixture JWT, not a real login or OAuth claim. The shared API binary serves
only the existing account endpoint inside this fixture, not a desktop process.

## Actual evidence

| Gate | Result |
|---|---|
| Real local Temporal 1.32.0 / SDK 1.20.2 | 10/10 passed through real gateway HTTP, workflow, persistent restart, Update/replay, privacy and cleanup |
| Exact cleanup | Wrong digest denied; raw visibility root empty; actual Describe/history/visibility absence after 35.104 s |
| Synthetic latency baseline | 20 warm-worker fresh starts/results, 0 errors; p50 148.15 ms / p95 153.19 ms / max 153.88 ms; not production latency |
| Real browser/API/PG | 1/1 story passed: manual profile save, reload, actual persisted name/empty matches; 7 Jobs + 2 account requests, no page errors/HTTP 5xx |
| Jobs full quality | 2,039 tests (804/219/308/321/387), all five typechecks/builds, installed audit zero findings |
| Extra gates | Report boundary 2/2; real Chromium exact-submit fixture passed; release 32/32; parity 113 tables / 91 indexes; CI guard self-tests passed |
| Cleanup | Two Temporal and two API processes had code 0 / signal null and dead PIDs; private PG stopped; all 13 task scratch parents empty |

The earlier 1,838 full Rust tests and strict Clippy remain exact unchanged-source
baseline evidence: no Rust, SQL, native, host/overlay, root web or workflow CI
configuration changed here. Rust binaries were freshly built and executed for
the real browser/PG test; the full Rust suite was not unnecessarily repeated.

The CLI archive was pinned/checksummed (not globally installed). Test keys,
Temporal data, PostgreSQL databases and Chromium profiles were disposable and
removed. Failed attempts and corrected test-fixture assumptions remain in IMPL.
No preview server remains running after this qualification.

## Reproduce safely

Every heavy command must use the existing shared `mac-build` lock and canonical
`scripts/run-bluey-tests.sh`; never run against owner/default/prod URLs.
The scheduler lives in `/Users/uno/Projects/shared-runners/`.

Temporal (the wrapper admits only its pinned Darwin arm64 CLI):

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
  with-machine-lock --machine mac-build -- \
  scripts/run-bluey-tests.sh -- node jobs/scripts/run-nonprod-temporal-tests.mjs
```

Actual private PostgreSQL / browser / API:

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
  with-machine-lock --machine mac-build -- \
  scripts/run-bluey-tests.sh -- node jobs/scripts/run-nonprod-postgres-tests.mjs \
  --pg-bin /opt/homebrew/opt/postgresql@17/bin -- bash -c 'set -euo pipefail
cargo build --offline --locked --manifest-path server/Cargo.toml --bins
node jobs/scripts/run-nonprod-browser-api-tests.mjs'
```

The PG harness generates its own URL inside the isolated boundary. No account,
mailbox, provider, employer or production credential is required.

## Required next authority / release evidence

All changes remain local. No push/PR, merge, deployment, registry write,
production signing-key creation, production flag change, real application, email, calendar
action or paid infrastructure occurred. The owner was asked for explicit
permission to push only the Jobs branch and run CI; no answer is inferred.

Remaining: exact-tip hosted CI, final-source Linux artifacts, approved signing
and registry read-back, hosted PostgreSQL/Temporal/TLS and real runner capacity,
dedicated nonproduction object storage/provider/OAuth/ATS canaries, plus rollout,
in-flight rollback and kill-switch rehearsal. The Phase 629 emulated Linux image
is historical smoke evidence, not an artifact for these changed workflow bytes.
Do not promote a debug binary, fixture key, test authority or local dev service.
