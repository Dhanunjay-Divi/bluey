# ROUND-459 Legal Acceptance And Login Device Link

Date: 2026-07-09
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Why

The question was whether Bluey already had a durable Terms/Privacy acceptance path. The web UI had checkbox gating in account flows, but that was not enough by itself: support, disputes, and trial abuse review need server-side evidence tied to the account and request signals.

The same round also covered a related sign-in problem: running `bluey login` could approve the browser flow but not reliably show the desktop under **My Computers**, because the CLI login path did not send the same stable desktop identity that `bluey on` uses.

## What Changed

- Added a durable `legal_acceptances` table for SQLite and Postgres.
- Added `server/src/db/legal_acceptances.rs` as the shared insert path.
- Recorded acceptance for:
  - `trial_terms_privacy`
  - `signup_terms_privacy`
  - `trial_convert_terms_privacy`
- Stored account ID, purpose, Terms version, Privacy version, Terms/Privacy text hashes, hashed email/IP/user-agent/device/IP+UA signals, metadata JSON, retention expiration, accepted timestamp, and created timestamp.
- Required `terms_accepted` for Try Us and signup start server-side.
- Added the Try Us Terms/Privacy checkbox before creating the 15-minute temporary trial.
- Added integration coverage proving Try Us rejects missing consent and writes the acceptance ledger when accepted.
- Updated `bluey login` to run the same pre-login update gate as `bluey on`.
- Updated `bluey login` to send stable desktop identity metadata during device flow approval so the approved desktop can appear under **My Computers**.

## Design Notes

- Signup acceptance is recorded when the account is actually created during email verification, because before that there is no durable account row.
- Try Us and trial conversion record immediately because they already have an account ID.
- The ledger is append/idempotent by account, purpose, Terms version, and Privacy version.
- Raw IP, user agent, and device identifiers are not stored in the ledger; only hashed request signals are stored.
- The acceptance purpose names are legal/audit intent names, not product UI names, so Try Us can be queried directly as `trial_terms_privacy`.
- This is not a fake web UI flag. The server rejects trial/signup requests without consent and records acceptance server-side after the account exists.

## Verification

- `node --check web/assets/bluey-site.js`
- `cargo check -p cue-cli --quiet`
- `cargo check --manifest-path server/Cargo.toml --quiet`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e trial_start_requires_and_records_terms_acceptance --quiet`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e signup_otp_email_confirms_and_marks_email_verified --quiet`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e signup_after_account_delete_reuses_email_without_new_trial --quiet`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e trial_start_creates_temporary_account_with_fifteen_minutes --quiet`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e temporary_trial_converts_to_verified_account --quiet`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e signup_start_requires_turnstile_when_flagged --quiet`
- `cargo fmt --manifest-path server/Cargo.toml --check`
- `git diff --check`

## Not Deployed

Per instruction, this round was not deployed and did not use GitHub Actions. Changes are local until an explicit deploy request.

## Follow-Up

- If we want evidence for abandoned signup OTP attempts, add a separate pre-account acceptance/evidence table keyed by email hash and OTP request ID.
- Trial usage is still a shared account-level pool. For temporary trials, the production-safe behavior should be one active trial work session at a time, or a concurrency-safe reservation/debit for every simultaneous session.
