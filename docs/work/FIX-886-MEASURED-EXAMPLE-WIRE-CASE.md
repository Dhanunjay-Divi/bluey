# FIX-886: Use the existing runtime-attestation wire case in the example fixture

> **Codex preflight:** `$bluey-ops`; current source/local evidence govern.

## Issue

Actual FIX884 Rust run2230 exited1 in the new example schema case. Formatting,
launcher self-test, strict both-feature all-target Clippy and11 focused library
cases passed. The example passed3 and failed1; the full server suite was not
reached. Journal33a668069d3018dc2c6dee971d049f6140f220588d057bcab4e994463ffc5856.
Exact source780/9c4ca5 and index were unchanged; canonical root/parent/PIDs and
related-process cleanup passed. This attempt remains failed.

## Root Cause

The new deserialization-only example test hand-authored six nested
`process_runtime` keys in snake_case. The existing
`RunnerProcessRuntimeAttestation` is camelCase and denies unknown fields.
Deserialization correctly refused `automation_bundle_sha256` before the new
expected-D1 assertions could run. The private startup envelope is snake_case;
the nested public attestation has its own unchanged wire contract.

## Fix Summary

Correct only that synthetic example's six field spellings to the existing
camelCase contract. Add an explicit refusal for an extra snake_case attestation
field. No production type, serde attribute, policy, grant or admission check is
weakened. Parent cross-checked every nested envelope against the current types
and the already-passing library request fixture.

## Files Modified

| File | Change |
|------|--------|
| `server/examples/jobs-nonprod-runner-control-plane-fixture.rs` | Correct test-only keys; explicit wrong-case denial |

## Edge Cases Handled

- Existing camelCase runtime attestation remains the only accepted wire form.
- The new expected-D1 field family still requires all keys and denies extras.
- Private envelope naming is not incorrectly propagated into nested protocols.

## How to Test

Post-fix formatting/whitespace and independent source/controller review passed.
Fresh exact-source run12973 TERMINATED0: launcher self-test, fmt, strict
both-feature all-target Clippy,11+4 focused and1948 full all-target server tests
passed, zero failed/ignored. Focused repetitions are not added to the full count.
Source/module/index equality and exact root/parent/process cleanup passed.
JournalSHAed22415d42a0572fefea58f9f0b5c19c82eddcf297b56c0a563bc7d5705a1bf6.
No historical pass is transferred and physical PostgreSQL remains uncredited.
New source780 manifest2aa70a44581c40841a759665713ff75d7a3a9a9870117d1c88ff52b6bf578a37;
exampleSHA4b954a9affe35fd4f8ea7fd83df834c1e13ae3e10c501d273cbce90e86838e3a.
The separate unchanged fixture library remainsd45390aa38d82fe1290ecf7ce35309c6adb69ecd5947feb14c300d69c7ac1339.
Use the canonical launcher/shared machine lock and reviewed mechanical
controller successor. The full minimum1948 and exact11+4 names do not shrink.

## Known Limitations

No actual D1 artifact, normal measured startup, physical PostgreSQL, production
or release proof follows from this correction. No production data, flag,
service, application/email, CI dispatch or paid resource changed.
