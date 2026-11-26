import { createHash, createHmac } from "node:crypto";
import { isDeepStrictEqual } from "node:util";

import {
  createApplicationReceipt,
} from "@bluey/jobs-automation";
import {
  createJobsWorkerAuthHeaders,
} from "@bluey/jobs-automation/worker-auth";
import {
  ExecutionLeaseClient,
} from "../dist/execution-lease.js";
import {
  finalizeLeasedRun,
} from "../dist/leased-run.js";
import {
  durableResultScope,
  readManagedResult,
  readManagedResultState,
  stageManagedResult,
  writeManagedResult,
} from "../dist/result-store.js";
import {
  subjectStoragePaths,
} from "../dist/subject-storage-layout.js";

// BEGIN WORKFLOW RECEIPT ACTIVITY SEAM
const WORKFLOW_RESULT_ORIGIN = "http://127.0.0.1:43109";
const WORKFLOW_RESULT_TOKEN = "phase660-receipt-workflow-activity-test-token";
const PRIVATE_JSON_HEADERS = Object.freeze({
  "cache-control": "no-store",
  "content-type": "application/json",
  "x-content-type-options": "nosniff",
});
const WORKFLOW_SHA256 = /^[a-f0-9]{64}$/;
const WORKFLOW_NONCE = /^[a-f0-9]{32}$/;
const WORKFLOW_SAFE_ID = /^[A-Za-z0-9._:+-]{1,160}$/;
const WORKFLOW_MAX_RESPONSE_BYTES = 1024 * 1024;

class WorkflowBridgeRefusal extends Error {
  constructor(code) {
    super(code);
    this.name = "WorkflowBridgeRefusal";
    this.code = code;
  }
}

function workflowRefuse(code) {
  throw new WorkflowBridgeRefusal(code);
}

function workflowExactKeys(value, keys, code) {
  if (!value || typeof value !== "object" || Array.isArray(value)) workflowRefuse(code);
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length
    || actual.some((key, index) => key !== expected[index])) workflowRefuse(code);
  return value;
}

function workflowIdentifier(value, code) {
  if (typeof value !== "string" || !WORKFLOW_SAFE_ID.test(value)) workflowRefuse(code);
  return value;
}

function workflowLoopbackOrigin(value) {
  if (typeof value !== "string") workflowRefuse("invalid_api_origin");
  const match = /^http:\/\/127\.0\.0\.1:([1-9][0-9]{3,4})$/.exec(value);
  const port = match ? Number(match[1]) : 0;
  if (!match || !Number.isSafeInteger(port) || port < 1024 || port > 65_535) {
    workflowRefuse("invalid_api_origin");
  }
  return value;
}

function workflowClosedReceiptPath(value, applicationId) {
  if (applicationId === "." || applicationId === ".."
    || value !== `/api/jobs/internal/applications/${encodeURIComponent(applicationId)}/receipt`) {
    workflowRefuse("invalid_receipt_path");
  }
  return value;
}

function workflowPrivateJson(headers) {
  return Object.entries(PRIVATE_JSON_HEADERS)
    .every(([name, value]) => headers.get(name) === value);
}

async function workflowBoundedBytes(response) {
  if (!response.body) return Buffer.alloc(0);
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > WORKFLOW_MAX_RESPONSE_BYTES) workflowRefuse("api_response_too_large");
      chunks.push(Buffer.from(value));
    }
    return Buffer.concat(chunks, size);
  } finally {
    await reader.cancel().catch(() => {});
  }
}

function workflowResponseRecord(bytes) {
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    workflowRefuse("invalid_api_response");
  }
}

function workflowRequestUrl(value) {
  if (typeof value === "string") return value;
  if (value instanceof URL) return value.href;
  if (value instanceof Request) return value.url;
  workflowRefuse("invalid_fetch_input");
}

function workflowValidateReceiptSignature({ headers, body, workerId, signingKey, path, nonces }) {
  const timestamp = headers.get("x-bluey-jobs-worker-timestamp");
  const nonce = headers.get("x-bluey-jobs-worker-nonce");
  const digest = createHash("sha256").update(body).digest("hex");
  if (!/^\d+$/.test(timestamp ?? "") || !WORKFLOW_NONCE.test(nonce ?? "") || nonces.has(nonce)
    || headers.get("x-bluey-jobs-worker-id") !== workerId
    || headers.get("x-bluey-jobs-worker-audience") !== "bluey-jobs-api"
    || headers.get("x-bluey-jobs-worker-scope") !== "receipt"
    || headers.get("x-bluey-jobs-worker-content-sha256") !== digest) {
    workflowRefuse("invalid_receipt_auth");
  }
  const canonical = [
    "bluey-jobs-worker-v1", timestamp, nonce, workerId, "bluey-jobs-api",
    "receipt", "POST", path, digest,
  ].join("\n");
  const expected = createHmac("sha256", signingKey).update(canonical).digest("hex");
  if (headers.get("x-bluey-jobs-worker-signature") !== expected) {
    workflowRefuse("invalid_receipt_signature");
  }
  nonces.add(nonce);
  return { digest, nonce };
}

