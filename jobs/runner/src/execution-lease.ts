import { createHash, randomBytes } from "node:crypto";
import { createJobsWorkerAuthHeaders } from "@bluey/jobs-automation/worker-auth";

const DEFAULT_REQUEST_TIMEOUT_MS = 5_000;
const DEFAULT_HEARTBEAT_INTERVAL_MS = 10_000;
const DEFAULT_MAX_RESPONSE_BYTES = 16 * 1024;
const MAX_OWNER_BYTES = 128;
const MAX_WORKER_SIGNING_KEY_BYTES = 4_096;
const MAX_LEASE_TOKEN_BYTES = 256;
const PROCESS_OWNER_ID = `runner-${process.pid}-${randomBytes(8).toString("hex")}`;

export type ExecutionLeaseFinishOutcome =
  | "released"
  | "failed"
  | "side_effect_unknown"
  | "submitted";

export type ExecutionLeaseErrorCode =
  | "configuration"
  | "invalid_response"
  | "lease_unavailable"
  | "redirect_blocked"
  | "request_failed"
  | "response_too_large"
  | "timed_out"
  | "invalid_state";

export interface ExecutionLeaseClaim {
  accountId: string;
  applicationId: string;
  runId: string;
  browserProfileId: string;
}

export interface ExecutionLeaseClientOptions {
  origin: string;
  workerSigningKey: string;
  ownerId: string;
  requestTimeoutMs?: number;
  heartbeatIntervalMs?: number;
  maxResponseBytes?: number;
  fetch?: typeof globalThis.fetch;
}

export class ExecutionLeaseError extends Error {
  constructor(
    readonly operation: "claim" | "heartbeat" | "irreversible" | "finish" | "configuration",
    readonly code: ExecutionLeaseErrorCode,
    readonly status?: number,
  ) {
    super(`Execution lease ${operation} failed (${code}).`);
    this.name = "ExecutionLeaseError";
  }
}

interface LeaseGrant {
  leaseToken: string;
  fence: number;
  expiresAtMs: number;
}

interface LeaseOperations {
  heartbeat(): Promise<void>;
  irreversible(): Promise<void>;
  finish(outcome: ExecutionLeaseFinishOutcome): Promise<void>;
}

export class ActiveExecutionLease {
  readonly #operations: LeaseOperations;
  readonly #heartbeatIntervalMs: number;
  #heartbeatTimer?: ReturnType<typeof setTimeout>;
  #heartbeatInFlight?: Promise<void>;
  #heartbeatStopped = false;
  #heartbeatFailure?: ExecutionLeaseError;
  #finishOutcome?: ExecutionLeaseFinishOutcome;
  #finishPromise?: Promise<void>;
  #finalSubmitAttempted = false;
  #finalSubmitAuthorized = false;
  #activationOutcome?: "activated" | "activation_uncertain";

  constructor(
    operations: LeaseOperations,
    heartbeatIntervalMs: number,
    readonly fence: number = 1,
    readonly expiresAtMs: number = Date.now() + heartbeatIntervalMs,
    readonly ownerId: string = "runner-unknown",
  ) {
    this.#operations = operations;
    this.#heartbeatIntervalMs = heartbeatIntervalMs;
    this.scheduleHeartbeat();
  }

  get finalSubmitAttempted(): boolean {
    return this.#finalSubmitAttempted;
  }

  get finalSubmitAuthorized(): boolean {
    return this.#finalSubmitAuthorized;
  }

  get finalSubmitActivationOutcome(): "activated" | "activation_uncertain" | undefined {
    return this.#activationOutcome;
  }

  get heartbeatFailureCode(): ExecutionLeaseErrorCode | undefined {
    return this.#heartbeatFailure?.code;
  }

  get heartbeatActive(): boolean {
    return !this.#heartbeatStopped;
  }

