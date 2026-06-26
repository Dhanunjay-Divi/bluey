# Round 205 - Local Visible Mode Run

## Trigger

Owner asked to run Bluey in visible mode so the overlay can be seen in screenshots or screen recording.

## What Happened

The first attempt used the installed release CLI via `scripts/bluey-visible-local.sh`.
It restarted Bluey, but the release daemon/overlay intentionally compile out capture-visible behavior, so the visible flags were not present on the running overlay process.

To run real visible mode, rebuilt and ran the debug desktop stack:

```bash
cargo build -p cue-cli -p cue-daemon
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh
```

## Verification

Confirmed the running overlay process is from the debug build and includes the local visible flags:

```text
/Users/uno/Downloads/cue/native/macos/cue-overlay/.build/BlueyOverlay.app/Contents/MacOS/bluey-overlay-macos
--bluey-dev-overlay
--bluey-local-visible-overlay
--bluey-overlay-capture-visible
```

Running debug daemon:

```text
/Users/uno/Downloads/cue/target/debug/bluey-daemon
```

## Current State

- Bluey is currently running in local visible mode from debug binaries.
- This is QA-only and should not be used for production release artifacts.
- Return to normal capture-excluded mode with:

```bash
target/debug/bluey off
bluey on
```

Or, if using only installed release binaries:

```bash
bluey off && bluey on
```
