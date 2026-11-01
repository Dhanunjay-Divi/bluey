#!/usr/bin/env node
// Run only inside the canonical launcher and fresh private PostgreSQL harness,
// after compiling exact source with the debug integration-test-support feature.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, lstat, mkdir, readFile, readdir, realpath } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { build, stop as stopEsbuild } from "esbuild";
import { assertAutoSubmitPreparedRepeatReport, assertAutoSubmitRepeatCausalRedReport, assertCausalRedReport, assertCheckpointPreparedRecoveryReport, assertCheckpointRecoveryCausalRedReport, assertPreparedLifecycleReport, assertPreparedProtocolNegativeReport } from "./nonprod-runner-control-plane.mjs";

assert.ok(process.argv.length === 3 && ["--expect-target-missing", "--prepared-lifecycle", "--expect-auto-submit-repeat-conflict", "--auto-submit-prepared-repeat", "--expect-checkpoint-session-missing", "--checkpoint-prepared-recovery", "--auto-submit-checkpoint-recovery", "--prepared-protocol-negatives", "--auto-submit-protocol-negatives", "--preflight-client"].includes(process.argv[2]),
  "explicit_qualification_scenario_required");
const preflightOnly = process.argv[2] === "--preflight-client";
// Bind expected mode to the invocation, never to the untrusted report's own claimed mode.
const protocolNegativeMode = process.argv[2] === "--prepared-protocol-negatives" ? "review_first"
  : process.argv[2] === "--auto-submit-protocol-negatives" ? "auto_submit" : null;
const jobs = fileURLToPath(new URL("..", import.meta.url));
const repo = resolve(jobs, "..");
const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT || "/missing-owned-root");
assert.equal(root, process.env.BLUEY_TEST_RUN_ROOT, "canonical_launcher_root");
assert.match(root, /^\/(?:private\/)?tmp\/bluey-test-runs-[^/]+\/run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$/);
assert.equal((await lstat(root)).mode & 0o7777, 0o700, "private_launcher_root");
assert.equal((await lstat(root)).uid, process.getuid(), "owned_launcher_root");
assert.equal(resolve(process.env.CARGO_TARGET_DIR), join(root, "cargo-target"), "private_cargo_target");
assert.equal(process.env.BLUEY_DATABASE_URL, undefined, "no_owner_database");
const home = await realpath(process.env.HOME);
assert.ok(home.startsWith(root + "/"), "private_home");
const database = new URL(process.env.BLUEY_TEST_POSTGRES_URL || "invalid:");
assert.equal(database.hostname, "127.0.0.1", "private_pg_host");
assert.equal(database.pathname, "/bluey_jobs_nonprod", "private_pg_database");
const hashes = new Map();
async function snapshot(path) {
  const info = await lstat(path);
  assert.equal(info.isSymbolicLink(), false, "source_symlink_forbidden");
  if (info.isDirectory()) {
    for (const name of (await readdir(path)).sort()) await snapshot(join(path, name));
  } else if (info.isFile()) hashes.set(path, createHash("sha256").update(await readFile(path)).digest("hex"));
}
for (const path of ["automation/src", "runner/src", "runner/nonprod", "scripts", "automation/package.json",
  "runner/package.json", "package.json", "package-lock.json"]) await snapshot(join(jobs, path));
for (const path of ["src", "examples", "Cargo.toml"]) await snapshot(join(repo, "server", path));
await snapshot(join(repo, "Cargo.toml"));
await snapshot(join(repo, "Cargo.lock"));
const work = join(root, preflightOnly ? "runner-control-plane-client-preflight" : "runner-control-plane");
await mkdir(work, { mode: 0o700 });
const adapter = join(work, "control-plane-client.mjs");
// Resolve workspace source, not possibly stale dist output. All output is private.
const automation = join(jobs, "automation/src");
const automationExports = await readFile(join(automation, "index.ts"), "utf8");
assert.ok(automationExports.includes('export * from "./final-submit-proof.js";'), "production_proof_export_changed");
try { await build({ entryPoints: [join(jobs, "runner/nonprod/control-plane-client.mjs")], outfile: adapter,
  platform: "node", target: "node24", format: "esm", bundle: true, treeShaking: true, logLevel: "silent",
  plugins: [{ name: "exact-automation-source", setup(plugin) {
    plugin.onResolve({ filter: /^@bluey\/jobs-automation(?:\/|$)/ }, (args) => {
      const suffix = args.path.slice("@bluey/jobs-automation".length);
      assert.ok(["", "/worker-auth", "/managed-cloud-execution", "/managed-cloud-runtime-client"].includes(suffix),
        "unexpected_workspace_export");
      // The lease client's sole runtime root export is assertFinalSubmitProof.
      // Resolve its actual production re-export module, not a substitute/mock;
      // the unrelated top-level document exports eagerly initialize PDF.js.
      if (!suffix) assert.equal(args.importer, join(jobs, "runner/src/execution-lease.ts"), "unexpected_root_export_importer");
      return { path: join(automation, suffix ? suffix.slice(1) + ".ts" : "final-submit-proof.ts") };
    });
  } }],
}); } finally { stopEsbuild(); }
await chmod(adapter, 0o600);
const imported = await import(pathToFileURL(adapter).href);
assert.equal(typeof imported.createFixtureLeaseClient, "function", "compiled_client_import");
assert.equal(typeof imported.fixtureWorkerAuthHeaders, "function", "compiled_worker_auth_import");
if (preflightOnly) {
  assert.throws(() => imported.createFixtureLeaseClient({ claimFault: "unexpected" }),
    (error) => error?.code === "ERR_ASSERTION" && error.message === "fixture_claim_fault",
    "unknown_claim_fault_rejected_before_private_keys");
  for (const [path, digest] of hashes) {
    assert.equal(createHash("sha256").update(await readFile(path)).digest("hex"), digest, "preflight_source_changed");
  }
  process.stdout.write(JSON.stringify({ event: "runner_control_plane_client_preflight", sourceFiles: hashes.size,
    exactProductionExport: true, sourceUnchanged: true, sourceAccepted: false }) + "\n");
  process.exit(0);
}
const artifacts = [];
for (const name of ["bluey-jobs-api", "examples/jobs-nonprod-runner-control-plane-fixture"]) {
  const path = join(root, "cargo-target/debug", name);
  const info = await lstat(path);
  assert.ok(info.isFile() && !info.isSymbolicLink() && info.uid === process.getuid()
    && (info.mode & 0o022) === 0 && (info.mode & 0o100) !== 0, "owned_binary_identity");
  artifacts.push({ name, sha256: createHash("sha256").update(await readFile(path)).digest("hex") });
}
const env = { PATH: "/opt/homebrew/bin:/usr/bin:/bin", HOME: home, LANG: "C", TZ: "UTC",
  TMPDIR: process.env.TMPDIR, TMP: process.env.TMP, TEMP: process.env.TEMP,
  SQLITE_TMPDIR: process.env.SQLITE_TMPDIR, BLUEY_TEST_RUN_ROOT: root,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR, BLUEY_TEST_POSTGRES_URL: database.href };
