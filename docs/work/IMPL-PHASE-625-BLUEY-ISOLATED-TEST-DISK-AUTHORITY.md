# Implementation — Phase 625 Bluey Isolated Test Disk Authority

> **Codex preflight:** Loaded `$bluey-ops` and `$codex-storage-archive`, including the local-project
> archive safety reference. The current worktree is authoritative; no SSD fallback was needed.

**Status:** independent local source review accepted; exact-tip hosted CI required after push

## Scope

**Does:**

- provide one canonical launcher with `--self-test`, `all`, and direct-command modes;
- isolate Cargo, temporary, primary SQLite, data, config, runtime, and log paths in one private run;
- force SQLite, remove inherited PostgreSQL URLs, supervise a distinct process group, and execute
  every mode from the physical repository root;
- disable incremental and dev/test debug artifact growth;
- defer signals through the `mkdir` ownership transition, safely clean an early unmarked owned
  root, and fail closed after marker establishment if that marker is absent or malformed;
- clean the exact validated run after success, failure, HUP, INT, or TERM while preserving the
  command result;
- conservatively recover eligible SIGKILL/crash leftovers on a later launch; and
- protect the behavior with fast fake-command tests plus a Jobs CI structural guard.

**Does NOT:**

- use the SSD, delete pre-existing targets/databases, or inspect unrelated build output;
- promise a trap can run after SIGKILL or host loss;
- clean stale unmarked, linked, foreign-owned, malformed, young, future-dated, active, or
  process-inspection-ambiguous runs;
- support commands that deliberately escape the supervised process group; or
- run or replace full Rust, Docker, hosted database, or release verification.

## Files Created / Modified

| File | Action | Purpose |
|---|---|---|
| `scripts/run-bluey-tests.sh` | Created | Own the canonical lifecycle, process-group boundary, reaper, and fake-command self-test. |
| `.github/workflows/jobs-ci.yml` | Modified | Execute the fast self-test before expensive gates. |
| `jobs/scripts/ci-guards-self-test.mjs` | Modified | Fail closed if CI wiring or critical safety clauses regress. |
| `AGENTS.md`, `README.md` | Modified | Establish the local usage contract. |
| `CHANGELOG.md` | Modified | Record the bounded reliability correction. |
| Round 625, FIX-785, this record | Created | Preserve design, root cause, evidence, and limitations. |

## Build & Test

```text
bash -n scripts/run-bluey-tests.sh
  PASS
bash scripts/run-bluey-tests.sh --self-test
  PASS — modes/env/DB/log/repository-CWD/group/zombie/reaper/marker-fail-closed/
  cleanup-failure/parent/post-mkdir-signal/residue
node jobs/scripts/ci-guards-self-test.mjs
  PASS — includes negative isolated test-disk authority fixtures
bash scripts/check-bluey-ops-docs.sh
  PASS — all agent entry points, work templates, and runbooks covered
git diff --check
  PASS
actionlint .github/workflows/jobs-ci.yml
  SKIP — actionlint was not installed; no download was attempted
```

No Rust or Docker build was run in this phase.

## Deviations from Plan

| Deviation | Rationale |
|---|---|
| No heavy Rust/Docker verification | Explicit Phase 625 resource constraint; the behavioral suite uses fake commands. |
| Independent review completed after implementation | The committed REVIEW record preserves the clean source verdict and hosted-evidence boundary. |

## Known Follow-ups

- Require the exact-tip hosted Jobs self-test to pass after push.
- Keep full Rust/Docker release gates on a resource-capable host or CI and invoke local ad hoc
  checks through the canonical launcher.

## Review Checklist (for reviewer)

- [ ] Early unmarked cleanup is limited to the generated root after deferred-signal ownership;
  marker-established cleanup requires the exact valid marker-owned immediate child.
- [ ] Symlinks and active launcher/process-group state always fail closed.
- [ ] Success, failure, HUP, INT, and TERM preserve defined results and cleanup failure is explicit.
- [ ] SIGKILL is described only as later stale recovery, never direct cleanup.
- [ ] CI behavior and negative structural fixtures cover every critical source clause.
- [ ] No unrelated source, production state, existing cache, database, or SSD path changed.
