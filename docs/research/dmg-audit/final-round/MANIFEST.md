# Final Round 2.4.0 DMG manifest

Audit date: 2026-07-12

Artifact: `/Users/uno/Downloads/dmg_backtrack_code/final-round-desktop-2.4.0-arm64-mac.dmg`

Bluey comparison commit: `eb923a89f24b69664c73479607bf665725789658`

## Scope and method

This is an evidence-preserving interoperability and implementation-comparison audit of the owner-supplied Final Round desktop DMG. Static inspection used a read-only, non-browsing mount. A trusted local Electron ASAR utility expanded the archive into a disposable directory; no bundled script or package lifecycle hook ran. Two explicitly approved, outbound-network-denied unauthenticated launch attempts were bounded and cleaned up. The first failed before UI due to a harness conflict; the second reached one renderer under a clearly labeled harness-only `--no-sandbox` flag. No credentials, clicks, permission grants, updates, payments, or external actions were used. See [runtime evidence](evidence/runtime-unauthenticated.txt).

## Identity and integrity

| Field | Observed value | Evidence |
|---|---|---|
| DMG | `final-round-desktop-2.4.0-arm64-mac.dmg` | [hashes](hashes.txt) |
| Size | 180,434,644 bytes | [identity](evidence/identity.txt) |
| SHA-256 | `fbeeb1fc108ef4808d0e3a7af72dfa1f5700645bfbb6809126e1183bb381cabc` | [hashes](hashes.txt) |
| Image | UDZO/zlib, checksummed, unencrypted, read-only; APFS payload | [identity](evidence/identity.txt) |
| Filesystem volume name | `Untitled` | [identity](evidence/identity.txt) |
| App | Final Round 2.4.0 (build 2.4.0) | `Info.plist`; [identity](evidence/identity.txt) |
| Bundle ID / URL scheme | `com.finalround.desktop` / `frai` | `Info.plist`; [identity](evidence/identity.txt) |
| Minimum macOS | 12.0 | `Info.plist`; [identity](evidence/identity.txt) |
| Architecture | thin arm64 | `file`, `lipo -info`; [identity](evidence/identity.txt) |
| Framework | Electron 39.2.7 | package/framework metadata; [identity](evidence/identity.txt) |
| Signing | Developer ID Application: Final Round AI Inc (JP52NT2HD8) | `codesign`; [identity](evidence/identity.txt) |
| Gatekeeper/notarization | accepted, stapled notarization valid | `spctl`, `stapler`; [identity](evidence/identity.txt) |
| ASAR integrity | Electron header hash exactly verified | [hashes](hashes.txt); [identity](evidence/identity.txt) |

## Bundle inventory

- Electron helpers: main/general, GPU, Renderer, and Plugin; Electron, Squirrel, ReactiveObjC, and Mantle frameworks; Squirrel ShipIt and crashpad update/crash helpers. No launch agent, daemon, login item, app extension, or XPC service appeared in the complete bundle inventory. [Identity evidence](evidence/identity.txt)
- Principal resources: `app.asar` (80,124,272 bytes), `silero_vad.onnx`, `audio_capture.node`, `audio_detect.node`, `keyboard_monitor.node`, icon assets, and `app-update.yml`. Hashes are in [hashes.txt](hashes.txt).
- The ASAR contained 6,719 files and 220,440 KiB expanded, including cross-platform native dependencies. First-party output had no sourcemaps, TypeScript, or TSX; original source history is therefore unavailable. [Identity evidence](evidence/identity.txt)
- Native audio capture links CoreAudio, AudioToolbox, AudioUnit, AVFoundation, CoreMedia, and ScreenCaptureKit; keyboard and microphone-state addons link the expected macOS frameworks. This identifies implementation primitives, not source provenance. [Architecture evidence](evidence/architecture-static.txt)

## Signing and entitlement boundary

The current app and inspected helpers/addons verify correctly, are notarized, and use hardened runtime. Their entitlement set is nevertheless broad: JIT, unsigned executable memory, DYLD environment variables, disabled library validation, audio input, and camera are allowed, while no App Sandbox entitlement was observed. `Info.plist` also allows arbitrary ATS loads and local-network exceptions. These are observed attack-surface facts, not evidence of exploitation. [Identity evidence](evidence/identity.txt)

## Update system

`electron-updater` 6.3.9 uses a generic HTTPS feed at `https://releases.finalroundai.com/latest`, checks after one minute and every four hours, does not auto-download, and installs on quit after user flow. The current bundle is properly signed/notarized. The application-level `verifyAuthenticodeChain` branch is a no-op off Windows; this does not prove that electron-updater omits macOS signature validation. Sources: `app-update.yml`, `main:5997-6021`, `main:6767-6769`, `main:6882-7124`; [security evidence](evidence/security-static.txt).

## Provenance and reuse decision

No Final Round code or assets should be copied into Bluey. The useful findings are architectural patterns and product behaviors. Reuse is limited to independently implementing ideas already supported by public platform APIs or Bluey's own code after dependency/license review. The opaque/minified first-party bundle, proprietary server contracts, public ingestion identifiers, model asset, UI assets, and native addons are `reject for direct reuse`. See [Bluey gap map](BLUEY-GAP-MAP.md).

## Known limits

- Authenticated and permissioned behavior was intentionally not exercised; backend persistence, authorization, encryption, retention, and billing enforcement remain unknown. [Network evidence](evidence/network-static.txt)
- Native binaries are partly stripped, first-party JavaScript is minified, and server code is absent.
- The successful runtime used `--no-sandbox` only as an approved test-harness workaround under an outer macOS network sandbox. No production renderer-sandbox conclusion is drawn. [Runtime evidence](evidence/runtime-unauthenticated.txt)
- Startup registered a `frai -> com.finalround.desktop` LaunchServices preference entry. Scoped bundle unregistration ran, but a stale preference entry remained; the audit did not reset or rewrite LaunchServices. [Runtime evidence](evidence/runtime-unauthenticated.txt)

## Cleanup verification

All Final Round PIDs were terminated, its own image device `/dev/disk5` was ejected, and disposable runtime/ASAR/audit directories were deleted. Separate mounted images belonging to concurrent work were not touched. [Runtime evidence](evidence/runtime-unauthenticated.txt)
