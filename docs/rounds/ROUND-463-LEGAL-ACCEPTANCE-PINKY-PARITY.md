# ROUND-463 Legal Acceptance Pinky Parity

Date: 2026-07-09
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User Issue

The user asked whether Bluey has the same kind of durable Terms/Privacy acceptance evidence as Pinky, specifically for Try Us. The existing Bluey path had a legal acceptance ledger and server-side `terms_accepted` enforcement, but Try Us was recorded as `try_us_trial` and the ledger did not yet store policy text hashes or an explicit retention timestamp.

## Changes

- Renamed legal acceptance purposes to explicit audit names:
  - `trial_terms_privacy`
  - `signup_terms_privacy`
  - `trial_convert_terms_privacy`
- Expanded the legal acceptance schema for SQLite and Postgres with:
  - Terms text hash
  - Privacy text hash
  - email/request signal hash
  - IP + user-agent combined hash
  - retention expiration timestamp
  - explicit accepted timestamp
- Kept raw IP, raw user agent, and raw device identifiers out of the ledger; Bluey stores hashed request signals.
- Updated the Try Us integration test to verify the richer evidence row, not just row existence.
- Updated the prior legal-acceptance round doc so it no longer references the old `try_us_trial` purpose.

## Notes

- The web UI already gates Try Us behind the Terms/Privacy checkbox and sends `terms_accepted: true` only after the checkbox is checked.
- The backend rejects `/auth/trial/start` when `terms_accepted` is false, so the trial is not created before consent.
- No deploy was performed in this round.

## Verification

- `cargo test --manifest-path server/Cargo.toml --test integration_e2e trial_start_requires_and_records_terms_acceptance --quiet`
- `cargo check --manifest-path server/Cargo.toml --quiet`
- `git diff --check`
