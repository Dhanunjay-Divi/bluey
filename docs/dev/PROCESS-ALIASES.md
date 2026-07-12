# Bluey Process Aliases

Bluey keeps user-facing commands obvious, but background helper names are stable
Bluey-owned aliases so they do not collide with Pinky, Terminal.app, or generic
OS tools.

## Current Names

- `bluey`: user CLI.
- `bluey-daemon`: canonical daemon binary and compatibility fallback.
- `termb` / `termb.exe`: background daemon identity launched by `bluey on`.
- `hostovb` / `hostovb.exe`: native overlay helper identity.
- `adriverb` / `adriverb.exe`: native audio helper identity.

## Source Of Truth

Rust runtime code imports executable names from
`crates/cue-core/src/process_aliases.rs`. Add or reorder a runtime alias there
first, then mirror the same transition in macOS/Windows build and install
scripts. Audio-helper discovery is owned by
`crates/cue-daemon/src/audio/system_capture.rs`; permission checks and capture
must use that same resolver so they cannot select different binaries.

## Compatibility Fallbacks

Install, update, doctor, and uninstall still recognize older helper names:

- `Terminal` / `Terminal.exe`
- `host-overlay` / `host-overlay.exe`
- `audio-driver` / `audio-driver.exe`
- old `bluey-*` and `cue-*` helper binaries

These names are compatibility fallbacks only. New code should prefer the current
Bluey-owned names above.

## Build And Release Outputs

Every installer or release bundle should include both the current aliases and
the compatibility fallbacks during the transition:

- daemon: `bluey-daemon`, `termb`, `Terminal`
- overlay: `bluey-overlay-macos` / `bluey-overlay.exe`, `hostovb`, `host-overlay`
- audio: `bluey-audio-macos` / `bluey-audio.exe`, `adriverb`, `audio-driver`

Do not copy Pinky icon or image assets into Bluey. Bluey replicates Pinky's
process-identity mechanism, not Pinky's branding. Helper bundle metadata and
icons should remain Bluey-owned or generic.

## Rules

- Do not introduce random executable names. Put random or short hashes in logs,
  sockets, and support refs instead.
- Do not publish `termb` or `Terminal` into shared PATH directories. Keep daemon
  identity helpers inside the Bluey install root.
- Before launching a daemon identity candidate, verify it is a Bluey daemon with
  a bounded `--version` probe.
- Ignore symlinked daemon identity candidates on Unix/macOS.
- Cleanup must be install-root filtered so Bluey never stops another app's
  helper with the same basename.

## Why

The previous generic `Terminal` convention was easy to collide with another
local tool. Stable Bluey-owned aliases keep process cleanup and support reliable
without making the process table noisy with obvious product helper names.
