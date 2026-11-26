import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { closeSync, constants, fstatSync, lstatSync, mkdtempSync, openSync,
  readFileSync, readSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { FOCUSED_RUNNER_CASES, verifyFocusedVitestReport } from "./verify-focused-vitest-report.mjs";

const MAX_BYTES = 2 * 1024 * 1024;
const RUN_ROOT = /^\/private\/tmp\/bluey-test-runs-[A-Za-z0-9_-]+\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/;
const failedRuns = new WeakMap();

// Explicit retrieval for a private evidence writer. Never put raw child
// output into an exception message, enumerable property, or console log.
export function focusedVitestFailureEvidence(error) {
  const evidence = failedRuns.get(error);
  if (!evidence) return undefined;
  const copyStream = ({ bytes, truncated }) => ({ bytes: bytes && Buffer.from(bytes), truncated });
  return {
    terminal: { ...evidence.terminal },
    streams: { stdout: copyStream(evidence.streams.stdout), stderr: copyStream(evidence.streams.stderr) },
    streamsComplete: evidence.streamsComplete,
    reportStatus: evidence.reportStatus,
    report: evidence.report && Buffer.from(evidence.report),
    verified: false,
  };
}

function privateDirectoryIdentity(path, label) {
  assert.equal(realpathSync(path), path, `${label}_path`);
  const stat = lstatSync(path, { bigint: true });
  assert(stat.isDirectory() && stat.uid === BigInt(process.getuid()) && (stat.mode & 0o7777n) === 0o700n,
    `${label}_mode`);
  return [stat.dev, stat.ino];
}

function readReport(path) {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = fstatSync(fd, { bigint: true });
    assert(before.isFile() && before.nlink === 1n && before.uid === BigInt(process.getuid())
      && before.size > 0n && before.size <= BigInt(MAX_BYTES) && (before.mode & 0o022n) === 0n,
    "vitest_report_custody");
    const bytes = Buffer.alloc(Number(before.size));
    for (let pos = 0; pos < bytes.length;) {
      const count = readSync(fd, bytes, pos, bytes.length - pos, pos);
      assert(count > 0, "vitest_report_length");
      pos += count;
    }
    const identity = ({ dev, ino, uid, nlink, mode, size, mtimeNs, ctimeNs }) => (
      [dev, ino, uid, nlink, mode, size, mtimeNs, ctimeNs]
    );
    assert.deepEqual(identity(fstatSync(fd, { bigint: true })), identity(before), "vitest_report_fd_drift");
    assert.deepEqual(identity(lstatSync(path, { bigint: true })), identity(before), "vitest_report_path_drift");
    return bytes;
  } finally {
    closeSync(fd);
  }
}

// Call only inside the scheduler-admitted, network/write-fenced launcher.
// The caller must bind the source and installed dependencies, retain the
// returned evidence before canonical cleanup, and prove process/root closure.
// No CLI or standalone admission path is provided here.
export function runFocusedVitest() {
  const root = process.env.BLUEY_TEST_RUN_ROOT;
  assert(typeof root === "string" && RUN_ROOT.test(root) && realpathSync(root) === root, "vitest_owned_root");
  const rootStat = lstatSync(root);
  assert(rootStat.isDirectory() && rootStat.uid === process.getuid() && (rootStat.mode & 0o7777) === 0o700,
    "vitest_owned_root_mode");
  const jobs = join(root, "runner-gate-source/jobs");
  const workspace = join(jobs, "runner");
  const vitest = join(jobs, "node_modules/vitest");
  const cli = join(vitest, "vitest.mjs");
  assert.equal(realpathSync(workspace), workspace, "vitest_workspace_path");
  assert.equal(realpathSync(cli), cli, "vitest_cli_path");
  assert.equal(JSON.parse(readFileSync(join(vitest, "package.json"), "utf8")).version, "4.1.12", "vitest_version");
  const tmp = join(root, "tmp");
  assert.equal(process.env.TMPDIR, tmp, "vitest_owned_tmp");
  const tmpIdentity = privateDirectoryIdentity(tmp, "vitest_tmp");
  const home = mkdtempSync(join(root, "focused-vitest-home-"));
  const env = {
    PATH: "/usr/bin:/bin", HOME: home, TMPDIR: tmp, LANG: "C", TZ: "UTC", CI: "true",
    PLAYWRIGHT_BROWSERS_PATH: join(home, "no-browser"), PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: "1",
  };

  const output = mkdtempSync(join(root, "focused-vitest-"));
  const parentIdentity = () => privateDirectoryIdentity(output, "vitest_report_parent");
  const parent = parentIdentity();
  const path = join(output, "report.json");
  assert.throws(() => lstatSync(path), { code: "ENOENT" });
  const args = [cli, "run", ...Object.keys(FOCUSED_RUNNER_CASES), "--pool=threads", "--maxWorkers=2",
    "--reporter=default", "--reporter=json", `--outputFile=${path}`];
  const result = spawnSync(process.execPath, args, {
    cwd: workspace, env, timeout: 120_000, killSignal: "SIGKILL", maxBuffer: MAX_BYTES,
  });
  const terminal = { code: result.status, signal: result.signal, error: result.error?.code ?? null };
  const capture = (bytes) => ({
    bytes: Buffer.isBuffer(bytes) ? Buffer.from(bytes.subarray(0, MAX_BYTES)) : null,
    truncated: Buffer.isBuffer(bytes) && bytes.length > MAX_BYTES,
  });
  const streams = { stdout: capture(result.stdout), stderr: capture(result.stderr) };
  const streamsComplete = !result.error && terminal.signal === null
    && Object.values(streams).every(({ bytes, truncated }) => bytes !== null && !truncated);
  let report;
  let reportStatus = "unavailable";
  try {
    assert.deepEqual(privateDirectoryIdentity(tmp, "vitest_tmp"), tmpIdentity, "vitest_tmp_drift");
    assert.deepEqual(parentIdentity(), parent, "vitest_report_parent_drift");
    const bytes = readReport(path);
    assert.deepEqual(parentIdentity(), parent, "vitest_report_parent_drift");
    report = bytes;
    reportStatus = "captured";
  } catch (error) {
    reportStatus = error?.code === "ENOENT" ? "missing" : "custody_rejected";
  }
  const fail = (reason) => {
    const error = new Error(reason);
    failedRuns.set(error, { terminal, streams, streamsComplete, reportStatus, report });
    throw error;
  };
  if (terminal.code !== 0 || terminal.signal !== null || result.error) fail("vitest_terminal");
  if (!report) fail(`vitest_report_${reportStatus}`);
  if (!streamsComplete) fail("vitest_streams");
  const stdout = streams.stdout.bytes.toString("utf8");
  const stderr = streams.stderr.bytes.toString("utf8");
  try {
    const proof = verifyFocusedVitestReport({ report, terminal, stdout, stderr, workspace });
    return { ...proof, report, terminal, stdout, stderr, workspace, args };
  } catch {
    fail("vitest_report_verification");
  }
}
