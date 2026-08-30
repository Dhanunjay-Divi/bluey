# FIX-749: Jobs SmartRecruiters Cross-Host Integrity Binding

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Severity:** P1 provider-compatibility blocker

**Status:** Implemented; focused and aggregate evidence pending

## Issue

A valid SmartRecruiters Phase 614 source binds provider host `jobs.smartrecruiters.com` and exact
application host `www.smartrecruiters.com`. Phase 614B rejected every such signed attestation and
expected-source resolution by requiring those two independently bound hosts to be equal.

## Root Cause

`validate_job_integrity_attestation` and `job_integrity_expected_source_valid` treated provider
host and application domain as aliases. That assumption holds for some providers but contradicts
the existing provider-specific SmartRecruiters source contract. The canonical application URL was
already required to match the exact application domain, and later source comparison independently
binds provider host and application domain, so the equality added no valid integrity guarantee.

## Fix Summary

- Remove only the provider-host/application-domain equality requirement from attestation and
  expected-source validation.
- Retain strict domain validation and exact HTTPS application-URL-to-application-domain binding.
- Retain exact independent comparison of provider host, application domain, canonical URL, target,
  record identity, and source material during current-authority resolution.
- Add a signed SmartRecruiters positive lifecycle regression and a composition regression that
  preserve the two distinct hosts.
- Prove provider-host drift remains a typed mismatch rather than weakening either binding.

## Files Modified

| File                                                         | Change                                      |
| ------------------------------------------------------------ | ------------------------------------------- |
| `server/src/db/jobs/job_integrity_authority.rs`               | Permit valid cross-host authority bindings  |
| `server/src/db/jobs/job_integrity_composition.rs`             | Pin exact cross-host composition semantics  |
| `docs/work/FIX-749-jobs-smartrecruiters-cross-host-integrity-binding.md` | Record defect and evidence ledger |

## Edge Cases Handled

- Provider host and application domain may differ without becoming interchangeable.
- The application URL must still use the exact signed application domain.
- A caller that substitutes the application domain for the provider host receives a mismatch.
- Other provider, tenant, job, variant, material, and ATS-target bindings remain exact.

## How To Test

```text
smartrecruiters_cross_host_authority_resolves_with_independent_exact_bindings     PENDING
smartrecruiters_composition_preserves_distinct_provider_and_application_hosts     PENDING
Full signed-integrity, composition, schema, and release gates                     PENDING
```

## Known Limitations

- This fix consumes existing signed/source authority; it does not add or operate a
  SmartRecruiters adapter, credential, login, fetch, or provider write.
