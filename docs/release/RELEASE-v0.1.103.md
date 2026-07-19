# Bluey Release v0.1.103

Released: 2026-07-19

Audience: known production-beta users

## Summary

`0.1.103` is the startup-recovery and cross-platform release. It repairs the
legacy session-ownership upgrade that could make the `0.1.102` daemon exit
before becoming ready, and it hardens overlay restart rehydration so a
replacement process cannot report success before it has received the complete
current generation of state.

The desktop artifacts are built from sealed source commit
`381cbd532edee25ab596e01d1b30fa8dbd6e6d4b`. Later Jobs/server work has separate
source and deployment provenance and does not change the packaged desktop
inputs.

Signed live-manifest, installer, updater, and rollback checks are recorded in
`docs/rounds/ROUND-548-BLUEY-STARTUP-OWNERSHIP-RECOVERY-AND-CROSS-PLATFORM-RELEASE.md`
The desktop release is live. Corrected Jobs/server deployment remains a
separate, disabled-by-default workstream and is not implied by this release.

## Desktop Changes

- Startup adopts only an exact same-session legacy projection whose owner is
  `NULL`; it never adopts a row owned by another account.
- Existing turns, saved answers, and the active-session pointer survive the
  ownership upgrade.
- Overlay initialization is generation-scoped and ordered. Ready is
  acknowledged only after the current state has been enqueued.
- Restart recovery requires complete hydration of the exact replacement
  generation and uses one bounded retry budget.
- Ordinary overlay events remain ordered and backpressured instead of being
  silently discarded.
- Programmatic macOS opacity restoration no longer feeds back as a user
  preference change.
- Windows native capture builds cleanly even when platform headers define
  `min` and `max` macros.

## Platform Artifacts

All artifacts below were built from the sealed desktop source commit. The
macOS archives were built twice and reproduced byte for byte. The Windows ZIP
was deterministically canonicalized twice from the exact Windows build and
reproduced byte for byte.

| Platform | Bytes | SHA-256 |
|---|---:|---|
| macOS arm64 | 22,029,698 | `ffe67c1ac100ac7e02022746d0103103166bb194a00d1a04fc75cb58b36647e8` |
| macOS universal | 45,421,535 | `ba043ea6e03da1bc7e38ae9a8bd5e66c3f855b5d1673be7c5deb1e542baef94e` |
| macOS x86_64 | 23,385,574 | `f53763814cf9d000911a64fa95cc56c2f95a6bf2d473a8d35dd5ad2b0d27db9d` |
| Windows x86_64 | 22,734,864 | `3b857f1afeb455d8aff2be67abe82a77a06b2468d0b5e75bea46457defe5daba` |

The source archive is
`bluey-0.1.103-381cbd53-source.tar.gz`, 33,104,840 bytes, SHA-256
`18660a89dce368f476dd4756014dee96b4856a4ef81e9013a2b03277ddfd1808`.
Its tar payload is byte-identical to `git archive` for the sealed commit and
contains 2,153 safe relative members.

## Installed-Runtime Verification

The exact packaged archives passed installed-runtime checks on:

- macOS arm64, natively;
- macOS universal, natively;
- macOS universal, under Rosetta;
- macOS x86_64, under Rosetta; and
- Windows x86_64 in an interactive Session 1 desktop.

Each lane verified the packaged CLI and daemon version, exact executable
identity, stable daemon ownership, a real visible secure overlay window,
capture exclusion, owner-only IPC, clean shutdown, and zero remaining test
processes. Windows additionally verified a topmost layered tool window with
`WDA_EXCLUDEFROMCAPTURE` (`0x11`).

All runtime gates explicitly set these values to `0`:

- `BLUEY_USE_OS_KEYCHAIN`
- `BLUEY_USE_SECURE_STORE`
- `BLUEY_LEGACY_KEYRING_FALLBACK`
- `BLUEY_ALLOW_PLAINTEXT_TOKENS`

The release does not require Keychain or Windows Credential Manager access.

## Jobs And Managed Resume Generation

Managed resume generation is a separate server capability. It remains
default-off and must remain explicitly disabled in production until its exact
final source passes truth-boundary, quota, spend-reservation, provider-health,
concurrency, PostgreSQL restore, migration replay, and rollback gates.

Bluey must compose candidate claims only from exact selected evidence. Model
text may not create or reassign employers, dates, skills, credentials, metrics,
or outcomes. Deterministic tailoring remains the fail-closed path when managed
generation is disabled or unavailable.

Browser execution distribution also remains explicitly disabled. No release
claim implies that a Temporal worker, local browser runner, cloud Chromium
pool, mailbox integration, or calendar integration exists in production.

## Security And Privacy

- Account ownership upgrades remain fail-closed across account boundaries.
- Overlay capture exclusion is verified on the real native windows, not only
  from unit-test state.
- Account tokens remain in Bluey's owner-private local profile unless the user
  separately opts into an operating-system secure store.
- Provider credentials, prompts, resumes, job descriptions, customer records,
  and restored database contents are not included in release evidence.
- Signed release publication uses immutable versioned assets and an Ed25519
  manifest signature.
- The terminal release does not require signed/notarized application-bundle
  distribution; macOS installed executables are ad-hoc signed and verified by
  the installer.

## Publication Verification

The immutable release and signed manifest are live at `https://bluey.sh`.
Verification completed for:

1. signed manifest SHA-256
   `fce60206d0af59eac71539aea5ff656e4ea0f64ac2978d64ed03fafc02f0f3eb`;
2. detached signature SHA-256
   `56f01623b45416cd961d91fc7b0fd01a3ffa2c5884cedd9f09283b950c831d49`;
3. every macOS and Windows platform URL, size, hash, content type, checksum,
   installer hash, and manifest signature;
4. an isolated fresh install that reported CLI and daemon `0.1.103` without
   changing the existing global CLI link; and
5. an isolated signed updater run from `0.1.102` to `0.1.103` that started the
   exact packaged daemon, reported capture exclusion, and shut down cleanly.

Jobs/server truth, accounting, PostgreSQL migration 008, and scoped deployment
gates remain pending and model/browser execution remains disabled in
production. They do not block or weaken the shipped desktop startup repair.
