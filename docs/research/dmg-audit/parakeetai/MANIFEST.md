# ParakeetAI 3.6.21 manifest

## Scope and handling

This audit covers static inspection of the owner-supplied `/Users/uno/Downloads/dmg_backtrack_code/ParakeetAI-3.6.21.dmg` plus one separately authorized, constrained, unauthenticated runtime observation. The image remained read-only and the application was not installed. Static work executed no bundled script, helper, native module, or lifecycle hook; the later runtime pass executed only the signed app entrypoint with isolated temporary state and external networking denied. Exact commands, harness limitations, and cleanup are in [the evidence ledger](evidence/README.md) and [runtime record](evidence/runtime-unauthenticated.txt).

## Identity and integrity

| Property | Observed value |
| --- | --- |
| DMG size | 226,405,227 bytes |
| DMG SHA-256 | `94e86fe16a3b639d97989a88e4436d41a526f6db800d3be7e8c3e3efee386259` |
| Image format | `UDZO`, read-only zlib-compressed UDIF, not encrypted |
| Image logical size | 594,205,696 bytes |
| Image checksum | CRC32 `$6CB64F02`; HFS partition CRC32 `$BFF88C4B` |
| Volume | `ParakeetAI`, HFS+, UUID `1D6B71F1-8865-38E4-B47F-5881235445E5` |
| App | `/Volumes/ParakeetAI/ParakeetAI.app` |
| Version | `3.6.21` (`CFBundleVersion` and short version) |
| Bundle ID | `org.parakeetai.ParakeetAI` |
| Executable | `ParakeetAI`, universal `x86_64` + `arm64` |
| Minimum macOS | 12.0 |
| Build metadata | macOS 14.5 SDK; Xcode 15.4 |
| URL scheme | `parakeetai` |
| Main archive | `Contents/Resources/app.asar`, 100,668,529 bytes |
| Main archive raw SHA-256 | `e976a3905580addfee17c17ed7c202ccf7334f2d0cff19ef7188cc73b6a9926a` |

These values come from `stat`, `shasum`, `hdiutil imageinfo`, `diskutil info`, `file`, and `Info.plist`; normalized command results are in [identity.txt](evidence/identity.txt) and [hashes.txt](hashes.txt). `Info.plist` separately contains Electron's `ElectronAsarIntegrity` value `12c4ae...621a`. That field and the raw whole-file SHA-256 are recorded as distinct measurements; this audit does not assume they use identical byte coverage.

The ASAR parser enumerated 959 directories, 11,726 files, and no links. Thirty-two members were marked unpacked. All 11,694 packed members carrying ASAR SHA-256 records verified with zero mismatch. See [bundle-static.txt](evidence/bundle-static.txt) and the retained, non-executing [parser](evidence/asar_static_extract.py).

## Signing and platform security metadata

The app is signed by `Developer ID Application: Jure Sotosek (836KU58BNR)`, team `836KU58BNR`, with hardened-runtime flag, timestamp `Jul 11, 2026 at 11:22:33 AM`, and CDHash `61cbce835ec4d5186fe397e2e7839405652d7dd1`. Deep strict signature validation succeeded; Gatekeeper reported `Notarized Developer ID`; the stapled notarization ticket validated. These are authenticity/integrity observations, not an endorsement of application behavior. [identity.txt](evidence/identity.txt)

Declared entitlements are JIT, unsigned executable memory, disabled library validation, audio input/microphone, user-selected read/write files, and outbound network client. `Info.plist` declares microphone, screen capture, camera, and Bluetooth purposes. App Transport Security allows arbitrary loads and local networking; localhost and `127.0.0.1` receive temporary insecure-HTTP/TLS-1.0 exceptions. [identity.txt](evidence/identity.txt)

## Technology inventory

Observed technology is Electron 38.8.6 with a minified JavaScript main process, a context-bridge preload, and a React renderer. The root package manifest is `parakeetai-desktop` 3.6.21 and points to `dist/main/main.js`. Static modules identify TanStack Query, tRPC, Radix UI, Vercel AI SDK, Speechmatics' realtime client, Mixpanel, `electron-updater`, Electron Settings, and Electron loopback-audio support. No source map for the app-owned main, preload, microphone-worker, or renderer bundle was found; packaged dependencies do include their own maps. [bundle-static.txt](evidence/bundle-static.txt)

Bundled frameworks and helpers:

- Electron Framework 38.8.6, Mantle, ReactiveObjC, and Squirrel.
- Main, GPU, Plugin, and Renderer Electron helper applications, all covered by deep signature verification.
- Chrome crashpad, FFmpeg, EGL/GLES, and Vulkan SwiftShader components.
- A NAPI-RS/Rust native package for active-input-process detection and audio processing, with macOS and Windows binaries for x86-64 and ARM64.
- Squirrel/`electron-updater` using the GitHub repository `parakeetai/parakeetai-desktop-releases` and cache name `parakeetai-desktop-updater`.

The universal launcher links only Electron Framework and `libSystem` and exposes only the Mach-O header symbol through `nm -gU`. Each macOS N-API module links AudioUnit, AudioToolbox, CoreAudio, OpenAL, CoreMIDI, CoreFoundation, `libiconv`, and `libSystem`; each exports only `_napi_register_module_v1`, so internal Rust symbols are stripped or not exported. Their embedded minimum versions are macOS 10.12 (x86-64) and 11.0 (ARM64), below the enclosing app's macOS 12 minimum. Exact install names, signatures, and helper IDs are in [native-metadata.txt](evidence/native-metadata.txt).

No `.appex`, `.xpc`, embedded LaunchAgents directory, or launch-agent plist was found. Login-at-startup is instead requested dynamically through Electron's `app.setLoginItemSettings` (`app.asar::dist/main/main.js`, byte 401900; member hash in [bundle-static.txt](evidence/bundle-static.txt)).

## Native-module provenance boundary

The unpacked native package includes Rust source and four platform builds. Its manifest points to a `sonora` Git dependency pinned to revision `dec6a074725e82ef371a024fc45236d60f0acf52`. The macOS source statically shows CoreAudio enumeration of active input process IDs/bundle IDs and input device identifiers; the audio processor wraps WebRTC-style echo processing. [bundle-static.txt](evidence/bundle-static.txt)

Ownership and redistribution rights for this source, the Git dependency, and bundled third-party components were not established. Therefore none is approved for copying into Bluey. Its behavior may inform clean-room requirements only after legal, license, dependency, and provenance review.

## Completeness boundary

The renderer/main bundles are minified but readable; native binaries are signed and their symbols/resources were inspected statically. The unauthenticated run confirmed the initial login page, process split, context isolation, Node-global exclusion, generic preload surface, initial files, and blocked startup requests. Server implementations, authenticated routes, remote configuration, retention, authorization enforcement, payment behavior, and model-provider routing remain outside this evidence. Absence of a client string is reported as “not observed,” not proof of server-side absence. Remaining unknowns are enumerated in [SECURITY.md](SECURITY.md) and [DATA-AND-NETWORK.md](DATA-AND-NETWORK.md).
