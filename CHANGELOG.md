# Changelog

All notable Bluey changes are documented here. Historical release detail lives
under `docs/release/`.

## [Unreleased]

### Recovery checkpoint (not release-ready)

- Added a retrievable next-developer starting point and preserved the remaining
  Windows predecessor edit as an explicitly unverified WIP checkpoint. Raw
  recovery evidence remains private; this does not complete or release Phase 625.
- Added a portable branch-local `bluey-ops` skill, round ownership/handoff
  checklist, and linked agent/work/PR templates. Recovery remains stopped and
  incomplete; this documentation update does not certify or deploy product code.
- Preserved partial Phase 625 desktop UI/latency source after the temporary
  checkout disappeared. Recovery, review, and release gates remain open; see
  `docs/work/HANDOFF-PHASE-625-RECOVERY-20260921.md` before continuing.

### Added

- Added an opt-in, account-scoped, metadata-only diagnostic spine that
  correlates native input, audio/STT, RAG, model streaming, persistence, and
  native rendering without storing transcript, question, answer, prompt, path,
  URL, token, or provider-response content.
- Added append-only support-diagnostic consent receipts, consent-epoch and
  deletion tombstones, and capability-bound account-deletion reconciliation so
  revocation, response loss, and stale uploads fail closed.
- Added a disposable local Rust-test launcher that isolates Cargo, databases,
  application state, logs, temporary files, and external authority, then cleans
  the complete owned process group after success, failure, or interruption.
- Added a one-time, keyboard-accessible macOS shortcut coachmark after visible
  desktop sign-in, anchored to the real Shortcuts control without enlarging the
  112 by 30 Bluey pill.
- Linked the Bluey landing, account, download, and policy headers to Bluey Jobs
  with a compact, accessible new-tab action across desktop and mobile layouts.
- Added an opt-in, verified R2 cold-storage lifecycle for expired, unreferenced
  global job candidates. PostgreSQL drops the heavy candidate body only after
  object-store read-back matches the exact bytes and SHA-256 written.
- Added revisioned, Track-scoped Auto-submit authorization bound to one
  verified application identity, current source resume, Career Track policy,
  and confirmed candidate facts.
- Added post-fill and immediate pre-submit ATS form read-back so silently
  discarded answers or documents pause the application instead of producing an
  incomplete submission.

### Changed

- Bound cloud clients and delayed daemon work to one exact credential snapshot
  across owner, profile generation, API origin, device, and token pair, while
  keeping OS Keychain use opt-in rather than enabling new prompts.
- Routed active Bluey test, smoke, CI, handoff, review, and release runbooks
  through the isolated launcher; release/package artifacts remain outside its
  cleanup boundary so the exact tested artifact can be promoted.
- Routed opt-in pre-commit Clippy through the same isolated launcher and
  expanded its authority scrub to managed tokens, API aliases, FFmpeg, and the
  context-picker override.
- Simplified desktop account linking to one explicit `Connect this Bluey`
  confirmation because the browser URL already carries the one-time code;
  manual code entry remains an explicit fallback.
- Reduced managed General-answer prompt duplication while preserving explicit
  mode and session rules, and refined the macOS overlay with a true 112 by 30
  compact pill, higher-contrast light surfaces, accessible history and
  confirmation controls, fitted shortcut help, and a bounded focus workspace.
- Preserved three remaining Codex-owned local branch tips on named remote archive
  branches and documented why their older discovery, spend-accounting, and
  optional-provider patches must not be bulk-merged over current main.
- Preserved the complete historical Bluey Jobs experimental worktree on a
  dedicated remote snapshot branch and reconciled every remaining Codex-owned
  branch against current main without merging obsolete runtime, generated
  asset, or incomplete Coach/IPC changes.
- Promoted the reviewed Round 586 Jobs authority and ATS form read-back build
  through a backed-up, exact-artifact manual deployment, while keeping model
  generation, Browser distribution, and mailbox synchronization disabled.
- Made global discovery refreshes semantic and candidate writes
  content-addressed, so unchanged manifests preserve their completed schedule
  and unchanged encrypted job payloads no longer rewrite canonical rows.
- Required the local `$bluey-ops` operating-memory preflight across Bluey agent
  entry points, work templates, and runbooks, with CI coverage for future docs.
- Replaced the landing-page overlay mockup with a faithful, responsive preview
  of the shipped Bluey host toolbar, answer workspace, caption rail, and
  composer.

### Fixed

- Prevented stale Account A balance, listen, audio/STT, sync, RAG, answer, and
  deletion work from clearing or publishing against refreshed Account A2 or
  replacement Account B authority.
- Prevented support-diagnostic, session, account, object, and derived-memory
  resurrection after consent revocation or deletion, including reserved-upload
  and response-loss races.
- Removed transcript text, provider error bodies, credentials, endpoints,
  vocabulary, and local paths from STT error/debug/trace formatting across
  OpenAI, Deepgram, the provider factory, and local Whisper.
- Kept disposable overlay-protocol smoke helpers debug-only and accepted them
  only beneath the exact marked test workspace. The marked-root verifier now
  compiles only for debug/tests, keeping release builds warning-free while
  release helper verification remains unchanged.
- Kept shortcut help fully readable in macOS light mode at every overlay
  opacity, including live theme changes, while delayed onboarding now preserves
  active modals, history, and composer focus.
