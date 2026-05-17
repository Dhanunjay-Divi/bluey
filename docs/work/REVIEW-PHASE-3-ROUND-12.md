# REVIEW: Phase 3 Round 12 — R11 Carry-Over Nits + GA Readiness

**Commit range:** `bccd05c..f1dc3d2`
**Reviewer:** Codex
**Date:** 2026-05-17

## Per-Task Review

### R12.1 — Responses Final-Chunk Handling

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/routes/Responses.tsx`, `crates/cue-dashboard/ui/src/routes/responseReducer.ts`, `crates/cue-dashboard/ui/src/routes/responseReducer.test.ts`, `crates/cue-dashboard/ui/package.json`, `crates/cue-dashboard/ui/package-lock.json` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 The final streaming delta bug is fixed. `applyChunk` appends `partial_text` before applying `finished`, so providers that send non-empty terminal chunks no longer lose the last token.
- 🟢 Moving the logic into a pure reducer is the right shape for this UI path. It gives us deterministic behavior for chunk ordering, response-id isolation, kind preservation, immutability, and final cleanup without relying on a mounted Tauri window.
- 🟢 The 13 Vitest cases cover the regression that caused the original R11 nit: `finished:true` with non-empty `partial_text`.

---

### R12.2 — Shared Overlay UI State

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/tests/overlay_production_path.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The production daemon and reader now share a single `Arc<Mutex<OverlayUiState>>`, so modal-state changes made by handlers are visible to `validate_and_decode_overlay_line`. This resolves the earlier split-state problem.
- 🟢 The production-path tests now prove the intended state transitions: attach open admits `AttachFilesRequested`, submit returns to idle, instructions open admits `InstructionsUpdated`, and the shared state is visible cross-thread.
- 🟡 `AttachRequested` and `InstructionsRequested` set the shared state to `AttachOpen` / `InstructionsOpen` before calling the daemon-owned dialog handlers, but the state only returns to `Idle` when the follow-up submit event is handled. If the dialog is cancelled or errors before submit, the gate can remain permissive for that modal event type. Current impact is limited because the daemon-owned handlers drive the normal submit path, but Round 13 should either reset state in a `finally`-style guard after the request handler returns or explicitly model a cancellable/open modal lifecycle.

---

### R12.3 — Random Overlay Session Token

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/overlay.rs`, `crates/cue-daemon/Cargo.toml`, `Cargo.toml`, `Cargo.lock` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `generate_session_token()` now draws 32 bytes from OS entropy and hex-encodes them as a 64-character token. That matches the R11 claim of a true 256-bit random token.
- 🟢 The tests cover shape, uniqueness, and the prior UUID-v4 fixed-nibble bias. The bias test is a good regression guard for the exact issue called out in R11.
- 🟡 Non-blocking: `expect("OS random source unavailable")` is acceptable for this process-local handshake because there is no secure fallback, but a future hardening round could return `Result<String>` and fail overlay startup gracefully instead of panicking.

---

### R12.4 / R12.5 — Deferred Work

| Field | Value |
|-------|-------|
| Files | `docs/work/PHASE-3-ROUND-12-HANDOFF-FOR-CODEX-REVIEW.md` |
| Verdict | 🟢 accept deferral |

**Findings:**
- 🟢 Deferring sqlite-vec / ANN RAG is reasonable. It is a product capability, not a blocker for these R11 carry-over correctness fixes.
- 🟢 Deferring Windows real whisper.cpp remains reasonable if Windows bench access is the blocker. Keep Windows whisper clearly documented as stub/fallback until it is tested on hardware.

## Cross-Task Findings

- 🟡 Release-scope nit: the R12 handoff says v0.1.0 today only ships macOS arm64, but public-facing install/product surfaces still advertise broader platform support. `INSTALL.md` lists darwin-arm64, darwin-x86_64, linux-x86_64, and windows-x86_64 release artifacts, and `web/index.html` describes macOS and Windows native helpers. Before tagging v0.1.0 GA, either produce and smoke-test the full matrix or narrow the docs/site/release notes to the artifact set that actually ships.
- 🟡 Release-scope nit: the distribution note in `docs/work/PHASE-3-ROUND-12-HANDOFF-FOR-CODEX-REVIEW.md` states that CLI binaries fetched from terminal are not subject to Gatekeeper and that overlay child processes bypass quarantine. That is too absolute for release documentation. Terminal/brew distribution can reduce friction, but the GA docs should say signing/notarization are deferred and require clean-machine validation, not promise a platform security bypass.
- 🟢 Telemetry counters for rejected overlay-reader lines should be a separate Round 13 item. Logging is enough for this R12 merge; counters become more valuable once there is a production telemetry sink and privacy policy.
- 🟢 A `scripts/install.sh` is not required for R12, but if the intended GA call-to-action is `curl | sh`, it should be added and tested before public launch. Otherwise publish tarball/manual or brew-only instructions for v0.1.0.

## Build & Test Verification

```bash
cargo fmt --all --check                                             # ✅
cargo clippy --all-targets -- -D warnings                           # ✅
cargo build --all-targets --release                                 # ✅
cargo test --all-targets                                            # ✅ 361 passed, 14 ignored
cd crates/cue-dashboard/ui && npm test                              # ✅ 13 passed
cd crates/cue-dashboard/ui && npm run build                         # ✅
swift build -c release --package-path native/macos/cue-overlay      # ✅
swift build -c release --package-path native/macos/cue-whisper      # ✅
git diff --check main..HEAD                                         # ✅
```

Additional targeted verification:

