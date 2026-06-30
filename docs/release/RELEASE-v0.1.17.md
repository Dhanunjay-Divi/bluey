# Bluey 0.1.17

This release packages the latest Bluey desktop and backend reliability work for the public download path.

## Changed

- Improves AnswerPlan routing diagnostics for code, follow-up, behavioral, transcript, and screen/doc questions.
- Adds privacy-safe answer logs for transcript length/hash, plan source, intent, lane, provider, canvas status, web-search status, and billed cents.
- Adds STT regression smoke coverage for empty transcripts, duplicate transcript sends, no-bill listen start/stop, and mic/system source separation.
- Adds balance warning color states so low balance is clearer in the overlay.

## Operational Notes

- The macOS arm64 artifact is built with the embedded Bluey update public key.
- `latest.json` is signed with the Bluey Ed25519 release key and pins installer/archive SHA256 values.
- Windows remains installer-ready through `install.ps1`, but this release publishes only the macOS arm64 desktop artifact unless a Windows build artifact is supplied by the Windows build host.