function createClosedWorkflowReceiptFetchDraft({
  actualFetch,
  apiOrigin,
  workerId,
  signingKey,
  path,
  lookup,
  committedResult,
  expectedReceiptRequest,
}) {
  if (typeof actualFetch !== "function") workflowRefuse("invalid_actual_fetch");
  const origin = workflowLoopbackOrigin(apiOrigin);
  workflowIdentifier(workerId, "invalid_worker_id");
  if (typeof signingKey !== "string" || Buffer.byteLength(signingKey) < 32) {
    workflowRefuse("invalid_signing_key");
  }
  workflowExactKeys(lookup, [
    "accountId", "applicationId", "applicationIdentityId", "browserSessionId", "runId", "requestId",
  ], "invalid_lookup");
  for (const [key, value] of Object.entries(lookup)) workflowIdentifier(value, `invalid_lookup_${key}`);
  const closedPath = workflowClosedReceiptPath(path, lookup.applicationId);
  workflowExactKeys(expectedReceiptRequest, [
    "account_id", "lease_token", "fence", "receipt", "evidence_objects",
  ], "invalid_expected_receipt");
  if (!Number.isSafeInteger(expectedReceiptRequest.fence) || expectedReceiptRequest.fence < 1) {
    workflowRefuse("invalid_expected_fence");
  }
  const attempts = [];
  const nonces = new Set();
  let resultReads = 0;
  let withholdFirstAcknowledgement = true;

  const closedFetch = async (input, init = {}) => {
    const url = workflowRequestUrl(input);
    if (url === `${WORKFLOW_RESULT_ORIGIN}/results`) {
      const headers = new Headers(init.headers);
      if (init.method !== "POST" || init.redirect !== "error"
        || headers.get("authorization") !== `Bearer ${WORKFLOW_RESULT_TOKEN}`
        || headers.get("content-type") !== "application/json"
        || typeof init.body !== "string") workflowRefuse("invalid_result_lookup_request");
      let body;
      try {
        body = JSON.parse(init.body);
      } catch {
        workflowRefuse("invalid_result_lookup_body");
      }
      if (!isDeepStrictEqual(body, lookup)) workflowRefuse("result_lookup_binding_mismatch");
      resultReads += 1;
      return Response.json({ ...committedResult, requestId: lookup.requestId }, {
        status: 200,
        headers: PRIVATE_JSON_HEADERS,
      });
    }

    if (url !== `${origin}${closedPath}`) workflowRefuse("out_of_scope_fetch");
    const headers = new Headers(init.headers);
    if (init.method !== "POST" || init.redirect !== "error"
      || headers.get("content-type") !== "application/json"
      || typeof init.body !== "string") workflowRefuse("invalid_receipt_request");
    let body;
    try {
      body = JSON.parse(init.body);
    } catch {
      workflowRefuse("invalid_receipt_body");
    }
    if (!isDeepStrictEqual(body, expectedReceiptRequest)) workflowRefuse("receipt_body_mismatch");
    const auth = workflowValidateReceiptSignature({
      headers,
      body: init.body,
      workerId,
      signingKey,
      path: closedPath,
      nonces,
    });
    const response = await actualFetch(url, init);
    const copy = response.clone();
    const bytes = await workflowBoundedBytes(copy);
    if (response.status !== 200 || !workflowPrivateJson(response.headers)) {
      workflowRefuse("receipt_response_contract");
    }
    const value = workflowResponseRecord(bytes);
    if (!value || typeof value !== "object" || Array.isArray(value)
      || value.id !== lookup.applicationId
      || value.run_id !== lookup.runId
      || value.state !== "submitted") workflowRefuse("receipt_response_binding");
    const attempt = Object.freeze({
      requestBodySha256: auth.digest,
      requestNonce: auth.nonce,
      status: response.status,
    });
    attempts.push(attempt);
    if (withholdFirstAcknowledgement) {
      withholdFirstAcknowledgement = false;
      throw new TypeError("Synthetic receipt acknowledgement lost");
    }
    return response;
  };

  return Object.freeze({
    fetch: closedFetch,
    resultReads: () => resultReads,
    attempts: () => [...attempts],
  });
}

function setWorkflowEnvironment(values) {
  const saved = new Map();
  for (const [key, value] of Object.entries(values)) {
    saved.set(key, Object.prototype.hasOwnProperty.call(process.env, key)
      ? process.env[key] : undefined);
    process.env[key] = value;
  }
  return () => {
    for (const [key, value] of saved) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  };
}

async function executeCommittedReceiptWorkflowReplay({
  actualFetch,
  apiOrigin,
  workerId,
  signingKey,
  receiptPath: path,
  lookup,
  committedResult,
  expectedReceiptRequest,
  activityInput,
  loadActivity,
  onFirstAcknowledgementWithheld,
}) {
  if (typeof loadActivity !== "function" || typeof onFirstAcknowledgementWithheld !== "function") {
    workflowRefuse("invalid_activity_callbacks");
  }
  const bridge = createClosedWorkflowReceiptFetchDraft({
    actualFetch,
    apiOrigin,
    workerId,
    signingKey,
    path,
    lookup,
    committedResult,
    expectedReceiptRequest,
  });
  const originalFetch = globalThis.fetch;
  const restoreEnvironment = setWorkflowEnvironment({
    BLUEY_JOBS_API_ORIGIN: workflowLoopbackOrigin(apiOrigin),
    BLUEY_JOBS_RUNNER_ORIGIN: WORKFLOW_RESULT_ORIGIN,
    BLUEY_JOBS_RUNNER_TOKEN: WORKFLOW_RESULT_TOKEN,
    BLUEY_JOBS_WORKER_SIGNING_KEY: signingKey,
    BLUEY_JOBS_WORKFLOW_WORKER_ID: workerId,
  });
  try {
    globalThis.fetch = bridge.fetch;
    const activity = await loadActivity();
    if (!activity || typeof activity.persistSubmissionReceipt !== "function") {
      workflowRefuse("missing_receipt_activity");
    }
    let lost = false;
    try {
      await activity.persistSubmissionReceipt(activityInput);
    } catch (error) {
      lost = error instanceof TypeError
        && error.message === "Synthetic receipt acknowledgement lost";
      if (!lost) throw error;
    }
    if (!lost || bridge.attempts().length !== 1 || bridge.resultReads() !== 1) {
      workflowRefuse("first_acknowledgement_not_withheld");
    }
    await onFirstAcknowledgementWithheld(bridge.attempts()[0]);
    await activity.persistSubmissionReceipt(activityInput);
    const attempts = bridge.attempts();
    if (attempts.length !== 2 || bridge.resultReads() !== 2
      || attempts[0].requestBodySha256 !== attempts[1].requestBodySha256
      || attempts[0].requestNonce === attempts[1].requestNonce) {
      workflowRefuse("receipt_replay_mismatch");
    }
    return Object.freeze({
      schema: "phase660_receipt_workflow_activity_draft_v1",
      resultReads: 2,
      attempts,
    });
  } finally {
    globalThis.fetch = originalFetch;
    restoreEnvironment();
  }
}

