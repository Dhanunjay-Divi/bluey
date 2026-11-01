# FIX-816: Resolve stored workflow binding before gateway validation

> Preflight: bluey-ops, current AGENTS, Round638; private Jobs-only testing.

## Issue

First Phase638 build74589 terminated101 with Rust E0308 before API execution.
This was a new fixture compilation failure, not the intended product causal red.

## Root Cause

The post-queue fixture passed ManagedCloudStoredWorkflowBinding directly to
validate_managed_cloud_gateway_authority, which requires its validated/resolved
ManagedCloudWorkflowBinding. The two types represent different trust boundaries.

## Fix Summary

Use the existing require_exact_managed_cloud_workflow_binding conversion with
the actual command/account/application/run/workflow IDs and fixed staging scope.
It checks subject HMACs, canonical memo bytes/hash and binding digest before
the existing gateway validator runs. Do not cast, reconstruct a trusted type
manually, weaken validation or mutate any production row.

## Files Modified

- server/src/db/jobs/runner_control_plane_fixture.rs: exact validated conversion.

## Edge Cases Handled

Mismatched/corrupt stored bindings continue to fail through existing validation;
the READ ONLY transaction, distinct session/run IDs and no-preclaim checks remain.

## How to Test

Run COMMAND-PHASE-638-CAUSAL-RED.md after independent delta review and privacy scan.
Scoped rustfmt/diff pass c16d8e; corrected build/runtime remains unverified here.

## Known Limitations

No production execution binding has been changed. Actual causal red, positive
lease lifecycle, Rust fixture tests and broader release gates remain pending.
