# Bluey v0.1.32

## Summary

This release fixes the overlay movement model:

- Bluey opens with click-through off by default.
- In click-through-off mode, blank overlay space drags/moves the window.
- History and keyboard/tone popups also allow blank-area window dragging.
- In click-through-on mode, blank space clicks through and a dedicated blue/cyan move handle appears for repositioning.
- Shortcut help copy is shorter and explains the two modes.
- Windows source has the same default and move-handle behavior.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo check -p cue-daemon --offline`
- `cargo check -p cue-dashboard --locked`
- `cargo test -p cue-daemon --lib --locked`
- `cargo test -p cue-core sign_in_event_serializes --locked`
- `git diff --check`
- Release artifact dev-flag/secret scan passed.
- Live `latest.json` reports `0.1.32`.
- `latest.json.sig` verified successfully.
- `https://bluey.sh/install.sh` serves `application/x-shellscript`.
- Local update installed `bluey 0.1.32` with `overlay_capture_excluded: true`.

## Notes

The Mac release artifact is expected to publish through the standard Bluey release script. Windows source parity is included, but this Mac host does not produce the MSVC Windows ZIP artifact.
