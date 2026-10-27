# Jobs-only Phase 629 local handoff

Date: 2026-10-27. Nonproduction validation, not release/deployment approval.
Status: bounded nonproduction batch complete; no production actions authorized.

## Authority

- Checkout: `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`
- Branch: `feat/phase-629-jobs-nonprod-integration-validation`
- Base: `130ef807bc952874a2cb391e43511d7364ab1b0a`
- PostgreSQL authority/validation commit:
  `26dad0dfd65f3ce60bbd1d8af4a2f8063a2429f7`
- Jobs dependency correction commit:
  `e6dd473f40b9d5314b365a820fa983ff8635b9d8`
- Documentation closeout: `docs(jobs): record nonproduction integration validation`;
  resolve its final ID with `git log -1 --format=oneline` on this branch.
  All three commits are local only; no push or PR creation occurred.
- Current code and local handoffs are authority. SSD is only a specific missing
  historical-detail fallback; none was used in this batch.
- Never edit/start `/Users/uno/Downloads/cue`, the Bluey desktop host/overlay,
  Pinky, existing product VMs or production resources for this Jobs batch.
- Preserve the inherited untracked canonical
  `docs/work/REVIEW-PHASE-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md` unchanged and
  unstaged. It is not self-approved by this successor.

Read `bluey-ops`, checkout AGENTS.md, Round 629, IMPL/REVIEW 629 and FIX-795–801.
The Phase 628 handoff's missing-pgvector/unavailable-Docker statements are
historical: this batch established actual private PostgreSQL and Linux evidence.
Its two-hour fixture preview has expired; no new preview was started.

## Bounded changes

1. Disposable PostgreSQL 17/pgvector lifecycle inside the canonical isolated
   launcher. Strict owned-server/credential/environment checks, clean shutdown
   requirement, and a fresh random database per exact regression case.
2. Three production Rust corrections only: explicit BIGINT ready-sequence
   binding; typed exact PostgreSQL boot-trigger denial to generic HTTP 409;
   status-sensitive missing-capacity handling while retaining row-lock-before-
   clock ordering and fail-closed irreversible/terminal handling.
3. Configured PostgreSQL/API/lease/replay/migration/cascade tests and corrected
   beta fixtures. No migration bytes or production authority fence weakened.
4. Audited Jobs-only dependency corrections, CSV header regressions and current
   isolated Jobs Browser Electron package contract. No host-overlay dependency
   was changed; historical signed fixtures remain unchanged.

## Observed evidence

| Gate | Result |
|---|---|
| Configured PostgreSQL 17.10 / vector 0.8.3 | 30/30 exact cases, separate fresh databases; clean shutdown |
| Rust formatting / strict all-target Clippy | Pass with both integration-support features |
| Full all-target Rust | 1,703 library + 1 binary + 134 integration = 1,838 pass; zero failed/ignored; wrapper exit 0 |
| Jobs workspaces | 2,025 passing tests; all five typechecks/builds; installed audit 0 |
| Real Chromium synthetic-submit guard | Pass, not skipped; no employer request |
| Release / schema / CI guard | 32/32; 113 tables / 91 indexes; pass |
| Linux amd64 QEMU Docker | Pinned build and exact-image hardened native smoke pass |
| Linux native storage | 1 unit + 13 integration cases pass |

The configured API test enters real Axum handlers with an already-authenticated
worker extension and real PG; it is not socket/fleet-HMAC proof. Offline full
tests have no configured PostgreSQL URL; early-return PG cases there are not
counted as configured PG evidence. Electron binary download was disabled for
workspace quality, so no signed/native Electron artifact proof is claimed.

Local smoke image:
`sha256:bcab3b4e6cf7386acec09b1d48bfc2fd8f5293e2f495b30f7fd51f199083f385`.
It binds base `130ef807` plus the seven corrected Jobs manifests/lock files,
not a final-source signed production candidate. Measurement: 410 files of the
unchanged 512 bound; digest and lock identity are in IMPL. QEMU execution does
not establish native amd64 performance.

## Resources and continuation

The dedicated VM `bluey-jobs-629-linux-amd64`, its containers/volume and data
were explicitly removed; four existing profiles remained stopped and Docker
context stayed `default`. No broad cleanup occurred. Completed Jobs quality
and earlier PG roots were verified absent.

Completed final full-run session: `79633`. Canonical root:
`/private/tmp/bluey-test-runs-phase629-final30-20261027/run.1793120675.5618.2087088370`.
The exact root is absent and all 20 Phase 629 parents are empty after automatic
cleanup; free space recovered from about 32 to 39 GiB. No live test remains to
resume. The library took 2,509.19 s and the HTTP suite passed 113/113 in 538.61 s.
Source/manifests/harness matched the staged tested bytes at completion. Local
commits contain only explicit Phase 629 paths; no push follows automatically.

The final unchanged-source release/schema/CI guard rerun also passed and its
additional owned root was removed; all 21 task-specific parents were empty at
closeout. The separately authored Phase 629 REVIEW accepts only this bounded
local batch, with no open finding and no production-release approval.

Heavy commands must use the existing shared `mac-build` machine lock and
`scripts/run-bluey-tests.sh`; scheduler policy/runtime are under
`/Users/uno/Projects/shared-runners/`. Never supply an owner/default/hosted
database URL. The new harness creates and injects its own URL inside that
boundary. See IMPL for failure history, negative harness checks and qualifications.

## Still separate release gates

Exact-tip hosted CI, registry publication/read-back, signing and protected
activation, hosted PostgreSQL/Temporal/network qualification, provider/ATS
canaries and OAuth approvals, live runner capacity and in-flight rollback /
kill-switch rehearsal remain separate evidence and authorization gates. No
deployment, production flag, production account mutation, real application,
email, calendar action or paid infrastructure was used or enabled here.
