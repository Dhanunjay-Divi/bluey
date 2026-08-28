import { Buffer } from "node:buffer";

import {
  originalSourceSha256,
  type OriginalSourceVerificationFailureCode,
  type OriginalSourceVerificationObservation,
} from "@bluey/jobs-automation/original-source-verification";
import { createJobsWorkerAuthHeaders } from "@bluey/jobs-automation/worker-auth";

const DEFAULT_REQUEST_TIMEOUT_MS = 10_000;
const DEFAULT_REPORT_ATTEMPTS = 3;
const DEFAULT_REPORT_RETRY_MS = 250;
const MAX_RESPONSE_BYTES = 256 * 1024;
const SAFE_ID = /^[A-Za-z0-9_-]{20,128}$/;
const SAFE_WORKER_ID = /^[A-Za-z0-9._:-]{3,128}$/;
const SHA256 = /^[a-f0-9]{64}$/;
const TOKEN = /^[A-Za-z0-9_-]{43}$/;

export interface OriginalSourceVerifierBinding {
  worker_id: string;
  runtime_instance_id: string;
  runtime_instance_epoch: number;
  runtime_authority_sha256: string;
  runtime_session_token: string;
}

export interface OriginalSourceVerificationLease {
  assignment_id: string;
  subject_sha256: string;
  canonical_subject_json: string;
  attempt_id: string;
  fence: number;
  lease_token: string;
  lease_expires_at_ms: number;
  hard_deadline_at_ms: number;
  heartbeat_sequence: number;
}

export interface OriginalSourceVerificationHeartbeat {
  lease_expires_at_ms: number;
  hard_deadline_at_ms: number;
  heartbeat_sequence: number;
  replayed: boolean;
}

export interface OriginalSourceVerificationHead {
  head_revision: number;
  material_generation: number;
  assignment_id: string;
  receipt_id: string;
  receipt_sha256: string;
  subject_sha256: string;
  material_sha256: string;
  assurance: string;
  result: string;
  checked_at_ms: number;
  expires_at_ms: number;
}

export type OriginalSourceVerificationPublishedObservation =
  OriginalSourceVerificationObservation & {
    worker_runtime_identity_sha256: string;
  };

export interface OriginalSourceVerificationTerminal {
  assignment_id: string;
  state: string;
  replayed: boolean;
  receipt_sha256: string | null;
  head: OriginalSourceVerificationHead | null;
}

export interface OriginalSourceVerificationWorkerApi {
  lease(): Promise<OriginalSourceVerificationLease | null>;
  heartbeat(
    lease: OriginalSourceVerificationLease,
    heartbeatSequence: number,
  ): Promise<OriginalSourceVerificationHeartbeat>;
  complete(
    lease: OriginalSourceVerificationLease,
    requestId: string,
    observation: OriginalSourceVerificationPublishedObservation,
  ): Promise<OriginalSourceVerificationTerminal>;
  fail(
    lease: OriginalSourceVerificationLease,
    requestId: string,
    errorCode: OriginalSourceVerificationFailureCode,
    observation: OriginalSourceVerificationPublishedObservation,
  ): Promise<OriginalSourceVerificationTerminal>;
}

export type OriginalSourceVerificationApiFetch = (
  input: string | URL | Request,
  init?: RequestInit,
) => Promise<Response>;

export type OriginalSourceVerificationApiErrorCode =
  | "api_rejected"
  | "api_timeout"
  | "api_unavailable"
  | "authority_unavailable"
  | "conflict"
  | "invalid_response"
  | "not_found";

export class OriginalSourceVerificationApiError extends Error {
  readonly code: OriginalSourceVerificationApiErrorCode;
  readonly status?: number;

  constructor(code: OriginalSourceVerificationApiErrorCode, message: string, status?: number) {
    super(message);
    this.name = "OriginalSourceVerificationApiError";
    this.code = code;
    this.status = status;
  }
}

