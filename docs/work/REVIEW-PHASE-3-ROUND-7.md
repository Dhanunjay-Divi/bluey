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

## Fix-Wave Re-review

**Re-reviewed branch:** `feat/phase-3-round-9`
**Fix commits reviewed:** `0ef0e1d`, `2a8cabc`, `04ee1d0`, `13ed133`, `af0bd0d`, `628a347`, `901a117`, `1a19535`, `2b71262`
**Date:** 2026-05-16

### Resolved Items

- 🟢 Windows whisper helper now compiles under a strict C syntax check. `native/windows/cue-whisper/main.c:53-55` uses valid quoted JSON strings, and `clang -fsyntax-only -std=c89 -pedantic -Wall -Wextra native/windows/cue-whisper/main.c` passes locally.
- 🟢 The dashboard Tauri build now runs from `crates/cue-dashboard` in both `.github/workflows/release.yml` and `Makefile`.
- 🟢 Binary names are largely aligned to canonical `bluey` / `bluey-daemon` names across release packaging, Homebrew, Scoop, and install docs.
- 🟢 Windows helper build steps were added to the release workflow.

### Remaining Findings

- 🔴 `LocalWhisper` is still not a real production fallback when the primary cloud provider is absent or cannot connect. `crates/cue-daemon/src/stt/factory.rs:25-35` returns `Err("no STT API key configured")` before it ever reaches the OpenAI or LocalWhisper branches, so `BLUEY_STT_LOCAL_WHISPER=1` cannot run local-only. Also, `DeepgramProvider::connect(...).await.map_err(...)?` at `crates/cue-daemon/src/stt/factory.rs:30-32` makes a Deepgram connection failure fatal before OpenAI/LocalWhisper are attempted. That means "Deepgram offline -> local fallback" is not true at startup. The factory should collect providers opportunistically, warn on unavailable providers, and succeed if any enabled fallback is available.
- 🔴 The main real-audio startup path still requires a cloud API key before local STT can participate. `crates/cue-daemon/src/app.rs:1403-1405` returns `Ok(None)` when `BLUEY_STT_API_KEY` / `OPENAI_API_KEY` are unset, even if `BLUEY_STT_LOCAL_WHISPER=1` and a helper binary are configured. `build_mic_stt_provider()` exists but is not called anywhere in production, so the mic path is still not actually using the new factory chain.
- 🔴 The release workflow's SHA256 manifest generation is broken. `.github/workflows/release.yml:203-213` runs Python with unquoted file names/mode (`open(SHA256SUMS.txt)` and `open(sha256-manifest.json, w)`), which fails with `NameError` before `sha256-manifest.json` can be uploaded. Use a heredoc or quote `"SHA256SUMS.txt"`, `"sha256-manifest.json"`, and `"w"`.
- 🟡 The live transcript startup duplicate issue is not actually fixed. `LiveTranscript.tsx` still appends every `live_transcript` event without checking the new `index`, `LiveTranscriptList.tsx` does not include `index` in `TranscriptSegment`, and the background poller still starts with `last_count = 0` in `crates/cue-dashboard/src/lib.rs:142`. The new test simulates `last_count` starting at the catch-up count, but production has no connection between route catch-up and the global poller. This can stay a nit if acceptable for alpha, but the previous duplicate path remains.

## Recheck-2 Review

**Re-reviewed branch:** `feat/phase-3-round-9`
**Fix commits reviewed:** `1fbcbb8`, `dbc7bf1`, `9f54c03`, `2cbc147`, `4d2ea18`
**Date:** 2026-05-16

### Resolved Items

- 🟢 `build_stt_chain()` now attempts enabled providers independently and skips missing/failed primary providers instead of aborting before fallbacks. Empty chains return `SttError::NotActive`.
- 🟢 Local-only streaming STT is now possible when `BLUEY_STT_LOCAL_WHISPER=1` and a helper binary is configured. The ignored factory test suite now includes `factory_local_only_no_deepgram_key` and related empty/missing-key coverage.
- 🟢 The mic/chunked-REST path is now explicitly scoped out of the streaming provider factory in `crates/cue-daemon/src/stt/factory.rs` and the implementation docs. That satisfies the "wire it or document the scope" alternative from the previous review.
- 🟢 The old inline Python quoting bug is gone; `infra/scripts/build-sha256-manifest.py` builds the expected manifest when present in the working directory.
- 🟢 `LiveTranscript.tsx` now tracks `lastSeenIndexRef` and drops live events whose `index` has already been rendered from catch-up.

### Remaining Finding

- 🔴 The release job cannot run the new standalone manifest script because the `release` job never checks out the repository. `.github/workflows/release.yml:190-203` downloads artifacts, computes `SHA256SUMS.txt`, then calls `python3 infra/scripts/build-sha256-manifest.py`, but `infra/scripts/build-sha256-manifest.py` is not present in that job workspace without an `actions/checkout` step. Add checkout to the `release` job before running the script, or keep this logic inline in the workflow.

### Recheck-2 Nits

- 🟡 The local-only factory tests prove provider construction, but they use `/bin/cat`; an end-to-end local-only smoke test should use the real `whisper_stub` protocol so it proves transcript events too.
- 🟡 There is still a small route-start race: a live event can arrive before the initial catch-up promise resolves, and the catch-up `setSegments(tail)` can overwrite that newer event. This is narrower than the old duplicate bug and can be handled next round by merging catch-up and live state by `{ session_id, index }`.

## Cross-Task Findings

- 🔴 R7 no longer has the STT fallback construction blockers, but the release workflow still has one hard failure: the release job calls a repository script without checking out the repository.
- 🟡 Live transcript de-dupe is substantially improved and acceptable for alpha, with a remaining startup race to fold into the next polish round.

## Build & Test Verification

```bash
cargo fmt --all --check                                      # ✅ pass per Kiro
cargo clippy --all-targets -- -D warnings                    # ✅ pass per Kiro
cargo build --all-targets                                    # ✅ pass per Kiro
cargo test --all-targets                                     # ✅ pass per Kiro
clang -fsyntax-only -std=c89 -pedantic -Wall -Wextra \
  native/windows/cue-whisper/main.c                          # ✅ pass
cargo test --test live_transcript_emit                       # ✅ 4 passed
cargo test --test stt_factory_integration -- --ignored \
  --test-threads=1                                           # ✅ 10 passed after recheck-2
tmpdir smoke for infra/scripts/build-sha256-manifest.py       # ✅ pass locally
cargo fmt --all --check                                      # ✅ pass locally
cd crates/cue-dashboard/ui && npm run build                  # ✅ pass locally
git diff --check                                             # ✅ pass locally
```

## Overall Verdict

🔴 **REQUEST CHANGES** — One release workflow blocker remains.

## Follow-ups for Next Batch

- Add `actions/checkout@v4` to the `release` job before `python3 infra/scripts/build-sha256-manifest.py`, or inline the manifest builder so no repository file is required.
- Add an end-to-end local-only Whisper factory smoke test with `whisper_stub`.
- Merge live transcript catch-up/live state by `{ session_id, index }` to eliminate the remaining startup overwrite race.

## Final Recheck — Stacked Tip `b199058`

**Date:** 2026-05-16

### Resolved Items

- 🟢 The release job now checks out the repository before running `infra/scripts/build-sha256-manifest.py`.
- 🟢 LocalWhisper/OpenAI fallback construction, Windows whisper compile, artifact naming, Tauri build cwd, Windows helper packaging, and live transcript de-dupe are all fixed in the stacked branch.

### Final Verdict

🟢 **ACCEPT** — R7 blockers are resolved in the current stacked branch.
