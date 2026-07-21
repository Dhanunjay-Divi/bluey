import { randomUUID } from "node:crypto";
import { createJobsWorkerAuthHeaders } from "@bluey/jobs-automation/worker-auth";

const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;
const DEFAULT_REPORT_ATTEMPTS = 3;
const DEFAULT_REPORT_RETRY_MS = 250;
const MAX_RESPONSE_BYTES = 1024 * 1024;
const MAX_TIMER_MS = 2_147_000_000;
const SAFE_WORKER_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const PROCESS_WORKER_ID = `global-discovery-${process.pid}-${randomUUID()}`;

export interface GlobalDiscoverySourceInput {
  provider: "jobhive";
  source_key: string;
  source_family: string;
  artifact_url: string;
  artifact_sha256: string;
  expected_rows: number;
  snapshot_at_ms: number;
  run_interval_ms: number;
}

export interface GlobalDiscoverySourceRecord {
  id: string;
  provider: string;
  source_key: string;
  config: unknown;
  status: string;
  health: string;
  consecutive_failures: number;
  run_interval_ms: number;
  next_run_at_ms: number;
  last_success_at_ms: number | null;
  last_failure_at_ms: number | null;
  last_error_code: string | null;
  lease_expires_at_ms: number | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface GlobalDiscoverySourceLease {
  source: GlobalDiscoverySourceRecord;
  lease_token: string;
  replay_key: string;
  scheduled_for_ms: number;
}

export interface GlobalDiscoveredJobInput {
  external_id: string;
  canonical_url: string;
  title: string;
  company: string;
  source_catalog_id: string;
  requires_original_revalidation: true;
  location: string;
  workplace: string;
  description: string;
  compensation: string;
  employment_type: string;
  engagement_type: string;
  posted_at_ms: number | null;
}

export interface GlobalIngestionBatchInput {
  lease_token: string;
  replay_key: string;
  scheduled_for_ms: number;
  batch_index: number;
  artifact_sha256: string;
  jobs: GlobalDiscoveredJobInput[];
}

export interface GlobalIngestionBatchResult {
  run_id: string;
  batch_index: number;
  row_count: number;
  received_rows: number;
  received_batches: number;
  replayed: boolean;
}

export interface GlobalIngestionCompleteInput {
  lease_token: string;
  replay_key: string;
  scheduled_for_ms: number;
  artifact_sha256: string;
  expected_rows: number;
  expected_batches: number;
  complete_snapshot: true;
}

export interface GlobalIngestionFailureInput {
  lease_token: string;
  replay_key: string;
  scheduled_for_ms: number;
  artifact_sha256: string;
  error_code: string;
}

export interface GlobalIngestionRunResult {
  run_id: string;
  replay_key: string;
  status: string;
  received_rows: number;
  received_batches: number;
  expired_count: number;
  replayed: boolean;
}

export interface GlobalDiscoveryWorkerApi {
  syncSources(sources: GlobalDiscoverySourceInput[]): Promise<GlobalDiscoverySourceRecord[]>;
  lease(): Promise<GlobalDiscoverySourceLease | null>;
  ingestBatch(sourceId: string, input: GlobalIngestionBatchInput): Promise<GlobalIngestionBatchResult>;
  complete(sourceId: string, input: GlobalIngestionCompleteInput): Promise<GlobalIngestionRunResult>;
  fail(sourceId: string, input: GlobalIngestionFailureInput): Promise<GlobalIngestionRunResult>;
}

export type GlobalDiscoveryApiErrorCode =
  | "api_rejected"
  | "api_timeout"
  | "api_unavailable"
  | "invalid_response";

export class GlobalDiscoveryApiError extends Error {
  readonly code: GlobalDiscoveryApiErrorCode;
  readonly status?: number;

  constructor(code: GlobalDiscoveryApiErrorCode, message: string, status?: number) {
    super(message);
    this.name = "GlobalDiscoveryApiError";
    this.code = code;
    this.status = status;
  }
}

export type GlobalDiscoveryApiFetch = (
  input: string | URL | Request,
  init?: RequestInit,
) => Promise<Response>;

export interface GlobalDiscoveryApiClientOptions {
  origin: string;
  signingKey: string;
  workerId?: string;
  fetch?: GlobalDiscoveryApiFetch;
  requestTimeoutMs?: number;
  reportAttempts?: number;
  reportRetryMs?: number;
  sleep?: (milliseconds: number) => Promise<void>;
}

export class GlobalDiscoveryApiClient implements GlobalDiscoveryWorkerApi {
  private readonly origin: string;
  private readonly signingKey: string;
  private readonly workerId: string;
  private readonly fetcher: GlobalDiscoveryApiFetch;
  private readonly requestTimeoutMs: number;
  private readonly reportAttempts: number;
  private readonly reportRetryMs: number;
  private readonly sleep: (milliseconds: number) => Promise<void>;

