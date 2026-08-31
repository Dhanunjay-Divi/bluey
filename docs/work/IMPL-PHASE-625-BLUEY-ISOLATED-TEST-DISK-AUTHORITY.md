# Implementation — Phase 625 Bluey Isolated Test Disk Authority

> **Codex preflight:** Loaded `$bluey-ops` and `$codex-storage-archive`, including the local-project
> archive safety reference. The current worktree is authoritative; no SSD fallback was needed.

**Status:** independent source-delta review GREEN with no P0–P3 findings; exact-tip hosted CI and
integrated-successor evidence remain required

## Scope

**Does:**

- provide one canonical launcher with `--self-test`, `all`, and direct-command modes;
- isolate Cargo, temporary, primary SQLite, data, config, runtime, and log paths in one private run;
- force SQLite, remove inherited PostgreSQL URLs, supervise a distinct process group, and execute
  every mode from the physical repository root;
- disable incremental and dev/test debug artifact growth;
- defer signals through the `mkdir` ownership transition, safely clean an early unmarked owned
  root, and fail closed after marker establishment if that marker is absent or malformed;
- bind current cleanup to the root device/inode, marker device/inode/mtime, and all nine canonical
  marker fields established by the latest marker write, while assigning root identity only once;
- pre-bind each complete marker temp identity, use an atomic rename, and reject post-rename marker
  or immutable-root substitution before committing new cleanup authority;
- clean the exact validated run after success, failure, HUP, INT, or TERM while preserving the
  command result;
- forward HUP, INT, and TERM portably to the command group and prove actual child receipt;
- reset signals and establish the wrapper's private group before its ready handshake, defer
  launcher signals until exact PID/PGID and running-marker authority exists, and bound both group
  and PID termination before reaping;
- conservatively recover eligible SIGKILL/crash leftovers on a later launch, while refusing to
  start a new command when owned retained metadata is unknown or eligible deletion fails;
- reject non-canonical/overflow marker arithmetic, cross-bind name epoch/PID and state/group, and
  revalidate root/marker device, inode, modification time, fields, age, owner, and group twice;
- atomically quarantine an eligible current or stale root under the same private parent, prove the
  original path absent, revalidate the moved root/marker twice, and perform same-filesystem deletion
  from the verified quarantine directory rather than recursively deleting a validated pathname;
- reject exact/ancestral Git metadata and registered-worktree overlap before parent mutation, with
  producer-status-preserving worktree enumeration capped before buffering at 1 MiB and bounded to
  ten seconds;
- keep the microphone-setting SQLite test in collision-safe launcher-owned temporary storage; and
- protect the behavior with fast fake-command tests plus a Jobs CI structural guard.

**Does NOT:**

- use the SSD, delete pre-existing targets/databases, or inspect unrelated build output;
- promise a trap can run after SIGKILL or host loss;
- delete linked, foreign-owned, malformed, young, active, or process-inspection-ambiguous runs;
- support commands that deliberately escape the supervised process group; or
- run or replace full Rust, Docker, hosted database, or release verification.

## Files Created / Modified

| File | Action | Purpose |
|---|---|---|
| `scripts/run-bluey-tests.sh` | Created | Own the canonical lifecycle, process-group boundary, reaper, and fake-command self-test. |
| `.github/workflows/jobs-ci.yml` | Modified | Execute the fast self-test before expensive gates. |
| `jobs/scripts/ci-guards-self-test.mjs` | Modified | Fail closed if CI wiring or critical safety clauses regress. |
| `crates/cue-daemon/Cargo.toml`, `Cargo.lock` | Modified | Add the test-only RAII temporary-directory dependency without a new resolved package. |
| `crates/cue-daemon/tests/mic_device_selection.rs` | Modified | Replace shared `/tmp` SQLite naming with launcher-controlled collision-safe temporary storage. |
| `AGENTS.md`, `README.md` | Modified | Establish the local usage contract. |
| `CHANGELOG.md` | Modified | Record the bounded reliability correction. |
| Round 625, FIX-785, this record | Created | Preserve design, root cause, evidence, and limitations. |

## Build & Test

