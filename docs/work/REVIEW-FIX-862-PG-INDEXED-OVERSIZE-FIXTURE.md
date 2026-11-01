# REVIEW FIX862 — PostgreSQL indexed-oversize fixture correction

> **Codex preflight:** `$bluey-ops` was loaded and reconciled against the
> Phase640 branch, Round640, the existing cost-evidence review, the actual
> failed PostgreSQL evidence, and the corrected frozen source.

**Commit range:** `32ca96dbfbe151b1ca0294ad945ce58a4276087f..working tree`
**Reviewer:** independent source/security review (`jobs_cost_final_review`)
**Date:** 2026-11-01
**Scope:** test-fixture correction and successor-controller pin review only; no
product-source edit, test/build/database/VM/service/provider execution, or
commit by this reviewer

## Frozen source

| File | Reviewed SHA-256 |
|---|---|
| `server/src/db/jobs/customer_data.rs` | `ea7591cbc98aa3c07aaac1857656356722a0213ede4463df62ba2abe0597e07f` |
| `server/src/db/jobs_provider_cost_holds.rs` | `3f3ad4172e9f75105367003037779867011d1cab200ab717d38edc2a01028402` |
| `server/src/db/jobs/tests.rs` | `7e746b3a219b328508bb9d43cc970ad6defcd3b1a9af10d15ea20fda3931f67d` |

The supplied hashes matched the reviewed files. Only the inline
`#[cfg(test)]` PostgreSQL oversize fixture changed for FIX862. Report,
settlement, billing, schema, public API, and production logic are unchanged.

## Finding review

### PostgreSQL rejection is exact and cannot be mistaken for another failure

**Verdict:** 🟢 source accept; no P0-P3 finding.

The corrected PostgreSQL branch still attempts the real update against
`usage_events.request_id`, but now requires all of the following before treating
the result as the expected schema limit (`customer_data.rs:3304-3336`):

- `execute()` must return an error;
- the error must be a PostgreSQL database error;
- its SQLSTATE must be `PROGRAM_LIMIT_EXCEEDED` (`54000`); and
- its message must begin with `index row requires `.

This does not swallow connection, permission, timeout, syntax, or unrelated
database failures. PostgreSQL documents `54000` as `program_limit_exceeded`,
and its index-tuple construction raises that code with the asserted message
prefix when the tuple cannot fit the index-size field:

- https://www.postgresql.org/docs/current/errcodes-appendix.html
- https://github.com/postgres/postgres/blob/master/src/backend/access/common/indextuple.c#L188-L196

The fixture uses a plain pooled `postgres::Client`, not an explicit transaction,
so the failed statement is an atomic autocommit statement rather than a
transaction-aborting substep. It immediately proves the complete authority and
financial snapshot is unchanged, then successfully uses the same client for
the derived query and later catalog query. This checks both no stored mutation
and continued connection usability.

### Actual index authority is preserved

**Verdict:** 🟢 source accept; no P0-P3 finding.

The queried names match the actual PostgreSQL runtime schema:

- `idx_usage_events_request` indexes `usage_events(request_id)`; and
- unique `idx_usage_events_dedupe` indexes
  `usage_events(account_id, request_id, kind)`.

Those definitions are in
`infra/postgres/server-runtime/001_server_runtime_compat.sql:156-161`, which is
the first migration included by `server/src/db/mod.rs:2963,3100`. The fixture's
catalog read is bound to the active `usage_events` relation by `indrelid`,
requires exactly those two names, captures `pg_get_indexdef`, `indisvalid`, and
`indisready`, and requires both flags true. It compares the full captured rows
after the rejected update and synthetic projection, so neither definition nor
valid/ready state may change (`customer_data.rs:3305-3321,3349`). No index is
dropped, disabled, rebuilt, or bypassed to make the oversized stored value fit.

### Projection coverage is intentionally synthetic and separately counted

**Verdict:** 🟢 source accept; no P0-P3 finding.

Because PostgreSQL cannot store this oversized value while the real indexes
remain authoritative, the replacement query applies the same bounded
projection to derived `TEXT` only. It requires a `NULL` projected value and a
charge above `STORED_REPORT_BYTE_LIMIT`, then verifies unchanged indexes,
unchanged complete state, and a still-valid normal report
(`customer_data.rs:3338-3358`). Comments explicitly say this is projection-only
and cannot be credited as an oversized stored-row report path.

The counters preserve backend truth:

- PostgreSQL: five actual stored oversize/report rejections plus one separate
  index rejection; and
- SQLite: six actual stored oversize/report rejections and zero index
  rejections.

