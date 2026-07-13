# Bluey Release v0.1.100

Released: 2026-07-12
Audience: known production-beta users

## Summary

`0.1.100` closes interrupted security, session-sync, account-recovery, and
overlay-continuity work without importing stale experimental branches.

## What Changed

- Disclosure guards scan the complete untrusted request and block forged
  `Question:` envelopes without false-positive blocking normal coding context.
- Password reset changes the password and revokes every existing refresh
  session atomically.
- Cloud session sync rejects stale parent and child updates, reports only rows
  actually applied, and cannot relink a stale artifact to another session.
- Session deletion keeps a tombstone, purges transcript, response, context, and
  RAG rows, and prevents a later desktop sync from restoring them.
- The desktop answer task no longer blocks attachment or screen-context events
  while an answer is streaming.
- Signed-out macOS balance/status labels open the existing sign-in flow and
  return to draggable header behavior after authentication.
- The optional metered AnswerPlan classifier is documented as disabled by
  default, matching the deterministic local-rules-first runtime.

## Platform Scope

- macOS Apple silicon: rebuilt native overlay and shared Rust runtime.
- Windows x86-64: rebuilt shared Rust CLI/daemon for MSVC. Unchanged native
  overlay/audio/Whisper helpers are carried forward byte-for-byte from the
  verified `0.1.99` Windows package.
- Windows account-state chrome remains a separately documented native UX parity
  gate; server and shared daemon auth enforcement remain fail closed.

## Security And Privacy

- No provider, payment, signing, or storage secret is included in release
  artifacts.
- Temporary source audio remains deleted after transcription by default.
- Submitted content is not used for model training.
- Raw provider errors stay in redacted diagnostics; user-visible recovery uses
  bounded retry and short references.

## Verification

Source, test, package, signature, deployment, backup, and live evidence is in
`docs/rounds/ROUND-519-INTERRUPTED-ASKS-RECONCILIATION.md`.
