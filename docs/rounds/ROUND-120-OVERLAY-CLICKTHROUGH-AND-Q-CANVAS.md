# Round 120 - Overlay click-through and Q canvas grouping

## Why

Bluey was still catching clicks across large generated-text regions. That made remote-control sessions and normal desktop work feel fragile because answer text, caption previews, and empty overlay space could intercept the user's click even when the user only meant to interact with the app underneath.

The canvas was also single-slot: each coding or system-design answer replaced the previous workspace. That made multi-question interviews, coding drills, and follow-up edits hard to track.

## Change

- Make the expanded overlay accept mouse events only over real controls:
  - header controls
  - composer / Ask anything
  - Answer, Listen, Screen, attach, tone, model, opacity controls
  - drawers, dialogs, resize edges, copy buttons, and canvas controls
- Remove live caption strip/text from pointer hit-testing so captions pass through.
- Keep generated answer/feed regions pass-through by default.
- Repurpose the interaction-mode button into an informational "controls-only click-through" affordance instead of toggling Bluey back into a full click-eating panel.
- Replace the single `latestCanvas` slot with a canvas list.
- Give new canvas workspaces question-style names such as `Q1 Coding`, `Q2 System Design`, or `Q3 Screen`.
- Add previous/next canvas controls and a compact `1/3` position label in the canvas header.
- Use the nearest preceding question card to decide when a coding/design answer is an obvious follow-up.
- Append obvious follow-up answers into the active same-kind canvas under `FOLLOW-UP N` sections instead of replacing the workspace.
- Fix the local visible-mode helper so it waits for a real daemon PID to disappear instead of trusting `bluey status` exit code.

## Behavior

Normal answers still appear in the chat feed. Canvas is a persistent side workspace for code, system design, screen analysis, document notes, and long structured answers.

The same model request powers both. Canvas is display organization, not a separate provider call.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- Local visible overlay started with `overlay_capture_excluded: false`.
