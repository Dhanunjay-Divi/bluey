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
- 🟡 The daemon-side `broadcast::Sender<LiveTranscriptEvent>` is not consumed by any production subscriber. `add_audio_transcript_segment` sends to `live_transcript_tx`, but the dashboard uses a separate file poller over `active-meeting.json`. This is not fatal for the UI, but the committed daemon event channel is dormant infrastructure rather than the active bridge described in the handoff.
- 🟡 The dashboard can duplicate persisted rows at route startup. `LiveTranscript.tsx` catches up via `get_live_transcripts({ sinceIndex: 0 })`, while the global poller starts with `last_count = 0` and emits the same active-meeting rows on its first tick. Add a segment de-dupe key or initialize the poller from the current transcript count.

---

### R7.2 — Local Whisper Fallback

| Field | Value |
|-------|-------|
| Files | `native/macos/cue-whisper/**`, `native/windows/cue-whisper/**`, `crates/cue-daemon/src/stt/whisper/**`, `crates/cue-daemon/src/stt/router.rs`, `crates/cue-daemon/src/bin/whisper_stub.rs`, `crates/cue-daemon/tests/whisper_integration.rs`, `crates/cue-daemon/src/app.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 LocalWhisper is not wired into the production STT factory. In `crates/cue-daemon/src/app.rs:1346-1367`, the router-enabled system-audio provider path only adds Deepgram when configured, then always appends `EchoProvider`; `LocalWhisperProvider` is never constructed outside its module/tests. So `BLUEY_STT_LOCAL_WHISPER=1` cannot produce the documented Deepgram → OpenAI → LocalWhisper runtime chain.
- 🔴 The Windows native whisper helper does not compile. `native/windows/cue-whisper/main.c:46-47` contains `printf({"type":...}n);` expressions instead of quoted/escaped C strings. `clang -fsyntax-only native/windows/cue-whisper/main.c` fails with `expected expression` on both lines.
- 🟡 The router integration test manually constructs `SttRouter::new(vec![p1, p2, Box::new(p3)])`, so it proves trait compatibility but not that the daemon factory honors `BLUEY_STT_LOCAL_WHISPER=1` or that OpenAI is actually in the fallback chain. Add a deterministic provider-factory test with mocked providers.
- 🟡 `LocalWhisperProvider::close()` drops the stdin sender and marks state closed, but it does not await or force-kill a stuck helper. That is tolerable for a stub helper that exits on EOF, but it does not satisfy the handoff checklist claim that close kills and waits.

---

### R7.3 — Distribution Scaffolding

| Field | Value |
|-------|-------|
| Files | `.github/workflows/release.yml`, `Makefile`, `infra/homebrew/bluey.rb`, `infra/scoop/bluey.json`, `infra/scripts/bump-formulae.sh`, `infra/scripts/download-whisper-model.sh`, `INSTALL.md`, `crates/cue-dashboard/tauri.conf.json` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The release workflow and Makefile package a nonexistent CLI binary. `crates/cue-cli/Cargo.toml:7-13` defines bins `cue` and `bluey`; there is no `cue-cli`. But `.github/workflows/release.yml:89-90`, `.github/workflows/release.yml:103-104`, and `Makefile:32-43` package `cue-cli` / `cue-cli.exe`. Windows silently omits `bluey.exe`; Makefile packaging fails outright.
- 🔴 The Homebrew formula does not match produced artifacts. The workflow creates `bluey-darwin-arm64.tar.gz` and packages files named `bluey-daemon` / `bluey`, while `infra/homebrew/bluey.rb:9-20` downloads `bluey-#{version}-darwin-arm64.tar.gz` and installs `cue-daemon` / `cue-cli`. A first brew install from the generated release will fail.
- 🟡 `cargo tauri build` is invoked from repo root in `.github/workflows/release.yml:72-73` and `Makefile:10-26`, but the Tauri config lives in `crates/cue-dashboard/tauri.conf.json`. Run the command from `crates/cue-dashboard` or pass the config explicitly.
- 🟡 Windows native helpers are not built or packaged. The workflow has a macOS helper build step, but no PowerShell step for `native/windows/*/build.ps1`, so Windows release zips miss helper binaries even after the C syntax is fixed.

---

### R7.4 — Lint Fix / Docs

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/router.rs`, `docs/work/IMPL-PHASE-3-ROUND-7.md`, `docs/work/PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md`, `docs/work/HANDOFF-TO-CODEX-FROM-KIRO.md`, `docs/work/AGENT-ONBOARDING.md` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 R7 documentation says "10 commits ahead" in some places and "9 unique commits" in others. Actual `git log main..feat/phase-3-round-7` has 10 commits including the docs commit `1f5a829`.

## Cross-Task Findings

- 🔴 R7 overstates two core deliverables: Local Whisper exists as a provider/test seam but is unreachable from the daemon runtime, and distribution scaffolding exists but would generate broken or incomplete install artifacts.

## Build & Test Verification

```bash
cargo fmt --all --check                              # ✅ pass on current R8 tip
cargo clippy --all-targets -- -D warnings            # ✅ pass on current R8 tip
cargo build --all-targets                            # ✅ pass on current R8 tip
cargo test --all-targets                             # ✅ pass on current R8 tip (224 passed, 2 ignored)
cd crates/cue-dashboard/ui && npm run build          # ✅ pass on current R8 tip
git diff --check                                     # ✅ clean
clang -fsyntax-only native/windows/cue-whisper/main.c # ❌ invalid C at lines 46-47
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved.

## Follow-ups for Next Batch

- Wire the real production STT provider factory so the daemon can build the documented provider chain.
- Fix and compile-check the Windows whisper helper.
- Reconcile binary and artifact names across release workflow, Makefile, Homebrew, Scoop, and INSTALL.
- Move Tauri release builds to the dashboard crate/config.
- Add live transcript de-dupe.
