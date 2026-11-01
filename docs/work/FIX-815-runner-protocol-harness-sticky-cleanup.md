# FIX-815: Runner protocol harness must retain cleanup failure

> Codex preflight: bluey-ops, current AGENTS and Round638; Jobs-only checkout.

## Issue

Independent pre-runtime review found a false-cleanup-success path in the new
Phase638 reproduction harness. No runtime or production failure is claimed.

## Root Cause

The initial stop function set a boolean before awaiting shutdown. If that shutdown
rejected, its error handler called stop again, received an immediate successful
return, and could report cleanup:true. The boolean remembered invocation rather
than the actual outcome. The initial report also used an overly broad noMutation
label even though worker authentication intentionally reserves a replay nonce.

## Fix Summary

Cache one cleanup promise before execution and return it to every caller. Both
success and failure are sticky; the second caller cannot erase a failed outcome.
Require actual clean owned process exits, listener close and loopback connection
refusal. Report only protectedClaimStateUnchanged across14 explicit DB families,
including durable commands/request-start authority, rather than all system state.
Worker-auth replay-guard reservations are explicitly outside that contract.

## Files Modified

- jobs/scripts/nonprod-runner-control-plane.mjs: single-flight cleanup and precise
  protected state contract.
- jobs/scripts/nonprod-runner-control-plane.test.mjs: repeated success/failure,
  residual process/listener, omitted family and exaggerated evidence rejection.

## Edge Cases Handled

- Concurrent/repeated callers share the same promise and execute shutdown once.
- A rejected cleanup remains rejected after the initial caller handles it.
- PID inspection uncertainty or forced shutdown cannot qualify cleanup.
- Missing HTTP, unrelated errors or synthetic authority cannot qualify a causal red.

## How to Test

```sh
env -u NODE_OPTIONS -u NODE_PATH node --test jobs/scripts/nonprod-runner-control-plane.test.mjs
```

Observed27 passed, zero failed/skipped, command6a41d8 exit0. Actual process shutdown
and ordinary API/PG reproduction are still unexecuted and not proved by unit tests.

## Known Limitations

This fixes the test harness, not the suspected execution-run/browser-session
binding bug. That production fix requires an independently reviewed actual causal
red first. No deployment, browser, Temporal or employer effect was run.
