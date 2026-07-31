# Round 582: Jobs global candidate cold storage and PostgreSQL CPU

Date: 2026-07-31

## Outcome

Bluey Jobs did not have a primary-key duplicate explosion. It had a more
expensive write-amplification problem: the same canonical jobs were rewritten
on repeated signed-manifest refreshes, and expired heavy candidate bodies never
left PostgreSQL.

This round makes global ingestion content-addressed and adds a default-off,
verified R2 cold-storage lifecycle. PostgreSQL remains the live authority.

## Production Evidence

The read-only production audit associated the DigitalOcean CPU alert with
global discovery ingestion:

- approximately 139,000 canonical candidate rows;
- approximately 22 million candidate updates;
- approximately 22 million membership updates;
- the same immutable source revisions completing repeatedly;
- expired candidates retaining full normalized bodies.

The alert resolved without a process restart. This round changes source
semantics and storage lifecycle rather than masking the workload with a larger
database.

## Root Causes

1. Signed-manifest snapshot metadata changed during refresh and was treated as
   a new artifact revision, resetting `next_run_at_ms`.
2. Candidate hashes covered randomized ciphertext instead of deterministic
   plaintext, so identical jobs appeared changed.
3. Expiration removed active membership but never archived the heavy encrypted
   candidate body.

## Implemented Contract

### Ingestion

- Only source family, artifact SHA-256, or expected row count changes schedule
  an immediate global re-ingestion.
- URL and snapshot metadata may refresh without resetting a completed run.
- Candidate content hashes are SHA-256 over normalized plaintext JSON.
- An unchanged hot candidate is a database no-op.

### Cold archive

A candidate may be archived only when it is:

- expired;
- older than the configured retention window;
- absent from active source memberships;
- absent from account materializations;
- leased exclusively by the current archive worker.

The object contains the already encrypted candidate envelope. The worker:

1. serializes a versioned archive envelope;
2. writes it to a content-addressed private R2 key;
3. reads the object back;
4. verifies exact bytes and SHA-256;
5. transactionally records archive evidence and replaces the heavy body with a
   small encrypted tombstone.

Any failure leaves the complete PostgreSQL body in place. Rediscovery restores
the complete hot row and clears archive state.

## PostgreSQL And R2 Boundary

PostgreSQL keeps:

- canonical job IDs and searchable fields;
- active/expired state and source memberships;
- account materializations, applications, identities, Career Tracks, and
  metering;
- claim provenance, receipt references, hashes, and authorization state.

R2 keeps:

- original uploaded resumes;
- rendered resume/cover-letter/application documents;
- screenshots and confirmation evidence;
- immutable receipt artifacts;
- encrypted cold global-candidate bodies.

Moving all historical application rows to R2 now would break authorization,
interview-prep, export, and deletion queries. That requires a separate hydrated
terminal-bundle design. It is intentionally not hidden inside this database CPU
fix.

## Resume And Interview Improvement Boundary

Historical artifacts may ground private, same-account resume improvement,
application comparison, and interview preparation. This is retrieval and
evaluation for the owning account.

Bluey must not silently pool resumes, answers, screenshots, or interview
artifacts across accounts for model training. Any cross-customer training
program requires explicit opt-in, de-identification, a versioned dataset
manifest, access auditing, deletion propagation, and reviewed Terms and Privacy
disclosures.

## Rollout

The worker defaults to disabled. Production activation requires:

- migrated API candidate;
- least-privilege R2 credentials;
- test-object PUT/GET/exact-read-back proof;
- eligibility-query review;
- small canary batch;
- database CPU/dead-tuple/source-freshness/retry monitoring;
- independent PostgreSQL backup and restore proof.

Disabling the worker stops new archives. Existing tombstones remain searchable
and rediscovery remains the supported rehydration path.

## Verification

Focused lifecycle tests cover:

- unchanged source scheduling;
- unchanged candidate no-op writes;
- verified archive completion;
- read-back mismatch preservation;
- active membership and account materialization guards;
- corrupt legacy-row isolation;
- legacy deterministic hashing;
- rediscovery rehydration.

Full command results are recorded in
`docs/work/REVIEW-JOBS-GLOBAL-CANDIDATE-ARCHIVE.md`.

The final pass completed:

- 876 server tests with zero failures;
- strict server Clippy with warnings denied;
- Rust formatting and diff whitespace checks;
- Jobs SQLite/PostgreSQL schema parity;
- Jobs privacy and client/server boundary checks;
- Jobs dependency, license, and source-provenance checks.

The repository's informational SQLite-boundary audit still reports 53
pre-existing mixed-backend references. This round does not claim the broader
PostgreSQL-only migration is complete.
