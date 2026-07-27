# FIX-022: Movable Pill and Reliable Ask Shortcut

## Issue

The collapsed meeting pill could not be placed reliably because finishing a drag
also expanded it. In the expanded overlay, the Ask shortcut required a
non-discoverable double-click to open its input, while a normal click could
appear to do nothing.

## Root Cause

`useDragHeader` ran while the overlay was expanded, before the conditionally
rendered pill existed. Its effect saw a null ref and never ran again because the
ref object itself stayed stable, so no native drag listener was installed.
When a listener was installed, the pill body was also an expand click target:
macOS delivered a click after the native drag, so `FloorplanPill`/`Pill`
immediately expanded at the new location.

A later attempt also marked that same body as a deep native Tauri drag region.
Tauri begins those drags on `mousedown`, before the movement-threshold hook can
record that a drag happened, so drag and expand could race again.

`FloatingStack` delayed every single Ask click for 230 ms and routed it to an
implicit "answer the recent question" action. The text input opened only when a
second click arrived inside that timing window.

## Fix Summary

- Re-run `useDragHeader` when the conditionally rendered pill becomes active,
  then install the listener against the now-mounted element.
- Return a drag-consumption ref and mark it as soon as the native drag threshold
  is crossed.
- Use a statically loaded Tauri window handle after the four-pixel threshold
  instead of making the click body a deep native drag region. This keeps the
  window responsive without swallowing the click/drag discriminator.
- Have both compact pill implementations consume the post-drag click instead of
  expanding.
- Disable WebKit text selection while dragging, prevent Space from scrolling,
  and surface `permission_denied`/audio failure in the collapsed pill instead of
  presenting it as ordinary idle.
- Clamp pill morphs and the restored full panel to the active monitor work area,
  so expanding a pill parked at an edge cannot strand the panel off-screen.
- Make the Ask shortcut open or close its text input on one click, focus the
  input immediately, and keep detected-question answering on its explicit dock.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-meeting-overlay/ui/src/lib/useDragHeader.ts` | Attach on active mounts and report whether a click followed a native window drag. |
| `crates/cue-meeting-overlay/ui/src/App.tsx` | Activate the drag hook only after the pill mounts and share its consumption ref. |
| `crates/cue-meeting-overlay/ui/src/components/FloorplanPill.tsx` | Suppress expand after dragging. |
| `crates/cue-meeting-overlay/ui/src/components/Pill.tsx` | Suppress expand after dragging in the legacy skin. |
| `crates/cue-meeting-overlay/ui/src/lib/useCollapse.ts` | Preserve edge placement while keeping resized windows inside the current work area. |
| `crates/cue-meeting-overlay/ui/src/components/floorplan/FloatingStack.tsx` | Replace double-click timing with a direct single-click Ask input. |
| `crates/cue-meeting-overlay/ui/src/screens/OpenFloorScreen.tsx` | Remove the obsolete implicit recent-question callback. |

## Edge Cases Handled

- A click without movement still expands the pill.
- Enter and Space still expand the keyboard-focused pill.
- Dragging the label does not select transcript text.
- Pill trailing controls remain independent from the drag/expand body.
- Expanding or growing a pill at any monitor edge remains fully reachable.
- Ask remains disabled while an answer is already streaming.

## How to Test

```bash
cd crates/cue-meeting-overlay/ui
npm run build

# In a capture-visible local run:
# 1. Minimize to the pill.
# 2. Drag the pill to another part of the screen and release.
# 3. Verify it stays collapsed at the release location.
# 4. Click (without dragging) and verify it expands.
# 5. Click Ask once and verify the focused text input opens immediately.
```

## Known Limitations

- The OS remembers the moved position for the running overlay process. A future
  preference could persist it across app restarts.
