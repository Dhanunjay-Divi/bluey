# Codex → Kiro: Phase 3 Round 11 Review Handoff

## 1. R11 Verdict

🔴 **REQUEST CHANGES**

Review written to: `docs/work/REVIEW-PHASE-3-ROUND-11.md`

Blockers:
- Streaming chunks are still wrong in the UI. The daemon emits cumulative text while `Responses.tsx` appends each payload, so live responses duplicate text.
- Overlay hardening is not wired into the production daemon path. `app.rs` still starts/restarts the overlay through the legacy `spawn_overlay()` function, bypassing the new gated resolver, binary verification, session token, state machine, and length checks.
- The legacy production overlay path still accepts `BLUEY_OVERLAY_BIN` / `CUE_OVERLAY_BIN` without requiring `BLUEY_DEV_OVERLAY=1`.
- The new `NativeOverlayHandle` reader validates token only; it does not enforce command length limits or `OverlayUiState`.
- The Windows native overlay still emits tokenless JSON events, so the documented cross-platform token handshake is incomplete.

## 2. R10 Verdict Status

🔴 **Still blocked by the R11 fix wave**

Good R10 fixes landed:
- Streaming auth header names are obfuscated.
- SwiftWhisper is pinned to `.exact("1.2.0")`.
- PCM16 decode uses `loadUnaligned`.

Still not closed:
- The user-facing streaming fix is incomplete because the dashboard appends cumulative text. R10 should remain blocked until the streaming payload/UI contract is fixed and covered by a UI/reducer test.

## 3. Older Pending Verdicts

- R7-fix-3 release workflow checkout/script blocker: 🟢 **ACCEPT** based on the checkout before manifest generation in `.github/workflows/release.yml`.
- R8 fix-wave: 🟡 **ACCEPT WITH NITS** from the prior review remains valid.
- R9 full feature review: not completed in this pass; I prioritized R11 because it is the current requested review and contains security-critical claims.

## 4. What I Implemented

No product code changes.

Documentation changes only:
- Added `docs/work/REVIEW-PHASE-3-ROUND-11.md`.
- Overwrote this handoff with the R11 verdict and current pending-status summary.

## 5. What I Skipped and Why

- Product naming/white-label/runtime wording: intentionally not revisited per user direction.
- R9 full review: skipped because R11 has merge-blocking production/security issues that should be fixed before further stacked review.
- Additional feature implementation: skipped because this was a review request, and the branch is not merge-ready.

## 6. Pipeline Status

Checks run locally on `feat/phase-3-round-11`:

```bash
cargo fmt --all --check                              # ✅
cargo clippy --all-targets -- -D warnings            # ✅
cargo build --all-targets                            # ✅
cargo test --all-targets                             # ✅ 331 passed, 14 ignored locally
cd crates/cue-dashboard/ui && npm run build          # ✅
cargo test -p cue-daemon --test cue_streaming_integration
                                                       # ✅ 5 passed
cargo test -p cue-daemon --test overlay_security_integration --test overlay_pipe_integration
                                                       # ✅ 13 passed
cd native/macos/cue-overlay && swift build            # ✅
cd native/macos/cue-whisper && swift build            # ✅
cd native/macos/cue-audio && swift build              # ✅
git diff --check feat/phase-3-round-10..HEAD          # ✅
```

## 7. Next Action for Kiro

Do not merge R11 yet.

Recommended fix order:
1. Fix streaming UI semantics: choose delta or cumulative and make daemon/dashboard agree.
2. Move production overlay startup/restart onto the hardened overlay path, or port all R11 hardening into the actual `app.rs::spawn_overlay()` path.
3. Enforce token + field limits + state machine in the daemon receiver, with production-path integration tests.
4. Add Windows overlay token emission and a fixture/integration test for the Windows event envelope.
5. Re-hand as an R11 fix wave with focused tests proving the production path, not only helper modules.
