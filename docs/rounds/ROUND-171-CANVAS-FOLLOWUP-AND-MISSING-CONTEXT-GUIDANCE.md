# Round 171 - Canvas Follow-Up And Missing Context Guidance

## What changed

- Bluey now treats the canvas as current workbench state instead of a place to mirror every answer.
- Related explanation follow-ups keep the existing canvas open and answer in chat.
- Related edit/fix/change follow-ups still attach to the existing canvas path.
- Plain unrelated answers close stale canvas state instead of leaving an old code/design pane open.
- Answer canvases are limited to code and system design artifacts. Screen-analysis text stays in chat unless it produces real code or architecture workbench content.
- The answer prompt now tells Bluey not to guess when a screenshot, test, file, or document is insufficient.
- For coding/debugging screenshots without enough code or error detail, Bluey should ask for the next concrete evidence: run tests, share failure output, show the project tree, and open or attach likely files.

## Why

When users solve coding challenges from screenshots, guessing from partial UI causes wrong answers and stale canvases. The better product behavior is to either solve from enough context or guide the user toward the missing evidence needed to reach the final patch.

## Verification

- Rust prompt tests cover the new missing-context guidance.
- Swift parse/build verification should confirm the canvas routing compiles.
