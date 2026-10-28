# REVIEW: Phase 635 Jobs current Rust qualification

Date: 2026-10-27 (America/New_York)
Base/source under qualification: `863042a02afbdce4a898b8bc77978845612a569f`
Reviewer role: independent read-only procedure and evidence review
Verdict: 🟢 bounded current-source Rust qualification accepted; separate product finding remains

## Scope reviewed

- `docs/rounds/ROUND-635-JOBS-CURRENT-RUST-QUALIFICATION.md`.
- `docs/work/IMPL-PHASE-635-JOBS-CURRENT-RUST-QUALIFICATION.md`.
- The exact 30 configured PostgreSQL selections and Cargo flags recorded in the Phase 629
  implementation evidence.
- `jobs/scripts/run-nonprod-postgres-tests.mjs`, including private cluster creation, exact-case
  discovery, fresh-database allocation, child environment, shutdown and result reporting.
- `scripts/run-bluey-tests.sh` launcher boundary and the current server features/test-profile flags.
- The Phase 629 host-listener history and Phase 634 evidence boundary.

No product, test, manifest or harness source was edited by this reviewer. No Cargo command,
PostgreSQL server, VM, container, provider, production account or external action was started.

## Procedure review

No P0–P3 procedure finding remains in the reviewed frozen plan.

- The PostgreSQL wrapper refuses inherited owner/test URLs, creates its own password-authenticated
  PostgreSQL 17 loopback cluster, verifies the postmaster PID/data directory, address, port and
  pgvector query before testing, and injects the generated URL plus required-test flag only into
  the bounded Cargo child environment.
- Every configured filter is syntax-bounded, discovered through Cargo before execution, required
  to match at least one test and prohibited from overlapping another filter. The resulting exact
  cases are bounded and each executes serially with `--exact`, one test thread and a fresh private
  database plus vector extension. This preserves the global activation/head isolation established
  in Phase 629 rather than falsely combining incompatible fixtures.
- Child exit, signal and spawn-error evidence is checked. PostgreSQL must remain live throughout
  the configured cases, then stop cleanly with code zero, null signal and no `postmaster.pid`.
  Forced shutdown is an explicit qualification failure.
- The subsequent full-Rust stage is procedurally separate from PostgreSQL: it runs only after the
  database shutdown assertion, reuses only the owned compiler/source caches, restores SQLite as
  primary and omits both PostgreSQL URL and require flag. This prevents configured-only cases from
  silently substituting for ordinary SQLite/all-target coverage.
- Server formatting, warnings-denied all-target Clippy and all-target tests use both
  `integration-test-support` and `business-messaging-simulator-test-support`. The full test command
  retains the established test-only opt-level 3 settings for curve25519-dalek, ed25519-dalek and
  sha2; no production optimization or feature flag is changed.
- The complete aggregate runs under the canonical isolated launcher and shared machine lock. Its
  private data/config/runtime/log/temp/SQLite/target roots, signal supervision and exact cleanup
  are required evidence, not inferred from Cargo output alone.

The source freeze is material: `863042a02afbdce4a898b8bc77978845612a569f` was the current HEAD
at review time, and no source, test or manifest edit may be admitted between command start and
terminal evidence. Any fix requires a recorded failure and fresh relevant execution.

## Host-listener evidence boundary

Phase 629 observed that a Colima-generated VM could create a host DNS listener despite
`port-forwarder=none`. Phase 634 validly proved its exact container's network-none/read-only/native
smoke and VM cleanup, but it did not inventory host listeners while that VM was alive. Phase 635
does not start a VM and cannot close that gap. The Rust qualification must not be described as VM
network-isolation evidence. Official-source review shows VZ management needs a loopback SSH TCP
listener even with vsock, so literal zero listeners is not a valid acceptance criterion. Any future
VM reuse must reject public, DNS and application listeners, explicitly inventory the permitted
loopback management listener and every owned host-agent/driver/usernet PID, then prove those PIDs
and listeners absent before deleting private state.

