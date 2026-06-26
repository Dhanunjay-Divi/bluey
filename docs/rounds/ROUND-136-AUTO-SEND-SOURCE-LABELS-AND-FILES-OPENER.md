# Round 136 - Auto Send Source Labels And Files Opener - 2026-06-23

## What changed

- Made system-audio auto-send the default in the overlay.
- Reworded the auto-send dropdown so each option says exactly what will happen:
  - Don't auto-send
  - Auto-send when mic stops
  - Auto-send when system stops
  - Auto-send when mic or system stops
- Tightened auto-send so Stop only sends when the selected audio source has captions ready.
- Changed the top file badge from passive status text into a clear opener:
  - `Show N files`
  - `Hide N files`
- Kept the bottom file row as a horizontal strip and added clearer tooltips telling users to scroll sideways for more files.
- Mirrored the auto-send default and labels in the Windows overlay.

## User behavior

- Default: system audio can auto-send when listening stops.
- If the user does not want this, choose `Don't auto-send`.
- If ten documents are attached, the top badge opens and closes the full attached-file strip.
- The bottom strip remains for pending files and can be scrolled horizontally when there are more files than fit.

## Verification

- `./native/macos/cue-overlay/build.sh`
- Windows overlay compile smoke with `x86_64-w64-mingw32-g++`
- Restarted local Bluey with `./scripts/bluey-visible-local.sh`

