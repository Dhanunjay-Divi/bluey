# Round 639 — Real isolated managed-runner startup toward browser/receipt proof

Date: 2026-11-01. Base: `2b85339a403cb5272a56f1b2f0bf8065ca3b19ed`.
Branch: `feat/phase-639-jobs-managed-runtime-startup`.
Status: D0 disposable guest compatibility/isolation/cleanup qualified on exact
source; D1 foundations tested. Actual image and D2/D3 managed-runtime acceptance
remain open.

## Authority and objective

Owner approved the annotated disposable local VM request: approximately4 CPUs/
6 GiB RAM, no paid resources, production data, real applications or emails.
Preserve Jobs-only isolation and inherited untracked REVIEW628. Do not touch
meeting/desktop/Pinky resources, push, merge, deploy, publish externally, change
production flags or use owner account/provider credentials. Current local code
and handoff remain authority; no SSD lookup is needed.

Phase638 is a verified local checkpoint, not a release. Exact candidate tree
`d0dc4eed2c0b58fe1cc8c52b15a1790d3c46e25d`,96 explicitly owned paths, indexed
privacy/whitespace pass; only inherited REVIEW628 remained untracked. See its
handoff/review and durable journals for test scopes. No push occurred.

Reach real normal runner startup/native volume authority, then join the existing
runner/browser/receipt chain. Do not replace this outcome with extracted-core
proof or synthetic runtime labels. `DECISION-PHASE-638-BROWSER-RECEIPT-BOUNDARY.md`
selects a closed test-only transport facade, not weaker production routing.

## Ordered gates

### D0 — Isolated Linux/x86_64 compatibility and cleanup

- Use shared mac-build lock, canonical launcher/private HOME and a separately
  owned short VM-state root. Install bounded signal/stop/cleanup handlers first.
- New private Colima/Lima/Docker state only. No old HQ/Pinky/default profile,
  host mount, SSH-agent/config forwarding, owner Docker credentials or context
  mutation. Resources4 CPUs/6 GiB RAM/8 GiB root+16 GiB data; host free>=32 GiB.
- Preflight installed Colima0.10.3/Lima2.2.1/QEMU11.1.2 CLI options. The target is
  a new x86_64 QEMU guest, not a relabeled ARM64 VZ guest. Observe guest/kernel,
  engine architecture, resources, mounts and owned host listeners. Only known
  loopback management listeners may appear; no published service ports.
- Prove stopped VM inventory and recorded PID absence before deleting exact
  owned disks. Unknown shutdown/identity retains state and fails explicitly.
- A compatibility-only gate may run first to avoid a long build on an invalid
  guest. It does not complete startup or the product. Report emulation explicitly;
  never infer native-x86 performance, hardware or production capacity proof.

### D1 — Actual image, browser and runtime evidence

- Immutable HEAD-bound admitted build context; current Dockerfile/lockfiles and
  actual version3 config/migration/protocol contract hashes. No zero or invented
  authority measurements, no context containing secrets or unrelated files.
- Build/inspect the actual Linux/amd64 runner artifact; distinguish local image
  config ID from OCI manifest/registry digest rather than substituting one.
- Read actual embedded runtime measurement, automation bundle, Playwright and
  Chromium executable/version evidence through bounded private output files.
- Preserve the declared builder mode; no unreviewed global plugin/tool install
  or silent fallback. Build/network prerequisites may fetch official pinned
  dependencies; runtime browser/provider traffic stays isolated and synthetic.

### D2 — Fresh authority and normal startup

- Guarded integration-test-support fixture only; consume exact private measured
  descriptors and publish a staging-only private authority matching the actual
  runner component. No production scope, public seed route or direct trust SQL.
- Issue fresh UNCONSUMED managed-runtime, volume-admission and process-runtime
  grants. Do not import the old fixture volume seed or pre-enroll/claim on the
  runner's behalf. The normal runner generates and signs its own native identity.
- Start actual runner server via normal entrypoint, with real native storage,
  current storage attestation and isolated API/PostgreSQL. Run within one owned
  guest/container network; no host-published service ports. Loopback-only HTTP
  control-plane clients may share the API container's network namespace.
- First acceptance: real health readiness plus API/PG binding readback; zero
  runs, residencies, applications and effects. Startup cannot prove registration
  of a profile that has not yet been requested.

### D3 — Normal profile/run and synthetic provider receipt

- Ordinary synthetic profile/source/packet approval/queue must create the real
  run/session/attempt. Normal claim must register exactly one managed-v2 residency.
- Use the approved test-only transport facade for exact synthetic HTTPS provider
  requests, with no real continuation or employer transport. Do not alter the
  normal network guard's continuation semantics or fake managed profile paths.
- Explicit-review final-submit proof schema3 and receipt schema1 are distinct.
  Observe actual readback, one exact POST/confirmation, managed artifacts/result,
  workflow transport and transactional API/PG receipt finalization/replay.
- Drift/stale authority =>zero POST; lost response after one POST =>unknown/no
  retry; missing/mixed confirmation never submitted; tampered receipt authority/
  content/replay rejects without canonical mutation. No auto/schema4 inference.

## Review, ownership and completion

Parent owns plan, integration, native commands, builds/services, source freezes,
durable evidence and actual cleanup. VM builder owns only its explicitly assigned
new private harness and tests. Existing Sol specialists supply startup authority
contracts and independent source/procedure review. Workers are not alone; do not
revert others' changes or run builds/services/VMs/commits independently.

Run narrow pure tests before any VM dispatch, complete indexed privacy/whitespace
including new files, and independently review exact command/source. Freeze every
admitted source through terminal and final identity/cleanup. Record failures as
failures, not fallback successes. No guessed evidence from intent or static code.
The broader Jobs/C2C/MCP/messaging/source/billing/cost/latency/release goal remains
active until its full completion ledger is proven; this round does not redefine it.
