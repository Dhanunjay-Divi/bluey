# FIX-743: Jobs Workflow Resume Current Integrity

**Severity:** P2 stale internal state

**Status:** 🟡 Implemented in start/resume wiring; stale-authority zero-mutation case pending

## Issue

Fresh workflow resume could approve an intervention and stage a command without current composed
execution authority. A later request-start denial prevented the external effect but still allowed
stale internal state mutation.

## Required Fix

- Resolve current composed authority before intervention approval or resume-command admission.
- Reuse the canonical prelock without reacquiring managed/discovery locks.
- Prove stale authority leaves intervention and command state unchanged.

## Implementation

`server/src/db/jobs/workflow_commands.rs` reuses caller-owned current execution authority before a
fresh workflow start or resume stages commands, and its PostgreSQL after-prelock helpers do not
reacquire managed/discovery authority. `server/src/api/jobs.rs` routes approval, queue, and cloud
resume through the current composed application authority.

## Evidence

Mapped coverage:

```text
fresh_workflow_effects_require_current_composed_authority_before_mutation PRESENT (STATIC)
fresh_start_and_resume_use_post_lock_database_time_for_temporal_effects  PRESENT (STATIC)
approval_queue_and_cloud_resume_use_current_composed_authority           PRESENT (STATIC)
Stale-authority intervention/command zero-mutation behavioral regression PENDING
Frozen-source aggregate execution                                        PENDING
```

Static order/source coverage is not a substitute for the missing stale-authority behavioral case,
so this FIX remains yellow.