export interface OriginalSourceVerificationApiClientOptions {
  origin: string;
  signingKey: string;
  binding: OriginalSourceVerifierBinding;
  fetch?: OriginalSourceVerificationApiFetch;
  requestTimeoutMs?: number;
  reportAttempts?: number;
  reportRetryMs?: number;
  sleep?: (milliseconds: number) => Promise<void>;
}

export class OriginalSourceVerificationApiClient
implements OriginalSourceVerificationWorkerApi {
  private readonly origin: string;
  private readonly signingKey: string;
  private readonly binding: OriginalSourceVerifierBinding;
  private readonly fetcher: OriginalSourceVerificationApiFetch;
  private readonly requestTimeoutMs: number;
  private readonly reportAttempts: number;
  private readonly reportRetryMs: number;
  private readonly sleep: (milliseconds: number) => Promise<void>;

  constructor(options: OriginalSourceVerificationApiClientOptions) {
    this.origin = normalizeOrigin(options.origin);
    if (Buffer.byteLength(options.signingKey, "utf8") < 32) {
      throw new Error("BLUEY_JOBS_WORKER_SIGNING_KEY must contain at least 32 bytes");
    }
    this.signingKey = options.signingKey;
    this.binding = parseBinding(options.binding);
    this.fetcher = options.fetch ?? fetch;
    this.requestTimeoutMs = boundedInteger(
      options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS,
      100,
      120_000,
      "request timeout",
    );
    this.reportAttempts = boundedInteger(
      options.reportAttempts ?? DEFAULT_REPORT_ATTEMPTS,
      1,
      5,
      "report attempts",
    );
    this.reportRetryMs = boundedInteger(
      options.reportRetryMs ?? DEFAULT_REPORT_RETRY_MS,
      0,
      10_000,
      "report retry delay",
    );
    this.sleep = options.sleep ?? ((milliseconds) =>
      new Promise((resolve) => setTimeout(resolve, milliseconds)));
  }

  async lease(): Promise<OriginalSourceVerificationLease | null> {
    const response = await this.requestWithRetry(
      "/api/jobs/internal/original-source-verifications/lease",
      { binding: this.binding },
    );
    if (response.status === 204) return null;
    return parseLease(await authoritativeJson(response));
  }

  async heartbeat(
    lease: OriginalSourceVerificationLease,
    heartbeatSequence: number,
  ): Promise<OriginalSourceVerificationHeartbeat> {
    const path = assignmentPath(lease.assignment_id, "heartbeat");
    const value = await this.jsonWithRetry(path, {
      binding: this.binding,
      assignment_id: lease.assignment_id,
      attempt_id: lease.attempt_id,
      fence: lease.fence,
      lease_token: lease.lease_token,
      heartbeat_sequence: nonNegativeInteger(heartbeatSequence, "heartbeat sequence"),
    });
    const heartbeat = parseHeartbeat(value);
    if (heartbeat.heartbeat_sequence !== heartbeatSequence
      || heartbeat.lease_expires_at_ms > heartbeat.hard_deadline_at_ms) {
      throw invalidResponse("Original-source heartbeat identity is inconsistent");
    }
    return heartbeat;
  }

  async complete(
    lease: OriginalSourceVerificationLease,
    requestId: string,
    observation: OriginalSourceVerificationPublishedObservation,
  ): Promise<OriginalSourceVerificationTerminal> {
    return this.terminal(lease, "complete", {
      request_id: exactId(requestId, "request ID"),
      observation,
    });
  }

  async fail(
    lease: OriginalSourceVerificationLease,
    requestId: string,
    errorCode: OriginalSourceVerificationFailureCode,
    observation: OriginalSourceVerificationPublishedObservation,
  ): Promise<OriginalSourceVerificationTerminal> {
    return this.terminal(lease, "fail", {
      request_id: exactId(requestId, "request ID"),
      error_code: errorCode,
      observation,
    });
  }

  private async terminal(
    lease: OriginalSourceVerificationLease,
    action: "complete" | "fail",
    terminalInput: Record<string, unknown>,
  ): Promise<OriginalSourceVerificationTerminal> {
    const value = await this.jsonWithRetry(assignmentPath(lease.assignment_id, action), {
      binding: this.binding,
      assignment_id: lease.assignment_id,
      attempt_id: lease.attempt_id,
      fence: lease.fence,
      lease_token: lease.lease_token,
      ...terminalInput,
    });
    const terminal = parseTerminal(value);
    if (terminal.assignment_id !== lease.assignment_id) {
      throw invalidResponse("Original-source terminal identity is inconsistent");
    }
    return terminal;
  }

  private async jsonWithRetry(path: string, input: unknown): Promise<unknown> {
    return authoritativeJson(await this.requestWithRetry(path, input));
  }

  private async requestWithRetry(path: string, input: unknown): Promise<Response> {
    const body = JSON.stringify(input);
    let lastError: unknown;
    for (let attempt = 1; attempt <= this.reportAttempts; attempt += 1) {
      try {
        return await this.request(path, body);
      } catch (error) {
        lastError = error;
        if (!retryable(error) || attempt === this.reportAttempts) throw error;
        await this.sleep(this.reportRetryMs * 2 ** (attempt - 1));
      }
    }
    throw lastError;
  }

  private async request(path: string, body: string): Promise<Response> {
    const headers = createJobsWorkerAuthHeaders({
      signingKey: this.signingKey,
      workerId: this.binding.worker_id,
      method: "POST",
      path,
      body,
    });
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.requestTimeoutMs);
    let response: Response;
    try {
      response = await this.fetcher(`${this.origin}${path}`, {
        method: "POST",
        headers: {
          ...headers,
          Accept: "application/json",
          "Content-Type": "application/json",
        },
        body,
        redirect: "error",
        signal: controller.signal,
      });
    } catch {
      throw new OriginalSourceVerificationApiError(
        controller.signal.aborted ? "api_timeout" : "api_unavailable",
        controller.signal.aborted
          ? "Original-source verification API timed out"
          : "Original-source verification API is unavailable",
      );
    } finally {
      clearTimeout(timer);
    }
    if (response.status === 204) return response;
    if (!response.ok) throw responseError(response.status);
    return response;
  }
}