```bash
cargo test -p cue-daemon --test overlay_production_path handler_transition -- --nocapture        # ✅ 2 passed
cargo test -p cue-daemon --test overlay_production_path instructions_handler -- --nocapture      # ✅ 1 passed
cargo test -p cue-daemon --test overlay_production_path cross_thread_arc_visibility -- --nocapture # ✅ 1 passed
cargo test -p cue-daemon overlay::tests::token --lib                                            # ✅ 3 passed
```

## Overall Verdict

🟡 **ACCEPT WITH NITS** — R12 implementation is mergeable, but I would not drop the `-alpha` suffix and tag v0.1.0 GA until the release/support-matrix wording is corrected.

There are no R12 code blockers. The reducer fix, shared overlay state, and 256-bit token fix all address the carry-over nits. The remaining issues are release-readiness nits: platform support claims and the overly broad Gatekeeper/quarantine language.

## Follow-ups for Next Batch

- Before GA: choose the v0.1.0 support matrix explicitly. Either ship/test macOS arm64 + macOS x86_64 + Windows + Linux artifacts, or scope v0.1.0 docs/site/install instructions to macOS arm64.
- Before GA: soften the terminal-only distribution note so it says signing/notarization are deferred and must be validated on a clean Mac, rather than claiming Gatekeeper/quarantine bypass.
- Round 13: reset `overlay_ui_state` to `Idle` on attach/instructions dialog cancellation/error, or make the modal lifecycle explicit if the overlay will own those submit events later.
- Round 13: add optional counters for production overlay-reader rejections once telemetry/log aggregation exists.
- Future distribution: add a tested `scripts/install.sh` only if `curl | sh` is the intended primary install path.

## Recheck 1 — Commit `27dc114`

**Date:** 2026-05-17

**Verdict:** 🔴 **REQUEST CHANGES**

The two original R12 release-scope nits are substantively fixed:

- 🟢 Platform support is now scoped to macOS arm64 in `INSTALL.md`, `web/index.html`, and the release notes. Windows/Linux/Intel are described as future work instead of supported platforms.
- 🟢 Gatekeeper/quarantine wording is now conservative. It describes signing/notarization as deferred, calls out quarantine as environment-dependent, and documents `xattr -d com.apple.quarantine` as remediation rather than promising bypass behavior.

Remaining blockers before tagging `v0.1.0` GA:

- 🔴 `git diff --check main..HEAD` fails on `docs/release/RELEASE-v0.1.0-alpha.md:3` because of trailing whitespace. This means the stated pipeline is not green on the reviewed tip.
- 🔴 The branch is not ready to tag as `v0.1.0` GA on the same tip because release-facing docs still point to alpha artifacts and alpha audience language: `INSTALL.md` downloads `bluey-0.1.0-alpha-macos-arm64.tar.gz`, and `docs/release/RELEASE-v0.1.0-alpha.md` still says `v0.1.0-alpha`, `internal alpha only`, `Tag: v0.1.0-alpha`, and lists the alpha tarball name. Either keep this as a narrowed alpha release, or make the GA naming/docs consistent before tagging `v0.1.0`.

Additional nit:

- 🟡 `docs/work/FIX-PHASE-3-ROUND-12.md` says tip `27af6b5`, but the reviewed branch tip is `27dc114`. Update the handoff text so the next agent is not chasing a non-tip commit.

Verification for the recheck:

```bash
git diff --check main..HEAD          # ❌ trailing whitespace in docs/release/RELEASE-v0.1.0-alpha.md:3
git diff --check 27dc114^..27dc114   # ❌ same trailing whitespace
```

Fix should be tiny: remove the trailing spaces and decide whether this branch is still an alpha-support cleanup or the actual GA tag source. If it is the GA tag source, update install/release artifact names and audience/tag wording to `v0.1.0` before re-handing.

## Recheck 2 — Commit `5c6cac7`

**Date:** 2026-05-17

**Verdict:** 🟡 **ACCEPT WITH NITS**

R12 implementation and the original R12 doc-scope fixes are accepted. The branch is now clean as a macOS-arm64 alpha/support-scope cleanup, but it still should not be tagged `v0.1.0` GA until the user makes the release-identity call and any required alpha-to-GA rename pass lands.

Resolved since Recheck 1:

- 🟢 `git diff --check main..HEAD` is clean. The trailing whitespace in `docs/release/RELEASE-v0.1.0-alpha.md` is gone.
- 🟢 `docs/work/FIX-PHASE-3-ROUND-12.md` no longer references the stale `27af6b5` SHA.
- 🟢 The support matrix and Gatekeeper/quarantine wording remain correctly narrowed/conservative.

Remaining release nit:

- 🟡 The branch still intentionally points at alpha artifacts and alpha audience wording (`bluey-0.1.0-alpha-macos-arm64.tar.gz`, `v0.1.0-alpha`, `internal alpha only`). That is fine if the user chooses to leave this as alpha-scoped cleanup. If the user wants to tag `v0.1.0` GA from this line, do one final rename/release-note pass first.

Verification for the recheck:

```bash
git diff --check main..HEAD             # ✅
git diff --check 27dc114..HEAD          # ✅
cargo fmt --all --check                 # ✅
cargo clippy --all-targets -- -D warnings # ✅
cargo test --all-targets                # ✅ 361 passed, 14 ignored
cd crates/cue-dashboard/ui && npm test  # ✅ 13 passed
```

Next decision is product/release identity, not implementation correctness:

- If staying alpha: merge this cleanup, keep `v0.1.0-alpha`, do not tag GA.
- If going GA: update install/release artifact names and audience/tag wording to `v0.1.0`, then hand back for one last string-only recheck.
