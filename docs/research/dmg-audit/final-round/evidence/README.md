# Evidence index

This directory contains normalized, safe text evidence only. It deliberately contains no DMG bytes, application binaries, ASAR contents, source extracts, credentials, cookies, profiles, user data, public analytics ingestion values, or runtime cache payloads.

Evidence was collected on 2026-07-12 from the owner-supplied DMG at `/Users/uno/Downloads/dmg_backtrack_code/final-round-desktop-2.4.0-arm64-mac.dmg`. Static inspection used a read-only `hdiutil` mount and trusted local analysis tools. The ASAR was expanded only into a disposable `/private/tmp` directory and deleted after analysis. Bundled application scripts and package lifecycle hooks were never executed.

Source references use these stable logical paths:

- `Info.plist`: `Final Round.app/Contents/Info.plist`
- `main`: `Final Round.app/Contents/Resources/app.asar/out/main/index.mjs`
- `preload`: `Final Round.app/Contents/Resources/app.asar/out/preload/main-window/index.cjs`
- `renderer`: `Final Round.app/Contents/Resources/app.asar/out/renderer/assets/index-xYTSJeR5.js`
- `package`: `Final Round.app/Contents/Resources/app.asar/package.json`

Files:

- `identity.txt`: image, bundle, signature, notarization, entitlements, architectures, helpers, and update metadata.
- `architecture-static.txt`: process, renderer, IPC, storage, audio, lifecycle, and technology observations.
- `features-static.txt`: first-party UI, route, service, and API capability inventory.
- `network-static.txt`: endpoints, request paths, socket behavior, telemetry, device headers, and unknown server behavior.
- `security-static.txt`: trust-positive controls and evidence-backed security/privacy risks.
- `runtime-unauthenticated.txt`: the two bounded, network-denied launch attempts and cleanup result.
- `bluey-code-map.txt`: Bluey comparison anchors revalidated at repository commit `eb923a89f24b69664c73479607bf665725789658`.

Classification vocabulary:

- `OBSERVED`: directly supported by a command result or first-party bundle/repository resource.
- `INFERENCE`: reasoned from observed implementation, not directly exercised.
- `UNKNOWN`: inaccessible server behavior or behavior requiring authenticated/permissioned runtime validation.