// END WORKFLOW RECEIPT ACTIVITY SEAM
const INPUT_SCHEMA = "phase658_managed_receipt_journey_node_input_v1";
const CONTINUE_SCHEMA = "phase658_managed_receipt_journey_node_continue_v1";
const EVENT_SCHEMA = "phase658_managed_receipt_journey_node_event_v1";
const RESULT_SCHEMA = "phase658_managed_receipt_journey_node_result_v1";
const MAX_INITIAL_BYTES = 512 * 1024;
const MAX_CONTROL_BYTES = 1024;
const MAX_STDIN_BYTES = 640 * 1024;
const MAX_EVENT_BYTES = 16 * 1024;
const MAX_EVENT_TOTAL_BYTES = 64 * 1024;
const MAX_STDOUT_BYTES = 16 * 1024;
const MAX_ISO_TIMESTAMP_MS = 8_640_000_000_000_000;
const MAX_HTTP_BYTES = 1024 * 1024;
const SAFE_ID = /^[A-Za-z0-9._:+-]{1,160}$/;
const SHA256 = /^[a-f0-9]{64}$/;
const PROFILE_SCOPE = /^[a-f0-9]{40}$/;
const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
const PHASES = Object.freeze([
  "claimed",
  "effect_checksum_rejected",
  "effect_authorized",
  "irreversible_checksum_rejected",
  "irreversible_fence_rejected",
  "irreversible_authorized",
  "finish_before_commit",
  "result_committed",
  "receipt_terminalization_deferred",
  "receipt_posted",
  "receipt_replayed",
  "fingerprint_rejected",
]);

class DriverFailure extends Error {
  constructor(code, phase) {
    super(code);
    this.name = "DriverFailure";
    this.code = code;
    this.phase = phase;
  }
}

function fail(code, phase = "input") {
  throw new DriverFailure(code, phase);
}

function staticFailure(code, phase, operation) {
  try {
    return operation();
  } catch {
    fail(code, phase);
  }
}

function exactKeys(value, expected, phase = "input") {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail("invalid_shape", phase);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    fail("invalid_shape", phase);
  }
  return value;
}

function text(value, name, maximum = 4096) {
  if (typeof value !== "string" || value.length < 1 || Buffer.byteLength(value) > maximum) {
    fail(`invalid_${name}`);
  }
  return value;
}

function identifier(value, name) {
  const candidate = text(value, name, 160);
  if (!SAFE_ID.test(candidate)) fail(`invalid_${name}`);
  return candidate;
}

function sha256(value, name) {
  if (typeof value !== "string" || !SHA256.test(value)) fail(`invalid_${name}`);
  return value;
}

function integer(value, name, minimum = 0) {
  if (!Number.isSafeInteger(value) || value < minimum) fail(`invalid_${name}`);
  return value;
}

function canonical(value) {
  if (value === null || typeof value === "boolean" || typeof value === "string") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) fail("noncanonical_number");
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (!value || typeof value !== "object") fail("noncanonical_value");
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
}

function canonicalBytes(value) {
  return Buffer.from(`${canonical(value)}\n`, "utf8");
}

function digest(value) {
  return createHash("sha256").update(canonicalBytes(value)).digest("hex");
}

function same(left, right) {
  return canonical(left) === canonical(right);
}

function decodeBase64(value, name, maximum) {
  const encoded = text(value, name, Math.ceil(maximum * 4 / 3) + 8);
  if (!BASE64.test(encoded)) fail(`invalid_${name}`);
  const decoded = Buffer.from(encoded, "base64");
  if (decoded.length < 1 || decoded.length > maximum || decoded.toString("base64") !== encoded) {
    decoded.fill(0);
    fail(`invalid_${name}`);
  }
  return decoded;
}

function loopbackOrigin(value) {
  const raw = text(value, "origin", 256);
  let parsed;
  try {
    parsed = new URL(raw);
  } catch {
    fail("invalid_origin");
  }
  if (!/^https?:$/.test(parsed.protocol)
    || !new Set(["127.0.0.1", "[::1]"]).has(parsed.hostname)
    || parsed.username || parsed.password || parsed.pathname !== "/" || parsed.search || parsed.hash) {
    fail("invalid_origin");
  }
  return parsed.origin;
}

function makeFrameReader() {
  let buffered = Buffer.alloc(0);
  let total = 0;
  const iterator = process.stdin[Symbol.asyncIterator]();
  return async (maximum) => {
    for (;;) {
      const newline = buffered.indexOf(0x0a);
      if (newline >= 0) {
        if (newline === 0 || newline > maximum || buffered[newline - 1] === 0x0d) fail("invalid_frame");
        const line = buffered.subarray(0, newline).toString("utf8");
        buffered = buffered.subarray(newline + 1);
        try {
          return JSON.parse(line);
        } catch {
          fail("invalid_frame");
        }
      }
      if (buffered.length > maximum) fail("frame_too_large");
      const next = await iterator.next();
      if (next.done) fail("unexpected_eof");
      const chunk = Buffer.from(next.value);
      total += chunk.length;
      if (total > MAX_STDIN_BYTES) fail("stdin_too_large");
      buffered = Buffer.concat([buffered, chunk]);
    }
  };
}

const readFrame = makeFrameReader();
let emittedBytes = 0;

async function checkpoint(nonce, phase, fields = {}) {
  if (!PHASES.includes(phase)) fail("invalid_phase", phase);
  const event = { schema: EVENT_SCHEMA, kind: "phase_ready", nonce, phase, ...fields };
  const line = `${canonical(event)}\n`;
  const size = Buffer.byteLength(line);
  if (size > MAX_EVENT_BYTES || emittedBytes + size > MAX_EVENT_TOTAL_BYTES) fail("event_limit", phase);
  emittedBytes += size;
  if (!process.stderr.write(line, "utf8")) await new Promise((resolve) => process.stderr.once("drain", resolve));
  const control = exactKeys(await readFrame(MAX_CONTROL_BYTES), ["schema", "kind", "nonce", "phase"], phase);
  if (control.schema !== CONTINUE_SCHEMA || control.kind !== "continue"
    || control.nonce !== nonce || control.phase !== phase) fail("control_mismatch", phase);
}

async function readResponseBytes(response, maximum, phase) {
  if (!response.body) return Buffer.alloc(0);
  const reader = response.body.getReader();
  const chunks = [];
  let bytes = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > maximum) fail("response_too_large", phase);
      chunks.push(Buffer.from(value));
    }
    return Buffer.concat(chunks, bytes);
  } finally {
    reader.releaseLock();
  }
}

function jsonBytes(bytes, phase) {
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    fail("invalid_response_json", phase);
  }
}

