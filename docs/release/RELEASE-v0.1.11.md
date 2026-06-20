# Bluey v0.1.11

Live-test release for the paid alpha path.

## Included

- Latest Bluey desktop fixes for transcript-to-answer flow, managed answer prompts, and overlay polish.
- Account, reload, and billing safety fixes for the live Square-backed credit path.
- Hardened managed streaming, account reload, browser session, and pricing copy updates already deployed to `bluey.sh`.
- Signed update manifest support remains required for auto-update and install.

## Operator Notes

- Publish with the Ed25519 release signing key; do not publish unsigned manifests.
- Verify `latest.json`, `latest.json.sig`, artifact checksums, and `curl https://bluey.sh/install.sh | bash` before live testing.
- `bluey-dev.db` is local-only and must not be staged.
