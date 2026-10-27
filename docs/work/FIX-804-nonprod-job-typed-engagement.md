# FIX-804: Nonproduction positive job omitted typed W2 evidence

Preflight: `bluey-ops`; current Phase 633 source and actual private runtime.

## Issue

The initial nine-case browser/API/PG journey reached fresh factual re-preparation
but failed `queue_local_allowed` before approval.

## Root Cause

The debug seed helper supplied `employment_type: "full_time"` and mentioned W2
only in description prose. `classify_candidate_category` in `candidate_policy.rs`
uses typed evidence for positive classification; description context can reject
negated/mixed claims, but cannot mint positive engagement authority. The candidate
selected W2/C2C, so W2 engagement remained unverified and queueing correctly failed.
Production discovery joins typed employment and engagement as `full_time w2`;
existing classification positive/negative tests explicitly define this contract.

## Fix Summary

Use that existing typed representation in the synthetic job, before all normal
source/ATS/integrity lifecycle publications. The shared test-only discovery
adapter splits only an exact canonical pair into the normal separate fields;
passing the combined field directly would correctly fail discovery validation.
Plain and malformed inputs remain unchanged for normal validation, and two
focused unit tests cover all 32 canonical pairs and malformed/mixed/space cases.
The helper additionally requires
both typed-category passed checks when positive-queue qualification is requested.
The browser asserts each required classification/policy/source/certification
check before asserting queue authority, and prints only closed assertion labels.
No eligibility rule, profile selection, Track authority or execution guard changes.

## Files Modified

| File | Change |
|---|---|
| `server/examples/jobs-nonprod-browser-fixture.rs` | Correct synthetic typed input; fail early if categories remain unproven |
| `server/src/db/jobs/production_positive_authority_fixture.rs` | Exact canonical-pair test adapter and two focused regressions; absent from default production builds |
| `jobs/scripts/run-nonprod-browser-api-tests.mjs` | Include the changed shared fixture in admitted source hashes |
| `jobs/portal/nonprod/browser-api.integration.mjs` | Exact job selector, required passed checks and privacy-safe failure stages/labels |

## Edge Cases Handled

- Missing, mixed or negated engagement still cannot queue under the unchanged classifier.
- Source/certification signatures bind the corrected job from the start; no stored trust mutation.
- A fresh factual packet must preserve both old immutable history and current source evidence.

## How to Test

Run the Phase 633 canonical private-PG reproduction in its IMPL/handoff record:
default API builds, feature-gated helper build/five guards/strict Clippy, then
three focused fixture-module units, the existing category regression, and the
actual nine-case journey. Frozen Run3/session89002 passed all of these with
clean shutdown, unchanged source identity and exact-root cleanup. The final
v4 report required all nine cases, not just a successful queue response.

## Known Limitations

This repairs synthetic qualification input, not a production classifier defect.
It does not prove hosted sources, real ATS execution, subscription or deployment.