function responseHeaders(response) {
  return {
    privateNoStore: response.headers.get("cache-control") === "private, no-store",
    noStore: response.headers.get("cache-control") === "no-store",
    nosniff: response.headers.get("x-content-type-options") === "nosniff",
    contentType: response.headers.get("content-type"),
  };
}

function instrumentedFetch(captures) {
  return async (input, init) => {
    const response = await fetch(input, init);
    const bytes = await readResponseBytes(response, MAX_HTTP_BYTES, "http");
    const url = new URL(String(input));
    const count = (captures.get(url.pathname)?.count ?? 0) + 1;
    captures.set(url.pathname, {
      status: response.status,
      headers: responseHeaders(response),
      body: bytes.length ? jsonBytes(bytes, "http") : null,
      bytes: bytes.length,
      count,
    });
    return new Response(response.body === null ? null : bytes,
      { status: response.status, statusText: response.statusText, headers: response.headers });
  };
}

async function signedPost(input, path, body, phase) {
  const serialized = JSON.stringify(body);
  const workerHeaders = createJobsWorkerAuthHeaders({
    signingKey: input.worker.signingKey,
    workerId: input.worker.ownerId,
    method: "POST",
    path,
    body: serialized,
  });
  const response = await fetch(`${input.origin}${path}`, {
    method: "POST",
    redirect: "error",
    headers: {
      ...workerHeaders,
      "Content-Type": "application/json",
    },
    body: serialized,
  });
  const bytes = await readResponseBytes(response, MAX_HTTP_BYTES, phase);
  if (phase === "receipt" && response.ok
    && (response.headers.get("content-type") !== "application/json"
      || response.headers.get("cache-control") !== "no-store"
      || response.headers.get("x-content-type-options") !== "nosniff")) {
    fail("receipt_response_headers_invalid", "receipt_posted");
  }
  return {
    status: response.status,
    headers: responseHeaders(response),
    body: bytes,
    json: response.ok && bytes.length ? jsonBytes(bytes, phase) : null,
    requestBodySha256: createHash("sha256").update(serialized).digest("hex"),
    requestNonce: workerHeaders["x-bluey-jobs-worker-nonce"],
  };
}

class MemoryDirectory {
  constructor(relativePath, deviceId, files = new Map()) {
    this.relativePath = relativePath;
    this.canonicalPath = `/memory/${relativePath}`;
    this.deviceId = deviceId;
    this.linkCount = 2;
    this.files = files;
  }
  async ensureChildDirectory(name) { return new MemoryDirectory(`${this.relativePath}/${identifier(name, "directory")}`, this.deviceId); }
  async openChildDirectory(name) { return this.ensureChildDirectory(name); }
  async writeFileExclusive(name, contents) {
    if (this.files.has(name)) return false;
    this.files.set(name, Buffer.from(contents));
    return true;
  }
  async replaceFile(name, contents) { this.files.set(name, Buffer.from(contents)); }
  async readFileBounded(name, maximum) {
    const value = this.files.get(name);
    if (!value || value.length > maximum) fail("memory_storage_read");
    return Buffer.from(value);
  }
  async inventory() {
    const entries = [...this.files.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([name, bytes]) => ({
      relativePath: name,
      kind: "file",
      deviceId: this.deviceId,
      linkCount: 1,
      sizeBytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    }));
    return { entries, count: entries.length, bytes: entries.reduce((sum, item) => sum + item.sizeBytes, 0), sha256: digest(entries) };
  }
  async removeEntry(name) { this.files.delete(name); }
}

