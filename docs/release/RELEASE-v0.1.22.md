# Bluey 0.1.22

## Balance Recovery

- Fixed a daemon-side balance polling recovery issue where the overlay could show `Balance --` even though the CLI account and credits commands could fetch the linked account balance.
- Long-running background balance polling now reloads tokens from the secure token store and immediately retries once after a poll failure, so browser login, CLI login, or token refreshes can repair the running overlay without a full restart.
- Added a focused client test that proves a cached token can be refreshed from the persistent store.

## Verification

- `cargo fmt`
- `cargo test -p cue-cloud-client reload_tokens_from_store_refreshes_cached_tokens -- --nocapture`
- `cargo check -p cue-daemon -p cue-cli --quiet`
- Release artifact scanned clean for configured secrets and visible-overlay dev flags.
- `latest.json` is signed with the Bluey Ed25519 release key.
- Live installer smoke from `https://bluey.sh/install.sh` installed `bluey 0.1.22`.
- Local `bluey credits` returned `Balance: $14.56` after installing and restarting.