function parseBinding(value: unknown): OriginalSourceVerifierBinding {
  const row = exactRecord(value, [
    "runtime_authority_sha256",
    "runtime_instance_epoch",
    "runtime_instance_id",
    "runtime_session_token",
    "worker_id",
  ], "runtime binding");
  const workerId = requiredString(row.worker_id, "worker ID", 128);
  if (!SAFE_WORKER_ID.test(workerId)) throw new Error("Original-source worker ID is invalid");
  return {
    worker_id: workerId,
    runtime_instance_id: exactId(row.runtime_instance_id, "runtime instance ID"),
    runtime_instance_epoch: positiveInteger(row.runtime_instance_epoch, "runtime instance epoch"),
    runtime_authority_sha256: sha256(row.runtime_authority_sha256, "runtime authority"),
    runtime_session_token: canonicalToken(row.runtime_session_token, "runtime session token"),
  };
}

function parseLease(value: unknown): OriginalSourceVerificationLease {
  const row = exactRecord(value, [
    "assignment_id",
    "attempt_id",
    "canonical_subject_json",
    "fence",
    "hard_deadline_at_ms",
    "heartbeat_sequence",
    "lease_expires_at_ms",
    "lease_token",
    "subject_sha256",
  ], "lease");
  const canonicalSubject = requiredString(
    row.canonical_subject_json,
    "canonical subject",
    128 * 1024,
  );
  const subjectSha256 = sha256(row.subject_sha256, "subject");
  if (originalSourceSha256(canonicalSubject) !== subjectSha256) {
    throw invalidResponse("Original-source lease subject digest is inconsistent");
  }
  const lease = {
    assignment_id: exactId(row.assignment_id, "assignment ID"),
    subject_sha256: subjectSha256,
    canonical_subject_json: canonicalSubject,
    attempt_id: exactId(row.attempt_id, "attempt ID"),
    fence: positiveInteger(row.fence, "fence"),
    lease_token: requiredString(row.lease_token, "lease token", 4_096),
    lease_expires_at_ms: positiveInteger(row.lease_expires_at_ms, "lease expiry"),
    hard_deadline_at_ms: positiveInteger(row.hard_deadline_at_ms, "hard deadline"),
    heartbeat_sequence: nonNegativeInteger(row.heartbeat_sequence, "heartbeat sequence"),
  } satisfies OriginalSourceVerificationLease;
  if (lease.lease_expires_at_ms > lease.hard_deadline_at_ms) {
    throw invalidResponse("Original-source lease time bounds are inconsistent");
  }
  return lease;
}