The final assertions enforce those exact totals
(`customer_data.rs:3440-3443`). The other five PostgreSQL stored mutations,
all six SQLite paths, cumulative charging, normal report behavior, and product
logic are unchanged.

## Successor controller admission review

The successor private controller
`/private/tmp/bluey-phase639-controller-OR0wQV/phase640-rust-gates-pg-index-fix.mjs`
had reviewed SHA-256
`8f006056e40080a17399cfde5fc63964539fdb8f90e7af019cb543027a3868b3`
and mode `0600`. An independent unified comparison against the previously
reviewed controller at SHA-256
`788e0b1401b7b6dfaf0220ad5e39db4b863ed27917f8bc66d9335219e80a9964`
showed exactly three literal changes:

1. the controller self path;
2. manifest pin `5d0a37...` to
   `91ef6fae3f3e49c8a98c3566076abcbc9c8aa84069d02f036847069f3a06cdc0`;
3. `customer_data.rs` pin `8a24407c...` to the reviewed `ea7591cb...`.

Stage ordering, disk guards, environment closure, Cargo commands, required
physical-PostgreSQL case, result-count checks, journaling, source-before/after
checks, process cleanup, and database cleanup are byte-identical to the
reviewed controller. Parent reported `node --check` passed. Parent also reported
that the complete 778-file scope replay produced manifest `91ef6f...`, status
`a00846d9519ee35d4268e4ca99098a8fda19db6025cfab6747a69561fc010d8c`,
and that substituting only the prior customer-data hash reconstructs the prior
manifest exactly. This reviewer verified the controller diff and pins, not a
controller execution at that review point.

## Prior failed evidence and corrected execution

Parent reported actual run 39075 on the prior source freeze:

- exact Rust formatting and server `cargo fmt --all --check` passed;
- strict all-target Clippy passed;
- eight focused financial SQLite and eight focused receipt SQLite cases passed;
- the required-fresh PostgreSQL case reached the real update and failed with
  SQLSTATE `54000` before report execution;
- the full server suite therefore did not run; and
- PostgreSQL/server/compiler/root/PID cleanup plus source-after checks passed.

The reported prior journal SHA-256 is
`7d0ce2e673e8006d417b19aea0997f73ae7b062343e89ff5360f30bc4f708846`.
That failed run is diagnostic evidence, not a pass and not transferable to the
new hash.

The reviewed successor later completed against the exact FIX862 source freeze.
The immutable 18-event journal is:

`/Users/uno/.codex/visualizations/2026/08/04/019fcdb9-9999-70c3-a063-c83542b6b5f0/phase640-rust-f1e300ef2190.jsonl`

Its independently verified SHA-256 is
`a996e087c253d653dc54c7b449a571b5ea3a720c950f9519ef9b7ce93e8842d3`.
The journal is 8,037 bytes, records the exact successor controller SHA, and
terminates with `controller_finished.success=true`. Its bounded raw stdout and
stderr siblings independently hash to the exact values recorded by the
journal.

The corrected run established:

- exact Rust formatting, server-wide formatting, and strict all-target Clippy
  passed;
- focused discovery found exactly eight financial and eight receipt cases, and
  both focused SQLite stages passed 8/8;
- the required wrapper exposed a live owned PostgreSQL 17 process at PID 6457
  and private port 53419, ran the exact corrected report case 1/1 with zero
  failure, ignore, or skip, and proved the PID and postmaster file absent before
  the no-PG full suite;
- the full all-target vector
  `[1784, 0, 1, 14, 1, 1, 2, 113, 2, 1, 5, 9]` totaled 1,933 with zero failures
  and zero ignored tests, including all sixteen new SQLite names; and
- the launcher ended code zero, the exact root, parent, owned PIDs, and related
  processes were absent, and the 778-file manifest `91ef6f...`, status, module
  hashes, empty index, and source were unchanged.

The physical PostgreSQL case executes the shared backend lifecycle, including
the corrected five-stored-plus-one-index oversize accounting and the normal
report readback. The conditional PostgreSQL name in the later no-PG full suite
was explicitly not credited as the physical pass.

## Overall verdict

🟢 **FIX862 QUALIFIED FOR THE BOUNDED TEST-ONLY PROTOTYPE** — The fixture
represents the real PostgreSQL schema limit without weakening indexes or
mislabeling a derived projection as stored-row evidence. The corrected physical
PostgreSQL case, focused SQLite cases, strict Clippy, complete all-target suite,
cleanup, and exact-source checks passed. This does not qualify production,
billing, release, invoice COGS, employer delivery, or historical completeness.
