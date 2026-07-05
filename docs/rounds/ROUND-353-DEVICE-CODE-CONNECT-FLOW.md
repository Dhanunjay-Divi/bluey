# Round 353 - Device Code Connect Flow

## Trigger

- The owner asked whether a local Bluey device code could be shared with a friend who has credits, causing that friend's account to connect automatically.
- Desired flow: `bluey on` should show the code in Terminal, open the browser, let the user sign in or create an account, and also support "already have an account, enter code to connect" clearly.

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Security Behavior

- Device codes are still one-time and short-lived.
- Approval still binds the desktop device to the currently authenticated browser account.
- Existing server tests confirm:
  - an approved code can be polled only once,
  - another account cannot overwrite a code after the owner approves it.
- A shared code is therefore an explicit authorization request, not a password. The risk is human approval of the wrong code, so this round makes the confirmation copy clearer.

## What Changed

- `/auth/device/start` now returns a verification URL that includes the generated `user_code`:
  - `https://bluey.sh/login?user_code=XXXX-XXXX`
- CLI and daemon login URL builders now avoid duplicating `user_code` if the server already included it.
- `bluey login` and background `bluey on` prompts now warn that the code should be approved only if it matches the user's own Bluey desktop and expires in 10 minutes.
- The login/dashboard page now renders a "Have a Bluey desktop code?" entry form even when the URL has no code.
- If a code is present, the browser page now says the desktop showing that code will connect to the account signed into that browser.
- Approval remains an explicit `Connect desktop` click after sign-in.
- Desktop workspace version bumped to `0.1.92`.

## Windows Parity

- Server, CLI, daemon, and web changes are platform-neutral.
- Windows builds should get the same connect-code behavior when the Windows artifact is built from this commit.

## Verification

```bash
node --check web/assets/bluey-site.js
cd /Users/uno/Downloads/cue && node --check web/assets/bluey-site.js
cargo test -p cue-cli device_login_url -- --nocapture
cargo test --manifest-path server/Cargo.toml auth_device_ --test integration_e2e -- --nocapture
```

Results:

- Runtime web JS syntax passed.
- Parallel web checkout JS syntax passed.
- CLI device-login URL tests passed, including the prefilled server-code case.
- Server device-flow tests passed:
  - `auth_device_poll_is_single_use_after_approval`
  - `auth_device_approve_cannot_overwrite_approved_code`

## Current State

- Product code is changed locally and ready for release.
- Deployment status will be updated after API/web/desktop publishing.
