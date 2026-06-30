# Bluey 0.1.20

## Update Restart Reliability

- `bluey on` now gives the daemon up to 20 seconds to become ready after launch, instead of failing after 5 seconds.
- Startup now detects if the daemon process exits early and reports that directly.
- `bluey off` now waits for the daemon to stop before returning, reducing auto-update restart races.
- The auto-update relaunch path now starts the current installed `bluey` binary path instead of relying on PATH order.

## Listen Responsiveness

- Listen still requires verified desktop sign-in before any audio capture or STT billing can start.
- After a successful account verification, the daemon caches that verification briefly so repeated Listen toggles do not re-run the same `/account/me` check every click.
- Logout and failed account checks clear the Listen verification cache.

## Verification

- `cargo check -p cue-cli --quiet`
- `cargo check -p cue-daemon --quiet`
- `cargo test -p cue-cli relaunch_ --quiet`
- `cargo test -p cue-daemon listen_auth_gate_requires_linked_cloud_account --quiet`
- Release artifact scanned clean for configured secrets and visible-overlay dev flags.
- `latest.json` is signed with the Bluey Ed25519 release key.
- Live installer smoke from `https://bluey.sh/install.sh` installed `bluey 0.1.20`.
- Local install smoke started the `0.1.20` daemon successfully.