```text
bash -n scripts/run-bluey-tests.sh
  PASS
bash scripts/run-bluey-tests.sh --self-test
  PASS — modes/env/DB/log/repository-CWD/group/zombie/reaper/child HUP-INT-TERM receipt/
  unknown-retained-metadata/stale-deletion-failure/marker-fail-closed/cleanup-failure/
  canonical-number/name/state binding/future/young-copied-root/path-identity/same-field-inode-swap/
  current-root/marker substitution/pre-running-root and post-rename-marker transition/
  pre-quarantine and current+stale final-delete substitution/retained-quarantine admission/
  exact-Git/worktree-enumeration/parent/post-mkdir-signal/pre-gate-HUP-INT-TERM/residue
node jobs/scripts/ci-guards-self-test.mjs
  PASS — includes independent negative canonical-number/name/state/age/reread-identity,
  all-nine-field, current/stale root+marker snapshot/revalidation, atomic marker/quarantine
  transitions, anchored deletion, producer status/time/byte bounds, exact-Git/worktree enumeration,
  private-handshake/deferred-delivery, bounded-stop, and same-field-inode authority fixtures
bash scripts/run-bluey-tests.sh -- cargo test -p cue-daemon \
  --test mic_device_selection load_mic_device_setting_round_trips -- --exact
  PASS — 1 passed; 0 failed; 4 filtered out; isolated root removed
bash scripts/check-bluey-ops-docs.sh
  PASS — all agent entry points, work templates, and runbooks covered
git diff --check
  PASS
actionlint .github/workflows/jobs-ci.yml
  SKIP — actionlint was not installed; no download was attempted
independent frozen-byte source-delta review
  GREEN — no P0–P3 findings
  reviewed 12-path manifest:
  37a00810346ebe0093c23267da0312c3f2fecac08fb350c17a6c1b378aee7dca
  PASS — native macOS Bash 3.2 shell syntax and launcher self-test (26.1 seconds)
  PASS — Node syntax and complete Jobs CI guard (15.4 seconds)
  PASS — Bluey operations-doc guard, diff check, process check, and residue check
  SKIP — actionlint unavailable; no Rust/Cargo/Docker/network/commit/push/production action
```

The focused microphone-setting Rust integration test was compiled and run for the earlier
correction layer and those Rust files are unchanged by this follow-up. Per correction scope, no
Rust command was rerun for the latest launcher-only safety delta; no full Rust suite or Docker
build was run.

## Deviations from Plan

| Deviation | Rationale |
|---|---|
| No full Rust/Docker verification | The focused affected Rust test and behavioral suite are proportionate; full release gates remain hosted. |
| Exact-tip hosted and integrated-successor evidence pending | The independent GREEN verdict covers the frozen source delta only and is not aggregate release authority. |

## Known Follow-ups

- Require the exact-tip hosted Jobs self-test to pass after push.
- Re-run the applicable gates on the integrated successor before treating this phase as aggregate
  release evidence.
- Keep full Rust/Docker release gates on a resource-capable host or CI and invoke local ad hoc
  checks through the canonical launcher.

## Review Checklist (for reviewer)

- [x] Early unmarked cleanup is limited to the generated root after deferred-signal ownership;
  marker-established cleanup requires the established root/marker identities and canonical fields.
- [x] Symlinks and active launcher/process-group state always fail closed.
- [x] Success, failure, HUP, INT, and TERM preserve defined results; the child proves signal receipt;
  cleanup failure is explicit.
- [x] Missing/malformed owned retained metadata and failed eligible stale deletion block before
  child start with exact-root guidance.
- [x] Marker arithmetic, name/state cross-binding, filesystem age, both path identities, exact
  fields, and fresh age/owner/group checks remain enforced before deletion.
- [x] Root identity is assigned once; marker transition authority is committed only after the
  pre-bound temp marker survives atomic rename and post-rename root/marker checks.
- [x] Stale and current deletion share one same-parent atomic quarantine path, verify original
  absence and moved authority, use anchored same-filesystem deletion, and retain/block on ambiguity.
- [x] Exact Git metadata and registered-worktree overlap are rejected before mutation; worktree
  producer failure, timeout, or an oversized response cannot authorize or fully buffer a run.
- [x] SIGKILL is described only as later stale recovery, never direct cleanup.
- [x] CI behavior and negative structural fixtures cover every critical source clause.
- [x] No unrelated source, production state, existing cache, database, or SSD path changed.
