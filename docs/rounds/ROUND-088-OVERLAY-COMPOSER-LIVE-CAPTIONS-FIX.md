# Round 088 - Overlay Composer And Live Captions Fix - 2026-06-21

## Why

The live Mac test exposed two customer-facing issues in the overlay:

- the Ask Anything/composer and opacity controls could feel unclickable when the expanded window entered click-through mode
- live transcript preview looked like broken chunks instead of one readable, horizontally scrolling caption line

This round intentionally stays in the native macOS overlay. It does not change provider billing, STT reservation, server routing, or the current production database backend.

## What Changed

- Expanded the guaranteed interactive hit zones in pass-through mode:
  - header controls
  - live transcript strip
  - composer and bottom control band
- Kept the transcript state label stable (`LIVE` while active) instead of flipping between `Mic` and `System`.
- Rendered the source label inside the horizontal transcript line, for example:
  - `Mic: ...`
  - `System: ...`
- Merged live transcript preview text per source so interim/final captions read as a continuous line instead of repeated fragments.
- Cleared the live preview buffer after Answer sends the current transcript context, so later answers do not resend already-consumed transcript text.

## Production Wiring Status

- Valkey/Redis is production-wired through the systemd drop-in and preflight has passed.
- Postgres/pgvector is provisioned and migrated, but production is intentionally still SQLite-backed.
- The Postgres runtime is not safe to flip yet because the current adapter foundation uses a synchronous Postgres pool in the Tokio server path. A staging boot already exposed the runtime nesting failure. The safe next step is an async Postgres adapter or a proven blocking boundary before setting `BLUEY_SERVER_DB_BACKEND=postgres` in production.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- Installed the rebuilt arm64 overlay locally to:
  - `~/.bluey/bin/bluey-overlay-macos`
  - `~/.bluey/bin/cue-overlay-macos`
- Ad-hoc signed both local overlay helpers.
- Restarted local Bluey with the refreshed overlay.

## Remaining Risk

- Need live user click smoke:
  - click Ask Anything and type
  - Enter sends, Shift+Enter inserts a newline
  - click/drag Opacity
  - click Listen and confirm real Mic/System captions appear in the live strip
- Need the Postgres runtime adapter before production can move off SQLite.
