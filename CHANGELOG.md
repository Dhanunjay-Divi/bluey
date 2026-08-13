# Changelog

All notable Bluey changes are documented here. Historical release detail lives
under `docs/release/`.

## [Unreleased]

### Added

- Added a disabled-by-default, revisioned Bluey Jobs operational-hold control
  plane for discovery, managed generation, application queueing, runner claims,
  final Submit authorization, mailbox sync, and reviewed communication
  dispatch. Holds are scoped by server-owned global, source, ATS, employer,
  account, Career Track, region, runner, mailbox-provider, and model-provider
  identities; they stop only new authority and preserve receipt persistence and
  reconciliation after a possible side effect. Existing source pauses and
  signed ATS circuits remain independent fail-closed authorities. Canonical
  head validation covers held and released states across admission, readiness,
  listing, replay, and release; semantic schema guards protect paired trigger,
  ancestry, and immutability rules. Exact provider request-start and local
  `click_started` recovery survives a later hold. A local run keeps its exact
  frozen Browser build/release binding while allowing a current-server
  deployment `A -> B` only when the immutable bound activation accepted both
  IDs; an unaccepted server ID is denied. Signed v2 submit grace, a local
  distribution pause, and object-storage maximum drift preserve only durable
  recovery, reusing its exact active reserved capacity under current upload
  limits. Signed v1/v2 result reconciliation retains this capacity for
  `click_started`/`side_effect_unknown`; claimed, `needs_input`, and new paths
  derive current configuration, while legacy submit and invalid durable
  authority remain denied. Mailbox sync validates canonical, relational, and
  encrypted provider projections before leasing. Discovery admission freezes
  Career Track mutations for the lease, rejects Track projection drift, omits
  inactive Tracks from curated selection, and retains Region holds for an
  explicitly bound inactive Track. A partially materialized curated posting
  without its exact managed discovery membership fails closed until repaired.
  PostgreSQL application/job context and every relevant discovery writer share
  one account fence, so admission evaluates a stable scope snapshot. An
  `unassigned` application reservation omits runner-kind scope; later cloud or
  local claims evaluate their concrete runner kind before binding it.
  Operational API, metrics, debug, and
  discovery-diagnostic projections redact private identities and isolate child
  process secrets. No provider, tenant, credential, release flag, or production
  state is changed by this source work.
- Added a disabled-by-default, server-owned reviewed communication execution
  authority for Gmail/Outlook replies and Google/Microsoft calendar events,
  with explicit write-consent and action revisions, exact Reply-To and provider
  request/read-back markers, append-only attempt evidence, ambiguity-safe lookup
  reconciliation, durable deletion/disconnect drain fences, and truthful portal
  review controls. OAuth credentials remain inside the in-process server
  boundary (`bluey-jobs-api` or `bluey-server`); no provider write, lookup,
  consent upgrade, or production flag is enabled by this source change.
- Added a disabled-by-default, root-authorized ATS certification framework for
  exact Greenhouse and Lever targets, with signed evidence and layout sets,
  immutable manifests, channel activations, account-scoped canary allowlists,
  revocation, quarantine, safety circuits, exact local/cloud runtime bindings,
  atomic preflight and one-use submission authority, and terminal receipt
  recovery. No provider or production flag is enabled by this source change.
- Added a disabled-by-default Bluey Browser release authority with root-signed
  trust rotation, threshold-signed immutable manifests, channel activations,
  compare-and-swap rollback, append-only revocation, exact packaged build and
  protocol claims, transactional pre-click fencing, account-scoped immutable
  download metadata, and a no-rebuild native package gate that binds—but does
  not execute—canonical external canary evidence. Local distribution remains
  disabled pending approved credentials, immutable hosting/read-back, and
  physical macOS/Windows certification. Installed-app self-update is not
  implemented by this batch.
