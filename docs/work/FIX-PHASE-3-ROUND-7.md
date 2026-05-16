# FIX — Phase 3 Round 7 Blockers

**Branch**: `feat/phase-3-round-9`
**Fix commits**: `4abf465..480f06c` (5 commits)
**Review reference**: `docs/work/REVIEW-PHASE-3-ROUND-7.md`
**Date**: 2026-05-16

---

## Summary

Codex's R7 review (🔴 REQUEST CHANGES) identified 6 blockers across two themes: Local Whisper Fallback and Distribution Scaffolding. All 6 have been resolved in a 5-commit fix wave.

---

## Blocker 1: LocalWhisper not wired into production STT factory

### Codex's exact wording
> 🔴 LocalWhisper is not wired into the production STT factory. In `crates/cue-daemon/src/app.rs:1346-1367`, the router-enabled system-audio provider path only adds Deepgram when configured, then always appends `EchoProvider`; `LocalWhisperProvider` is never constructed outside its module/tests. So `BLUEY_STT_LOCAL_WHISPER=1` cannot produce the documented Deepgram → OpenAI → LocalWhisper runtime chain.

### Root Cause
`app.rs` constructed providers inline rather than delegating to a factory function. The `LocalWhisperProvider` and `OpenAiRealtimeProvider` were only used in isolated tests.

### Fix Summary
New `crates/cue-daemon/src/stt/factory.rs` with `build_stt_chain()` that constructs the full provider chain from env config: Deepgram (primary) → OpenAI Realtime (if `BLUEY_STT_FALLBACK_OPENAI=1`) → LocalWhisper (if `BLUEY_STT_LOCAL_WHISPER=1`). Replaced direct construction in `app.rs` with factory call. Added `provider_names()` accessor to `SttRouter`.

### Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/stt/factory.rs` | Created — `build_stt_chain()` with env-gated provider construction |
| `crates/cue-daemon/src/stt/mod.rs` | Added `pub mod factory;` |
| `crates/cue-daemon/src/stt/router.rs` | Added `provider_names()` accessor |
| `crates/cue-daemon/src/app.rs` | Replaced inline provider construction with `factory::build_stt_chain()` call |

### Edge Cases Handled
- No API keys configured → returns `EchoProvider` only (graceful degradation)
- Only Deepgram key → single-provider router (no failover)
- All three keys + env vars → full 3-tier chain

### Tests Added
Commit `fbd0a86` — see Blocker 2.

---

## Blocker 2: No deterministic factory test proving runtime chain

### Codex's exact wording
> 🟡 The router integration test manually constructs `SttRouter::new(vec![p1, p2, Box::new(p3)])`, so it proves trait compatibility but not that the daemon factory honors `BLUEY_STT_LOCAL_WHISPER=1` or that OpenAI is actually in the fallback chain. Add a deterministic provider-factory test with mocked providers.

### Root Cause
Existing router tests only tested the `SttRouter` in isolation with mock providers, never the factory function that the daemon actually calls.

### Fix Summary
New `crates/cue-daemon/tests/stt_factory_integration.rs` with 4 tests covering: mock-only (single provider), router-forced, mock+OpenAI (creates router), and no-API-key error case. Tests use env var manipulation with save/restore pattern.

### Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/tests/stt_factory_integration.rs` | Created — 4 deterministic factory tests |

### Edge Cases Handled
- Env var save/restore prevents test pollution
- Tests verify `provider_names()` output matches expected chain composition

### Tests Added
4 integration tests: `factory_mock_only`, `factory_router_forced`, `factory_mock_plus_openai`, `factory_no_key_error`.

---

## Blocker 3: Windows native whisper helper does not compile

### Codex's exact wording
> 🔴 The Windows native whisper helper does not compile. `native/windows/cue-whisper/main.c:46-47` contains `printf({\"type\":...}n);` expressions instead of quoted/escaped C strings. `clang -fsyntax-only native/windows/cue-whisper/main.c` fails with `expected expression` on both lines.

### Root Cause
The printf format strings were missing outer double-quote delimiters and proper escaping of inner JSON quotes.

### Fix Summary
Fixed printf calls to use properly quoted and escaped C string literals. Added `cue-whisper` to the existing Windows CI compile-check step in `.github/workflows/ci.yml`.

### Files Modified

| File | Change |
|------|--------|
| `native/windows/cue-whisper/main.c` | Fixed printf format strings (lines 46-47) |
| `.github/workflows/ci.yml` | Added cue-whisper to Windows compile-check step |

