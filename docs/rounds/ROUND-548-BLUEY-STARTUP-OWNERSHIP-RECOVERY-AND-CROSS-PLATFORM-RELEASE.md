# Round 548: Bluey Startup Ownership Recovery And Cross-Platform Release

Date: 2026-07-19

Status: release candidate; publication and corrected Jobs deployment are still
gated by the checks marked pending below.

## Scope

Repair the `0.1.102` first-restart failure reported after the updater installed
successfully, verify the exact `0.1.103` desktop artifacts on every supported
architecture, reconcile concurrent mainline Jobs work without weakening its
truth, quota, spend, or recovery boundaries, and deploy only artifacts that
pass exact installed-runtime and rollback checks.

The unrelated main Bluey API and Caddy are outside the Jobs deployment scope.
No employer application is submitted by this round.

## Reported Failure And Root Cause

The updater downloaded, checksum-verified, installed, ad-hoc signed, and linked
`0.1.102`, then the restarted daemon exited before readiness.

The failing account already had a meeting record with an account owner while
its older SQLite session projection still had a `NULL` owner. Startup attempted
an owner-scoped mutation without first adopting that exact legacy projection.
The database correctly rejected the ownership mismatch, but the daemon treated
the migration case as fatal.

The repair adopts only an exact same-session `NULL`-owner projection into the
meeting's known owner scope. A projection owned by any other account still
fails closed. Tests preserve turns, saved answers, archived state, and the
active-session pointer, and cover idempotent restart plus cross-account and
owned-to-local rejection.

## Overlay Recovery Corrections

The restart audit also found that overlay readiness could race with initial
state delivery. The corrected lifecycle now has:

- a per-process generation and hydration state;
- ready acknowledgement only after the initialization payload is enqueued;
- one ordered, backpressured channel for ordinary events;
- bounded priority handling only for readiness/control messages;
- generation-fenced writes so a stale process cannot mutate its replacement;
- persisted visibility intent before delivery;
- transition-locked suppression of programmatic feedback;
- exact-generation hydration as the restart success condition; and
- one five-attempt replacement budget rather than nested retry multiplication.

The macOS overlay emits opacity events for user changes, not for programmatic
state restoration.

## Sealed Desktop Provenance

```text
Source commit: 381cbd532edee25ab596e01d1b30fa8dbd6e6d4b
Git tree: c8f0dab28365bdbb228b9a9fe1e772d5c0ded997
SOURCE_DATE_EPOCH: 1784436621
Source archive: bluey-0.1.103-381cbd53-source.tar.gz
Source archive bytes: 33,104,840
Source archive SHA-256: 18660a89dce368f476dd4756014dee96b4856a4ef81e9013a2b03277ddfd1808
Uncompressed tar members: 2,153
```

The compressed and uncompressed source payloads are byte-identical to `git
archive` for the sealed commit. Every member is a safe relative path. The later
Jobs/server merge changes none of the desktop packaging inputs; final release
documentation records desktop and server provenance separately.

## Reproducible Desktop Artifacts

| Target | Bytes | SHA-256 | Reproducibility |
|---|---:|---|---|
| `darwin-arm64` | 22,029,698 | `ffe67c1ac100ac7e02022746d0103103166bb194a00d1a04fc75cb58b36647e8` | two builds byte-identical |
| `darwin-universal` | 45,421,535 | `ba043ea6e03da1bc7e38ae9a8bd5e66c3f855b5d1673be7c5deb1e542baef94e` | two builds byte-identical |
| `darwin-x86_64` | 23,385,574 | `f53763814cf9d000911a64fa95cc56c2f95a6bf2d473a8d35dd5ad2b0d27db9d` | two builds byte-identical |
| `windows-x86_64` | 22,734,864 | `3b857f1afeb455d8aff2be67abe82a77a06b2468d0b5e75bea46457defe5daba` | two canonicalizations byte-identical |

All checksum sidecars pass. Archive members have no duplicate or unsafe paths.
Every macOS executable has the declared architecture. Every Windows executable
is PE32+ AMD64. The raw 17-member Windows builder ZIP is retained only as build
provenance and must never be published; the canonical 15-member ZIP above is
the release artifact.

## Exact Installed-Runtime Gates

| Lane | Native process | Window proof | Capture exclusion | Result |
|---|---|---|---|---|
| macOS arm64 native | arm64 | real `760x500` window | sharing state `0` | PASS |
| macOS universal native | arm64 | real `760x500` window | sharing state `0` | PASS |
| macOS universal Rosetta | translated x86_64 | real `760x500` window | sharing state `0` | PASS |
| macOS x86_64 Rosetta | translated x86_64 | real `760x500` window | sharing state `0` | PASS |
| Windows x86_64 Session 1 | x86_64 | real `860x460` topmost layered tool window | `WDA_EXCLUDEFROMCAPTURE=0x11` | PASS |

Every lane installed the exact hashed package and verified version `0.1.103`,
the expected process architecture, a stable daemon PID/start time, hidden to
visible overlay transition, owner-only IPC, clean shutdown, and zero remaining
test processes. Windows also removed its temporary scheduled task. A later
read-only audit again found zero candidate processes and zero test tasks.

The runtime gates set all secure-store and plaintext fallback variables to
`0`. No Keychain or Windows Credential Manager path was used.

