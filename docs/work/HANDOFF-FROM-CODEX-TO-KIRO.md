# Codex → Kiro: R7 Review + Implementation Handoff

## 1. R7 Verdict

🔴 **REQUEST CHANGES**

Review written to: `docs/work/REVIEW-PHASE-3-ROUND-7.md`

Primary blockers:
- LocalWhisper is implemented as a provider/test seam, but production daemon code never constructs it. `BLUEY_STT_LOCAL_WHISPER=1` does not currently add LocalWhisper to the daemon STT chain.
- `native/windows/cue-whisper/main.c` does not compile because the NDJSON `printf` lines are invalid C.
- Distribution scaffolding packages/installs wrong artifact names: the CLI package defines `cue` and `bluey`, but workflow/Makefile/formula references `cue-cli`; Homebrew URLs also do not match workflow artifact names.

## 2. What I Implemented

Nothing beyond review documentation. I did not add feature work on top of R7 because the branch needs a fix pass first; adding P0/P1 implementation now would make Kiro's re-review surface noisier and hide the actual R7 blockers.

## 3. What I Skipped and Why

- Real whisper.cpp integration: skipped because R7's current LocalWhisper stub is not reachable from production code yet.
- LocalWhisperProvider crash restart loop: skipped because the provider wiring and helper compile issues should be fixed before supervisor behavior.
- Structured logging/crash reporting, stress tests, bookmarks, AI summaries: skipped to preserve a clean review/fix loop.

## 4. Pipeline Status

Checks run:

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

The normal Rust/UI pipeline is green. The verdict is red because the Windows native helper and release packaging blockers are outside that pipeline.

## 5. New Test Count

No tests added by Codex in this pass. R7 remains at Kiro's reported `213 passing, 2 ignored` for Rust tests, but the native Windows helper blocker sits outside that count.

## 6. Branches Ready for Kiro Review

None. Current branch `feat/phase-3-round-7` needs a fix round.

## 7. Pending Followups

Required R7 fix pass:
- Wire LocalWhisper and OpenAI into the actual daemon STT router/factory path, not only manual tests.
- Add a production-factory test proving env/config can produce the documented fallback chain without real network calls.
- Fix `native/windows/cue-whisper/main.c` and add a compile/syntax check to CI or release verification.
- Reconcile binary/artifact names across `.github/workflows/release.yml`, `Makefile`, `infra/homebrew/bluey.rb`, `infra/scoop/bluey.json`, and `INSTALL.md`.
- Build/package Windows native helpers in the release workflow.
- Either remove the unused daemon live transcript broadcast channel or expose a real subscriber path; add UI de-dupe for file-poller catch-up.
