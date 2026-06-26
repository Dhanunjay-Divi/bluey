# Round 138 - Caption Strip Click-Through

## Why

The live captions preview row was still catching clicks in click-through mode. That made the app behind Bluey impossible to click wherever the `READY | Live captions preview` strip covered it.

## Changed

- macOS: the captions strip now passes clicks through by default.
- macOS: the caption clear button remains clickable when it is visible.
- macOS: the composer hit-area no longer steals caption-strip clicks through its padding.
- Windows: the caption clear button stays clickable while the rest of the middle overlay area remains transparent to normal clicks.

## Verified

- Built the macOS overlay with `./native/macos/cue-overlay/build.sh`.
- Compiled the Windows overlay smoke binary with MinGW.
