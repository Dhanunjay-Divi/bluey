# FIX-721: Original-Source Managed-Authority Loss Was Misclassified As Storage Failure

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the current Phase 614
> lease scheduler, private verifier API error contract, and focused lifecycle evidence. The SSD
> archive was not used.

**Status:** Implemented; focused lifecycle, fmt, all-target check, and strict Clippy green

## Issue

Assignment-authority rechecks converted managed-registry expiry or revocation into the generic
`Storage` variant. The lease still failed closed, but operators and the private API could not
distinguish expected managed-authority loss from a database or integrity failure.

## Root Cause

Both SQLite and PostgreSQL assignment-authority helpers applied the generic storage-error adapter
to every `ManagedCloudRegistryError`. That erased the closed `Unavailable`/`Revoked` operational
classification before the scheduler or API boundary could handle it.

## Fix Summary

- Map only managed-registry `Unavailable` and `Revoked` to
  `ManagedRuntimeAuthorityUnavailable`.
- Preserve every other managed-registry error as `Storage`; invalid authority, identity conflict,
  expired grants returned outside the closed lookup contract, and genuine storage/integrity faults
  are not relabeled as ordinary unavailability.
- Apply the mapper to both SQLite and PostgreSQL assignment-authority rechecks.
- Prove heartbeat expiry and grant revocation supersede the assignment with
  `managed_authority_revoked`, create no attempt or receipt, and return the typed unavailable error
  from the public lease boundary.

## Files Modified

| File                                                                               | Change                                                                                      |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `server/src/db/jobs/original_source_verification.rs`                               | Add the narrow managed-authority error adapter and focused SQLite/paired-source regressions |
| `server/src/db/jobs/managed_cloud_release_authority.rs`                            | Add a canonical long-horizon verifier runtime fixture for heavy parallel tests only         |
| `docs/work/FIX-721-jobs-original-source-managed-authority-error-classification.md` | Record the operational-classification defect, correction, and evidence boundary             |
| Phase 614 Round/IMPL/REVIEW/CHANGELOG                                              | Include the correction without promoting hosted runtime evidence                            |

## Edge Cases Handled

- Expired heartbeat and revoked runtime grant share the typed unavailable classification.
- Both paths persist `superseded:managed_authority_revoked` and mint no attempt or receipt.
- Invalid authority, identity conflict, and unexpected grant-expired errors remain storage/integrity
  failures rather than being hidden as availability events.
- The PostgreSQL source path uses the same narrow adapter; live PostgreSQL behavior remains
  unclaimed.

## How To Test

```text
Typed assignment-authority classification regression       1 / 1 (4.82s final-source rerun)
Original-source verification module, normal parallel       25 / 25 in four owner/reviewer runs
Public SQLite lifecycle/replay subset                        6 / 6 owner and reviewer
Rust all-target check and strict Clippy                     passed
```

The long-horizon verifier authority used by the heavy scheduler/lifecycle regressions is test-only.
It keeps parallel wall-clock load from expiring unrelated fixture authority and does not change a
production TTL, activation, or runtime policy.

## Known Limitations

- The pre-fix path already denied authority; this correction restores deterministic operational
  classification rather than broadening access.
- Live PostgreSQL expiry/revocation propagation and hosted monitoring remain external evidence.
