# Phase 638 two synthetic browser gates

**Status: prepared, not executed.** This command runs only the two browser tests
excluded from the completed 2,380-test offline Jobs gate. It uses Playwright's
configured Chromium in a fresh private root; it never uses a system browser,
browser profile, account, PostgreSQL credential, provider, or employer endpoint.

Run from the canonical checkout after source review. The parent wrapper must
retain the terminal event and exit status, then verify the exact run root is
absent and the private parent is empty after canonical cleanup.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
  with-machine-lock --machine mac-build -- \
  env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-two-browser-gates-20261101 \
  scripts/run-bluey-tests.sh -- \
  env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, realpath } from "node:fs/promises";
import { join } from "node:path";

const EXPECTED_HEAD = "409498589b8d2e62b5ef54ebdee1372aeb8d005f";
const EXPECTED_SOURCE_FILES = 774;
const EXPECTED_SOURCE_MANIFEST =
  "3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1";
const repo = process.cwd();
const jobs = join(repo, "jobs");
const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT);
assert(root.startsWith(
  "/private/tmp/bluey-test-runs-phase638-two-browser-gates-20261101/",
));
const rootInfo = await lstat(root);
assert.equal(rootInfo.uid, process.getuid());
assert.equal(rootInfo.mode & 0o777, 0o700);

const home = join(root, "home");
const browserRoot = join(root, "playwright");
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
  PLAYWRIGHT_DOWNLOAD_CONNECTION_TIMEOUT: "30000",
  npm_config_cache: join(root, "npm-cache"),
  npm_config_userconfig: join(home, "npmrc"),
  npm_config_globalconfig: join(home, "global-npmrc"),
  XDG_CONFIG_HOME: join(home, "xdg-config"),
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_OPTIONAL_LOCKS: "0",
};
await mkdir(env.XDG_CONFIG_HOME, { mode: 0o700 });
for (const denied of [
  "BLUEY_DATABASE_URL",
  "BLUEY_TEST_POSTGRES_URL",
  "BLUEY_JWT_SECRET",
  "BLUEY_ACCESS_TOKEN",
  "AWS_ACCESS_KEY_ID",
  "AWS_SECRET_ACCESS_KEY",
]) assert.equal(env[denied], undefined, `${denied}_must_be_absent`);

const group = spawnSync(
  "/bin/ps",
  ["-o", "pgid=", "-p", String(process.pid)],
  { env, encoding: "utf8", timeout: 10_000 },
);
assert.equal(group.status, 0);
assert.equal(Number(group.stdout.trim()), process.pid, "canonical_group_required");