function parseHeartbeat(value: unknown): OriginalSourceVerificationHeartbeat {
  const row = exactRecord(value, [
    "hard_deadline_at_ms",
    "heartbeat_sequence",
    "lease_expires_at_ms",
    "replayed",
  ], "heartbeat");
  return {
    lease_expires_at_ms: positiveInteger(row.lease_expires_at_ms, "lease expiry"),
    hard_deadline_at_ms: positiveInteger(row.hard_deadline_at_ms, "hard deadline"),
    heartbeat_sequence: positiveInteger(row.heartbeat_sequence, "heartbeat sequence"),
    replayed: booleanValue(row.replayed, "heartbeat replay"),
  };
}

function parseTerminal(value: unknown): OriginalSourceVerificationTerminal {
  const row = exactRecord(value, [
    "assignment_id",
    "head",
    "receipt_sha256",
    "replayed",
    "state",
  ], "terminal");
  return {
    assignment_id: exactId(row.assignment_id, "assignment ID"),
    state: requiredString(row.state, "terminal state", 64),
    replayed: booleanValue(row.replayed, "terminal replay"),
    receipt_sha256: row.receipt_sha256 === null
      ? null
      : sha256(row.receipt_sha256, "receipt"),
    head: row.head === null ? null : parseHead(row.head),
  };
}

function parseHead(value: unknown): OriginalSourceVerificationHead {
  const row = exactRecord(value, [
    "assurance",
    "assignment_id",
    "checked_at_ms",
    "expires_at_ms",
    "head_revision",
    "material_generation",
    "material_sha256",
    "receipt_id",
    "receipt_sha256",
    "result",
    "subject_sha256",
  ], "head");
  const head = {
    head_revision: positiveInteger(row.head_revision, "head revision"),
    material_generation: positiveInteger(row.material_generation, "material generation"),
    assignment_id: exactId(row.assignment_id, "assignment ID"),
    receipt_id: exactId(row.receipt_id, "receipt ID"),
    receipt_sha256: sha256(row.receipt_sha256, "receipt"),
    subject_sha256: sha256(row.subject_sha256, "subject"),
    material_sha256: sha256(row.material_sha256, "material"),
    assurance: requiredString(row.assurance, "assurance", 64),
    result: requiredString(row.result, "result", 64),
    checked_at_ms: positiveInteger(row.checked_at_ms, "check time"),
    expires_at_ms: positiveInteger(row.expires_at_ms, "expiry"),
  } satisfies OriginalSourceVerificationHead;
  if (head.checked_at_ms >= head.expires_at_ms) {
    throw invalidResponse("Original-source head time bounds are inconsistent");
  }
  return head;
}

