# FIX-019: Source-Aware Audio Permission Recovery

## Issue

After macOS denied an audio permission, expanded overlay controls repeatedly
opened System Settings and never sent a new capture request after access was
granted. They could also open the wrong pane because the wire state was only an
aggregate `permission_denied`.

## Root Cause

The daemon omitted the denied source from `listening_state_changed`. Both
expanded views returned immediately whenever the aggregate state was denied, so
their controls could not reach `startListening`.

## Fix Summary

The listening-state command now optionally carries `permission_denied_source`
(`system` or `microphone`). Native monitor and immediate start failures attach
the source when known. Shared meeting state opens the matching allowlisted pane
on the first click and marks that source ready to retry; the next click issues a
fresh capture request. Expanded controls label only the affected source and
change from Grant to Retry after Settings has opened. The source field remains
present when the other source is still live, so a microphone denial cannot be
hidden by active system capture (or vice versa).

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-core/src/overlay.rs` | Added the backward-compatible denied-source wire field and serialization test. |
| `crates/cue-daemon/src/app.rs` | Propagated known system/microphone permission failures. |
| `crates/cue-meeting-overlay/ui/src/lib/types.ts` | Added source-aware listening payload types. |
| `crates/cue-meeting-overlay/ui/src/lib/client.ts` | Updated the client subscription contract. |
| `crates/cue-meeting-overlay/ui/src/lib/tauriClient.ts` | Validated and translated the wire source. |
| `crates/cue-meeting-overlay/ui/src/lib/meetingState.tsx` | Centralized the Settings-then-retry recovery state. |
| `crates/cue-meeting-overlay/ui/src/screens/AskScreen.tsx` | Repaired legacy expanded controls. |
| `crates/cue-meeting-overlay/ui/src/screens/OpenFloorScreen.tsx` | Repaired floor-plan expanded controls. |
| `crates/cue-meeting-overlay/ui/src/components/Composer.tsx` | Added source-specific Grant/Retry affordances. |
| `crates/cue-meeting-overlay/ui/src/components/floorplan/FloatingStack.tsx` | Added source-specific Grant/Retry affordances. |
| `crates/cue-meeting-overlay/ui/src/components/Pill.tsx` | Surfaced source-aware permission recovery in the legacy collapsed pill. |
| `crates/cue-meeting-overlay/ui/src/components/FloorplanPill.tsx` | Surfaced source-aware permission recovery in the floor-plan collapsed pill. |
| `CHANGELOG.md` | Recorded the recovery fix. |

## Edge Cases Handled

- System permission denial does not block starting microphone-only capture.
- Microphone permission denial does not block starting system-only capture.
- A live source keeps its active state while the denied source shows Grant/Retry.
- Older daemon builds without a denied-source field retain a safe fallback.
- A failed retry can be retried again without being trapped in a Settings loop.

## How to Test

```bash
cargo test -p cue-core permission_denied_state_serializes_source
cargo test -p cue-core listening_state_can_report_a_denied_secondary_source
cargo check -p cue-daemon
cd crates/cue-meeting-overlay/ui && npm run build
```

Manually deny each macOS permission, click the affected expanded control, grant
access in the opened pane, return to Bluey, and click the control labeled
`Retry`.

## Known Limitations

- When an older daemon omits the source, both source controls may offer their
  respective grant flow until one is retried.
