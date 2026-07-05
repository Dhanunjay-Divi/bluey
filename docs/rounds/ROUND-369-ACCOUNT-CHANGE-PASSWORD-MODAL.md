# Round 369 - Account Change Password Modal

Date: 2026-07-05
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Match Pinky's account menu behavior by letting signed-in Bluey users change their password from the dashboard without leaving for the reset-password page.

## Changes

- Changed the account-menu `Change Password` item from a reset-page link to an in-dashboard modal.
- Added current password, new password, and confirm password fields with compact Bluey styling.
- Added frontend validation for missing current password, short new password, and confirmation mismatch.
- Added authenticated `POST /auth/password/change`.
- The server verifies the current password, hashes the new password with the existing password policy, updates the account hash, revokes old refresh tokens, and returns fresh auth tokens.
- The browser stores the fresh tokens after a successful password change.
- Added dark/light modal styling.
- Updated the static bundle cache key.

## Production Notes

- During deploy prep, production `bluey-api.service` was already failing with `status=203/EXEC` because `/usr/local/bin/bluey-server` had been replaced by a macOS arm64 binary.
- Restored the previous Linux x86_64 binary from `/opt/bluey-build-codex-round363-try-us/server/target/release/bluey-server`, reset the systemd failure state, and confirmed `/health` returned 200 before deploying this round.
- Built the new Linux release binary on the droplet from `/opt/bluey-build-codex-round369-password-modal`.
- Installed the new binary to `/usr/local/bin/bluey-server` and restarted `bluey-api.service`.

## Files

- `server/src/api/auth_routes.rs`
- `server/src/api/mod.rs`
- `web/index.html`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`

## Verification

- Passed local `cargo fmt --check`.
- Passed local `node --check web/assets/bluey-site.js`.
- Passed local `git diff --check`.
- Passed local workspace `cargo check`.
- Passed local `cargo check` in `server/`.
- Passed remote `cargo check --manifest-path server/Cargo.toml --bin bluey-server`.
- Passed remote `cargo build --release --manifest-path server/Cargo.toml --bin bluey-server`.
- Deployed static web assets to `bluey.sh`.
- Confirmed `https://bluey.sh/account` serves cache key `2026070426` and includes the change-password modal.
- Confirmed deployed JS and CSS include the modal behavior and styles.
- Confirmed unauthenticated `POST /auth/password/change` returns `401`.
- Confirmed `https://bluey.sh/health` returns 200 on the Linux x86_64 API binary.
- Confirmed protected installer/signature artifacts still return 200 after static deploy.
- Confirmed no warning/error/panic/failed logs in `bluey-api.service` after restart.