function validateInput(raw) {
  const input = exactKeys(raw, [
    "schema", "nonce", "origin", "worker", "claim", "runnerVolume", "managedRuntime",
    "finalSubmitProof", "result", "receipt",
  ]);
  if (input.schema !== INPUT_SCHEMA) fail("invalid_schema");
  input.nonce = identifier(input.nonce, "nonce");
  input.origin = loopbackOrigin(input.origin);
  input.worker = exactKeys(input.worker, ["ownerId", "signingKey"]);
  input.worker.ownerId = identifier(input.worker.ownerId, "owner_id");
  input.worker.signingKey = text(input.worker.signingKey, "signing_key", 4096);
  if (Buffer.byteLength(input.worker.signingKey) < 32) fail("invalid_signing_key");
  input.claim = exactKeys(input.claim, [
    "accountId", "applicationId", "runId", "browserProfileId", "workflowRequestId", "managedCloudRelease",
  ]);
  for (const key of ["accountId", "applicationId", "runId", "browserProfileId", "workflowRequestId"]) {
    input.claim[key] = identifier(input.claim[key], key);
  }
  if (!input.claim.managedCloudRelease
    || typeof input.claim.managedCloudRelease !== "object"
    || Array.isArray(input.claim.managedCloudRelease)
    || input.claim.managedCloudRelease.version !== 1
    || Buffer.byteLength(canonical(input.claim.managedCloudRelease)) > 16 * 1024) {
    fail("invalid_managed_cloud_release");
  }
  input.runnerVolume = exactKeys(input.runnerVolume, [
    "volumeId", "enrollmentEpoch", "processInstanceId", "keyFingerprint", "runtimeGrantId", "runtimeSha256",
    "expectedClaimBinding", "volumeProof",
  ]);
  input.runnerVolume.volumeId = identifier(input.runnerVolume.volumeId, "volume_id");
  input.runnerVolume.enrollmentEpoch = integer(input.runnerVolume.enrollmentEpoch, "enrollment_epoch", 1);
  input.runnerVolume.processInstanceId = identifier(input.runnerVolume.processInstanceId, "process_instance_id");
  input.runnerVolume.keyFingerprint = sha256(input.runnerVolume.keyFingerprint, "key_fingerprint");
  input.runnerVolume.runtimeGrantId = identifier(input.runnerVolume.runtimeGrantId, "runtime_grant_id");
  input.runnerVolume.runtimeSha256 = sha256(input.runnerVolume.runtimeSha256, "runtime_sha256");
  exactKeys(input.runnerVolume.expectedClaimBinding, [
    "accountId", "applicationId", "runId", "browserProfileId", "ownerId", "workflowRequestId",
    "managedCloudRelease", "managedCloudReleaseSha256", "managedCloudRuntimeInstanceId",
    "managedCloudRuntimeInstanceEpoch",
  ]);
  exactKeys(input.runnerVolume.volumeProof, [
    "version", "audience", "operation", "requestId", "volumeId", "enrollmentEpoch", "processInstanceId",
    "issuedAtMs", "payloadSha256", "signature",
  ]);
  if (input.runnerVolume.volumeProof.version !== 1
    || input.runnerVolume.volumeProof.audience !== "bluey-jobs-runner-volume-authority"
    || input.runnerVolume.volumeProof.operation !== "execution_lease_claim") fail("invalid_volume_proof");
  const runtimeKeys = [
    "grantId", "runtimeInstanceId", "runtimeIdentitySha256", "workerId", "scope", "activationSha256",
    "activationExpiresAtMs", "manifestSha256", "componentId", "role", "headRevision", "transitionSha256",
    "artifactSha256", "configSchemaSha256", "migrationSetSha256", "protocolSetSha256", "taskQueueSha256",
    "failureConverterSha256", "dependencyEvidenceSha256", "instanceEpoch", "nextHeartbeatSequence", "claimedAtMs",
    "replayed",
  ];
  const actualRuntimeKeys = Object.keys(input.managedRuntime ?? {});
  const optionalBootKeys = actualRuntimeKeys.includes("bootId") || actualRuntimeKeys.includes("bootGeneration")
    ? ["bootId", "bootGeneration"]
    : [];
  input.managedRuntime = exactKeys(input.managedRuntime, [...runtimeKeys, ...optionalBootKeys]);
  input.result = exactKeys(input.result, [
    "context", "subjectSha256", "browserSessionId", "encryptionKeyBase64", "receiptInput", "evidenceObjects",
  ]);
  input.result.context = exactKeys(input.result.context, ["requestId", "profileScope"]);
  input.result.context.requestId = identifier(input.result.context.requestId, "result_request_id");
  if (!PROFILE_SCOPE.test(input.result.context.profileScope)) fail("invalid_profile_scope");
  input.result.subjectSha256 = sha256(input.result.subjectSha256, "subject_sha256");
  input.result.browserSessionId = identifier(input.result.browserSessionId, "browser_session_id");
  input.result.receiptInput = exactKeys(input.result.receiptInput, [
    "receiptId", "accountId", "runId", "runner", "applicationIdentityId", "browserProfileId",
    "adapter", "adapterVersion", "job", "packet", "documents", "events", "result", "generatedAt",
    "finalUrl", "screenshotKeys",
  ]);
  input.result.receiptInput.receiptId = identifier(input.result.receiptInput.receiptId, "receipt_id");
  if (input.result.receiptInput.accountId !== input.claim.accountId
    || input.result.receiptInput.runId !== input.claim.runId
    || input.result.receiptInput.runner !== "cloud"
    || input.result.receiptInput.packet?.applicationId !== input.claim.applicationId
    || input.result.receiptInput.result?.status !== "submitted") {
    fail("receipt_input_binding_mismatch");
  }
  if (!Array.isArray(input.result.evidenceObjects) || input.result.evidenceObjects.length < 1 || input.result.evidenceObjects.length > 8) {
    fail("invalid_evidence_objects");
  }
  const receiptUsesWorkflowActivity = Boolean(input.receipt
    && typeof input.receipt === "object" && !Array.isArray(input.receipt)
    && Object.prototype.hasOwnProperty.call(input.receipt, "useWorkflowActivity"));
  input.receipt = exactKeys(input.receipt, [
    "applicationId", "deferTerminalUpdate", ...(receiptUsesWorkflowActivity ? ["useWorkflowActivity"] : []),
  ]);
  if (typeof input.receipt.deferTerminalUpdate !== "boolean"
    || (receiptUsesWorkflowActivity && typeof input.receipt.useWorkflowActivity !== "boolean")) {
    fail("invalid_receipt_retry_mode");
  }
  input.receipt.useWorkflowActivity = receiptUsesWorkflowActivity
    ? input.receipt.useWorkflowActivity : false;
  if (input.receipt.useWorkflowActivity && input.receipt.deferTerminalUpdate) {
    fail("workflow_activity_terminal_failure_scope");
  }
  input.receipt.applicationId = identifier(input.receipt.applicationId, "receipt_application_id");
  if (input.receipt.applicationId !== input.claim.applicationId) fail("receipt_application_mismatch");
  return input;
}

function claimProjection(record) {
  if (!record || typeof record !== "object" || Array.isArray(record)) fail("missing_claim_capture", "claimed");
  const token = text(record.lease_token, "lease_token", 256);
  const projected = structuredClone(record);
  projected.lease_token = createHash("sha256").update(token).digest("hex");
  return {
    token,
    leaseTokenSha256: projected.lease_token,
    fence: integer(record.fence, "claim_fence", 1),
    leaseExpiresAtMs: integer(record.lease_expires_at_ms, "lease_expiry", 1),
    claimResponseProjectionSha256: digest(projected),
  };
}

function managedBody(input, lease, extra = {}) {
  const releaseSha256 = sha256(input.runnerVolume.expectedClaimBinding.managedCloudReleaseSha256, "release_sha256");
  return {
    account_id: input.claim.accountId,
    application_id: input.claim.applicationId,
    lease_token: lease.token,
    fence: lease.fence,
    workflow_request_id: input.claim.workflowRequestId,
    managed_cloud_release: input.claim.managedCloudRelease,
    managed_cloud_release_sha256: releaseSha256,
    managed_cloud_runtime_instance_id: input.managedRuntime.runtimeInstanceId,
    managed_cloud_runtime_instance_epoch: input.managedRuntime.instanceEpoch,
    ...extra,
  };
}

async function expectNegative(input, path, body, expectedStatus, expectedText, phase) {
  const response = await signedPost(input, path, body, phase);
  if (response.status !== expectedStatus || response.body.toString("utf8") !== expectedText) {
    fail("negative_contract_mismatch", phase);
  }
  await checkpoint(input.nonce, phase, { status: response.status });
}

function managedStorage(input) {
  const scope = durableResultScope(input.result.context);
  const paths = subjectStoragePaths(input.result.subjectSha256).result(scope);
  const device = "memory-result-device-v1";
  return {
    kind: "result",
    subjectSha256: input.result.subjectSha256,
    scope,
    paths,
    root: new MemoryDirectory(paths.root.relativePath, device),
    temporary: new MemoryDirectory(paths.temporary.relativePath, device),
  };
}

