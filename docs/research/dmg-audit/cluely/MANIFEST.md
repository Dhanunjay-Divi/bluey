# Cluely (New) 2.1.19 manifest

## Scope and result

This audit covers `/Users/uno/Downloads/dmg_backtrack_code/Cluely (New) 2.1.19.dmg` and compares its statically observed product with the cited Bluey product files as revalidated at repository commit `eb923a89f24b69664c73479607bf665725789658`. The DMG was mounted read-only. No helper, package hook, bundled script, authenticated flow, or UI was run. One isolated eight-second direct-binary probe reached only the pre-readiness boundary; it was write- and network-denied and is fully described in [runtime-assessment.txt](evidence/runtime-assessment.txt). See also [identity-and-integrity.txt](evidence/identity-and-integrity.txt) and [hashes.txt](hashes.txt).

The product is a signed and notarized universal Electron 40.8.0 desktop meeting/interview copilot. Its core workflow is live microphone plus system-audio capture, local voice-activity detection, cloud transcription and chat, an always-on-top/content-protected overlay, custom prompt-and-file modes, meeting/calendar context, session history, and paid usage gates. Static evidence does **not** support job discovery, browser automation, profile isolation, ATS submission, job-application queues, or application receipts. Those absences are important: Cluely is principally a live conversation assistant, not a Bluey-like end-to-end job application system.

## Identity

| Field | Observed value | Provenance |
|---|---|---|
| DMG filename | `Cluely (New) 2.1.19.dmg` | filesystem stat |
| DMG size | 230,124,489 bytes | filesystem stat |
| DMG SHA-256 | `411b2f24…e7effc` | `shasum -a 256`; full value in [hashes.txt](hashes.txt) |
| Image format | UDZO/zlib, unencrypted, checksummed | `hdiutil imageinfo` |
| Volume | `Cluely (New) 2.1.19-universal` | read-only `hdiutil attach` |
| App | `Cluely (New).app` | mounted volume |
| Bundle ID | `com.cluely.app.april22` | `Contents/Info.plist` |
| Version/build | 2.1.19 / 2.1.19 | `Contents/Info.plist` |
| Minimum macOS | 12.0 | `Contents/Info.plist` |
| Architectures | arm64, x86_64 | `file`, `lipo -info` |
| Signing identity | Bounty Studio, Inc. (`89PS65RKC6`) | `codesign -dv` |
| Gatekeeper/notarization | accepted; stapled ticket valid | `spctl`, `stapler` |
| Technology | Electron 40.8.0 / Chrome 144.0.7559.236 | framework version resources |

## Integrity and extraction

`app.asar` is 28,435,497 bytes with SHA-256 `33565aeb…06742`. The SHA-256 calculated over its JSON header exactly matches the `ElectronAsarIntegrity` hash recorded in `Info.plist`. A custom read-only parser extracted 3,261 regular files into a temporary directory and verified every file against the embedded ASAR hash; there were zero mismatches. Five link/unpacked entries were skipped rather than followed. Full measurements are in [identity-and-integrity.txt](evidence/identity-and-integrity.txt).

## Bundle composition

- Electron main process: `app.asar/dist-electron/main.js`.
- Preload bridge: `app.asar/dist-electron/preload.mjs`.
- Renderer: Vite-style hashed JavaScript/CSS assets in `app.asar/dist/assets/`.
- Native audio: universal `AudioTee` and SoX helpers; a Windows SoX distribution is also sealed in the bundle.
- Local speech gating: Silero VAD ONNX plus ONNX Runtime Web/WASM.
- Frameworks: Electron, Mantle, ReactiveObjC, Squirrel.
- Electron helpers: general, GPU, Plugin, Renderer.
- Updater: `electron-updater` with an S3/R2 configuration and a production release feed override.
- No app extension, XPC service, launch agent, `Contents/Library`, or `Contents/PlugIns` subtree was found.

## Signing and entitlements

The main app and nested helpers pass deep strict code-sign verification and are universal. The main app and each Electron helper receive `allow-jit`, `allow-unsigned-executable-memory`, audio-input, and camera entitlements. `Info.plist` also declares microphone, camera, audio-capture, and Bluetooth usage descriptions. The app is not App Sandbox-entitled. See [SECURITY.md](SECURITY.md) for the resulting trust-boundary assessment.

## Static-analysis limitations

Minification and bundling preserve useful symbols, route names, RPC procedure names, URLs, state keys, and control flow, but do not prove server-side authorization, response semantics, or production feature-flag values. Marketing copy was not used as proof. The only runtime was a bounded pre-readiness probe because the production main process calls `app.moveToApplicationsFolder()` before normal readiness and initializes the updater/login-item path; see [runtime-assessment.txt](evidence/runtime-assessment.txt). Every UI, authenticated, server-side, or permission-dependent conclusion is explicitly labeled unknown.

## Audit documents

- [ARCHITECTURE.md](ARCHITECTURE.md)
- [FEATURES.md](FEATURES.md)
- [DATA-AND-NETWORK.md](DATA-AND-NETWORK.md)
- [SECURITY.md](SECURITY.md)
- [BLUEY-GAP-MAP.md](BLUEY-GAP-MAP.md)
- [hashes.txt](hashes.txt)
- [evidence/](evidence/)
