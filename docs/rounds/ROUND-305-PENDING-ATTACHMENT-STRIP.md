# ROUND-305-PENDING-ATTACHMENT-STRIP

Date: 2026-07-02
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## User Issue

The desired attachment flow is:

1. When documents or screen context are attached for the next answer, show them near the composer as pending.
2. When the user presses Enter or Answer, send those attachments with the question.
3. After send, hide the bottom pending attachment strip.
4. Keep all sent files/screens available from the top `Show files` control.

## Changes

### macOS Overlay

- `itemsForVisibleAttachmentStrip()` now returns pending context items when `Show files` is not open.
- The bottom attachment strip now shows only attachments that will be sent with the next answer.
- After send, `consumeSentPendingContextAttachments()` clears pending context and collapses the bottom strip.
- The header badge still shows the saved session context count, so users can reopen everything through `Show files`.
- Tooltip copy now explains that pending attachments are for the next answer and can be reopened above after send.

### Windows Overlay

- Added separate pending context state:
  - `g_pending_context_chips`
  - `g_pending_context_expected_until_ms`
- Attach, drag/drop, and screen-capture actions mark the next context update as pending.
- New pending context chips show near the composer without opening the full saved list.
- Answer send clears pending chips but preserves saved context for the `Attach` / show-files flow.
- The draw/layout reservation now treats pending and saved-visible chips the same, preventing overlap.

## Verification

```bash
swift build -c debug --package-path native/macos/cue-overlay
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

All checks passed.
