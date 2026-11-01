# Phase 638 saved non-production account qualification

## Status and authority

**PREPARED, NOT EXECUTED.** The saved-account qualification itself has not run.
Parent API-preservation run `59610` reached terminal exit `0`; all ten durable
events and its PostgreSQL, PID, listener, launcher-root, source and empty-parent
cleanup checks passed. Its validated journal SHA-256 is
`fde65fbce37805a55ed17820658c67f177236b93527396d8bffaf08ac34be441`.

The parent then applied the independently reviewed four-file patch:

- `docs/work/PREPARED-PHASE-638-HUMAN-HOLD.patch`
- required patch SHA-256:
  `7617a146edd08f685c9dfaf008ec2f429f90d1dca77ae3f0d9b7dc9ad11f77f5`

Parent-recorded post-application checks passed for all four JavaScript syntax
targets and all nine report-unit cases. The exact current authority is:

- 774-file source manifest SHA-256
  `9b1ee47e1deeb796af1d771513f6c2dd60362f832f1f4c5711bd8d9b2990c764`;
- raw scoped-status SHA-256
  `932ee3fd34f0e460355ecf5ef07063c2c940cd431cacf8e2fc22da5588722fdc`;
- HEAD `409498589b8d2e62b5ef54ebdee1372aeb8d005f`.

Parent prospective privacy passed for 89 changed paths, 2,958 tracked paths
and 2,679 text files, with prospective tree
`e2261e50254a9607e3a89e9d8ce61bcbc450bd3b`.

No result below may be joined to the pre-application `3de70` proof or described
as already executed. Freeze this source, scoped status and real index through
all four terminals; recheck their exact equality before every launcher dispatch
and after final cleanup.

This qualification uses only generated accounts and data, the canonical
launcher, the shared `mac-build` lock, password-authenticated loopback
PostgreSQL 17, configured Playwright Chromium, and existing production API
parsers. It authorizes no provider, employer, email, Temporal, production,
owner-profile, or real-application effect.

## Four fresh roots

The wrapper creates `browser-qualification` with non-recursive `mkdirSync`.
Never invoke it twice in one `BLUEY_TEST_RUN_ROOT`. Use these four distinct
canonical launcher parents, one invocation each:

- `/private/tmp/bluey-test-runs-phase638-saved-account-default-20261101`
- `/private/tmp/bluey-test-runs-phase638-saved-account-human-20261101`
- `/private/tmp/bluey-test-runs-phase638-saved-account-signal-startup-20261101`
- `/private/tmp/bluey-test-runs-phase638-saved-account-signal-ready-20261101`

Each parent must begin absent or owned, private, and empty. Each invocation
gets a distinct child root, Cargo target, portal build, PostgreSQL cluster, and
Chromium directory. Do not copy a Cargo target or browser directory between
roots, redirect `PLAYWRIGHT_BROWSERS_PATH`, or use ambient Chrome/profile data.
The bounded cost is four offline Rust binary/example builds, four private
portal builds, and four configured Chromium preparations. Canonical Cargo
registry/git cache links are the only admitted cache reuse. If the configured
Chromium preparation cannot complete from the admitted cache/download path,
stop; never fall back to an ambient browser, profile, or copied engine.

The retained Phase 633 prerequisites are explicit: default API binaries must
be built before the feature-gated fixture example; the helper example tests,
positive-authority fixture tests, mixed/negated engagement regression and
warnings-denied example Clippy remain covered by their unchanged-source/full
Rust evidence. The applied patch changed only the four JavaScript test files,
whose four syntax checks and nine report-unit cases are recorded above. The
fresh per-root builds below remain mandatory; the already-proven tests are not
silently reclassified as results of this qualification.

## Positive commands

Define this shell function from the physical repository root after the source
pin is verified. The API/full-Rust prerequisite and post-application syntax and
report tests above are already proven, so these runtime commands do not rerun
them. Each fresh root still builds the required default APIs and feature-gated
fixture example before invoking the wrapper.