  constructor(options: GlobalDiscoveryApiClientOptions) {
    this.origin = normalizeOrigin(options.origin);
    if (Buffer.byteLength(options.signingKey, "utf8") < 32) {
      throw new Error("BLUEY_JOBS_WORKER_SIGNING_KEY must contain at least 32 bytes");
    }
    this.signingKey = options.signingKey;
    this.workerId = normalizeWorkerId(options.workerId ?? PROCESS_WORKER_ID);
    this.fetcher = options.fetch ?? fetch;
    this.requestTimeoutMs = boundedInteger(
      options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS,
      100,
      15 * 60_000,
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
    this.sleep = options.sleep ?? ((milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)));
  }

  async syncSources(sources: GlobalDiscoverySourceInput[]): Promise<GlobalDiscoverySourceRecord[]> {
    const value = await this.report("/api/jobs/internal/global-discovery/sources/sync", { sources });
    if (!Array.isArray(value)) throw invalidResponse("Global discovery source response is invalid");
    return value.map(parseSource);
  }

  async lease(): Promise<GlobalDiscoverySourceLease | null> {
    const response = await this.request("/api/jobs/internal/global-discovery/lease", {
      method: "POST",
      headers: { Accept: "application/json" },
    });
    if (response.status === 204) return null;
    return parseLease(await readJson(response));
  }

  async ingestBatch(
    sourceId: string,
    input: GlobalIngestionBatchInput,
  ): Promise<GlobalIngestionBatchResult> {
    return parseBatchResult(await this.report(sourcePath(sourceId, "batches"), input));
  }

  async complete(
    sourceId: string,
    input: GlobalIngestionCompleteInput,
  ): Promise<GlobalIngestionRunResult> {
    return parseRunResult(await this.report(sourcePath(sourceId, "complete"), input));
  }

  async fail(
    sourceId: string,
    input: GlobalIngestionFailureInput,
  ): Promise<GlobalIngestionRunResult> {
    return parseRunResult(await this.report(sourcePath(sourceId, "fail"), input));
  }

  private async report(path: string, input: unknown): Promise<unknown> {
    const body = JSON.stringify(input);
    let lastError: unknown;
    for (let attempt = 1; attempt <= this.reportAttempts; attempt += 1) {
      try {
        return await readJson(await this.request(path, {
          method: "POST",
          headers: { Accept: "application/json", "Content-Type": "application/json" },
          body,
        }));
      } catch (error) {
        lastError = error;
        if (!retryable(error) || attempt === this.reportAttempts) throw error;
        await this.sleep(this.reportRetryMs * 2 ** (attempt - 1));
      }
    }
    throw lastError;
  }

  private async request(path: string, init: RequestInit): Promise<Response> {
    if (init.body !== undefined && init.body !== null && typeof init.body !== "string") {
      throw new GlobalDiscoveryApiError("api_rejected", "Global discovery request body is not signable");
    }
    const headers = new Headers(init.headers);
    const workerHeaders = createJobsWorkerAuthHeaders({
      signingKey: this.signingKey,
      workerId: this.workerId,
      method: init.method ?? "GET",
      path,
      body: init.body ?? undefined,
    });
    for (const [name, value] of Object.entries(workerHeaders)) headers.set(name, value);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.requestTimeoutMs);
    try {
      const response = await this.fetcher(`${this.origin}${path}`, {
        ...init,
        headers,
        redirect: "error",
        signal: controller.signal,
      });
      if (!response.ok) throw responseError(response.status);
      return response;
    } catch (error) {
      if (error instanceof GlobalDiscoveryApiError) throw error;
      if (controller.signal.aborted) {
        throw new GlobalDiscoveryApiError("api_timeout", "Global discovery API request timed out");
      }
      throw new GlobalDiscoveryApiError("api_unavailable", "Global discovery API request failed");
    } finally {
      clearTimeout(timer);
    }
  }
}

async function readJson(response: Response): Promise<unknown> {
  const declaredLength = Number(response.headers.get("content-length") ?? "0");
  if (Number.isFinite(declaredLength) && declaredLength > MAX_RESPONSE_BYTES) {
    throw invalidResponse("Global discovery API response is too large");
  }
  const text = await response.text();
  if (Buffer.byteLength(text, "utf8") > MAX_RESPONSE_BYTES) {
    throw invalidResponse("Global discovery API response is too large");
  }
  try {
    return JSON.parse(text) as unknown;
  } catch {
    throw invalidResponse("Global discovery API response is invalid");
  }
}

function parseSource(value: unknown): GlobalDiscoverySourceRecord {
  const source = record(value, "source");
  return {
    id: requiredString(source.id, "source ID", 200),
    provider: requiredString(source.provider, "provider", 128),
    source_key: requiredString(source.source_key, "source key", 200),
    config: source.config,
    status: requiredString(source.status, "source status", 32),
    health: requiredString(source.health, "source health", 32),
    consecutive_failures: requiredInteger(source.consecutive_failures, "consecutive failures", 0, 1_000_000),
    run_interval_ms: requiredInteger(source.run_interval_ms, "run interval", 1, MAX_TIMER_MS),
    next_run_at_ms: requiredInteger(source.next_run_at_ms, "next run time", 0, Number.MAX_SAFE_INTEGER),
    last_success_at_ms: nullableInteger(source.last_success_at_ms, "last success time"),
    last_failure_at_ms: nullableInteger(source.last_failure_at_ms, "last failure time"),
    last_error_code: nullableString(source.last_error_code, "last error code", 128),
    lease_expires_at_ms: nullableInteger(source.lease_expires_at_ms, "lease expiry"),
    created_at_ms: requiredInteger(source.created_at_ms, "created time", 0, Number.MAX_SAFE_INTEGER),
    updated_at_ms: requiredInteger(source.updated_at_ms, "updated time", 0, Number.MAX_SAFE_INTEGER),
  };
}

