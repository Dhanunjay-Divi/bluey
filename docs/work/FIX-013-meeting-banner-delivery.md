# FIX-013: Reliable and Private Meeting-Prep Banners

## Issue

Meeting-prep banners could be invisible, reopen after dismissal, appear on the
wrong display, or receive unrelated meeting command traffic.

## Root Cause

The banner webview did not have Tauri event-listen capability. The daemon
retries each calendar warmup event, but the overlay stored only one pending
value and remembered dismissal by event ID alone. A retry could reopen the
panel, a second due meeting could overwrite the first, and recurring or moved
occurrences sharing an event ID could consume one another. The command bridge
broadcast transcript/card traffic to every webview, banner capture protection
was disabled in the default configuration, and placement used fixed screen
assumptions instead of the active display's work area.

## Fix Summary

- Grant only the banner window the event capability it needs.
- Queue due offers and keep bounded dismissal tombstones keyed by
  `(event ID, occurrence start)`, deduplicating both pending and post-dismiss
  retries without dropping back-to-back meetings.
- Echo the same occurrence identity through the TypeScript client and daemon
  response handler so a stale response cannot remove another occurrence.
- Route normal daemon commands to the meeting webview and the dedicated banner
  event only to the banner webview.
- Restore capture protection by default and reassert it after NSPanel
  conversion; visible development mode remains an explicit opt-in.
- Position the banner at the top-right of the cursor display's work area.
- Remove redundant delayed re-emits and tolerate capability rejection in the
  TypeScript listener without crashing the window.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-meeting-overlay/capabilities/banner.json` | Authorize banner event listening. |
| `crates/cue-core/src/overlay.rs` | Carry event ID and occurrence start in banner responses. |
| `crates/cue-daemon/src/app.rs` | Track and consume pending prep by exact occurrence. |
| `crates/cue-meeting-overlay/src/ipc.rs` | Queue/deduplicate delivery, target events, protect and position the panel. |
| `crates/cue-meeting-overlay/src/commands.rs` | Advance the queue only for the matching occurrence. |
| `crates/cue-meeting-overlay/src/lib.rs` | Register the pending-banner state and command. |
| `crates/cue-meeting-overlay/tauri.conf.json` | Restore default content protection. |
| `crates/cue-meeting-overlay/ui/src/BannerWindow.tsx` | Acknowledge dismissal with the event ID. |
| `crates/cue-meeting-overlay/ui/src/lib/tauriClient.ts` | Handle listener capability errors safely. |

## Edge Cases Handled

- Daemon retries cannot resurrect a banner occurrence the user already
  dismissed.
- Back-to-back meetings remain queued, and a stale acknowledgement leaves the
  current offer intact.
- Recurring and rescheduled events sharing an ID remain independent.
- A banner received before the webview listener mounts remains pullable.
- Poisoned pending-state locks recover without crashing the overlay.
- Negative-origin and scaled secondary displays use their own work area.
- Visible-mode smoke tests can still opt into screenshot-visible windows.

## How to Test

```bash
cargo test -p cue-meeting-overlay \
  pending_banner

cargo test -p cue-daemon pending_meeting_prep

cd crates/cue-meeting-overlay/ui
npm run build
```

In a visible local run, trigger the same meeting-prep event repeatedly, dismiss
it, and confirm it does not reopen. Move the pointer to another display before
triggering a new event and confirm the banner stays within that display.

## Known Limitations

- The non-activating banner intentionally does not take keyboard focus. Its
  actions are mirrored by the main overlay for keyboard-first workflows.
