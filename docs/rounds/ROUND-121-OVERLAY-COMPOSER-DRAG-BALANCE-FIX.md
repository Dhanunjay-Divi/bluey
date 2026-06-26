# Round 121 - Overlay Composer, Drag, And Balance Fix - 2026-06-22

## What was broken

- The expanded macOS overlay could flip the whole window into `ignoresMouseEvents` while the pointer was over generated text or empty space.
- That made the first click on `Ask anything...` or the top header bar pass through the window instead of focusing the composer or starting a drag.
- The simplified empty start surface removed the visible document drop affordance users expected.
- The overlay balance showed `Balance --` because production Postgres refresh-token writes were binding RFC3339 strings into `timestamptz` parameters, causing `/auth/refresh` to return `500`.

## What changed

- The expanded overlay now keeps the window mouse-capable and performs click-through at the view hit-test layer.
- Header, composer, attachment chips, canvas controls, modal panels, resize edges, and buttons remain interactive.
- Generated answer text, captions, screenshots/background areas, and non-control surfaces pass clicks through in click-through mode.
- The compact empty state with `Drop documents here` is restored.
- Postgres auth-time helpers now bind real `chrono::DateTime<Utc>` values for refresh tokens, link codes, device codes, signup OTPs, and verification/reset tokens.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `cargo check --manifest-path server/Cargo.toml`
- `git diff --check`
- `scripts/release-hygiene-scan.sh`
- Local visible mode restarted with the rebuilt overlay.
- Production `bluey-api` was rebuilt on the droplet, restarted, and confirmed healthy.
- The previously failing flow now succeeds: expired local access token -> `/auth/refresh` 200 -> `/account/me` 200 -> `bluey credits` shows `$12.68`.

## Operator note

Visible local mode remains intentionally local-only via `scripts/bluey-visible-local.sh`. Normal deploy/release flows still keep the overlay capture-excluded.
