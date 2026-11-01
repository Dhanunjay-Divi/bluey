# FIX-822: Ordinary cloud-run ID is not its reusable browser-session ID

> Preflight: bluey-ops, current AGENTS, Round638 and independent causal review.

## Issue

Actual ordinary route/real-client reproduction79181 passed all prior prerequisites
and reached exact target404 with no protected-state mutation. It deliberately
does not qualify source, workflow execution or employer submission.

## Root Cause

The normal API creates a40-hex execution run ID and a distinct reusable browser
session cloud-applicationID. Durable command material and workflow/runner transport
preserve both. Active execution target lookup instead selects session.id=run_id
and payload validation requires that equality; ATS PhaseA independently substitutes
run ID. Existing direct fixtures that manually align IDs masked this ordinary seam.

## Fix Summary

Implemented locally, runtime qualification pending. Resolve the exact session from
durable account/application/run START authority and bind the current START/RESUME
request chain when supplied; cardinality and frozen payloads must be exact. Carry
the same ID into target payload validation and ATS PhaseA. No broad OR lookup,
current/latest-session selection, predicted run IDs, alias rows or history rewrite.
Do not move active command/session dependencies into retained heartbeat, finish
or submitted receipt replay. Existing terminal authorities remain authoritative.

## Files Modified

server/src/db/jobs/execution_session_authority.rs contains the active SQLite/PG
transaction resolvers, pure invariants and frozen-application identity bridge.
execution_leases.rs carries exact session IDs through target validation, both PG
child locks and ATS PhaseA at five active consumers. jobs.rs wires private helper
and DB-test modules. tests.rs preserves the legacy fixture's aligned IDs but now
derives real frozen workflow fields and canonical request/result IDs instead of
placeholder material. The separate DB-test file exercises SQLite command lifecycle.
The Node driver/controller/adapter add --prepared-lifecycle with real client claim,
observed heartbeat, safe release and fresh signed HTTP terminal replay (unrun).

## Edge Cases Handled

Required: distinct IDs, session reuse, exact command/request chain, missing/wrong/
ambiguous/pre-start authority rejection, account/application/run/profile mismatch,
exact ATS binding, unchanged terminal replay under command/policy advancement.

## How to Test

Reproduction79181 exit0 is a verified causal red:1 real signed HTTP404 claim,
readiness/normal queue replay,1 loopback503/no retry>=11s,14 protected families
unchanged, distinct IDs/noalias,3 clean child exits/PIDs absent/listeners refused,
source772 manifest8758dc34013ed91b323dc72c87bc8b4ba0370b47fe14d395455257455fa401d3
andHEAD/status equal, private PG/root cleanup complete. No positive claim yet.

## Known Limitations

Binding correction is implemented, not runtime-qualified. The prepared lifecycle
driver and its54 report/cleanup/privacy tests are distinct evidence: only the
report tests have passed. Actual PG positive lifecycle, broader negative/concurrency
matrix and full Jobs/server/source gates remain pending. Review separately found
retained checkpoint recovery still assumes session.id=run_id; leave its historical
authority unchanged until a bounded frozen-session fix is designed. No production
release or external effect.