  async beforeFinalSubmit(): Promise<void> {
    if (this.#finishPromise || this.#finalSubmitAttempted) {
      throw new ExecutionLeaseError("irreversible", "invalid_state");
    }
    // Set this before I/O: a lost success response must permanently consume the local attempt.
    this.#finalSubmitAttempted = true;
    await this.#operations.irreversible();
    this.#finalSubmitAuthorized = true;
  }

  async afterFinalSubmit(outcome: "activated" | "activation_uncertain"): Promise<void> {
    if (!this.#finalSubmitAuthorized || this.#activationOutcome) {
      throw new ExecutionLeaseError("irreversible", "invalid_state");
    }
    this.#activationOutcome = outcome;
  }

  async stopHeartbeat(): Promise<void> {
    if (this.#heartbeatStopped) return;
    this.#heartbeatStopped = true;
    if (this.#heartbeatTimer) clearTimeout(this.#heartbeatTimer);
    this.#heartbeatTimer = undefined;
    await this.#heartbeatInFlight;
  }

  finish(outcome: ExecutionLeaseFinishOutcome): Promise<void> {
    if (this.#finishPromise) {
      if (this.#finishOutcome !== outcome) {
        return Promise.reject(new ExecutionLeaseError("finish", "invalid_state"));
      }
      return this.#finishPromise;
    }
    this.#finishOutcome = outcome;
    this.#finishPromise = (async () => {
      await this.stopHeartbeat();
      await this.#operations.finish(outcome);
    })();
    return this.#finishPromise;
  }

