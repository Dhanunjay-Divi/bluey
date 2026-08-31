# FIX-783: Managed-Cloud Candidate Still Emitted Release V1

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the defect against the current Phase 611
> release workflow, Phase 614 source verifier, Phase 614B integrity authority, and the exact branch
> state. The SSD archive was not used.

**Status:** Source accepted; three independent-review P1 corrections applied, lightweight source
verification green, and correction rereview complete; exact-tip hosted evidence remains pending

## Issue

The current source contained the complete release-v2 source-verification validator and worker, but
the only executable managed-cloud candidate workflow still generated release-v1 contracts, emitted
`sourceVerification=false`, and measured the workflows image for gateway/worker roles only. A
future candidate run therefore could not contain the exact verifier authority already implemented.

## Root Cause

Phase 614 added v2 as a successor without changing the Phase 611 production candidate path. That
was correct while source verification was deliberately parked, but the handoff never advanced the
workflow and Docker measurement defaults after the verifier and Phase 614B integrity authority were
completed. Existing v2 unit fixtures proved the validator in isolation, not the checked-in release
workflow configuration.

Independent review then found three executable contradictions:

- every security-critical release script used ambient runner `node`;
- workflow dispatch could select a non-default source ref; and
- OCI inspection still unconditionally rejected every source-verifier-named path, including the
  exact v2 entrypoint required by candidate assembly.

## Fix Summary

- Generate managed-cloud contracts with explicit version 2.
- Emit descriptor audience/version 2 with `sourceVerification=true` and direct/global discovery
  false.
- Measure the workflows image for the exact sorted source-verifier/gateway/worker role set in both
  Docker stages and the candidate build invocation.
- Keep candidate assembly and isolated verification responsible for the exact v2 protocol,
  entrypoint, runtime-identity, migration-head, and stored-byte relationships.
- Require source-verifier readiness in v2 activation evidence and prove omission fails closed.
- Add two independent structural guards that reject v1 fallback, role omission, or discovery
  enablement.
- Install the full-SHA-pinned existing setup-node action with exact Node 22.23.2 in all five jobs,
  assert the interpreter before the first evidence Node command, and mutation-test omission, wrong
  patch, and late placement.
- Require the repository default branch at job scope for candidate, verify, authorize, promote,
  and rollback; mutation-test omission and inverted comparison.
- Narrow OCI admission to the one exact nonempty regular verifier entrypoint for `jobs-workflows`;
  reject renamed/alias paths and symmetrically reject verifier bytes from release-v1 assembly.
- Document the exact separate verifier process and per-role grant/configuration boundary without
  changing any checked-in flag.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/jobs-managed-cloud-release.yml` | Build and assemble the exact v2 candidate |
| `jobs/workflows/Dockerfile` | Default both image stages to the exact v2 workflows role set |
| `jobs/scripts/managed-cloud-release-gate.mjs` | Enforce the checked-in v2 workflow contract |
| `jobs/scripts/managed-cloud-release-gate.test.mjs` | Cover v2 measurement, canary/readback, and workflow mutations |
| `jobs/scripts/ci-guards-self-test.mjs` | Independently pin workflow and Docker v2 bindings |
| `ops/bluey-jobs.env.example` | Document separate verifier runtime settings and no standalone flag |
| `jobs/OPERATIONS.md` | Add the release-v2 verifier runbook and stop conditions |
| Round/IMPL/REVIEW/CHANGELOG | Record the bounded authority and evidence |

## Edge Cases Handled

- release v1 with source verification true is still invalid;
- release v2 with source verification false is still invalid;
- removing the exact verifier capability, protocol, measured role, compiled entrypoint, or readiness
  check fails closed;
- enabling direct or global discovery remains invalid;
- the verifier uses a distinct one-time role grant and cannot configure its measured identity;
- a missing/stale runtime heartbeat or unsuccessful dependency poll cannot supply readiness; and
- rollback remains a higher-sequence transition over stored verified bytes, never a rebuild.

## How To Test

```text
node --test jobs/scripts/managed-cloud-release-gate.test.mjs
node jobs/scripts/managed-cloud-release-gate.mjs workflow \
  --file .github/workflows/jobs-managed-cloud-release.yml
node jobs/scripts/ci-guards-self-test.mjs
npm test --workspace @bluey/jobs-workflows -- original-source-verification-runtime.test.ts
npm test --workspace @bluey/jobs-automation -- original-source-verification.test.ts \
  managed-cloud-runtime.test.ts managed-cloud-runtime-client.test.ts
npm run typecheck --workspace @bluey/jobs-automation
npm run typecheck --workspace @bluey/jobs-workflows
```

## Observed Evidence

On 2026-08-31 the managed-cloud gate passed 20/20, the checked-in workflow contract and independent
Jobs CI guard passed, the automation verifier/runtime suites passed 40/40, and the workflows
verifier lifecycle suite passed 9/9. Both affected package builds and typechecks passed. An
explicit v2 contract generation resolved SQLite head 057, PostgreSQL head 035, and
`source_verification` protocol v1.

The focused Rust rerun was stopped during compilation without a test failure when the shared host
fell to 18 GiB free (96% used) while an unrelated Phase 623 build was active. This fix does not
change Rust or migration source and relies on the retained reviewed Phase 614 authority until an
exact-tip resource-capable CI rerun supplies fresh Rust evidence.

The originating release owner separately configured and read back the build-only Actions variable
`BLUEY_JOBS_NODE_IMAGE` as
`node:22.23.2-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5`.
That closes the missing image-input prerequisite only; no candidate execution or deployment is
claimed.

Independent correction rereview found no remaining P0-P3 issue. It re-ran the final 20-test gate,
workflow contract, CI guard, YAML parse, and diff checks against the corrected source.

## Known Limitations

- This fix prepares a future release-v2 candidate; it does not run the workflow or create a signed
  manifest/activation.
- It reuses the reviewed paired 057/035 runtime and lease authority; it does not implement V3
  direct/global discovery.
- Docker/Linux, hosted database/Temporal, provider, signing, monitoring, cohort, and rollback
  evidence remain external release gates.
- Fresh focused Rust evidence remains pending because the local capacity stop was honored.
- Exact-tip Actions execution must still prove setup-node acquisition and default-branch behavior;
  local structural evidence does not substitute for a protected run.
- All production/provider-write flags remain off and no external effect is authorized.
