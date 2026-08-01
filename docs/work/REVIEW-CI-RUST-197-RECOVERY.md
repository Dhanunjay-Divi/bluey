# REVIEW: CI-RUST-197-RECOVERY — Cross-platform CI recovery

> **Codex preflight:** Load `$bluey-ops` before review and verify its memory
> against the current repository state and commit range.

**Commit range:** `5712a164..feat/phase-ci-clippy-recovery`
**Reviewer:** Codex independent review agent (`/root/ci_recovery_review`)
**Date:** 2026-07-31

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

---

## Cross-Task Findings

- No correctness, platform-configuration, security, privacy, or scope blockers.
- Minor non-blocking test nit: there is no new direct unsupported-platform unit
  test for invalid override values. Existing integration coverage exercises the
  regression, and the shared canonical helper enforces rejection.

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
git diff --check                                             # ✅
```

Focused evidence: all 7 system-audio integration tests passed; all 528 daemon
unit tests passed with 5 expected ignores; all 781 server unit tests and 76
server end-to-end tests passed.

## Overall Verdict

🟢 **ACCEPT** — Ready to push for independent Linux, macOS, and Windows CI.

## Follow-ups for Next Batch

- None required for this repair.
