# Bluey Release v0.1.101

Released: 2026-07-16
Audience: known production-beta users

## Summary

`0.1.101` completes the post-Round 519 account, session, streaming, and
mainline-reconciliation work. It prevents stale credentials and cross-account
session state from surviving account changes, preserves safe partial answers
when upstream streaming fails, and keeps signed-out recovery controls usable
without weakening signed-in window dragging.

## What Changed

- Credential writes and refreshes use generation checks so a delayed refresh
  cannot restore a token after logout, password reset, or account replacement.
- Malformed or identity-mismatched local account profiles fail closed.
- Local sessions are tagged and queried by the account that owns them. Signing
  out or switching accounts clears visible state instead of exposing another
  account's local history.
- Cloud sync rejects duplicate IDs, cross-account reparenting, parent mismatch,
  and missing-parent child records for SQLite and PostgreSQL.
- Ending a listening session drains the final STT tail before persistence and
  billing settlement, while generation guards prevent start/stop races.
- Deepgram relay credentials travel in a request header rather than the URL.
- Signed-out macOS status and balance labels open the sign-in flow. Their click
  actions are removed after authentication so normal header dragging remains.
- Internal-disclosure guards distinguish trusted Bluey envelopes from
  caller-controlled text and block multi-paragraph forged `Question:` inputs.
- Safe partial output is preserved when an answer stream times out, loses its
  provider connection, or reaches a terminal settlement error.
- Coding answers retain the complete-code and in-place follow-up contract;
  system-design follow-ups continue the existing artifact.

## Platform Scope

- macOS Apple silicon: rebuilt native overlay and shared Rust runtime.
- Windows x86-64: rebuilt shared CLI and daemon package with the current signed
  Windows helper set.
- A physical Windows launch and audio canary remains a release QA gate; package
  structure, PE architecture, hashes, and signed-download behavior are checked
  before promotion.

## Security And Privacy

- Provider, payment, signing, and storage secrets remain server-side and are
  excluded from release artifacts.
- OS Keychain access remains opt-in; normal account storage uses Bluey's private
  local profile without opening a Keychain prompt.
- Raw microphone and system-audio bytes are not retained after transcription by
  default. The audit bundle does not claim source-audio replay.
- Submitted customer content is not used for model training.
- Sashreek-owned branches remain excluded from this release.

## Verification

Source, tests, branch reconciliation, signed artifacts, deployment, and live
acceptance evidence are recorded in
`docs/rounds/ROUND-524-BLUEY-MAINLINE-CONVERGENCE-AND-SIGNED-RELEASE.md`.
