# REVIEW: Phase640 — Read-only Jobs provider-exposure evidence

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the
> specified branch, Round640, corrected Phase638 cost acceptance, and the final
> source freeze.

**Commit range:** `32ca96dbfbe151b1ca0294ad945ce58a4276087f..working tree`
**Reviewer:** independent source/security review (`jobs_cost_final_review`)
**Date:** 2026-11-01

## Per-Task Review

### D1/D2 — Private report, stored-receipt adapter, and fixtures

| Field | Value |
|-------|-------|
| Files | `server/src/db/jobs_provider_cost_holds.rs`; `server/src/db/jobs/customer_data.rs`; `server/src/db/jobs/tests.rs` |
| Verdict | 🟢 bounded test-only prototype qualified; no remaining P0-P3 source or runtime finding |

**Resolved findings:**

1. **Original P1 resolved — relational resume authority is exact.** The SQLite
   and PostgreSQL receipt queries now select `jobs_applications.resume_version_id`
   and `stored_report_application` requires a nonempty exact match with the
   decrypted application (`customer_data.rs:2298-2318`, `2360-2393`,
   `2504-2537`). Shared negatives set that relational binding to `NULL` and to a
   different existing same-account resume, require `invalid_application`, prove
   the report does not repair the row, restore the exact binding, and then
   require validation (`customer_data.rs:3116-3213`, invoked for both backends
   at `3466`).

2. **Original P2 resolved in source — byte rejection precedes owned report
   copies.** `bounded_report_projection` computes UTF-8 byte lengths in SQL,
   returns `NULL` text sentinels for an individually oversized row, and returns
   a conservative six-times-JSON charge (`customer_data.rs:2139-2177`). Both
   report backends stream rows, charge the cumulative budget before extracting
   text, and keep strict row sentinels (`jobs_provider_cost_holds.rs:1128-1437`).
   Application/evidence/object validation uses the same boundary
   (`customer_data.rs:2350-2628`), and final report serialization writes through
   a budgeted sink rather than allocating a complete JSON copy
   (`customer_data.rs:2187-2206`; `jobs_provider_cost_holds.rs:1660-1662`). The
   multibyte/sentinel negatives cover every mutable text family on SQLite and in
   the shared PostgreSQL case (`customer_data.rs:3215-3397`, `3467`); the
   many-individually-valid cumulative exhaustion negative is explicit on SQLite
   (`jobs_provider_cost_holds.rs:2154-2202`).

3. **Static reachability correction resolved in source.** The report and adapter
   initially remained `pub(crate)` under `integration-test-support` even though
   every consumer is an inline unit test, creating a probable dead-code failure
   in the required non-test feature build. The final freeze narrows the complete
   new graph to `#[cfg(test)]`; it does not suppress lint or create a public
   report surface. The existing physical-PostgreSQL wrapper can still execute
   the inline unit case.

## Cross-Task Findings

- No public route, schema, dependency, billing, admission, settlement,
  production flag, provider call, or employer action is introduced. Tenant
  predicates and the same-snapshot design remain intact: one SQLite transaction
  and PostgreSQL repeatable-read/read-only.
- Each hold contributes only its hold amount once. Usage only annotates; duplicate
  linkage hard-fails; missing/mismatched linkage stays explicit. Released rows
  must be zero-settlement, missing-provenance and event-free before exclusion.
- The report truthfully keeps original request-content recomputation, remote
  object bytes, employer delivery, historical completeness, and per-success
  economics unavailable. Stored metadata consistency is not verified employer
  success or invoice reconciliation.
- The fixture's sequential empty-family reads are useful fresh-test preconditions
  but are not a transaction/time-bound durable observation watermark. The report
  correctly leaves `completeness = "unverifiable"`. No executable observer or
  owner-database isolation driver was added; exercise remains restricted to the
  separately proven fresh private wrapper.

## Final Source Freeze

- `jobs_provider_cost_holds.rs`:
  `3f3ad4172e9f75105367003037779867011d1cab200ab717d38edc2a01028402`
