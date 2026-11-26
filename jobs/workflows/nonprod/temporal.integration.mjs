// Disposable real Temporal qualification. Never imports production activities.
import { spawn } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { lstat, realpath, writeFile } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { Client, Connection, WorkflowNotFoundError, isGrpcServiceError } from "@temporalio/client";
import { ApplicationFailure } from "@temporalio/common";
import { DefaultLogger, NativeConnection, Runtime, Worker } from "@temporalio/worker";

const MAX_RUNTIME_MS = 240_000;
const OPERATION_MS = 12_000;
const LOG_LIMIT = 2 * 1024 * 1024;
const startedAt = Date.now();
const deadline = startedAt + MAX_RUNTIME_MS;
const cases = [];
const serverExits = [];
const scenarios = new Map();
const histories = new Set();
const privateSentinel = `private-answer-${randomUUID()}@example.test`;
const token = randomBytes(32).toString("hex");
const namespace = `bluey-phase630-${randomUUID()}`;
const taskQueue = `bluey-phase630-queue-${randomUUID()}`;
const logCounts = { sdk: 0, serverBytes: 0 };
let stage = "preflight";
let interrupted = false;
let serverProcess;
let serverCompletion;
let connection;
let nativeConnection;
let client;
let worker;
let workerCompletion;
let gateway;
let gatewayOrigin;
let temporalPort;
let runtimeDir;
let temporalBin;
let sdkRuntime;
let latency;
let cleaningUp = false;
let workflowDist;
let failureConverterPath;
let createGatewayHttpServer;
let createGatewayService;
let createTemporalCleanupClient;
let createWorkflowCleanupService;
let v2WorkflowTargetDigest;
let WORKFLOW_COMMAND_PATH;
let WORKFLOW_COMMAND_RECONCILIATION_PATH;
let WORKFLOW_CLEANUP_PATH;
let bundleDiagnosticLines = 0;
let bundleDiagnosticBytes = 0;
let cleanupAdapter;
const cleanupRpcDiagnostics = new Set();

function verify(value, code) {
  if (!value) throw new Error(code);
}

