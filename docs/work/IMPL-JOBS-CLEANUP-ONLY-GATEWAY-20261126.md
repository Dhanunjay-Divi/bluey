# Bluey Jobs: bind the workflow gateway to cleanup-only duties

> Bluey Ops. Local successor; not part of the sealed Phase 663 candidate.
> No deployment, hosted requests, workflow actions, or release approval.

## Why

Disabling dispatch in the Jobs API stops that caller, but the existing
private workflow gateway still installs its command and reconciliation
routes. A cleanup-only canary should not mount either command capability.

## Boundary

`BLUEY_JOBS_WORKFLOW_GATEWAY_MODE=cleanup_only` requires
`BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED=true`, matching approved namespaces,
and an absence of managed runtime configuration on that gateway process.
Invalid mode or prerequisite values refuse before connecting to Temporal
or claiming a managed runtime. The cleanup-only gateway never constructs
a command executor. Authenticated command and reconciliation routes return
404 before reading the request body, and unauthenticated routes return 401.
The cleanup route retains the existing token, schema, authority, size and
readiness checks. Default and explicit `combined` behavior are unchanged.

This setting is gateway-specific. It does not provision a preproduction
namespace, turn on any upstream effect, or authorize a cleanup operation.
The [operator rollout](../../jobs/OPERATIONS.md#phase-610-workflow-cleanup-rollout)
still requires an isolated approved target, exact inventory/cleanup proof,
and an actual synthetic deletion canary with other effect flags off.

## Verification and review

Actual `46348/486c59/d4e98d` exited 0: **369 tests in 14 files**, zero
skips or failures, including 27 gateway cases and the three new
cleanup-only cases. Automation build, Workflows build/typecheck, and
compiled exports passed on pinned Node 22.23.4. The offline closure had
211 lock-integrity-verified dependency archives. The preflight approved
one loopback port, refused another port, and checked the fixture was
actually bound to IPv4 loopback. Test scratch and the listener closed
(`69610a`); launcher self-test passed (`8737/4d7c4c`). No real Temporal
service was contacted.

Earlier attempts `61899`, `26715` and `26877` refused at the sandbox
probe before compilation (unsupported literal hostname filter and a
wildcard-bind negative the symbolic filter does not establish). The final
probe used a symbolic single-port rule, explicit IPv4 fixture readback,
and a denied different-port canary. No product assertion was removed.

Independent source review found no open P0/P1 and requested focused
mode and HTTP coverage, which passed in the run above. Current worktree
projection SHA `68afb036…8e48`, gateway `58aa1480…2bca`, fixture
`309e6ad5…cd52`, test report `8dfdf27d…2ff4`. The
offline controller
is `e879b5a8…366f`. The change has not been packaged, pushed, or qualified
against a hosted namespace. It does not close the release deletion gate.
