# Round 319 - Tone Typing Indicator

## Trigger

The owner reported that clicking the Tone editor should show a clear blue typing/focus indicator so users can immediately tell the Tone input is active.

## Root Cause

- The Tone field had a cyan insertion point, but the field border was subtle and did not visibly change when the field editor became active.
- In white theme, the active typing state could feel too quiet because the input background stayed bright while the focus border remained low contrast.

## Fix

- Added a dedicated Tone typing indicator beside the Tone input.
- Brightened the Tone input border and added a blue glow while the Tone field is focused.
- Wired the indicator to Tone open, click/begin editing, end editing, save, dismiss, and theme refresh paths.
- Kept the cursor behavior unchanged: the overlay still uses the normal arrow cursor inside Bluey controls.
- Windows parity check: the current Windows overlay does not have the same Tone editor control path, so there was no equivalent Windows UI to update in this round. The Windows overlay still passed syntax verification.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo check -p cue-daemon --quiet`
- `cargo test -p cue-core overlay --lib`

## Current State

The local code is tested and ready for desktop release packaging as `0.1.58`.

## Remaining QA/Gates

- Publish the signed desktop artifact.
- Smoke-test Tone in dark and white themes after update install.
