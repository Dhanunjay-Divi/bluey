# Jobs-only Phase 634 local handoff

Date: 2026-10-27. Bounded local Linux/ARM64 qualification passed; overall Jobs
production/parity goal remains active and unfinished.

## Authority

- Checkout: `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`
- Branch: `feat/phase-634-jobs-isolated-linux-runner-qualification`
- Tested image source: `d9a5b0db398386c2701b98ecb006cc3e0f46c96b`.
- Code history: `e5532431` harness, `263798aa` socket/diagnostics FIX805,
  `d9a5b0db` exact versioned Lima PID adapter.
- Resolve closeout subject `docs(jobs): record isolated Linux runner qualification`.
  That docs-only successor is NOT the tested image's source SHA or a release.
- Read `bluey-ops`, checkout AGENTS.md, Round/IMPL/REVIEW634, FIX805 and the
  Jobs completion ledger. Current code/local handoffs are authority; SSD unused.
- Preserve inherited untracked REVIEW628; never edit/start meeting-owned
  `/Users/uno/Downloads/cue`, desktop/overlay/Pinky or unrelated Colima profiles.
- Project-scoped task: `task-1793138516187-n6pb5w`.

## Actual new proof

Session74829 exited0. A fresh private native aarch64 VZ/Colima VM passed live
4CPU/6GiB/8GiB-root+16GiB-data checks, no host mounts and exact private socket.
Immutable HEAD-only context:111 files/2,508,373 bytes; source unchanged. Existing
pinned runner Dockerfile built in explicit `legacy-explicit` mode, no new plugin
or global context/settings. Image was Linux/ARM64 with the expected nonroot user.

Image ID:
`sha256:ef2ae6e9c1976a490d08081579c2570373f1d39e8190772581778f98506b96f3`.
Embedded measurement SHA256:
`b69ad2395160ade4337a5e2708be2bdb2dad9522f7d90c93f8df5d64727c1dd6`.
Readback bound exact source/build/component/role and CI-only zero authority
hashes. These cannot grant production authority.

Existing native-addon script ran unchanged via stdin and passed1/1. Actual
finished-container inspection required no network/published ports/binds,
read-only root, no added capabilities/privilege escalation, bounded CPU/RAM/PIDs
and only bounded private tmpfs. It verified open/identity, exclusive write,
bounded read, inventory and removal; no Jobs runner server or employer call.

Automatic cleanup proved stopped inventories/PID2988 absent before exact VM/data
deletion; final private inventories empty, Docker socket gone, state root
`/private/tmp/bluey-jobs-linux-p634-IfYImk` absent. Parent also checked observed
helper/mux/client PIDs2986/3019/3183 absent. Four unrelated HQ/Pinky profiles stayed
stopped and the global Docker context stayed `default`. No image/key/disk remains.
Whole-host free-space delta was-48,373,760 bytes, not attributed to a leak;
all owned roots were absent. It is not a capacity/latency benchmark.

Fresh source session80351 exited0: Node8+7+17, workspaces804/219/308/321/387,
2,071 passes, zero skips; all five typechecks. Configured Chromium was private
and the exact-submit/beacon synthetic guard actually ran. Launcher self-test
session17598 exited0, including intentional negative cleanup cases. All five
Phase634 canonical parents were empty; both failed/successful separate VM-state
roots absent. Final scoped independent review is green; privacy closeout is
recorded in IMPL634. No new full Rust/all-target Clippy claim.

## Failure/root cause preserved

Initial source `e5532431`, run10495 failed before VM creation: a generated
Lima SSH socket was105 bytes while macOS requires<104. Diagnostics78399 proved
the exact cause on stdout, stop/not-running and empty inventories. FIX805 records
inode/owner/marker/lsof checks and removal of only331MiB recreatable cache/state.
The first result remains failed, not averaged into the accepted run.

The corrected harness preflights UTF8 socket byte budgets, uses private `/l`,
captures bounded private stdout AND stderr, preserves primary/cleanup failures
separately, and retains only bounded diagnostics after fully proven VM cleanup.
Unknown cleanup preserves complete state. Lima2.2.1's documented zero-PID JSON
omission is interpreted only for the exact reviewed VZ/aarch64/no-error protocol;
live host PID and pre-delete recorded-process absence are still mandatory.

## Reproduce

Runtime requires committed clean Jobs inputs, at least32GiB free, installed
approved Colima/Lima/Docker and native Darwin arm64. Do not use owner/default
credentials, HOME, contexts or preexisting profiles. Canonical launcher does NOT
set private HOME; the explicit wrapper must create/pass it:

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
  with-machine-lock --machine mac-build -- scripts/run-bluey-tests.sh -- \
  env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module -e '
import { mkdir, realpath } from "node:fs/promises";
import { join } from "node:path";
import { spawn } from "node:child_process";
const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT);
const childHome = join(root, "linux-cli-home");
await mkdir(childHome, { mode: 0o700 });
const child = spawn(process.execPath, ["jobs/scripts/run-nonprod-linux-runner-tests.mjs"], {
  env: { PATH: "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin", HOME: childHome,
    BLUEY_TEST_RUN_ROOT: root }, stdio: "inherit",
});
const handlers = ["SIGHUP", "SIGINT", "SIGTERM"].map(signal => {
  const handler = () => child.kill(signal);
  process.on(signal, handler);
  return [signal, handler];
});
const closed = await new Promise(resolve => {
  child.once("error", () => resolve({ code: 1 }));
  child.once("close", (code, signal) => resolve({ code, signal }));
});
for (const [signal, handler] of handlers) process.off(signal, handler);
process.exitCode = closed.code ?? 1;
'
```

For pure tests (no VM side effect):
`npm run test:nonprod-linux-harness --prefix jobs`.

## Next work and holds

Full current server all-target/Rust gates remain a dated-baseline gap. The
candidate journey still ends at internal queue; stitch matching private HTTP
runner claim/heartbeat/release, then controlled browser/receipt evidence. Do not
reuse local ATS certification as cloud authority or weaken network/availability/
source/identity gates. Hosted object/provider/model/OAuth/billing canaries,
amd64/BuildKit/exact-tip CI/artifacts, capacity/rollback, MCP/C2C/business messaging
and client parity remain in the ledger. A native probe is not full autonomy.

No push/PR/merge, registry/signing publication, deployment, paid resource,
production flag, real application/email/calendar or desktop/overlay change.
