# Round 630 — Jobs local runtime qualification

Date: 2026-10-27. Base: `edeb6a76`. Branch:
`feat/phase-630-jobs-runtime-qualification`.
Status: bounded local qualification complete. Temporal 10/10, real browser/API/PG
story 1/1 and all five Jobs workspace gates passed; not production release approval.
Preflight: `bluey-ops`, verification, performance-analysis, security-audit,
agent-toolkit, swarm-orchestration and SPARC; current code/local handoff authority.

## Story and scope

The candidate's approved command flows through the workflow gateway into a
durable workflow, pauses for an exact intervention, and returns a terminal
result without duplicating work after retries or restart. Phase 629 proved
local API/data and mocked workflow behavior; this round adds actual local
Temporal transport, persistence and history evidence. Injected activities are
synthetic and perform no API/ATS/provider action. This is not hosted-service,
production identity, live runner capacity, or actual employer submission proof.

The additional browser story uses a fixture access JWT and one synthetic
verified/enrolled account: portal → real Jobs API plus the shared account API →
private PostgreSQL → persisted profile after reload. Both HTTP binaries are
isolated fixtures, not the desktop host/overlay. Resume objects, login/OAuth and
external providers remain outside this proof. Ephemeral test signing keys are
generated only in memory; no production signing key is created or reused.

No push, workflow dispatch, registry publication, production signing-secret creation,
deployment, paid infrastructure, production flag, real application, email,
calendar action, or host/overlay/Pinky change is authorized by this test pass.
Do not use a default/owner database or production credentials. Preserve the
untracked inherited canonical Phase 628 REVIEW and frozen `docs/reviews/`.

## Acceptance

1. Pin and checksum the official Temporal CLI in a fresh canonical test root;
   use a child-only empty credential home and allowlisted environment. No
   global install or existing service is used. All endpoints bind loopback.
2. Real gateway HTTP authentication/validation must reject malformed/private
   bodies before creating a workflow. Reconcile-only cannot create missing work.
3. Fresh/concurrent/exact replay and changed authority use the actual Temporal
   SDK and production workflow/gateway code, with observed first-run identity
   and synthetic activity counts rather than mocked SDK assertions.
4. Exercise intervention Update identity, worker/service restart with the same
   owned persistent database, terminal uncertainty, real failure-history
   redaction and exact owned workflow cleanup. Never reinterpret a timeout as
   permission to replay an employer effect.
5. Measure a bounded local gateway/workflow latency baseline, reporting sample
   count, errors and p50/p95/max without claiming hosted production performance.
6. Fail at the first broken boundary. Review bounded corrections and rerun the
   affected checks. Stop on credentials/resource/authority requirements rather
   than silently spending or enabling production.
7. Cleanly stop owned children before automatic exact-root removal; verify
   residue, unchanged product source, privacy and separate review at closeout.

## Ownership

Parent: CLI admission/lifecycle wrapper, documentation and execution. Existing
Sol worker: one standalone runtime integration file, synthetic activity doubles
only. Separate reviewer: read-only source/evidence review. Shared `mac-build`
queue governs all heavy work; no competing agent starts services or builds.

Hosted exact-tip CI needs explicit push/dispatch permission. Hosted PostgreSQL,
Temporal/TLS and provider integrations need separately identified nonproduction
resources and credentials. Passing a local dev service cannot close those gates.
