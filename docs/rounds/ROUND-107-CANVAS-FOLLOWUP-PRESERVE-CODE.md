# Round 107 - Canvas Follow-Up Preservation

## Problem

Explanation-only follow-ups such as "why are we using `vector<vector>`?" could replace or append the coding canvas. That made Bluey look like it changed the solution when the user only wanted the existing code explained.

## Change

- The macOS overlay now classifies explanation follow-ups separately from edit follow-ups.
- Questions with signals like `why`, `how`, `explain`, or `walk me through` keep the active canvas unchanged.
- Edit signals like `change`, `fix`, `replace`, `optimize`, `refactor`, or `add` can still update the canvas.
- The route badge no longer switches to `Code` for ignored explanation artifacts.
- The daemon prompt now tells managed models not to emit a new code artifact for explanation-only coding follow-ups.

## Manual Check

1. Ask a coding question that opens a canvas.
2. Ask a follow-up such as "why did you use a 2D vector here?"
3. Confirm the answer appears in chat and the original canvas stays intact.
4. Ask an edit follow-up such as "optimize this to a 1D vector."
5. Confirm the canvas updates only for the explicit edit request.
