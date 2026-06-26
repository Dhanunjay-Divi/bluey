# Round 103 - Answer Style And Route Badge Polish

Date: 2026-06-22

## Why

A normal explanation like "what is a VPC" was still coming back with a polished
reference-note shape: headings, bullets, and generic definitions. That felt too
AI-written for live use, especially when the user had already supplied a style
prompt for a more spoken answer.

The overlay also did not clearly show which Bluey lane was being used while the
answer was starting.

## Changes

- Tightened the daemon human-speak contract so simple definition/explanation
  questions start as 2-4 natural spoken sentences instead of a default bullet
  outline.
- Added explicit handling for user-provided prompt/style/interview guides so
  Bluey uses those for tone and format while treating ordinary attached docs as
  context.
- Added a quick-answer output rule: compact spoken answers are preferred over
  polished reference notes for "what is" / "explain" questions.
- Simplified the macOS route badge during answer startup:
  - normal auto answers show `Auto · Balanced`
  - screen/image answers show `Auto · Vision`
  - manual menu choices show `Instant`, `Balanced`, or `Deep`
- Reset the badge back to `Ready` after plain answers finish, while keeping
  workbench labels for code/design/screen artifacts.

## Product Rule

Bluey should feel like a live copilot first and a reference document second.
If the user asks casually, answer casually. Use the canvas/workbench only when it
actually helps the task.

