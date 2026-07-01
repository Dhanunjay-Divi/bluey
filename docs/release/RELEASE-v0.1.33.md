# Bluey v0.1.33

## Summary

This release fixes desktop login retry behavior and adds uninstall support:

- `bluey on` opens a one-time desktop-code login flow when the app is not linked.
- Overlay sign-in retries reopen the active desktop-code URL instead of doing nothing.
- The web login page preserves the pending desktop code and keeps the Connect desktop banner visible.
- Device login URLs now include `desktop=1&user_code=...` for clearer account-page copy.
- `bluey uninstall` removes the local desktop install while preserving data by default.
- Installers now tell users about `bluey uninstall`.
- Installer files are hardened so `/install.sh` and `/install.ps1` are served as real files, not the web app shell.

## Verification

- `node --check web/assets/bluey-site.js`
- `cargo test -p cue-cli device_login_url --quiet`
- `cargo test -p cue-cli uninstall_root_detection --quiet`
- `cargo check -p cue-cli --quiet`
- `cargo check -p cue-daemon --quiet`
- `cargo check -p cue-dashboard --quiet`
- `cargo run -p cue-cli --bin bluey --quiet -- uninstall --help`
- `cargo test -p cue-daemon overlay_sign_in_event_is_accepted_by_production_validator --quiet`
- live `latest.json.sig` verified successfully
- live artifact checksum verified against `SHA256SUMS.txt`
- temp-root installer smoke verified `bluey 0.1.33` and `bluey uninstall --help`

## Notes

The Mac release artifact is published through the standard signed Bluey release script. Windows CLI uninstall parity is included in source and installer copy; Windows release packaging still depends on the Windows build host.
