# FIX-627: Keep Synthetic ATS Layouts Shadow-Only

> **Codex preflight:** Loaded `$bluey-ops` and checked the current Round 604
> manifest-to-activation boundary locally.

## Issue

A manifest with production-class evidence could still move beyond shadow when
one of its bound layout observations was synthetic.

## Root Cause

Activation admission counted production evidence objects but did not separately
count synthetic layout observations referenced by the immutable manifest.

## Fix Summary

- Count manifest-bound synthetic layout observations in both SQLite and
  PostgreSQL activation checks.
- Reject canary/general activation when any synthetic layout is present.
- Preserve shadow activation for validator and fixture rehearsal.
- Add focused activation-boundary coverage.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/ats_certification_authority.rs` | Bind synthetic-layout count into activation admission for both dialects and test the boundary. |
| `docs/work/FIX-627-jobs-ats-synthetic-layout-activation.md` | Record the shadow-authority defect and repair. |

## Edge Cases Handled

- Production evidence cannot launder a synthetic layout into canary/general.
- A mixed real/synthetic layout set remains shadow-only.
- Shadow activation still exercises signed-object validation without gaining
  employer-facing authority.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  synthetic_layout_observation_is_shadow_only_and_cannot_activate_production
cargo test --manifest-path server/Cargo.toml synthetic_evidence
```

## Known Limitations

- Synthetic fixtures remain local validator evidence only and never represent
  authorized provider certification.
