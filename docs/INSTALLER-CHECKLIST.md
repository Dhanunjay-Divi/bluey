# Installer Checklist

Bluey v0.1.0 installs as a macOS arm64 terminal-distributed local-first product.
The longer-term commercial target is a signed desktop product with cloud auth,
visible permissions onboarding, local resilience, and support diagnostics.

## Shared Requirements

- For v0.1.0, bundle `bluey`, `bluey-daemon`, native overlay sidecar, native audio helper, and native whisper helper.
- For future public GUI distribution, bundle settings UI and updater metadata.
- Code signing is deferred until signed installers/app bundles become the distribution model.
- Include app version, build SHA, channel, and update URL in a readable manifest.
- Create per-user config, data, runtime, and log directories on first launch.
- Store refresh tokens only in Keychain or Credential Manager.
- Register URL scheme for auth callback.
- Provide uninstall path that can optionally delete local cache and tokens.
- Include crash/log export command for support.
- Run smoke test after package assembly.

## macOS

- v0.1.0 builds an explicitly architecture-targeted macOS arm64 terminal tarball.
- Future app-bundle release: sign app, helper binaries, and embedded frameworks with hardened runtime.
- Future app-bundle release: notarize and staple the app.
- Request microphone and screen-recording permissions through visible prompts/onboarding.
- Bundle the ScreenCaptureKit/CoreAudio audio helper; do not require loopback-driver installation for the primary audio path.
- Register login item only after explicit opt-in.
- Verify overlay capture exclusion with `NSWindow.sharingType = .none`.
- For v0.1.0, verify `bluey on`/`bluey off` work from a clean terminal install.
- For future GUI release, verify no terminal window is required for normal launch.
- For future auto-update, verify updater preserves permissions, config, tokens, and login item choice.

## Windows

- Build signed installer and signed binaries.
- Bundle and sign the WASAPI audio helper; do not require third-party audio driver installation for the primary path.
- Replace the Windows whisper stub with real whisper.cpp integration.
- Install app per user by default unless enterprise MSI requires machine scope.
- Register tray entry, notification settings, URL auth callback, and updater.
- Request microphone permission and validate WASAPI loopback availability.
- Verify `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` on Windows 10 and 11.
- Add Start Menu shortcut for app and settings.
- Verify uninstall removes services/tasks created by installer.
- Verify enterprise install can disable auto-update through policy.

## First-Run Checklist

1. Launch app.
2. Login through browser/device code.
3. Register device.
4. Choose workspace.
5. Review privacy and retention defaults.
6. Grant microphone permission.
7. Grant screen capture permission if user enables visual context.
8. Confirm visible capture indicator behavior.
9. Select audio devices.
10. Confirm shortcuts.
11. Run a one-minute test meeting.
12. Confirm sync, RAG, and answer health.

## Release Gates

- Fresh macOS arm64 install works without developer tools.
- Upgrade from previous version preserves local data.
- Offline launch works and queues sync events.
- Login refresh survives app restart.
- Export request and deletion request can be created from settings.
- Installer logs are available for support.
- Crash-free smoke run passes on a clean macOS arm64 machine.
- Windows release gate is separate: clean Windows 10/11 overlay, audio, whisper,
  page capture, installer, and uninstall validation must pass before support is
  claimed.
