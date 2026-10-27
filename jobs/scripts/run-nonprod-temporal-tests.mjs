#!/usr/bin/env node

// Test-only local service qualification. Never accepts a service URL, owner
// credentials, arbitrary executable or production activity implementation.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  closeSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync,
  readdirSync, realpathSync, statSync, symlinkSync, writeFileSync, writeSync,
} from "node:fs";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const CLI = Object.freeze({
  version: "1.9.1",
  bytes: 43_195_714,
  sha256: "41e0425378fcb4fb5766340b97435e20fe47bbff2d7bf644ec2d51f7662b7c56",
  url: "https://github.com/temporalio/cli/releases/download/v1.9.1/temporal_cli_1.9.1_darwin_arm64.tar.gz",
});

export function assertRuntimeReport(report, pidExists = processExists) {
  assert.deepEqual(Object.keys(report).sort(),
    ["failed", "latency", "passed", "servers", "tests", "version"]);
  assert.equal(report.version, 1);
  assert(Number.isSafeInteger(report.tests) && report.tests > 0 && report.tests <= 128);
  assert.equal(report.passed, report.tests);
  assert.deepEqual(report.failed, []);
  assert(Array.isArray(report.servers) && report.servers.length > 0 && report.servers.length <= 8);
  for (const server of report.servers) {
    assert.deepEqual(Object.keys(server).sort(), ["code", "pid", "signal"]);
    assert(Number.isSafeInteger(server.pid) && server.pid > 1 && server.pid <= 2_147_483_647);
    assert.equal(server.code, 0, "nonclean Temporal exit cannot pass");
    assert.equal(server.signal, null, "signal termination cannot pass");
    assert.equal(pidExists(server.pid), false, "owned Temporal process still exists");
  }
  assert.deepEqual(Object.keys(report.latency).sort(), ["maxMs", "p50Ms", "p95Ms", "samples"]);
  assert(Number.isSafeInteger(report.latency.samples) && report.latency.samples >= 20);
  const { p50Ms, p95Ms, maxMs } = report.latency;
  assert([p50Ms, p95Ms, maxMs].every((v) => Number.isFinite(v) && v >= 0));
  assert(p50Ms <= p95Ms && p95Ms <= maxMs);
}

function processExists(pid) {
  try {
    process.kill(pid, 0); // Read-only liveness check; never terminate a recorded PID.
    return true;
  } catch (error) {
    if (error.code === "ESRCH") return false;
    throw error; // Permission/inspection ambiguity is not proof of absence.
  }
}

function ownedDirectory(path) {
  const value = lstatSync(path);
  assert(value.isDirectory() && !value.isSymbolicLink());
  assert.equal(value.uid, process.getuid());
  assert.equal(value.mode & 0o777, 0o700);
}