- `jobs/customer_data.rs`:
  `ea7591cbc98aa3c07aaac1857656356722a0213ede4463df62ba2abe0597e07f`
- `jobs/tests.rs`:
  `7e746b3a219b328508bb9d43cc970ad6defcd3b1a9af10d15ea20fda3931f67d`

The completed runtime controller bound those module hashes to HEAD `32ca96db`,
branch `feat/phase-640-jobs-cost-evidence`, 778 files, manifest
`91ef6fae3f3e49c8a98c3566076abcbc9c8aa84069d02f036847069f3a06cdc0`,
status SHA-256
`a00846d9519ee35d4268e4ca99098a8fda19db6025cfab6747a69561fc010d8c`,
and an empty real index before, inside, and after the run.

## Build & Test Verification

This independent reviewer did not dispatch compilation, tests, databases,
services, VMs, or providers. I independently reconciled the completed parent-run
Rust controller journal and its bounded raw siblings. The immutable journal is:

`/Users/uno/.codex/visualizations/2026/08/04/019fcdb9-9999-70c3-a063-c83542b6b5f0/phase640-rust-f1e300ef2190.jsonl`

Its independently verified SHA-256 is
`a996e087c253d653dc54c7b449a571b5ea3a720c950f9519ef9b7ce93e8842d3`.
It contains 18 ordered events in 8,037 bytes and terminates with
`controller_finished.success=true`. Its mode-`0600` raw siblings are 217,293
and 45,713 bytes. Their independently reproduced SHA-256 values exactly match
the journal's terminal stdout/stderr hashes:
`37cbd5731878cab6298ae1349b3f5d306726e133e36ec9c3d6063f2a28609bf1`
and
`54dce9f0e886f7f26dda2cd06211191d62431b549d122b7337f704cb3c55deef`.

The resulting bounded evidence is:

```text
exact three-file rustfmt       # pass
server cargo fmt --all --check # pass
strict all-target Clippy       # pass
focused SQLite cases           # 8 financial + 8 receipt; all pass
required-fresh physical PG     # 1/1 pass; zero fail/ignore/skip
full all-target server suite   # 1,933 pass; zero fail/ignore
terminal cleanup/source proof  # pass
observer/isolation driver      # NOT IMPLEMENTED
current managed-cloud release gate child # historical 37/37 narrow pass below
```

The focused discovery stages found exactly eight financial and eight receipt
cases. Both focused SQLite stages passed 8/8. The controller then observed a
live owned PostgreSQL 17 process at PID 6457 and private port 53419, ran the
required report case 1/1 with no skip, and proved the PID and postmaster file
absent before starting the no-PG full suite. Thus the environment-conditional
case was exercised physically rather than credited through its no-URL path.

The full all-target vector was
`[1784, 0, 1, 14, 1, 1, 2, 113, 2, 1, 5, 9]`, totaling 1,933 with zero failures
and zero ignored tests. It contained all sixteen new SQLite names; the
conditional PostgreSQL name in this no-PG stage was explicitly not credited as
the separately physical pass. The launcher ended code zero with no signal,
interruption, protocol failure, or spawn failure. The exact owned root, run
parent, owned PIDs, and related processes were absent; the source manifest,
status, module hashes, and empty index were unchanged.

The completed controller passed its unchanged 32-GiB admission checks. The old
approximately-30-GiB capacity hold describes an earlier checkpoint and is not a
current gate result. It remains correct that future heavy runs must pass their
own capacity checks rather than weakening the guard.

This qualifies the bounded, test-only read-only prototype against SQLite and
one actual private PostgreSQL lifecycle. The fixtures still use synthetic
object metadata and normal database finalization, not remote object bytes, a
managed runner, provider or employer effects, or employer delivery. The
sequential empty-family fixture checks are not a durable observation watermark;
historical completeness remains unverifiable.

## Lightweight release-gate evidence reconciliation

The parent dispatched one actual lightweight run through the canonical launcher
and shared `mac-build` scheduler. The preserved, content-addressed five-line
journal is:

`/Users/uno/.codex/visualizations/2026/08/04/019fcdb9-9999-70c3-a063-c83542b6b5f0/phase640-release-guards-ab08d14a8b11.jsonl`

