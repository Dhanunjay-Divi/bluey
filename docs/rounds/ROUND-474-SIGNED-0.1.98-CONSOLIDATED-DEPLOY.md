# Round 474 - Signed 0.1.98 Consolidated Deploy

Date: 2026-07-10
Branch: `codex/bluey-web-ui-parallel-20260704`
Release version: `0.1.98`
Backup task id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Promote the reviewed work from Rounds 458 through 473 as one traceable, signed
Bluey desktop/web/API release without GitHub Actions.

## Release Invariants

- Private evaluation output and Python bytecode stay outside git/artifacts.
- Provider keys, the release private key, customer content, and raw private
  evaluation answers stay outside git and public artifacts.
- Production data and the current API binary are backed up before replacement.
- Capture-visible/dev-marker binaries are rejected.
- Windows is not claimed without a Windows/MSVC-built executable.
- A failed release artifact remains immutable and is superseded by a new version.

## Scope

- account connection, legal acceptance, trial, and signed-out behavior
- transcript final flush, deduplication, captions, and listen auto-stop
- humanized role-aware answers and 50-question routing regressions
- bounded provider fallback and detailed internal phase diagnostics
- durable session sync/audit records and stable ownership
- macOS/Windows process aliases and installer cleanup
- Auto/Quick/Thorough, context readiness, source chips, recovery, and workbench
  version preservation
- static web/account/session-history cleanup

## Pre-Deploy Verification

- release hygiene passed across 83 release-facing files
- scoped staged secret scan passed without printing candidate material
- root workspace library tests: 646 passed, 0 failed, 5 ignored
- root workspace all-target tests passed
- server library tests: 296 passed, 0 failed
- server integration tests: 54 passed, 0 failed
- strict Clippy passed for both Rust workspaces
- formatting, JavaScript syntax, Swift typecheck, Windows C syntax, touched shell
  syntax, and staged whitespace checks passed

The integration gate caught and fixed a real sync defect: `/sync/batch` accepted
a safe legacy session ID while lookup required a UUID. Validation now accepts
bounded alphanumeric/hyphen/underscore IDs and rejects traversal, slash,
whitespace, and control-character shapes.

## 0.1.97 Gate Rejection

Commit `ba90b46f904abb0bb02771b51e9e9acb2720a66c` built and published a signed
`0.1.97` macOS archive. The live verifier rejected it because the Makefile
omitted `termb`, `hostovb`, and `adriverb`, although runtime and installers
already required those identities. The signed `latest.json` was immediately
rolled back to verified `0.1.96`. The `0.1.97` archive was not modified.

The corrective `0.1.98` package adds all identity copies on macOS and equivalent
Windows package aliases. Final source commit, artifact SHA256, backup paths, API
binary SHA256, health identity, MIME/signature checks, service state, and log
scan are recorded after the corrected promotion.

## Windows Installer Gate

A real Windows install smoke exposed another release blocker while `latest`
still pointed at `0.1.96`: `install.ps1` invented a Windows ZIP URL when the
signed manifest omitted `windows-x86_64`, then surfaced a raw HTTP 404. The
installer now stops before download with a clear no-files-changed message when
the platform is absent.

The manual Windows builder now produces the required
`dist/bluey-<version>-windows-x86_64.zip` with a `bin\...` layout in addition to
the loose developer folder. It includes daemon/overlay/audio identity aliases
and the Windows whisper helper. The future workflow package has the same alias
contract, but no GitHub workflow is used in this release.

The first clean MSVC gate also caught an unconditional import of the Unix-only
daemon executable-name helper in `cue-cli`. The import is now platform-scoped;
native and cross-target Windows Clippy both pass before the MSVC rebuild.

## Deployment Status

Signed promotion completed without GitHub Actions.

### Source And Artifacts

- final source commit:
  `54d4cee28faad1b9f4ecc34191e390df52969803`
- macOS arm64 artifact:
  `releases/v0.1.98/bluey-0.1.98-darwin-arm64.tar.gz`
- macOS arm64 bytes: `19969543`
- macOS arm64 SHA256:
  `d87338fedd51c2171cd1d7c93567b20bb1c6273d78fe09ea3e515d08744930f9`
- Windows x86_64 artifact:
  `releases/v0.1.98/bluey-0.1.98-windows-x86_64.zip`
- Windows x86_64 bytes: `28926550`
- Windows x86_64 SHA256:
  `19b9137eef41c32f571bda37b14773c387c58daa2291c8c3944766eb2fa56682`

The public signed manifest now advertises both platforms. The macOS archive
contains `bluey-daemon`, `termb`, `Terminal`, `hostovb`, `host-overlay`,
`adriverb`, and `audio-driver`. The Windows ZIP contains the equivalent `.exe`
identities plus `cue-whisper.exe`; every identity alias is byte-identical to its
canonical executable.

### Windows Proof

- clean MSVC strict Clippy passed on the real Windows 11 builder
- optimized MSVC build and all native helper builds passed
- the public `irm https://bluey.sh/install.ps1 | iex` flow was run on Windows
  into an isolated install root
- the installer downloaded `28,926,550` bytes and verified the signed-manifest
  SHA before extraction
- all 11 required installed executables were present
- installed CLI reported `bluey 0.1.98`
- installed daemon reported `bluey-daemon 0.1.98`
- daemon, overlay, and audio identity hashes matched their canonical binaries
- the temporary install root and temporary user-PATH entry were removed after
  the smoke test

This directly closes the raw HTTP 404 shown in `IMG_3783.HEIC`.

### API Safety And Promotion

- fresh pre-deploy PostgreSQL backup:
  `/var/backups/bluey-api/hourly/bluey-postgres-20260711T061843Z.pgdump`
- backup bytes: `14400009`
- backup checksum: verified
- previous API binary:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260711T062044Z`
- previous API binary SHA256:
  `68092366f5cead91e79c7c5dfdcde2e82937f207a96d4f26256ac0b5cf94df57`
- promoted API binary SHA256:
  `e8b87b2ffd72fd51bdbd495075a13b3c1331f69b1494f8027a5d2bab6a3daa88`
- public `/health` reports commit:
  `54d4cee28faad1b9f4ecc34191e390df52969803`
- `bluey-api.service`: active, running, `NRestarts=0`
- post-restart journal: clean startup and health traffic, with no warning/error
  loop

### Public Verification

- `latest.json` signature verified after promotion
- live version is `0.1.98`
- macOS and Windows public artifact SHA256 values match the signed manifest
- `install.sh` is served as `application/x-shellscript`
- `install.ps1` is served as `application/x-powershell`
- the Windows release is served as `application/zip`
- the macOS archive unpacks with all aliases and both main binaries report
  `0.1.98`
- `/`, `/download`, `/pricing`, `/login`, `/account`, `/privacy`, `/terms`,
  `/llms.txt`, `/sitemap.xml`, and `/robots.txt` return HTTP 200
- live desktop and 390px mobile visual checks passed without horizontal overflow
- production disk is 36% used with 37 GB free after the deploy

`0.1.97` remains an immutable rejected audit artifact. It was superseded by
`0.1.98`; it was never left as the signed live `latest` release after the gate
failure.
