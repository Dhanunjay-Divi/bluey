# LockedIn static evidence index

Audit captured: `2026-07-12T08:44:31Z`

Source image: `/Users/uno/Downloads/dmg_backtrack_code/LockedIn-1.7.5-universal.dmg`

This directory contains text-only records derived first from read-only static inspection and then from one separately authorized, isolated, unauthenticated runtime launch. During the static phase, no executable was launched. During the runtime phase, only the mounted app entrypoint and its ordinary Electron children ran with a disposable HOME/profile and blocked external egress. No app was installed, no credential/account was used, no permission was granted, and no bundled script, package lifecycle hook, helper/remote-control action, update download, or update install was invoked. The app's automatic update check did run but failed at the closed proxy before a remote response. No binary or extracted application code is copied here.

Evidence labels used throughout the audit:

- **Observed**: established directly by a command result or a named static bundle resource.
- **Inferred**: a likely behavior derived from observed wiring, but not validated at runtime.
- **Unknown**: requires runtime or server-side evidence.

Files:

- `mount-and-image.txt`: source-image integrity, format, and read-only mount proof.
- `bundle-identity.txt`: app identity, signing, notarization, entitlements, frameworks, and helpers.
- `asar-inventory.txt`: ASAR inventory, selected-entry hashes, and integrity verification.
- `native-components.txt`: architecture and purpose indicators for native components.
- `electron-main-observations.md`: line-addressable main-process observations.
- `preload-ipc-observations.md`: exposed renderer-to-main boundary.
- `renderer-features.md`: static UI, workflow, feature, and remote-control wiring.
- `network-inventory.txt`: origins, endpoint paths, and request-shape observations with credential values omitted.
- `storage-inventory.md`: local and cloud persistence references.
- `bluey-code-map.md`: exact Bluey comparison locations.
- `limitations.md`: static-analysis boundaries and runtime unknowns.
- `runtime-unauthenticated.md`: exact approved launch command, initial route, process tree, blocked network/update observations, complete application-profile file inventory, and cleanup proof.

The `.env.local` resource was treated as sensitive. Only variable names are recorded; values were neither copied into this repository nor included in command output captured here.