async function postReceipt(input, lease, receipt, evidenceObjects) {
  const path = `/api/jobs/internal/applications/${encodeURIComponent(input.receipt.applicationId)}/receipt`;
  return signedPost(input, path, {
    account_id: input.claim.accountId,
    lease_token: lease.token,
    fence: lease.fence,
    receipt,
    evidence_objects: evidenceObjects,
  }, "receipt");
}

function receiptInputAfterAuthorizedBinding(input, authority, phase) {
  const observedAtMs = Date.now();
  const bindingConsumedAtMs = integer(
    authority?.bindingConsumedAtMs,
    "receipt_binding_consumed_at_ms",
    1,
  );
  if (!Number.isSafeInteger(observedAtMs) || observedAtMs < 1
    || observedAtMs > MAX_ISO_TIMESTAMP_MS || bindingConsumedAtMs > MAX_ISO_TIMESTAMP_MS) {
    fail("receipt_timestamp_invalid", phase);
  }
  const timestamp = new Date(Math.max(observedAtMs, bindingConsumedAtMs)).toISOString();
  const receiptInput = structuredClone(input.result.receiptInput);
  if (!receiptInput.result || typeof receiptInput.result !== "object"
    || Array.isArray(receiptInput.result) || !Array.isArray(receiptInput.events)
    || receiptInput.events.length !== 1 || !receiptInput.events[0]
    || typeof receiptInput.events[0] !== "object" || Array.isArray(receiptInput.events[0])) {
    fail("receipt_timing_shape", phase);
  }
  receiptInput.generatedAt = timestamp;
  receiptInput.events[0].occurredAt = timestamp;
  receiptInput.result.submittedAt = timestamp;
  return receiptInput;
}

function receiptPostFailure(posted, applicationId, runId) {
  if (posted.status === 200) {
    if (posted.json?.id !== applicationId) return "receipt_post_200_wrong_id";
    if (posted.json?.run_id !== runId) return "receipt_post_200_wrong_run";
    return "receipt_post_200_wrong_state";
  }
  if ([400, 401, 403, 404, 409, 413, 422, 429].includes(posted.status)) return `receipt_post_${posted.status}`;
  if (posted.status >= 500 && posted.status <= 599) return "receipt_post_5xx";
  return "receipt_post_other";
}

