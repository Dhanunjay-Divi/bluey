# ROUND-253 Account Delete Link Recovery Guard

Date: 2026-06-30
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Investigate the low balance screenshot, recover the accidentally deleted internal test account if possible, make login-code wording safer, and harden account deletion so users must clearly consent to losing account data and unused credits.

## Findings

- The `$4.82 low` balance shown in the screenshot was not the restored internal admin test account. Live Postgres showed it belonged to `codex-smoke-20260608183100@bluey.sh`.
- `internal-admin-20260606023943@bluey.sh` had been hard-deleted from live Postgres.
- The deleted internal admin account still existed in the hourly SQLite backup at `/var/backups/bluey-api/hourly/bluey-20260630T150001Z.db`.
- The Bluey device login code is an OAuth-style device flow:
  - the browser approval must happen inside an authenticated web account
  - an approved device code is single-use
  - already-approved codes cannot be overwritten to a different account
- The risky part was product clarity: older copy could make the code feel like a magic login token instead of a deliberate "link this desktop to the currently signed-in account" confirmation.
- Account deletion on web/dashboard was still using native browser dialogs in places, which made the warning feel like a Chrome popup instead of a Bluey-owned consent flow.

## Recovered Account

Restored `internal-admin-20260606023943@bluey.sh` into live Postgres from the hourly backup:

- account id: `902803b6-1a4f-478e-97d5-e38c40cbe36b`
- original password hash preserved
- `email_verified_at` preserved
- `is_admin=true`
- `billing_restricted=false`
- auto top-up settings preserved
- balance reset to `$15.00` for internal testing

Added a live balance ledger entry:

- event type: `internal_credit`
- amount: `1500` cents
- reason: `restore_deleted_internal_admin_test_account_to_15_usd`
- metadata notes the restore source and backup path

## Fixed

- CLI login now prints:
  - `Approve this code only in the Bluey account you want this desktop to use.`
- Web account auth copy now says the desktop link must be confirmed after signing in, instead of implying automatic linking.
- Server `/account/delete` now rejects deletion unless the request includes:
  - `confirm_text: "DELETE"`
  - `accept_data_loss: true`
  - `accept_credit_loss: true`
- CLI account deletion now sends the explicit deletion consent payload and warns that unused Bluey credits are lost after deletion.
- Dashboard account deletion now sends the same explicit consent payload.
- Dashboard settings now uses an in-app Bluey deletion modal with:
  - data-loss checkbox
  - credit-loss checkbox
  - typed `DELETE` confirmation
  - disabled delete button until all confirmations are complete
- Web account page now uses an in-app Bluey deletion modal with:
  - data-loss checkbox
  - credit-loss checkbox
  - typed `DELETE` confirmation
  - disabled delete button until all confirmations are complete
- Privacy copy now documents that account deletion removes saved cloud sessions, synced files/screenshots/transcripts/extracted text/generated answers/usage history/search indexes where technically available.
- Terms copy now documents that deleting an account forfeits unused Bluey credits and users should contact support before deletion for billing review.

## User-Facing Policy

Deleting a Bluey account is permanent. It deletes account-related data and any unused Bluey credits are lost. The user must explicitly agree to both before deletion can proceed.

## Verification

Passed locally:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo check -p cue-cli -p cue-dashboard
cargo test --manifest-path server/Cargo.toml account_delete_requires_typed_delete_and_credit_loss_consent -- --nocapture
cargo test --manifest-path server/Cargo.toml auth_device -- --nocapture
cargo build --manifest-path server/Cargo.toml --bin bluey-server
cargo build --release --manifest-path server/Cargo.toml --bin bluey-server
git diff --check
```

Live database verification:

- restored internal admin account exists
- balance is `1500` cents
- latest ledger row is the internal restore credit
- smoke account still shows the lower balance independently

Live production delete-guard smoke:

```text
email=codex-delete-guard-1782835926@bluey.local
empty_payload_status=422 exists=1
missing_credit_status=400 exists=1
full_consent_status=200 exists=0
```

The throwaway test account was cleaned up after the smoke test.

## Deploy Status

Deployed:

- Synced web static files to `/var/www/bluey/`.
- Built the Linux x86_64 server binary on the droplet from `/opt/bluey-build-codex-delete-guard` with the rustup stable Linux toolchain.
- Installed the new binary to `/usr/local/bin/bluey-server`.
- Restarted `bluey-api.service`.

Live checks passed:

```bash
curl -fsS http://127.0.0.1:8080/health
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/assets/bluey-site.js | rg "confirm the desktop link|accept_credit_loss"
curl -fsS https://bluey.sh/ | rg "Account Deletion|unused Bluey credits|confirm the link after signing in"
```

Service status:

- `bluey-api.service` active
- clean restart
- public health returned `status=ok`
- journal showed delete guard requests returning `422`, `400`, then `200` for full consent
- no temporary delete-guard accounts remained

## Remaining

- Consider adding an admin-only "recover deleted internal test account" runbook so future test-account mistakes can be restored without manually inspecting backups.
