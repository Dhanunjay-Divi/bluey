# FIX868 — Confine measured startup artifacts to the guarded run root

## Issue / root cause

Initial Phase641 source accepted any absolute canonical same-user artifact
directory. That unnecessarily widened private fixture file reads beyond its
verified launcher workspace. Independent static review caught it before execution.

## Fix / files

`server/examples/jobs-nonprod-runner-control-plane-fixture.rs` now accepts exactly
the derived `runner-measured-artifacts` child of the guarded root. Caller aliases,
outside/sibling/nested paths and symlinks refuse. Reads use the derived path,
validate private ownership/regular files and revalidate current directory/file
identities around reads and immediately before ordinary issuance.

Original export device/inode fields remain provenance for the original export;
copied guest inputs receive fresh local identities with exact content hashes.
No claim of inode equivalence across transport is made. Late interference or DB
failure retains partial private state; there is no atomic-filesystem/DB claim.

## Verification / limitations

Source-authored tests cover exact child acceptance, outside/sibling/nested/relative
and dot/parent aliases, and replacement of a same-root directory inode. The
first gate32702 failed and remains failed. Corrected actual run12973 subsequently
passed focused11+4 and full1948 all-target tests, zero failed/ignored, formatting,
strict all-target Clippy, exact source equality and cleanup. Current tuple and
journal are recorded in IMPL/REVIEW641. No positive actual D1/startup input exists.

This remains debug/integration-only fixture code. No production route, provider,
owner database, application/email, deployment or paid resource is involved.