- Added signed managed runner-volume identity, process leases, chained
  current-storage attestations, conservative account-deletion fan-out, exact
  purge acknowledgements, opaque restore tombstones, provider-bound destruction
  evidence, and fleet cutover authority. Account deletion now returns durable
  pending status while any offline or legacy volume is unresolved, and clears
  account credentials only after verified hard deletion. Signed keyset polling
  prepares every bounded command page before the first acknowledgement, so
  global legacy-zero evidence cannot deadlock across subjects; unresolved
  storage attestations are promoted, retried byte-for-byte, discarded, or
  rejected according to exact predecessor, tombstone, enrollment, and local
  evidence bindings. Runner account data crosses a native retained-handle
  boundary under a dedicated unprivileged container principal. Container bases
  are digest-pinned, and Linux and Darwin gates load and smoke the exact staged
  native addon while runtime storage operations reject symlinks, hardlinks,
  special files, mount/root replacement, or a second runner.
- Added a durable Bluey Jobs submission-evidence lifecycle with protected
  pre-click capacity, immutable receipt and screenshot objects, authenticated
  integrity-checked downloads, account-deletion write fencing, bounded
  cross-replica PostgreSQL writer/deleter coordination, and migration coverage
  for legacy resume and encrypted browser-profile objects. Missing or
  unreachable object-storage namespaces retain the deletion fence instead of
  allowing an unproven GDPR success response.
- Added process-crash recovery for cloud and local Jobs runners with offline
  browser startup, guard-before-network restoration, fenced submitted-result
  replay, per-profile failure isolation, and serialized browser-session writes.
- Added durable, encrypted Bluey Browser profile recovery for cloud runners,
  with account-scoped object storage, lease-fenced generation updates,
  integrity-checked replacement-runner restore, and replay-safe metadata.
- Added an owner-confirmed reconciliation path for employer submissions whose
  result became unknown after the final click, with durable audit evidence,
  idempotent release of application attempts, and a bounded late-receipt path
  for trusted runners that subsequently prove the application was submitted.
- Added server-authoritative discovery evidence for canonical job identity,
  employer and application-domain binding, scam screening, original-source
  freshness, and immutable evidence hashes before Bluey Jobs can prepare or
  queue an application.
- Added fail-closed, application-identity-scoped Bluey Browser profiles with
  opaque server-compatible profile IDs, collision detection, path confinement,
  and upgrade-compatible profile directories.
- Added encrypted, account-scoped communication drafts for reviewed recruiter
  replies and interview calendar actions, with explicit approval, idempotency,
  mailbox binding, fenced worker leases, and non-retryable ambiguous outcomes.
- Added evidence-grounded, job-specific cover letters to managed Bluey Jobs
  application kits, with cited Career Profile facts, server-owned target-role
  framing, transactional persistence, and exact packet-review rendering.
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

- Made `/jobs/automation` the canonical Bluey Jobs execution surface and the
  managed cloud Background runner the sole customer queue choice. The launch
  portal no longer asks customers to install, download, select, or open a
  separate local Bluey Browser; legacy `/jobs/browser` links redirect to the
  Automation view, while active managed browser sessions, interventions,
  takeover, and final form review remain available. Cloud execution is still
  plan- and distribution-gated, every production flag remains unchanged, and
  the unmerged Phase 607 local updater work stays parked for possible P2 demand.
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

- Made Jobs provider write-grant upgrades advance their credential-refresh CAS
  timestamp even when both mutations occur in the same clock tick. A stale
  token refresh now deterministically loses authority instead of reaching a
  later mailbox-shape check, eliminating the hosted CI race without weakening
  the grant digest or transaction fence.
- Updated the Jobs and observability CI gates for hosted Rust 1.97 without
  changing runtime behavior: optional final-submit filename parsing now follows
  the current Clippy contract, while target-dependent Unix `libc` conversions
  and the shared fallible ACL boundary remain explicit and portable.
- Reduced the Browser release workflow to GitHub's 25-input dispatch limit by
  combining promotion-only evidence into one exact, digest-checked JSON
  envelope. Independent Jobs CI now rejects future input-limit drift and tests
  authority materialization even when GitHub cannot schedule a malformed
  release workflow. No release, promotion, credential, or production flag is
  enabled by this fix.
- Bounded hosted Jobs CI disk use before the server Rust matrix by disabling
  incremental/debug-heavy CI artifacts and reclaiming the already-verified
  Docker, native-storage, and Node dependency outputs. This prevents the
  ephemeral runner from exhausting its disk at the final integration-test step
  without weakening any test, privacy, or production gate.