```sh
run_phase638_saved_positive() {
  local parent="$1" mode="$2"
  /opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
    with-machine-lock --machine mac-build -- \
    env BLUEY_TEST_RUN_PARENT="$parent" scripts/run-bluey-tests.sh -- \
    env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node \
    jobs/scripts/run-nonprod-postgres-tests.mjs \
    --pg-bin /opt/homebrew/opt/postgresql@17/bin -- \
    /bin/zsh -f -c 'set -euo pipefail
cargo build --offline --locked --manifest-path server/Cargo.toml --bins
cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --example jobs-nonprod-browser-fixture
if [[ "$1" == default ]]; then
  node jobs/scripts/run-nonprod-browser-api-tests.mjs
else
  node jobs/scripts/run-nonprod-browser-api-tests.mjs --human-hold
fi' phase638 "$mode"
}

run_phase638_saved_positive \
  /private/tmp/bluey-test-runs-phase638-saved-account-default-20261101 default

run_phase638_saved_positive \
  /private/tmp/bluey-test-runs-phase638-saved-account-human-20261101 human
```

Run the default invocation first and require the unchanged v4 ordered `9/9`
report. Start the human invocation only after default cleanup is complete. On
the exact `human_browser_ready` event, the agent may use CUA to operate only
the headed isolated fixture: confirm the compact test panel does not obscure
the focused Prepare control, create exactly one prepared Review First
application, reload the actual Applications UI, observe the persisted packet,
then activate **Finish synthetic test**. File inputs and downloads remain
denied. Use no owner file, profile, account, browser session, or real data.

The headed result is an agent-operated synthetic smoke, not an owner approval
or production human test. It must produce only the separate v1
`human-browser-api-result.json`; the default root must produce only
`browser-api-result.json`. Never rename, merge, or substitute those reports.
These two positive invocations are the immediate stage. Run the signal-negative
follow-up only after both positives terminate, clean up, and retain the exact
source/status/index pins above.

## Signal-negative controller

For each negative root, build the default binaries/example exactly as in the
positive function, then run the following controller instead of calling the
wrapper directly. Pass `startup` for the startup root and `ready` for the
after-ready root. The controller sends SIGTERM to the wrapper, which must allow
the patched child its bounded cooperative cleanup before escalation.

