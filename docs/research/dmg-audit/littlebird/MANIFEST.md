# Littlebird 0.81.11 arm64 manifest

This audit covers the owner-supplied `Littlebird-Mac-arm64-0.81.11-Installer.dmg`. Static inspection was read-only; no package scripts or bundled application code were executed. Sensitive embedded client configuration is redacted. See [the evidence index](evidence/README.md) and [the complete hash list](hashes.txt).

## Identity

| Field | Observed value | Provenance |
| --- | --- | --- |
| DMG filename | `Littlebird-Mac-arm64-0.81.11-Installer.dmg` | E-LB-ID-001 |
| DMG size | 336,925,123 bytes | E-LB-ID-001 |
| DMG SHA-256 | `e6e247957338518d9888c8a16147cc66648ae91d4ef347426146233de066234b` | E-LB-ID-001 |
| Image format | UDIF read-only zlib-compressed (`UDZO`), unencrypted | E-LB-ID-001 |
| Mounted volume | `Littlebird 0.81.11-arm64` | E-LB-ID-002 |
| App | Littlebird 0.81.11 | E-LB-ID-004 |
| Bundle ID | `com.genos.littlebird` | E-LB-ID-004 |
| Minimum macOS | 13.0.0 | E-LB-ID-004 |
| Architecture | arm64, native execution required | E-LB-ID-004 |
| Technology | Electron 39.8.7 with Swift/native helper apps | E-LB-ID-004, E-LB-ID-007, E-LB-ID-008 |
| URL scheme | `little-bird` | E-LB-ID-004 |

## Integrity and trust metadata

The app is signed by `Developer ID Application: Little Bird Software, LLC (ML63743965)`, uses hardened runtime, passes deep strict code-signature verification, is accepted by Gatekeeper as a notarized Developer ID app, and has a valid stapled notarization ticket. The signature timestamp is 2026-07-08 12:36:45. These checks establish package integrity and Apple notarization status, not product security. Evidence: E-LB-ID-005.

Electron asar integrity is present. Its recorded raw-header SHA-256, `7ec60b0c34bf06ecc6a8d89031bdb8e88bc5b5a26798c955829c2900d98e236c`, matched an independent recomputation. The entire `app.asar` SHA-256 is `6f8d78b53c5b343236a3ae9718d8d98207575446cc17adaa540f94c05795605c`. Evidence: E-LB-ARCH-001 and `hashes.txt`.

## Signed bundle inventory

| Component | Identity / role | Notes | Provenance |
| --- | --- | --- | --- |
| `Littlebird.app` | Electron main application | Main bundle, arm64 | E-LB-ID-004 |
| Electron Framework | 39.8.7 | Chromium/Node application runtime | E-LB-ID-007 |
| Four Electron helpers | main, GPU, Plugin, Renderer | Version 0.81.11 | E-LB-ID-007 |
| `ContextKitCore.app` | `com.genos.contextkit-cli` | LSUIElement Swift context-observation daemon | E-LB-ID-008 |
| `LittlebirdAudioTranscription.app` | `com.genos.littlebird-audio-transcription` | LSUIElement audio/transcription helper | E-LB-ID-008 |
| Mantle / ReactiveObjC / Squirrel | 1.0 / 3.1.0 / 1.0 | Electron macOS/update support | E-LB-ID-007 |
| Category seed SQLite | Six categories, 76,161 domains | Static exclusion/category data, not user data | E-LB-ARCH-010 |
| `app.asar.unpacked` native modules | window events, permissions, sharp/libvips | Hashes preserved | `hashes.txt` |
| Bundled utilities | `rg`, `sentry-cli` | Not executed | `hashes.txt` |

No LaunchAgent, LaunchDaemon, app/system extension, or bundled XPC service was found. Evidence: E-LB-ID-007.

## Permissions and entitlements

The main app and helpers request a powerful capability set: Apple Events automation, audio/microphone/camera, calendar-related access, client networking, JIT, unsigned executable memory, and disabled library validation. The main app is not App Sandbox-entitled. Its usage descriptions include browser tab URL inspection, cross-app audio, calendar, contacts, microphone, speech, and reminders. ATS permits arbitrary loads and local networking, including insecure localhost exceptions. Evidence: E-LB-ID-006.

## Inspection state

- Static bundle, signing, source-map, database, binary metadata, linked-library, symbol/string, feature, network, and Bluey comparison passes: complete.
- Bounded runtime: complete for unauthenticated main-process initialization and isolated state/process behavior. The outer no-network/write sandbox prevented Chromium renderer startup, so no UI behavior was validated (E-LB-RUN-001 through E-LB-RUN-007).
- Network behavior: endpoints and request shapes are static observations only; no client token or authenticated API was exercised.
- Proprietary source: selected files were extracted to `/tmp` for analysis and were not copied into Bluey.
