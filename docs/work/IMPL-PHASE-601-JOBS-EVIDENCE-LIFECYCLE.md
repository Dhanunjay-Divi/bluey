# IMPL: PHASE-601 - Jobs Evidence Lifecycle and Exact Submit

> **Codex preflight:** Loaded `$bluey-ops`, verified its Jobs authority and
> release boundaries against this worktree, and kept the meeting checkout and
> production environment out of scope.

## Scope

**Does:**

- Binds certified Greenhouse and Lever submission to the exact approved job,
  effective target, ordered successful controls, hidden-field schema, and PDF
  bytes.
- Validates the browser-generated multipart request after isolated-world DOM
  inspection and durable server authorization, then hydrates only file bodies
  omitted by Chromium while preserving its boundary and headers byte-for-byte.
- Prevents unrelated network traffic after applicant data is filled and permits
  only the exact submit plus causal main-frame navigation bound to that same
  provider job; Submitted still requires the explicit confirmation URL.
- Accepts only `2xx` or intended `301`/`302`/`303`/`307`/`308` submit responses
  and treats either a returned unique submit control or ambiguous submit
  controls or negative submission language as a failed confirmation boundary.
- Reserves evidence capacity before an irreversible action and atomically binds
  immutable receipt, document, and one-to-four screenshot objects afterward.
- Adds authenticated, integrity-checked evidence download and portal rendering.
- Makes resume-source and browser-profile objects participate in a durable
  upload/outbox lifecycle and account-deletion fence.
- Serializes account object writers against deletion across PostgreSQL replicas
  without consuming the primary query pool, and blocks deletion when known
  cloud-runner state still requires a future verified purge acknowledgement.
- Requires the artifact and effective audit namespaces to be swept successfully
  before account ownership is removed; missing configuration or storage failure
  preserves the durable fence and returns a retryable service-unavailable result.
- Makes fresh and recovered runner pages offline-first, isolates checkpoint
  corruption by profile, serializes browser-session mutation, and replays exact
  staged submitted results after a lost response.
- Adds paired SQLite/PostgreSQL migrations and broad unit, integration, browser,
  workflow, and portal regression coverage.

**Does NOT:**

- Enable public Greenhouse or Lever final submission, certify a live ATS tenant,
  or widen final-submit capability to another provider.
- Deploy, restart services, run production migrations, mutate R2/S3, access
  secrets, or change production flags.
- Claim live PostgreSQL, physical power-cut, or real-device/browser-distribution
  certification from local source tests.
- Implement or claim cloud-runner volume purge/fan-out acknowledgements; known
  cloud-runner records deliberately keep account deletion at `409 Conflict`.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/automation/src/certified-submit-form.ts` | Created | Isolated-world successful-control and file snapshot. |
| `jobs/automation/src/{effective-submit-target,final-submit-proof,provider-job-key,trusted-submit}.ts` | Created | Exact job, target, proof, multipart, and request validation. |
| `jobs/automation/src/submission-confirmation.ts` | Created | Full-body negative-outcome veto shared by certified adapters. |
| `jobs/automation/src/**/*.ts` | Modified | Certified adapter, document, network, receipt, and execution integration. |
| `jobs/automation/tests/` | Modified | Unit fixtures plus real Chromium request-byte and beacon coverage. |
| `jobs/browser/src/` | Modified | Offline-first local launch, exact recovery, checkpoint, and failure fencing. |
| `jobs/runner/src/` | Modified | Cloud launch guard, durable result recovery, profile isolation, and session serialization. |
| `jobs/workflows/src/` | Modified | Exact durable result recovery and separate canonical receipt persistence. |
| `jobs/portal/src/` | Modified | Complete evidence verification, safe authenticated downloads, and lifecycle UI. |
| `server/src/api/{jobs,jobs_resume_assets,account,sync}.rs` | Modified | Evidence upload/download, replay, resume publication, and deletion APIs. |
| `server/src/db/jobs/` | Modified | Final proof, execution capacity, evidence manifest, profile, resume, and replay transactions. |
| `server/src/db/{account_data,object_uploads,mod}.rs` | Modified | Durable deletion fence, bounded cross-replica lifecycle locks, upload ledger, backfill, and migration registration. |
| `server/src/object_storage.rs` | Modified | Account-prefix listing, bounded reads, and lifecycle cleanup support. |
| `infra/{sqlite,postgres}/server-runtime/` | Added | Dialect-paired deletion, capacity, and object-ledger backfill migrations. |
| `server/tests/` and Jobs workspace tests | Modified | Database, signed HTTP, browser, runner, workflow, portal, and parity coverage. |
| `web/jobs/` | Rebuilt | Checked-in portal production bundle from the verified source build. |
| `CHANGELOG.md` and Round 601 work docs | Modified/created | Release note and auditable implementation record. |

## Build & Test

```bash
cd jobs
npm test                              # 988 tests passed
npm run typecheck                     # five workspaces passed
npm run build                         # five workspaces passed
npm run smoke --workspace @bluey/jobs-automation  # passed