function diagnostic(error) {
  const errorName = (error instanceof Error ? error.name : "Error")
    .replace(/[^A-Za-z0-9]/g, "").slice(0, 40) || "Error";
  const raw = error instanceof Error ? error.message : "non_error_failure";
  const message = raw.replaceAll(privateSentinel, "[redacted]").replaceAll(token, "[redacted]")
    .replace(/https?:\/\/[^\s"'<>]+/gi, "[url]")
    .replace(/[\u0000-\u001f\u007f]/g, " ").slice(0, 300);
  return { errorName, message };
}

function cleanupDiagnostic(stage, error) {
  process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_failure", stage, ...diagnostic(error) })}\n`);
}

function captureSdkLog(entry) {
  logCounts.sdk += 1;
  if (stage !== "worker_gateway_startup" || typeof entry.message !== "string"
    || (entry.level !== "ERROR" && !entry.message.includes("ERROR in"))) return;
  for (const line of entry.message.split(/\r?\n/)) {
    if (!/ERROR in|UnhandledSchemeError|Can't resolve|Module build failed|disallowed\s*modules/i.test(line)) continue;
    if (bundleDiagnosticLines >= 8) break;
    const event = JSON.stringify({ event: "temporal_bundle_diagnostic", stage,
      ...diagnostic(new Error(line)) }) + "\n";
    const bytes = Buffer.byteLength(event);
    if (bundleDiagnosticBytes + bytes > 2048) break;
    bundleDiagnosticLines += 1;
    bundleDiagnosticBytes += bytes;
    process.stdout.write(event);
  }
}

function observeCleanupClient(base) {
  const observed = { ...base };
  for (const operation of ["terminate", "delete", "describe", "historyProbe", "listPage"]) {
    observed[operation] = async (...args) => {
      try { return await base[operation](...args); }
      catch (error) {
        if (stage === "exact_cleanup_denies_wrong_digest_then_proves_absence") {
          const grpcCode = isGrpcServiceError(error) && Number.isInteger(error.code)
            && error.code >= 0 && error.code <= 16 ? error.code : null;
          const errorName = diagnostic(error).errorName;
          const key = `${operation}:${grpcCode}:${errorName}`;
          if (cleanupRpcDiagnostics.size < 8 && !cleanupRpcDiagnostics.has(key)) {
            cleanupRpcDiagnostics.add(key);
            process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_rpc_error", operation,
              grpcCode, errorName })}\n`);
          }
        }
        throw error;
      }
    };
  }
  return observed;
}

async function reportServerVersion() {
  try {
    const info = await bounded(client.workflow.withDeadline(Date.now() + 2000,
      () => connection.workflowService.getSystemInfo({})), 2500);
    const available = typeof info.serverVersion === "string"
      && /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.test(info.serverVersion);
    process.stdout.write(`${JSON.stringify({ event: "temporal_embedded_server", available,
      ...(available ? { serverVersion: info.serverVersion } : {}) })}\n`);
  } catch {
    process.stdout.write(`${JSON.stringify({ event: "temporal_embedded_server", available: false })}\n`);
  }
}

function check() {
  verify(!interrupted && Date.now() < deadline, "interrupted_or_deadline");
  if (serverProcess) verify(serverProcess.exitCode === null && serverProcess.signalCode === null,
    "owned_server_exited");
}

async function bounded(operation, timeout = OPERATION_MS) {
  const available = cleaningUp ? timeout : Math.max(1, Math.min(timeout, deadline - Date.now()));
  let timer;
  try {
    return await Promise.race([
      operation,
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("operation_timeout")), available); }),
    ]);
  } finally { clearTimeout(timer); }
}

async function poll(predicate, timeout = OPERATION_MS) {
  const until = Math.min(deadline, Date.now() + timeout);
  while (Date.now() < until) {
    check();
    if (await predicate()) return;
    await delay(50);
  }
  throw new Error("poll_timeout");
}

function opaque(prefix) { return `${prefix}-${randomUUID()}`; }
function digest() { return createHash("sha256").update(randomBytes(32)).digest("hex"); }
function command(mode = "submitted") {
  const value = { schemaVersion: 2, operation: "start", requestId: opaque("request"),
    workflowId: opaque("workflow"), payloadDigest: digest() };
  scenarios.set(value.workflowId, { mode, execute: 0, publish: 0, resume: 0, finalize: 0,
    interventionId: opaque("intervention") });
  return value;
}

function scenario(authority) {
  const value = scenarios.get(authority.workflowId);
  verify(value !== undefined, "unknown_synthetic_authority");
  return value;
}

const opaqueActivities = {
  async executeApplicationCommand(authority) {
    const value = scenario(authority);
    value.execute += 1;
    if (value.mode === "failure") {
      const error = ApplicationFailure.nonRetryable(privateSentinel, "PrivateSentinelFailure",
        { privateAnswer: privateSentinel });
      error.stack = `private-stack-${privateSentinel}`;
      throw error;
    }
    if (value.mode === "intervention") return { state: "intervention_prepared", interventionId: value.interventionId };
    return { state: value.mode };
  },
  async publishApplicationIntervention(input) {
    const value = scenario(input.command);
    verify(input.interventionId === value.interventionId, "synthetic_publish_binding");
    value.publish += 1;
    return { state: "needs_input", interventionId: input.interventionId };
  },
  async resumeApplicationCommand(input) {
    const value = scenario(input.workflow);
    verify(input.command.workflowId === input.workflow.workflowId
      && input.command.interventionId === value.interventionId, "synthetic_resume_binding");
    value.resume += 1;
    return { state: "submitted" };
  },
  async finalizeApplicationCommand(input) {
    const value = scenario(input.command);
    value.finalize += 1;
    verify(input.terminalState === "side_effect_unknown" && input.reasonCode === "runner_ambiguous",
      "synthetic_terminal_binding");
    return { state: input.terminalState };
  },
};

async function preflight() {
  for (const key of Object.keys(process.env)) {
    verify(!/^(TEMPORAL_|BLUEY_JOBS_|OPENAI_|ANTHROPIC_|DEEPGRAM_|SMTP_|AWS_|STRIPE_|SQUARE_|CLOUDFLARE_)/.test(key)
      && !["BLUEY_DATABASE_URL", "BLUEY_TEST_POSTGRES_URL"].includes(key), "inherited_authority_environment");
  }
  const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT || "/missing-owned-root");
  runtimeDir = await realpath(process.env.BLUEY_TEMPORAL_RUNTIME_DIR || "/missing-owned-temporal-root");
  verify(runtimeDir === path.join(root, "nonprod-temporal"), "runtime_root_binding");
  for (const directory of [root, runtimeDir, await realpath(process.env.HOME || "/missing-isolated-home")]) {
    const info = await lstat(directory);
    verify(info.isDirectory() && info.uid === process.getuid() && (info.mode & 0o777) === 0o700,
      "private_directory_identity");
  }
  verify(await realpath(process.env.HOME) === path.join(runtimeDir, "child-home"), "isolated_home_binding");
  temporalBin = await realpath(process.env.BLUEY_TEMPORAL_BIN || "/missing-owned-temporal-bin");
  verify(temporalBin === path.join(runtimeDir, "temporal"), "owned_binary_binding");
  const binary = await lstat(temporalBin);
  verify(binary.isFile() && binary.uid === process.getuid() && (binary.mode & 0o777) === 0o700,
    "private_binary_identity");
  const workspace = await realpath(process.env.BLUEY_TEMPORAL_WORKSPACE_DIR || "/missing-owned-workspace");
  verify(workspace === path.join(runtimeDir, "workspace"), "owned_workspace_binding");
  for (const directory of [workspace, path.join(workspace, "automation"), path.join(workspace, "workflows")]) {
    const info = await lstat(directory);
    verify(await realpath(directory) === directory && info.isDirectory() && info.uid === process.getuid()
      && (info.mode & 0o022) === 0, "owned_workspace_directory_identity");
  }
  for (const name of ["automation", "workflows"]) verify(
    await realpath(path.join(workspace, "node_modules", "@bluey", `jobs-${name}`)) === path.join(workspace, name),
    "owned_project_package_resolution");
  workflowDist = path.join(workspace, "workflows", "dist");
  for (const file of ["workflows.js", "failure-converter.js", "gateway-service.js", "gateway.js",
    "gateway-cleanup-service.js"]) {
    const absolute = path.join(workflowDist, file);
    const info = await lstat(absolute);
    verify(await realpath(absolute) === absolute && info.isFile() && info.uid === process.getuid()
      && (info.mode & 0o022) === 0, "fresh_compiled_source_identity");
  }
  failureConverterPath = path.join(workflowDist, "failure-converter.js");
  // Forward Core logs too: the SDK otherwise prints native errors independently.
  sdkRuntime = Runtime.install({
    logger: new DefaultLogger("TRACE", captureSdkLog),
    telemetryOptions: { logging: { forward: {}, filter: { core: "WARN", other: "WARN" } } },
    shutdownSignals: [],
  });
  // Never evaluate ambient repository dist: the outer launcher compiles this exact
  // owned workspace and verifies the copied current-source manifest before/after.
  ({ createGatewayHttpServer } = await import(pathToFileURL(path.join(workflowDist, "gateway.js")).href));
  ({ createGatewayService, WORKFLOW_COMMAND_PATH, WORKFLOW_COMMAND_RECONCILIATION_PATH } =
    await import(pathToFileURL(path.join(workflowDist, "gateway-service.js")).href));
  ({ createTemporalCleanupClient, createWorkflowCleanupService, v2WorkflowTargetDigest, WORKFLOW_CLEANUP_PATH } =
    await import(pathToFileURL(path.join(workflowDist, "gateway-cleanup-service.js")).href));
  const reservation = net.createServer();
  await bounded(new Promise((resolve, reject) => {
    reservation.once("error", reject);
    reservation.listen(0, "127.0.0.1", resolve);
  }));
  temporalPort = reservation.address().port;
  await bounded(new Promise((resolve, reject) => reservation.close((error) => error ? reject(error) : resolve())));
  verify(temporalPort > 1024, "loopback_port");
}

async function startTemporal() {
  check();
  serverProcess = spawn(temporalBin, ["server", "start-dev", "--ip", "127.0.0.1", "--port", String(temporalPort),
    "--namespace", namespace, "--db-filename", path.join(runtimeDir, "temporal.db"), "--headless"], {
    cwd: runtimeDir,
    env: Object.fromEntries(Object.entries(process.env).filter(([key]) =>
      ["PATH", "HOME", "TMPDIR", "TMP", "TEMP", "LANG", "LC_ALL"].includes(key))),
    stdio: ["ignore", "pipe", "pipe"],
  });
  const owned = serverProcess;
  serverCompletion = new Promise((resolve) => {
    owned.once("error", () => resolve({ pid: owned.pid ?? 0, code: null, signal: "spawn_error" }));
    owned.once("close", (code, signal) => resolve({ pid: owned.pid, code, signal }));
  });
  for (const stream of [owned.stdout, owned.stderr]) stream.on("data", (chunk) => {
    logCounts.serverBytes += chunk.length;
    if (logCounts.serverBytes > LOG_LIMIT) { interrupted = true; owned.kill("SIGINT"); }
  });
  const until = Date.now() + 35_000;
  while (Date.now() < until) {
    check();
    try {
      connection = await bounded(Connection.connect({ address: `127.0.0.1:${temporalPort}`,
        tls: false, connectTimeout: "1 second" }), 1500);
      client = new Client({ connection, namespace, dataConverter: {
        failureConverterPath,
      } });
      await bounded(client.workflow.withDeadline(Date.now() + 1000,
        () => connection.workflowService.describeNamespace({ namespace })), 1500);
      await reportServerVersion();
      return;
    } catch {
      await connection?.close().catch(() => undefined);
      connection = undefined;
      client = undefined;
      await delay(100);
    }
  }
  throw new Error("owned_temporal_readiness_timeout");
}

async function startWorkerAndGateway() {
  check();
  nativeConnection = await bounded(NativeConnection.connect({ address: `127.0.0.1:${temporalPort}`, tls: false }));
  worker = await bounded(Worker.create({
    connection: nativeConnection, namespace, taskQueue,
    workflowsPath: path.join(workflowDist, "workflows.js"),
    dataConverter: { failureConverterPath },
    activities: opaqueActivities, shutdownGraceTime: "2 seconds", shutdownForceTime: "5 seconds",
    maxConcurrentActivityTaskExecutions: 4, maxConcurrentWorkflowTaskExecutions: 4,
  }), 30_000);
  workerCompletion = worker.run().then(() => true, () => false);
  await poll(async () => worker.getState() === "RUNNING");
  cleanupAdapter = observeCleanupClient(createTemporalCleanupClient(client.workflow));
  gateway = createGatewayHttpServer(token, createGatewayService({ client: client.workflow, taskQueue,
    rpcTimeoutMs: 3000 }), createWorkflowCleanupService({
    client: cleanupAdapter, namespace, rpcTimeoutMs: 2000,
    requestTimeoutMs: 8000, visibilityMaxPages: 8,
  }));
  await bounded(new Promise((resolve, reject) => {
    gateway.once("error", reject);
    gateway.listen(0, "127.0.0.1", resolve);
  }));
  gatewayOrigin = `http://127.0.0.1:${gateway.address().port}`;
}

async function stopWorkerAndGateway() {
  let failed = false;
  if (gateway) {
    const owned = gateway;
    gateway = undefined;
    await bounded(new Promise((resolve, reject) => {
      owned.close((error) => error ? reject(error) : resolve());
      owned.closeAllConnections();
    }), 5000).catch((error) => { failed = true; cleanupDiagnostic("gateway_close", error); });
  }
  if (worker) {
    const owned = worker;
    worker = undefined;
    try {
      if (owned.getState() === "RUNNING") owned.shutdown();
      verify(await bounded(workerCompletion, 10_000), "worker_shutdown_failure");
    } catch (error) { failed = true; cleanupDiagnostic("worker_shutdown", error); }
  }
  if (nativeConnection) {
    await bounded(nativeConnection.close(), 5000).catch((error) => {
      failed = true; cleanupDiagnostic("native_connection_close", error);
    });
    nativeConnection = undefined;
  }
  if (connection) {
    await bounded(connection.close(), 5000).catch((error) => {
      failed = true; cleanupDiagnostic("client_connection_close", error);
    });
    connection = undefined;
    client = undefined;
  }
  verify(!failed, "worker_or_gateway_shutdown_failure");
}

async function stopTemporal() {
  if (!serverProcess) return;
  const owned = serverProcess;
  const completion = serverCompletion;
  serverProcess = undefined;
  if (owned.exitCode === null && owned.signalCode === null) owned.kill("SIGINT");
  let result;
  try { result = await bounded(completion, 12_000); }
  catch {
    owned.kill("SIGKILL");
    await bounded(completion, 5000);
    throw new Error("owned_server_forced_shutdown");
  }
  serverExits.push(result);
  verify(result.pid > 0 && result.code === 0 && result.signal === null, "owned_server_nonzero_shutdown");
}

async function post(value, route = WORKFLOW_COMMAND_PATH, bearer = token) {
  check();
  const response = await bounded(fetch(`${gatewayOrigin}${route}`, {
    method: "POST", headers: { "content-type": "application/json", authorization: `Bearer ${bearer}` },
    body: JSON.stringify(value), signal: AbortSignal.timeout(10_000),
  }));
  verify(response.headers.get("cache-control") === "no-store", "gateway_private_cache_header");
  const text = await bounded(response.text());
  verify(Buffer.byteLength(text) < 32_768, "gateway_response_bound");
  return { status: response.status, body: JSON.parse(text) };
}

async function absent(workflowId) {
  try {
    await bounded(client.workflow.withDeadline(Date.now() + 2000,
      () => client.workflow.getHandle(workflowId).describe()), 3000);
    return false;
  } catch (error) { if (error instanceof WorkflowNotFoundError) return true; throw error; }
}

async function accepted(input) {
  const response = await post(input);
  verify(response.status === 202 && response.body.outcome === "accepted", "fresh_start_not_accepted");
  verify(typeof response.body.temporalRunId === "string", "missing_first_run_id");
  histories.add(input.workflowId);
  return response.body.temporalRunId;
}

async function result(input, expected = "submitted") {
  const value = await bounded(client.workflow.getHandle(input.workflowId).result());
  verify(value?.state === expected && Object.keys(value).length === 1, "workflow_terminal_result");
}

function resume(input, interventionId) {
  return { schemaVersion: 2, operation: "resume", requestId: opaque("resume"),
    workflowId: input.workflowId, payloadDigest: digest(), interventionId };
}

async function updateAndReplay(input, runId) {
  const value = scenario(input);
  const update = resume(input, value.interventionId);
  const response = await post(update);
  verify(response.status === 202 && response.body.outcome === "accepted"
    && response.body.temporalRunId === runId, "update_not_accepted");
  await result(input);
  const replay = await post(update);
  verify(replay.status === 202 && replay.body.outcome === "already_accepted"
    && replay.body.temporalRunId === runId, "update_receipt_not_replayed");
  verify(value.execute === 1 && value.publish === 1 && value.resume === 1, "update_repeated_activity");
}

function scanHistory(value) {
  let visited = 0;
  function walk(item, depth = 0) {
    verify(++visited < 200_000 && depth < 50, "history_scan_bounds");
    if (item === null || item === undefined) return;
    if (item instanceof Uint8Array) return walk(Buffer.from(item).toString("utf8"), depth + 1);
    if (typeof item === "string") {
      verify(!item.includes(privateSentinel) && !item.includes("PrivateSentinelFailure"), "private_history_content");
      if ((item.startsWith("{") || item.startsWith("[")) && item.length < 1024 * 1024) {
        let decoded;
        try { decoded = JSON.parse(item); } catch { return; }
        walk(decoded, depth + 1);
      }
      return;
    }
    if (typeof item === "object") for (const [key, entry] of Object.entries(item)) {
      walk(key, depth + 1);
      walk(entry, depth + 1);
    }
  }
  walk(value);
}

async function runCase(name, operation) {
  stage = name;
  check();
  const start = Date.now();
  await operation();
  check();
  cases.push(name);
  process.stdout.write(`${JSON.stringify({ event: "temporal_case", case: name, passed: true,
    durationMs: Date.now() - start })}\n`);
}

async function diagnoseCleanup(target, response) {
  const closed = (value, allowed) => allowed.includes(value) ? value : "unrecognized";
  const keys = (value) => value && typeof value === "object" ? Object.keys(value).slice(0, 16)
    .map((key) => key.replaceAll(privateSentinel, "redacted").replaceAll(token, "redacted")
      .replace(/[^A-Za-z0-9_]/g, "").slice(0, 64)) : [];
  process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_response_shape", status: response.status,
    outcome: closed(response.body?.outcome, ["rejected", "identity_conflict", "pending", "absence_observed"]),
    reason: closed(response.body?.reason, ["invalid_request", "identity_conflict", "not_found", "temporal_unavailable",
      "workflow_running", "termination_pending", "history_delete_pending", "visibility_pending", "absence_observed"]) })}\n`);
  const cleanupClient = createTemporalCleanupClient(client.workflow);
  const expected = Buffer.from(JSON.stringify({ schemaVersion: 2, requestId: target.startRequestId,
    workflowId: target.workflowId, payloadDigest: target.startPayloadDigest }), "utf8");
  const shape = (value) => {
    const fields = value?.memoFields;
    const payload = fields?.bluey_jobs_command_v2;
    const encoding = payload?.metadata?.encoding;
    const decodedEncoding = encoding instanceof Uint8Array ? Buffer.from(encoding).toString("utf8") : undefined;
    return {
      workflowIdMatch: value?.workflowId === target.workflowId,
      runIdMatch: target.knownRunIds.includes(value?.runId),
      firstRunMatch: value?.firstExecutionRunId === target.firstExecutionRunId,
      firstRunType: typeof value?.firstExecutionRunId,
      workflowTypeType: typeof value?.workflowType,
      workflowType: closed(value?.workflowType, ["applicationWorkflow", "applicationWorkflowV2"]),
      statusType: typeof value?.status,
      status: closed(value?.status, [0, 1, 2, 3, 4, 5, 6, 7, "RUNNING", "COMPLETED", "FAILED", "CANCELED",
        "TERMINATED", "CONTINUED_AS_NEW", "TIMED_OUT"]),
      startTimeKeys: keys(value?.startTime),
      secondsType: typeof value?.startTime?.seconds,
      nanosType: typeof value?.startTime?.nanos,
      nanosInteger: Number.isInteger(value?.startTime?.nanos),
      memoFieldKeys: keys(fields),
      payloadOwnKeys: keys(payload),
      metadataKeys: keys(payload?.metadata),
      dataIsBytes: payload?.data instanceof Uint8Array,
      encodingIsBytes: encoding instanceof Uint8Array,
      encodingUtf8: closed(decodedEncoding, ["json/plain", "binary/plain", "binary/null", "json/protobuf", "binary/protobuf"]),
      byteMatchExpected: payload?.data instanceof Uint8Array && Buffer.from(payload.data).equals(expected),
      externalPayloadsEmpty: Array.isArray(payload?.externalPayloads) && payload.externalPayloads.length === 0,
    };
  };
  try {
    const described = await bounded(cleanupClient.withDeadline(Date.now() + 2500,
      () => cleanupClient.describe(target.workflowId)), 3000);
    process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_provider_shape", source: "describe",
      ...shape(described) })}\n`);
  } catch (error) {
    process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_provider_shape", source: "describe",
      failed: true, errorName: diagnostic(error).errorName })}\n`);
  }
  try {
    const page = await bounded(cleanupClient.withDeadline(Date.now() + 2500,
      () => cleanupClient.listPage(`WorkflowId = "${target.workflowId}"`, new Uint8Array(), 10)), 3000);
    process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_provider_shape", source: "visibility",
      count: page.executions.length, hasNextPage: Boolean(page.nextPageToken?.length),
      executions: page.executions.slice(0, 3).map(shape) })}\n`);
  } catch (error) {
    process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_provider_shape", source: "visibility",
      failed: true, errorName: diagnostic(error).errorName })}\n`);
  }
}

async function qualify() {
  await runCase("gateway_auth_and_private_input", async () => {
    const unauthorized = command();
    verify((await post(unauthorized, WORKFLOW_COMMAND_PATH, "wrong-test-token")).status === 401, "unauthorized_status");
    verify(await absent(unauthorized.workflowId), "unauthorized_workflow_created");
    const malformed = command();
    verify((await post({ ...malformed, payloadDigest: "bad" })).status === 400, "invalid_command_status");
    verify(await absent(malformed.workflowId), "invalid_workflow_created");
    const privateInput = command();
    verify((await post({ ...privateInput, privateAnswer: privateSentinel })).status === 400, "private_extra_status");
    verify(await absent(privateInput.workflowId), "private_input_reached_temporal");
  });
  await runCase("reconcile_missing_never_starts", async () => {
    const input = command();
    verify((await post(input, WORKFLOW_COMMAND_RECONCILIATION_PATH)).status === 503, "missing_reconciliation_status");
    verify(await absent(input.workflowId), "reconciliation_created_workflow");
  });
  await runCase("fresh_start_submitted", async () => {
    const input = command();
    await accepted(input);
    await result(input);
    verify(scenario(input).execute === 1, "fresh_activity_count");
  });
  await runCase("concurrent_exact_start_and_digest_fence", async () => {
    const input = command();
    const responses = await bounded(Promise.all(Array.from({ length: 6 }, () => post(input))));
    verify(responses.every((response) => response.status === 202), "concurrent_start_status");
    verify(new Set(responses.map((response) => response.body.temporalRunId)).size === 1, "concurrent_first_run_binding");
    verify(responses.filter((response) => response.body.outcome === "accepted").length === 1, "concurrent_creation_count");
    histories.add(input.workflowId);
    await result(input);
    verify(scenario(input).execute === 1, "concurrent_activity_repeated");
    verify((await post({ ...input, payloadDigest: digest() })).status === 409, "changed_digest_status");
  });
  await runCase("intervention_update_exact_replay_and_wrong_binding", async () => {
    const input = command("intervention");
    const runId = await accepted(input);
    await poll(async () => scenario(input).publish === 1);
    verify((await post(resume(input, opaque("wrong-intervention")))).status === 409, "wrong_update_binding_status");
    verify(scenario(input).resume === 0, "wrong_update_resumed_activity");
    await updateAndReplay(input, runId);
  });
  await runCase("persistent_restart_pending_intervention", async () => {
    const input = command("intervention");
    const runId = await accepted(input);
    await poll(async () => scenario(input).publish === 1);
    await stopWorkerAndGateway();
    await stopTemporal();
    await startTemporal();
    await startWorkerAndGateway();
    const replay = await post(input);
    verify(replay.status === 202 && replay.body.outcome === "already_accepted"
      && replay.body.temporalRunId === runId, "restart_start_replay");
    await updateAndReplay(input, runId);
  });
  await runCase("side_effect_unknown_finalizes_without_retry", async () => {
    const input = command("side_effect_unknown");
    const runId = await accepted(input);
    await result(input, "side_effect_unknown");
    const replay = await post(input);
    verify(replay.status === 202 && replay.body.temporalRunId === runId, "unknown_terminal_replay");
    verify(scenario(input).execute === 1 && scenario(input).finalize === 1, "unknown_terminal_activity_repeated");
  });
  await runCase("real_activity_failure_history_is_opaque", async () => {
    const input = command("failure");
    await accepted(input);
    let rejected = false;
    try { await bounded(client.workflow.getHandle(input.workflowId).result()); } catch { rejected = true; }
    verify(rejected, "synthetic_failure_not_terminal");
    const history = await bounded(client.workflow.getHandle(input.workflowId).fetchHistory());
    verify(history.events.some((event) => event.activityTaskFailedEventAttributes), "missing_real_activity_failure_event");
    verify(history.events.some((event) => event.workflowExecutionFailedEventAttributes), "missing_terminal_failure_event");
    scanHistory(history);
    verify(scenario(input).execute === 1, "nonretryable_activity_retried");
  });
  await runCase("exact_cleanup_denies_wrong_digest_then_proves_absence", async () => {
    const input = command("intervention");
    const runId = await accepted(input);
    await poll(async () => scenario(input).publish === 1);
    const target = { schemaVersion: 3, operation: "reconcile_v2_target", cleanupRequestId: opaque("cleanup"),
      cleanupGenerationId: opaque("cleanup-generation"), targetSetDigest: digest(), namespace,
      workflowType: "applicationWorkflowV2", workflowId: input.workflowId, firstExecutionRunId: runId,
      startRequestId: input.requestId, startPayloadDigest: input.payloadDigest, knownRunIds: [runId],
      targetDigest: "0".repeat(64), cleanupFence: 1, observationPass: 1 };
    target.targetDigest = v2WorkflowTargetDigest(target);
    verify((await post({ ...target, targetDigest: digest() }, WORKFLOW_CLEANUP_PATH)).status === 409,
      "wrong_cleanup_digest_status");
    const description = await bounded(client.workflow.getHandle(input.workflowId).describe());
    verify(description.status.name === "RUNNING", "wrong_cleanup_mutated_workflow");
    const rawPage = await bounded(client.workflow.withDeadline(Date.now() + 2500,
      () => client.workflow.workflowService.listWorkflowExecutions({ namespace,
        query: `WorkflowId = "${input.workflowId}"`, pageSize: 10, nextPageToken: new Uint8Array() })), 3000);
    const rawRow = rawPage.executions?.find((row) => row.execution?.workflowId === input.workflowId
      && row.execution?.runId === runId);
    process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_raw_visibility_root",
      visibilityRowPresent: rawRow !== undefined,
      firstRunEmpty: rawRow !== undefined
        && (rawRow.firstRunId === undefined || rawRow.firstRunId === null || rawRow.firstRunId === ""),
      firstRunMatchesExpected: rawRow?.firstRunId === runId })}\n`);
    const cleanupStarted = Date.now();
    const cleanupUntil = Math.min(deadline, cleanupStarted + 90_000);
    let observedAbsence = false;
    let previousTransition;
    while (Date.now() < cleanupUntil) {
      check();
      const response = await post(target, WORKFLOW_CLEANUP_PATH);
      if (response.status !== 202) await diagnoseCleanup(target, response);
      verify(response.status === 202, "exact_cleanup_status");
      const outcome = ["pending", "absence_observed"].includes(response.body.outcome)
        ? response.body.outcome : "unrecognized";
      const reason = ["workflow_running", "termination_pending", "history_delete_pending", "visibility_pending",
        "temporal_unavailable", "absence_observed"].includes(response.body.reason) ? response.body.reason : "unrecognized";
      const transition = `${outcome}:${reason}`;
      if (transition !== previousTransition) {
        previousTransition = transition;
        process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_transition", outcome, reason,
          elapsedMs: Date.now() - cleanupStarted })}\n`);
      }
      if (outcome === "absence_observed") { observedAbsence = true; break; }
      await delay(Math.min(5000, Math.max(1, cleanupUntil - Date.now())));
    }
    process.stdout.write(`${JSON.stringify({ event: "temporal_cleanup_readback", observedAbsence,
      cleanupElapsedMs: Date.now() - cleanupStarted })}\n`);
    verify(observedAbsence, "cleanup_async_absence_timeout");
    verify(await absent(input.workflowId), "cleanup_did_not_delete_execution");
    let historyAbsent = false;
    try {
      await bounded(cleanupAdapter.withDeadline(Date.now() + 2000,
        () => cleanupAdapter.historyProbe(input.workflowId, runId)), 2500);
    } catch (error) {
      verify(error instanceof WorkflowNotFoundError || (isGrpcServiceError(error) && error.code === 5),
        "cleanup_history_absence_ambiguous");
      historyAbsent = true;
    }
    verify(historyAbsent, "cleanup_history_still_present");
    const finalVisibility = await bounded(cleanupAdapter.withDeadline(Date.now() + 2000,
      () => cleanupAdapter.listPage(`WorkflowId = "${input.workflowId}"`, new Uint8Array(), 100)), 2500);
    verify(finalVisibility.executions.length === 0 && !finalVisibility.nextPageToken?.length,
      "cleanup_visibility_not_absent");
    histories.delete(input.workflowId);
  });
  await runCase("latency_baseline_20_fresh_starts", async () => {
    const samples = [];
    for (let index = 0; index < 20; index += 1) {
      const input = command();
      const start = performance.now();
      await accepted(input);
      await result(input);
      samples.push(performance.now() - start);
    }
    samples.sort((left, right) => left - right);
    latency = { samples: samples.length, p50Ms: samples[9], p95Ms: samples[18], maxMs: samples[19] };
    verify(Object.values(latency).every(Number.isFinite), "latency_nonfinite");
    process.stdout.write(`${JSON.stringify({ event: "temporal_latency", ...latency, errors: 0,
      scope: "local_synthetic_no_production_slo" })}\n`);
  });
  stage = "all_history_privacy_scan";
  for (const workflowId of histories) scanHistory(await bounded(client.workflow.getHandle(workflowId).fetchHistory()));
}

const stop = () => {
  interrupted = true;
  serverProcess?.kill("SIGINT");
  try { if (worker?.getState() === "RUNNING") worker.shutdown(); } catch { /* Cleanup still owns every handle. */ }
};
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(signal, stop);
const watchdog = setTimeout(stop, MAX_RUNTIME_MS);
let passed = false;
try {
  await preflight();
  stage = "temporal_startup";
  await startTemporal();
  stage = "worker_gateway_startup";
  await startWorkerAndGateway();
  await qualify();
  passed = true;
} catch (error) {
  process.stdout.write(`${JSON.stringify({ event: "temporal_failure", stage, passed: false,
    ...diagnostic(error) })}\n`);
} finally {
  stage = "cleanup";
  cleaningUp = true;
  for (const [cleanupStage, cleanup] of [["worker_gateway", stopWorkerAndGateway],
    ["temporal_server", stopTemporal], ["sdk_runtime", async () => {
    if (sdkRuntime) await bounded(sdkRuntime.shutdown(), 5000);
  }]]) {
    try { await cleanup(); } catch (error) { passed = false; cleanupDiagnostic(cleanupStage, error); }
  }
  clearTimeout(watchdog);
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.off(signal, stop);
}
try {
  verify(passed && !interrupted, "qualification_failed");
  verify(cases.length === 10 && serverExits.length === 2 && latency?.samples === 20, "qualification_evidence_counts");
  const report = { version: 1, tests: cases.length, passed: cases.length, failed: [], servers: serverExits, latency };
  const bytes = JSON.stringify(report) + "\n";
  verify(Buffer.byteLength(bytes) < 64 * 1024, "report_size_bound");
  await writeFile(path.join(runtimeDir, "result.json"), bytes, { flag: "wx", mode: 0o600 });
  process.stdout.write(`${JSON.stringify({ event: "temporal_finished", tests: cases.length, passed: cases.length,
    servers: serverExits.length, sdkLogsSuppressed: logCounts.sdk })}\n`);
} catch (error) {
  process.stdout.write(`${JSON.stringify({ event: "temporal_finished", testsPassed: cases.length, failed: true,
    ...diagnostic(error) })}\n`);
  process.exitCode = 1;
}
