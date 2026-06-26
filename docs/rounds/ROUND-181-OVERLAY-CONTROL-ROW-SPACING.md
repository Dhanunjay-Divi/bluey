# Round 181 - Overlay Control Row Spacing

## Trigger

Owner pointed out that the bottom overlay controls had a huge visual gap between `Opacity` and `Auto-send`, and asked to make that space feel like the rest of the screen.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 02:34 EDT

## Root Cause

- The macOS overlay anchored `Opacity` with the left utility controls.
- `Auto-send` was anchored to the right model/analyze control group.
- The only relationship between the two was a loose less-than-or-equal spacing constraint, so wide overlay widths created a large empty gap.

## Fix

- Anchored `Auto-send` directly after the opacity control with the same 6 px spacing used by adjacent compact controls.
- Kept the model menu and Analyze button on the right by changing the model menu's leading constraint to a flexible greater-than-or-equal relationship after `Auto-send`.
- Left control sizing unchanged, so the fix only affects spacing and does not change the overlay's overall density.

## Mac Windows Parity

- macOS needed the fix because it has a bottom control row with `Opacity`, `Auto-send`, model, and analyze controls.
- Windows has no equivalent opacity/click-through row in this location; its auto-send combo already lives inside the composer/control area beside the input controls.
- No Windows code change was required for this specific spacing issue.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`

## Files Touched

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-181-OVERLAY-CONTROL-ROW-SPACING.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Current State

- `Opacity` and `Auto-send` now sit together as a compact left-side control cluster.
- The model menu and Analyze button still stay on the right side of the composer bar.
- The rebuilt macOS overlay binary has been installed to `~/.bluey/bin` under both expected overlay names.
- The screenshot-reported large gap should be gone after relaunching the macOS overlay.

## Remaining QA Gates

- Manual macOS visual QA after relaunch should confirm the row looks correct at compact, default, and wide overlay widths.
- No Windows GUI QA is required for this round unless a matching Windows opacity/click-through row is added later.
