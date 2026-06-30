# Bluey 0.1.18

## Desktop Login

- `bluey on` now starts the browser device-code sign-in flow automatically when the desktop is not linked.
- The macOS overlay Sign in button now asks the daemon to start the same device-code flow instead of opening a plain login page.
- After browser approval, the daemon saves the local account tokens, refreshes balance/cloud state, and switches the overlay to `Bluey online`.

## Verification

- Release artifact scanned clean for configured secrets and visible-overlay dev flags.
- `latest.json` is signed with the Bluey Ed25519 release key.
- Live installer smoke from `https://bluey.sh/install.sh` completed successfully.
