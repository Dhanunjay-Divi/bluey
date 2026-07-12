# Round 510 - Jobs Open-Source ATS Intelligence

## Scope

Selective production reuse from the job-application repositories supplied by
the project owner. This round stays entirely inside the isolated Bluey Jobs and
web/legal surfaces. It does not change the existing Bluey overlay, meeting
runtime, audio, transcription, or native saved-session behavior.

## Source review

- Cloned the nine supplied repositories and five additional MIT candidates
  from the GitHub job-application topic under ignored `_refs/jobs-research/`
  directories.
- Recorded exact commits, observed licenses, reuse decisions, and the project
  owner's explicit reuse authorization in `jobs/THIRD_PARTY_PROVENANCE.md`.
- Retained required MIT notices in `jobs/THIRD_PARTY_NOTICES.md`.
- Documented the selection rationale and rejected legacy patterns in
  `jobs/OPEN_SOURCE_RESEARCH.md`.

## Implementation

### Public ATS discovery

- Added source-configured Greenhouse, Lever, Ashby, SmartRecruiters, and
  Workday discovery.
- Normalized every source to the shared `NormalizedJob` contract.
- Added role/location/exclusion filtering and canonical deduplication.
- Added HTTPS-only endpoint construction, exact host pinning, redirect
  rejection, payload cap, timeout, bounded retry/backoff, and bounded
  pagination.

### Application form intelligence

- Added standard field aliases for identity, contact, location, links,
  employment, authorization, compensation, and availability.
- Added company, Career Track, then account answer-memory precedence.
- Added job-specific resume and cover-letter upload planning.
- Added required-field and sensitive-question interventions.
- Added the invariant that Auto-submit cannot use an unconfirmed generated
  Career Profile fact.

### Receipts

- Added an immutable application receipt containing the exact job snapshot,
  resume version, answers, confirmed claims, document hashes, run events,
  screenshots, timestamps, and final outcome.
- Added deterministic canonical serialization and SHA-256 fingerprints.

### Customer terms

- Clarified that approved application materials are sent to the selected
  employer and its application provider.
- Clarified local versus encrypted cloud browser profile handling and removal.
- Clarified connected email/calendar processing.
- Distinguished longer-lived Career Profile and application history from the
  90-day meeting/session sync window.
- Added open-source software and no-endorsement language.
- Kept implementation internals out of visible product copy.

## Verification

- `npm run typecheck --workspace @bluey/jobs-automation`
- `npm test --workspace @bluey/jobs-automation`
- `npm run build --workspace @bluey/jobs-automation`
- `npm run smoke --workspace @bluey/jobs-automation`
- `git diff --check`

Focused automation coverage includes normalization, filtering, dedupe,
Workday pagination, retry behavior, source identifier rejection, fact and
answer-memory resolution, sensitive questions, Auto-submit confirmation, exact
receipt contents, and deterministic receipt fingerprints.

The package import smoke test also catches a runtime-only entry-point mismatch
that TypeScript workspace resolution would otherwise hide.

## Remaining release gate

This round completes public discovery and shared planning. Deterministic
fixture-backed browser submission adapters for the five ATS families, licensed
discovery credentials, provider OAuth, packaged desktop installers, and the
production cloud browser pool remain beta gates and must not be represented as
generally available until their integration suites pass.
