# Round 230 - Live Caption Rail Send Bounds

Date: 2026-06-29 11:26 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed a case where pressing Enter after Listen turned a long live transcript into a huge Question bubble with repeated-looking chunks. They also asked why the live strip behaves like a clipped caption instead of a horizontal transcript rail, whether this material is being saved to R2, and whether the transcript UI should be bounded so it does not overload the overlay.

## Findings

- macOS already had an `NSScrollView` around the live transcript label, but the strip had no explicit scroll-wheel forwarding and the scrollbar auto-hidden, so it felt like a clipped one-line caption.
- `composedQuestionForAnswer` pasted the current transcript buffer directly into the submitted question when the input was empty, and appended it below typed text when the input was non-empty.
- That made the visible Question card and saved conversation turn bulky, even though the daemon already supplies recent meeting transcript as `active meeting transcript` answer context.
- Transcript segments are not uploaded through the artifact object endpoint/R2 path. Artifact files/screenshots use `/sync/artifacts/:artifact_id/object`; transcript text goes through `/sync/batch` into cloud transcript tables and RAG chunks.
- Windows had a separate 1024-character transcript buffer and an older empty-input fallback prompt, so it needed a parity wording update even though it does not have the same macOS horizontal strip.

## Fix

- macOS live captions now behave more like a bounded rail:
  - the transcript strip keeps a horizontal scroller available
  - scroll-wheel events over the strip are forwarded to the transcript scroll view
  - live rail display is capped to the most recent 520 characters
  - live preview memory is capped to the most recent 1400 characters
- macOS ask/auto-send now sends a short live-caption intent instead of dumping the raw transcript into the question:
  - `Answer the latest live captions from the current session transcript...`
  - typed questions remain typed questions; transcript context is supplied by the daemon's active meeting transcript path
- Duplicate ask suppression now includes a private compact transcript fingerprint so two different spoken sends are not treated as the same short prompt. The fingerprint is not logged.
- Transcript merging now catches longer overlap windows and near-identical revisions before appending, reducing repeated chunk buildup in the overlay buffer.
- Windows empty-input transcript sends now use the same live-caption intent when transcript context exists, while retaining the broader fallback when there is no transcript.

## Mac / Windows Parity

- macOS received the rail scrolling/bounding behavior because that UI exists only in the Swift overlay.
- Windows received the equivalent send behavior change for empty-input transcript asks.
- Windows already stores only a capped local transcript preview buffer (`1024` wide chars) and does not have the same macOS transcript `NSScrollView` rail.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/macos/cue-overlay/build.sh`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`

Current local status after the change:

- daemon pid `77359`
- active meeting id `259b7fee-a3a2-478a-9061-325aedf31dcd`
- transcript segments `12`
- overlay visible `false`
- overlay capture excluded `false` because the current local daemon is still in visible QA mode
- screen capture active `false`

## Remaining QA / Gates

- Restart the local visible QA overlay from the rebuilt macOS overlay binary before testing this UX live.
- Press Listen, speak a long phrase, and verify:
  - the green live strip scrolls horizontally instead of behaving like a clipped static label
  - the strip shows a bounded recent tail rather than the entire transcript
  - pressing Enter creates a short Question bubble, not a giant transcript blob
  - the answer still uses the active meeting transcript context
  - repeated transcript chunks are reduced
- Before any release/upload, turn off visible QA mode and verify `overlay_capture_excluded: true`.
