import { createHash } from "node:crypto";
import { createJobsWorkerAuthHeaders } from "@bluey/jobs-automation/worker-auth";
import { runnerOwnerId } from "./execution-lease.js";
import type { EncryptedProfileSnapshot } from "./profile-store.js";

const DEFAULT_REQUEST_TIMEOUT_MS = 15_000;
const MAX_ENCRYPTED_SNAPSHOT_BYTES = 25 * 1024 * 1024;
const MAX_RESPONSE_BYTES = 36 * 1024 * 1024;
const MAX_SIGNING_KEY_BYTES = 4_096;

export interface BrowserProfileSnapshotLeaseContext {
  accountId: string;
  applicationId: string;
  runId: string;
  browserProfileId: string;
  leaseToken: string;
  fence: number;
}

export interface StoredBrowserProfileSnapshot {
  generation: number;
  envelopeVersion: 2;
  sha256: string;
  sizeBytes: number;
}

export interface BrowserProfileSnapshotClientOptions {
  origin: string;
  workerSigningKey: string;
  ownerId: string;
  requestTimeoutMs?: number;
  fetch?: typeof globalThis.fetch;
}

export type BrowserProfileSnapshotErrorCode =
  | "configuration"
  | "invalid_response"
  | "redirect_blocked"
  | "request_failed"
  | "response_too_large"
  | "timed_out";

export class BrowserProfileSnapshotError extends Error {
  constructor(
    readonly operation: "restore" | "store" | "configuration",
    readonly code: BrowserProfileSnapshotErrorCode,
    readonly status?: number,
  ) {
    super(`Browser profile snapshot ${operation} failed (${code}).`);
    this.name = "BrowserProfileSnapshotError";
  }
}

export class BrowserProfileSnapshotClient {
  readonly #origin: string;
  readonly #workerSigningKey: string;
  readonly #ownerId: string;
  readonly #requestTimeoutMs: number;
  readonly #fetch: typeof globalThis.fetch;

  constructor(options: BrowserProfileSnapshotClientOptions) {
    this.#origin = normalizedOrigin(options.origin);
    this.#workerSigningKey = boundedSigningKey(options.workerSigningKey);
    this.#ownerId = boundedOwnerId(options.ownerId);
    this.#requestTimeoutMs = boundedInteger(
      options.requestTimeoutMs,
      DEFAULT_REQUEST_TIMEOUT_MS,
      100,
      60_000,
    );
    this.#fetch = options.fetch ?? globalThis.fetch;
    if (typeof this.#fetch !== "function") {
      throw new BrowserProfileSnapshotError("configuration", "configuration");
    }
  }

  async restore(
    context: BrowserProfileSnapshotLeaseContext,
  ): Promise<EncryptedProfileSnapshot | undefined> {
    const response = await this.request(
      "restore",
      profilePath(context.runId, "restore"),
      accessBody(context),
      true,
    );
    if (response === undefined) return undefined;
    const record = objectRecord(response, "restore");
    assertIdentity(record, context, "restore");
    const metadata = parseMetadata(record, "restore");
    const encoded = record.encrypted_snapshot_base64;
    if (typeof encoded !== "string" || !isCanonicalBase64(encoded)) {
      throw invalidResponse("restore");
    }
    const bytes = Buffer.from(encoded, "base64");
    if (bytes.length !== metadata.sizeBytes
      || sha256(bytes) !== metadata.sha256
      || bytes.length < 8
      || bytes.subarray(0, 8).toString("ascii") !== "BLUEYJP2") {
      throw invalidResponse("restore");
    }
    return {
      bytes,
      generation: metadata.generation,
      envelopeVersion: metadata.envelopeVersion,
    };
  }

  async store(
    context: BrowserProfileSnapshotLeaseContext,
    snapshot: EncryptedProfileSnapshot,
  ): Promise<StoredBrowserProfileSnapshot> {
    assertSnapshot(snapshot);
    const digest = sha256(snapshot.bytes);
    const response = await this.request("store", profilePath(context.runId, "store"), {
      ...accessBody(context),
      expected_generation: snapshot.generation,
      envelope_version: snapshot.envelopeVersion,
      sha256: digest,
      size_bytes: snapshot.bytes.length,
      encrypted_snapshot_base64: snapshot.bytes.toString("base64"),
    });
    const record = objectRecord(response, "store");
    assertIdentity(record, context, "store");
    const metadata = parseMetadata(record, "store");
    if (metadata.generation !== snapshot.generation + 1
      || metadata.sha256 !== digest
      || metadata.sizeBytes !== snapshot.bytes.length
      || metadata.envelopeVersion !== snapshot.envelopeVersion) {
      throw invalidResponse("store");
    }
    return metadata;
  }

  private async request(
    operation: "restore" | "store",
    path: string,
    body: Record<string, unknown>,
    allowNoContent = false,
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
        await discardBounded(response, operation);
        throw new BrowserProfileSnapshotError(operation, "redirect_blocked", response.status);
      }
      const responseText = await readBounded(response, operation);
      if (allowNoContent && response.status === 204) return undefined;
      if (!response.ok || !responseText) {
        throw new BrowserProfileSnapshotError(operation, "request_failed", response.status);
      }
      try {
        return JSON.parse(responseText) as unknown;
      } catch {
        throw invalidResponse(operation, response.status);
      }
    } catch (error) {
      if (error instanceof BrowserProfileSnapshotError) {
        if (timedOut && error.code === "request_failed") {
          throw new BrowserProfileSnapshotError(operation, "timed_out");
        }
        throw error;
      }
      throw new BrowserProfileSnapshotError(operation, timedOut ? "timed_out" : "request_failed");
    } finally {
      clearTimeout(timeout);
    }
  }
}

