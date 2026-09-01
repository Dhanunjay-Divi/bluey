# Review — Phase 626 Jobs Authenticated Workspace Pagination

**Status:** 🟢 independent source/static review; release evidence remains gated

**Base:** `95696dd0`

## Current verdict

The fresh independent full line-by-line review of the frozen 50-path candidate is **GREEN for
source/static review: P0: 0, P1: 0, P2: 0, P3: 0**. Before and after review, the candidate remained
at base/HEAD `95696dd0`, branch `feat/phase-626-jobs-workspace-pagination`, with the canonical
path-sorted per-file SHA-256 manifest
`c5217a647ad967537d52c65f76cfdb8901a1df88f20fab9069138d93a507bd8b`. No implementation,
fixture, migration, or other non-review file was changed by the reviewer. No commit, push,
migration, deployment, production action, or release activation is claimed.

The reviewed correction candidate:

- recomposes current profile, safe preferences, Career Track, attempt reservations, discovery,
  signed source destination, ATS, and integrity authority inside the first packet-metering
  transaction before allowance, debit, ledger, or meter effects; exact replay still returns first;
- traverses exhaustive account export in 100-row authority-composition chunks and includes
  source-positive 102-row SQLite and configured-PostgreSQL fixtures across the 100/2 boundary;
- supplies SQLite's sixth lease-liveness binding on the first verified-import repair claim and
  includes a first-owner runtime regression;
- takes the PostgreSQL parent Account deletion/write fence before policy and the exact application/
  posting, then locks the entitlement before any reservation read. The packet-specific Account
  lock is `FOR NO KEY UPDATE`, preserving the deletion fence while permitting an entitlement owner
  to satisfy a reservation's Account foreign-key check. The entitlement values are reused for
  metering. One post-lock database timestamp governs current composition, posting/discovery
  freshness, reservation period, packet/allowance timestamps, entitlement updates, and any
  overage ledger entry;
- replaces the full-account reservation scan in both dialects with one active same-company row
  plus at most 50 active current-period rows, excluding the current application and de-duplicating
  IDs. Static source proof enforces replay → globals → Account → application/posting → entitlement
  → reservations and rejects reservation SQL between application/posting and entitlement;
- includes the prior configured Account-deletion race plus a new configured entitlement race that
  observes real packet commit waiting on E, commits a relevant R without lock timeout, then requires
  the awakened commit to re-read R and reject with all packet-effect state unchanged; and
- binds match-page continuation installation to the exact installed workspace authority epoch as
  well as its cursor, so a refresh or policy mutation rejects an in-flight old response.

The earlier correction set also retains exact job-detail handoff revalidation, bounded
canary/quarantine/circuit reads, signed manifest child-cardinality checks, database-time-live
repair release, and a non-`Debug`, non-`Clone` secret-bearing lease.

## Evidence status

Source regressions exist for the earlier five blockers and the remaining P1 correction, including
zero balance/credit/allowance/meter effects after Track, profile, preference, and reservation
drift; exact replay after policy drift; Account/A/E/R static order; Account-deletion and
entitlement/reservation races; 102 positive export rows; first SQLite claim; and portal
authority-epoch response rejection. These fixtures have not been executed on the new correction
source in this worktree.

A bounded independent review of only the final packet correction delta first found a database-time
freshness/effect gap and an overly broad race-error assertion. After those were corrected, the same
reviewer returned GREEN. The subsequent fresh full review inspected all 50 paths and independently
confirmed **P0: 0, P1: 0, P2: 0, P3: 0**. It verified exact replay before new authority locks; the
PostgreSQL global → parent Account (`FOR NO KEY UPDATE`) → policy → application/posting (A) →
entitlement (E) → bounded relevant reservations (R) order; no reservation read between A and E;
one database timestamp captured only after E and reused through current discovery/eligibility and
all packet effects; and current-effect rejection before any allowance, debit, ledger, entitlement,
or meter mutation. The source/static verdict is therefore GREEN.

The safe non-build evidence rerun against the exact reviewed source is: Bluey ops documentation
guard PASS; Jobs client/server boundary guard PASS; edge/crawler policy guard PASS;
`git diff --check` PASS; and retained portal executables `vitest`, `tsc`, `prettier`, and `eslint`
absent. No package installation or network access was used. Previously recorded focused Rust and
382-case portal passes predate this correction set and are not exact-source release evidence.

Local Rust execution remains stopped because the prior isolated Phase 626 target grew to about
14 GiB physical (28.1 GiB apparent). It was proven inactive and cleaned with the exact target-dir
`cargo clean`; measured residuals were 277 MiB in the temporary root and 172 KiB in its cargo-home.
Future Rust evidence must use the canonical auto-cleaning launcher after integration. PostgreSQL
execution likewise remains a hosted isolated-database gate.

## Required fresh gates

- Exact-source focused and full Rust tests, check, formatting, and strict Clippy through the
  reviewed auto-cleaning launcher on the integrated successor.
- Isolated PostgreSQL migration, current-effect drift, exact replay, parent-Account deletion race,
  entitlement/reservation contention race, 102-row positive export, scalar/page parity, and
  rollback-only query-plan fixtures on the integrated successor.
- Portal typecheck and complete Vitest suite from integrated or hosted dependencies, including the
  workspace-authority epoch continuation cases.
- Hosted CI and resource-capable exact-tip evidence for every required release gate.

The source/static review is GREEN. Until the remaining executable and hosted gates pass on the
integrated successor, Phase 626 remains release-gated and must not be promoted.
