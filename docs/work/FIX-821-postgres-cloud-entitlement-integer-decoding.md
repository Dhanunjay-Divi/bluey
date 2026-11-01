# FIX-821: Cloud queue panics when INTEGER entitlement is decoded as Boolean

> Preflight: bluey-ops, current AGENTS, security review and Round638; private PG.

## Issue

Actual ordinary API queue run96006 lost its connection in251ms with UND_ERR_SOCKET.
Private redacted API diagnostics locate a PostgreSQL row-type panic at
workflow_commands.rs:2317. No lease claim or session-ID causal red was reached.

## Root Cause

Authoritative PostgreSQL002_jobs.sql:159 defines cloud_browser INTEGER(INT4).
The existing normal entitlement reader correctly decodesi32/nonzero. Five effect
consumers instead call Row::get<bool>, which panics rather than returning a typed
storage error: queue/resume, managed effect admission and two lease operations.
This is a server availability/release blocker, not an authority bypass.

## Fix Summary

Use one shared private named-column Row::try_get<i32> decoder and preserve !=0
semantics. Replace all five wrong consumers and centralize the already-correct
normal reader. Keep local_browser unchanged. Optional missing rows remain false;
decode errors follow existing typed storage/unavailable conversions. No Boolean
schema migration, SQL cast, type fallback, lock/order/authority/replay change.

## Files Modified

- server/src/db/jobs/applications.rs: decoder, normal reader, one configured-PG
  schema/typed-row regression.
- server/src/db/jobs/workflow_commands.rs: queue and resume consumers.
- server/src/db/jobs/managed_cloud_release_authority.rs: effect admission consumer.
- server/src/db/jobs/execution_leases.rs: two entitlement consumers only, not the
  still-unobserved session-binding correction.

## Edge Cases Handled

Required actual-PG evidence: migrated column INT4,0 false,1/other nonzero true,
BOOL/INT8/missing-column Err without unwind. Actual ordinary queue and real-client
claim remain separately required; decoder tests alone cannot qualify that journey.

## How to Test

Regression implementation/qualification pending at record creation. Run configured
PG with BLUEY_JOBS_REQUIRE_POSTGRES_TESTS=1 in the canonical private harness; no
owner DB/credential, destructive reset, browser or provider action is authorized.

Implementation is now frozen for review in the four listed modules. Exact filter:
db::jobs::queue_admission_tests::postgres_cloud_browser_integer_decoder_is_fallible_when_configured.
The migrated schema assertion and0/1/2/-1 plus BOOL/INT8/missing-column no-unwind
cases are implemented. Scoped formatting/whitespace pass; runtime remains pending.

Focused run37153 exit0 passed9/9 (decoder PG, cutover PG, helper2, example4,
SQLite1), zero skipped. Decoder asserted actual migratedINT4,0/1/2/-1 and error/
no-unwind cases. Source772 manifest8758dc34013ed91b323dc72c87bc8b4ba0370b47fe14d395455257455fa401d3
andHEAD/status matched before/after; PG/root cleanup complete. Ordinary API queue/
real claim and full-source qualification are still pending.

Ordinary run79181 then passed queue/replay and reached exactly1 real signed lease
claim, receiving the separate intended target404 rather than a row-type panic.
It passed strict causal-red/state/readiness/cleanup checks, with unchanged772
manifest8758dc34013ed91b323dc72c87bc8b4ba0370b47fe14d395455257455fa401d3.
This closes the prerequisite's ordinary queue/claim blocker, not the independent
session-binding defect or full source/release qualification.

## Known Limitations

No production deployment or positive lease lifecycle yet. The intended distinct-
ID target404, its binding fix, full Jobs/server gates and release remain pending.
