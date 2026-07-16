# Bluey Release v0.1.102

Released: 2026-07-16

Audience: known production-beta users

## Summary

`0.1.102` is the context, recovery, and release-integrity release. It adds the
consent-first foreground-work awareness requested after the Littlebird review,
hardens the native overlay/audio/STT lifecycle, gives terminal-only users the
same privacy controls as the optional dashboard, and makes Bluey Jobs recover
safe work after a crash without replaying a potentially completed submission.

This release uses patterns learned from the supplied owner-authorized reference
material, but ships Bluey's independently maintained Rust, Swift, C/C++, and
TypeScript implementations. It does not paste minified bundles or unreviewed
third-party binary code into Bluey.

## Product Changes

- Explicit Context Watch with foreground supported-browser observation,
  semantic text first, query/fragment stripping, content deduplication,
  app/domain exclusions, bounded local history, and screenshot fallback off by
  default.
- Local meeting detection with Start, Ignore, and Settings actions. Detection
  alone never starts capture.
- Generation-fenced overlay launch and rehydration with bounded supervised
  restart.
- Dual-source STT retry and final-segment deduplication with typed terminal
  failure instead of silent or fabricated output.
- Private atomic page-context and screenshot files with owner, link,
  symlink/reparse-point, and permission checks.
- Cloud upload and RAG indexing require both current persisted enablement and
  consent, including revocation during queued work.
- Bluey Jobs onboarding can import supported resumes, preserve factual profile
  edits, and launch a Career Track.
- Bluey Jobs stores encrypted, scope-bound recovery checkpoints and restores
  only safe pre-submit phases. Ambiguous irreversible boundaries become
  `side_effect_unknown` and require reconciliation.
- Accessible portal dialogs, focus containment, visible request errors, and
  explicit browser-takeover availability.
- Inbox/calendar controls are explicitly request-only: no OAuth, mailbox read,
  timeline sync, or slot charge is claimed.

## Platform Scope

- macOS Apple silicon.
- macOS Intel.
- macOS universal.
- Windows x86-64.

The terminal release contains CLI, daemon, and the platform-native helpers.
The optional dashboard remains source-tested but is not required by the
terminal install. Windows does not ship a placeholder local-Whisper helper;
managed live captions are the supported speech path until a real local engine
is packaged and validated.

The separate Electron-based Bluey Browser remains gated even when a Jobs plan
would otherwise include it. It becomes visible only after a versioned
distribution, updater, and physical install/launch canaries are approved.

## Security And Privacy

- No capture begins merely because a meeting or supported browser is detected.
- Screenshot fallback is explicit and off by default.
- Account tokens remain in Bluey's owner-private local account profile by
  default. OS Keychain/Credential Manager access remains opt-in or legacy-only.
- Jobs recovery never persists a root claim ticket, worker signing key, lease
  token, or plaintext active browser profile.
- Release manifests use Ed25519 signatures and immutable versioned asset URLs.
  Publication moves the signed `latest.json` only after versioned assets,
  checksums, installers, and signature are visible.
- The macOS installer ad-hoc signs and verifies every installed executable and
  the nested file-picker application bundle before it reports success.
- Raw microphone/system-audio bytes are not retained after transcription by
  default.

## Verification And Evidence

Source, cross-product findings, tests, artifacts, deployment evidence, and
remaining hardware/provider canaries are recorded in
`docs/rounds/ROUND-527-CONTEXT-RECOVERY-UX-AND-ATOMIC-RELEASE.md`.
