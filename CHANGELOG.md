# Changelog

All notable Bluey changes are documented here. Historical release detail lives
under `docs/release/`.

## [Unreleased]

## [0.1.103] - 2026-07-19

### Fixed

- Made the `0.1.96` to `0.1.102` ownership upgrade recover legacy unscoped
  session projections without losing turns, saved answers, or active-session
  state.
- Kept account isolation fail-closed: startup only adopts an exact same-ID
  `NULL`-owner row and still rejects every non-null cross-account mismatch.
- Made overlay restart readiness generation-scoped and dependent on complete,
  ordered state hydration instead of process launch alone.
- Kept programmatic overlay restoration from feeding back as a user preference
  change, and fixed the Windows capture build under platform `min`/`max` macros.

See `docs/release/RELEASE-v0.1.103.md` for release detail.

## [0.1.102] - 2026-07-16

### Added

- Consent-first continuous work context with semantic browser capture,
  app/domain exclusions, bounded local retention, and an explicit screenshot
  fallback.
- Local meeting detection and a native start/ignore/settings banner without
  recording until the user chooses to start.
- Durable encrypted Jobs checkpoints for safe crash recovery, with terminal
  `side_effect_unknown` handling across irreversible submission boundaries.
- Terminal controls for context privacy, meeting suggestions, exclusions,
  retention, cadence, and screenshot fallback.
- macOS Intel and universal release artifacts alongside Apple silicon and
  Windows x86-64.

### Changed

- Rebuilt the overlay lifecycle around generation-fenced restart and complete
  state rehydration.
- Hardened microphone/system-audio routing, local VAD, STT retry, final-segment
  deduplication, and explicit exhaustion errors.
- Made cloud processing require both persisted enablement and consent at every
  upload/index boundary.
- Made page text and screenshots owner-private, atomic, and symlink/reparse
  resistant.
- Merged the Jobs resume-import onboarding, accessible modal UX, visible error
  handling, and production Web build with the recovery work.
- Made signed release publication immutable and ordered: versioned assets and
  signature first, signed manifest second, convenience installer aliases last.
- Made the macOS installer ad-hoc sign and strictly verify both top-level
  executables and nested helper application bundles before reporting success.
- Removed the Windows placeholder-transcription helper from release packages;
  Windows now fails closed when a real local STT capability is unavailable.

See `docs/release/RELEASE-v0.1.102.md` for release detail.

## [0.1.99] - 2026-07-12

### Added

- Atomic managed-usage reservations with detached stream settlement and stale
  reservation reconciliation.
- Account/workspace ownership for local RAG and durable quota/outbox records for
  artifact and session-audit object uploads.
- Durable Stripe Auto Reload attempts with exactly-once credit and reversal.
- Bounded dynamic Windows NDJSON parsing for long and fragmented answer events.

### Changed

- Repositioned Bluey as a consent-first live-context assistant for engineering
  meetings and technical work.
- Replaced the homepage terminal simulation with the real overlay experience
  and reduced first-run command and shortcut clutter.
- Made cloud session sync off by default for new installs and exposed its real
  persisted preference in desktop Settings.
- Made Auto Reload an unselected opt-in during balance setup.
- Reframed capture exclusion as a best-effort screen-share privacy control and
  removed automatic app-identity disguise from the dashboard path.
- Aligned public privacy and terms copy with current behavior: raw audio is not
  retained after transcription by default, and submitted content is not used
  for model training.
- Updated platform, install, update, and help claims to the `0.1.99` public
  release manifest.
- Bound audio, transcripts, answers, and artifacts to the session/account that
  dispatched the work, including a final-transcript high-water mark.

See `docs/release/RELEASE-v0.1.99.md` for release detail.

## [0.1.98] - 2026-07-10

### Added

- Signed, checksum-pinned release artifacts for macOS Apple silicon and Windows
  x86-64.
- Durable context, answer, transcript, artifact, and UI-event identities for
  session sync and diagnostics.
- Source chips, answer recovery, context readiness states, and workbench
  follow-up continuity.

### Changed

- Kept `Auto` as the default answer path with optional `Quick` and `Thorough`
  overrides.
- Improved transcript settlement, listening idle protection, provider fallback,
  device linking, and account-state handling.
- Corrected packaged macOS and Windows process aliases after the rejected
  `0.1.97` identity gate.

See `docs/release/RELEASE-v0.1.98.md` for checksums and verification detail.

## [0.1.0] - 2026-05-12

### Added

- Initial Rust workspace, native macOS overlay foundation, CLI lifecycle, and
  development workflow documentation.
