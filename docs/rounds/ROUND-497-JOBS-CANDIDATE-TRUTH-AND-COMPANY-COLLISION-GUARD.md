# Round 497 - Jobs Candidate Truth And Company Collision Guard

Date: 2026-07-11

Repository: `/Users/uno/Downloads/cue-bluey-jobs`

Branch: `codex/bluey-jobs-20260710`

## Decision

Bluey treats one Jobs account as one real candidate.

An application email is an inbox and browser-session identity for that
candidate. It is not a second person, a second Career Profile, or permission to
bypass employer-level limits.

Current policy:

| Situation | Bluey behavior |
| --- | --- |
| SDE application is in progress at Acme, then Data Engineer is found at Acme | Block before creating the second resume |
| SDE application was submitted at Acme, then another role is found | Block before preparation or queueing |
| Second role uses another Career Track | Still blocked |
| Second role uses another verified application email | Still blocked |
| Company appears as `Acme, Inc.` and later `The Acme LLC` | Treat as the same company |
| First attempt failed before an employer-facing side effect and its reservation was released | A corrected retry can proceed |
| A genuinely different person wants to use Bluey | Use a separate Bluey account and separate consent/profile, not an email alias |

Bluey does not currently offer a same-person, multiple-role exception. A future
exception would need an explicit cooldown, user approval, a canonical company,
the same candidate-truth fingerprint, compatible non-contradictory materials,
and Review-only submission. A new email alone will never be enough.

## Why This Is Necessary

Submitting materially different resumes to one employer can:

- create contradictory work histories, titles, dates, education, or projects;
- make a candidate appear deceptive even when the mismatch was accidental;
- cause recruiter and ATS duplicate-profile confusion;
- damage the candidate's reputation more than missing one additional role;
- let aliases or Career Tracks accidentally behave like fake personas.

The safe boundary is the real candidate, not the email address.

## Existing Protection Confirmed

Before this round, Bluey already had:

- tenant/account-scoped application identities;
- identity-scoped browser profiles;
- an atomic `jobs_attempt_reservations` ledger;
- a unique active-company index scoped to the Bluey account;
- `submitted` included as a protected company state;
- exact resume-version and application-identity IDs frozen in receipts.

This meant email aliases did not bypass the database uniqueness constraint.
However, the Settings UI exposed a switch that implied the rule could be
disabled, and resume enhancement could invent a target headline or generic
"experience aligned" summary when the source profile had neither.

## Implementation

### Server-Owned Company Guard

`server/src/db/jobs.rs` now:

- forces `apply_once_per_company` on when preferences are read or saved;
- evaluates every in-progress, uncertain, or submitted company reservation
  regardless of Career Track, resume version, or application email;
- blocks the second packet before resume generation;
- rechecks the same rule atomically at reservation time;
- uses the same policy for SQLite and PostgreSQL;
- keeps released pre-submission failures retryable.

The eligibility reason is:

```text
company_application_exists
```

Its customer-facing message explains that another Track, resume, or email does
not create a second candidate.

### Company Name Normalization

Company keys now ignore:

- capitalization and punctuation;
- a leading `The`;
- common legal suffixes such as `Inc`, `LLC`, `Ltd`, `Limited`, `Corp`, `PLC`,
  `GmbH`, `Pte`, and `Pty`.

This is a conservative bridge until Bluey ships canonical company IDs from
authoritative discovery sources. It does not attempt to infer parent/subsidiary
relationships or mergers.

### No Fabricated Empty Resume Sections

When the Career Profile has no headline or summary:

- the job title is no longer inserted as the candidate's headline;
- Enhance mode leaves the summary empty;
- Bluey does not claim that the candidate has experience aligned to the role.

Employment, education, projects, certifications, and skills still come from
the user's one Career Profile. Skills may be reordered/selected from that
approved set; new skills are not added from the job description.

### Candidate-Truth Fingerprint

Each new resume and prepared receipt now freezes a versioned SHA-256 candidate
truth fingerprint derived from normalized core facts:

- legal candidate name;
- LinkedIn identity;
- employer, title, and employment dates;
- school, degree, field, and education dates;
- project name and role;
- certifications.

The fingerprint deliberately excludes application email, target company, target
role, summary styling, and skill ordering. Changing an email alias therefore
does not change candidate truth, while changing a prior job title does.

Stored fields:

```text
provenance.candidate_truth_fingerprint
provenance.candidate_truth_fingerprint_version
receipt.candidate_truth_fingerprint
receipt.candidate_truth_fingerprint_version
receipt.career_track_id
```

### Portal Copy

`jobs/portal/src/views/SettingsView.tsx` now shows the company rule as
`Locked` instead of a toggle. The application-email section says aliases
isolate inbox and site sessions but do not create separate candidate profiles
or bypass company limits.

`jobs/portal/src/components/Onboarding.tsx` now states that every Career Track
and application email shares one candidate truth.

## Regression Coverage

The focused server suite proves:

1. Two verified emails under one account cannot bypass the company guard.
2. SDE and Data Engineering Career Tracks cannot bypass the guard.
3. `Acme, Inc.` and `The Acme LLC` collide as one company.
4. A caller cannot save preferences with the guard disabled.
5. The second company packet cannot be prepared or auto-submitted.
6. Enhance mode cannot invent an empty headline or summary.
7. Candidate-truth fingerprints ignore email aliases.
8. Candidate-truth fingerprints change when core work history changes.
9. Company collision and daily-attempt limits remain separate atomic checks.

## Remaining Work

Before any controlled multiple-role exception:

1. Add authoritative `canonical_companies` and parent/subsidiary metadata.
2. Persist the fingerprint directly on the attempt ledger for transactional
   comparison and historical migration.
3. Add a user-visible comparison of the prior and proposed role, Track,
   identity, resume, and core facts.
4. Define withdrawal, rejection, and cooldown semantics with recruiting/legal
   review.
5. Require explicit typed confirmation and Review-only submission.
6. Measure duplicate-candidate and employer complaint rates.

Until those exist, one company means one in-progress or submitted Bluey
application per candidate account.

## Verification

- `cargo test jobs::tests`: 26 passed.
- Full server: 305 unit tests and 55 integration tests passed.
- `cargo check --bins` passed.
- `cargo fmt -- --check` passed.
- Portal: 6 tests, TypeScript check, and production build passed.
- Vite's existing chunk-size warning remains; no new build warning was added.
- Desktop Settings visual review passed.
- Mobile 390 x 844 Settings visual and horizontal-overflow checks passed.

## Handoff Prompt

```text
Read ROUND-497-JOBS-CANDIDATE-TRUTH-AND-COMPANY-COLLISION-GUARD.md.

Preserve the invariant that one Bluey Jobs account represents one candidate.
Application emails and Career Tracks must never bypass company collision or
candidate-truth checks. Do not reintroduce a user-disableable company guard or
generate a target headline/experience claim from an empty source profile.

If implementing multiple roles at one company later, first add canonical
companies, a durable cooldown/exception record, transactional truth-fingerprint
comparison, a prior-vs-proposed packet diff, explicit user confirmation, and a
Review-only path. A different email alone is never an exception.
```
