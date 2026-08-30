# FIX-726: Employer Domain And Hosted ATS Separation

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Status:** Implemented; mapped focused regressions present; frozen-source aggregate evidence pending

## Issue

A provider/application domain identifies where an application is hosted; it does not independently
identify the employer's corporate domain. Treating a Greenhouse, Lever, Ashby, SmartRecruiters,
Workday, or other shared ATS host as employer-domain proof would let provider presence self-mint
employer identity and weaken the independent signed-integrity boundary.

## Root Cause

Phase 614 intentionally records `application_domain` as part of exact original-source and
destination authority. Before Phase 614B there was no separate signed corporate-domain authority.
Any shared field, fixture, or resolver that reuses the application domain as the employer domain
therefore conflates two facts with different issuers, semantics, and risk.

The historical operational-hold surface made that conflation authorization-relevant. Source
projection aliased the application host into the canonical-employer-domain field, while hold
context builders trusted raw posting JSON. A mutable posting could therefore influence hold scope
without the independently signed corporate-domain authority required by Phase 614B.

## Required Fix

The implemented correction:

- preserve the Phase 614 source `applicationDomain` as an exact provider/destination fact;
- add independently signed `canonicalEmployerId` and `canonicalEmployerDomain` fields under the
  `employer_identity` role;
- normalize corporate and application domains independently with strict ASCII IDNA/host rules;
- require explicit employer verification methods and bounded signed evidence;
- bind the ATS tenant separately through `atsTenantBindingSha256`;
- allow application and employer domains to differ, including shared multi-tenant ATS hosts;
- treat equality as an observed coincidence, never an authorization shortcut;
- reject missing, mismatched, cross-employer, cross-tenant, Unicode-confusable, suffix-host, or
  stale corporate-domain authority;
- require callers to resolve the signed corporate domain after the common authority prelocks and
  pass the resolved value into operational-hold context construction;
- prohibit operational-hold helpers from internally reacquiring authority locks;
- deny malformed or mutable raw-posting employer-domain storage without widening hold scope; and
- keep the account-independent attestation free of candidate identity, PII, and Auto-submit data.

## Files Modified

| File                                                     | Change                                                                    |
| -------------------------------------------------------- | ------------------------------------------------------------------------- |
| `server/src/db/jobs/job_integrity_authority.rs`          | Validate independent signed employer identity/domain and destination      |
| `server/src/db/jobs/job_integrity_composition.rs`        | Compose exact source, ATS target, employer identity, and risk authority    |
| `server/src/db/jobs/operational_holds.rs`                | Require a typed caller-resolved signed employer domain                     |
| `server/src/db/jobs/applications.rs`                     | Feed current signed employer domain into queue/finalization hold contexts  |
| `server/src/db/jobs/execution_authority.rs`              | Derive current/submitted hold scope from authenticated receipts            |
| `server/src/db/jobs/tests.rs`                            | Cover typed employer-domain holds at claim and FinalSubmit boundaries      |

## Edge Cases To Handle

- Many employers share one hosted-ATS domain.
- One employer uses multiple ATS tenants or migrates providers.
- A direct employer application host equals the corporate domain.
- A vanity application subdomain differs from both provider and corporate domains.
- Unicode/IDNA, trailing-dot, port, suffix-host, or userinfo forms attempt identity confusion.
- A valid provider tenant points to the wrong canonical employer.
- Greenhouse hosts an Acme application while the signed corporate domain remains `acme.example`.
- Raw posting JSON mutates the employer domain after authority resolution.
- Employer-domain evidence expires or is revoked while source authority remains current.
- A successor employer identity replaces an earlier verified head; old positive authority cannot
  return through fallback.

## How To Test

Mapped focused evidence in the current source:

```text
smartrecruiters_cross_host_authority_resolves_with_independent_exact_bindings PRESENT
smartrecruiters_composition_preserves_distinct_provider_and_application_hosts PRESENT
expected_source_uses_exact_phase614_binding_and_ats_target_sha256              PRESENT
application_context_uses_employer_identity_and_exact_ats_authority             PRESENT
sqlite_application_context_requires_explicit_signed_employer_domain            PRESENT
signed_domain_after_prelock_context_helpers_do_not_reacquire_authority_locks   PRESENT
signed_employer_domain_requires_canonical_ascii_and_rejects_mutable_idn_storage PRESENT
fix_728_employer_domain_runner_claim_holds_leave_local_and_cloud_unmodified     PRESENT
fix_728_employer_domain_final_submit_holds_leave_local_and_cloud_unmodified     PRESENT
Frozen-source aggregate and configured PostgreSQL gates                        PENDING
```

`PRESENT` means the named regression exists in the mapped source. This documentation-only refresh
does not record a new execution result.

## Known Limitations

- This fix validates imported signed employer authority; it does not crawl, discover, or generate
  employer evidence.
- Public Tsenta/Giraffy or other competitor-facing pages are not employer-identity evidence and are
  outside this fix.
- It does not add a provider adapter, contact an employer, deploy, enable flags, or authorize a
  provider write.