## Open P1 product-invariant finding: canonical employer alias uniqueness

The attempt-reservation table and active-company unique index bind `(account_id, company_key)`,
where `company_key` is derived from the posting's normalized display company. Eligibility,
packet-commit lookup and local/cloud staging all compare that display key. The posting can already
carry a signed canonical employer identity, but it is not persisted in this reservation authority.
Two simultaneously verified postings for the same signed employer can therefore evade the
one-active-application-per-company invariant when their display aliases normalize differently.

Evidence: `server/src/db/mod.rs:1342-1356`, `server/src/db/jobs/eligibility.rs:1351-1361`,
`server/src/db/jobs/applications.rs:5359-5373,5404-5415`, and
`server/src/db/jobs/workflow_commands.rs:1256-1283,1338-1356`.

This did not make the qualification run false-green: the run accurately tested the frozen current
source. It remains a release blocker for enabling irreversible employer submission. A successor
needs a canonical employer binding and active uniqueness constraint in both databases, conservative
legacy handling, and packet/pre-click revalidation without removing the existing display-name
guard. The regression must use two concurrently Verified postings with distinct display keys but
the same signed canonical employer identity and prove the second reservation is rejected without
mutating either authority.

## Final parent-reported executable evidence

The following runtime evidence was supplied by the parent and was not independently rerun by this
reviewer:

- Aggregate session 61600 exited zero with unchanged source
  `863042a02afbdce4a898b8bc77978845612a569f`; the controller's final event reported configured
  PostgreSQL, formatting, strict all-target Clippy and full all-target tests all true.
- The exact 30 configured PostgreSQL cases passed in 30 fresh databases. The wrapper reported
  status zero, shutdown attempted and server exited; the controller observed the postmaster PID
  file absent before starting the SQLite/full-Rust stage.
- Formatting and warnings-denied all-target Clippy passed sequentially with both support features.
  All-target tests passed 1,845 with zero failed or ignored: library 1,705; server binary 1;
  integration 134; guarded example 5; Jobs API binary 0. The library took 2,471.54 seconds and the
  113-case HTTP integration target took 539.80 seconds. Configured-only SQLite early returns are
  not counted as additional PostgreSQL proof.
- The exact canonical root was absent and its parent empty. Observed controller/Cargo/library PIDs
  30310, 39087 and 39264 were absent and no process command matched the owned root. Host free space
  was approximately 38 GiB after cleanup; this is not a peak, leak, capacity or SLO measurement.
- Fresh launcher self-test session 84754 exited zero. Its deliberate active-process refusal/status
  39 is expected negative coverage, not a failed cleanup. Its parent was absent afterward.
- Privacy session 40897 exited zero over 2,854 indexed paths and 2,578 text files; the cached diff
  contained only the five Phase 635 documentation files at that checkpoint. Final restaging must
  include this review update and rerun the indexed privacy/diff checks before commit.

## Unverified gates and limits

- This is local native macOS/ARM64 source qualification, not a Linux server/release-profile build,
  protected release capsule, hosted CI or signed/published artifact.
- Phase 635 did not start a VM, browser, runner, provider or hosted PostgreSQL service and does not
  close the host-listener gap above.
- No production/provider/account flag, application, email, calendar, desktop/overlay or external
  side effect was exercised. The result is not capacity, latency, cost, durability or SLO proof.

## Current verdict and limits

🟢 **BOUNDED CURRENT-SOURCE RUST QUALIFICATION ACCEPTED** — the frozen source's exact configured
PostgreSQL selection, formatting, strict all-target Clippy, full all-target tests and cleanup gates
passed with the parent-reported evidence above. No P0–P3 defect was found in the qualification
procedure or evidence. The separate P1 canonical-employer uniqueness finding remains unresolved,
so this is not overall Jobs readiness, production or release approval. It does not authorize or
prove deployment, registry/signing, provider behavior, VM listener isolation, hosted services,
real application/email/calendar effects, desktop/overlay behavior, capacity, latency or SLO
readiness.
