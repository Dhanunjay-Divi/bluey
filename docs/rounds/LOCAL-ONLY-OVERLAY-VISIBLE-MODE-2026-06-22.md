# Local-only overlay visible mode

## Why

Normal Bluey overlay windows are capture-excluded so screen share, screenshots, and screen analysis do not include Bluey itself.
For local visual QA we still need a way to make the overlay visible to screenshots, especially when testing layout, scroll behavior, and screen-answer flows.

## Change

- Added `scripts/bluey-visible-local.sh` as the supported local command:

```bash
scripts/bluey-visible-local.sh
```

- The helper restarts Bluey with all three required local/debug flags:
  - `BLUEY_DEV_OVERLAY=1`
  - `BLUEY_LOCAL_VISIBLE_OVERLAY=1`
  - `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1`
- The daemon now passes the local-visible flag through to both raw overlay helper launches and `.app` launches.
- The macOS overlay now allows capture-visible mode in release-built local binaries only when all three gates are present.
- The visual smoke script uses the same three-flag handshake.
- The release hygiene scan now fails on visible-overlay flag assignments outside allowlisted local QA scripts or docs.

## Production guard

Default `bluey on` remains capture-excluded.
Release/deploy paths must not set any visible-overlay flags.
If one visible flag accidentally appears without the full local/debug handshake, the overlay stays capture-excluded.

To return from local visible mode:

```bash
bluey off && bluey on
```

