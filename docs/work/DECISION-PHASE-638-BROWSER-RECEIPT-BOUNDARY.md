# Phase 638 exit decision: normal runner to browser to receipt

Date: 2026-11-01. Read-only source decision with two independent native-agent
assessments. No product implementation or runtime acceptance follows from it.
Current source: 774/9b1ee, HEAD40949858, scoped status932ee. The complete product
goal and completion ledger remain unchanged and unfinished.

## What already exists, and what is not yet proved

The production chain exists in source: `ExecutionLeaseClient.claim`, heartbeat,
irreversible authorization and finish; normal runner `executeRun`; the actual
Playwright page and provider state machines; receipt/evidence construction;
workflow receipt transport; and API/PostgreSQL receipt finalization. Existing
protocol and standalone browser proofs do not join that chain.

Primary seams:

- `jobs/runner/src/execution-lease.ts`: claim and retained lease authority.
- `jobs/runner/src/server.ts`: normal launch around2327, `executeRun` around2367,
  profile registration around3400 and tracked write residency around3525.
- `jobs/runner/src/runner-volume-client.ts`: native profile/runtime/volume binding.
- `jobs/automation/src/providers/{greenhouse,lever}.ts`: exact form readback,
  one-shot submit and provider-bound confirmation/unknown outcomes.
- `jobs/workflows/src/activities.ts`: committed result readback and receipt POST.
- `server/src/api/jobs.rs`: worker receipt and immutable storage readback.
- `server/src/db/jobs/execution_leases.rs`: retained receipt/lease authority.

The current private runner fixture explicitly constructs synthetic, unmeasured
Linux/x86_64 descriptors. It cannot establish native startup, mounted-volume
identity, tracked profile residency or the normal runner result store. Its
keys and paths must not be imported into a nominally real runner to skip that
boundary. The native Mac API fixture remains useful protocol evidence only.

## Competing fixture transport designs

### A — change the production network guard to fall through

Replace `route.continue()` with `route.fallback()` so another route can serve a
synthetic employer page after URL validation. This is feasible but not selected:
it changes production routing composition solely for testing. A subsequent
handler could modify or fulfill traffic after the original check, adding an
unneeded security/review surface. Extracting the whole `executeRun` routine into
a generalized executor would also couple or bypass server-private residency,
checkpoint, artifact and lease state. It is not a small pure refactor.

### B — retain production guards and use a closed test-only transport facade

`installBrowserNetworkGuard(context, validate)` already accepts a context-shaped
object and an injected validator. A private fixture can register the real guard
on a real Playwright context while giving its callback a bounded Route facade:
real request/abort, but approved `continue()` performs only an exact fixture
`fulfill()`. Real network continuation must never be called. Later exact-submit
handlers already fall through to earlier guards; no production routing change
is needed for that composition.

Both assessments found B feasible and preferable. Required fixture controls:

- Exact synthetic HTTPS origin, paths, GET/POST methods and bounded payloads.
- One-shot facade continuation only after the injected validator succeeds.
- Credentials, malformed/private/non-target URLs and unrecognized methods fail
  before fixture fulfillment. Redirects and subresources cannot escape scope.
- Actual reserved non-target request abortion and WebSocket closure observed.
- No production switch, default change, public seed endpoint or general executor
  bypass. Any internal construction seam retains normal production defaults.

This is fixture transport isolation, not proof of production DNS lookup,
DNS rebinding protection, network-namespace egress, TLS or employer transport.
Existing production URL/DNS checks remain unchanged and separately tested.

## Dependency decision: real managed startup comes first

Do not substitute a convenient extracted-core test for the required joined
runner proof. `executeRun` enters tracked profile residency immediately. Normal
registration and `withProfileWriteResidency` require the actual active lease,
native root/device/path bindings, volume identity and matching managed-v2 profile.
Therefore the next full integration must enter through normal runner lifecycle
and HTTP orchestration, not invoke an extracted function with invented paths.

The current managed release target set names Linux/x86_64 API/runner/workflows.
The shared Mac Linux worker is ARM64; its documented amd64 emulation is not native
amd64 performance or hardware evidence. Do not relax this release target or
relabel the old ARM64 construction probe. Runtime compatibility, actual measured
artifact identity and representative production performance are distinct gates.

The owner subsequently approved the disposable local VM via the annotated
permission question on November1: approximately4 CPUs/6 GiB RAM, no production
data/real effects/paid hosted resources. No VM has started yet; a bounded successor
phase and exact preflight remain required. If that resource cannot provide the
required runtime/storage authority, retain the hold rather than manufacture it.
Other independent code/product gaps may continue.

## Acceptance for the successor, not an existing green result

1. Admit the exact isolated platform/artifact and normal measured runtime;
   provision volume/profile residency through normal guarded lifecycle.
2. Ordinary private candidate/profile/source/packet approval and cloud queue
   create the real run/session/attempt. No post-bootstrap trusted SQL rewrite.
3. Normal runner claim/register/launch uses real native managed storage and B's
   test-only transport. No fixture signing-key import or fake storage object.
4. Explicit review-first only: final-submit proof schema3, review receipt schema1.
   These are different artifacts; do not call either a schema3 receipt. No
   reusable auto-submit/schema4 authority is inferred.
5. Actual provider readback, exact POST and confirmation create the real managed
   artifacts/result. Workflow receipt transport and API/PG finalization must
   validate hashes/lease authority and preserve exact receipt replay.
6. Drift or stale authority produces zero POST. Response loss after the one POST
   becomes unknown with no retry. Missing/mixed confirmation cannot become
   submitted. Tampered receipt authority/content rejects without canonical change.
7. All artifact, listener, process, VM and temporary-root cleanup must be observed;
   retain immutable bounded non-secret evidence. No real employer/provider/mail
   transport, production flag, publication, deployment or paid resource.

Passing an intermediate compatibility/core test does not finish this acceptance,
the fully autonomous platform, or the broader C2C/MCP/messaging/source/release work.