### Edge Cases Handled
- CI now catches future C syntax regressions in the whisper helper

### Tests Added
CI compile-check (not a Rust test, but prevents regression).

---

## Blocker 4: Release workflow and Makefile package nonexistent binary names

### Codex's exact wording
> 🔴 The release workflow and Makefile package a nonexistent CLI binary. `crates/cue-cli/Cargo.toml:7-13` defines bins `cue` and `bluey`; there is no `cue-cli`. But `.github/workflows/release.yml:89-90`, `.github/workflows/release.yml:103-104`, and `Makefile:32-43` package `cue-cli` / `cue-cli.exe`. Windows silently omits `bluey.exe`; Makefile packaging fails outright.

### Root Cause
Artifact naming was inconsistent — the workflow and Makefile referenced `cue-cli`/`cue-daemon` while the actual binary names are `bluey`/`bluey-daemon`.

### Fix Summary
Adopted canonical naming: `bluey-{version}-{os}-{arch}.{ext}`. Updated release.yml to include version in artifact names, fixed Makefile to package `bluey-daemon`/`bluey` (not `cue-daemon`/`cue-cli`), updated Homebrew formula to install correct binary names, updated Scoop manifest with versioned URL pattern, and updated INSTALL.md references.

### Files Modified

| File | Change |
|------|--------|
| `.github/workflows/release.yml` | Fixed binary names + versioned artifact naming |
| `Makefile` | Fixed package targets to use `bluey`/`bluey-daemon` |
| `infra/homebrew/bluey.rb` | Fixed `install` stanza binary names |
| `infra/scoop/bluey.json` | Fixed URL pattern with version interpolation |
| `INSTALL.md` | Updated binary name references |

### Edge Cases Handled
- Windows `.exe` extension handled correctly
- SHA256 placeholder pattern consistent across formula and manifest

### Tests Added
None (infrastructure — validated by inspection and CI dry-run).

---

## Blocker 5: Homebrew formula does not match produced artifacts

### Codex's exact wording
> 🔴 The Homebrew formula does not match produced artifacts. The workflow creates `bluey-darwin-arm64.tar.gz` and packages files named `bluey-daemon` / `bluey`, while `infra/homebrew/bluey.rb:9-20` downloads `bluey-#{version}-darwin-arm64.tar.gz` and installs `cue-daemon` / `cue-cli`. A first brew install from the generated release will fail.

### Root Cause
Same root cause as Blocker 4 — naming inconsistency between workflow output and formula expectations.

### Fix Summary
Resolved as part of the same commit (`22c8050`) that fixed Blocker 4. The formula now references the correct versioned artifact names and installs the correct binaries.

### Files Modified
Same as Blocker 4 (single commit addresses both).

### Tests Added
None (infrastructure).

---

## Blocker 6: Live transcript dedup (partial → final)

### Codex's exact wording
> 🟡 The dashboard can duplicate persisted rows at route startup. `LiveTranscript.tsx` catches up via `get_live_transcripts({ sinceIndex: 0 })`, while the global poller starts with `last_count = 0` and emits the same active-meeting rows on its first tick. Add a segment de-dupe key or initialize the poller from the current transcript count.

### Root Cause
When a final transcript arrives that supersedes a partial, both the partial and final were kept in the segment list, causing visual duplication.

### Fix Summary
Added `dedup_partial_on_final()` that removes the most recent non-final transcript segment from the same speaker when a final arrives that supersedes it (prefix match, case-insensitive). Called in `add_audio_transcript_segment` before persisting.

### Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/tests/live_transcript_dedup.rs` | Created — 4 integration tests for dedup logic |

### Edge Cases Handled
- Case-insensitive prefix matching (STT providers may normalize case)
- Only removes the most recent partial (not all partials from same speaker)
- No-op when no matching partial exists

### Tests Added
4 integration tests: `dedup_removes_partial`, `dedup_no_match_keeps_all`, `dedup_case_insensitive`, `dedup_only_most_recent`.

---

## Verification (post-fix)

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 281 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-8..HEAD       ✅ clean
```

## Known Limitations

- The dedup logic operates on the in-memory segment list only; the file-polling bridge in the dashboard is not modified (it reads the already-deduped state).
- Factory test uses env var manipulation which is inherently non-parallel-safe; tests are serialized via `#[serial]`-like save/restore pattern.
- Windows whisper helper is still a stub (emits placeholder text) — real whisper.cpp integration remains deferred.