Its independently verified SHA-256 is
`fc461a952da06100200ee44bd7640f547c492f1a5e816647715ba12a1ef954c2`.
The source-before/source-after records are equal, and an independent replay of
the controller's read-only snapshot calculation reproduced HEAD `32ca96db`,
branch `feat/phase-640-jobs-cost-evidence`, 778 files, manifest
`5d0a37d8afac19b0cec963f18aa93627d62e51446b0da9512b78d169e159bde1`,
and status hash
`a00846d9519ee35d4268e4ca99098a8fda19db6025cfab6747a69561fc010d8c`.
The command child was terminal zero with `37 tests / 37 pass / 0` for every
failure, cancellation, skip, and todo category; cleanup removed the owned test
parent and the scoped source remained unchanged.

The one-off controller at
`/private/tmp/bluey-phase639-controller-OR0wQV/phase640-release-guards.mjs`
(independently observed SHA-256
`b25a86432b794e311d661578fa14c2e6e4b29e23a83b8653057aa0c8d9e9578b`)
incorrectly required 40 tests. Therefore it correctly recorded
`controller_finished.success=false` and exited the outer shell nonzero even
though its launcher child succeeded. Preserve that failure; do not rewrite the
journal or describe the controller as green.

The 40 expectation came from the separately held, uncommitted Phase639 D1 file,
not this source. The current gate file is byte-identical to HEAD (Git blob
`886c06a8892a573f4f918e84317e0b2f83da6590`, SHA-256
`e9e326068d7ca61040a94ec608e3d2bca31d614c0c29787dd9005ad71f43da32`)
and has exactly 37 top-level registrations. The held file is `+68/-5`; it still
has 37 static `test(` sites but adds a loop that registers three PAX negatives
at runtime, producing 40. Those three tests and the held OCI assertions were not
in the 778-file Phase640 manifest, so the observed 37 is the complete current
suite, not a lost current-source test.

No rerun of the unchanged 37-test source is warranted merely to manufacture a
green outer status. Accept this only as a **narrow child-pass / harness-count
failure**: the exact current managed-cloud release-gate suite passed and cleanup
and scoped source invariance were observed; the one-off controller failed its
stale count assertion. If the held Phase639 changes are later admitted, they
require their own exact-source 40-test run. Future controllers should bind the
expected count to the asserted source manifest (or record an explicit approved
count map), record their own hash in the journal, and retain bounded raw output
or another independently verifiable summary artifact. This journal contains
only output hashes and a parsed summary, so it does not independently preserve
raw TAP or scheduler/lock text.

A non-executed one-off successor now exists at
`/private/tmp/bluey-phase639-controller-OR0wQV/phase640-release-guards-v2.mjs`.
Its independently verified SHA-256 is
`ec1c5019fae9a67620f6f7be178af07b637f8721662f45fbb21ad52d76c3b4ab`;
the parent reports `node --check` passed. Static comparison against the preserved
predecessor shows only the intended scope correction: a distinct `v2` journal
name, exact pre-run assertions for the current test-file SHA-256
`e9e326068d7ca61040a94ec608e3d2bca31d614c0c29787dd9005ad71f43da32`
and 37 static registrations, and success expectations changed from 40 to 37.
Those assertions run before journal creation or child dispatch. No Rust or
product source changed, and the predecessor controller and journal hashes remain
unchanged.

The successor has not executed and supplies no new test evidence; it merely
corrects the identified harness-count bug for a possible future run. Do not use
its existence or syntax check to replace, overwrite, or green the predecessor
failure. It also retains the predecessor's evidence limitations: the controller
SHA is not self-bound into a journal, and bounded raw TAP plus scheduler/lock
text would not be preserved. No rerun is presently needed for the unchanged
37-test source.

## Rust-gate controller review and execution

