# ROUND-465-COMPANION-IDENTITY-COLLISION-GUARD

Date: 2026-07-09
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Context

Pinky and older Bluey builds both used a generic background process identity named `Terminal` on macOS, with `Terminal.exe` on Windows. This round first made that legacy convention safe by proving binary ownership before launch. Round 466 then moved Bluey's preferred identity to `termb` / `termb.exe`, while keeping `Terminal` / `Terminal.exe` as a compatibility fallback.

The risk is that a shared global path or a polluted local install can make one product launch the other product's `Terminal` binary. On this Mac, the local Bluey install showed exactly that shape: Bluey's `~/.bluey/bin/Terminal` matched the Pinky binary hash and hung on a version probe, while `~/.bluey/bin/bluey-daemon` was the real Bluey daemon.

## Goal

Keep the existing convention safe during the transition:

- `bluey` as the user CLI.
- `termb` / `termb.exe` as the preferred background daemon identity.
- `Terminal` / `Terminal.exe` as a legacy fallback only.
- `hostovb` / `adriverb` as the preferred overlay/audio helper aliases, with `host-overlay` / `audio-driver` as legacy fallbacks.

Make both the preferred and legacy names safe even when another app also uses a similar helper name.

## Changes

- Bluey CLI daemon resolution now prefers the canonical installed executable root before the invoked path.
- `Terminal` / `termb` are no longer trusted just because the filename exists.
- Before Bluey launches a daemon candidate, it probes the candidate with a bounded `--version` check and only accepts output starting with `bluey-daemon `.
- Symlinked `Terminal` identity candidates are rejected outright on Unix/macOS.
- If `termb` or `Terminal` is missing, polluted, symlinked, or hangs, Bluey falls back to `bluey-daemon` / `cue-daemon`.
- macOS install scripts now replace the local `~/.bluey/bin/termb` and `~/.bluey/bin/Terminal` aliases from the Bluey daemon binary every install/update, instead of preserving an old or polluted alias.
- The install scripts no longer publish daemon helper aliases into shared PATH directories. Those identities stay inside Bluey's install root, while shared PATH continues to expose `bluey` and `bluey-daemon`.
- Windows installer parity now replaces `termb.exe` and `Terminal.exe` from `bluey-daemon.exe` / `cue-daemon.exe` during install.

## Why This Handles Repeated Collisions

The important invariant is not the name. The important invariant is ownership proof.

Even if another app creates a same-named helper binary, Bluey will only use the sibling helper if it proves it is a Bluey daemon. If the proof fails or hangs, Bluey ignores it and uses the canonical `bluey-daemon` fallback.

This keeps legacy installs working while removing the hidden dependency on a globally unique helper basename.

## Verification

- `cargo test -p cue-cli resolve_daemon_bin --quiet`
- `bash -n ops/install/install.sh`
- `bash -n scripts/install.sh`
- `git diff --check -- crates/cue-cli/src/app.rs ops/install/install.sh scripts/install.sh ops/install/install.ps1`

PowerShell parser validation was not run because `pwsh` is not installed on this Mac.

## Deployment

No deploy was performed in this round. Per owner instruction, deployment and GitHub Actions should only run when explicitly requested.
