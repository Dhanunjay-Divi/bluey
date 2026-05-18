# Codex -> Kiro: R12 Pill Regression + R13 Production Hardening + Docs/UX Audit

## 1. Overall Verdict

🟢 **ACCEPT / IMPLEMENTED** — Codex kept the R12 pill-regression fix, then moved the highest-risk R13 production items forward in the same worktree:

- restored compact pill-first `bluey on` startup;
- fixed overlay modal state cleanup on cancel/error/success;
- changed overlay session-token generation to return `Result`;
- aligned macOS arm64 release packaging, helper discovery, and install docs;
- added a tested `scripts/install.sh` path for local archives / release archives;
- added a production-readiness source of truth and updated stale product docs;
- started a reference-informed native UI polish pass and wrote the UX direction
  brief.

## 2. Round Verdict

- R12 overlay-pill regression: 🟢 **ACCEPTED after Codex follow-up**
- R13.1 overlay UI state reset: 🟢 **IMPLEMENTED**
- R13.2 session token `Result`: 🟢 **IMPLEMENTED**
- R13.7 install/package alignment: 🟢 **PARTIALLY IMPLEMENTED**
  - local archive installer smoke passes;
  - clean-machine validation is still required before `curl | sh` becomes public primary install.
- R13 documentation/readiness audit: 🟢 **IMPLEMENTED**
  - `docs/PRODUCTION-READINESS.md` is now the current implementation matrix;
  - stale active docs no longer claim streaming, VAD, STT routing, or the inline
    composer are future work.
- R13 UI/UX direction pass: 🟡 **STARTED**
  - reference apps were inspected for actual UI patterns;
  - macOS native overlay received a low-risk product-surface polish;
  - remaining markdown/code/copy/status-chip work is documented for the next UI
    round.

## 3. What Codex Changed

### Pill-first overlay UX

- `crates/cue-cli/src/app.rs`
  - `bluey on` no longer sends `OverlayShow`, because `show` expands the full panel. Startup now launches the native overlay pill first and sends `OverlayBoot` to seed the hidden feed.
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - collapsed pill is now `146x32`, compact, dark-blue glass, with a Swift-drawn Bluey mark, subtle cyan border/glow, status dot, `Bluey` wordmark, and chevron.

### R13.1 overlay state cleanup

- `crates/cue-daemon/src/app.rs`
  - Added an `OverlayUiStateScope` guard.
  - `AttachRequested` and `InstructionsRequested` enter modal state while the daemon-owned picker/prompt is open and always reset to `Idle` when the handler returns.
  - `AttachFilesRequested` and `InstructionsUpdated` also reset to `Idle` on scope exit, so errors do not leave the production reader gate permissive.
  - Added unit coverage for state guard enter/reset behavior.

### R13.2 token hardening

- `crates/cue-daemon/src/overlay.rs`
  - `generate_session_token()` now returns `Result<String, getrandom::Error>`.
  - Daemon startup propagates entropy failure with normal `anyhow` context instead of panicking.
  - Token tests and overlay IPC tests now explicitly unwrap token generation.

### Release/install production path

- `.github/workflows/release.yml`
  - v0.1.0 release matrix narrowed to macOS arm64 only, matching the documented support matrix.
  - Removed dashboard/Tauri build and updater manifest from the terminal-only release path.
  - macOS helper build failures now fail the release job instead of being swallowed by `|| true`.
  - Packager copies both `bluey-*` and `cue-*` helper aliases into `staging/bin`.
- `Makefile`
  - Terminal package targets build CLI/daemon only; dashboard remains a separate dev-tool target.
  - `package-darwin-arm64` now includes overlay, audio, and whisper helpers.
- `native/macos/cue-whisper/build.sh`
  - Produces `.build/cue-whisper` and `.build/bluey-whisper-macos` for stable packaging.
- `scripts/build-macos.sh`
  - Copies both whisper helper names into the local dist folder.
- `scripts/install.sh`
  - New macOS arm64 installer.
  - Supports `BLUEY_ARCHIVE` for local tarball validation.
  - Supports release download with `SHA256SUMS.txt` verification.
  - Installs to `~/.local/bluey/<version>` with symlinks in `~/.local/bin`.
- `INSTALL.md` and `docs/release/RELEASE-v0.1.0.md`
  - Updated artifact naming to `bluey-0.1.0-darwin-arm64.tar.gz`.
  - Documented `SHA256SUMS.txt` / `sha256-manifest.json`.

### Installed-helper discovery

- `crates/cue-daemon/src/app.rs`
  - Overlay helper discovery now checks both the symlink directory and the canonical daemon executable directory, so `~/.local/bin/bluey` symlinks work.
  - Release-mode overlay verification uses the canonical daemon path as the install root.
