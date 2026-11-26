import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const stages = new Set([
  "setup",
  "build",
  "fixture_server",
  "browser_launch",
  "browser_endpoint",
  "browser_connect",
  "navigation",
  "document_import",
  "validation",
  "cleanup",
  "closure",
  "complete",
]);

const launchCodes = new Map([
  ["ENOENT", "launch_missing_path"],
  ["EACCES", "launch_access_denied"],
  ["EPERM", "launch_permission_denied"],
  ["EAGAIN", "launch_resource_unavailable"],
  ["EMFILE", "launch_descriptor_limit"],
  ["EADDRINUSE", "launch_address_in_use"],
]);
const codes = new Set([
  "assertion_failure", "timeout", "runtime_error", "signal", ...launchCodes.values(),
  "launch_shared_library_reported", "launch_sandbox_refusal_reported", "launch_process_exit_observed",
]);

export function assertDocumentImportStage(stage) {
  if (!stages.has(stage)) throw new Error("document_import_stage_refused");
  return stage;
}

// Keep the launch-only control and the full fixture on the exact same call.
export function launchDocumentImportBrowser(chromium, executablePath) {
  return chromium.launchServer({
    host: "127.0.0.1",
    executablePath,
    headless: true,
    chromiumSandbox: true,
    timeout: 15_000,
  });
}

function classifyFailure(error, stage) {
  try {
    if (error?.code === "ERR_ASSERTION") return "assertion_failure";
    if (error?.name === "TimeoutError" ||
        (typeof error?.message === "string" && error.message.startsWith("timeout_"))) return "timeout";
    if (stage === "browser_launch") {
      // Native errors may be typed or wrapped in a Playwright launch error.
      // Match only closed OS codes in the launch prefix, not arbitrary logs.
      const typed = launchCodes.get(error?.code);
      if (typed) return typed;
      const message = typeof error?.message === "string" ? error.message : "";
      const heading = message.slice(0, 512);
      if (/^browserType\.launchServer: Timeout [1-9][0-9]{0,6}ms exceeded\.(?:\r?\n|$)/u.test(heading)) return "timeout";
      const wrapped = /^browserType\.launchServer: Failed to launch: Error: spawn [^\r\n]+ ([A-Z][A-Z0-9]+)(?:\r?\n|$)/u.exec(heading);
      const classified = wrapped && launchCodes.get(wrapped[1]);
      if (classified) return classified;
      if (heading.startsWith("browserType.launchServer:")) {
        // These are observations from a bounded Playwright launch log, not a
        // conclusion about the host or a reason to relax the sandbox.
        const launchLog = message.slice(0, 16_384);
        if (/^\[pid=[0-9]+\]\[err\] [^\r\n]*: error while loading shared libraries:/mu.test(launchLog)) {
          return "launch_shared_library_reported";
        }
        if (/^\[pid=[0-9]+\]\[err\] [^\r\n]*No usable sandbox!/mu.test(launchLog)) {
          return "launch_sandbox_refusal_reported";
        }
        if (/^\[pid=[0-9]+\] <process did exit: exitCode=(?:[0-9]+|null), signal=(?:SIG[A-Z]+|null)>\r?$/mu.test(launchLog)) {
          return "launch_process_exit_observed";
        }
      }
    }
  } catch {}
  return "runtime_error";
}

function failureRecord(stage, code) {
  assertDocumentImportStage(stage);
  if (!codes.has(code)) throw new Error("document_import_failure_code_refused");
  return { stage, code };
}

function preservedFailure(primaryCaught, primaryFailure, cleanupCaught, cleanupFailure) {
  const error = new Error("document_import_qualification_failed");
  Object.defineProperties(error, {
    primaryCaught: { value: primaryCaught },
    primaryFailure: { value: primaryFailure },
    cleanupCaught: { value: cleanupCaught },
    cleanupFailure: { value: cleanupFailure },
  });
  return error;
}