async function main() {
  let phase = "input";
  let encryptionKey;
  let active;
  const decodedEvidence = [];
  const input = validateInput(await readFrame(MAX_INITIAL_BYTES));
  try {
    encryptionKey = decodeBase64(input.result.encryptionKeyBase64, "encryption_key", 32);
    if (encryptionKey.length !== 32) fail("invalid_encryption_key");
    for (const object of input.result.evidenceObjects) {
      exactKeys(object, ["original_key", "kind", "media_type", "sha256", "bytes_base64"]);
      if (!new Set(["resume", "cover_letter", "attachment", "screenshot"]).has(object.kind)) fail("invalid_evidence_kind");
      if (!new Set(["application/pdf", "image/png"]).has(object.media_type)) fail("invalid_evidence_media_type");
      const bytes = decodeBase64(object.bytes_base64, "evidence_bytes", 8 * 1024 * 1024);
      decodedEvidence.push(bytes);
      if (createHash("sha256").update(bytes).digest("hex") !== sha256(object.sha256, "evidence_sha256")) {
        fail("evidence_hash_mismatch");
      }
      if (object.media_type === "application/pdf" && bytes.subarray(0, 5).toString("ascii") !== "%PDF-") fail("invalid_pdf");
      if (object.media_type === "image/png" && !bytes.subarray(0, 8).equals(Buffer.from("89504e470d0a1a0a", "hex"))) fail("invalid_png");
    }
    const captures = new Map();
    const client = new ExecutionLeaseClient({
      origin: input.origin,
      workerSigningKey: input.worker.signingKey,
      ownerId: input.worker.ownerId,
      runnerVolume: {
        volumeId: input.runnerVolume.volumeId,
        enrollmentEpoch: input.runnerVolume.enrollmentEpoch,
        processInstanceId: input.runnerVolume.processInstanceId,
        keyFingerprint: input.runnerVolume.keyFingerprint,
        runtimeGrantId: input.runnerVolume.runtimeGrantId,
        runtimeSha256: input.runnerVolume.runtimeSha256,
        createExecutionLeaseClaimProof(binding) {
          if (!same(binding, input.runnerVolume.expectedClaimBinding)) fail("claim_binding_mismatch", "claim");
          return structuredClone(input.runnerVolume.volumeProof);
        },
      },
      managedCloudRuntime: input.managedRuntime,
      fetch: instrumentedFetch(captures),
      requestTimeoutMs: 10_000,
      heartbeatIntervalMs: 60_000,
      maxResponseBytes: MAX_HTTP_BYTES,
    });
    phase = "claim";
    active = await client.claim(input.claim);
    const claimPath = "/api/jobs/internal/execution-leases/claim";
    const lease = claimProjection(captures.get(claimPath)?.body);
    await checkpoint(input.nonce, "claimed", {
      leaseTokenSha256: lease.leaseTokenSha256,
      tokenCanonical32Bytes: /^[A-Za-z0-9_-]+$/.test(lease.token)
        && Buffer.from(lease.token, "base64url").length === 32
        && Buffer.from(lease.token, "base64url").toString("base64url") === lease.token,
      fence: lease.fence,
      leaseExpiresAtMs: lease.leaseExpiresAtMs,
      claimResponseProjectionSha256: lease.claimResponseProjectionSha256,
    });

    const runPath = encodeURIComponent(input.claim.runId);
    const effectPath = `/api/jobs/internal/execution-leases/${runPath}/authorize-managed-effect`;
    const badEffect = managedBody(input, lease);
    badEffect.managed_cloud_release_sha256 = "0".repeat(64);
    phase = "effect_checksum_rejected";
    await expectNegative(input, effectPath, badEffect, 400, "Invalid managed-cloud execution authority.", phase);
    phase = "effect_authorized";
    await active.authorizeManagedEffect(input.claim.workflowRequestId, input.claim.managedCloudRelease);
    const effectCapture = captures.get(effectPath);
    if (!effectCapture || effectCapture.status !== 200 || !effectCapture.headers.privateNoStore || !effectCapture.headers.nosniff) {
      fail("effect_response_contract", phase);
    }
    await checkpoint(input.nonce, phase, {
      status: effectCapture.status,
      privateNoStore: true,
      nosniff: true,
      authorizedAtMs: integer(
        effectCapture.body?.managedCloud?.authorization?.authorizedAtMs,
        "effect_authorized_at_ms",
        1,
      ),
      responseSha256: digest(effectCapture.body),
    });

    const irreversiblePath = `/api/jobs/internal/execution-leases/${runPath}/irreversible`;
    const irreversibleBody = managedBody(input, lease, { action: "submit", final_submit_proof: input.finalSubmitProof });
    const badIrreversible = structuredClone(irreversibleBody);
    badIrreversible.managed_cloud_release_sha256 = "0".repeat(64);
    phase = "irreversible_checksum_rejected";
    await expectNegative(input, irreversiblePath, badIrreversible, 400, "Invalid managed-cloud execution authority.", phase);
    const staleIrreversible = structuredClone(irreversibleBody);
    staleIrreversible.fence = lease.fence + 1;
    phase = "irreversible_fence_rejected";
    await expectNegative(input, irreversiblePath, staleIrreversible, 404, "Execution lease target not found.", phase);
    phase = "irreversible_authorized";
    await active.beforeFinalSubmit(input.finalSubmitProof);
    await active.afterFinalSubmit("activated");
    const irreversibleCapture = captures.get(irreversiblePath);
    if (!irreversibleCapture || irreversibleCapture.status !== 200
      || !irreversibleCapture.headers.privateNoStore || !irreversibleCapture.headers.nosniff) {
      fail("irreversible_response_contract", phase);
    }
    const authority = active.atsCertifiedReceiptAuthority;
    if (!authority) fail("missing_receipt_authority", phase);
    await checkpoint(input.nonce, phase, {
      status: irreversibleCapture.status,
      privateNoStore: true,
      nosniff: true,
      authorizedAtMs: integer(
        irreversibleCapture.body?.managedCloud?.authorization?.authorizedAtMs,
        "irreversible_authorized_at_ms",
        1,
      ),
      atsCertifiedReceiptAuthority: structuredClone(authority),
      responseSha256: digest(irreversibleCapture.body),
    });

    const receiptInput = staticFailure(
      "receipt_timing_failed",
      phase,
      () => receiptInputAfterAuthorizedBinding(input, authority, phase),
    );
    const receipt = staticFailure(
      "receipt_create_failed",
      phase,
      () => createApplicationReceipt({
        ...receiptInput,
        atsCertifiedReceiptAuthority: authority,
      }),
    );
    const checkpointAuthority = staticFailure(
      "receipt_checkpoint_metadata_failed",
      phase,
      () => active.checkpointMetadata(),
    );
    if (checkpointAuthority.leaseToken !== lease.token || checkpointAuthority.fence !== lease.fence) {
      fail("receipt_authority_mismatch", phase);
    }
    const storedResult = {
      accountId: input.claim.accountId,
      applicationId: input.claim.applicationId,
      applicationIdentityId: input.result.receiptInput.applicationIdentityId,
      browserSessionId: input.result.browserSessionId,
      runId: input.claim.runId,
      receipt: receiptInput.result,
      receiptBundle: receipt,
      evidenceObjects: input.result.evidenceObjects,
      receiptAuthority: {
        leaseToken: checkpointAuthority.leaseToken,
        fence: checkpointAuthority.fence,
      },
    };
    const storage = staticFailure("result_storage_setup_failed", phase, () => managedStorage(input));
    const order = [];
    phase = "finalize";
    let finalizeFailure = "finish_submitted_failed";
    let commitEntered = false;
    let committed;
    try {
      committed = await finalizeLeasedRun({
        lease: active,
        intendedOutcome: "submitted",
        cleanup: async () => { order.push("cleanup"); },
        stage: async () => {
          order.push("stage");
          try {
            await stageManagedResult(storage, input.result.context, storedResult, encryptionKey);
          } catch {
            finalizeFailure = "result_stage_write_failed";
            throw new Error();
          }
          let staged;
          try {
            staged = await readManagedResultState(storage, input.result.context, encryptionKey);
          } catch {
            finalizeFailure = "result_stage_readback_failed";
            throw new Error();
          }
          let stagedMatches;
          try {
            stagedMatches = Boolean(staged)
              && staged.state === "staged"
              && same(staged.result, storedResult);
          } catch {
            finalizeFailure = "result_stage_mismatch";
            throw new Error();
          }
          if (!stagedMatches) {
            finalizeFailure = "result_stage_mismatch";
            throw new Error();
          }
        },
        commit: async () => {
          commitEntered = true;
          const finishPath = `/api/jobs/internal/execution-leases/${encodeURIComponent(input.claim.runId)}/finish`;
          const finish = captures.get(finishPath);
          if (finish?.count !== 1 || finish.status !== 204 || finish.bytes !== 0) {
            fail("finish_acknowledgement_invalid", "irreversible_authorized");
          }
          await checkpoint(input.nonce, "finish_before_commit", { finishOutcome: "submitted" });
          order.push("commit");
          await writeManagedResult(storage, input.result.context, storedResult, encryptionKey);
          const state = await readManagedResultState(storage, input.result.context, encryptionKey);
          const result = await readManagedResult(storage, input.result.context, encryptionKey);
          if (!state || state.state !== "committed" || !same(state.result, storedResult) || !same(result, storedResult)) {
            fail("committed_readback_mismatch", "commit");
          }
          return { result, resultSha256: state.resultSha256 };
        },
      });
    } catch (error) {
      if (error instanceof DriverFailure) throw error;
      if (commitEntered) throw error;
      fail(finalizeFailure, "irreversible_authorized");
    }
    if (!same(order, ["cleanup", "stage", "commit"])) fail("finalize_order_mismatch", "result_committed");
    await checkpoint(input.nonce, "result_committed", { resultSha256: committed.resultSha256 });

    const nonces = new Set();
    let requestBodySha256;
    const verifyAttempt = (attempt, unchanged) => {
      if (!SHA256.test(attempt.requestBodySha256)
        || !/^[a-f0-9]{32}$/.test(attempt.requestNonce) || nonces.has(attempt.requestNonce)
        || (requestBodySha256 !== undefined && (attempt.requestBodySha256 === requestBodySha256) !== unchanged)) {
        fail("receipt_request_replay_mismatch", phase);
      }
      nonces.add(attempt.requestNonce);
      requestBodySha256 ??= attempt.requestBodySha256;
    };
    let posted;
    let replayed;
    if (input.receipt.useWorkflowActivity) {
      phase = "receipt_posted";
      const workflowLookup = {
        accountId: input.claim.accountId,
        applicationId: input.claim.applicationId,
        applicationIdentityId: committed.result.applicationIdentityId,
        browserSessionId: committed.result.browserSessionId,
        runId: input.claim.runId,
        requestId: input.result.context.requestId,
      };
      const workflowInput = {
        accountId: input.claim.accountId,
        applicationId: input.claim.applicationId,
        applicationIdentityId: committed.result.applicationIdentityId,
        browserProfileId: input.claim.browserProfileId,
        browserSessionId: committed.result.browserSessionId,
        idempotencyKey: input.claim.runId,
        resultRequestId: input.result.context.requestId,
        jobId: input.result.receiptInput.packet.jobId,
        canonicalJobKey: input.result.receiptInput.job.canonicalUrl,
        packetId: input.result.receiptInput.packet.resumeVersionId,
        packet: input.result.receiptInput.packet,
        job: input.result.receiptInput.job,
        runner: "cloud",
        url: input.result.receiptInput.job.canonicalUrl,
      };
      const receiptPath = `/api/jobs/internal/applications/${encodeURIComponent(input.receipt.applicationId)}/receipt`;
      const activityReplay = await executeCommittedReceiptWorkflowReplay({
        actualFetch: globalThis.fetch.bind(globalThis),
        apiOrigin: input.origin,
        workerId: input.worker.ownerId,
        signingKey: input.worker.signingKey,
        receiptPath,
        lookup: workflowLookup,
        committedResult: committed.result,
        expectedReceiptRequest: {
          account_id: input.claim.accountId,
          lease_token: lease.token,
          fence: lease.fence,
          receipt: committed.result.receiptBundle,
          evidence_objects: committed.result.evidenceObjects,
        },
        activityInput: workflowInput,
        loadActivity: () => import("../../workflows/dist/activities.js"),
        onFirstAcknowledgementWithheld: async (attempt) => {
          verifyAttempt(attempt, true);
          await checkpoint(input.nonce, phase, {
            status: attempt.status,
            applicationState: "submitted",
            workflowActivity: true,
            acknowledgementWithheld: true,
          });
        },
      });
      posted = activityReplay.attempts[0];
      phase = "receipt_replayed";
      replayed = activityReplay.attempts[1];
      verifyAttempt(replayed, true);
      await checkpoint(input.nonce, phase, {
        status: replayed.status,
        applicationState: "submitted",
        workflowActivity: true,
        resultReads: activityReplay.resultReads,
      });
    } else {
      phase = "receipt_posted";
      const sendCommittedReceipt = () => postReceipt(
        input,
        lease,
        committed.result.receiptBundle,
        committed.result.evidenceObjects,
      );
      posted = await sendCommittedReceipt();
      verifyAttempt(posted, true);
      if (input.receipt.deferTerminalUpdate) {
        if (posted.status !== 503
          || posted.body.toString("utf8") !== "Bluey Jobs is still finalizing this submitted application. Please retry.") {
          fail("receipt_terminalization_not_deferred", phase);
        }
        await checkpoint(input.nonce, "receipt_terminalization_deferred", { status: posted.status });
        posted = await sendCommittedReceipt();
        verifyAttempt(posted, true);
      }
      if (posted.status !== 200 || posted.json?.id !== input.claim.applicationId
        || posted.json?.run_id !== input.claim.runId || posted.json?.state !== "submitted") {
        fail(receiptPostFailure(posted, input.claim.applicationId, input.claim.runId), phase);
      }
      await checkpoint(input.nonce, phase, { status: posted.status, applicationState: "submitted" });
      phase = "receipt_replayed";
      replayed = await sendCommittedReceipt();
      verifyAttempt(replayed, true);
      if (replayed.status !== 200 || replayed.json?.id !== input.claim.applicationId
        || replayed.json?.run_id !== input.claim.runId || replayed.json?.state !== "submitted") {
        fail("receipt_replay_failed", phase);
      }
      await checkpoint(input.nonce, phase, { status: replayed.status, applicationState: "submitted" });
    }
    phase = "fingerprint_rejected";
    const changed = structuredClone(committed.result.receiptBundle);
    changed.receiptId = `${changed.receiptId}-changed`;
    const rejected = await postReceipt(input, lease, changed, committed.result.evidenceObjects);
    verifyAttempt(rejected, false);
    const rejection = jsonBytes(rejected.body, phase);
    if (rejected.status !== 409 || !rejected.headers.noStore || !rejected.headers.nosniff
      || rejected.headers.contentType !== "application/json" || !same(rejection, {
        schema_version: 1, outcome: "rejected", reason: "identity_conflict",
        application_id: input.claim.applicationId, run_id: input.claim.runId,
      })) {
      fail("fingerprint_negative_mismatch", phase);
    }
    await checkpoint(input.nonce, phase, { status: rejected.status });
    return {
      schema: RESULT_SCHEMA,
      ok: true,
      nonce: input.nonce,
      applicationId: input.claim.applicationId,
      runId: input.claim.runId,
      resultSha256: committed.resultSha256,
      receiptStatus: posted.status,
      replayStatus: replayed.status,
      fingerprintStatus: rejected.status,
    };
  } finally {
    if (active) {
      try {
        await active.stopHeartbeat();
      } catch {
        // Best-effort failure-path quiescence must not replace the primary result.
      }
    }
    encryptionKey?.fill(0);
    for (const bytes of decodedEvidence) bytes.fill(0);
  }
}

let output;
try {
  output = await main();
} catch (error) {
  output = {
    schema: RESULT_SCHEMA,
    ok: false,
    code: error instanceof DriverFailure ? error.code : "driver_failed",
    phase: error instanceof DriverFailure ? error.phase : "internal",
  };
}
const finalLine = `${canonical(output)}\n`;
if (Buffer.byteLength(finalLine) > MAX_STDOUT_BYTES) {
  process.stdout.write(`${JSON.stringify({ schema: RESULT_SCHEMA, ok: false, code: "result_too_large", phase: "output" })}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(finalLine);
  if (!output.ok) process.exitCode = 1;
}
