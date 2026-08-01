# REVIEW: CI-RUST-197-RECOVERY — Cross-platform CI recovery

> **Codex preflight:** Load `$bluey-ops` before review and verify its memory
> against the current repository state and commit range.

**Commit range:** `5712a164..feat/phase-ci-clippy-recovery`
**Reviewer:** Codex independent review agent (`/root/ci_recovery_review`)
**Date:** 2026-08-01

## Per-Task Review

### Rust 1.97 warning gate

| Field | Value |
|-------|-------|
| Files | `app.rs`, `system_capture.rs`, `whisper/mod.rs` |
| Verdict | 🟢 accept |

**Findings:**

- Platform-only imports and functions use the same target conditions as their
  callers.
- The expression cleanup is behavior-preserving.

### Linux system-audio integration gate

| Field | Value |
|-------|-------|
| Files | `system_capture.rs`, existing `system_audio_integration.rs` coverage |
| Verdict | 🟢 accept |

**Findings:**

- Unsupported debug builds accept only an explicit helper override after
  canonical file and executable validation.
- Unsupported release builds still compile to no helper, while macOS and
  Windows discovery remain unchanged.

### Jobs observability policy gate

| Field | Value |
|-------|-------|
| Files | `server/src/rate_limit.rs` |
| Verdict | 🟢 accept |

**Findings:**

- The warning now uses the canonical hashed account support key without
  changing authentication, bucket keys, or limiter behavior.

### Intentional audio-stop cancellation

| Field | Value |
|-------|-------|
| Files | `system_capture.rs`, `FIX-029-audio-stop-ci-flake.md` |
| Verdict | 🟢 accept |

**Findings:**

- Stop notification aborts the diagnostic reader before bounded helper reap,
  then awaits task cancellation and returns the intentional `Clean` result.
- Ordinary helper exits still drain and validate diagnostic output.
- The deterministic direct-child fixture and public stop deadline remove the
  loaded-runner timing flake without weakening production time bounds.

### GitHub Actions capacity and trigger recovery

| Field | Value |
|-------|-------|
| Files | `ci.yml`, `jobs-ci.yml`, `observability-policy.yml` |
| Verdict | 🟢 accept |

**Findings:**

- Feature updates run through one base-unrestricted pull-request event, so
  stacked Phase branches retain the full matrix without duplicate push runs.
- Workspace and server suites run on separate clean runners with distinct
  cache keys and target roots.
- The legacy `tests` context is an `always()` aggregate and cannot pass unless
  both isolated suites succeed, preserving required-check semantics.

---

## Cross-Task Findings

- No correctness, platform-configuration, security, privacy, or scope blockers.
- Minor non-blocking test nits: there is no new direct unsupported-platform
  unit test for invalid override values, and the direct `sleep` fixture does
  not independently model a descendant-held stderr pipe. Existing integration
  and public-deadline coverage exercise the regressions, while the shared
  canonical helper and simple abort path enforce the reviewed behavior.

## Build & Test Verification

```bash
cargo fmt --all --check                                      # ✅
cargo clippy --all-targets -- -D warnings                    # ✅
cargo build --all-targets --release                          # ✅
cargo test --all-targets                                     # ✅
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings  # ✅
cargo test --manifest-path server/Cargo.toml                 # ✅
python3 scripts/analyze-tracing-calls.py --check-only        # ✅
bash scripts/check-bluey-ops-docs.sh                         # ✅
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.7 \
  .github/workflows/ci.yml .github/workflows/jobs-ci.yml \
  .github/workflows/observability-policy.yml                 # ✅
git diff --check                                             # ✅
```

Focused evidence: all 7 system-audio integration tests passed; all 528 daemon
unit tests passed with 5 expected ignores; all 781 server unit tests and 76
server end-to-end tests passed. The exact stalled-helper cancellation test also
passed 50 consecutive repetitions.

## Overall Verdict

🟢 **ACCEPT** — Ready to push for independent Linux, macOS, Windows,
Jobs privacy, and isolated observability CI.

## Follow-ups for Next Batch

- None required for this repair.
