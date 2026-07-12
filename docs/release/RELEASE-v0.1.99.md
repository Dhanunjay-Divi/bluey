# Bluey Release v0.1.99

Released: 2026-07-12
Audience: known production-beta users

## Summary

`0.1.99` is a reliability and trust release. It binds live audio, transcripts,
answers, artifacts, and local retrieval to the account/session that started the
work; reserves paid usage before provider dispatch; makes Stripe Auto Reload and
R2 uploads durable and idempotent; and removes misleading stealth claims from
the normal product experience.

## Supported Platforms

| Platform | Status | Release gate |
|---|---:|---|
| macOS Apple silicon | Supported | Native release build, signed manifest, checksum, install smoke |
| Windows x86-64 | Supported | Native MSVC build, signed manifest, checksum, install smoke |
| macOS Intel/universal | Not advertised | No exact promoted artifact in this release |
| Linux | Not advertised | Desktop overlay is not production-certified |

## What Changed

- Final transcript answers wait for the bounded STT drain and consume only the
  words actually submitted.
- Session/account changes cancel stale audio and answer generations.
- Local RAG is partitioned by account and optional workspace.
- Managed answers reserve trial/balance usage atomically before dispatch and
  settle even when the client disconnects.
- Stripe Auto Reload persists attempts before confirmation and reconciles
  success, failure, refund, and dispute exactly once.
- Artifact/session-audit uploads have account quotas, stable keys, durable
  metadata, cleanup, and retry outbox state.
- Windows uses bounded dynamic NDJSON parsing instead of truncating real answers.
- New installs use explicit saved-session sync state; signed-in legacy installs
  retain their existing sync preference.
- Bluey presents screen-share privacy honestly as best effort and focuses the
  product on consent-first live technical context.

## Security And Privacy

- Provider and payment secrets remain server-side and are scanned out of release
  artifacts.
- Temporary raw audio is deleted after transcription by default.
- Submitted content is not used for model training.
- Account-scoped local retrieval and object ownership prevent cross-account
  context leakage on shared devices.
- Capture-visible development flags are rejected during release publishing.

## Verification

The detailed source, test, migration, platform-parity, billing, and storage
evidence is recorded in
`docs/rounds/ROUND-478-RELIABILITY-TRUST-AND-USAGE-HARDENING.md`.

Live artifact hashes, production backup identity, API commit, signature checks,
and installation smoke results are recorded in the signed deployment round.
