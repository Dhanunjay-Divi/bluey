# Bluey v0.1.34

## Summary

This release cleans up the production updater recovery path for older installed Bluey builds.

- Replaces developer/local-testing update failure text with production-safe recovery copy.
- Tells users with unverifiable legacy builds to reinstall once from the production installer.
- Keeps strict signed-update verification intact.
- Removes local-testing wording from the installer checksum dependency failure.
- Adds a regression test so the old user-facing wording does not come back.

## Verification

- `cargo test -p cue-cli old_build_update_message_is_production_safe --quiet`
- `cargo test -p cue-cli unverified_manifest_is_not_installable_by_default --quiet`
- `cargo check -p cue-cli --quiet`
- `cargo fmt --all`
- `cargo run -p cue-cli --bin bluey --quiet -- update --check-only`

## Notes

Customers already on very old binaries may need one production reinstall because those binaries cannot verify the signed production manifest. Current builds include the embedded update verifier and continue through the normal signed update path.
