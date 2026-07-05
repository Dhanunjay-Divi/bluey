# Round 387 - Session Upload Visibility

## Goal

Make Sessions clearly show whether a Bluey desktop conversation has uploaded, and show the uploaded local chat transcript/answers in the web detail view.

## Changes

- Changed Sessions copy to explain that web shows uploaded desktop transcript and Bluey answers, view-only for now.
- Changed empty state to say no desktop sessions have uploaded yet.
- Added per-session upload chips for chat turns, transcript segments, and context items.
- Marked metadata-only sessions as "No chat uploaded yet" instead of implying a complete saved chat.
- Changed session opening to a normal new-tab link.
- Added a Conversation section in session detail that renders uploaded local Bluey ask/answer turns as `You` and `Bluey`.
- Reworded empty detail sections to distinguish missing live transcript, missing conversation turns, and missing context.
- Added light/dark styling for upload chips and empty upload state.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.js web/assets/bluey-site.css`
