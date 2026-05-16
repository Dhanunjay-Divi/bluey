# Codex → Kiro: R7 Recheck-2 + R8 Fix-Wave Handoff

## 1. R7 Verdict

🔴 **REQUEST CHANGES**

Review written to: `docs/work/REVIEW-PHASE-3-ROUND-7.md`

Blockers:
- The `release` job calls `python3 infra/scripts/build-sha256-manifest.py` but never checks out the repository. GitHub Actions jobs do not share the build job workspace, so the script will not exist in the release job unless `actions/checkout@v4` is added or the manifest generation is inlined.

Fixed from the prior review/recheck:
- Windows whisper helper now compiles under strict C syntax check.
- Canonical `bluey` / `bluey-daemon` binary names are mostly aligned across packaging.
- Tauri build cwd is fixed.
- Windows helper build steps were added to release workflow.
- STT factory now attempts all enabled fallbacks independently.
- Local-only streaming STT now builds with `BLUEY_STT_LOCAL_WHISPER=1`.
- Mic/chunked-REST path scoping is explicitly documented as outside the streaming factory.
- The standalone SHA manifest script works locally when present.
- Live transcript de-dupe now uses the segment `index` cursor.

Nits still open:
- Add an end-to-end local-only Whisper smoke test using `whisper_stub`, not `/bin/cat`.
- Live transcript still has a small startup overwrite race if a live event arrives before catch-up resolves.

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
- R8 remains stacked on the R7 release blocker and should not merge independently.

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
                                                       # ✅ 10 passed
cargo test --test live_transcript_emit                # ✅ 4 passed
tmpdir smoke for infra/scripts/build-sha256-manifest.py
                                                       # ✅ pass locally
cargo fmt --all --check                               # ✅ pass locally
cd crates/cue-dashboard/ui && npm run build           # ✅ pass locally
git diff --check                                      # ✅ pass locally
```

## 6. New Test Count

No tests added by Codex.

- R7 reported test count: `213 passed, 2 ignored`
- R8 reported test count: `224 passed, 2 ignored`
- Current fix-wave reported test count: `284 passing`
- Ignored factory recheck suite: `10 passed`

## 7. Branches Ready for Kiro Review

None yet. R8-specific fixes are acceptable with nits, and the R7 STT blockers are cleared, but the stacked `feat/phase-3-round-9` branch still needs the release-job checkout/script blocker fixed before merge.
