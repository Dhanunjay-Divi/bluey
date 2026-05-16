# REVIEW: Phase 3 Round 7 — Live Transcript + Whisper Fallback + Distribution

**Commit range:** `6126b28..1f5a829`
**Reviewer:** Codex
**Date:** 2026-05-16

## Per-Task Review

### R7.1 — Live Transcript UX

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/tests/live_transcript_emit.rs`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/routes/LiveTranscript.tsx`, `crates/cue-dashboard/ui/src/components/LiveTranscriptList.tsx`, `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/ui/src/components/Sidebar.tsx` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 The daemon-side `broadcast::Sender<LiveTranscriptEvent>` is currently not consumed anywhere. `add_audio_transcript_segment` sends on `live_transcript_tx`, but `rg "live_transcript_tx|subscribe_live"` shows no subscriber path in daemon IPC/Tauri, while the dashboard uses an independent file poller. This is not a runtime blocker because the file-polling path can still work, but it means commit `40b9340` is largely dormant infrastructure rather than an active event bridge.
- 🟡 The route catch-up and global poller can duplicate existing rows on startup. `LiveTranscript.tsx` calls `get_live_transcripts({ sinceIndex: 0 })` on mount, while the background poller in `crates/cue-dashboard/src/lib.rs:84-130` starts with `last_count = 0` and emits the same persisted rows on its first tick. A simple event de-dupe key or initializing the poller from the current active transcript count would avoid this.

---

### R7.2 — Local Whisper Fallback

| Field | Value |
|-------|-------|
| Files | `native/macos/cue-whisper/**`, `native/windows/cue-whisper/**`, `crates/cue-daemon/src/stt/whisper/**`, `crates/cue-daemon/src/stt/router.rs`, `crates/cue-daemon/src/bin/whisper_stub.rs`, `crates/cue-daemon/tests/whisper_integration.rs`, `crates/cue-daemon/src/app.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 LocalWhisper is not wired into the production STT router path. `build_system_audio_stt_provider()` only pushes Deepgram when an API key exists, then always pushes `EchoProvider` (`crates/cue-daemon/src/app.rs:1346-1367`). `rg "LocalWhisperProvider|BLUEY_STT_LOCAL_WHISPER" crates/cue-daemon/src` confirms the new provider is never constructed outside its own module/test seam. So setting `BLUEY_STT_LOCAL_WHISPER=1` cannot produce the documented Deepgram → OpenAI → LocalWhisper chain in the daemon.
- 🔴 The Windows native whisper helper does not compile. `native/windows/cue-whisper/main.c:46-47` has invalid `printf({"type":...}n);` expressions instead of quoted/escaped C strings. I verified with `clang -fsyntax-only native/windows/cue-whisper/main.c`, which fails with `expected expression` on both lines. This breaks the advertised Windows helper stub.
- 🟡 The R7 router test manually constructs `SttRouter::new(vec![p1, p2, Box::new(p3)])`, so it proves the trait object can sit behind the router but not that the daemon factory honors `BLUEY_STT_LOCAL_WHISPER=1` or that OpenAI is in the fallback chain. Add a production-factory test or expose a small provider-list builder that can be tested without real network calls.
- 🟡 `LocalWhisperProvider::close()` only drops the stdin sender and marks state closed; it does not await the helper task or force-kill a stuck helper. The native stubs exit on EOF, so this is acceptable for the stub round, but the checklist item "kills child process and waits" is not actually implemented.

---

### R7.3 — Distribution Scaffolding

| Field | Value |
|-------|-------|
| Files | `.github/workflows/release.yml`, `Makefile`, `infra/homebrew/bluey.rb`, `infra/scoop/bluey.json`, `infra/scripts/bump-formulae.sh`, `infra/scripts/download-whisper-model.sh`, `INSTALL.md`, `crates/cue-dashboard/tauri.conf.json` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The release and Makefile package commands reference a nonexistent CLI binary name. `crates/cue-cli/Cargo.toml:7-13` defines binaries `cue` and `bluey`; there is no `cue-cli` binary. But `.github/workflows/release.yml:89-90`, `.github/workflows/release.yml:103-104`, and `Makefile:32-43` package `cue-cli` / `cue-cli.exe`. On Windows the copy is silently ignored, so the zip misses `bluey.exe`; on Makefile packaging, `tar`/`zip` fail because the file does not exist.
- 🔴 The Homebrew formula does not match the GitHub release artifacts. The workflow produces `bluey-darwin-arm64.tar.gz` and packages renamed files `bluey-daemon`/`bluey` (`.github/workflows/release.yml:88-96`), but the formula downloads `bluey-#{version}-darwin-arm64.tar.gz` and installs `cue-daemon`/`cue-cli` (`infra/homebrew/bluey.rb:9-20`). A first brew install from the produced release will fail.
- 🟡 `cargo tauri build` is invoked from repo root in both `.github/workflows/release.yml:72-73` and `Makefile:10-26`, but this workspace keeps `tauri.conf.json` under `crates/cue-dashboard/`. Unless the Tauri CLI is explicitly passed the dashboard config/cwd, the release job is likely building from the wrong directory. Use `working-directory: crates/cue-dashboard` in Actions and `cd crates/cue-dashboard && cargo tauri build ...` in Makefile targets.
- 🟡 Windows native helpers are not built or packaged in the release workflow. There is a macOS helper build step (`.github/workflows/release.yml:78-83`), but no equivalent PowerShell step for `native/windows/*/build.ps1`, so Windows release zips will miss native helper binaries even after the C syntax is fixed.