function compileCurrentSource(work, env) {
  const jobs = fileURLToPath(new URL("..", import.meta.url));
  const workspace = join(work, "workspace");
  mkdirSync(workspace, { mode: 0o700 });
  const manifest = [];
  let sourceBytes = 0;
  const copy = (source, target, relative) => {
    const metadata = lstatSync(source);
    assert(!metadata.isSymbolicLink(), "project source symlinks are not admitted");
    if (metadata.isDirectory()) {
      mkdirSync(target, { mode: 0o700 });
      for (const name of readdirSync(source).sort()) {
        copy(join(source, name), join(target, name), `${relative}/${name}`);
      }
      return;
    }
    assert(metadata.isFile());
    const contents = readFileSync(source);
    sourceBytes += contents.length;
    assert(sourceBytes <= 32 * 1024 * 1024 && manifest.length < 2_000);
    const sha256 = createHash("sha256").update(contents).digest("hex");
    manifest.push({ source, path: relative, sha256 });
    writeFileSync(target, contents, { flag: "wx", mode: 0o600 });
  };
  for (const name of ["automation", "workflows"]) {
    mkdirSync(join(workspace, name), { mode: 0o700 });
    for (const entry of ["package.json", "tsconfig.json", "src"]) {
      copy(join(jobs, name, entry), join(workspace, name, entry), `${name}/${entry}`);
    }
  }
  const modules = join(workspace, "node_modules");
  mkdirSync(modules, { mode: 0o700 });
  for (const name of readdirSync(join(jobs, "node_modules")).sort()) {
    if (name.startsWith(".") || name === "@bluey") continue;
    symlinkSync(realpathSync(join(jobs, "node_modules", name)), join(modules, name));
  }
  mkdirSync(join(modules, "@bluey"), { mode: 0o700 });
  for (const name of ["automation", "workflows"]) {
    symlinkSync(join(workspace, name), join(modules, "@bluey", `jobs-${name}`));
  }
  const compiler = realpathSync(join(jobs, "node_modules/typescript/lib/tsc.js"));
  for (const name of ["automation", "workflows"]) {
    execFileSync(process.execPath, [compiler, "-p", join(workspace, name, "tsconfig.json")], {
      cwd: workspace, env, timeout: 60_000, maxBuffer: 1024 * 1024,
    });
  }
  for (const entry of manifest) {
    assert.equal(createHash("sha256").update(readFileSync(entry.source)).digest("hex"), entry.sha256,
      "project source changed during isolated compile");
  }
  const sourceManifest = manifest.map(({ path, sha256 }) => ({ path, sha256 }));
  const sourceSha256 = createHash("sha256").update(JSON.stringify(sourceManifest)).digest("hex");
  const lockSha256 = createHash("sha256").update(readFileSync(join(jobs, "package-lock.json"))).digest("hex");
  writeFileSync(join(work, "source-manifest.json"), JSON.stringify({
    sourceSha256, lockSha256, files: sourceManifest,
  }), { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify({ event: "temporal_source_compiled", files: sourceManifest.length,
    sourceSha256, lockSha256 }));
  return { workspace, manifest, lockSha256, lockPath: join(jobs, "package-lock.json") };
}

async function downloadArchive(path) {
  const response = await fetch(CLI.url, { signal: AbortSignal.timeout(90_000) });
  assert(response.ok && response.body, "official CLI download failed");
  assert(["github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com"]
    .includes(new URL(response.url).hostname), "unexpected release download host");
  const descriptor = openSync(path, "wx", 0o600);
  const digest = createHash("sha256");
  let size = 0;
  try {
    for await (const chunk of response.body) {
      size += chunk.length;
      assert(size <= CLI.bytes, "CLI archive exceeds pinned size");
      digest.update(chunk);
      let offset = 0;
      while (offset < chunk.length) offset += writeSync(descriptor, chunk, offset);
    }
  } finally {
    closeSync(descriptor);
  }
  assert.equal(size, CLI.bytes, "CLI archive size differs from official release");
  assert.equal(digest.digest("hex"), CLI.sha256, "CLI archive checksum mismatch");
}

async function main() {
  assert.equal(process.argv.length, 2, "this harness accepts no service URLs or extra options");
  assert.equal(process.platform, "darwin", "this pinned qualification is Darwin arm64 only");
  assert.equal(process.arch, "arm64", "this pinned qualification is Darwin arm64 only");
  for (const key of Object.keys(process.env)) {
    assert(!key.startsWith("TEMPORAL_"), "inherited Temporal configuration is forbidden");
  }
  assert.equal(process.env.BLUEY_DATABASE_URL, undefined);
  assert.equal(process.env.BLUEY_TEST_POSTGRES_URL, undefined);
  assert.equal(process.env.NODE_OPTIONS, undefined, "inherited Node hooks are forbidden");
  const root = realpathSync(process.env.BLUEY_TEST_RUN_ROOT || "/missing-owned-test-root");
  assert(root.startsWith("/private/tmp/bluey-test-runs-") || root.startsWith("/tmp/bluey-test-runs-"));
  ownedDirectory(root);
  assert.equal(resolve(process.env.CARGO_TARGET_DIR), join(root, "cargo-target"));
  assert.equal(process.env.BLUEY_SERVER_DB_BACKEND, "sqlite");
  const marker = join(root, ".bluey-test-run-v2");
  assert(!lstatSync(marker).isSymbolicLink());
  assert(statSync(marker).size < 4_096);
  const authority = readFileSync(marker, "utf8").split("\n");
  assert.equal(authority[0], "magic=bluey-test-run-v2");
  assert.equal(authority[1], `run_root=${root}`);
  assert.equal(authority[2], `run_name=${basename(root)}`);
  const work = join(root, "nonprod-temporal");
  assert(!existsSync(work), "Temporal workspace must be fresh");
  mkdirSync(work, { mode: 0o700 });
  const home = join(work, "child-home");
  mkdirSync(home, { mode: 0o700 });
  for (const dir of [".config", ".cache", "tmp"]) mkdirSync(join(home, dir), { mode: 0o700 });
  const archive = join(work, "temporal-cli.tar.gz");
  await downloadArchive(archive);
  const executable = join(work, "temporal");
  // Extract only this exact file, never archive paths or links into a directory.
  const bytes = execFileSync("/usr/bin/tar", ["-xOzf", archive, "temporal"], {
    maxBuffer: 256 * 1024 * 1024, timeout: 30_000,
  });
  assert(bytes.length > 1_000_000);
  writeFileSync(executable, bytes, { mode: 0o700, flag: "wx" });
  const env = {
    PATH: "/opt/homebrew/bin:/usr/bin:/bin",
    HOME: home,
    XDG_CONFIG_HOME: join(home, ".config"),
    XDG_CACHE_HOME: join(home, ".cache"),
    TMPDIR: join(home, "tmp"),
    TMP: join(home, "tmp"),
    TEMP: join(home, "tmp"),
    LANG: "en_US.UTF-8",
    TZ: "UTC",
    BLUEY_TEST_RUN_ROOT: root,
    BLUEY_TEMPORAL_RUNTIME_DIR: work,
    BLUEY_TEMPORAL_BIN: executable,
  };
  const version = execFileSync(executable, ["--version"], {
    env, encoding: "utf8", timeout: 10_000, maxBuffer: 4_096,
  });
  assert.match(version, /\b1\.9\.1\b/);
  const compiled = compileCurrentSource(work, env);
  env.BLUEY_TEMPORAL_WORKSPACE_DIR = compiled.workspace;
  console.log(JSON.stringify({ event: "temporal_cli_admitted", version: CLI.version,
    archiveSha256: CLI.sha256, archiveBytes: CLI.bytes }));
  const childScript = fileURLToPath(new URL("../workflows/nonprod/temporal.integration.mjs", import.meta.url));
  const child = spawn(process.execPath, [childScript], { env, stdio: ["ignore", "pipe", "pipe"] });
  const closed = new Promise((resolveClose, reject) => {
    child.once("error", reject);
    child.once("close", (code, signal) => resolveClose({ code, signal }));
  });
  let outputBytes = 0;
  let interrupted = false;
  const interrupt = () => { interrupted = true; child.kill("SIGTERM"); };
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(signal, interrupt);
  const timeout = setTimeout(interrupt, 300_000);
  let escalation;
  const onOutput = (stream) => (chunk) => {
    outputBytes += chunk.length;
    if (outputBytes > 1024 * 1024) { interrupt(); return; }
    stream.write(chunk);
  };
  child.stdout.on("data", onOutput(process.stdout));
  child.stderr.on("data", onOutput(process.stderr));
  // Keep an escalation bound even if a failed test cannot complete cleanup.
  const escalationTimer = setInterval(() => {
    if (interrupted && !escalation) escalation = setTimeout(() => child.kill("SIGKILL"), 20_000);
  }, 250);
  try {
    const result = await closed;
    assert(!interrupted, "runtime tests interrupted or exceeded their bound");
    assert.deepEqual(result, { code: 0, signal: null }, "runtime child did not exit cleanly");
    const reportPath = join(work, "result.json");
    assert(!lstatSync(reportPath).isSymbolicLink());
    assert(statSync(reportPath).size <= 64 * 1024);
    const report = JSON.parse(readFileSync(reportPath, "utf8"));
    assertRuntimeReport(report);
    for (const entry of compiled.manifest) {
      assert.equal(createHash("sha256").update(readFileSync(entry.source)).digest("hex"), entry.sha256,
        "project source changed during runtime qualification");
    }
    assert.equal(createHash("sha256").update(readFileSync(compiled.lockPath)).digest("hex"), compiled.lockSha256,
      "dependency lock changed during runtime qualification");
    console.log(JSON.stringify({ event: "nonprod_temporal_finished", ...report }));
  } finally {
    clearTimeout(timeout);
    clearInterval(escalationTimer);
    clearTimeout(escalation);
    for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.off(signal, interrupt);
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM");
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
