# Round 574 - Jobs Career Track Authority And Normalization

Date: 2026-07-25

Status: implementation and local verification complete; feature branch only

## Objective

Turn Career Tracks from editable search labels into server-authoritative
application identities. A track now means:

```text
canonical role family
+ selected locations and workplace policy
+ verified application identity
+ exact source resume asset
+ relevant non-overlapping experience
+ employment, engagement, and authorization policy
+ immutable evidence revision
```

This closes the candidate-truth gate before Bluey can safely certify
employer-facing runners.

## Product Behavior

### Role normalization

Common aliases map to full canonical roles. Examples include:

- SDE and SWE to Software Engineer;
- DE to Data Engineer;
- PM to Product Manager;
- TPM to Technical Program Manager;
- CRA to Clinical Research Associate;
- CRC to Clinical Research Coordinator.

Users select a full role name or enter an explicit custom role. A parsed
employer/title string is not accepted as a target role merely because it came
from a resume.

### Experience policy

Bluey computes role-family experience from relevant employment intervals:

- unrelated roles do not count;
- overlapping jobs are counted once;
- explicit employment selections cannot cross the Career Track role family;
- the search window is one year below through two years above the candidate's
  relevant experience;
- required years and preferred years remain separate;
- senior, staff, lead, principal, manager, director, and executive titles use
  an independent title-seniority guard.

Tailoring may rewrite and emphasize supported skills and outcomes. It does not
invent employers, dates, titles, degrees, or years of experience.

### Hard filters

One server-owned eligibility decision evaluates:

- active Career Track;
- verified application identity;
- exact current source resume;
- role family;
- job availability and freshness;
- healthy discovery authority;
- location and workplace policy;
- salary floor;
- employment and engagement type;
- work authorization and sponsorship;
- excluded companies and titles;
- duplicate-company protection;
- daily reservations;
- ATS capability.

Unknown hard-filter facts fail closed into Review or a hard stop instead of
being guessed.

### Evidence and execution

Each evidence revision has a stable hash. Each supported resume claim points
to evidence IDs. PostgreSQL rejects updates to frozen evidence revisions and
claim-evidence rows.

Application packet finalization and employer-facing execution recheck:

- account;
- Career Track;
- identity;
- source resume;
- resume version;
- evidence revision;
- claim IDs;
- job;
- eligibility;
- discovery health;
- execution capability.

Receipts preserve the exact resume and identity used.

## Portal

Onboarding and Settings now:

- explain that location is a hard filter;
- provide complete bounded location suggestions with retries;
- normalize role aliases to full names;
- support Full-time, Part-time, Contract, Temporary, Internship,
  Apprenticeship, Seasonal, and Per diem;
- support W-2, C2C, 1099, and Direct hire contract engagement;
- present compact skills and certifications editors;
- show server-managed application policy rather than arbitrary user-facing
  daily pace and Auto-submit threshold fields;
- warn when a track still points to an old resume revision.

PDF and DOCX import tests cover malformed combined employer, title, and
location layouts in addition to narrative-boundary protection. The parser also
normalizes Word private-use bullets, spaced section headings, ordinary hyphen
employment headers, multiword cities, certification link suffixes, and wrapped
certification names without stripping meaningful colon-qualified titles.

Server matching canonicalizes raw role aliases before role-family evaluation.
Skill comparison uses token boundaries and known aliases for technologies such
as Node.js, React, PostgreSQL, AWS, GCP, Azure, Kubernetes, and REST APIs.
Short skills such as Go no longer match inside unrelated words.

## Bundle Boundary

The portal runtime is split from React and routing dependencies. Document
parsers remain dynamic imports. The lazy DOCX parser is permitted a 550 kB
on-demand budget; it is not part of the startup path. Source maps remain off.

## Verification

```text
Server full test command
  passed
  799 unit tests
  76 integration tests
  all focused test binaries

Rust formatting
  passed

Rust strict Clippy
  passed with -D warnings

Portal
  13 test files
  87 tests passed
  typecheck passed
  production build passed without warnings

Full Jobs workspace
  automation: 188 tests
  Browser: 100 tests
  runner: 50 tests
  workflows: 51 tests
  portal: 87 tests
  476 tests passed
  complete workspace typecheck passed

Repository gates
  Jobs privacy gate passed
  Jobs schema parity passed
  provenance and license checks passed
  client/server boundary check passed
  server SQLite boundary check completed with the existing 43-line migration warning
  no new SQLite access was added outside server/src/db
  no production source maps

Responsive visual QA
  Settings desktop, dark and light
  Settings mobile
  Onboarding desktop
  Onboarding mobile
  Career Track goals mobile
```

No horizontal overflow, clipped text, overlapping controls, or stale internal
policy fields were observed.

The shared `bluey-ops` operating skill was updated and validated. Active Bluey
agent onboarding, handoff, operations, deployment, release, Jobs, and work
documentation templates now require that skill as a preflight while keeping
repository code and current runbooks authoritative.

## Release Boundary

This round does not deploy production and does not change:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

The native overlay, audio, STT, meeting runtime, Caddy, main API, signed native
release, and production services are untouched.

The branch is ready for review under the repository's feature-branch workflow.