cd ../server
cargo fmt --all -- --check            # passed
cargo clippy --all-targets --all-features -- -D warnings  # passed
cargo check --bin bluey-jobs-api       # passed
cargo test --all-features              # 936 lib + 98 E2E + 6 auxiliary passed

cd ..
node jobs/scripts/ci-guards-self-test.mjs          # passed
node jobs/scripts/check-jobs-schema-parity.mjs     # passed
node jobs/scripts/check-provenance-licenses.mjs    # passed
bash scripts/check-bluey-ops-docs.sh               # passed
python3 scripts/analyze-tracing-calls.py --check-only  # passed after log fix
cargo fmt --all --check                           # passed
cargo clippy --all-targets -- -D warnings         # passed
cargo build --all-targets --release               # passed
cargo test --all-targets                          # passed
git diff --check                                  # passed
```

The privacy guard is rerun against the final staged index because that script
intentionally scans index blobs rather than unstaged working-tree files.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Bluey hydrates only file bodies omitted from the intercepted multipart request. | Chromium preserves the multipart boundary, headers, order, and text bytes but omits selected file bodies; Node verifies those content-addressed files and inserts only the missing bytes before revalidating the complete request. |
| Proof schema v3 carries a global ordered kind/index list in addition to the bounded text and file records. | The authorization now preserves the browser-observed multipart interleaving instead of reconstructing order from separate lists. |
| Account deletion returns `409 Conflict` while known cloud-runner sessions or leases exist. | Runner-local persistent volumes do not yet expose a complete account purge/fan-out/ack protocol, so returning success would overclaim deletion. |
| Directory sync errors that mean the operation is unsupported are tolerated. | Process-crash recovery remains portable; physical power-loss certification is explicitly outside this local batch. |

## Known Follow-ups

- Certify the narrow Greenhouse and Lever DOM and hidden-field allowlists against
  owner-authorized tenants, including optional file inputs and post-submit
  navigation behavior.
- Replay migrations and concurrency cases against an isolated live PostgreSQL
  service and test immutable-object recovery against an isolated R2/S3 bucket.
- Add a signed, multi-runner account-volume purge protocol, reconcile or wipe
  legacy runner volumes, and only then remove the cloud-runner deletion gate.
- Add a durable registry of every historical artifact/audit bucket and prefix
  before supporting live namespace rotation; current deletion proves the two
  configured effective namespaces and fails closed when either is unavailable.
- Run real-device crash and power-loss matrices before claiming stronger than
  process-crash durability or enabling Browser distribution.
- Keep all Jobs model, Browser, mailbox, and employer-facing production flags
  disabled until their independent gates pass.

## Review Checklist (for reviewer)

- [x] Files match the integrated Round 601 scope described above
- [x] No production configuration, deployment, or secret is included
- [x] Tests cover exact authority, persistence, recovery, deletion, and evidence
- [x] Rust and strict TypeScript quality gates pass
- [x] External tenant, PostgreSQL, object-store, and device gates are not overclaimed
