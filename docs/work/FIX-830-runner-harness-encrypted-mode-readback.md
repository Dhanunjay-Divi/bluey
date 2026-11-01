# FIX-830: Protocol harness casts encrypted application storage as JSON

## Issue

Run65097 received real managed claim200/1256ms, then the new mode-readback psql
failed with22P02. This is a private harness failure, not a failed product claim.

## Root Cause

The harness cast application_json toJSONB, assuming the column held plaintext.
Production jobs.rs to_json encrypts payloads; parse_json decrypts them, and
parse_application_json also binds authoritative application/job IDs. The column
is opaque encrypted TEXT, not a database JSON document. No production encryption
or storage-format change is needed.

## Fix Summary

Reuse the existing post_queue fixture's exact account/application/job read and
production parse_application_json inside its database-enforced READ ONLY snapshot.
Require review_first and return only the observed closed submission_mode string.
The Node driver validates this field, removes the invalid SQL cast, and immediately
after claim proves the immutable-family snapshot still equals its preclaim value.
That set includes jobs_applications, binding the decrypted mode proof to unchanged
stored ciphertext. The final lifecycle immutable check remains too. Do not export
keys, application/resume text, ciphertext, or any other newly decrypted fields.

## Files Modified

- server/src/db/jobs/runner_control_plane_fixture.rs (debug/integration fixture)
- jobs/scripts/nonprod-runner-control-plane.mjs
- Phase638 evidence records.

## Edge Cases Handled

Non-review-first or changed application state fails closed. Existing strict result
shape rejects missing/extra mode fields. Normal production parsing failures remain
failures; no lossy JSON, default mode, direct decryption in SQL, or guard relaxation.

## How to Test

65 Node report/diagnostic/cleanup tests pass, zero skipped; scoped fixture formatting
passes. Independent source review and the actual private API rerun are pending at
record creation. The earlier native/TS proof-ID tests remain their dated passes.

Independent implementation review then cleared the exact read-only/result-shape/
immutable-proof boundary. Actual80446 exited0: API/fixture build1m32s; ordinary
review_first queue and real managed client completed prepared claim200, observed
heartbeat with same token/fence and extended expiry, safe release204, fresh signed
terminal replay204 with unchanged14-family snapshot, explicit total/exact volume
binding and three ATS-zero scopes/no alias. Nine immutable families and the
attempt binding stayed unchanged, and observed submit/evidence/irreversible counts
werezero. One owned gateway503/no second post>=11s is not Temporal acceptance.
Strict report/wrapper validation passed, three recorded child exits wereclean,
PIDsabsent/listenersrefused; API/PG/root cleanup passed, parentempty/38GiBfree.
774-source manifest6b6fccb221db48534ee54dea6632666f9d4a83732805481f8b21c4e61ea70c6b
and HEAD/status matched before/after. API SHA
a4126a85eee633faae25e0219f0a429422833e86593f225fbef48c6e34bfa867;
fixture SHA83e43de57ad232ab4e1892d8c21adf323e80c4e36534f810db6c212ec67d90b8.
This qualifies only the bounded review-first control-plane lifecycle.

Run65097 exit1: build1m29s, claim200/1256ms, private PG22P02/invalid_input,
APIcleanuptrue/PGexited/rootremoved/parentempty; source774 manifest
a7a3035d746dc2f7c473d494a8f97ff81a40b22ae018b7751293369d4abd08b3
and HEAD/status matched. API SHA9774fed6ea20a364fbce2f79fa16911a6af912c2b2ef6a323af684dac98d0d1e;
fixture SHAa8f5530ce4736282a2df47bc3f8d92c5fa7f811e5deb8033b28bb290a57b71b1.
The exact private log tail hash is retained in the tool evidence; no native error
message or stored payload was disclosed.

## Known Limitations

This review-first pass does not establish auto-submit ATS coverage, full negative
matrix, native cloud execution, employer submission or production/source readiness.
