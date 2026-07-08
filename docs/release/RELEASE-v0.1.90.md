# Bluey 0.1.90

## Summary

- Enables the Windows release lane so the public manifest can include `windows-x86_64`.
- Removes the preview-only guard from `install.ps1`; Windows installs now use the same signed `latest.json` and checksum-pinned artifact flow as macOS.
- Carries forward the code-follow-up behavior from `0.1.89`: code follow-ups replace the active code canvas in place and request the complete updated implementation.

## Verification

- macOS arm64 and Windows x86_64 artifacts were built in GitHub Actions and downloaded before publish.
- Release hygiene scan passed for both published artifacts.
- `latest.json` and `latest.json.sig` were verified after publish.
- macOS arm64 binary version smoke passed after unpacking the live artifact.
- Windows x86_64 live artifact SHA verification passed.
