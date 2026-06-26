# Round 132 - Sticky Pill Position

## Problem

When Bluey was minimized to the small pill, dragged somewhere else, opened, and minimized again, the pill returned to the center. That made the user's chosen placement feel ignored.

## Change

- Fresh launch still defaults the pill to the center of the active screen.
- Dragging the pill now records its final frame for the current overlay session.
- Collapsing from the expanded window restores the recorded pill position instead of recentering.
- Preset position commands also update the sticky pill frame, while the `center` preset clears the sticky frame and returns to default center behavior.

## Manual Check

1. Start Bluey in visible mode.
2. Drag the pill to a screen edge or corner.
3. Open Bluey.
4. Minimize it again.
5. Confirm the pill returns to the place where it was left, not the center.
