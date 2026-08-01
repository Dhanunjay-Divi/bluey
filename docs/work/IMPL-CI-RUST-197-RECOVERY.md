# IMPL: CI-RUST-197-RECOVERY — Cross-platform CI recovery

> **Codex preflight:** Load `$bluey-ops` before implementation and verify its
> memory against the current repository state.

## Scope

**Does:**

- Restore warning-free Rust 1.97 builds across Linux, macOS, and Windows.
- Restore Linux debug integration coverage for the system-audio protocol stub.
- Keep intentional helper cancellation bounded under loaded CI scheduling.
- Keep workspace and server test artifacts on separate hosted runners and run
  feature-branch gates once through their required pull request.
- Bring the Jobs rate-limit diagnostic back under the observability privacy
  policy.

**Does NOT:**

- Enable system-audio capture on unsupported release targets.
- Change production audio-helper discovery on macOS or Windows.
- Change Jobs rate limits, authentication, or employer-facing automation.
- Build, publish, restart, or deploy a Bluey runtime artifact.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/app.rs` | Modified | Align command imports and formatting with Rust 1.97. |
| `crates/cue-daemon/src/audio/system_capture.rs` | Modified | Preserve validated debug-test stub discovery and release fail-closed behavior. |
| `crates/cue-daemon/src/stt/whisper/mod.rs` | Modified | Compile packaged resolver only where used. |
| `server/src/rate_limit.rs` | Modified | Hash the account observability field. |
| `.github/workflows/ci.yml` | Modified | Avoid duplicate feature-push and pull-request matrices. |
| `.github/workflows/jobs-ci.yml` | Modified | Avoid duplicate feature-push and pull-request privacy gates. |
| `.github/workflows/observability-policy.yml` | Modified | Isolate large test targets and avoid duplicate feature runs. |
| `docs/work/FIX-027-rust-197-clippy-gate.md` | Created | Record the compiler-gate diagnosis and repair. |
| `docs/work/FIX-028-linux-ci-policy-gates.md` | Created | Record the Linux test and privacy-policy repair. |
| `docs/work/FIX-029-audio-stop-ci-flake.md` | Created | Record the intentional-stop cleanup and test-fixture repair. |
| `docs/work/FIX-030-actions-disk-and-duplicate-runs.md` | Created | Record runner isolation and trigger deduplication. |
| `docs/work/REVIEW-CI-RUST-197-RECOVERY.md` | Created | Preserve the independent line-by-line review verdict. |
| `CHANGELOG.md` | Modified | Record user- and operator-visible correctness changes. |

## Build & Test

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test -p cue-daemon --all-targets
cargo test -p cue-daemon stalled_helper_read_is_interrupted_by_stop_notification
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml
python3 scripts/analyze-tracing-calls.py --check-only
bash scripts/check-bluey-ops-docs.sh
git diff --check
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Full Ubuntu tests exposed two gates after the initial Clippy repair. | Both were fixed in the same CI-recovery branch before merge. |

## Known Follow-ups

- None.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from the plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
