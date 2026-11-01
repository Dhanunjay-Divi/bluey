# Phase 638 current-source API prepared-retry preservation

## Status

**EXECUTED: terminal 0, session 59610.** This command preserved the pre-human-mode
source only. Journal SHA-256:
`fde65fbce37805a55ed17820658c67f177236b93527396d8bffaf08ac34be441`.
All ten events, protocol validators, final 774-file identity, PostgreSQL shutdown,
both owned PID absences, listener refusal, exact root removal, empty parent and
unchanged real index passed. Report: 3,066 bytes, SHA-256
`5a00662946d89bcc2dd43e141344459bdad2fe09c288593d6c0bea5694ba61f0`.
Executed recipe/document SHA before this status amendment:
`fa9061d20a36d350564b013244e9678846d3944bdb9f3545b5031dcbb8ffcd00`.
This is bounded local protocol evidence, not source or production acceptance.

Parent-owned full Rust gate `52148` reached terminal exit `0` on that source:
`1916` tests with suite counts
`[1767,0,1,14,1,1,2,113,2,1,5,9]`, zero failed, zero ignored, and one
passing execution of each of the four FIX-831 regressions named below. Its
nine-event journal was validated at SHA-256
`61ab2f12353a85110fc96714514ccb9051b1dc2dfd609cd7357865597cebd2d5`.

Preparation history: the earlier version targeted source `50f7` and depended
on gate `56416`, which failed its library source guard. That older pin was
never valid for dispatch and is retained here only as history, not as an
executable alternative.

The repinned prepared recipe below is a narrow source-preservation record,
not source acceptance, release approval, publication authority, production
authority, or evidence of a native/container/cloud runtime.

Gate `52148` ran without PostgreSQL and recorded final source equality,
launcher-root removal, an absent launcher PID
and `run.1793511542.16881.2974195990`, empty parent and real index, and
exact final HEAD/scoped-status identity. All were captured before any source
edit. The separately recorded 43 actual PostgreSQL cases retain their dated
same-source scope; this API run must prove its own PostgreSQL shutdown and PID
absence. Do not blend any partial or older run into that prerequisite. The API
command was dispatched only after the parent repeated its prospective-index
privacy check and acquired the shared Mac build lock.

## Exact pre-dispatch authority

Immediately before dispatch, the parent must:

1. complete the existing prospective-index privacy and whitespace procedure;
2. prove that it used a temporary index, did not change the real index, and
   record the passing path/text counts plus prospective tree identity;
3. reuse the exact source-scope/hash algorithm in
   `COMMAND-PHASE-638-FIX841-RUST-GATES.md` and require:
   - `774` source files;
   - source manifest
     `3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1`;
   - HEAD `409498589b8d2e62b5ef54ebdee1372aeb8d005f`; and
   - one captured exact scoped status, retained for byte-for-byte comparison
     after cleanup; and
4. verify that the dedicated parent below is absent or an empty, current-user
   private directory and that no earlier run or PostgreSQL PID belongs to it.

The successful `52148` full Rust output contains one passing, non-ignored
execution of each FIX-831 regression below. Do not compile or rerun them in
this API command; the exact current-source full gate already proves all four:

- `db::jobs::managed_cloud_prepared_ats_retry_requires_same_owner_live_exact_lease`
- `db::jobs::ats_certification_authority_tests::managed_cloud_ats_retry_rejects_incompatible_record_and_runtime`
- `db::jobs::ats_certification_authority_tests::phase_a_nonce_scope_packet_and_exact_replay_fail_closed`
- `db::jobs::ats_certification_authority_tests::certified_cloud_context_runs_phase_a_then_atomic_phase_b`

If any name is absent, failed, ignored, or ambiguous in that full-gate output,
stop and run a newly reviewed Rust gate; do not silently add tests to this API
preservation command.

## Exact command

