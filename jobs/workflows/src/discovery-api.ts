import { randomUUID } from "node:crypto";
import { workerAuthHeaders } from "./worker-auth.js";

const DEFAULT_REQUEST_TIMEOUT_MS = 10_000;
const DEFAULT_REPORT_ATTEMPTS = 3;
const DEFAULT_REPORT_RETRY_MS = 250;
const MAX_LEASE_RESPONSE_BYTES = 256 * 1024;
const MAX_TIMER_MS = 2_147_000_000;
const SAFE_WORKER_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const PROCESS_DISCOVERY_WORKER_ID = `discovery-${process.pid}-${randomUUID()}`;

export interface DiscoverySourceRecord {
  id: string;
  account_id: string;
  track_id: string;
  provider: string;
  source_key: string;
  config: unknown;
  status: string;
  health: string;
  run_interval_ms: number;
  consecutive_failures?: number;
  next_run_at_ms?: number;
  last_success_at_ms?: number | null;
  last_failure_at_ms?: number | null;
  last_error_code?: string | null;
  lease_expires_at_ms?: number | null;
}

export interface DiscoverySourceLease {
  source: DiscoverySourceRecord;
  lease_token: string;
  replay_key: string;
  scheduled_for_ms: number;
}

export interface DiscoveredJobInput {
  external_id: string;
  canonical_url: string;
  title: string;
  location: string;
  workplace: string;
  description: string;
  compensation: string;
  posted_at_ms: number | null;
}

export interface DiscoveryCompleteInput {
  lease_token: string;
  replay_key: string;
  scheduled_for_ms: number;
  jobs: DiscoveredJobInput[];
  complete_snapshot: true;
}

export interface DiscoveryFailInput {
  lease_token: string;
  replay_key: string;
  scheduled_for_ms: number;
  error_code: string;
}

export interface DiscoveryWorkerApi {
  lease(): Promise<DiscoverySourceLease | null>;
  complete(sourceId: string, input: DiscoveryCompleteInput): Promise<void>;
  fail(sourceId: string, input: DiscoveryFailInput): Promise<void>;
}

export type DiscoveryApiErrorCode =
  | "api_rejected"
  | "api_timeout"
  | "api_unavailable"
  | "invalid_response";

export class DiscoveryApiError extends Error {
  readonly code: DiscoveryApiErrorCode;
  readonly status?: number;

  constructor(code: DiscoveryApiErrorCode, message: string, status?: number) {
    super(message);
    this.name = "DiscoveryApiError";
    this.code = code;
    this.status = status;
  }
}

export type DiscoveryApiFetch = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;

export interface DiscoveryApiClientOptions {
  origin: string;
  signingKey: string;
  workerId?: string;
  fetch?: DiscoveryApiFetch;
  requestTimeoutMs?: number;
  reportAttempts?: number;
  reportRetryMs?: number;
  sleep?: (milliseconds: number) => Promise<void>;
}

export class DiscoveryApiClient implements DiscoveryWorkerApi {
  private readonly origin: string;
  private readonly signingKey: string;
  private readonly workerId: string;
  private readonly fetcher: DiscoveryApiFetch;
  private readonly requestTimeoutMs: number;
  private readonly reportAttempts: number;
  private readonly reportRetryMs: number;
  private readonly sleep: (milliseconds: number) => Promise<void>;

  constructor(options: DiscoveryApiClientOptions) {
    this.origin = normalizeOrigin(options.origin);
    if (!options.signingKey) throw new Error("BLUEY_JOBS_WORKER_SIGNING_KEY is required");
    this.signingKey = options.signingKey;
    this.workerId = normalizedWorkerId(options.workerId ?? PROCESS_DISCOVERY_WORKER_ID);
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
    this.sleep = options.sleep ?? ((milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)));
  }

  async lease(): Promise<DiscoverySourceLease | null> {
    const response = await this.request("/api/jobs/internal/discovery/lease", {
      method: "POST",
      headers: { Accept: "application/json" },
    });
    if (response.status === 204) return null;
    if (!response.ok) throw responseError(response.status);
    const text = await response.text();
    if (Buffer.byteLength(text, "utf8") > MAX_LEASE_RESPONSE_BYTES) {
      throw new DiscoveryApiError("invalid_response", "Discovery lease response is too large");
    }
    try {
      return parseLease(JSON.parse(text) as unknown);
    } catch (error) {
      if (error instanceof DiscoveryApiError) throw error;
      throw new DiscoveryApiError("invalid_response", "Discovery lease response is invalid");
    }
  }

  async complete(sourceId: string, input: DiscoveryCompleteInput): Promise<void> {
    await this.report(
      `/api/jobs/internal/discovery/${encodeURIComponent(requiredId(sourceId, "source ID"))}/complete`,
      input,
    );
  }

  async fail(sourceId: string, input: DiscoveryFailInput): Promise<void> {
    await this.report(
      `/api/jobs/internal/discovery/${encodeURIComponent(requiredId(sourceId, "source ID"))}/fail`,
      input,
    );
  }

  private async report(path: string, input: DiscoveryCompleteInput | DiscoveryFailInput): Promise<void> {
    const body = JSON.stringify(input);
    let lastError: unknown;
    for (let attempt = 1; attempt <= this.reportAttempts; attempt += 1) {
      try {
        const response = await this.request(path, {
          method: "POST",
          headers: { Accept: "application/json", "Content-Type": "application/json" },
          body,
        });
        if (!response.ok) throw responseError(response.status);
        return;
      } catch (error) {
        lastError = error;
        if (!retryableReportError(error) || attempt === this.reportAttempts) throw error;
        await this.sleep(this.reportRetryMs * 2 ** (attempt - 1));
      }
    }
    throw lastError;
  }

  private async request(path: string, init: RequestInit): Promise<Response> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.requestTimeoutMs);
    try {
      if (init.body !== undefined && typeof init.body !== "string") {
        throw new DiscoveryApiError("invalid_response", "Jobs worker request body must be encoded JSON");
      }
      return await this.fetcher(`${this.origin}${path}`, {
        ...init,
        headers: {
          ...init.headers,
          ...workerAuthHeaders({
            signingKey: this.signingKey,
            workerId: this.workerId,
            method: init.method || "GET",
            path,
            body: init.body,
          }),
        },
        redirect: "error",
        signal: controller.signal,
      });
    } catch (error) {
      if (controller.signal.aborted) {
        throw new DiscoveryApiError("api_timeout", "Jobs API request timed out");
      }
      throw new DiscoveryApiError("api_unavailable", "Jobs API request failed");
    } finally {
      clearTimeout(timer);
    }
  }
}

