# Remote Control Best-Effort Passthrough - 2026-06-22

## Goal

Reduce accidental Bluey clicks when a remote-control user is driving the host machine.

## Behavior

- Existing manual interactive/click-through mode remains the reliable override.
- Pinky-trusted input events still trigger short full-window passthrough.
- Bluey now also watches event source process ids for common remote-control apps and briefly ignores mouse events when one is detected.
- Bluey also checks the frontmost app for common remote-control clients and briefly arms click-through while one is active.

## Known Limit

macOS does not reliably label every third-party remote click as remote. If a remote tool injects mouse events that are indistinguishable from local HID events and does not expose a recognizable process id/frontmost app, Bluey cannot perfectly classify it. In that case, explicit click-through mode is still required.

## Remote App Signatures

The best-effort matcher includes common clients such as AnyDesk, Chrome Remote Desktop, Jump Desktop, Microsoft Remote Desktop, Parsec, RealVNC, Remotix, RustDesk, Screen Sharing, Splashtop, TeamViewer, and VNC Viewer.

## Verification

The macOS overlay release build verifies the heuristics compile with AppKit/CoreGraphics event handling.
