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

- Deployed live in this round.
- Desktop release `0.1.92` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.92/bluey-0.1.92-darwin-arm64.tar.gz`
- Artifact SHA256:
  `67a2d4c9081bb4cb19717ca31aadd113614102a1c10d7d6f73c55abbe15db935`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.92`
- Production API health reports commit `c2619f818dcf8cd081c4ff0a078448a6b36244b8`.
- Production binary SHA256:
  `76490dbe730790fae7a2ac8077955a843937807471f3b4635531addb22779dfd`
- Previous API binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260705T032322Z`
- Live `/auth/device/start` smoke confirmed:
  - response includes a `user_code`
  - `verification_uri` is `https://bluey.sh/login?...`
  - `verification_uri` contains the same `user_code`
  - code TTL is 600 seconds
  - polling interval is 5 seconds
- Live web JS already contains the manual desktop-code entry and explicit approval copy.
- Recent production warning/error scan after restart returned no entries.
- Public installer smoke installed `0.1.92` locally and both installed binaries report `0.1.92`.
- Non-interactive install could not prompt sudo and correctly fell back to `/Users/uno/.local/bin/bluey`.
