#!/usr/bin/env node

// Run after a fresh server --bins build, inside the owned PostgreSQL harness
// and canonical launcher. No owner URL, profile, OAuth key or provider is used.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { assertBrowserApiReport } from "./nonprod-browser-api-report.mjs";

assert.equal(process.argv.length, 2);
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
const sourceHashes = new Map();
function snapshot(path) {
  const info = lstatSync(path);
  assert(!info.isSymbolicLink());
  if (info.isDirectory()) {
    for (const name of readdirSync(path).sort()) snapshot(join(path, name));
  } else if (info.isFile()) {
    sourceHashes.set(path, createHash("sha256").update(readFileSync(path)).digest("hex"));
  }
}
for (const path of ["portal/src", "portal/index.html", "portal/package.json", "package.json", "package-lock.json"]) {
  snapshot(join(jobs, path));
}
for (const path of [
  "portal/nonprod/browser-api.integration.mjs",
  "scripts/run-nonprod-browser-api-tests.mjs",
  "scripts/nonprod-browser-api-report.mjs",
]) snapshot(join(jobs, path));
const artifacts = [];
for (const name of ["bluey-jobs-api", "bluey-server"]) {
  const binary = join(process.env.CARGO_TARGET_DIR, "debug", name);
  const info = lstatSync(binary);
  assert(info.isFile() && !info.isSymbolicLink() && info.uid === process.getuid());
  artifacts.push({ name, sha256: createHash("sha256").update(readFileSync(binary)).digest("hex") });
}
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
console.log(JSON.stringify({ event: "browser_api_chromium_install_start" }));
execFileSync(process.execPath, [join(jobs, "node_modules/playwright/cli.js"), "install", "chromium"], {
  env, timeout: 180_000, maxBuffer: 2 * 1024 * 1024,
});
const childEnv = { ...env,
  BLUEY_TEST_RUN_ROOT: root, BLUEY_NONPROD_ROOT: root,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR,
  BLUEY_TEST_POSTGRES_URL: process.env.BLUEY_TEST_POSTGRES_URL,
  BLUEY_POSTGRES_BIN: "/opt/homebrew/opt/postgresql@17/bin",
  BLUEY_PORTAL_DIST: dist,
};
const child = spawn(process.execPath, [join(jobs, "portal/nonprod/browser-api.integration.mjs")], {
  env: childEnv, stdio: "inherit",
});
const completion = new Promise((resolveClose, reject) => {
  child.once("error", reject);
  child.once("close", (code, signal) => resolveClose({ code, signal }));
});
let interrupted = false;
const stop = () => { interrupted = true; child.kill("SIGTERM"); };
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(signal, stop);
const timeout = setTimeout(stop, 240_000);
try {
  const closed = await completion;
  assert(!interrupted);
  assert.deepEqual(closed, { code: 0, signal: null });
  const path = join(root, "browser-api-result.json");
  const reportInfo = lstatSync(path);
  assert(reportInfo.isFile() && !reportInfo.isSymbolicLink());
  assert.equal(reportInfo.uid, process.getuid());
  assert.equal(reportInfo.mode & 0o777, 0o600);
  assert(reportInfo.size > 0 && reportInfo.size < 64 * 1024);
  const report = JSON.parse(readFileSync(path, "utf8"));
  assertBrowserApiReport(report);
  for (const [path, digest] of sourceHashes) {
    assert.equal(createHash("sha256").update(readFileSync(path)).digest("hex"), digest,
      "portal source changed during browser qualification");
  }
  console.log(JSON.stringify({ event: "nonprod_browser_api_finished", ...report }));
} finally {
  clearTimeout(timeout);
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.off(signal, stop);
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM");
}