  private scheduleHeartbeat(): void {
    if (this.#heartbeatStopped) return;
    this.#heartbeatTimer = setTimeout(() => {
      this.#heartbeatTimer = undefined;
      const heartbeat = this.#operations.heartbeat()
        .then(() => { this.#heartbeatFailure = undefined; })
        .catch((error: unknown) => {
          this.#heartbeatFailure = asLeaseError("heartbeat", error);
        })
        .finally(() => {
          if (this.#heartbeatInFlight === heartbeat) this.#heartbeatInFlight = undefined;
          this.scheduleHeartbeat();
        });
      this.#heartbeatInFlight = heartbeat;
    }, this.#heartbeatIntervalMs);
    this.#heartbeatTimer.unref?.();
  }
}

export class ExecutionLeaseClient {
  readonly #origin: string;
  readonly #workerSigningKey: string;
  readonly #ownerId: string;
  readonly #requestTimeoutMs: number;
  readonly #heartbeatIntervalMs: number;
  readonly #maxResponseBytes: number;
  readonly #fetch: typeof globalThis.fetch;

  constructor(options: ExecutionLeaseClientOptions) {
    this.#origin = normalizedOrigin(options.origin);
    this.#workerSigningKey = boundedSigningKey(options.workerSigningKey);
    this.#ownerId = boundedOwnerId(options.ownerId);
    this.#requestTimeoutMs = boundedInteger(options.requestTimeoutMs, DEFAULT_REQUEST_TIMEOUT_MS, 100, 60_000);
    this.#heartbeatIntervalMs = boundedInteger(
      options.heartbeatIntervalMs,
      DEFAULT_HEARTBEAT_INTERVAL_MS,
      100,
      60_000,
    );
    this.#maxResponseBytes = boundedInteger(
      options.maxResponseBytes,
      DEFAULT_MAX_RESPONSE_BYTES,
      256,
      1024 * 1024,
    );
    this.#fetch = options.fetch ?? globalThis.fetch;
    if (typeof this.#fetch !== "function") {
      throw new ExecutionLeaseError("configuration", "configuration");
    }
  }

  async claim(input: ExecutionLeaseClaim): Promise<ActiveExecutionLease> {
    const common = {
      account_id: input.accountId,
      application_id: input.applicationId,
    };
    const payload = await this.request("claim", "/api/jobs/internal/execution-leases/claim", {
      ...common,
      run_id: input.runId,
      browser_profile_id: input.browserProfileId,
      owner_id: this.#ownerId,
    });
    const grant = parseGrant(payload, input.runId);
    const runPath = encodeURIComponent(input.runId);
    const operations: LeaseOperations = {
      heartbeat: async () => {
        const response = await this.request("heartbeat", `/api/jobs/internal/execution-leases/${runPath}/heartbeat`, {
          ...common,
          lease_token: grant.leaseToken,
          fence: grant.fence,
        });
        parseLeaseRecord("heartbeat", response, input.runId, grant.fence, ["prepared", "click_started"]);
      },
      irreversible: async () => {
        const response = await this.request("irreversible", `/api/jobs/internal/execution-leases/${runPath}/irreversible`, {
          ...common,
          lease_token: grant.leaseToken,
          fence: grant.fence,
          action: "submit",
        });
        parseLeaseRecord("irreversible", response, input.runId, grant.fence, ["click_started"]);
      },
      finish: async (outcome) => {
        await this.request("finish", `/api/jobs/internal/execution-leases/${runPath}/finish`, {
          ...common,
          lease_token: grant.leaseToken,
          fence: grant.fence,
          outcome,
        });
      },
    };
    return new ActiveExecutionLease(
      operations,
      this.#heartbeatIntervalMs,
      grant.fence,
      grant.expiresAtMs,
      this.#ownerId,
    );
  }

  private async request(
    operation: "claim" | "heartbeat" | "irreversible" | "finish",
    path: string,
    body: Record<string, unknown>,
  ): Promise<unknown> {
    const controller = new AbortController();
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, this.#requestTimeoutMs);
    timeout.unref?.();
    try {
      const serializedBody = JSON.stringify(body);
      const response = await this.#fetch(`${this.#origin}${path}`, {
        method: "POST",
        headers: {
          ...createJobsWorkerAuthHeaders({
            signingKey: this.#workerSigningKey,
            workerId: this.#ownerId,
            method: "POST",
            path,
            body: serializedBody,
          }),
          "Content-Type": "application/json",
        },
        body: serializedBody,
        redirect: "error",
        signal: controller.signal,
      });
      if (response.redirected || (response.status >= 300 && response.status < 400)) {
        await discardBounded(response, this.#maxResponseBytes, operation);
        throw new ExecutionLeaseError(operation, "redirect_blocked", response.status);
      }
      const responseText = await readBounded(response, this.#maxResponseBytes, operation);
      if (!response.ok) {
        const code = operation === "claim" && (response.status === 409 || response.status === 423)
          ? "lease_unavailable"
          : "request_failed";
        throw new ExecutionLeaseError(operation, code, response.status);
      }
      if (!responseText) return undefined;
      try {
        return JSON.parse(responseText) as unknown;
      } catch {
        throw new ExecutionLeaseError(operation, "invalid_response", response.status);
      }
    } catch (error) {
      if (error instanceof ExecutionLeaseError) {
        if (timedOut && error.code === "request_failed") {
          throw new ExecutionLeaseError(operation, "timed_out");
        }
        throw error;
      }
      throw new ExecutionLeaseError(operation, timedOut ? "timed_out" : "request_failed");
    } finally {
      clearTimeout(timeout);
    }
  }
}

export function createExecutionLeaseClientFromEnv(
  env: NodeJS.ProcessEnv = process.env,
): ExecutionLeaseClient {
  return new ExecutionLeaseClient({
    origin: env.BLUEY_JOBS_API_ORIGIN ?? "",
    workerSigningKey: env.BLUEY_JOBS_WORKER_SIGNING_KEY ?? "",
    ownerId: runnerOwnerId(env.BLUEY_JOBS_RUNNER_ID),
  });
}

export function runnerOwnerId(configured?: string): string {
  const value = configured?.trim();
  if (!value) return PROCESS_OWNER_ID;
  return boundedOwnerId(value);
}

function normalizedOrigin(rawOrigin: string): string {
  try {
    const url = new URL(rawOrigin);
    const loopback = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]).has(url.hostname);
    if (!/^https?:$/.test(url.protocol)
      || (url.protocol === "http:" && !loopback)
      || url.username
      || url.password
      || url.pathname !== "/"
      || url.search
      || url.hash) {
      throw new Error("invalid origin");
    }
    return url.origin;
  } catch {
    throw new ExecutionLeaseError("configuration", "configuration");
  }
}