export async function runDocumentImportLifecycle({ run, cleanup, currentStage, emit }) {
  let primaryCaught = false;
  let primaryFailure;
  let primaryStage;
  try {
    await run();
  } catch (error) {
    primaryCaught = true;
    primaryFailure = error;
    try { primaryStage = assertDocumentImportStage(currentStage()); }
    catch { primaryStage = "setup"; }
  }

  let cleanupCaught = false;
  let cleanupFailure;
  try {
    await cleanup();
  } catch (error) {
    cleanupCaught = true;
    cleanupFailure = error;
  }

  if (
    primaryStage === "cleanup"
    && primaryCaught
    && cleanupCaught
    && (typeof primaryFailure === "object" || typeof primaryFailure === "function")
    && primaryFailure !== null
    && primaryFailure === cleanupFailure
  ) {
    primaryCaught = false;
    primaryFailure = undefined;
    primaryStage = undefined;
  }

  if (!primaryCaught && !cleanupCaught) return;
  emit({
    event: "document_import_failure_v1",
    primary: primaryCaught ? failureRecord(primaryStage, classifyFailure(primaryFailure, primaryStage)) : null,
    cleanup: cleanupCaught ? failureRecord("cleanup", classifyFailure(cleanupFailure)) : null,
  });
  throw preservedFailure(primaryCaught, primaryFailure, cleanupCaught, cleanupFailure);
}

export async function reportDocumentImportSignal({ signalStage, cleanup, emit }) {
  let cleanupCaught = false;
  let cleanupFailure;
  try {
    await cleanup();
  } catch (error) {
    cleanupCaught = true;
    cleanupFailure = error;
  }
  emit({
    event: "document_import_failure_v1",
    primary: failureRecord(assertDocumentImportStage(signalStage), "signal"),
    cleanup: cleanupCaught ? failureRecord("cleanup", classifyFailure(cleanupFailure)) : null,
  });
}

