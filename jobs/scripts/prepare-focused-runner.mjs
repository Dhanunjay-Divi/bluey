import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync, mkdtempSync, readFileSync, realpathSync } from "node:fs";
import { dirname, join } from "node:path";
import { focusedVitestFailureEvidence, runFocusedVitest } from "./run-focused-vitest.mjs";

const RUN_ROOT = /^\/private\/tmp\/bluey-test-runs-[A-Za-z0-9_-]+\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/;
const LOCK = "ac64330dca973a22beb4569c96551af683fabd5edb0ee5592c43554ac098b00b";
const NPM = "8e5f6f3429f8cdbe693cdc29904e9d5a7b127a494bd15c804bd54c7403bfcbe7";
const MAX_STREAM = 2 * 1024 * 1024;
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");

function privateDirectory(path) {
  assert.equal(realpathSync(path), path, "focused_prepare_path");
  const stat = lstatSync(path);
  assert(stat.isDirectory() && stat.uid === process.getuid() && (stat.mode & 0o7777) === 0o700,
    "focused_prepare_directory");
  return [stat.dev, stat.ino];
}

export function focusedPreparationPlan(root, npmCli) {
  assert(typeof root === "string" && RUN_ROOT.test(root), "focused_prepare_root");
  const jobs = join(root, "runner-gate-source/jobs");
  const tsc = join(jobs, "node_modules/typescript/bin/tsc");
  return [
    { name: "offline_install", cwd: jobs, args: [npmCli, "ci", "--offline", "--ignore-scripts",
      "--include=dev", "--no-audit", "--no-fund"], timeout: 120_000 },
    { name: "automation_build", cwd: join(jobs, "automation"), args: [tsc, "-p", "tsconfig.json"], timeout: 60_000 },
    { name: "runner_typecheck", cwd: join(jobs, "runner"), args: [tsc, "-p", "tsconfig.json", "--noEmit"], timeout: 60_000 },
  ];
}

// Library only. The scheduler-admitted outer controller must first qualify
// the staged source, exact Node/tool closure and private cache copy, and apply
// the network/write fence. No standalone mode or capacity bypass is provided.
// recordPrivateStep must retain bounded diagnostics before root cleanup.
export async function prepareAndRunFocusedRunner({ recordPrivateStep }) {
  assert.equal(typeof recordPrivateStep, "function", "focused_prepare_evidence_writer");
  const root = process.env.BLUEY_TEST_RUN_ROOT;
  assert(typeof root === "string" && RUN_ROOT.test(root), "focused_prepare_root");
  privateDirectory(root);
  const tmp = join(root, "tmp");
  const cache = join(root, "runner-gate-cache");
  assert.equal(process.env.TMPDIR, tmp, "focused_prepare_tmp");
  const tmpIdentity = privateDirectory(tmp);
  const cacheIdentity = privateDirectory(cache);
  const home = mkdtempSync(join(root, "focused-prepare-home-"));
  const jobs = join(root, "runner-gate-source/jobs");
  const lock = join(jobs, "package-lock.json");
  const npmCli = join(dirname(dirname(process.execPath)), "lib/node_modules/npm/bin/npm-cli.js");
  assert.equal(realpathSync(npmCli), npmCli, "focused_prepare_npm_path");
  assert.equal(sha(readFileSync(npmCli)), NPM, "focused_prepare_npm_pin");
  assert.equal(sha(readFileSync(lock)), LOCK, "focused_prepare_lock_pin");
  for (const path of [join(jobs, "node_modules"), join(jobs, "automation/dist"), join(jobs, ".npmrc")]) {
    assert.throws(() => lstatSync(path), { code: "ENOENT" });
  }
  const env = {
    HOME: home, PATH: `${dirname(process.execPath)}:/usr/bin:/bin`, TMPDIR: tmp, LANG: "C", LC_ALL: "C", TZ: "UTC",
    CI: "true", PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: "1", PLAYWRIGHT_BROWSERS_PATH: join(home, "no-browser"),
    NPM_CONFIG_CACHE: cache, NPM_CONFIG_USERCONFIG: join(home, "no-user-npmrc"),
    NPM_CONFIG_GLOBALCONFIG: join(home, "no-global-npmrc"), NPM_CONFIG_UPDATE_NOTIFIER: "false",
  };
  for (const step of focusedPreparationPlan(root, npmCli)) {
    assert.equal(realpathSync(step.cwd), step.cwd, "focused_prepare_cwd");
    assert.equal(realpathSync(step.args[0]), step.args[0], "focused_prepare_command_path");
    if (step.name === "automation_build") {
      const metadata = JSON.parse(readFileSync(join(jobs, "node_modules/typescript/package.json"), "utf8"));
      assert.equal(metadata.version, "5.9.3", "focused_prepare_typescript_version");
    }
    const result = spawnSync(process.execPath, step.args, {
      cwd: step.cwd, env, timeout: step.timeout, killSignal: "SIGKILL", maxBuffer: MAX_STREAM,
    });
    const terminal = { code: result.status, signal: result.signal, error: result.error?.code ?? null };
    const copy = (value) => Buffer.isBuffer(value) ? Buffer.from(value.subarray(0, MAX_STREAM)) : null;
    const streamsComplete = !result.error && terminal.signal === null && [result.stdout, result.stderr].every(
      (bytes) => Buffer.isBuffer(bytes) && bytes.length <= MAX_STREAM,
    );
    await recordPrivateStep({ name: step.name, terminal, streamsComplete,
      stdout: copy(result.stdout), stderr: copy(result.stderr) });
    assert(terminal.code === 0 && terminal.signal === null && !result.error && streamsComplete,
      `focused_prepare_${step.name}_terminal`);
    assert.deepEqual(privateDirectory(tmp), tmpIdentity, "focused_prepare_tmp_drift");
    assert.deepEqual(privateDirectory(cache), cacheIdentity, "focused_prepare_cache_drift");
    assert.equal(sha(readFileSync(lock)), LOCK, "focused_prepare_lock_drift");
  }
  let proof;
  try {
    proof = runFocusedVitest();
  } catch (error) {
    const evidence = focusedVitestFailureEvidence(error);
    if (evidence) await recordPrivateStep({ name: "focused_vitest", ...evidence });
    throw error;
  }
  await recordPrivateStep({ name: "focused_vitest", ...proof, verified: true });
  return proof;
}
