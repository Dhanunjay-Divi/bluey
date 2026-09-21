# Bluey

> **Codex:** Start every Bluey task by loading the branch-local
> [$bluey-ops](.agents/skills/bluey-ops/SKILL.md), then follow the
> [agent round checklist](docs/work/BLUEY-AGENT-ROUND-CHECKLIST.md) and verify
> guidance against the current repository state.

Bluey is a consent-first live-context assistant for engineering meetings and
technical work. Its native desktop overlay can answer from the conversation,
screen context, files, code, and saved project context that the user chooses.

Bluey is not positioned as covert, undetectable, or a way to bypass workplace,
assessment, security, or consent rules.

## Current Release

This source tree targets Bluey `0.1.104`. The signed live manifest at
`https://bluey.sh/latest.json` is the authority for the version and artifacts
that customers can download; source, tags, or locally built packages do not
expand that public support promise.

| Platform | Current public artifact |
| --- | --- |
| macOS | Apple silicon, Intel, and universal archives |
| Windows | x86-64; install-smoked on Windows 11 |
| Linux | Not listed in the current manifest |

The installers verify the signed release manifest and checksum-pinned artifact
before installing. This is a distribution-integrity statement, not a claim
that every binary has platform code-signing or notarization.

## Product Contract

- Listening, screen analysis, file attachment, and answering start from visible
  user controls.
- Cloud session sync is off by default on new installs and remains separate
  from sign-in. It can be changed in desktop Settings.
- Raw audio is processed transiently for transcription. Bluey does not keep a
  raw-audio library in the current release; transcript text may be saved.
- Submitted prompts, transcripts, audio, files, screenshots, and answers are
  not used to train or fine-tune models.
- Auto Reload is off until the account owner deliberately enables it and
  approves a saved card, threshold, and amount.
- Supported desktop paths request capture exclusion for the overlay. That is a
  best-effort screen-share privacy control, not an invisibility guarantee or a
  security boundary.

See `https://bluey.sh/privacy`, `https://bluey.sh/terms`, and
`https://bluey.sh/docs/disguise` for the public contract.

## Install And Run

macOS on Apple silicon or Intel:

```bash
curl -fsSL https://bluey.sh/install.sh | bash
bluey on
```

Windows x86-64 in PowerShell:

```powershell
irm https://bluey.sh/install.ps1 | iex
bluey on
```

`bluey on` starts the desktop overlay, checks for a newer signed release, and
opens browser sign-in only when managed cloud work needs an account. Use the
overlay for normal listening, context selection, asking, session history, and
settings. Stop the product completely with:

```bash
bluey off
```

The current visible `bluey help` surface includes lifecycle and maintenance
commands such as `on`, `off`, `status`, `update`, `usage`, `portal`, and
`support`. Developer and diagnostic commands remain hidden from normal help.

For support, run `bluey support`; it creates a redacted bundle intended for
`hello@bluey.sh`.

## Desktop Experience

Bluey starts as a compact pill and expands into the real product surface:

- a chronological conversation and answer feed;
- visible Mic and System transcript sources;
- attached file, screen, page, and saved-context indicators;
- `Auto`, `Quick`, and `Thorough` answer modes;
- compact answers with a larger canvas for code or detailed work;
- visible listening, screen-context, and attachment controls;
- account balance and session controls.

On macOS the global shortcut family uses `Ctrl+Option`; on Windows it uses
`Ctrl+Alt`. The overlay's keyboard guide is the source of truth for shortcuts,
so first-run setup does not require memorizing commands or key combinations.

## Development

Build the Rust workspace and the macOS native helpers:

```bash
bash native/macos/cue-overlay/build.sh
cargo build
```

Build release packages with the repository scripts:

```bash
scripts/build-macos.sh
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
```

Run the isolated smoke test:

```bash
scripts/smoke-test.sh
```

For a local debug session:

```bash
./target/debug/bluey on --title "Design review"
./target/debug/bluey off
```

The daemon listens on `127.0.0.1:57321` by default. Development overrides
include `BLUEY_DAEMON_ADDR`, `BLUEY_DAEMON_BIN`, `BLUEY_OVERLAY_BIN`,
`BLUEY_DATA_DIR`, `BLUEY_CONFIG_DIR`, and `BLUEY_RUNTIME_DIR`. Older `CUE_*`
names remain compatibility aliases.

## Repository Shape

- `crates/cue-cli`: customer lifecycle command and support tooling.
- `crates/cue-daemon`: local session, audio, context, answer, and sync runtime.
- `crates/cue-core`: shared protocol, cards, settings, paths, and state.
- `native/macos`: AppKit overlay and native helper sources.
- `native/windows`: Win32 overlay and helper sources.
- `server`: managed auth, routing, sync, billing, and account APIs.
- `web`: public site, account dashboard, legal pages, and downloads.

Useful product references:

- `docs/FEATURE-MAP.md`
- `docs/SESSION-FLOW.md`
- `docs/SETTINGS-UI-CONTRACT.md`
- `docs/PRODUCT-STRATEGY.md`
- `docs/SECURITY-HARDENING.md`
- `docs/RELEASE-RUNBOOK.md`