// Import-safe module is entered in a new process with a closed environment.
const entry = fileURLToPath(new URL("./nonprod-runner-control-plane.mjs", import.meta.url));
process.stdout.write(JSON.stringify({ event: "runner_control_plane_reproduction_start", binaries: artifacts,
  sourceFiles: hashes.size, sourceAccepted: false }) + "\n");
const child = spawn(process.execPath, [entry, process.argv[2]],
  { env, stdio: "inherit" });
const completion = new Promise((done) => {
  child.once("error", () => done({ code: null, signal: "spawn_error" }));
  child.once("close", (code, signal) => done({ code, signal }));
});
let interrupted = false;
const interrupt = () => { interrupted = true; child.kill("SIGTERM"); };
for (const signal of ["SIGHUP", "SIGINT", "SIGTERM"]) process.on(signal, interrupt);
const timer = setTimeout(() => { interrupted = true; child.kill("SIGTERM"); }, 120_000);
const killTimer = setTimeout(() => child.kill("SIGKILL"), 135_000);
let outcome;
try { outcome = await completion; }
finally {
  clearTimeout(timer); clearTimeout(killTimer);
  for (const signal of ["SIGHUP", "SIGINT", "SIGTERM"]) process.off(signal, interrupt);
}
assert.equal(interrupted, false, "reproduction_interrupted_or_timed_out");
assert.deepEqual(outcome, { code: 0, signal: null }, "reproduction_process_clean_exit");
for (const [path, digest] of hashes) {
  assert.equal(createHash("sha256").update(await readFile(path)).digest("hex"), digest, "source_changed_during_run");
}
const causalRed = process.argv[2] === "--expect-target-missing";
const autoRepeatRed = process.argv[2] === "--expect-auto-submit-repeat-conflict";
const autoRepeatGreen = process.argv[2] === "--auto-submit-prepared-repeat";
const recoveryRed = process.argv[2] === "--expect-checkpoint-session-missing";
const recoveryGreen = ["--checkpoint-prepared-recovery", "--auto-submit-checkpoint-recovery"].includes(process.argv[2]);
const reportPath = join(root, `runner-control-plane-${causalRed ? "causal-red" : autoRepeatRed ? "auto-submit-repeat-red"
  : autoRepeatGreen ? "auto-submit-prepared-repeat" : recoveryRed ? "checkpoint-session-red"
    : recoveryGreen ? "checkpoint-prepared-recovery" : protocolNegativeMode !== null
      ? "prepared-protocol-negatives" : "prepared-lifecycle"}.json`);
const info = await lstat(reportPath);
assert.ok(info.isFile() && !info.isSymbolicLink() && info.nlink === 1 && info.uid === process.getuid()
  && (info.mode & 0o7777) === 0o600 && info.size <= 16 * 1024, "causal_report_identity");
const report = JSON.parse(await readFile(reportPath, "utf8"));
if (causalRed) assertCausalRedReport(report); else if (autoRepeatRed) assertAutoSubmitRepeatCausalRedReport(report);
else if (autoRepeatGreen) assertAutoSubmitPreparedRepeatReport(report);
else if (recoveryRed) assertCheckpointRecoveryCausalRedReport(report);
else if (recoveryGreen) assertCheckpointPreparedRecoveryReport(report,
  process.argv[2] === "--auto-submit-checkpoint-recovery" ? "auto_submit" : "review_first");
else if (protocolNegativeMode !== null) assertPreparedProtocolNegativeReport(report, protocolNegativeMode);
else assertPreparedLifecycleReport(report);
process.stdout.write(JSON.stringify({ event: causalRed ? "runner_control_plane_red_confirmed" : autoRepeatRed
  ? "runner_control_plane_auto_repeat_red_confirmed" : autoRepeatGreen
    ? "runner_control_plane_auto_prepared_repeat_confirmed" : recoveryRed
      ? "runner_control_plane_checkpoint_session_red_confirmed" : recoveryGreen
        ? "runner_control_plane_checkpoint_prepared_recovery_confirmed" : protocolNegativeMode !== null
          ? "runner_control_plane_prepared_protocol_negatives_confirmed" : "runner_control_plane_prepared_confirmed", sourceFiles: hashes.size,
  sourceUnchanged: true, sourceAccepted: false }) + "\n");