export function createBrowserProfileSnapshotClientFromEnv(
  env: NodeJS.ProcessEnv = process.env,
): BrowserProfileSnapshotClient {
  return new BrowserProfileSnapshotClient({
    origin: env.BLUEY_JOBS_API_ORIGIN ?? "",
    workerSigningKey: env.BLUEY_JOBS_WORKER_SIGNING_KEY ?? "",
    ownerId: runnerOwnerId(env.BLUEY_JOBS_RUNNER_ID),
  });
}

function profilePath(runId: string, operation: "restore" | "store"): string {
  assertIdentifier(runId, 3, 160);
  return `/api/jobs/internal/execution-leases/${encodeURIComponent(runId)}/profile/${operation}`;
}

function accessBody(context: BrowserProfileSnapshotLeaseContext): Record<string, unknown> {
  assertIdentifier(context.accountId, 3, 160);
  assertIdentifier(context.applicationId, 3, 160);
  assertIdentifier(context.browserProfileId, 3, 160, true);
  if (!context.leaseToken || Buffer.byteLength(context.leaseToken, "utf8") > 256) {
    throw new BrowserProfileSnapshotError("configuration", "configuration");
  }
  if (!Number.isSafeInteger(context.fence) || context.fence <= 0) {
    throw new BrowserProfileSnapshotError("configuration", "configuration");
  }
  return {
    account_id: context.accountId,
    application_id: context.applicationId,
    browser_profile_id: context.browserProfileId,
    lease_token: context.leaseToken,
    fence: context.fence,
  };
}

function parseMetadata(
  record: Record<string, unknown>,
  operation: "restore" | "store",
): StoredBrowserProfileSnapshot {
  const generation = record.generation;
  const envelopeVersion = record.envelope_version;
  const digest = record.sha256;
  const sizeBytes = record.size_bytes;
  if (typeof generation !== "number"
    || !Number.isSafeInteger(generation)
    || generation <= 0
    || envelopeVersion !== 2
    || typeof digest !== "string"
    || !/^[a-f0-9]{64}$/.test(digest)
    || typeof sizeBytes !== "number"
    || !Number.isSafeInteger(sizeBytes)
    || sizeBytes <= 0
    || sizeBytes > MAX_ENCRYPTED_SNAPSHOT_BYTES) {
    throw invalidResponse(operation);
  }
  return { generation, envelopeVersion, sha256: digest, sizeBytes };
}

