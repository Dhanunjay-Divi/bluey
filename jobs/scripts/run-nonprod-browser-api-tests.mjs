#!/usr/bin/env node

// Run after a fresh server --bins build, inside the owned PostgreSQL harness
// and canonical launcher. No owner URL, profile, OAuth key or provider is used.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { lstatSync, mkdirSync, readFileSync, realpathSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  assertBrowserApiRunReceipt,
} from "./nonprod-browser-api-report.mjs";
import {
  assertBrowserInputsUnchanged, browserApiSourcePaths, browserPortalSourcePaths, snapshotBrowserInputs,
} from "./nonprod-browser-api-inputs.mjs";

const humanHold = process.argv.length === 3 && process.argv[2] === "--human-hold";
assert(process.argv.length === 2 || humanHold, "expected no arguments or exact --human-hold");
const root = realpathSync(process.env.BLUEY_TEST_RUN_ROOT || "/missing-owned-root");
assert(/^\/(private\/)?tmp\/bluey-test-runs-/.test(root));
assert.equal(lstatSync(root).uid, process.getuid());
assert.equal(lstatSync(root).mode & 0o777, 0o700);
assert.equal(resolve(process.env.CARGO_TARGET_DIR), join(root, "cargo-target"));
assert.equal(process.env.BLUEY_DATABASE_URL, undefined);
const database = new URL(process.env.BLUEY_TEST_POSTGRES_URL || "invalid:");
assert.equal(database.hostname, "127.0.0.1");
const jobs = fileURLToPath(new URL("..", import.meta.url));
const home = realpathSync(process.env.HOME);
assert(home.startsWith(root + "/"));
const work = join(root, "browser-qualification");
mkdirSync(work, { mode: 0o700 });
const browsers = join(work, "chromium");
const dist = join(work, "portal");
mkdirSync(browsers, { mode: 0o700 });
mkdirSync(dist, { mode: 0o700 });
const env = {
  PATH: "/opt/homebrew/bin:/usr/bin:/bin", HOME: home, LANG: "en_US.UTF-8", TZ: "UTC",
  TMPDIR: process.env.TMPDIR, TMP: process.env.TMP, TEMP: process.env.TEMP,
  SQLITE_TMPDIR: process.env.SQLITE_TMPDIR,
  PLAYWRIGHT_BROWSERS_PATH: browsers,
};
const sourcePaths = [...browserPortalSourcePaths(jobs), ...[
  "portal/nonprod/browser-api.integration.mjs",
  "scripts/run-nonprod-browser-api-tests.mjs",
  "scripts/nonprod-browser-api-inputs.mjs",
  "scripts/nonprod-browser-api-report.mjs",
  "scripts/nonprod-object-store.mjs",
].map((path) => join(jobs, path))];
sourcePaths.push(...browserApiSourcePaths(resolve(jobs, "..")));
const sourceInputs = snapshotBrowserInputs(sourcePaths);
const artifactNames = ["bluey-jobs-api", "bluey-server", "examples/jobs-nonprod-browser-fixture"];
const artifactPaths = artifactNames.map((name) => join(process.env.CARGO_TARGET_DIR, "debug", name));
const snapshotArtifacts = () => snapshotBrowserInputs(artifactPaths, { executables: true });
const artifactInputs = snapshotArtifacts();
const artifacts = artifactPaths.map((path, i) => ({ name: artifactNames[i], sha256: artifactInputs.get(path).sha256 }));
console.log(JSON.stringify({ event: "browser_api_build_start", binaries: artifacts }));
// Disable dotenv/config discovery. Keep the same portal plugins/base/target,
// with all build output confined to this private root (never web/jobs).
const buildCode = `import {build} from 'vite';import react from '@vitejs/plugin-react';
import tailwind from '@tailwindcss/vite';await build({configFile:false,envDir:false,
base:'/jobs/',plugins:[react(),tailwind()],build:{outDir:${JSON.stringify(dist)},
emptyOutDir:false,target:'es2022',sourcemap:false}});`;
execFileSync(process.execPath, ["--input-type=module", "-e", buildCode], {
  cwd: join(jobs, "portal"), env, timeout: 120_000, maxBuffer: 2 * 1024 * 1024,
});
const portalInputs = snapshotBrowserInputs([dist]);
console.log(JSON.stringify({ event: "browser_api_chromium_install_start" }));
execFileSync(process.execPath, [join(jobs, "node_modules/playwright/cli.js"), "install", "chromium"], {
  env, timeout: 180_000, maxBuffer: 2 * 1024 * 1024,
});
assertBrowserInputsUnchanged(sourceInputs, snapshotBrowserInputs(sourcePaths));
assertBrowserInputsUnchanged(artifactInputs, snapshotArtifacts());
assertBrowserInputsUnchanged(portalInputs, snapshotBrowserInputs([dist]));
const reportNonce = randomBytes(32).toString("hex");
const childEnv = { ...env,
  BLUEY_NONPROD_BROWSER_REPORT_NONCE: reportNonce,
  BLUEY_TEST_RUN_ROOT: root, BLUEY_NONPROD_ROOT: root,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR,
  BLUEY_TEST_POSTGRES_URL: process.env.BLUEY_TEST_POSTGRES_URL,
  BLUEY_POSTGRES_BIN: "/opt/homebrew/opt/postgresql@17/bin",
  BLUEY_PORTAL_DIST: dist,
  BLUEY_NONPROD_PACKET_FIXTURE_BIN: join(process.env.CARGO_TARGET_DIR,
    "debug/examples/jobs-nonprod-browser-fixture"),
};
const child = spawn(process.execPath, [join(jobs, "portal/nonprod/browser-api.integration.mjs"),
  ...(humanHold ? ["--human-hold"] : [])], {
  env: childEnv, stdio: "inherit",
});
const completion = new Promise((resolveClose, reject) => {
  child.once("error", reject);
  child.once("close", (code, signal) => resolveClose({ code, signal }));
});
let interrupted = false;
let escalation;
const signalHandlers = new Map();
const stop = () => {
  if (interrupted) return;
  interrupted = true;
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM");
  escalation = setTimeout(() => {
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
  }, 45_000);
};
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  const handler = () => stop(); signalHandlers.set(signal, handler); process.on(signal, handler);
}
const timeout = setTimeout(stop, humanHold ? 780_000 : 240_000);
try {
  const closed = await completion;
  assert(!interrupted);
  assert.deepEqual(closed, { code: 0, signal: null });
  const path = join(root, humanHold ? "human-browser-api-result.json" : "browser-api-result.json");
  const reportInfo = lstatSync(path);
  assert(reportInfo.isFile() && !reportInfo.isSymbolicLink());
  assert.equal(reportInfo.uid, process.getuid());
  assert.equal(reportInfo.mode & 0o777, 0o600);
  assert(reportInfo.size > 0 && reportInfo.size < (humanHold ? 16 : 64) * 1024);
  const report = assertBrowserApiRunReceipt(
    JSON.parse(readFileSync(path, "utf8")), reportNonce, humanHold,
  );
  assertBrowserInputsUnchanged(sourceInputs, snapshotBrowserInputs(sourcePaths));
  assertBrowserInputsUnchanged(artifactInputs, snapshotArtifacts());
  assertBrowserInputsUnchanged(portalInputs, snapshotBrowserInputs([dist]));
  if (humanHold) {
    console.log(JSON.stringify({ event: "nonprod_browser_api_finished", mode: report.mode,
      passed: report.passed, cases: report.cases.length, sourceAccepted: false }));
  } else console.log(JSON.stringify({ event: "nonprod_browser_api_finished", ...report }));
} finally {
  clearTimeout(timeout); clearTimeout(escalation);
  for (const [signal, handler] of signalHandlers) process.off(signal, handler);
  if (child.exitCode === null && child.signalCode === null) {
    child.kill("SIGTERM");
    const forced = setTimeout(() => {
      if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    }, 45_000);
    await completion; clearTimeout(forced);
  }
}
