# Round 586 - Jobs Track Auto-submit Authority And Form Read-back

Date: 2026-08-02

Status: implementation and verification complete; release pending

## Objective

Make Review first and Auto-submit separate, durable product authorities instead
of two labels that eventually share the same queue action.

Auto-submit now means that the user explicitly authorized one Career Track with:

```text
verified application identity
+ exact current source resume
+ current Career Track policy
+ current confirmed Career Profile facts
+ one revisioned server fingerprint
```

Changing any bound input makes the authorization require review again. Review
first continues to require approval of the exact prepared Application Kit.

## Product Behavior

### Track-scoped authorization

Settings shows Auto-submit independently for every Career Track:

- `Review first` when no authorization exists;
- `Active` after the user reviews and authorizes the Track;
- `Needs review` when the identity, resume, profile facts, or Track policy
  changes;
- `Unavailable` until the Career Profile has a saved source resume and the
  Track has a verified application identity.

One authorization never grants another role, location, identity, or resume
permission. Revoking it returns that Track to Review first without changing
other Tracks.

### Preparation and queueing

An Auto-submit Application Kit can be prepared only when:

- the Track authorization is active;
- the current server-owned eligibility decision permits Auto-submit;
- an entitled and distributed runner is available.

Review-first preparation does not inherit Auto-submit permission. Existing
Review-first approval remains an explicit, packet-specific action.

### Frozen execution authority

Before queueing, Bluey freezes schema-versioned execution authority into the
Application Kit:

- Review first stores a `review_approval` admission;
- Auto-submit stores a `track_auto_submit` admission with the authorization
  ID, Track ID, revision, and authority fingerprint;
- the admission, packet, and job are covered by one checksum.

Immediately before an irreversible Submit, the server rechecks the current
account, Track, verified identity, Career Profile source resume, confirmed
facts, eligibility, receipt, and frozen admission. Legacy packets remain valid
only for Review first. Legacy Auto-submit packets fail closed.

### Employer form read-back

The browser automation package records provider-neutral expectations after
each successful fill:

- text, email, phone, URL, and select values;
- radio and checkbox state;
- uploaded resume or cover-letter filename.

It reads a fresh form snapshot after filling and again immediately before
Submit. If Workday, Greenhouse, Lever, Ashby, SmartRecruiters, or the bounded
fallback form silently discards a value, Bluey pauses with a blocking,
human-readable issue instead of submitting an incomplete application.

Validation messages identify the field but never include the candidate's
answer, email, phone number, file path, or other private value.

## Career-ops Adaptation

The owner-provided `santifer/career-ops` reference was rechecked at
`267dfb7079877e2beb2a949eb7b404b7bd257332`.

Its useful production lesson is that a browser write is not proof that an ATS
accepted the value. React-controlled fields and save-step forms must be read
back before proceeding.

Bluey adapted that principle into its typed browser contract. No source file,
selector set, wording, or asset was copied. Bluey's implementation adds the
multi-tenant authority, durable revisions, exact packet checksums, pre-submit
revalidation, receipts, and idempotent accounting that the reference project
does not provide.

## Data And API

New durable records:

```text
jobs_auto_submit_authorizations
  account_id
  career_track_id
  application_identity_id
  source_resume_asset_id
  authority_fingerprint
  revision_no
  authorized_at_ms
  revoked_at_ms
```

The active row is unique per account and Career Track. SQLite uses an immediate
transaction. PostgreSQL uses a transaction and Track-scoped advisory lock so
concurrent authorization attempts cannot create two active revisions.

Authenticated endpoints:

```text
POST   /api/jobs/tracks/:track_id/auto-submit
DELETE /api/jobs/tracks/:track_id/auto-submit
```

The workspace includes the public authorization status and revision, while the
authority fingerprint remains server-only.

## Verification

```text
Server full library tests
  passed

Server Jobs HTTP integration
  16 passed, 61 filtered

Rust formatting and strict Clippy
  passed

Full Jobs workspace
  489 tests passed
    automation 198
    browser 100
    runner 50
    workflows 54
    portal 87
  complete TypeScript checks passed
  production build passed

Repository gates
  privacy: 2,230 tracked paths and 1,958 text files checked
  SQLite/PostgreSQL schema parity: 5 tables and 9 indexes checked
  provenance/license: 663 lock entries, 631 versions, and 14 pinned repos checked
  client/server boundary and CI guard passed

Dependency audit
  patched reachable archive, sanitizer, URI, and CSS advisories
  two React Router RSC/server-action advisories remain in the dependency graph
  but are unreachable because Jobs is a client-only Vite SPA with no RSC or
  server-action endpoint

Responsive visual QA
  Settings: 1,440 x 1,000 desktop and 390 x 844 mobile
  Matches: 1,440 x 1,000 desktop
  light and dark themes
  source-resume authority, Track state, buttons, navigation, and text fit
```

Regression coverage includes:

- authorization is Track-scoped and revisioned;
- changing the bound resume makes authorization stale;
- schema-one Review approval remains readable;
- schema-one Auto-submit fails closed;
- authorization admission changes the approved packet checksum;
- ignored browser writes block validation for the shared adapter path,
  Greenhouse, and Lever;
- read-back errors contain no prepared candidate values.

Visual evidence:

```text
/tmp/round586-settings-desktop.png
/tmp/round586-settings-mobile.png
/tmp/round586-matches-desktop.png
```

## Release Boundary

This round does not deploy production and does not change:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

The native overlay, audio, STT, meeting runtime, main dashboard, Caddy,
production services, and signed native release are untouched.

This is the authority and form-integrity layer required before certified
runner distribution. It does not claim that unattended production submission
is enabled.
