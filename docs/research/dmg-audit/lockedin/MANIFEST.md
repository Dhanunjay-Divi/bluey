# LockedIn 1.7.5 DMG manifest

## Scope and result

`LockedIn-1.7.5-universal.dmg` was inspected statically from a read-only mount, followed by one coordinator-approved unauthenticated launch from the same read-only image with a disposable HOME/profile and blocked external egress. The evidence supports a universal Electron/React live interview and meeting copilot with Firebase/Clerk authentication, custom native audio capture, screen capture, document context, Duo helper/WebRTC wiring, billing, telemetry, and S3 auto-updates. It does **not** support a conclusion that LockedIn implements Bluey Jobs-style discovery, ATS application automation, durable application queueing, or submission receipts. See the [feature evidence](evidence/renderer-features.md), [runtime evidence](evidence/runtime-unauthenticated.md), and [limitations](evidence/limitations.md).

## Image identity and integrity

| Field | Observed value |
|---|---|
| Source | `/Users/uno/Downloads/dmg_backtrack_code/LockedIn-1.7.5-universal.dmg` |
| Size | `302,369,574` bytes |
| SHA-256 | `a06fab03e233726569e1bc9569cf43f374a9ff7c27bae9ff55dacd89dd1b7501` |
| Image format | UDZO, zlib-compressed, checksummed, unencrypted GUID/APFS |
| Volume | `LockedIn 1.7.5-universal` |
| Mount | APFS, read-only, `nodev`, `nosuid`, `quarantine`, `nobrowse` |

Command provenance is in [mount-and-image.txt](evidence/mount-and-image.txt); file digests are in [hashes.txt](hashes.txt).

## Application identity

| Field | Observed value |
|---|---|
| App | `LockedIn.app` |
| Version / bundle version | `1.7.5` / `1.7.5` |
| Bundle ID | `com.lockedindesktopapp` |
| Minimum macOS | `11.0` |
| Architecture | Universal `arm64` and `x86_64` |
| UI mode | `LSUIElement=true` background/menu-style app |
| URL scheme | `locked-in:` |
| Technology | Electron `33.4.11`, Chromium `130.0.6723.191`, React `18` |
| Main entrypoint | `app.asar:public/electron.js` |
| Main renderer | `app.asar:build/static/js/main.8ad05cf1.js` |
| App size | `913,976` KiB on the mounted image |

The bundle identity, Info.plist fields, and runtime version are recorded in [bundle-identity.txt](evidence/bundle-identity.txt). Package and ASAR provenance are in [asar-inventory.txt](evidence/asar-inventory.txt).

## Trust and signing

- `codesign --verify --deep --strict` succeeded and the designated requirement was satisfied.
- Gatekeeper accepted the app as `Notarized Developer ID`; the stapled ticket validated.
- Signing identity: `Developer ID Application: Cyber Gravity LLC (F5JH5QMHRD)`, Team ID `F5JH5QMHRD`, timestamp July 9, 2026.
- Hardened runtime is enabled. Electron-related JIT, unsigned executable memory, disabled library validation, Apple Events, screen recording, microphone/audio/camera, and DYLD environment entitlements are present. App Sandbox is not.

These are direct command observations, not an assurance about application-level authorization. Full details are in [bundle-identity.txt](evidence/bundle-identity.txt).

## Bundle contents

Observed frameworks/helpers include Electron Framework, Squirrel, ReactiveObjC, Mantle, four Electron helper apps, Crashpad, and Squirrel ShipIt. Native components include per-architecture audio-capture addons/dylibs, universal robotjs/fsevents addons, and an x86_64-only global-key helper. No LaunchAgent, XPC service, app extension, or system extension was found. Runtime loaded/found only the arm64 audio addon; it did not start capture, robotjs, the key helper, or remote control. See [native-components.txt](evidence/native-components.txt).

The ASAR contains 49,270 entries. Every packed entry matched its recorded integrity. Forty-one unpacked, signed Mach-O files differed from pre-sign ASAR entry digests while the final bundle's strict deep signature verified; this is consistent with post-package signing and is documented as a build-pipeline artifact, not treated as tampering. See [asar-inventory.txt](evidence/asar-inventory.txt).

## Update system

`electron-updater` uses an S3 `latest` channel in bucket `desktop-app-updates-lockedin-ai`, region `ap-southeast-2`, path `updates/`. Static main-process settings allow prerelease and downgrade updates, auto-download updates, and install on quit. Runtime confirmed an automatic check was initiated; the closed proxy blocked it before any response, download, or install. Evidence: [bundle-identity.txt](evidence/bundle-identity.txt), `app.asar:public/electron.js:2218-2463` summarized in [electron-main-observations.md](evidence/electron-main-observations.md), and [runtime-unauthenticated.md](evidence/runtime-unauthenticated.md).

## Approved runtime checkpoint

The isolated launch reached local route `#/sign-in` with title `LockedIn AI`. The process tree contained main, crashpad, GPU, network-service, and renderer processes; the renderer was actually launched with `--enable-sandbox`, resolving the static BrowserWindow uncertainty. Firestore entered offline mode, model tiers fell back to static configuration, and the app attempted an automatic update check despite `ELECTRON_NO_UPDATER=1`; the closed proxy caused `ERR_PROXY_CONNECTION_FAILED`, so no remote connection, response, download, or install occurred. The full command, process tree, 51-file profile inventory, and cleanup proof are in [runtime-unauthenticated.md](evidence/runtime-unauthenticated.md).

## Audit method

1. Hashed the source image before inspection.
2. Mounted with `hdiutil attach -readonly -nobrowse` and verified the mount flags.
3. Inspected plist, signature, notarization, entitlements, architectures, linked libraries, package metadata, ASAR index, readable JavaScript, strings, and configurations without execution during the static phase.
4. Extracted only selected first-party text resources to a private temporary directory and verified each against ASAR integrity metadata.
5. Excluded `.env.local` values and all credential-like query data from evidence. Only environment-variable names are listed.
6. Compared static facts to exact Bluey repository locations recorded in [bluey-code-map.md](evidence/bluey-code-map.md).
7. After explicit approval, reattached the exact image read-only, launched only the mounted app entrypoint under an empty environment with isolated HOME/user-data and blocked external egress, made no UI interaction, recorded the initial route/process/socket/profile state, then terminated, attempted scoped LaunchServices unregister, erased the disposable root, and detached. LaunchServices retained stale unmounted path records; no global cleanup was used because it could affect unrelated app state.

Bluey comparison line references were last revalidated at repository HEAD `eb923a89f24b69664c73479607bf665725789658`.

The unresolved runtime and backend questions are enumerated in [limitations.md](evidence/limitations.md).
