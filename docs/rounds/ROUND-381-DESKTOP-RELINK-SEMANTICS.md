# Round 381 - Desktop Relink Semantics

## Trigger

Owner clarified that a Bluey desktop code should not imply that one account can absorb another account. A stable desktop can be moved to the currently signed-in account, but the physical desktop should not remain actively linked under multiple accounts at once.

## Product Rule

- One Bluey account can have many linked desktops.
- One stable Bluey desktop belongs to one active account link at a time.
- Approving a code under a different account moves that desktop to the new account and revokes the old active device row.
- Starting a new device-code flow from the same desktop rotates the code by invalidating older unconsumed codes for that stable desktop id.
- No account-level "reset code" UI is needed for now because codes are short-lived, one-time, and now rotated by starting a new desktop sign-in.

## Changes

- `account_devices` upsert now revokes active rows for the same stable `device_id` on other accounts before registering the desktop under the target account.
- Device-code insert now deletes older unconsumed rows for the same stable `device_id`, so a new desktop code supersedes an older one.
- Dashboard/landing copy now says "move this desktop" instead of suggesting a separate browser/account balance link.
- Added tests for moving a desktop between accounts and invalidating older codes for the same desktop.

## Verification

- `node --check web/assets/bluey-site.js` passed.
- `cargo test --manifest-path server/Cargo.toml device -- --nocapture` passed.
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server` passed.

