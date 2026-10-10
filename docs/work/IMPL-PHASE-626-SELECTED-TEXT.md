# IMPL: Phase 626 — owned selected-text evidence

Codex preflight: `bluey-ops`, `pinky-ops`, `pinky-bluey-integration-ops`.

## Scope

Does: additive default-off delegated text artifact create/list/delete, bounded
owned selection, content/hash replay, deletion-aware dispatch fence, dual
SQLite/PostgreSQL storage and USER-only untrusted-document projection.

Does not: deploy, enable media, change Jobs, activate real billing or replace
standalone auth/normal completion. Pinky owns the UI and bridge in its repo.

## Files created / modified

| Files | Purpose |
| --- | --- |
| `server/src/pinky_integration/{mod,delegation,store,http_tests}.rs` | validation, exact scope, storage, ownership, replay and boundary tests |
| `server/src/api/router/{completion,streaming_completion,prompt_contracts,tests}.rs` | delegated-only provider-visible document evidence and standalone regression |
| `scripts/run-pinky-integration-tests.sh` | include selected-document projector regression in focused gate |
| integration skill/runbook/continuation and CHANGELOG | discoverable ownership, test and handoff boundaries |

## Build & test

Formatting/diff checks passed. Initial two focused Rust builds caught a SQLite
tail-expression lifetime error; corrected in `edb2f089`. Default-off configuration
fixtures were updated in `ba53a2c6`. The 44 focused tests and one projector
regression passed on two runs. Strict Clippy caught a conditional/signature lint
and two test-only cloned-reference lints; corrected in `39880614` / `7a288472`.
At exact code revision `7a288472`, the final queued combined gate passed all 44
focused tests, one projector regression and strict all-target Clippy. Formatting
and diff checks passed; the owned temporary test root was removed.

The test launchers' cleanup self-tests passed success/failure/TERM. Failed build
temporary roots were removed. No permanent Cargo/test DB directory is created.

## Deviations and follow-ups

Selected text is 16 KiB aggregate, not unrestricted document indexing. Exact
Go JSON escaping requires a 128 KiB envelope on artifact-create alone; other
delegated routes retain 16 KiB. Managed reservation estimate preserves legacy
empty-context behavior and uses a conservative bound for selected evidence.

Real PostgreSQL, live selected-document grounding/delete and Windows native
file selection remain required. See the continuation for subsequent media,
history, billing and publication gates.
