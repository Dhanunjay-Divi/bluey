# Codex → Kiro: R5+R6 Review + Implementation Handoff

## 1. R5 + R6 Verdicts

- `docs/work/REVIEW-PHASE-3-ROUND-5.md`: 🔴 **REQUEST CHANGES**
- `docs/work/REVIEW-PHASE-3-ROUND-6.md`: 🔴 **REQUEST CHANGES**

I did the review pass first, as requested, and did not start P0 implementation because both reviewed rounds still have merge-blocking issues. The important one is R5: continuous system-audio STT still sends audio into one provider instance and drains events from a second provider instance, so the original "system audio transcripts never reach the session" blocker is not actually fixed.

## 2. What I Implemented

No product-code implementation was started. I created the review artifacts and this handoff doc only.

| Item | Files changed | Notes |
|------|---------------|-------|
| R5 post-fix re-review | `docs/work/REVIEW-PHASE-3-ROUND-5.md` | Re-reviewed all six claimed blocker fixes. Five pass; system-audio STT drain remains a blocker. |
| R6 first review | `docs/work/REVIEW-PHASE-3-ROUND-6.md` | Reviewed hotkeys/tray, mic selection, permission UX, and OpenAI Realtime STT. Found multiple end-to-end blockers. |
| Codex-to-Kiro handoff | `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md` | Captures verdicts, skipped work, and next required fix sequence. |

## 3. What I Skipped and Why

- P0 Item 1, R6 IMPL + HANDOFF docs: skipped because Round 6 needs a fix round first; writing implementation docs now would canonize behavior that is not actually working.
- P0 Item 2, Live transcript UX: skipped because the underlying R5/R6 live transcript sources are still blocked.
- P0 Item 3, Local Whisper fallback: skipped because the existing cloud-provider and router plumbing needs correction first.
- P0 Item 4, Distribution scaffolding: skipped to keep the branch focused on fixing runtime correctness before packaging.
- P1/P2 items: skipped because P0 correctness is not ready.

## 4. Pipeline Status

I ran the verification pipeline after adding the review/handoff docs. The branch has no Codex product-code changes, only docs.

```bash
cargo fmt --all --check                    # ✅
cargo clippy --all-targets -- -D warnings  # ✅
cargo build --all-targets                  # ✅
cargo test --all-targets                   # ✅ 185 passed, 2 ignored
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check                           # ✅
```

I did not run `swift build` because Codex did not touch native overlay code in this pass.

## 5. New Test Count

No tests were added by Codex in this pass.

Current reported state from Kiro's handoff:

| Branch | Reported running tests |
|--------|------------------------|
| `feat/phase-3-round-5` | 164 |
| `feat/phase-3-round-6` | 185 |

## 6. Branches Ready for Kiro Review

No new implementation branch is ready for Kiro review. The current branch `feat/phase-3-round-6` now contains review docs and needs a Kiro fix round before merge.

Paste-ready next instruction for Kiro:

```text
Fix R5/R6 blockers called out by Codex.

Read:
- docs/work/REVIEW-PHASE-3-ROUND-5.md
- docs/work/REVIEW-PHASE-3-ROUND-6.md

Required fixes:
1. R5: continuous system-audio STT must use one provider instance for both send_audio and next_event. Add a production-wiring test that proves captured system audio reaches add_audio_transcript_segment/session transcript.
2. R6: mic device selection must be consumed by daemon capture, not just saved/tested.
3. R6: permission denial UX must be emitted from real capture failures to the dashboard, and open_privacy_settings must use platform-specific launch commands.
4. R6: OpenAI Realtime STT must send transcription-session setup, parse conversation.item.input_audio_transcription.delta/completed, use a transcription model default, and test those real event names.
5. R6: write IMPL-PHASE-3-ROUND-6.md and PHASE-3-ROUND-6-HANDOFF-FOR-CODEX-REVIEW.md after fixes.

Then rerun:
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test --all-targets
cd crates/cue-dashboard/ui && npm run build
git -P diff --check feat/phase-3-round-5..HEAD

Hand back to Codex for re-review with fix doc(s) and updated test count.
```

## 7. Pending Followups

- After the R5/R6 fix round passes, resume P0 in the original order: R6 docs, live transcript UX, local Whisper fallback, distribution scaffolding.
- For OpenAI Realtime, use the official realtime transcription guide as the compatibility source for event names and session setup: https://platform.openai.com/docs/guides/realtime-transcription
- Consider moving hotkey/tray daemon IPC out of React listeners and into Rust-side handlers so background control survives webview reloads.
