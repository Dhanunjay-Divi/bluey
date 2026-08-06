# FIX-630: Admit a Cloud-Certified Application Lifecycle

> **Codex preflight:** Loaded `$bluey-ops` and reproduced the failure through a
> source-test signed cloud-runtime fixture and runner-volume claim path in the
> active Round 604 worktree.

## Issue

A valid cloud-only ATS certification could not move an application through the
shared `queued` or `running` lifecycle state, even though the server resolver
returned `can_queue_cloud = true` and the exact cloud claim was otherwise valid.

## Root Cause

`server/src/db/jobs/applications.rs::update_application` is a runner-neutral
lifecycle function, but its eligibility guard required `can_queue_local`.
Round 604 correctly intersects certification with an exact runner kind, so a
cloud-only manifest deliberately sets `can_queue_local = false`.

## Fix Summary

- Reject a shared queued/running transition only when both server-authorized
  runner paths are unavailable.
- Preserve exact runner enforcement in queue selection and the local/cloud
  claim APIs; the lifecycle transition does not mint execution or Submit
  authority.
- Exercise the repair through the signed cloud process-runtime, volume,
  certification, Phase A, and Phase B path rather than a database bypass.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/applications.rs` | Make shared lifecycle admission accept either server-authorized runner path. |
| `server/src/db/jobs/tests.rs` | Prove a cloud-only certification reaches the exact cloud claim and remains fenced from local authority. |
| `docs/work/FIX-630-jobs-cloud-certified-lifecycle-admission.md` | Record the blocker and repair. |

## Edge Cases Handled

- Both runner paths false still returns the existing fail-closed eligibility
  error.
- Cloud-only authority does not make `can_queue_local` true; local Phase A
  still rejects it.
- Local-only authority does not make `can_queue_cloud` true; cloud Phase A
  still rejects it.
- Exact runtime, application, attempt, packet, and certification checks remain
  in the selected runner claim transaction.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib certified_cloud_
cargo test --manifest-path server/Cargo.toml --lib \
  active_ats_status_enables_only_certified_runners_and_requires_a_loaded_binding
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- Authenticated fleet and live PostgreSQL concurrency remain external gates.
