# FIX-884: Admit the independently selected D1 source and tool closure

> **Codex preflight:** Load `$bluey-ops`; current code and local handoff are
> authoritative. Jobs only. No SSD lookup is needed.

## Issue

The debug/integration-only measured-startup fixture pins the old Phase639 D1
controller and source. It would reject a later correctly qualified Phase646
artifact before ordinary runner startup. No positive actual D1 is claimed yet.

## Root Cause

`validate_measured_d1_controller` embedded one historical controller digest,
commit, branch, file count and source manifest. Several tool hashes were checked
only for cross-record consistency rather than against an independent parent
admission. Replacing constants with report-derived values would remove the
intended provenance boundary instead of fixing the stale selection.

## Fix Summary

The fixture now requires a separate closed `MeasuredRunnerD1Expected` tuple.
Its thirteen fields select the reviewed commit/branch/count/manifest, source
archive, controller, harness, VM test, artifact helper, inspector, and distinct
image-tool and inspection-tool aggregates. Canonical bounded source fields and
nonzero lowercase digests are validated before metadata use. The example's
closed `MeasuredStartupFiles` request requires `expected_d1` separately from
artifact reports and passes it explicitly; reports never construct it.

The tuple is a trusted private test-controller input, not a signature or actual
execution proof. Existing exact artifacts, external terminal zero, stage order,
cleanup, trust/release inputs and no-preclaim/no-effect boundaries remain.
Production inputs/routes and ordinary authorization are not changed.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/runner_control_plane_fixture.rs` | Independent expectation validation/bindings and synthetic regression cases |
| `server/examples/jobs-nonprod-runner-control-plane-fixture.rs` | Required closed expected input, early validation, caller and schema test |

## Edge Cases Handled

- Missing, null, unknown, malformed, zero or oversized expected fields refuse.
- A coherent rewritten report cannot substitute for the independent tuple.
- Current-shaped and historical-shaped synthetic evidence cannot cross replay.
- Nonzero terminal, altered cleanup/source, unknown stage and reordered progress
  remain denials even when the surrounding journal hashes are rebound.
- Image-tool aggregate and the smaller inspection-tool aggregate are separate
  domains, not interchangeable digests.

## How to Test

Six new cases were authored and passed in corrected actual run12973: the five
`measured_startup_expected_d1_*` library tests and the example's
`measured_startup_files_require_separate_closed_expected_d1_admission` test.
Existing focused cases require explicit expectations. Parent executed the
reviewed canonical/shared-lock gate:11+4 focused and1948 full all-target cases,
zero failed/ignored, strict both-feature Clippy, fmt, source equality and cleanup.
IMPL/REVIEW641 record the exact current source and journal. No worker execution
or actual managed runner startup is credited.

Worker whitespace check passed. Parent independently computed the current C
three-file inspection aggregate from exact source bytes as
`ca8c1b226dd7490a08f131996f5f2faabd5e9c28c6457c0eee94c2a2659b7993`.
This is source admission, not D1 qualification. Unit fixtures intentionally use
a labeled synthetic inspection aggregate and contain no actual image bytes.

## Known Limitations

Independent source rereview and corrected Rust gates passed, no remaining
P0-P3 source finding. Historical1942 tests apply
only to the earlier exact778-file source; they do not qualify this successor.
The initial review held two P2 current-controller bindings: journal selfSha256
and explicit D1-only terminal semantics. Both are now corrected against the
current controller shape, including24 rebound-hash negative vectors. This is
bounded source/test acceptance, not measured runtime qualification.
Actual Phase646 D1 terminated1 on frozen14d56e77. Phase647 source gates passed,
but its retained predecessor archive remains failed; no positive package is
assumed. Normal measured startup/D2, hosted integrations, ATS
effects, costs and release remain unqualified. No production mutation, push,
deployment, application or email occurred in this slice.
