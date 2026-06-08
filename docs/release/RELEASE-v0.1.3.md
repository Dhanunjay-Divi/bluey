# Bluey v0.1.3

Overlay sizing fix:

- Opening the macOS pill now resets the expanded overlay to the compact centered frame.
- The expanded overlay remains manually resizable after it opens.
- This prevents stale AppKit layout state from reopening Bluey as an oversized panel.
