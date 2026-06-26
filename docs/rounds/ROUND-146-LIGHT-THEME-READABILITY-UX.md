# Round 146 - Light Theme Readability UX Pass - 2026-06-23

## Problem

The expanded Bluey overlay in light mode looked washed out over bright and dark backgrounds. Header text, the live captions strip, and the composer placeholder were especially low contrast because several controls still used dark-theme alpha values and transcript attributed strings still hardcoded dark-theme text colors.

## Changes

- Added a dedicated light overlay palette for chrome, panel, raised surfaces, text, borders, and shadows.
- Added a light-mode panel renderer with a solid readable fill, subtle blue edge, and inner highlight.
- Shifted light mode from a flat white surface to a gray Apple-style widget: graphite-gray top and bottom bars, a soft silver conversation card, and a subtle Bluey edge.
- Added a light-mode feed gradient and inner highlight so the main surface feels like material instead of one flat gray block.
- Routed expanded overlay chrome, cards, captions, composer, drawer, toast, and controls through theme-aware colors.
- Made light-mode shell, feed, bars, captions, cards, and controls respond to the opacity value instead of pinning near-white fixed alphas.
- Made the composer placeholder color configurable so it stays readable after theme changes.
- Rebuilt transcript strip attributed text from theme-aware foreground colors.
- Restored idle captions strip contrast in light mode after audio pulse state changes.
- Restored the dark-mode glass renderer to the original near-black gradient, subtle cyan edge, and inner highlight so light theme work does not alter dark theme.
- Fixed dark-after-light redraw by making the feed actively repaint its dark background and clear stale layer contents instead of relying on a previous light-mode draw cache.
- Lowered light-mode alpha floors so the Opacity slider changes the whole light overlay more like it does in dark mode.
- Changed first expanded-open default to the minimum comfortable window size, top-centered below the camera/menu area; saved moved positions still persist after the user drags the window.

## Verification

Commands run:

```sh
./native/macos/cue-overlay/build.sh
cp native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos
cp native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos
rm -rf ~/.bluey/bin/BlueyOverlay.app
cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app
./scripts/bluey-visible-local.sh
printf '{"type":"overlay_show"}\n' | nc 127.0.0.1 57321
screencapture -x /tmp/bluey-light-ux-pass3.png
~/.bluey/bin/bluey overlay opacity 45
screencapture -x /tmp/bluey-light-ux-cli-opacity45.png
~/.bluey/bin/bluey overlay opacity 92
screencapture -x /tmp/bluey-light-ux-cli-opacity92.png
./native/macos/cue-overlay/build.sh
./scripts/bluey-visible-local.sh
~/.bluey/bin/bluey overlay opacity 92
screencapture -x /tmp/bluey-light-grey-pass3-92.png
~/.bluey/bin/bluey overlay opacity 45
screencapture -x /tmp/bluey-light-grey-pass3-45.png
~/.bluey/bin/bluey overlay opacity 92
defaults write sh.bluey.overlay bluey.overlay.lightTheme -bool true
./scripts/bluey-visible-local.sh
~/.bluey/bin/bluey overlay opacity 92
screencapture -x /tmp/bluey-light-polish-pass2-92.png
~/.bluey/bin/bluey overlay opacity 45
screencapture -x /tmp/bluey-light-polish-pass2-45.png
~/.bluey/bin/bluey overlay opacity 92
defaults write sh.bluey.overlay bluey.overlay.lightTheme -bool false
./scripts/bluey-visible-local.sh
~/.bluey/bin/bluey overlay opacity 92
screencapture -x /tmp/bluey-dark-restored-final.png
defaults delete sh.bluey.overlay bluey.overlay.expanded.frame.v1 2>/dev/null || true
defaults delete sh.bluey.overlay bluey.overlay.expanded.frame.v2 2>/dev/null || true
defaults write sh.bluey.overlay bluey.overlay.lightTheme -bool false
./scripts/bluey-visible-local.sh
~/.bluey/bin/bluey overlay opacity 92
screencapture -x /tmp/bluey-default-top-min-dark.png
```

Screenshots checked:

- `/tmp/bluey-light-ux-pass3.png`
- `/tmp/bluey-light-ux-bottom-crop4.png`
- `/tmp/bluey-light-ux-pass5.png`
- `/tmp/bluey-light-ux-cli-opacity45.png`
- `/tmp/bluey-light-ux-cli-opacity92.png`
- `/tmp/bluey-light-ux-pass7.png`
- `/tmp/bluey-light-grey-pass3-92.png`
- `/tmp/bluey-light-grey-pass3-45.png`
- `/tmp/bluey-light-polish-pass2-92.png`
- `/tmp/bluey-light-polish-pass2-45.png`
- `/tmp/bluey-dark-restored-final.png`
- `/tmp/bluey-default-top-min-dark.png`

Result: light mode is readable and no longer looks like a blank white sheet. The top and bottom bars use darker gray widget-style chrome, the conversation surface is a soft silver card with a subtle gradient, and opacity visibly changes the whole overlay. Dark mode is back on locally, using the restored dark glass renderer, and the local visible Bluey install was restarted at 92% opacity. The default expanded window now opens at 760 x 500 near the top center of the screen.