function assertIdentity(
  record: Record<string, unknown>,
  context: BrowserProfileSnapshotLeaseContext,
  operation: "restore" | "store",
): void {
  if (record.browser_profile_id !== context.browserProfileId) {
    throw invalidResponse(operation);
  }
}

function assertSnapshot(snapshot: EncryptedProfileSnapshot): void {
  if (!Number.isSafeInteger(snapshot.generation) || snapshot.generation < 0
    || snapshot.envelopeVersion !== 2
    || snapshot.bytes.length <= 0
    || snapshot.bytes.length > MAX_ENCRYPTED_SNAPSHOT_BYTES
    || snapshot.bytes.length < 8
    || snapshot.bytes.subarray(0, 8).toString("ascii") !== "BLUEYJP2") {
    throw new BrowserProfileSnapshotError("store", "configuration");
  }
}

function objectRecord(
  value: unknown,
  operation: "restore" | "store",
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw invalidResponse(operation);
  }
  return value as Record<string, unknown>;
}

function isCanonicalBase64(value: string): boolean {
  if (!value || value.length > Math.ceil(MAX_ENCRYPTED_SNAPSHOT_BYTES / 3) * 4 + 4) return false;
  if (value.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(value)) return false;
  const decoded = Buffer.from(value, "base64");
  return decoded.toString("base64") === value;
}

function sha256(value: Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}

async function discardBounded(
  response: Response,
  operation: "restore" | "store",
): Promise<void> {
  await readBounded(response, operation);
}

async function readBounded(
  response: Response,
  operation: "restore" | "store",
): Promise<string> {
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > MAX_RESPONSE_BYTES) {
    await response.body?.cancel().catch(() => undefined);
    throw new BrowserProfileSnapshotError(operation, "response_too_large", response.status);
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
      if (total > MAX_RESPONSE_BYTES) {
        await reader.cancel().catch(() => undefined);
        throw new BrowserProfileSnapshotError(operation, "response_too_large", response.status);
      }
      chunks.push(value);
    }
  } catch (error) {
    if (error instanceof BrowserProfileSnapshotError) throw error;
    throw new BrowserProfileSnapshotError(operation, "request_failed", response.status);
  }
  return Buffer.concat(chunks.map((chunk) => Buffer.from(chunk))).toString("utf8");
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
    throw new BrowserProfileSnapshotError("configuration", "configuration");
  }
}

function boundedSigningKey(value: string): string {
  const bytes = Buffer.byteLength(value, "utf8");
  if (bytes < 32 || bytes > MAX_SIGNING_KEY_BYTES) {
    throw new BrowserProfileSnapshotError("configuration", "configuration");
  }
  return value;
}

function boundedOwnerId(value: string): string {
  if (!value
    || Buffer.byteLength(value, "utf8") > 128
    || !/^[A-Za-z0-9._:-]+$/.test(value)) {
    throw new BrowserProfileSnapshotError("configuration", "configuration");
  }
  return value;
}

function boundedInteger(value: number | undefined, fallback: number, minimum: number, maximum: number): number {
  const candidate = value ?? fallback;
  if (!Number.isInteger(candidate) || candidate < minimum || candidate > maximum) {
    throw new BrowserProfileSnapshotError("configuration", "configuration");
  }
  return candidate;
}

function assertIdentifier(value: string, minimum: number, maximum: number, allowColon = false): void {
  const expression = allowColon ? /^[A-Za-z0-9:_-]+$/ : /^[A-Za-z0-9_-]+$/;
  const bytes = Buffer.byteLength(value, "utf8");
  if (bytes < minimum || bytes > maximum || !expression.test(value)) {
    throw new BrowserProfileSnapshotError("configuration", "configuration");
  }
}

function invalidResponse(
  operation: "restore" | "store",
  status?: number,
): BrowserProfileSnapshotError {
  return new BrowserProfileSnapshotError(operation, "invalid_response", status);
}
