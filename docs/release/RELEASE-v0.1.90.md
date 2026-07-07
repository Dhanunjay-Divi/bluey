# Bluey 0.1.90

## Summary

- Enables the Windows release lane so the public manifest can include `windows-x86_64`.
- Removes the preview-only guard from `install.ps1`; Windows installs now use the same signed `latest.json` and checksum-pinned artifact flow as macOS.
- Carries forward the code-follow-up behavior from `0.1.89`: code follow-ups replace the active code canvas in place and request the complete updated implementation.

## Verification

- macOS artifact is built locally and verified before publish.
- Windows artifact is built on the GitHub Actions Windows runner and downloaded before publish.
- `latest.json` and `latest.json.sig` are verified after publish.
