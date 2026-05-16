# Codex → Kiro: R7 + R8 Fix-Wave Re-review Handoff

## 1. R7 Verdict

🔴 **REQUEST CHANGES**

Review written to: `docs/work/REVIEW-PHASE-3-ROUND-7.md`

Blockers:
- STT factory now mentions LocalWhisper, but startup fallback is still broken: missing Deepgram key or Deepgram connect failure returns before OpenAI/LocalWhisper can be used.
- Main real-audio startup still requires a cloud STT key before local STT can participate; `build_mic_stt_provider()` is currently dead code.
- Release workflow SHA256 manifest generation is broken by unquoted Python file names/mode in `.github/workflows/release.yml`.

Fixed from the prior review:
- Windows whisper helper now compiles under strict C syntax check.
- Canonical `bluey` / `bluey-daemon` binary names are mostly aligned across packaging.
- Tauri build cwd is fixed.
- Windows helper build steps were added to release workflow.

Nit still open:
- Live transcript route still does not use the new `index` field for real UI de-dupe; the test simulates a state production does not currently share.

## 2. R8 Verdict

🟡 **ACCEPT WITH NITS** for the R8-specific fixes.

Review written to: `docs/work/REVIEW-PHASE-3-ROUND-8.md`

Resolved:
- Settings page now uses keyring-backed STT key commands.
- Settings mic-device key now matches daemon key `audio.mic_device`.
- Runtime icon docs now mark icon switching as deferred.
- Startup reassertion reloads current mode instead of reapplying a stale startup value.

Nits/follow-ups:
- `save_settings` silently drops `api_key`-shaped keys instead of returning an explicit error.
- `load_stt_api_key` masking should use char-safe suffix logic.
- R8 remains stacked on R7/R9 blockers and should not merge independently.

## 3. What I Implemented

No feature implementation. I limited this pass to review documentation and targeted verification.

## 4. What I Skipped and Why

- Product naming/runtime branding decisions: intentionally not revisited per user direction.
- R9 feature review: skipped because this pass was specifically R7/R8 fix-wave re-review.
- Feature implementation: skipped until the remaining release/STT blockers are resolved.

## 5. Pipeline Status

Checks run or verified on current tip (`feat/phase-3-round-9`):

```bash
cargo fmt --all --check                              # ✅ pass per Kiro
cargo clippy --all-targets -- -D warnings            # ✅ pass per Kiro
cargo build --all-targets                            # ✅ pass per Kiro
cargo test --all-targets                             # ✅ pass per Kiro (284 passing reported)
cd crates/cue-dashboard/ui && npm run build          # ✅ pass per Kiro
clang -fsyntax-only -std=c89 -pedantic -Wall -Wextra \
  native/windows/cue-whisper/main.c                  # ✅ pass
cargo test --test stt_factory_integration -- --ignored --test-threads=1
                                                       # ✅ 6 passed
cargo test --test live_transcript_emit                # ✅ 4 passed
python3 -c '<release manifest snippet>'               # ❌ NameError: SHA256SUMS
```

## 6. New Test Count

No tests added by Codex.

- R7 reported test count: `213 passed, 2 ignored`
- R8 reported test count: `224 passed, 2 ignored`
- Current fix-wave reported test count: `284 passing`

## 7. Branches Ready for Kiro Review

None yet. R8-specific fixes are acceptable with nits, but the stacked `feat/phase-3-round-9` branch still needs the R7 release/STT blockers fixed before merge.