- Restored Rust 1.98 strict-Clippy compatibility across the root workspace and
  server without changing public API response shapes.
- Kept system-audio integration checks reliable under parallel workspace load
  while preserving a bounded five-second helper-startup failure deadline.
- Restored hosted managed answers by separating Bluey's exact signed-release
  system contract from caller-controlled answer rules at the disclosure guard,
  preserving exact v0.1.97 through v0.1.104 client compatibility while unknown
  contracts and disclosure-bearing fields still fail closed.
- Mapped the managed disclosure block through a bounded typed desktop error so
  the overlay can give a safe recovery instruction without exposing arbitrary
  server response text.
- Serialized Windows AppUserModelID updates so concurrent disguise reassertion
  cannot corrupt the `cue-stealth` process heap during normal use or tests.
- Made Bluey Jobs submission state and evidence runner-owned: customers can no
  longer forge evidence or mark an application submitted, final receipts commit
  atomically with the bound run and resume, browser writes are read back before
  submission, and packet allowances no longer leak or cross billing periods.
- Restored Rust 1.97 warning-as-error compatibility across Linux, macOS, and
  Windows builds by aligning platform-specific imports and helper functions
  with their actual compile targets.
- Restored the Linux system-audio integration stub without enabling unsupported
  release capture, and replaced a raw Jobs rate-limit account log with its
  canonical hashed observability identifier.
- Made intentional system-audio shutdown abort diagnostic draining immediately,
  keeping stalled-helper cancellation inside the public stop deadline under
  loaded Linux CI runners.
- Split workspace and server tests across isolated GitHub runners and removed
  duplicate feature-push executions when a pull request already supplies the
  same required CI gates.
- Kept Bluey Jobs match filters stable across refresh, back/forward navigation,
  direct links, themes, mobile layouts, and pagination; hid inactive Career
  Tracks, normalized workplace values, included 100% matches, and separated a
  filtered-empty result from an empty discovery account.
- Restored hourly PostgreSQL replication to the bucket-scoped R2 backup
  destination and verified a full remote read-back against the local archive
  checksum without restarting production services.
- Activated bounded global-feed row quarantine in production so malformed rows
  are recorded with typed, replay-safe evidence while valid Ashby and Lever
  snapshots continue to publish and remain healthy.
- Quarantined bounded semantically incomplete global-discovery rows with exact,
  replay-safe evidence so one malformed row no longer degrades an otherwise
  valid source snapshot.
- Kept both Bluey Jobs discovery workers independent from resume/PDF rendering
  dependencies so their Linux runtimes do not require an optional native canvas
  binding.
- Kept the Bluey Jobs global discovery worker independent from resume/PDF
  rendering dependencies so its Linux runtime no longer requires an optional
  native canvas binding.
- Rejected non-portable Bluey Jobs worker archives during the build so macOS
  metadata cannot create a second hidden release root on Linux.
- Made Bluey Jobs discovery workers independently supervised from API
  maintenance, deployed from one retained immutable runtime artifact, and
  monitored for both process availability and overdue source snapshots.
- Made the Jobs portal derive source health from the last successful sync so a
  historically healthy source cannot appear current after updates stop.
- Refreshed Jobs runtime dependencies for current archive, sanitizer, URI, CSS,
  and client-router security fixes; the portal remains a client-only Vite SPA
  with no React Server Components or server actions.
- Prevented Review-first packet approval from becoming reusable Auto-submit
  authority, and made legacy Auto-submit packets fail closed.
- Prevented an ATS-controlled field that only appears filled from passing
  validation when the employer form did not register the value.

## [0.1.104] - 2026-07-19

### Added

- Added a gated, undistributed Bluey Browser controller with explicit
  background opt-in, tray controls, intervention notifications, crash-safe
  checkpoints, dedicated platform icon families, and truthful
  local-versus-cloud status copy.
- Added scheduled, verified public-ATS discovery for Greenhouse, Lever, Ashby,
  SmartRecruiters, and Workday with canonical board ownership, atomic snapshot
  publication, bounded reads, deduplication, and stale-job filtering.
- Added live server authorization immediately before every irreversible local
  browser submit, with distinct result, resume, and submit capabilities.
- Added global managed-provider spend holds, authoritative usage provenance,
  conservative migration baselines, and fail-closed accounting across answer,
  search, embedding, transcription, vision, and Jobs generation routes.

### Changed

- Split the oversized Jobs persistence and answer-router modules into focused
  domain modules while preserving reconstructed source identity and behavior.
- Aligned embedded and operator PostgreSQL migrations, including the physical
  Jobs discovery-board ownership migration and operator-discoverable targets.

### Fixed

- Skip the macOS installer sudo prompt during an update when every public
  command symlink already points at the fixed Bluey install directory.
- Kept managed Jobs resume ranking extractive and default-off, with exact
  evidence composition, bounded provider deadlines and spend, fail-closed
  attempt accounting, and authoritative recovery of legacy application IDs.
- Kept secure-store access disabled when the release environment sets its four
  opt-out flags to `0`, including a current-user-only Windows recovery key path
  that does not invoke DPAPI.
- Removed Playwright dependency-scanner tooling that is not required by the
  packaged Windows Browser runtime.

See `docs/release/RELEASE-v0.1.104.md` for release detail.

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
