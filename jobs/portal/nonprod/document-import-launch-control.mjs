// Owned Linux launch diagnostic only. No page, product API, or external origin.
import assert from "node:assert/strict";
import { once } from "node:events";
import { access, lstat, readFile, realpath } from "node:fs/promises";
import { constants } from "node:fs";
import { networkInterfaces } from "node:os";
import { chromium } from "playwright";
import {
  launchDocumentImportBrowser,
  reportDocumentImportSignal,
  runDocumentImportLifecycle,
} from "./document-import-lifecycle.mjs";

let stage = "setup";
let browserServer;
let engineCompletion;
let cleanupPromise;
let emitted = false;

function emit(record) {
  if (emitted) return;
  emitted = true;
  process.stderr.write(`${JSON.stringify(record)}\n`);
}

async function bounded(operation, label) {
  let timer;
  try {
    return await Promise.race([operation, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`timeout_${label}`)), 5_000);
    })]);
  } finally { clearTimeout(timer); }
}

function cleanup() {
  return cleanupPromise ??= (async () => {
    if (!browserServer) return;
    let closeFailure;
    try { await bounded(browserServer.close(), "launch_control_close"); } catch (error) {
      closeFailure = error;
      await bounded(browserServer.kill(), "launch_control_kill");
    }
    assert.equal(await bounded(engineCompletion, "launch_control_engine"), true, "engine_closed");
    if (closeFailure) throw closeFailure;
  })();
}

for (const signal of ["SIGTERM", "SIGINT", "SIGHUP"]) {
  process.once(signal, () => {
    void reportDocumentImportSignal({ signalStage: stage, cleanup, emit })
      .then(() => process.exit(1), () => process.exit(1));
  });
}

try {
  await runDocumentImportLifecycle({
    async run() {
      assert.deepEqual(process.argv.slice(2), ["--owned-linux-launch-control"]);
      assert.equal(process.platform, "linux", "owned_linux_only");
      assert.equal(process.arch, "x64", "owned_linux_x64_only");
      assert.ok(process.getuid() > 0, "nonroot_fixture_actor");
      const status = await readFile("/proc/self/status", "utf8");
      assert.match(status, /^NoNewPrivs:\s*1$/mu, "owned_fixture_no_new_privileges");
      assert.match(status, /^CapEff:\s*0+$/mu, "owned_fixture_no_capabilities");
      assert.deepEqual(Object.keys(networkInterfaces()), ["lo"], "owned_fixture_loopback_only");
      const fixtureRoot = process.env.BLUEY_DOCUMENT_FIXTURE_ROOT;
      const executable = process.env.BLUEY_DOCUMENT_CHROMIUM_EXECUTABLE;
      assert.ok(typeof fixtureRoot === "string" && typeof executable === "string", "explicit_fixture_paths");
      const candidate = /^(\/var\/tmp\/bluey-n661-qa-[a-f0-9]{16})(?:\/|$)/u.exec(fixtureRoot)?.[1];
      assert.ok(candidate && executable.startsWith(`${candidate}/`), "same_owned_candidate");
      const rootInfo = await lstat(fixtureRoot);
      assert.ok(rootInfo.isDirectory() && rootInfo.uid === process.getuid() && (rootInfo.mode & 0o077) === 0,
        "private_fixture_root");
      assert.equal(await realpath(fixtureRoot), fixtureRoot, "physical_fixture_root");
      assert.equal(await realpath(executable), executable, "physical_browser_executable");
      assert.equal(executable, chromium.executablePath(), "pinned_playwright_engine_path");
      assert.ok((await lstat(executable)).isFile(), "regular_browser_executable");
      await access(executable, constants.X_OK);
      for (const name of ["HOME", "TMPDIR", "TMP", "TEMP"]) {
        const path = process.env[name];
        assert.ok(typeof path === "string" && path.startsWith(`${fixtureRoot}/`), "private_fixture_env");
        assert.equal(await realpath(path), path, "physical_fixture_env");
      }
      stage = "browser_launch";
      browserServer = await launchDocumentImportBrowser(chromium, executable);
      stage = "browser_endpoint";
      engineCompletion = once(browserServer.process(), "close").then(() => true, () => false);
      const endpoint = new URL(browserServer.wsEndpoint());
      assert.equal(endpoint.protocol, "ws:", "owned_browser_endpoint_protocol");
      assert.ok(["127.0.0.1", "localhost", "[::1]"].includes(endpoint.hostname), "owned_browser_endpoint_host");
      stage = "cleanup";
      await cleanup();
      stage = "complete";
    },
    cleanup,
    currentStage: () => stage,
    emit,
  });
  process.stdout.write(`${JSON.stringify({ event: "document_import_launch_control_v1", launched: true, engineClosed: true, documentImportExercised: false })}\n`);
} catch {
  process.exitCode = 1;
}
