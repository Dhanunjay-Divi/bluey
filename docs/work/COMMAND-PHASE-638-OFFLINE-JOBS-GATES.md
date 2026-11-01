# Phase 638 Offline Jobs Gates Command

**Status: successor executed, terminal0, session56666.** The pre-execution
document SHA-256 was
`69cc447ae5444c9672e25b45c1bb9db0095839a65281cb7e7a5c0a941e9593aa`.
Actual counts: Node306/7/17, Vitest806/219/310/321/397 =2383; zero extra
skips, five typechecks and all builds passed. Eleven-event durable journal:
`EVIDENCE-PHASE-638-SAVED-REGRESSION-20261101.jsonl`, SHA-256
`b443fa8ef4f0d1ea9e1cab857e5a11203eb1fed897ea69637a5112dc586c9b11`.
Final source/full status/portal/browser inventories/root/PID/group/parent/index
checks passed. This status amendment does not alter the executed command below.

Run only after the parent-owned Phase 638
Rust gate has terminated and the `mac-build` machine lock is available. This is
an offline, no-browser Jobs regression gate. It is not the Phase 637 zero-skip
browser qualification and does not close the current browser gate.

November 1 post-saved-session successor: source774/9b1ee, HEAD40949858,
scoped status932ee. The prior recipe at SHA
`5fabb1af3f5386c9450e2a70fbdeff3754023dc52110f7b967e4828dff009184`
ran as31214 and passed2380 on source3de70; separate browser61556 passed both
exclusions, and full Rust52148 passed1916 with strict Clippy/formatting. Those
remain dated exact-source evidence. The new four test-only files add three
report cases; all other770 source hashes reconstruct the exact3de70 manifest,
so Rust and prior browser inputs are unchanged, not newly executed.
This successor changes only the source pin, fresh private parent and a stronger
2383 test floor. No browser exclusion, environment, cleanup or assertion is weakened.

The command is derived from
`COMMAND-PHASE-637-FINAL-JOBS-GATES.md`. It retains the canonical launcher,
private environment, bounded command group, source/status freeze, and process
and open-file inventories. It deliberately does not install or launch
Playwright. The two real-browser tests are named exclusions, while every other
Jobs workspace test remains admitted.

The four non-portal builds rewrite the ignored shared paths
`jobs/{automation,browser,runner,workflows}/dist` under the exclusive machine
lock. Those writes are expected. Do not delete or clean another task's build
output. The portal build uses the actual `vite.config.ts` with a private output
override and never rewrites tracked `web/jobs`.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-saved-regression-20261101 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, readdir, realpath } from "node:fs/promises";
import { join } from "node:path";

const EXPECTED_HEAD = "409498589b8d2e62b5ef54ebdee1372aeb8d005f";
const EXPECTED_SOURCE_FILES = 774;
const EXPECTED_SOURCE_MANIFEST_SHA256 =
  "9b1ee47e1deeb796af1d771513f6c2dd60362f832f1f4c5711bd8d9b2990c764";
const OFFLINE_BASELINE_FLOOR = 2383;
const repo = process.cwd();
const jobsRoot = join(repo, "jobs");
const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT);
assert(root.startsWith(
  "/private/tmp/bluey-test-runs-phase638-saved-regression-20261101/",
));
const rootMetadata = await lstat(root);
assert.equal(rootMetadata.uid, process.getuid());
assert.equal(rootMetadata.mode & 0o777, 0o700);

const home = join(root, "jobs-home");
const browserRoot = join(root, "playwright-empty");
await mkdir(home, { mode: 0o700 });
await mkdir(browserRoot, { mode: 0o700 });
const env = {
  PATH: "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
  HOME: home,
  LANG: "en_US.UTF-8",
  TZ: "UTC",
  CI: "1",
  NO_COLOR: "1",
  TMPDIR: process.env.TMPDIR,
  TMP: process.env.TMP,
  TEMP: process.env.TEMP,
  SQLITE_TMPDIR: process.env.SQLITE_TMPDIR,
  BLUEY_TEST_RUN_ROOT: root,
  BLUEY_SERVER_DB_BACKEND: "sqlite",
  BLUEY_DB_PATH: process.env.BLUEY_DB_PATH,
  BLUEY_DATA_DIR: process.env.BLUEY_DATA_DIR,
  BLUEY_CONFIG_DIR: process.env.BLUEY_CONFIG_DIR,
  BLUEY_RUNTIME_DIR: process.env.BLUEY_RUNTIME_DIR,
  BLUEY_LOG_DIR: process.env.BLUEY_LOG_DIR,
  PLAYWRIGHT_BROWSERS_PATH: browserRoot,
  npm_config_cache: join(root, "npm-cache"),
  npm_config_userconfig: join(home, "npmrc"),
  npm_config_globalconfig: join(home, "global-npmrc"),
  XDG_CONFIG_HOME: join(home, "xdg-config"),
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_OPTIONAL_LOCKS: "0",
};
await mkdir(env.XDG_CONFIG_HOME, { mode: 0o700 });