async function authoritativeJson(response: Response): Promise<unknown> {
  if (response.status !== 200
    || response.headers.get("content-type") !== "application/json"
    || response.headers.get("cache-control") !== "no-store"
    || response.headers.get("x-content-type-options") !== "nosniff") {
    throw invalidResponse("Original-source verification API response is not authoritative");
  }
  const text = await response.text();
  if (Buffer.byteLength(text, "utf8") > MAX_RESPONSE_BYTES) {
    throw invalidResponse("Original-source verification API response is too large");
  }
  try {
    return JSON.parse(text) as unknown;
  } catch {
    throw invalidResponse("Original-source verification API response is invalid");
  }
}

function assignmentPath(assignmentId: string, action: "complete" | "fail" | "heartbeat"): string {
  return `/api/jobs/internal/original-source-verifications/${exactId(
    assignmentId,
    "assignment ID",
  )}/${action}`;
}

function normalizeOrigin(value: string): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("Original-source verification API origin is invalid");
  }
  const loopback = url.protocol === "http:"
    && ["127.0.0.1", "localhost", "::1"].includes(url.hostname);
  if ((url.protocol !== "https:" && !loopback)
    || url.username
    || url.password
    || url.search
    || url.hash
    || !["", "/"].includes(url.pathname)) {
    throw new Error("Original-source verification API origin is invalid");
  }
  url.pathname = "";
  return url.toString().replace(/\/$/, "");
}

function retryable(error: unknown): boolean {
  return error instanceof OriginalSourceVerificationApiError
    && ["api_timeout", "api_unavailable", "authority_unavailable"].includes(error.code);
}

function responseError(status: number): OriginalSourceVerificationApiError {
  if (status === 400) return new OriginalSourceVerificationApiError("api_rejected", "Invalid request", status);
  if (status === 404) return new OriginalSourceVerificationApiError("not_found", "Assignment missing", status);
  if (status === 409) return new OriginalSourceVerificationApiError("conflict", "Lease conflict", status);
  if (status === 503) {
    return new OriginalSourceVerificationApiError(
      "authority_unavailable",
      "Managed verifier authority unavailable",
      status,
    );
  }
  return new OriginalSourceVerificationApiError("api_unavailable", "API failed", status);
}

function invalidResponse(message: string): OriginalSourceVerificationApiError {
  return new OriginalSourceVerificationApiError("invalid_response", message);
}

function exactRecord(value: unknown, keys: readonly string[], label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw invalidResponse(`Original-source ${label} is invalid`);
  }
  const row = value as Record<string, unknown>;
  const actual = Object.keys(row).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw invalidResponse(`Original-source ${label} shape is invalid`);
  }
  return row;
}

function exactId(value: unknown, label: string): string {
  if (typeof value !== "string" || !SAFE_ID.test(value)) {
    throw invalidResponse(`Original-source ${label} is invalid`);
  }
  return value;
}

function requiredString(value: unknown, label: string, maximumBytes: number): string {
  if (typeof value !== "string"
    || value.length === 0
    || value !== value.trim()
    || Buffer.byteLength(value, "utf8") > maximumBytes
    || /\p{Cc}/u.test(value)) {
    throw invalidResponse(`Original-source ${label} is invalid`);
  }
  return value;
}

function sha256(value: unknown, label: string): string {
  if (typeof value !== "string" || !SHA256.test(value)) {
    throw invalidResponse(`Original-source ${label} SHA-256 is invalid`);
  }
  return value;
}

function canonicalToken(value: unknown, label: string): string {
  if (typeof value !== "string" || !TOKEN.test(value)) {
    throw invalidResponse(`Original-source ${label} is invalid`);
  }
  const bytes = Buffer.from(value, "base64url");
  if (bytes.length !== 32 || bytes.toString("base64url") !== value) {
    throw invalidResponse(`Original-source ${label} is invalid`);
  }
  return value;
}

function positiveInteger(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 1) {
    throw invalidResponse(`Original-source ${label} is invalid`);
  }
  return value as number;
}

function nonNegativeInteger(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw invalidResponse(`Original-source ${label} is invalid`);
  }
  return value as number;
}

function booleanValue(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") throw invalidResponse(`Original-source ${label} is invalid`);
  return value;
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}
