# Bluey Release v0.1.97

Date: 2026-07-10
Source branch: `codex/bluey-web-ui-parallel-20260704`
Source commit: pending release commit

## Summary

`0.1.97` is the consolidated reliability and product-experience release for
Rounds 458 through 473. It improves account linking and legal acceptance,
live-transcript finalization, listen billing guards, answer latency and
fallback, human interview voice, session durability, process identity safety,
and the normal Auto/Quick/Thorough overlay experience.

## User Experience

- Provider names, model names, internal lanes, raw confidence, and provider
  errors stay out of the normal product surface.
- `Auto` remains the default, with optional `Quick` and `Thorough` overrides.
- Recoverable partial answers remain visible and offer Continue; failures before
  useful output offer Retry.
- Screen and file context show Reading, Ready, or Needs-attention state and move
  behind the context control after submission.
- Research answers expose source chips without exposing retrieval internals.
- macOS workbench follow-ups preserve complete earlier code/design versions.
- Answers use a more direct, role-adaptive, first-person interview voice.

## Reliability And Safety

- Transcript sends wait for final provider settling and avoid replaying already
  consumed speech.
- Listen has an idle countdown and automatic stop guard to limit accidental
  billing.
- Signed-out/deleted accounts stop paid capture and answer work.
- Provider fallback has bounded connect/first-output waits and durable phase
  diagnostics.
- Session questions, answers, transcripts, context, artifacts, failures, and UI
  events persist with stable IDs for idempotent sync and audit.
- Trial/legal acceptance, device linking, STT reservation settlement, and
  account ownership paths received regression coverage.
- Bluey-owned process aliases are install-root scoped on macOS and Windows to
  avoid collisions with unrelated system applications.

## Supported Platforms

| Platform | Release status |
| --- | --- |
| macOS arm64 | Signed artifact planned in this release |
| macOS Intel/universal | Source/build parity retained; not promised unless the universal artifact passes the same release gate |
| Windows | Source, installer, overlay, audio-helper, and alias parity included; executable artifact requires the Windows/MSVC release builder |

## Verification

Pre-release checks include full daemon/server tests, strict formatting and
linting, Swift typecheck/build, Windows C syntax, JavaScript syntax, shell
syntax, release hygiene/secret scanning, package integrity, and live signed
manifest verification. Final artifact hashes and production results are
recorded in `ROUND-474-SIGNED-0.1.97-CONSOLIDATED-DEPLOY.md` after promotion.

## Known Gates

- Windows still needs the complete multi-version workbench available on macOS.
- Real-device Windows audio/overlay smoke remains required before inviting
  Windows paid users.
- Search progress can be moved to an earlier pre-retrieval stream phase in a
  future transport improvement.