- `crates/cue-daemon/src/audio/system_capture.rs`
  - System-audio helper discovery also checks canonical daemon sibling paths and `cue-*` aliases.
- `crates/cue-daemon/src/stt/whisper/mod.rs`
  - Local whisper discovery checks canonical daemon sibling paths, `cue-whisper`, and `bluey-whisper-macos`.

### Production-readiness docs sweep

- `docs/PRODUCTION-READINESS.md`
  - New source of truth for v0.1.0 scope, implemented features, and remaining
    paid-product gaps.
  - Calls the current release a macOS arm64 local-first product candidate, not a
    finished SaaS.
- Updated active product docs:
  - `README.md`
  - `docs/ROADMAP.md`
  - `docs/DEPLOYMENT-SCALING.md`
  - `docs/INSTALLER-CHECKLIST.md`
  - `docs/IMPLEMENTATION-SEAMS.md`
  - `docs/SELF-REVIEW.md`
  - `docs/ICON-GUIDE.md`
  - `docs/HANDOFF.md`
  - `docs/PRE-PRICING-REVIEW.md`
  - `docs/COMMERCIAL-PATH.md`
  - `docs/COMPETITIVE-GAPS.md`
  - `docs/REFERENCE-MAP.md`
  - `docs/SESSION-FLOW.md`
  - `docs/CLOUD-RAG.md`
  - `docs/FIRST-VERSION-TEST.md`
  - `docs/PRODUCT-STRATEGY.md`
  - `docs/FEATURE-MAP.md`
  - `docs/AI-COMPETITIVE-STUDY.md`
  - `docs/work/AGENT-ONBOARDING.md`
  - `docs/WORKLOG.md`
- Historical review/design docs under `docs/reviews` and old phase records were
  left intact as audit records.

### Reference-informed UI pass

- `docs/UX-UI-PRODUCT-DIRECTION.md`
  - New design brief covering reference lessons, visual direction, overlay
    architecture, card system, required user states, current status, and next UI
    slice.
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Expanded panel now has a darker product surface, top status strip,
    composer capsule, equal-width icon-led action buttons, styled close button,
    and role-labeled cards with accent rails.
  - This is intentionally low-risk: it keeps the existing NDJSON protocol and
    event names (`ask_requested`, `attach_requested`, `instructions_requested`,
    `recap_requested`) unchanged.
- `native/macos/cue-overlay/build.sh`
  - Mirrors the built overlay helper into existing `target/debug` and
    `target/release` directories. This preserves the hardened production
    verifier's side-by-side helper requirement while making local
    `./target/release/bluey on` smoke tests work after the native build step.
- `crates/cue-cli/src/app.rs`
  - Plain `bluey on` now opens Bluey's launcher state without silently creating
    a new session. `bluey on --title ...` still creates a titled session, which
    preserves scripted and smoke-test flows.
- `crates/cue-daemon/src/app.rs`
  - `Continue` now restores the latest saved meeting when no active meeting is
    loaded, instead of always creating a brand-new session.
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Follow-up launch-flow polish: smaller `112x28` pill, visible `New` and
    `Continue` session controls, `Analyse` action, and empty `Answer` now asks
    for the latest useful session context.
  - Added a parent-process watchdog so future orphaned overlay helpers exit if
    the daemon disappears unexpectedly.

## 4. Remaining Blockers

No blocker remains for macOS arm64 v0.1.0 terminal packaging or pill-first startup.

Still not completed from the broader product backlog:

- R13.3 sqlite-vec / ANN RAG.
- R13.4 Windows real whisper.cpp.
- R13.5 wider platform matrix and clean-machine QA.
- R13.6 telemetry counter for overlay-reader rejections, gated on telemetry/privacy decision.
- Clean-machine validation of `scripts/install.sh` before making `curl | sh` public primary install.
- Native overlay markdown/code rendering, answer/code copy controls, status
  chips, attachment drawer, recent-session picker, warning-card patterns, and
  visual regression harness.
- Cloud auth/device registration, encrypted sync, managed provider router,
  billing/plans, admin, and cloud RAG.
- Production OCR/vision artifact pipeline with citations and visible status.
- Crash diagnostics/support bundle and user-facing health view.

## 5. What Codex Did Not Change

- No remote push.
- No history rewrite.
- No naming/product-wording cleanup beyond release artifact/support-matrix accuracy.
- No Windows whisper.cpp implementation; still blocked on Windows bench validation.
- No cloud/billing/auth production service work; credentials and deployment plan still needed.

## 6. Verification Run

