# Littlebird evidence index

This directory contains analyst-authored, text-only records of a read-only inspection of:

`/Users/uno/Downloads/dmg_backtrack_code/Littlebird-Mac-arm64-0.81.11-Installer.dmg`

Each record has a stable `E-LB-*` identifier. The audit documents cite those identifiers rather than relying on unreferenced conclusions. Static command outputs were collected without executing application code. A later, separately authorized bounded runtime pass executed only the mounted app entrypoint under isolated state and denied external egress; it is recorded in `runtime.txt`. Sensitive client tokens and SDK values discovered in signed resources are deliberately omitted.

Bluey comparison target: repository `/Users/uno/Downloads/cue-bluey-jobs` at commit `eb923a89f24b69664c73479607bf665725789658`, revalidated during final coordination.

## Evidence records

| File | Scope |
| --- | --- |
| `identity-and-signing.txt` | DMG identity, mount state, bundle metadata, signatures, entitlements, architectures, frameworks, and helper inventory |
| `static-architecture.txt` | Electron/asar layout, process boundaries, IPC, source maps, native helpers, persistence, update flow, and recovery behavior |
| `static-feature-findings.txt` | Onboarding, authentication, assistant/context, meetings, integrations, billing, deletion, and negative job-workflow searches |
| `security-findings.txt` | Trust-boundary and sensitive-data findings, including the OTP telemetry path and redacted embedded client configuration |
| `bluey-references.txt` | Exact Bluey source locations used by `BLUEY-GAP-MAP.md` |
| `runtime.txt` | Completed bounded, isolated, unauthenticated runtime observation and cleanup proof |

## Method and provenance

- The image was attached with `hdiutil attach -readonly -nobrowse` and confirmed read-only with `diskutil info` (`E-LB-ID-002`).
- Signing and notarization were checked with `codesign`, `spctl`, and `xcrun stapler` (`E-LB-ID-005`).
- Mach-O metadata was inspected with `file`, `otool`, `nm`, and `strings`; native binaries were never executed during the static phase (`E-LB-ID-006`, `E-LB-ARCH-008`).
- `app.asar` was listed and selected files extracted with the repository's already-installed `@electron/asar` package. No package lifecycle hook or bundled script was invoked (`E-LB-ARCH-001`).
- SQLite inspection used `sqlite3 -readonly` (`E-LB-ARCH-010`).
- Renderer source maps shipped inside the signed asar were used only to identify original file paths and substantiate control flow. No Littlebird code was copied into Bluey (`E-LB-ARCH-003`).
- After explicit authorization, a bounded unauthenticated runtime pass used a disposable home/profile and denied external egress; no credentials, permissions, helpers, or user data were used, and all audit-owned runtime state was removed (`E-LB-RUN-001` through `E-LB-RUN-007`).
- Static absence means “not found in the inspected signed resources,” not proof that a server-controlled or runtime-fetched feature can never exist.

## Handling restrictions

The extracted working set remained in `/tmp/littlebird-static` during analysis. It is not included in this repository because it contains proprietary application code and embedded client configuration. Hashes are preserved in `../hashes.txt` so the evidence can be reproduced from the owner-supplied DMG.