Run from the physical repository root under the shared Mac build lock. The
canonical launcher creates a fresh private child root. The PostgreSQL wrapper
creates its own password-authenticated loopback PostgreSQL 17 cluster, private
HOME/Cargo view, and closed child environment; it strips owner database URLs,
provider/mailbox credentials, and inherited feature flags before any child.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-current-api-retry-20261101 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'set -euo pipefail
node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --preflight-client
cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --bin bluey-jobs-api --example jobs-nonprod-runner-control-plane-fixture
node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --auto-submit-prepared-repeat
node --input-type=module <<\NODE
import assert from "node:assert/strict";
import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { createHash } from "node:crypto";
import { basename, join } from "node:path";
const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT || "/missing-owned-root");
assert.equal(root, process.env.BLUEY_TEST_RUN_ROOT, "canonical_launcher_root");
const path = join(root, "runner-control-plane-auto-submit-prepared-repeat.json");
const pathBefore = await lstat(path);
assert.ok(pathBefore.isFile() && !pathBefore.isSymbolicLink(), "report_regular_file");
const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
try {
  const before = await file.stat();
  assert.ok(before.isFile() && before.nlink === 1 && before.uid === process.getuid(), "report_owned_single_link");
  assert.equal(before.mode & 0o7777, 0o600, "report_private_mode");
  assert.ok(before.size > 0 && before.size <= 16 * 1024, "report_bounded_size");
  assert.equal(await realpath(path), path, "report_canonical_path");
  assert.equal(pathBefore.dev, before.dev, "report_device");
  assert.equal(pathBefore.ino, before.ino, "report_inode");
  const bytes = await file.readFile();
  const after = await file.stat();
  const pathAfter = await lstat(path);
  assert.ok(after.dev === before.dev && after.ino === before.ino && after.size === before.size && after.mtimeMs === before.mtimeMs, "report_changed_during_read");
  assert.ok(pathAfter.dev === before.dev && pathAfter.ino === before.ino && !pathAfter.isSymbolicLink(), "report_path_changed_during_read");
  assert.equal(bytes.length, before.size, "report_short_read");
  const report = JSON.parse(bytes.toString("utf8"));
  assert.equal(report.sourceAccepted, false, "report_not_source_acceptance");
  process.stdout.write(JSON.stringify({ event: "runner_control_plane_auto_prepared_repeat_report_metadata", basename: basename(path), bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex"), sourceAccepted: false }) + "\n");
} finally {
  await file.close();
}
NODE'
```

Do not add `npx`, dependency installation, a browser, Temporal, Docker, a VM,
provider credentials, owner URLs, an external listener, or another test suite.
The debug `integration-test-support` build is deliberate and is not a release
artifact or native/platform measurement.

## Required observed result

The command is passing only when all existing validators succeed and the parent
records the following bounded evidence without copying private payloads:

- `runner_control_plane_client_preflight` reports exact production-export
  import, unchanged source, and `sourceAccepted:false`;
- `runner_control_plane_reproduction_start` records SHA-256 identities for the
  owned `bluey-jobs-api` and
  `jobs-nonprod-runner-control-plane-fixture` binaries;
- the private mode-`0600`, single-link
  `runner-control-plane-auto-submit-prepared-repeat.json` is nonempty and no
  larger than 16 KiB, and the strict validator emits
  `runner_control_plane_auto_prepared_repeat_confirmed`;
- before PostgreSQL or canonical-root cleanup, the bounded metadata reader
  revalidates that exact report through a no-follow file descriptor and emits
  `runner_control_plane_auto_prepared_repeat_report_metadata` containing only
  its basename, byte length, SHA-256, and `sourceAccepted:false`;
- the first claim is `200`; the exact-input fresh-proof repeat is `200` with a
  new token and strictly higher fence; the complete frozen ATS record remains
  unchanged; current lease/volume/process-runtime, attempt, immutable-family,
  and account-subject bindings remain exact;
- freshly signed stale-token heartbeat and safe-finish each return `409`
  without changing all 20 protected families; current heartbeat returns `200`,
  safe release returns `204`, and fresh signed terminal replay returns `204`;
- capacity, canary reservation, irreversible receipt, Temporal acceptance, and
  external-effect counts remain zero; all four recorded children exit `0`,
  their PIDs are absent, listeners refuse after shutdown, and the report keeps
  `sourceAccepted:false`; and
- `nonprod_postgres_finished` reports status `0`, shutdown attempted and server
  exited; the exact postmaster PID file is absent before launcher cleanup.

After terminal exit, rerun the same 774-file manifest/HEAD/scoped-status check
used before dispatch and require exact equality. Record the real index as
unchanged, both binary hashes, the bounded report identity/size, PostgreSQL and
child PID absence, listener refusal, exact launcher-root absence, and an empty
`/private/tmp/bluey-test-runs-phase638-current-api-retry-20261101` parent. Any
cleanup or final-source mismatch makes the run non-passing even if the protocol
report passed.

## Evidence boundary

This case exercises current native-Mac debug API/client behavior against an
owned loopback PostgreSQL fixture only. It does not run Phase B, click Submit,
recover an expired/takeover lease, start a browser, qualify a Linux image or
native addon, contact an ATS/employer/provider, publish an artifact, change a
flag, or prove hosted CI, release, production, latency, cost, or capacity.