## Jobs Mainline Reconciliation

Concurrent mainline work added managed job-specific resume generation. Review
found that the first version was not safe to deploy because it could:

- accept short credentials or move metrics across unrelated evidence;
- read different profile or posting revisions during one generation;
- persist a draft before async work completed;
- exceed the generation lease during provider retries;
- dispatch before reserving the user's Jobs packet allowance;
- double meter, strand, or mishandle allowance across crash and period rollover;
- use read-only spend checks that concurrent calls could all pass;
- bypass provider key health/cooldown behavior; and
- trust legacy embedded application IDs instead of authoritative database
  columns in some execution paths.

The corrected design is being independently reviewed. Its acceptance contract
is:

1. Model output selects exact evidence identifiers; Bluey composes text from
   those indivisible records and never accepts model-authored candidate claims.
2. The exact profile and posting snapshots used for the baseline are carried
   through generation and finalization with immutable fingerprints.
3. No application row is created or regressed until generation succeeds and a
   compare-and-swap finalization commits atomically.
4. One included Jobs packet is held before provider dispatch, then converted
   into ordinary packet metering without a second increment or chat-balance
   debit.
5. Quota holds are token-fenced, crash-recoverable, cancellation-safe, and
   correct across monthly rollover.
6. Projected provider cost is atomically held before every dispatch. Ordinary
   managed requests and Jobs calls share one global spend reservation boundary.
7. Missing usage, timeout, cancellation, and ambiguous provider failure retain
   the conservative projected exposure.
8. Account and provider/model rate limits plus provider key health/cooldown
   apply before dispatch.
9. Generation is default-off and requires explicit enablement, an account
   limiter, and an upstream spend guard.
10. Deterministic tailoring remains the fail-closed path.

The final Jobs/server source commit, test counts, PostgreSQL migration results,
and deployment identity will be added only after the independent P0/P1 review
is clear.

## Unexpected Production Drift And Containment

During the isolated restore preflight, production was observed running the
unreviewed managed-generation commit `3095d406…` with
`BLUEY_JOBS_MODEL_GENERATION_ENABLED=1`. Migration 007 and one completed
generation row already existed. This deployment was not performed by this
release path.

Containment was immediate and scoped:

1. the Jobs environment file was backed up;
2. managed generation was set explicitly to `0`;
3. local and cloud browser distribution were confirmed `0`;
4. only `bluey-jobs-api` was restarted; and
5. because the commit also changed persistence behavior, the service was
   restored to the exact pre-3095 binary.

The stable containment state is:

```text
Jobs commit: f3a0a04360febb36363c27f869e954f2d61f32e0
Jobs binary SHA-256: 07e12cb5d5668c4c8cd24129a9fbccc3f512e873d2400867b0fde55be288ac96
Managed generation: 0
Local browser distribution: 0
Cloud browser distribution: 0
Jobs service: active, NRestarts=0
```

The main Bluey API PID and Caddy PID remained unchanged throughout. Migration
007 is additive and is ignored by the restored binary. The unexpected binary
and environment snapshots are retained as evidence; the single generation row
is not printed or copied into release evidence.

## PostgreSQL Restore Drill

A preliminary drill restored the verified 2026-07-19 06:00 production backup
into a uniquely named PostgreSQL 18.4 clone. The backup is 24,784,932 bytes,
SHA-256 `c3c6ea38…`. Only aggregate counts were emitted: 26 accounts and 2,680
usage events.

The backup already contained migration 007. The clone alone was rewound to the
pre-007 state so migration replay and idempotent restart could be tested. No
production database or live service was written by the drill.

This result is preliminary because the corrected branch adds migration 008.
The exact final source must repeat the restore, migration replay, schema and
ledger checks, stale-token/quota/cascade tests, restart idempotence, and old
binary compatibility before production deployment.

## Pending Combined Gates

- [ ] Independent Jobs/server review reports no P0/P1 findings.
- [ ] Workspace tests, all-target Clippy with warnings denied, formatting, and
  release build pass on the final combined commit.
- [ ] Server unit and HTTP integration suites pass.
- [ ] Jobs JavaScript tests, typecheck, privacy/provenance/schema guards, and
  production portal build pass with no source maps.
- [ ] PostgreSQL 18 restored-backup drill passes against the exact final commit.
- [ ] Latest `origin/main` is reconciled without accepting the permissive
  cross-evidence narrative implementation.
- [ ] Documentation is committed and `git diff --check` passes.
- [ ] Signed `0.1.103` manifest and immutable artifacts publish successfully.
- [ ] Every live artifact hash, content type, checksum, and signature verifies.
- [ ] Isolated fresh install and `0.1.102` to `0.1.103` updater smoke pass.
- [ ] Corrected Jobs binary/portal deploy with model/browser execution disabled,
  scoped health checks, and rollback verification passes.

## Rollback Boundary

Desktop rollback restores the prior signed manifest only after its referenced
immutable assets and signature are reverified. Jobs rollback restores only the
previous `bluey-jobs-api` binary and Jobs portal. Additive database migrations
remain in place unless a separately approved database restore is required.

Never restart, replace, or reconfigure the unrelated main Bluey API or Caddy as
part of a Jobs rollback.