---

### R7.4 — Lint Fix / Docs

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/router.rs`, `docs/work/IMPL-PHASE-3-ROUND-7.md`, `docs/work/PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md`, `docs/work/HANDOFF-TO-CODEX-FROM-KIRO.md`, `docs/work/AGENT-ONBOARDING.md` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 The handoff says "10 commits ahead" and lists 10, but the implementation doc later says 9 unique commits plus docs. Actual `git log main..HEAD` has 10 commits including `1f5a829 docs(work): Phase 3 Round 7 impl + handoff for codex review`. Not a code issue, just update the count wording during the fix pass.

## Cross-Task Findings

- 🔴 The two biggest user-facing claims of R7 are currently overstated: Local Whisper fallback exists as a provider/test seam but is not reachable from the daemon, and distribution scaffolding exists but would produce broken or incomplete install artifacts.
- 🟡 The live transcript UI is directionally good and small enough to keep, but it should gain a de-dupe guard and either remove or actually expose the daemon broadcast channel.

## Build & Test Verification

```bash
cargo fmt --all --check                              # ✅ pass
cargo clippy --all-targets -- -D warnings            # ✅ pass
cargo build --all-targets                            # ✅ pass
cargo test --all-targets                             # ✅ pass (213 passed, 2 ignored)
cd crates/cue-dashboard/ui && npm run build          # ✅ pass
git diff --check                                     # ✅ clean
ruby -c infra/homebrew/bluey.rb                         # ✅ Syntax OK
jq empty infra/scoop/bluey.json                         # ✅ valid JSON
bash -n infra/scripts/bump-formulae.sh                  # ✅ syntax OK
bash -n infra/scripts/download-whisper-model.sh         # ✅ syntax OK
swift build -c release                                  # ✅ native/macos/cue-whisper builds
clang -fsyntax-only native/windows/cue-whisper/main.c   # ❌ invalid C at lines 46-47
```

The normal Rust/UI pipeline is green. The round is still blocked because the native Windows helper syntax failure and package-artifact mismatches sit outside that pipeline.

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved.

## Follow-ups for Next Batch

- Wire the real production STT provider factory so the router can build Deepgram → OpenAI → LocalWhisper when the relevant env/config flags are enabled; add a deterministic test around that factory.
- Fix and compile-check the Windows whisper helper in CI or at least in a local syntax/build step.
- Reconcile release artifact names across workflow, Makefile, Homebrew, Scoop, and INSTALL. Prefer packaging the actual `bluey` / `bluey-daemon` binary names consistently.
- Move Tauri build commands to `crates/cue-dashboard` or pass the correct config explicitly.
- Add live transcript de-dupe on either the poller or React event merge path.
