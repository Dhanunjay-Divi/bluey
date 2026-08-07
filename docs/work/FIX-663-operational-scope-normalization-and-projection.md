# FIX-663: Derive Only Canonical, Existing Operational Scope Authority

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, live database, provider, or production
> system was used.

## Issue

Equivalent Unicode regions and internationalized employer domains could map to
different operational-hold keys. Unknown-like locations or inconsistent
posting projections could also become misleading scope context. Unvalidated
caller-built context dimensions, company display names treated as domains,
non-ATS global feed families treated as ATS providers, or a row-backed target
deleted while a PostgreSQL hold was being created could manufacture false
confidence about which work was actually stopped.

## Root Cause

Categorical ASCII normalization was too broad for human regions, Unicode input
was not normalized to one composition form, and verified URL parsing produced
an ASCII IDNA host while an administrator could create the same domain hold in
Unicode. Context loading also needed to prove that denormalized posting columns
still matched the encrypted/canonical posting projection. Context insertion
normalized values but did not apply the same closed preemptive scope validator
as mutation. Row-backed target checks did not retain a key-share lock, and the
global feed family was accepted as an ATS dimension without first proving it
was one of the supported ATS providers.

## Fix Summary

Region and employer-domain values now use lowercase NFC normalization. A
verified canonical employer domain contributes both normalized Unicode and
ASCII IDNA forms when they differ, so either exact form reaches the same
admission context. Unknown-like regions are omitted. Application and job
context loaders compare canonical URL, company, location, and source columns
with the parsed posting before deriving scopes, and frozen ATS provider context
must agree with the provider derived from server-owned target evidence.

Every context insertion now passes the same closed scope validator as a hold
mutation. Employer scope comes only from verified canonical employer-domain
evidence, never the company label or ATS host. PostgreSQL row-backed account,
Career Track, and direct/global discovery-source targets are checked under
`FOR KEY SHARE` in the hold transaction. Global discovery always includes its
exact source and `jobhive` provider, but adds `sourceFamily` as an ATS provider
only when that family is a recognized ATS; for example, `greenhouse` is added
and `remoteok` is not.

## Files Modified

| File | Change |
|------|--------|
| `server/Cargo.toml` and `server/Cargo.lock` | Add the explicit Unicode-normalization dependency. |
| `server/src/db/jobs/operational_holds.rs` | Add NFC/IDN normalization, closed context validation, target locking, and fail-closed posting/ATS projections. |
| `server/src/db/jobs/discovery.rs` | Reuse the bounded region projection for discovery admission. |
| `server/src/db/jobs/global_discovery.rs` | Distinguish typed ATS families from non-ATS feed families. |

## Edge Cases Handled

- Decomposed and precomposed `München` match the same region hold.
- Unicode and punycode forms of a verified IDN employer domain both match.
- A trailing DNS root dot is removed before verified-domain projection.
- `unknown`, `n/a`, `na`, `not specified`, and `unspecified` do not create a
  region scope.
- A posting-column mismatch or malformed/mismatched frozen ATS context denies
  admission as storage/context failure.
- Unknown ATS, runner, mailbox, adapter, model-provider, model, employer-domain,
  or email-shaped region context fails closed before evaluation.
- A company display name never becomes an employer-domain hold scope.
- A non-ATS `remoteok` feed remains leasable under an unrelated ATS-family hold,
  while a Greenhouse family receives the exact typed ATS scope.
- A nonexistent row-backed target is rejected, and PostgreSQL deletion cannot
  race past target validation before the hold transaction commits.
- Non-ASCII regions such as `São Paulo` and `東京` remain valid.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml unicode_region_scope_normalizes_and_matches_context
cargo test --manifest-path server/Cargo.toml \
  unicode_employer_domain_hold_matches_verified_idn_context
cargo test --manifest-path server/Cargo.toml \
  malformed_or_mismatched_frozen_ats_context_fails_closed
cargo test --manifest-path server/Cargo.toml \
  discovery_track_operational_context_omits_unknown_region_sentinels
cargo test --manifest-path server/Cargo.toml \
  operational_context_rejects_unknown_or_malformed_authority_dimensions
cargo test --manifest-path server/Cargo.toml \
  application_context_uses_employer_identity_and_exact_ats_authority
cargo test --manifest-path server/Cargo.toml \
  global_discovery_non_ats_family_remains_leaseable_and_typed_ats_family_is_scoped
```

## Known Limitations

- IDN aliases require the posting's verified canonical employer-domain
  evidence; source logic does not manufacture that evidence.
- Live provider data and PostgreSQL target-deletion/concurrency behavior remain
  external gates.
