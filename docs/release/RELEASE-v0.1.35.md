# Bluey v0.1.35

## Summary

This release finalizes production-facing updater and uninstall copy.

- Older unverifiable installs now get a production recovery message instead of local-testing/operator wording.
- `bluey uninstall --help` now says `Remove Bluey from this device`.
- `--purge-data` copy refers to device account tokens and device data.
- macOS and Windows installers say `Bluey document tools`, not `Bluey-local document tools`.
- Signed-update verification remains strict.

## Verification

- `cargo test -p cue-cli old_build_update_message_is_production_safe --quiet`
- `cargo test -p cue-cli unverified_manifest_is_not_installable_by_default --quiet`
- `cargo check -p cue-cli --quiet`
- `cargo fmt --all`
- `cargo run -p cue-cli --bin bluey --quiet -- uninstall --help`
- live `latest.json.sig` verified successfully
- live artifact checksum verified against `SHA256SUMS.txt`
- temp-root installer smoke verified `bluey 0.1.35` and `bluey uninstall --help`

## Notes

`v0.1.34` was published first with the updater recovery wording. `v0.1.35` supersedes it with the final production-copy cleanup for install and uninstall output.