const group = spawnSync(
  "/bin/ps",
  ["-o", "pgid=", "-p", String(process.pid)],
  { env, encoding: "utf8", timeout: 10_000 },
);
assert.equal(group.status, 0);
assert.equal(
  Number(group.stdout.trim()),
  process.pid,
  "canonical_group_leader_required",
);

function git(args) {
  const result = spawnSync("/usr/bin/git", args, {
    env,
    encoding: "utf8",
    timeout: 10_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert.equal(result.error, undefined);
  assert.equal(result.signal, null);
  assert.equal(result.status, 0, `git ${args.join(" ")} failed`);
  return result.stdout;
}

const sourceScope = [
  "jobs",
  "server",
  "crates/cue-core",
  "infra",
  ".github",
  "scripts/run-bluey-tests.sh",
  "Cargo.toml",
  "Cargo.lock",
];
const head = git(["rev-parse", "HEAD"]).trim();
assert.equal(head, EXPECTED_HEAD);
const beforeScopedStatus = git([
  "status",
  "--porcelain=v1",
  "--untracked-files=all",
  "--",
  ...sourceScope,
]);
const beforeWorktreeStatus = git([
  "status",
  "--porcelain=v1",
  "--untracked-files=all",
]);
const sourceFiles = [
  ...new Set(
    git([
      "ls-files",
      "--cached",
      "--others",
      "--exclude-standard",
      "-z",
      "--",
      ...sourceScope,
    ])
      .split("\0")
      .filter(Boolean),
  ),
].sort();
assert.equal(sourceFiles.length, EXPECTED_SOURCE_FILES);
assert.equal(new Set(sourceFiles).size, sourceFiles.length);

async function hashes(files) {
  const result = {};
  for (const file of files) {
    result[file] = createHash("sha256")
      .update(await readFile(file))
      .digest("hex");
  }
  return result;
}

const beforeSourceHashes = await hashes(sourceFiles);
const sourceManifestSha256 = createHash("sha256")
  .update(JSON.stringify(beforeSourceHashes))
  .digest("hex");
assert.equal(sourceManifestSha256, EXPECTED_SOURCE_MANIFEST_SHA256);
const scopedStatusSha256 = createHash("sha256")
  .update(beforeScopedStatus)
  .digest("hex");
const worktreeStatusSha256 = createHash("sha256")
  .update(beforeWorktreeStatus)
  .digest("hex");

const trackedPortalFiles = git(["ls-files", "-z", "--", "web/jobs"])
  .split("\0")
  .filter(Boolean)
  .sort();
assert(trackedPortalFiles.length > 0, "tracked_portal_bundle_missing");
const beforeTrackedPortalHashes = await hashes(trackedPortalFiles);

for (const dist of [
  "jobs/automation/dist",
  "jobs/browser/dist",
  "jobs/runner/dist",
  "jobs/workflows/dist",
]) {
  git(["check-ignore", "-q", dist]);
}

const mandatoryExcludedBrowserFiles = [
  "jobs/automation/tests/playwright-submit-guard.integration.test.ts",
  "jobs/portal/src/lib/documents/browser-import.integration.test.ts",
];
assert.equal(mandatoryExcludedBrowserFiles.length, 2);
for (const file of mandatoryExcludedBrowserFiles) {
  assert((await lstat(file)).isFile(), `${file} must remain an explicit exclusion`);
}

let interrupted = false;
let stopping = false;
function stopOwnedGroup(reason) {
  if (stopping) return;
  stopping = true;
  interrupted = true;
  console.error(JSON.stringify({
    event: "phase638_offline_jobs_gate_stopping",
    reason,
    ownedGroup: process.pid,
  }));
  setTimeout(() => {
    try {
      process.kill(-process.pid, "SIGKILL");
    } catch {
      process.exit(125);
    }
  }, 5_000);
  try {
    process.kill(-process.pid, "SIGTERM");
  } catch {
    process.exit(125);
  }
}
const handlers = ["SIGHUP", "SIGINT", "SIGTERM"].map((signal) => {
  const handler = () => stopOwnedGroup(signal);
  process.on(signal, handler);
  return [signal, handler];
});

async function run(command, args, cwd = repo, timeoutMs = 300_000) {
  assert(!interrupted);
  const child = spawn(command, args, {
    env,
    cwd,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  let timedOut = false;
  let oversized = false;
  const consume = (chunk) => {
    if (oversized) return;
    process.stdout.write(chunk);
    output += chunk;
    if (output.length > 8 * 1024 * 1024) {
      oversized = true;
      stopOwnedGroup("output_bound");
    }
  };
  child.stdout.on("data", consume);
  child.stderr.on("data", consume);
  const timeout = setTimeout(() => {
    timedOut = true;
    stopOwnedGroup("command_timeout");
  }, timeoutMs);
  const closed = await new Promise((resolve) => {
    child.once("error", () => resolve({ code: null, signal: "spawn_error" }));
    child.once("close", (code, signal) => resolve({ code, signal }));
  });
  clearTimeout(timeout);
  assert(
    !interrupted && !timedOut && !oversized,
    "command_interrupted_timed_out_or_output_bound",
  );
  assert.deepEqual(closed, { code: 0, signal: null }, "jobs_gate_command_failed");
  return output.replace(/\x1b\[[0-9;]*m/g, "");
}

function browserProcesses() {
  const result = spawnSync(
    "/bin/ps",
    ["-axo", "pid=,uid=,comm="],
    { env, encoding: "utf8", timeout: 10_000, maxBuffer: 8 * 1024 * 1024 },
  );
  assert.equal(result.error, undefined);
  assert.equal(result.signal, null);
  assert.equal(result.status, 0);
  return result.stdout
    .split("\n")
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
    .filter(Boolean)
    .filter((row) => row[3].includes(`${browserRoot}/`));
}

async function assertEmptyBrowserRoot(stage) {
  assert.deepEqual(await readdir(browserRoot), [], `${stage}_browser_root_not_empty`);
  assert.equal(browserProcesses().length, 0, `${stage}_browser_process_survived`);
  const openFiles = spawnSync(
    "/usr/sbin/lsof",
    ["-nP", "-a", "-u", String(process.getuid()), "+D", browserRoot, "-F", "pfn"],
    { env, encoding: "utf8", timeout: 30_000, maxBuffer: 1024 * 1024 },
  );
  assert.equal(openFiles.error, undefined);
  assert.equal(openFiles.signal, null);
  assert.equal(openFiles.status, 1);
  assert.equal(openFiles.stdout.trim(), "");
  assert.equal(openFiles.stderr.trim(), "");
  console.log(JSON.stringify({
    event: "phase638_offline_browser_inventory",
    stage,
    files: 0,
    processes: 0,
    openFiles: 0,
  }));
}

function parseNodeSummary(output, label) {
  const totals = [
    ...output.matchAll(/^(?:#|ℹ)\s+tests\s+(\d+)\s*$/gm),
  ].map((match) => Number(match[1]));
  const passed = [
    ...output.matchAll(/^(?:#|ℹ)\s+pass\s+(\d+)\s*$/gm),
  ].map((match) => Number(match[1]));
  const failed = [
    ...output.matchAll(/^(?:#|ℹ)\s+fail\s+(\d+)\s*$/gm),
  ].map((match) => Number(match[1]));
  const skipped = [
    ...output.matchAll(/^(?:#|ℹ)\s+skipped\s+(\d+)\s*$/gm),
  ].map((match) => Number(match[1]));
  assert.deepEqual(
    [totals.length, passed.length, failed.length, skipped.length],
    [1, 1, 1, 1],
    `${label}_node_summary_missing_or_ambiguous`,
  );
  assert.equal(failed[0], 0, `${label}_node_failure`);
  assert.equal(skipped[0], 0, `${label}_unexpected_node_skip`);
  assert.equal(passed[0], totals[0], `${label}_node_count_mismatch`);
  return passed[0];
}

function parseVitestSummary(output, label) {
  const passed = [...output.matchAll(/Tests\s+(\d+) passed/g)].map(
    (match) => Number(match[1]),
  );
  assert.equal(passed.length, 1, `${label}_vitest_summary_missing_or_ambiguous`);
  assert(
    !/Tests[^\n]*\b\d+ skipped\b/.test(output),
    `${label}_unexpected_vitest_skip`,
  );
  assert(!/Tests[^\n]*\b\d+ failed\b/.test(output), `${label}_vitest_failure`);
  return passed[0];
}

await assertEmptyBrowserRoot("before");
console.log(JSON.stringify({
  event: "phase638_offline_jobs_gate_started",
  baseCommit: head,
  sourceFiles: sourceFiles.length,
  sourceManifestSha256,
  scopedStatusSha256,
  worktreeStatusSha256,
  privateHome: true,
  browserLaunchAuthorized: false,
  mandatoryExcludedBrowserFiles,
}));

try {
  for (const args of [
    ["jobs/scripts/ci-guards-self-test.mjs"],
    ["jobs/scripts/privacy-gate.mjs"],
    ["jobs/scripts/check-jobs-schema-parity.mjs"],
    ["jobs/scripts/check-provenance-licenses.mjs"],
    ["--test", "jobs/scripts/browser-release-ci-gate.test.mjs"],
    [
      "jobs/scripts/browser-release-ci-gate.mjs",
      "workflow",
      "--file",
      ".github/workflows/jobs-browser-release.yml",
    ],
    ["--test", "jobs/scripts/managed-cloud-release-gate.test.mjs"],
    [
      "jobs/scripts/managed-cloud-release-gate.mjs",
      "workflow",
      "--file",
      ".github/workflows/jobs-managed-cloud-release.yml",
    ],
    ["jobs/scripts/check-business-messaging-simulator-containment.mjs"],
  ]) {
    await run(process.execPath, args, repo, 600_000);
  }

  const nodeCounts = [];
  for (const [label, script] of [
    ["nonprod_reports", "test:nonprod-reports"],
    ["nonprod_storage", "test:nonprod-storage"],
    ["nonprod_linux_harness", "test:nonprod-linux-harness"],
  ]) {
    const output = await run(
      "/opt/homebrew/bin/npm",
      ["run", script],
      jobsRoot,
      300_000,
    );
    nodeCounts.push(parseNodeSummary(output, label));
  }

  await run(
    "/opt/homebrew/bin/npm",
    ["run", "build", "--workspace", "@bluey/jobs-automation"],
    jobsRoot,
  );

  const workspaceTests = [
    [
      "automation",
      "@bluey/jobs-automation",
      ["--exclude", "tests/playwright-submit-guard.integration.test.ts"],
    ],
    ["browser", "@bluey/jobs-browser", []],
    ["runner", "@bluey/jobs-runner", []],
    ["workflows", "@bluey/jobs-workflows", []],
    [
      "portal",
      "@bluey/jobs-portal",
      ["--exclude", "src/lib/documents/browser-import.integration.test.ts"],
    ],
  ];
  const vitestCounts = [];
  for (const [label, workspace, extra] of workspaceTests) {
    const output = await run(
      "/opt/homebrew/bin/npm",
      ["run", "test", "--workspace", workspace, "--", ...extra],
      jobsRoot,
      600_000,
    );
    vitestCounts.push(parseVitestSummary(output, label));
  }

  const total = [...nodeCounts, ...vitestCounts].reduce(
    (sum, value) => sum + value,
    0,
  );
  assert(
    total >= OFFLINE_BASELINE_FLOOR,
    `offline_jobs_test_floor_failed:${total}`,
  );
  console.log(JSON.stringify({
    event: "phase638_offline_jobs_tests_passed",
    nodeCounts,
    vitestCounts,
    total,
    baselineFloorOnly: OFFLINE_BASELINE_FLOOR,
    extraSkipped: 0,
    mandatoryExcludedBrowserFiles,
    fullBrowserGate: "unverified",
  }));

  const workspaces = [
    "@bluey/jobs-automation",
    "@bluey/jobs-browser",
    "@bluey/jobs-runner",
    "@bluey/jobs-workflows",
    "@bluey/jobs-portal",
  ];
  for (const workspace of workspaces) {
    await run(
      "/opt/homebrew/bin/npm",
      ["run", "typecheck", "--workspace", workspace],
      jobsRoot,
    );
  }

  for (const workspace of workspaces.slice(0, 4)) {
    await run(
      "/opt/homebrew/bin/npm",
      ["run", "build", "--workspace", workspace],
      jobsRoot,
    );
  }

  const privatePortalOutput = join(root, "portal-build");
  await mkdir(privatePortalOutput, { mode: 0o700 });
  const portalBuildCode =
    "import {build} from 'vite';" +
    "await build({configFile:'vite.config.ts',envDir:false,build:{outDir:" +
    JSON.stringify(privatePortalOutput) +
    ",emptyOutDir:false}});";
  await run(
    process.execPath,
    ["--input-type=module", "-e", portalBuildCode],
    join(jobsRoot, "portal"),
    180_000,
  );
  assert((await readdir(privatePortalOutput)).length > 0, "private_portal_output_empty");

  await assertEmptyBrowserRoot("after");
  assert.equal(git(["rev-parse", "HEAD"]).trim(), head);
  assert.equal(
    git([
      "status",
      "--porcelain=v1",
      "--untracked-files=all",
      "--",
      ...sourceScope,
    ]),
    beforeScopedStatus,
  );
  assert.equal(
    git(["status", "--porcelain=v1", "--untracked-files=all"]),
    beforeWorktreeStatus,
  );
  assert.deepEqual(await hashes(sourceFiles), beforeSourceHashes);
  assert.deepEqual(await hashes(trackedPortalFiles), beforeTrackedPortalHashes);
  const whitespace = spawnSync("/usr/bin/git", ["diff", "--check"], {
    env,
    encoding: "utf8",
    timeout: 30_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert.equal(whitespace.error, undefined);
  assert.equal(whitespace.signal, null);
  assert.equal(whitespace.status, 0);
  assert.equal(whitespace.stdout.trim(), "");
  assert.equal(whitespace.stderr.trim(), "");
  console.log(JSON.stringify({
    event: "phase638_offline_jobs_gate_finished",
    baseCommit: head,
    sourceFiles: sourceFiles.length,
    sourceManifestSha256,
    tests: total,
    baselineFloorOnly: OFFLINE_BASELINE_FLOOR,
    mandatoryExcludedBrowserFiles,
    extraSkipped: 0,
    typecheckWorkspaces: workspaces.length,
    builtNonPortalWorkspaces: 4,
    portalOutputPrivate: true,
    trackedPortalUnchanged: true,
    fullBrowserGate: "unverified",
    sourceUnchanged: true,
  }));
} finally {
  try {
    await assertEmptyBrowserRoot("finally");
  } finally {
    if (!stopping) {
      for (const [signal, handler] of handlers) process.off(signal, handler);
    }
  }
}
NODE
```

## Boundaries and expected evidence

- Never add a Playwright install step or point `PLAYWRIGHT_BROWSERS_PATH` at an
  ambient cache. Any browser file, process, or open handle fails the command.
- The two excluded tests are mandatory browser gates, not skipped successes.
  Record their exact names and keep `fullBrowserGate: "unverified"`.
- `2383` is the strengthened post-fixture no-browser floor (previous2380 plus
  three human-report tests). Actual totals must still be parsed from output
  and may be higher; do not replace them with the floor.
- The nonproduction object-store and selected workspace tests use bounded
  loopback listeners. They do not authorize an external provider, Temporal
  service, production account, paid scope, VM, Docker, or employer effect.
- `privacy-gate.mjs` verifies committed/indexed material. The parent-owned
  current-diff privacy review remains separate for dirty Phase 638 source.
- `npm ci`, `npm audit`, aggregate `npm run build --prefix jobs`, actual Linux
  harness execution, provider credentials, and release/publication commands are
  outside this gate.
- Success proves current offline Node regression, typecheck, build, static
  release-contract, source-freeze, and cleanup evidence only. It does not prove
  the real-browser cases, hosted CI, PostgreSQL, Linux runner, Temporal,
  publication, deployment, or production readiness.