```bash
cargo fmt --all --check                                             # ✅
cargo clippy --all-targets -- -D warnings                           # ✅
cargo build --all-targets --release                                 # ✅
cargo test --all-targets                                            # ✅ 363 passed, 14 ignored
cd crates/cue-dashboard/ui && npm test && npm run build             # ✅ 13 passed + build
swift build -c release --package-path native/macos/cue-overlay      # ✅
bash native/macos/cue-overlay/build.sh                              # ✅
swift build -c release --package-path native/macos/cue-whisper      # ✅
bash native/macos/cue-whisper/build.sh                              # ✅
bash scripts/build-macos.sh                                         # ✅ dist/bluey-macos-arm64
bash -n scripts/install.sh                                          # ✅
git diff --check main..HEAD && git diff --check                     # ✅
make package-darwin-arm64                                           # ✅
BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-arm64.tar.gz scripts/install.sh # ✅ with temp install dirs
scripts/smoke-test.sh                                               # ✅
```

Local release smoke:

```bash
./target/release/bluey on                                           # ✅ starts daemon + overlay
pgrep -fl 'bluey|bluey-overlay'                                     # ✅ bluey-daemon + bluey-overlay-macos
./target/release/bluey off                                          # ✅ stops both
```

Installed-path smoke:

```bash
tmp_install=$(mktemp -d)
BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-arm64.tar.gz \
  BLUEY_INSTALL_DIR="$tmp_install/bluey" \
  BLUEY_BIN_DIR="$tmp_install/bin" \
  scripts/install.sh
"$tmp_install/bin/bluey" on
# ✅ bluey-daemon starts from symlink path
# ✅ bluey-overlay-macos starts from canonical versioned install dir
"$tmp_install/bin/bluey" off
```

Observed processes during the installed-path smoke:

```text
.../tmp.../bin/bluey-daemon
.../tmp.../bluey/0.1.0/bin/bluey-overlay-macos
```

## 7. Next Action for Kiro

Review this worktree and commit it as one production-hardening commit or split it into two commits:

```bash
git add .github/workflows/release.yml INSTALL.md Makefile \
  crates/cue-cli/src/app.rs \
  crates/cue-daemon/src/app.rs \
  crates/cue-daemon/src/audio/system_capture.rs \
  crates/cue-daemon/src/overlay.rs \
  crates/cue-daemon/src/stt/whisper/mod.rs \
  crates/cue-daemon/tests/overlay_lifecycle.rs \
  crates/cue-daemon/tests/overlay_pipe_integration.rs \
  crates/cue-daemon/tests/overlay_restart_integration.rs \
  docs/release/RELEASE-v0.1.0.md \
  docs/PRODUCTION-READINESS.md \
  docs/AI-COMPETITIVE-STUDY.md \
  docs/CLOUD-RAG.md \
  docs/COMMERCIAL-PATH.md \
  docs/COMPETITIVE-GAPS.md \
  docs/DEPLOYMENT-SCALING.md \
  docs/FEATURE-MAP.md \
  docs/FIRST-VERSION-TEST.md \
  docs/HANDOFF.md \
  docs/ICON-GUIDE.md \
  docs/IMPLEMENTATION-SEAMS.md \
  docs/INSTALLER-CHECKLIST.md \
  docs/PRE-PRICING-REVIEW.md \
  docs/PRODUCT-STRATEGY.md \
  docs/REFERENCE-MAP.md \
  docs/ROADMAP.md \
  docs/SELF-REVIEW.md \
  docs/SESSION-FLOW.md \
  docs/UX-UI-PRODUCT-DIRECTION.md \
  docs/WORKLOG.md \
  docs/work/AGENT-ONBOARDING.md \
  docs/work/FIX-PHASE-3-OVERLAY-PILL-REGRESSION.md \
  docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md \
  docs/work/PHASE-3-ROUND-13-PLAN.md \
  docs/work/REVIEW-PHASE-3-ROUND-12.md \
  native/macos/cue-overlay/Sources/cue-overlay/main.swift \
  native/macos/cue-overlay/build.sh \
  native/macos/cue-whisper/build.sh \
  scripts/build-macos.sh \
  scripts/install.sh

git commit -m "fix(release): harden macOS arm64 terminal install path"
```

Suggested next implementation round:

```text
Phase 3 Round 13 continuation:
1. Clean-machine validate scripts/install.sh on a fresh Apple Silicon Mac.
2. Decide if v0.1.0 GA remains macOS arm64-only or if macOS x86_64 is required before tag.
3. If GA stays arm64-only, tag v0.1.0 after clean-machine install + bluey on/off smoke.
4. Start R13.3 sqlite-vec / ANN RAG as the next product capability round.
```