function boundedSigningKey(value: string): string {
  const bytes = Buffer.byteLength(value, "utf8");
  if (bytes < 32 || bytes > MAX_WORKER_SIGNING_KEY_BYTES) {
    throw new ExecutionLeaseError("configuration", "configuration");
  }
  return value;
}

function boundedOwnerId(value: string): string {
  if (value
    && Buffer.byteLength(value) <= MAX_OWNER_BYTES
    && /^[A-Za-z0-9._:-]+$/.test(value)) {
    return value;
  }
  if (!value) throw new ExecutionLeaseError("configuration", "configuration");
  return `runner-${createHash("sha256").update(value).digest("hex").slice(0, 48)}`;
}

function boundedInteger(value: number | undefined, fallback: number, minimum: number, maximum: number): number {
  const candidate = value ?? fallback;
  if (!Number.isInteger(candidate) || candidate < minimum || candidate > maximum) {
    throw new ExecutionLeaseError("configuration", "configuration");
  }
  return candidate;
}

function parseGrant(value: unknown, expectedRunId: string): LeaseGrant {
  if (!value || typeof value !== "object") {
    throw new ExecutionLeaseError("claim", "invalid_response");
  }
  const record = value as Record<string, unknown>;
  const leaseToken = record.lease_token;
  const fence = record.fence;
  const expiresAtMs = record.lease_expires_at_ms;
  if (record.run_id !== expectedRunId
    || record.phase !== "prepared"
    || typeof leaseToken !== "string"
    || !leaseToken
    || Buffer.byteLength(leaseToken) > MAX_LEASE_TOKEN_BYTES
    || typeof fence !== "number"
    || !Number.isSafeInteger(fence)
    || fence <= 0
    || typeof expiresAtMs !== "number"
    || !Number.isSafeInteger(expiresAtMs)
    || expiresAtMs <= Date.now()) {
    throw new ExecutionLeaseError("claim", "invalid_response");
  }
  return { leaseToken, fence, expiresAtMs };
}

function parseLeaseRecord(
  operation: "heartbeat" | "irreversible",
  value: unknown,
  expectedRunId: string,
  expectedFence: number,
  expectedPhases: readonly string[],
): void {
  if (!value || typeof value !== "object") {
    throw new ExecutionLeaseError(operation, "invalid_response");
  }
  const record = value as Record<string, unknown>;
  if (record.run_id !== expectedRunId
    || record.fence !== expectedFence
    || typeof record.phase !== "string"
    || !expectedPhases.includes(record.phase)) {
    throw new ExecutionLeaseError(operation, "invalid_response");
  }
}

async function discardBounded(
  response: Response,
  maximumBytes: number,
  operation: "claim" | "heartbeat" | "irreversible" | "finish",
): Promise<void> {
  await readBounded(response, maximumBytes, operation);
}

async function readBounded(
  response: Response,
  maximumBytes: number,
  operation: "claim" | "heartbeat" | "irreversible" | "finish",
): Promise<string> {
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maximumBytes) {
    await response.body?.cancel().catch(() => undefined);
    throw new ExecutionLeaseError(operation, "response_too_large", response.status);
  }
  if (!response.body) return "";
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.byteLength;
      if (total > maximumBytes) {
        await reader.cancel().catch(() => undefined);
        throw new ExecutionLeaseError(operation, "response_too_large", response.status);
      }
      chunks.push(value);
    }
  } catch (error) {
    if (error instanceof ExecutionLeaseError) throw error;
    throw new ExecutionLeaseError(operation, "request_failed", response.status);
  }
  return Buffer.concat(chunks.map((chunk) => Buffer.from(chunk))).toString("utf8");
}

function asLeaseError(
  operation: "heartbeat",
  error: unknown,
): ExecutionLeaseError {
  return error instanceof ExecutionLeaseError
    ? error
    : new ExecutionLeaseError(operation, "request_failed");
}
