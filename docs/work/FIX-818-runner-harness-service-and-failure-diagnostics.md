# FIX-818: Reap bundler service and preserve useful closed fixture diagnostics

> Preflight: bluey-ops, current AGENTS, Round638; isolated Jobs-only fixture.

## Issue

Run73022 reached bootstrap but failed with a generic closed code. After printing
its failure the Node controller retained an esbuild child handle and stayed live.
It required owner-process-tree-verified termination before PG/canonical cleanup.

## Root Cause

The controller imported esbuild's persistent service without explicitly stopping
it after bundle completion. Fixture errors were collapsed into bootstrap_lifecycle,
making the failed prerequisite invisible. Cleanup also treated an already-reaped
failed fixture as a cleanup failure, conflating the primary failure with residue.

## Fix Summary

Stop the esbuild service in finally after every bundle attempt. Keep complete
real-client import validation and source checks. In shutdown, require reaped
children and clean API exit; success reports separately require zero fixture exits.
An already-failed fixture never turns a failed test into a passed test. Add exact
allowlisted bootstrap stage diagnostics and a pure Rust redaction test; underlying
DB/key/candidate error strings are never emitted.

## Files Modified

- jobs/scripts/run-nonprod-runner-control-plane-tests.mjs: esbuild service finally.
- jobs/scripts/nonprod-runner-control-plane.mjs: primary failure versus cleanup.
- server/src/db/jobs/runner_control_plane_fixture.rs: static diagnostic contexts.
- server/examples/jobs-nonprod-runner-control-plane-fixture.rs: closed-code mapping
  and a test proving arbitrary payloads cannot escape.

## Edge Cases Handled

Bundler failure must also stop service. Sticky cleanup rejection stays intact;
forced shutdown still fails. Unknown fixture errors remain bootstrap_lifecycle.
Detailed diagnostics cannot change fixture authority or qualify the causal red.

## How to Test

Run the reviewed causal-red recipe and verify controller/esbuild/PG/root exits
without manual intervention. Syntax/scoped rustfmt/diff pass584b6e; actual corrected
shutdown and stage reproduction remain unverified at record creation.

## Known Limitations

Bootstrap's actual rejected prerequisite is still unknown. No product claim or
production fix has run; positive lifecycle and all release gates remain pending.
