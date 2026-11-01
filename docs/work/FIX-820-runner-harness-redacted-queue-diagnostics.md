# FIX-820: Retain actionable private queue failure metadata without payloads

> Preflight: bluey-ops, current AGENTS, Round638; controlled private cluster only.

## Issue

Run81361 passed the nonempty fleet fixture and ordinary factual packet approval,
then failed at queue with closed_stage_failure. The logger correctly hid raw
errors but also discarded the error type, exact local call site and request
timing, preventing an evidence-backed distinction among schema/transport/timeout.

## Root Cause

The failure report retained only a coarse stage and sanitized message. Native
errors intentionally fail that text allowlist, so the useful non-payload metadata
was lost. No underlying queue/product root cause is claimed yet.

## Fix Summary

Add queue substages, allowlisted error types/system codes, numeric owned-source
call-site line, method/status/elapsed-time metadata. Only internally branded
gate errors and labeled owned assertions may expose their fixed diagnostic code;
arbitrary native messages (including alphanumeric strings) remain redacted.
Before failure shutdown, optionally read at most32 private PG activity records
with PID/state/wait-event/blocker PIDs. No SQL text, bind values, paths, tokens,
connection strings, resume/contact content or raw server errors are emitted.

## Files Modified

- jobs/scripts/nonprod-runner-control-plane.mjs: bounded redacted diagnostics only.
- jobs/scripts/nonprod-runner-control-plane.test.mjs: private-text/type redaction.

## Edge Cases Handled

Unknown/native messages and codes remain closed. Metadata collection failure
cannot mask the primary error or prevent independent cleanup. Passing causal-red
reports still require exact HTTP failure/state/cleanup; diagnostics never qualify
the cause by themselves. Request timeout and every production rule stay unchanged.

## How to Test

30 Node report/cleanup/privacy tests pass, zero skipped (052e42). Repeat the
reviewed isolated causal-red recipe to obtain the actual queue failure metadata.

Observed84038: fetch TypeError at244ms/no headers, with18 sampled idle PG clients
and no blockers; clean automatic cleanup and equal772-source manifest. This is
not a15s timeout, but transport/server cause is still unknown. Further diagnostics
emit only allowlisted nested transport codes and a private bounded API stderr
panic fingerprint(source basename/line, fixed category, tail hash). No raw text.
32 Node diagnostic/privacy tests now pass589971, zero skipped; runtime pending.

## Known Limitations

Underlying queue error and run/session causal red remain unobserved. No product
identity fix, positive lease lifecycle, full source acceptance or release yet.