function git(args) {
  const result = spawnSync("/usr/bin/git", args, {
    env,
    encoding: "utf8",
    timeout: 10_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert.equal(result.error, undefined);
  assert.equal(result.signal, null);
  assert.equal(result.status, 0, `git_${args[0]}_failed`);
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
assert.equal(git(["rev-parse", "HEAD"]).trim(), EXPECTED_HEAD);
const beforeStatus = git([
  "status", "--porcelain=v1", "--untracked-files=all", "--", ...sourceScope,
]);
const sourceFiles = [...new Set(git([
  "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--",
  ...sourceScope,
]).split("\0").filter(Boolean))].sort();
assert.equal(sourceFiles.length, EXPECTED_SOURCE_FILES);

async function hashes(files) {
  const result = {};
  for (const file of files) {
    result[file] = createHash("sha256")
      .update(await readFile(file))
      .digest("hex");
  }
  return result;
}

const beforeSource = await hashes(sourceFiles);
assert.equal(
  createHash("sha256").update(JSON.stringify(beforeSource)).digest("hex"),
  EXPECTED_SOURCE_MANIFEST,
);
const webFiles = git(["ls-files", "-z", "--", "web/jobs"])
  .split("\0").filter(Boolean).sort();
assert(webFiles.length > 0, "tracked_web_jobs_missing");
const beforeWeb = await hashes(webFiles);
const fixtureSource = await readFile(
  join(jobs, "portal/nonprod/document-import.integration.mjs"),
  "utf8",
);
assert(fixtureSource.includes("envDir: false"), "fixture_env_discovery_not_disabled");

let interrupted = false;
let stopping = false;
function stopOwnedGroup(reason) {
  if (stopping) return;
  stopping = true;
  interrupted = true;
  console.error(JSON.stringify({
    event: "phase638_two_browser_gates_stopping",
    reason,
  }));
  setTimeout(() => {
    try { process.kill(-process.pid, "SIGKILL"); }
    catch { process.exit(125); }
  }, 5_000);
  try { process.kill(-process.pid, "SIGTERM"); }
  catch { process.exit(125); }
}
const handlers = ["SIGHUP", "SIGINT", "SIGTERM"].map((signal) => {
  const handler = () => stopOwnedGroup(signal);
  process.on(signal, handler);
  return [signal, handler];
});

async function run(stage, command, args, cwd, timeoutMs) {
  assert(!interrupted);
  console.log(JSON.stringify({ event: "phase638_browser_stage_started", stage }));
  const child = spawn(command, args, {
    cwd,
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  let oversized = false;
  let timedOut = false;
  const consume = (chunk) => {
    if (oversized) return;
    output += chunk;
    if (output.length > 4 * 1024 * 1024) {
      oversized = true;
      stopOwnedGroup(`${stage}_output_bound`);
    }
  };
  child.stdout.on("data", consume);
  child.stderr.on("data", consume);
  const timer = setTimeout(() => {
    timedOut = true;
    stopOwnedGroup(`${stage}_timeout`);
  }, timeoutMs);
  const closed = await new Promise((resolve) => {
    child.once("error", () => resolve({ code: null, signal: "spawn_error" }));
    child.once("close", (code, signal) => resolve({ code, signal }));
  });
  clearTimeout(timer);
  assert(!interrupted && !oversized && !timedOut, `${stage}_bounded`);
  assert.deepEqual(closed, { code: 0, signal: null }, `${stage}_failed`);
  console.log(JSON.stringify({ event: "phase638_browser_stage_passed", stage }));
  return output.replace(/\x1b\[[0-9;]*m/g, "");
}

function assertSingleton(output, label) {
  assert.deepEqual(
    [...output.matchAll(/Test Files\s+(\d+) passed \((\d+)\)/g)]
      .map((match) => [Number(match[1]), Number(match[2])]),
    [[1, 1]],
    `${label}_file_count`,
  );
  assert.deepEqual(
    [...output.matchAll(/Tests\s+(\d+) passed \((\d+)\)/g)]
      .map((match) => [Number(match[1]), Number(match[2])]),
    [[1, 1]],
    `${label}_test_count`,
  );
  assert(!/Tests[^\n]*\d+ skipped/.test(output), `${label}_skip`);
}

async function inventoryBrowser() {
  const inspect = () => {
    const result = spawnSync("/bin/ps", ["-axo", "pid=,uid=,comm="], {
      env,
      encoding: "utf8",
      timeout: 10_000,
      maxBuffer: 8 * 1024 * 1024,
    });
    assert.equal(result.status, 0);
    return result.stdout.split("\n")
      .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
      .filter(Boolean)
      .filter((row) => row[3].includes(`${browserRoot}/`));
  };
  let rows = inspect();
  const forced = rows.length > 0;
  for (const signal of ["SIGTERM", "SIGKILL"]) {
    for (const row of rows) {
      assert.equal(Number(row[2]), process.getuid());
      const current = inspect().find((item) => item[1] === row[1]);
      if (current) {
        assert.equal(current[3], row[3]);
        try { process.kill(Number(row[1]), signal); }
        catch (error) { if (error.code !== "ESRCH") throw error; }
      }
    }
    for (let attempt = 0; attempt < 50; attempt += 1) {
      rows = inspect();
      if (!rows.length) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    if (!rows.length) break;
  }
  assert.equal(rows.length, 0, "private_browser_process_survived");
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
    event: "phase638_private_browser_closed",
    processes: 0,
    openFiles: 0,
    forcedCleanup: forced,
  }));
  assert(!forced, "private_browser_required_forced_cleanup");
}

try {
  console.log(JSON.stringify({
    event: "phase638_two_browser_gates_started",
    sourceFiles: sourceFiles.length,
    sourceManifestSha256: EXPECTED_SOURCE_MANIFEST,
  }));
  await run(
    "chromium_install",
    process.execPath,
    [join(jobs, "node_modules/playwright/cli.js"), "install", "chromium"],
    repo,
    240_000,
  );
  const submit = await run(
    "exact_submit_guard",
    "/opt/homebrew/bin/npm",
    ["run", "test", "--workspace", "@bluey/jobs-automation", "--",
      "tests/playwright-submit-guard.integration.test.ts"],
    jobs,
    90_000,
  );
  assertSingleton(submit, "exact_submit_guard");

  const documentImport = await run(
    "document_import",
    "/opt/homebrew/bin/npm",
    ["run", "test", "--workspace", "@bluey/jobs-portal", "--",
      "src/lib/documents/browser-import.integration.test.ts"],
    jobs,
    150_000,
  );
  assertSingleton(documentImport, "document_import");
  const reportLine = documentImport.split("\n").map((line) => line.trim())
    .find((line) => line.startsWith('{"event":"document_import_qualification",'));
  assert(reportLine, "document_import_report_missing");
  const report = JSON.parse(reportLine);
  assert.deepEqual(report.cases, ["pdf", "docx-symbol", "txt", "malformed", "oversize"]);
  assert.equal(report.passed, 5);
  assert.equal(report.realPdfWorker, true);
  assert.equal(report.unexpectedRequests, 0);
  assert.equal(report.pageErrors, 0);
  assert.equal(report.browserClosed, true);
  assert.equal(report.engineClosed, true);
  assert.equal(report.serverClosed, true);
  assert.equal(report.listenerClosed, true);

  await inventoryBrowser();
  assert.equal(git(["rev-parse", "HEAD"]).trim(), EXPECTED_HEAD);
  assert.equal(git([
    "status", "--porcelain=v1", "--untracked-files=all", "--", ...sourceScope,
  ]), beforeStatus);
  assert.deepEqual(await hashes(sourceFiles), beforeSource);
  assert.deepEqual(await hashes(webFiles), beforeWeb);
  console.log(JSON.stringify({
    event: "phase638_two_browser_gates_passed",
    singletonTests: 2,
    skipped: 0,
    documentCases: 5,
    realPdfWorker: true,
    unexpectedRequests: 0,
    trackedWebJobsUnchanged: true,
    sourceUnchanged: true,
  }));
} finally {
  try { await inventoryBrowser(); }
  finally {
    if (!stopping) {
      for (const [signal, handler] of handlers) process.off(signal, handler);
    }
  }
}
NODE
```

Acceptance is exactly one passed, zero skipped test per file, the five nested
document cases, unchanged frozen source and `web/jobs`, no surviving private
Chromium process or open handle, terminal exit 0, and canonical root cleanup.
This is synthetic local-browser evidence only; it does not qualify a real
application, provider, account, PostgreSQL path, cloud runner, or release.
