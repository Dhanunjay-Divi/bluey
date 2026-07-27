# FIX-005: Development Reinstall Kept the Old Daemon Running

## Issue

`scripts/reinstall-dev.sh` rebuilt and copied fresh binaries but a live local
daemon could continue serving the previous build.

## Root Cause

The reinstall script called `bluey off`, whose product contract is to close
only the overlay while leaving the background daemon and calendar scheduler
running. It then copied a new executable over the install path without proving
the old process had exited. The `--no-start` path did not attempt a stop at all.

## Fix Summary

The script now calls the full `bluey quit` shutdown whenever `bluey-daemon` is
running, waits up to five seconds for that exact process name to exit, and
refuses to overwrite the live executable if shutdown does not complete. This
applies whether or not the caller asks the script to restart Bluey afterward.

## Files Modified

| File | Change |
|------|--------|
| `scripts/reinstall-dev.sh` | Use full shutdown, bounded polling, and fail-closed stale-process detection. |

## Edge Cases Handled

- `--no-start` still stops the old daemon before installing.
- An already-stopped daemon proceeds without a redundant IPC request.
- A wedged daemon is not silently overwritten.

## How to Test

```bash
bluey on
scripts/reinstall-dev.sh --no-start
pgrep -x bluey-daemon && exit 1 || true
scripts/reinstall-dev.sh
pgrep -x bluey-daemon
```

## Known Limitations

- A daemon that ignores graceful shutdown must be investigated or stopped
  explicitly; the reinstall script intentionally does not force-kill it.