function parseLease(value: unknown): DiscoverySourceLease {
  const lease = record(value, "lease");
  const source = record(lease.source, "source");
  return {
    source: {
      id: requiredString(source.id, "source ID", 200),
      account_id: requiredString(source.account_id, "account ID", 200),
      track_id: requiredString(source.track_id, "track ID", 200, true),
      provider: requiredString(source.provider, "provider", 128),
      source_key: requiredString(source.source_key, "source key", 200),
      config: source.config,
      status: requiredString(source.status, "source status", 32),
      health: requiredString(source.health, "source health", 32),
      run_interval_ms: requiredInteger(source.run_interval_ms, "run interval", 1, MAX_TIMER_MS),
      consecutive_failures: optionalInteger(source.consecutive_failures, "consecutive failures", 0, 1_000_000),
      next_run_at_ms: optionalInteger(source.next_run_at_ms, "next run time", 0, Number.MAX_SAFE_INTEGER),
      last_success_at_ms: optionalNullableInteger(source.last_success_at_ms, "last success time"),
      last_failure_at_ms: optionalNullableInteger(source.last_failure_at_ms, "last failure time"),
      last_error_code: optionalNullableString(source.last_error_code, "last error code", 128),
      lease_expires_at_ms: optionalNullableInteger(source.lease_expires_at_ms, "lease expiry"),
    },
    lease_token: requiredString(lease.lease_token, "lease token", 4_096),
    replay_key: requiredString(lease.replay_key, "replay key", 512),
    scheduled_for_ms: requiredInteger(lease.scheduled_for_ms, "scheduled time", 0, Number.MAX_SAFE_INTEGER),
  };
}

function retryableReportError(error: unknown): boolean {
  if (!(error instanceof DiscoveryApiError)) return false;
  return error.code === "api_timeout"
    || error.code === "api_unavailable"
    || error.status === 408
    || error.status === 429
    || (error.status !== undefined && error.status >= 500);
}

function responseError(status: number): DiscoveryApiError {
  const retryable = status === 408 || status === 429 || status >= 500;
  return new DiscoveryApiError(
    retryable ? "api_unavailable" : "api_rejected",
    "Jobs API rejected a discovery request",
    status,
  );
}

function normalizeOrigin(value: string): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("BLUEY_JOBS_API_ORIGIN is invalid");
  }
  const loopback = ["localhost", "127.0.0.1", "::1", "[::1]"].includes(url.hostname);
  if (!["http:", "https:"].includes(url.protocol)
    || (url.protocol === "http:" && !loopback)
    || url.username
    || url.password
    || url.search
    || url.hash
    || (url.pathname !== "/" && url.pathname !== "")) {
    throw new Error("BLUEY_JOBS_API_ORIGIN is invalid");
  }
  return url.origin;
}

function normalizedWorkerId(value: string): string {
  if (!SAFE_WORKER_ID.test(value)) {
    throw new Error("Bluey Jobs discovery worker ID is invalid");
  }
  return value;
}

function record(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new DiscoveryApiError("invalid_response", `Discovery ${label} is invalid`);
  }
  return value as Record<string, unknown>;
}

function requiredId(value: string, label: string): string {
  if (!value || value.length > 200) throw new Error(`Discovery ${label} is invalid`);
  return value;
}

function requiredString(value: unknown, label: string, maximum: number, allowEmpty = false): string {
  if (typeof value !== "string" || value.length > maximum || (!allowEmpty && value.length === 0)) {
    throw new DiscoveryApiError("invalid_response", `Discovery ${label} is invalid`);
  }
  return value;
}

function optionalNullableString(value: unknown, label: string, maximum: number): string | null | undefined {
  if (value === undefined || value === null) return value;
  return requiredString(value, label, maximum, true);
}

function requiredInteger(
  value: unknown,
  label: string,
  minimum: number,
  maximum: number,
): number {
  if (!Number.isSafeInteger(value) || (value as number) < minimum || (value as number) > maximum) {
    throw new DiscoveryApiError("invalid_response", `Discovery ${label} is invalid`);
  }
  return value as number;
}

function optionalInteger(
  value: unknown,
  label: string,
  minimum: number,
  maximum: number,
): number | undefined {
  return value === undefined ? undefined : requiredInteger(value, label, minimum, maximum);
}

function optionalNullableInteger(value: unknown, label: string): number | null | undefined {
  if (value === undefined || value === null) return value;
  return requiredInteger(value, label, 0, Number.MAX_SAFE_INTEGER);
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Discovery ${label} must be an integer from ${minimum} to ${maximum}`);
  }
  return value;
}