function parseLease(value: unknown): GlobalDiscoverySourceLease {
  const lease = record(value, "lease");
  return {
    source: parseSource(lease.source),
    lease_token: requiredString(lease.lease_token, "lease token", 4096),
    replay_key: requiredString(lease.replay_key, "replay key", 512),
    scheduled_for_ms: requiredInteger(lease.scheduled_for_ms, "scheduled time", 0, Number.MAX_SAFE_INTEGER),
  };
}

function parseBatchResult(value: unknown): GlobalIngestionBatchResult {
  const result = record(value, "batch result");
  return {
    run_id: requiredString(result.run_id, "run ID", 200),
    batch_index: requiredInteger(result.batch_index, "batch index", 0, Number.MAX_SAFE_INTEGER),
    row_count: requiredInteger(result.row_count, "row count", 1, 1_000),
    received_rows: requiredInteger(result.received_rows, "received rows", 0, 5_000_000),
    received_batches: requiredInteger(result.received_batches, "received batches", 0, 100_000),
    replayed: requiredBoolean(result.replayed, "replayed flag"),
  };
}

function parseRunResult(value: unknown): GlobalIngestionRunResult {
  const result = record(value, "run result");
  return {
    run_id: requiredString(result.run_id, "run ID", 200),
    replay_key: requiredString(result.replay_key, "replay key", 512),
    status: requiredString(result.status, "run status", 32),
    received_rows: requiredInteger(result.received_rows, "received rows", 0, 5_000_000),
    received_batches: requiredInteger(result.received_batches, "received batches", 0, 100_000),
    expired_count: requiredInteger(result.expired_count, "expired count", 0, 5_000_000),
    replayed: requiredBoolean(result.replayed, "replayed flag"),
  };
}

function sourcePath(sourceId: string, action: string): string {
  const id = requiredString(sourceId, "source ID", 200);
  return `/api/jobs/internal/global-discovery/${encodeURIComponent(id)}/${action}`;
}

function normalizeOrigin(value: string): string {
  let origin: URL;
  try {
    origin = new URL(value);
  } catch {
    throw new Error("BLUEY_JOBS_API_ORIGIN is invalid");
  }
  const loopback = origin.hostname === "127.0.0.1" || origin.hostname === "[::1]" || origin.hostname === "localhost";
  if ((origin.protocol !== "https:" && !(origin.protocol === "http:" && loopback))
    || origin.username || origin.password || origin.pathname !== "/" || origin.search || origin.hash) {
    throw new Error("BLUEY_JOBS_API_ORIGIN is invalid");
  }
  return origin.origin;
}

function normalizeWorkerId(value: string): string {
  const normalized = value.trim();
  if (!SAFE_WORKER_ID.test(normalized)) throw new Error("Global discovery worker ID is invalid");
  return normalized;
}

function retryable(error: unknown): boolean {
  return error instanceof GlobalDiscoveryApiError
    && (error.code === "api_timeout"
      || error.code === "api_unavailable"
      || (error.status !== undefined && error.status >= 500));
}

function responseError(status: number): GlobalDiscoveryApiError {
  return new GlobalDiscoveryApiError(
    status >= 500 ? "api_unavailable" : "api_rejected",
    `Global discovery API returned HTTP ${status}`,
    status,
  );
}

function invalidResponse(message: string): GlobalDiscoveryApiError {
  return new GlobalDiscoveryApiError("invalid_response", message);
}

function record(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw invalidResponse(`Global discovery ${label} is invalid`);
  }
  return value as Record<string, unknown>;
}

function requiredString(value: unknown, label: string, maximum: number): string {
  if (typeof value !== "string" || value.trim() === "" || value.length > maximum) {
    throw invalidResponse(`Global discovery ${label} is invalid`);
  }
  return value;
}

function requiredInteger(value: unknown, label: string, minimum: number, maximum: number): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw invalidResponse(`Global discovery ${label} is invalid`);
  }
  return value;
}

function nullableInteger(value: unknown, label: string): number | null {
  if (value === null || value === undefined) return null;
  return requiredInteger(value, label, 0, Number.MAX_SAFE_INTEGER);
}

function nullableString(value: unknown, label: string, maximum: number): string | null {
  if (value === null || value === undefined) return null;
  return requiredString(value, label, maximum);
}

function requiredBoolean(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") throw invalidResponse(`Global discovery ${label} is invalid`);
  return value;
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Global discovery ${label} is outside the allowed range`);
  }
  return value;
}