async function selfTest() {
  const launchSentinel = {};
  assert.equal(launchDocumentImportBrowser({
    launchServer(options) {
      assert.deepEqual(options, {
        host: "127.0.0.1", executablePath: "/synthetic/chromium", headless: true,
        chromiumSandbox: true, timeout: 15_000,
      });
      return launchSentinel;
    },
  }, "/synthetic/chromium"), launchSentinel);
  const secretPrimary = new Error("private primary details");
  const secretCleanup = new Error("private cleanup details");
  const emitted = [];
  let caught;
  try {
    await runDocumentImportLifecycle({
      run: async () => { throw secretPrimary; },
      cleanup: async () => { throw secretCleanup; },
      currentStage: () => "browser_connect",
      emit: (record) => emitted.push(record),
    });
  } catch (error) { caught = error; }
  assert.equal(caught.primaryCaught, true);
  assert.equal(caught.primaryFailure, secretPrimary);
  assert.equal(caught.cleanupCaught, true);
  assert.equal(caught.cleanupFailure, secretCleanup);
  assert.deepEqual(emitted, [{
    event: "document_import_failure_v1",
    primary: { stage: "browser_connect", code: "runtime_error" },
    cleanup: { stage: "cleanup", code: "runtime_error" },
  }]);
  assert.equal(JSON.stringify(emitted).includes("private"), false);

  const launchTimeout = Object.assign(new Error("private Playwright browser launch details"), { name: "TimeoutError" });
  const launchFailure = [];
  await assert.rejects(runDocumentImportLifecycle({
    run: async () => { throw launchTimeout; },
    cleanup: async () => {},
    currentStage: () => "browser_launch",
    emit: (record) => launchFailure.push(record),
  }), (error) => error.primaryFailure === launchTimeout);
  assert.deepEqual(launchFailure, [{
    event: "document_import_failure_v1",
    primary: { stage: "browser_launch", code: "timeout" },
    cleanup: null,
  }]);
  assert.equal(JSON.stringify(launchFailure).includes("private"), false);

  for (const [stage, error, code] of [
    ["browser_launch", Object.assign(new Error("private native launch details"), { code: "EAGAIN" }), "launch_resource_unavailable"],
    ["browser_launch", new Error("browserType.launchServer: Failed to launch: Error: spawn /private/path EACCES\nlog"), "launch_access_denied"],
    ["browser_launch", new Error("browserType.launchServer: Timeout 15000ms exceeded.\nprivate log"), "timeout"],
    ["browser_launch", new Error("browserType.launchServer: closed\n[pid=123][err] /private/path: error while loading shared libraries: example.so"), "launch_shared_library_reported"],
    ["browser_launch", new Error("browserType.launchServer: closed\n[pid=123][err] private diagnostic No usable sandbox!"), "launch_sandbox_refusal_reported"],
    ["browser_launch", new Error("browserType.launchServer: closed\n[pid=123] <process did exit: exitCode=1, signal=null>"), "launch_process_exit_observed"],
    ["browser_launch", new Error("private arbitrary log mentioning EACCES"), "runtime_error"],
    ["browser_endpoint", Object.assign(new Error("private endpoint details"), { code: "EADDRINUSE" }), "runtime_error"],
    ["browser_launch", Object.assign(new Error("private unknown code"), { code: "UNKNOWN" }), "runtime_error"],
  ]) {
    const reports = [];
    await assert.rejects(runDocumentImportLifecycle({
      run: async () => { throw error; },
      cleanup: async () => {},
      currentStage: () => stage,
      emit: (record) => reports.push(record),
    }), (failure) => failure.primaryFailure === error);
    assert.deepEqual(reports, [{
      event: "document_import_failure_v1",
      primary: { stage, code },
      cleanup: null,
    }]);
    assert.equal(JSON.stringify(reports).includes("private"), false);
  }

  const success = [];
  await runDocumentImportLifecycle({
    run: async () => {},
    cleanup: async () => {},
    currentStage: () => "complete",
    emit: (record) => success.push(record),
  });
  assert.deepEqual(success, []);

  const cleanupOnly = [];
  await assert.rejects(
    runDocumentImportLifecycle({
      run: async () => {},
      cleanup: async () => { throw new Error("timeout_http_close"); },
      currentStage: () => "cleanup",
      emit: (record) => cleanupOnly.push(record),
    }),
    (error) => !error.primaryCaught && error.cleanupCaught && error.cleanupFailure?.message === "timeout_http_close",
  );
  assert.deepEqual(cleanupOnly, [{
    event: "document_import_failure_v1",
    primary: null,
    cleanup: { stage: "cleanup", code: "timeout" },
  }]);

  const cachedCleanupFailure = new Error("closed cleanup detail");
  const cachedCleanup = async () => { throw cachedCleanupFailure; };
  const cachedCleanupRecords = [];
  await assert.rejects(runDocumentImportLifecycle({
    run: cachedCleanup,
    cleanup: cachedCleanup,
    currentStage: () => "cleanup",
    emit: (record) => cachedCleanupRecords.push(record),
  }));
  assert.deepEqual(cachedCleanupRecords, [{
    event: "document_import_failure_v1",
    primary: null,
    cleanup: { stage: "cleanup", code: "runtime_error" },
  }]);

  for (const falsy of [undefined, null, false]) {
    const falsyRecords = [];
    await assert.rejects(runDocumentImportLifecycle({
      run: async () => { throw falsy; },
      cleanup: async () => { throw falsy; },
      currentStage: () => { throw new Error("invalid stage getter"); },
      emit: (record) => falsyRecords.push(record),
    }), (error) => error.primaryCaught && error.cleanupCaught);
    assert.deepEqual(falsyRecords, [{
      event: "document_import_failure_v1",
      primary: { stage: "setup", code: "runtime_error" },
      cleanup: { stage: "cleanup", code: "runtime_error" },
    }]);
  }

  const signal = [];
  await reportDocumentImportSignal({
    signalStage: "navigation",
    cleanup: async () => {},
    emit: (record) => signal.push(record),
  });
  assert.deepEqual(signal, [{
    event: "document_import_failure_v1",
    primary: { stage: "navigation", code: "signal" },
    cleanup: null,
  }]);

  await assert.rejects(
    runDocumentImportLifecycle({
      run: async () => { throw Object.assign(new Error("hidden"), { code: "ERR_ASSERTION" }); },
      cleanup: async () => {},
      currentStage: () => "validation",
      emit: (record) => assert.equal(record.primary.code, "assertion_failure"),
    }),
    /document_import_qualification_failed/,
  );
  assert.throws(() => assertDocumentImportStage("unknown"), /document_import_stage_refused/);
  console.log("document_import_lifecycle_self_test_passed=true");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href && process.argv[2] === "--self-test") {
  await selfTest();
}