I independently read the complete 601-line private controller
`/private/tmp/bluey-phase639-controller-OR0wQV/phase640-rust-gates.mjs` at
SHA-256
`788e0b1401b7b6dfaf0220ad5e39db4b863ed27917f8bc66d9335219e80a9964`
against `COMMAND-PHASE-640-VERIFICATION.md` and the full Phase638 controller
pattern. This was source review only; the controller, Cargo, PostgreSQL, tests,
databases, services, and VMs were not executed by this reviewer.

FIX862's executed successor is
`/private/tmp/bluey-phase639-controller-OR0wQV/phase640-rust-gates-pg-index-fix.mjs`
at SHA-256
`8f006056e40080a17399cfde5fc63964539fdb8f90e7af019cb543027a3868b3`.
As recorded in the FIX862 review, it changes only its self path, the exact
manifest pin, and the corrected `customer_data.rs` pin. All guards and stage
logic reviewed below are unchanged.

No reproducible P0-P3 controller finding remains in the frozen source. The
controller pins the scheduler source and live direct-parent command, the exact
HEAD/branch/778-file manifest/status/three Rust-module hashes, and its own hash.
It requires 32 GiB before outer admission, inner admission, and every one of the
nine commands. Cargo runs with a private home/target/data environment, fixed
read-only owner registry/git cache links, offline/locked dependencies, no
inherited PostgreSQL/provider/auth variables, and fail-closed ancestor Cargo
configuration checks.

The evidence logic rejects the principal false-pass paths:

- discovery must exactly equal the eight financial and eight receipt names
  extracted from the frozen source, and each focused SQLite execution must
  report exactly eight passes plus every required name exactly once;
- the PostgreSQL stage must expose a live PG17 PID and exact owned data root,
  emit the exact five-event/one-case vector for the required case, contain no
  `SKIP`, report one pass, stop cleanly, and leave both PID and PID file absent
  before the no-PG full suite starts;
- the full all-target run must expose at least eleven summary rows, zero failed
  and ignored tests, a total of at least 1,933, and all sixteen SQLite names plus
  the conditional PostgreSQL name. The conditional no-PG appearance is
  explicitly not credited as the physical PostgreSQL pass;
- stage order, exit/signal state, stdout/stderr hashes and caps, source equality,
  terminal state, exact owned-root cleanup, launcher/inner/PG PID absence, and
  related-process absence are all acceptance conditions.

The external journal and raw streams are new exclusive mode-`0600`, identity-
revalidated, fsynced files capped at 32 KiB and 16 MiB per raw stream. Journal
events and fields are whitelisted and bounded; the controller retains no
database URL, environment dump, customer row, or report payload. Its final
success still requires the controller process to terminate zero after journal/
raw-file close, canonical launcher cleanup, and final source/self-hash checks;
the journal must not be accepted in isolation.

The successor completed exactly once with terminal zero and
`controller_finished.success=true`. Its strict Clippy, focused SQLite, actual
private PostgreSQL, full all-target, terminal cleanup, and final source-readback
conditions all passed as detailed in Build & Test Verification. This converts
the earlier prepare-only admission into bounded runtime evidence for the
test-only report graph.

This evidence is still not the held Phase639 image/PAX work, a real OCI image,
remote object readback, managed-runner execution, provider/employer proof,
invoice COGS, historical completeness, or release approval. The journal
correctly records `releaseVerified=false`,
`historicalCompletenessVerified=false`, and `invoiceCogsVerified=false`.

## Overall Verdict

🟢 **BOUNDED TEST-ONLY PROTOTYPE QUALIFIED.** No reproducible P0-P3 source
or bounded-runtime defect remains at manifest `91ef6f...`. Formatting, strict
all-target Clippy, all sixteen focused SQLite cases, the required physical
PostgreSQL case, the 1,933-case all-target suite, terminal cleanup, and source
invariance passed. This is not merge acceptance, release approval, production
evidence, invoice or full COGS qualification, historical completeness, or
overall Jobs-goal completion.

## Follow-ups

- Preserve the current honest limitations: no historical-completeness claim,
  verified employer-success denominator, invoice COGS, or cost advantage.
- Full COGS, runner/runtime cost, remote object readback, employer delivery, and
  the broader Jobs launch gates remain outside this prototype.