- Closed reviewed communication authority over the exact source message,
  application, mailbox connection, provider/payload schema, calendar time zone,
  atomic action/approval revision, payload hash, and provider grant. Failed or
  unknown drafts can no longer retry implicitly; the post-marker provider call
  is bounded by its absolute lease, and timeout, transport loss, 5xx, and
  malformed success remain `side_effect_unknown` until exact lookup proves
  success or bounded absence returns the immutable draft to fresh review.
  Mailbox disconnect, account deletion/export, and portal availability now
  preserve unresolved authority without exposing provider credentials, lease
  secrets, raw provider errors, or private evidence fingerprints.
- Preserved exact ATS authority from eligibility through packet approval,
  irreversible Submit, receipt persistence, and intervention reapproval.
  Auto-submit packets now freeze schema-three certification, Phase B precedes
  every durable click marker, receipts must match the immutable terminal server
  record, and packet revisions atomically invalidate unused preflight authority.
  The shared queued/running lifecycle accepts either server-authorized runner,
  while local and cloud execution still require their exact signed release,
  process-runtime grant, image, Browser, and Chromium claims.
- Hardened the disabled ATS certification control plane so all thirteen signed
  revocation scopes form a predecessor-bound monotonic chain; activation and
  quarantine replays require their immutable predecessors; circuit restoration
  through `newer_activation` is limited to an exact predecessor activation
  circuit and requires the applied successor's complete authority to remain
  current after database serialization; broader circuits require reviewed
  closure; non-shadow
  authority requires complete authorized-live and authorized-sandbox evidence
  for every runtime; and activation-wide total, account, concurrency, and UTC
  daily canary limits are enforced atomically before any irreversible marker.
  Canary promotion now commits to that exact canonical evidence manifest, and
  ambiguous local Phase-B HTTP 5xx responses preserve recovery instead of
  becoming retryable launch denials.
- Unified Greenhouse and Lever application-target classification across Jobs
  policy, adapter resolution, source metadata, and server parsing. Exact Lever
  EU jobs now reach the review-only provider state machine, while HTTP,
  credentialed, port-qualified, malformed-path, and spoofed provider URLs fail
  closed before provider automation.
- Bound exact-version Greenhouse and Lever final-submit mechanics to the approved
  provider job, submit target, ordered successful controls, and content-addressed
  PDF bytes. A Chromium isolated world and boundary-preserving multipart
  verification now prevent page-script mutation or unrelated post-fill traffic
  from widening the authorized employer-facing action; unintended redirect
  statuses and returned or ambiguous submit forms, as well as contradictory
  negative submission text, cannot be promoted as successful confirmations.
- Preserved every bounded confirmation screenshot as a separately indexed,
  immutable evidence object and required the server and portal to verify the
  exact complete set while retaining compatibility with legacy single-image
  receipts.
- Prevented intervention answers from resuming an application under an obsolete
  approved packet. Answer changes now revise the stored packet and receipt,
  invalidate prior execution approval, fence cloud and local runner authority,
  release the active attempt, and return the application to explicit review.
- Restricted employer-facing final submission to exact-version provider state
  machines. Generic ATS and semantic adapters now stop at review or takeover,
  spoofed provider hosts fail closed, and adapter output alone cannot forge a
  Submitted result.
- Made cloud-runner restart recovery durable and fail closed: checkpoint files
  now bind to the exact account, application, run, browser profile, lease owner,
  fence, and v2 lease token; safe pre-submit work releases authority atomically,
  while any possibly activated Submit remains `side_effect_unknown` unless the
  server already holds a complete trusted receipt and evidence bundle.
- Verified generated DOCX application kits before release: Bluey now preserves
  source package order, compression, permissions, timestamps, relationships,
  styles, numbering, headers, footers, and media; rejects duplicate or invalid
  package parts and normalized no-op rewrites; and reopens every tailored file
  to prove that only the intended document text changed.
- Prevented cloud and local browser retries from converting an uncertain
  employer-facing submission into a duplicate application or false Submitted
  state; the application now remains blocked until either trusted receipt
  evidence arrives or the owner explicitly confirms it was not submitted.
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
