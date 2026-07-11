# Bluey Release v0.1.98

Date: 2026-07-10
Source branch: `codex/bluey-web-ui-parallel-20260704`
Source commit: `54d4cee28faad1b9f4ecc34191e390df52969803`

## Summary

`0.1.98` is the promoted consolidated reliability and product-experience
release for Rounds 458 through 474. It contains the complete `0.1.97` feature
set plus the packaging correction required by the signed identity gate.

## User Experience

- `Auto` remains the default with optional `Quick` and `Thorough` overrides.
- Provider/model/lane names, raw confidence, and technical provider errors stay
  out of the normal product surface.
- Partial answers remain visible and offer Continue; pre-output failures offer
  Retry.
- Files and screen context show Reading, Ready, or Needs-attention state, then
  move behind the context control after submission.
- Research answers expose source chips and workbench follow-ups preserve prior
  complete code/design versions.
- Answers use a more direct, role-adaptive, first-person interview voice.

## Reliability And Safety

- Transcript sends wait for provider settling and do not replay consumed speech.
- Listen has idle countdown and automatic stop billing protection.
- Signed-out/deleted accounts stop capture and paid answer work.
- Provider fallback has bounded connect/first-output waits and durable phase
  diagnostics.
- Session questions, answers, transcripts, context, artifacts, failures, and UI
  events persist with stable IDs for idempotent sync and audit.
- Trial/legal acceptance, device linking, STT settlement, and ownership paths
  have regression coverage.
- macOS and Windows use install-root-scoped process aliases.

## Packaging Correction

The macOS archive now contains all canonical and compatibility identities:
`bluey-daemon`, `termb`, `Terminal`, `hostovb`, `host-overlay`, `adriverb`, and
`audio-driver`. Windows packaging mirrors the same contract with `.exe`
identities. This closes the gate that rejected `0.1.97`.

## Supported Platforms

| Platform | Release status |
| --- | --- |
| macOS arm64 | Signed artifact in this release |
| macOS Intel/universal | Source/build parity retained; publish only after the same gate passes |
| Windows x86_64 | Signed artifact built and install-smoked on Windows 11 |

## Published Artifacts

| Platform | Bytes | SHA256 |
| --- | ---: | --- |
| macOS arm64 | 19969543 | `d87338fedd51c2171cd1d7c93567b20bb1c6273d78fe09ea3e515d08744930f9` |
| Windows x86_64 | 28926550 | `19b9137eef41c32f571bda37b14773c387c58daa2291c8c3944766eb2fa56682` |

The final backup paths, API binary hashes, health identity, rollback point,
Windows install proof, and signed live verification are recorded in
`ROUND-474-SIGNED-0.1.98-CONSOLIDATED-DEPLOY.md`.
