# REVIEW: Observability Phase 2 — daemon + dashboard log rotation

**Commit range:** `7a5653c..fdf3611` (Phase 2 implementation: `fdf3611`)
**Reviewer:** Kiro
**Date:** 2026-05-22

## Verdict

🟢 **ACCEPT** — Phase 2 is shippable. Two Phase 4 ↔ Phase 2 integration
gaps in MY territory (doctor and logs export hardcode the log dir
instead of calling `cue_core::local_log_dir()`); fixing as a small
followup commit on top of this verdict, no fix-round on Phase 2 itself.

## What I Reviewed

- `docs/rounds/OBSERVABILITY-PHASE-2-FOR-KIRO-REVIEW.md`
- `crates/cue-core/src/logging.rs` (NEW, 398 lines)
- `crates/cue-core/src/lib.rs` (exports)
- `crates/cue-core/Cargo.toml` (added `tracing-appender`)
- `Cargo.toml` (workspace dep + lockfile updates)
- `crates/cue-daemon/src/app.rs` (subscriber init replacement)
- `crates/cue-dashboard/src/lib.rs` (subscriber init replacement)

## What's Right

### `cue_core::logging` design — 🟢 strong

- **`init_local_json_logging(component, default_filter) → LocalLogGuard`** is the right shape: takes the component name as a `&'static str`, a default RUST_LOG-shaped filter string, returns a guard that holds the WorkerGuard alive for the process lifetime.
- **`LocalLogGuard`** correctly separates file-mode and stderr-only-fallback. Drop semantics flush the appender's buffered queue.
- **`local_log_dir()`** prioritizes `BLUEY_LOG_DIR` then `CUE_LOG_DIR` env vars, then platform-specific defaults (`~/Library/Logs/Bluey` on macOS, `~/.local/state/bluey/log` on Linux, `dirs::data_local_dir().join("Bluey/logs")` on Windows). **Public API** — Phase 4 should call this directly (see §4 below).
- **`log_file_prefix(component)`** correctly maps `cue-daemon`/`bluey-daemon` → `daemon`, etc., and the actual on-disk file becomes `daemon-log.YYYY-MM-DD.log`. The `-log` suffix is from `tracing-appender`'s `filename_suffix`/`filename_prefix` shape; it's predictable and matches the unit tests.
- **`StandardJsonEventFormat`** emits the standard fields per Observability Round plan §2: `ts_ms`, `level`, `target`, `component`, `version`, `platform`, plus event-attached fields. JSON-encoded one-per-line.
- **`JsonFieldVisitor`** correctly visits all `tracing::field::Field` types: bool, i64, u64, f64, str, error (uses `to_string()`), debug (uses `format!("{:?}")`).
- **Retention** via `retain_recent_log_files` keeps only files matching the prefix AND the `.YYYY-MM-DD.log` suffix; sorted by date, retains the latest N=7. Verified by `retention_keeps_newest_matching_logs_only` test.
- **Subscriber-install failure** correctly degrades to stderr-only with `eprintln!` warning; product-friendly (Bluey startup never blocks on log-dir issues).

### Daemon + dashboard wiring — 🟢 strong

- `cue-daemon/src/app.rs::run()` replaces the previous `tracing_subscriber::fmt().with_env_filter(...).init()` with `cue_core::init_local_json_logging("cue-daemon", "...")`. The `_log_guard` binding holds the WorkerGuard for the function's lifetime. Correct.
- `cue-dashboard/src/lib.rs::run()` does the same with `cue-dashboard` as the component name. Correct.
- Default filter strings include all four major bluey crates (`cue_daemon`, `cue_core`, `cue_cloud_client`, `cue_llm`, `cue_router` for daemon; equivalents for dashboard). Sensible defaults.
- `RUST_LOG` overrides via `EnvFilter::try_from_default_env()` fallback — operator escape hatch.

### Smoke test on uno — 🟢 verified

```
$ RUST_LOG=info BLUEY_LOG_DIR=/tmp/bluey-phase2-smoke3 timeout 4 \
    ./target/release/bluey-daemon --no-overlay
$ ls /tmp/bluey-phase2-smoke3/
  daemon-log.2026-05-22.log  (608 bytes)
$ cat /tmp/bluey-phase2-smoke3/daemon-log.2026-05-22.log | head -3
{"component":"cue-daemon","level":"info","log_dir":"/tmp/bluey-phase2-smoke3","message":"local log rotation initialized","platform":"macos-aarch64","target":"cue_core::logging","ts_ms":1779435905637,"version":"0.1.0"}
{"component":"cue-daemon","level":"info","message":"RAG pipeline disabled: no OpenAI API key configured","platform":"macos-aarch64","target":"cue_daemon::app","ts_ms":1779435905652,"version":"0.1.0"}
{"component":"cue-daemon","level":"info","message":"Bluey daemon listening on 127.0.0.1:57321","platform":"macos-aarch64","target":"cue_daemon::app","ts_ms":1779435905654,"version":"0.1.0"}
```

JSON format is clean. All standard fields present. `target` is the
tracing module path. `component`/`version`/`platform` are stamped per
event by the formatter.

### Tests — 🟢 sufficient

Three unit tests in `crates/cue-core/src/logging.rs`:

- `log_file_prefix_matches_support_tool_filters` — covers `cue-daemon`, `bluey-daemon`, `cue-dashboard`, `cue-cloud-client` mappings.
- `retention_keeps_newest_matching_logs_only` — synthetic fixtures covering 9 daemon files + 1 dashboard file; asserts only newest 7 daemon files remain, dashboard file untouched.
- `local_json_logging_writes_standard_fields` — end-to-end smoke: sets BLUEY_LOG_DIR to a temp dir, calls init, emits one event, drops guard, reads the produced file, asserts component/version/platform/answer/message all appear in the JSON line.

The smoke test correctly handles RUST_LOG state preservation.

## Blockers

None.

## Nits

### N-1 🟡 Unused import warning in release builds

`crates/cue-core/src/logging.rs:17`:

```rust
use tracing_subscriber::fmt::writer::MakeWriterExt;
```

This import is only used in the `#[cfg(debug_assertions)]` block
(`file_writer.and(std::io::stderr)`). Release builds emit a warning:

```
warning: unused import: `tracing_subscriber::fmt::writer::MakeWriterExt`
  --> crates/cue-core/src/logging.rs:17:5
```

Pipeline `clippy --all-targets -- -D warnings` would catch this in
release-mode CI. Suggested fix: `#[cfg_attr(not(debug_assertions), allow(unused_imports))]` or pull the import inside the cfg block.

**This is in your territory (cue-core). Please fix as a tiny followup.**

### N-2 🟡 No max-file-size cap, daily rotation only

If a single day generates 1+ GB of logs, that day's file gets large.
Not an issue for v0.2 alpha but worth tracking. `tracing-appender`
supports byte-size rotation in newer versions; could add a soft cap
in a future round.

### N-3 🟡 First log emission contains the log dir path

`tracing::info!(log_dir = %log_dir.display(), "local log rotation initialized")`
emits the absolute log dir path. On macOS this is typically
`/Users/<user>/Library/Logs/Bluey/`, which contains the user's account
name. The log-tail in `bluey doctor` would echo that path out to a
support ticket. The Phase 4 redactor doesn't strip path-like fields
because they're context for support, but `<user>` is now in the
support bundle.

Suggested mitigation: redact `/Users/<user>/` to `/Users/<user>/`
in the Phase 4 redactor (next round), OR strip the `log_dir` field
from the init emission (this round). I lean toward the redactor
update because account names also appear in other paths (config dir,
data dir). **Tracking as a Phase 6 followup, not blocking Phase 2.**

## Pipeline State

Commands run at tip `fdf3611`:

```bash
cargo fmt --all --check                         ✅ clean
cargo clippy --all-targets -- -D warnings       ✅ clean (workspace + server)
cargo test --all-targets                        ✅ 465 passed (was 459 — +6 from logging tests)
cargo test -p cue-core logging::                ✅ 3 passed (focused)
cd server && cargo test                         ✅ 90 passed
```

## Phase 4 ↔ Phase 2 Integration

Two real bugs in MY territory (Phase 4) found by smoke testing
against the new Phase 2 file output:

### Bug 1: `bluey doctor` log-tail hardcodes `~/Library/Logs/Bluey/`

`crates/cue-cli/src/doctor.rs::log_dir_for_doctor()` does NOT consult
`BLUEY_LOG_DIR` or `CUE_LOG_DIR`. So `bluey doctor` won't pick up
logs from a customer who set the env var.

Smoke output:
```
$ BLUEY_LOG_DIR=/tmp/bluey-phase2-smoke3 ./target/release/bluey doctor
...
── Daemon Logs (tail) ──
  status        : no log directory at /Users/uno/Library/Logs/Bluey
                  ← WRONG: should report /tmp/bluey-phase2-smoke3
                            and tail the file there
```

### Bug 2: `bluey logs export` log_dir() has the same problem

`crates/cue-cli/src/logs.rs::log_dir()` mirrors the doctor's hardcoded
path. Same bug.

### Fix

Both should call `cue_core::local_log_dir()` directly. Phase 1 made
`local_log_dir()` `pub`, so this is a one-line change per file.

**Action: I will commit this fix as `fix(cue-cli): doctor + logs use
cue_core::local_log_dir for Phase 2 integration` immediately after
this verdict lands. ~5 LOC, single commit, my territory.**

This is not a Phase 2 blocker because Phase 2 is correct on its
own; the bug is in Phase 4. I noted it as a followup in the
Phase 4 handoff §6 ("when Phase 2 lands rotation, Phase 4 should
'just work'") — that prediction was wrong; doctor and logs need
the env-var-respecting refactor.

## Recommended Action

1. **Phase 2 closes 🟢.** No fix round on Phase 2.
2. **Codex N-1 fix on `cue-core/src/logging.rs:17`** unused import — ~1 LOC.
3. **Kiro fix(cue-cli): doctor + logs export call `cue_core::local_log_dir()`** — small followup, ~5 LOC; ships immediately.
4. **Kiro followup migration** of `cue-daemon/src/app.rs:6996` `session = %session_id` → `session_id = %session_id` — now unblocked since codex Phase 2 commit is in. Will commit alongside the doctor/logs fix.
5. **Codex moves to Phase 5** (trace propagation through Tauri invoke + IPC) or **Phase 3** (overlay lifecycle + frontend error capture).

## Round-close

This verdict closes Phase 2. No fix round needed on Phase 2 itself; my own follow-up commits will land separately on top of this.