```sh
run_phase638_saved_negative() {
  local parent="$1" target="$2"
  /opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
    with-machine-lock --machine mac-build -- \
    env BLUEY_TEST_RUN_PARENT="$parent" scripts/run-bluey-tests.sh -- \
    env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node \
    jobs/scripts/run-nonprod-postgres-tests.mjs \
    --pg-bin /opt/homebrew/opt/postgresql@17/bin -- \
    /bin/zsh -f -c 'set -euo pipefail
cargo build --offline --locked --manifest-path server/Cargo.toml --bins
cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --example jobs-nonprod-browser-fixture
node --input-type=module - "$1" <<\NODE
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { lstat } from "node:fs/promises";
import { join } from "node:path";

const target = process.argv[2];
assert(["startup", "ready"].includes(target), "exact negative target required");
const root = process.env.BLUEY_TEST_RUN_ROOT;
const cargoTarget = process.env.CARGO_TARGET_DIR;
assert(root && cargoTarget === join(root, "cargo-target"));
const args = ["jobs/scripts/run-nonprod-browser-api-tests.mjs"];
if (target === "ready") args.push("--human-hold");
const wrapper = spawn(process.execPath, args, { stdio: ["ignore", "pipe", "inherit"] });
let buffer = "";
let sent = false;
let observed = new Set();

function processes() {
  const result = spawnSync("/bin/ps", ["-axo", "pid=,ppid=,command="], {
    encoding: "utf8", timeout: 10_000, maxBuffer: 1024 * 1024,
  });
  assert.equal(result.status, 0, "process_inventory_failed");
  return result.stdout.split("\n").flatMap((line) => {
    const match = /^\s*(\d+)\s+(\d+)\s+(.*)$/.exec(line);
    return match ? [{ pid: Number(match[1]), ppid: Number(match[2]), command: match[3] }] : [];
  });
}
function descendants(records) {
  const ids = new Set([wrapper.pid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const record of records) if (ids.has(record.ppid) && !ids.has(record.pid)) {
      ids.add(record.pid); changed = true;
    }
  }
  ids.delete(wrapper.pid);
  return records.filter((record) => ids.has(record.pid));
}
function interrupt(records) {
  if (sent) return;
  observed = new Set(descendants(records).map((record) => record.pid));
  assert(observed.size > 0, "owned_child_process_required_before_signal");
  sent = true; wrapper.kill("SIGTERM");
}

wrapper.stdout.on("data", (chunk) => {
  process.stdout.write(chunk);
  buffer += chunk;
  assert(buffer.length <= 4 * 1024 * 1024, "bounded_wrapper_output");
  let newline;
  while ((newline = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, newline); buffer = buffer.slice(newline + 1);
    try {
      const event = JSON.parse(line);
      if (target === "ready" && event.event === "human_browser_ready") interrupt(processes());
    } catch {}
  }
});
const poll = setInterval(() => {
  if (target !== "startup" || sent) return;
  const records = processes();
  const children = descendants(records);
  if (children.some((record) => record.command.includes(join(cargoTarget, "debug", "bluey-jobs-api")))) {
    interrupt(records);
  }
}, 100);
const deadline = setTimeout(() => {
  if (!sent) wrapper.kill("SIGTERM");
}, target === "startup" ? 360_000 : 720_000);
const ended = await new Promise((resolve) => {
  wrapper.once("error", () => resolve({ code: null, signal: "spawn_error" }));
  wrapper.once("close", (code, signal) => resolve({ code, signal }));
});
clearInterval(poll); clearTimeout(deadline);
assert.equal(sent, true, "required_signal_point_not_reached");
assert.notDeepEqual(ended, { code: 0, signal: null }, "interrupted_wrapper_cannot_pass");
for (const pid of observed) {
  assert.throws(() => process.kill(pid, 0), (error) => error?.code === "ESRCH");
}
for (const name of ["browser-api-result.json", "human-browser-api-result.json"]) {
  await assert.rejects(lstat(join(root, name)), { code: "ENOENT" });
}
const browserRoot = join(root, "browser-qualification", "chromium");
if (existsSync(browserRoot)) {
  const lsof = spawnSync("/usr/sbin/lsof", ["-nP", "-Fp", "+D", browserRoot], {
    encoding: "utf8", timeout: 10_000, maxBuffer: 1024 * 1024,
  });
  assert([0, 1].includes(lsof.status));
  assert.equal(lsof.stdout.split("\n").filter((line) => /^p\d+$/.test(line)).length, 0,
    "owned_browser_open_handle_remains");
}
process.stdout.write(JSON.stringify({ event: "saved_account_signal_negative_confirmed",
  target, interrupted: true, reportsAbsent: true, pidsAbsent: true,
  sourceAccepted: false }) + "\n");
NODE' phase638 "$target"
}

run_phase638_saved_negative \
  /private/tmp/bluey-test-runs-phase638-saved-account-signal-startup-20261101 startup

run_phase638_saved_negative \
  /private/tmp/bluey-test-runs-phase638-saved-account-signal-ready-20261101 ready
```

The function runs the same two offline builds in each fresh negative root. Do
not add the Rust helper tests or rerun the default `9/9` in those roots.

## Acceptance and cleanup

- Default: exact v4 ordered `9/9`, no failure, bounded positive request counts,
  exact object-store counters, clean absent API/helper PIDs, refused listeners,
  and no human report.
- Human: exact closed v1 human cases, one prepared `review_first` application
  preserved across actual UI reload and API/opaque-PG readback, zero submission,
  browser/workflow/run/effect rows, exact blocked nonlocal canary, zero page or
  server errors, clean process/open-file/listener inventory, and no v4 report.
- Signal startup and ready: nonzero wrapper result, exact signal point reached,
  no pass report or success metadata, all captured descendants absent, and zero
  owned Chromium open handles. A negative is never reusable positive evidence.
- Every outer PostgreSQL wrapper must report status `0`, attempted shutdown and
  server exit, with its exact postmaster PID file absent. Every canonical child
  root must be absent and each named parent empty after terminal exit.
- After all four terminals, repeat the filled 774-file manifest, HEAD, exact
  scoped-status and real-index checks byte-for-byte. Any source, index, PG,
  process, listener, report, root, or parent mismatch makes the qualification
  non-passing.

## Known limits

The saved account exists only in one disposable PostgreSQL run and survives UI
reload inside that run; cleanup intentionally removes it. This does not prove a
password login, email flow, cross-restart durability, real owner testing,
employer navigation, submission, provider connectivity, cloud runner, receipt,
production deployment, accessibility conformance, or release authority. The
headed smoke must separately observe that the compact test panel does not
obscure the focused Prepare control; its inline style alone is not proof.
