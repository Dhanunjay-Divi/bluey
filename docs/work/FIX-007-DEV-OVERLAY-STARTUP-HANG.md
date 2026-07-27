# FIX-007: Visible Development Overlay Can Block Daemon Startup

## Issue

`bluey on` could report that the daemon did not become ready when a visible
development overlay override was configured, leaving a detached daemon process
without its IPC listener.

## Root Cause

`spawn_overlay` derived the development verification root with
`std::env::current_dir()` before binding daemon IPC. A live macOS process sample
showed that call blocked in `getcwd`/`open`, so startup never reached the
`TcpListener::bind` call and the CLI's readiness window expired.

The override path was already absolute and explicit, so resolving the entire
process working directory was unnecessary.

## Fix Summary

Development override verification now scopes the allowed root to the resolved
overlay binary's parent directory. This retains canonicalization and containment
checks while removing the blocking current-directory lookup from the startup
critical path.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Derive the development overlay verification root from the resolved binary path. |

## Edge Cases Handled

- An overlay path without a parent falls back to the filesystem root and still
  goes through canonicalization.
- Production installs continue to derive their trusted root from the daemon
  executable directory.
- Environment overrides remain gated by `BLUEY_DEV_OVERLAY=1` in release builds.

## How to Test

```bash
scripts/reinstall-dev.sh --no-start
BLUEY_DEV_OVERLAY=1 \
BLUEY_MEETING_CAPTURE_VISIBLE=1 \
BLUEY_OVERLAY_BIN="$PWD/target/aarch64-apple-darwin/debug/cue-meeting-overlay" \
~/.local/bin/bluey on --title "Meeting"

~/.local/bin/bluey status
```

The daemon should become ready on `127.0.0.1:57321` and the visible overlay
should open without leaving a listener-less background process.

## Known Limitations

- Filesystem or code-signing failures can still prevent the overlay child from
  starting, but they now fail as overlay errors rather than blocking daemon IPC.
