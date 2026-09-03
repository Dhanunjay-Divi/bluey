import { randomUUID } from "node:crypto";
import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createJobsWorkerAuthHeaders } from "./worker-auth.js";
import {
  assertManagedCloudRuntimeMeasurement,
  managedCloudRuntimeClaimRequest,
  type ManagedCloudRuntimeConfig,
  type ManagedCloudRuntimeRole,
  type ManagedCloudScope,
} from "./managed-cloud-runtime.js";

const MAX_RESPONSE_BYTES = 64 * 1024;
const DEFAULT_TIMEOUT_MS = 5_000;
const ROUTE_ID = /^[A-Za-z0-9_-]{20,128}$/;
const TOKEN_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const SHA256 = /^[a-f0-9]{64}$/;
const ORIGINAL_SOURCE_VERIFIER_STARTUP_AUDIENCE =
  "bluey-jobs-managed-cloud-original-source-verifier-startup-v1";

export function managedCloudTaskQueueSha256(
  namespace: string,
  taskQueue: string,
): string {
  for (const [value, label] of [
    [namespace, "namespace"],
    [taskQueue, "task queue"],
  ] as const) {
    if (
      value.length === 0 ||
      Buffer.byteLength(value, "utf8") > 240 ||
      Buffer.from(value, "utf8").toString("utf8") !== value ||
      value.includes("\ufffd") ||
      value !== value.trim() ||
      /\p{Cc}/u.test(value)
    ) {
      throw new Error(`Managed-cloud ${label} is invalid`);
    }
  }
  return createHash("sha256")
    .update("bluey-jobs-managed-cloud-task-queue-v1\0")
    .update(namespace, "utf8")
    .update("\0")
    .update(taskQueue, "utf8")
    .digest("hex");
}

export function managedCloudFailureConverterSha256(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

export async function managedCloudReadyObservation(
  instance: ManagedCloudRuntimeInstance,
  temporalNamespace: string,
  taskQueue: string,
  failureConverterPath: string | URL,
): Promise<ManagedCloudRuntimeObservation> {
  return {
    taskQueueSha256: managedCloudTaskQueueSha256(temporalNamespace, taskQueue),
    failureConverterSha256: managedCloudFailureConverterSha256(
      await readFile(failureConverterPath),
    ),
    dependencyEvidenceSha256: managedCloudDependencyEvidenceSha256(instance),
    healthState: "ready",
    reasonCode: null,
  };
}

export function managedCloudDependencyEvidenceSha256(
  instance: ManagedCloudRuntimeInstance,
): string {
  const hash = createHash("sha256").update(
    "bluey-jobs-managed-cloud-dependency-evidence-v1\0",
  );
  for (const value of [
    instance.role,
    instance.activationSha256,
    instance.manifestSha256,
    instance.componentId,
    instance.artifactSha256,
    instance.taskQueueSha256,
    instance.failureConverterSha256,
  ]) {
    hash.update(value, "utf8").update("\0");
  }
  return hash.digest("hex");
}

export type ManagedCloudHealth =
  | { healthState: "ready"; reasonCode: null }
  | { healthState: "draining"; reasonCode: "draining" }
  | {
      healthState: "degraded";
      reasonCode:
        | "artifact_mismatch"
        | "config_mismatch"
        | "dependency_unavailable"
        | "head_mismatch"
        | "migration_mismatch"
        | "probe_failed"
        | "protocol_mismatch"
        | "startup";
    };

export interface ManagedCloudRuntimeInstance {
  grantId: string;
  runtimeInstanceId: string;
  runtimeIdentitySha256: string;
  workerId: string;
  scope: ManagedCloudScope;
  activationSha256: string;
  activationExpiresAtMs: number;
  manifestSha256: string;
  componentId: string;
  role: ManagedCloudRuntimeRole;
  headRevision: number;
  transitionSha256: string;
  artifactSha256: string;
  configSchemaSha256: string;
  migrationSetSha256: string;
  protocolSetSha256: string;
  taskQueueSha256: string;
  failureConverterSha256: string;
  dependencyEvidenceSha256: string;
  instanceEpoch: number;
  nextHeartbeatSequence: number;
  claimedAtMs: number;
  replayed: boolean;
}

export type ManagedCloudRuntimeObservation = ManagedCloudHealth & {
  dependencyEvidenceSha256: string;
  taskQueueSha256: string;
  failureConverterSha256: string;
};

export interface ManagedCloudRuntimeHeartbeat {
  runtimeInstanceId: string;
  workerId: string;
  instanceEpoch: number;
  heartbeatSequence: number;
  observedHeadRevision: number;
  observedTransitionSha256: string;
  activationSha256: string;
  manifestSha256: string;
  componentId: string;
  role: ManagedCloudRuntimeRole;
  artifactSha256: string;
  migrationSetSha256: string;
  configSchemaSha256: string;
  protocolSetSha256: string;
  taskQueueSha256: string;
  failureConverterSha256: string;
  dependencyEvidenceSha256: string;
  healthState: ManagedCloudHealth["healthState"];
  reasonCode: ManagedCloudHealth["reasonCode"];
  heartbeatAtMs: number;
  replayed: boolean;
}

export interface ManagedCloudOriginalSourceVerifierStartupRequest {
  version: 1;
  audience: typeof ORIGINAL_SOURCE_VERIFIER_STARTUP_AUDIENCE;
  startupRequestId: string;
  runtimeInstanceId: string;
  workerId: string;
  sessionToken: string;
  instanceEpoch: number;
  expectedDegradedHeartbeatSequence: number;
  runtimeIdentitySha256: string;
  observedHeadRevision: number;
  observedTransitionSha256: string;
  activationSha256: string;
  manifestSha256: string;
  componentId: string;
  role: "original_source_verifier";
  artifactSha256: string;
  migrationSetSha256: string;
  configSchemaSha256: string;
  protocolSetSha256: string;
  taskQueueSha256: string;
  failureConverterSha256: string;
  dependencyEvidenceSha256: string;
}

export type ManagedCloudOriginalSourceVerifierProbeResult =
  | "idle"
  | "leaseable";

export type ManagedCloudOriginalSourceVerifierStartup = Omit<
  ManagedCloudOriginalSourceVerifierStartupRequest,
  "expectedDegradedHeartbeatSequence" | "sessionToken"
> & {
  degradedHeartbeatSequence: number;
  readyHeartbeatSequence: number;
  probeResult: ManagedCloudOriginalSourceVerifierProbeResult;
  startupAuthoritySha256: string;
  startupAtMs: number;
};

export type ManagedCloudRuntimeFetch = (
  input: string | URL | Request,
  init?: RequestInit,
) => Promise<Response>;

export interface ManagedCloudRuntimeApi {
  claim(): Promise<ManagedCloudRuntimeInstance>;
  heartbeat(
    instance: ManagedCloudRuntimeInstance,
    heartbeatSequence: number,
    observation: ManagedCloudRuntimeObservation,
  ): Promise<ManagedCloudRuntimeHeartbeat>;
}

export interface ManagedCloudOriginalSourceVerifierStartupApi
  extends ManagedCloudRuntimeApi {
  originalSourceVerifierStartup(
    instance: ManagedCloudRuntimeInstance,
    expectedDegradedHeartbeatSequence: number,
  ): Promise<ManagedCloudOriginalSourceVerifierStartup>;
}


// ---- Source-verifier boot generation client (Phase 628 idempotent protocol) ----

/**
 * Stable per-process boot identity. Computed once at module load so every retry
 * inside this process reuses the same identity, and a genuine restart produces a
 * new one. The server assigns the generation; the client never proposes it.
 */
const SOURCE_VERIFIER_BOOT_ID = `boot-${randomUUID().replace(/-/g, "")}`;

/**
 * Stable per-process idempotency tokens. Reusing these on retry is what makes
 * the calls naturally idempotent: identical payload replays the stored canonical
 * response, a changed payload is an explicit conflict.
 */
const SOURCE_VERIFIER_BOOT_REGISTRATION_REQUEST_ID = `regboot-${randomUUID().replace(/-/g, "")}`;
const SOURCE_VERIFIER_BOOT_STARTUP_REQUEST_ID = `startboot-${randomUUID().replace(/-/g, "")}`;

export function sourceVerifierBootId(): string {
  return SOURCE_VERIFIER_BOOT_ID;
}

export function sourceVerifierBootRegistrationRequestId(): string {
  return SOURCE_VERIFIER_BOOT_REGISTRATION_REQUEST_ID;
}

export function sourceVerifierBootStartupRequestId(): string {
  return SOURCE_VERIFIER_BOOT_STARTUP_REQUEST_ID;
}

export interface SourceVerifierBootRegistrationResult {
  bootId: string;
  bootGeneration: number;
  registeredAtMs: number;
}

export interface SourceVerifierBootStartupResult {
  bootGeneration: number;
  degradedHeartbeatSequence: number;
  readyHeartbeatSequence: number;
  probeResult: string;
  startupAtMs: number;
}

function requireSourceVerifierIdentity(
  instance: ManagedCloudRuntimeInstance,
): void {
  if (
    instance.role !== "original_source_verifier" ||
    instance.componentId !== "jobs-workflows"
  ) {
    throw new Error("Managed-cloud original-source verifier identity is invalid");
  }
}

function parseSourceVerifierBootRegistration(
  value: unknown,
): SourceVerifierBootRegistrationResult {
  if (typeof value !== "object" || value === null) {
    throw new Error("Source-verifier boot registration response is invalid");
  }
  const record = value as Record<string, unknown>;
  const bootId = record.bootId;
  const bootGeneration = Number(record.bootGeneration);
  const registeredAtMs = Number(record.registeredAtMs);
  if (typeof bootId !== "string" || !bootId.trim()) {
    throw new Error("Source-verifier boot registration is missing a boot id");
  }
  if (!Number.isSafeInteger(bootGeneration) || bootGeneration < 1) {
    throw new Error("Source-verifier boot generation is invalid");
  }
  if (!Number.isSafeInteger(registeredAtMs) || registeredAtMs < 0) {
    throw new Error("Source-verifier boot registration time is invalid");
  }
  return { bootId, bootGeneration, registeredAtMs };
}

function parseSourceVerifierBootStartup(
  value: unknown,
): SourceVerifierBootStartupResult {
  if (typeof value !== "object" || value === null) {
    throw new Error("Source-verifier boot startup response is invalid");
  }
  const record = value as Record<string, unknown>;
  const bootGeneration = Number(record.bootGeneration);
  const degradedHeartbeatSequence = Number(record.degradedHeartbeatSequence);
  const readyHeartbeatSequence = Number(record.readyHeartbeatSequence);
  const probeResult = record.probeResult;
  const startupAtMs = Number(record.startupAtMs);
  if (!Number.isSafeInteger(bootGeneration) || bootGeneration < 1) {
    throw new Error("Source-verifier boot startup generation is invalid");
  }
  if (
    !Number.isSafeInteger(degradedHeartbeatSequence) ||
    degradedHeartbeatSequence < 1 ||
    readyHeartbeatSequence !== degradedHeartbeatSequence + 1
  ) {
    throw new Error("Source-verifier boot startup sequence is inconsistent");
  }
  if (probeResult !== "idle" && probeResult !== "leaseable") {
    throw new Error("Source-verifier boot startup probe result is invalid");
  }
  if (!Number.isSafeInteger(startupAtMs) || startupAtMs < 0) {
    throw new Error("Source-verifier boot startup time is invalid");
  }
  return {
    bootGeneration,
    degradedHeartbeatSequence,
    readyHeartbeatSequence,
    probeResult,
    startupAtMs,
  };
}

export class ManagedCloudRuntimeApiClient {
  private readonly config: ManagedCloudRuntimeConfig;
  private readonly fetcher: ManagedCloudRuntimeFetch;
  private readonly timeoutMs: number;

  constructor(
    config: ManagedCloudRuntimeConfig,
    options: { fetch?: ManagedCloudRuntimeFetch; timeoutMs?: number } = {},
  ) {
    this.config = config;
    this.fetcher = options.fetch ?? fetch;
    this.timeoutMs = boundedInteger(
      options.timeoutMs ?? DEFAULT_TIMEOUT_MS,
      100,
      10_000,
    );
  }

  async claim(): Promise<ManagedCloudRuntimeInstance> {
    assertManagedCloudRuntimeMeasurement(this.config);
    const path = `/api/jobs/internal/managed-cloud/runtime-grants/${this.config.grantId}/claim`;
    const value = await this.request(
      path,
      managedCloudRuntimeClaimRequest(this.config),
    );
    return parseInstance(value, this.config);
  }

  async heartbeat(
    instance: ManagedCloudRuntimeInstance,
    heartbeatSequence: number,
    observation: ManagedCloudRuntimeObservation,
  ): Promise<ManagedCloudRuntimeHeartbeat> {
    if (heartbeatSequence === instance.nextHeartbeatSequence) {
      assertManagedCloudRuntimeMeasurement(this.config);
    }
    const sequence = positiveInteger(heartbeatSequence, "heartbeat sequence");
    const taskQueueSha256 = requiredSha256(
      observation.taskQueueSha256,
      "task queue",
    );
    const failureConverterSha256 = requiredSha256(
      observation.failureConverterSha256,
      "failure converter",
    );
    const dependencyEvidenceSha256 = requiredSha256(
      observation.dependencyEvidenceSha256,
      "dependency evidence",
    );
    if (
      taskQueueSha256 !== instance.taskQueueSha256 ||
      failureConverterSha256 !== instance.failureConverterSha256 ||
      dependencyEvidenceSha256 !== instance.dependencyEvidenceSha256
    ) {
      throw new Error(
        "Managed-cloud runtime dependency evidence is inconsistent",
      );
    }
    const input = {
      runtimeInstanceId: instance.runtimeInstanceId,
      workerId: instance.workerId,
      sessionToken: this.config.sessionToken,
      heartbeatSequence: sequence,
      observedHeadRevision: instance.headRevision,
      observedTransitionSha256: instance.transitionSha256,
      activationSha256: instance.activationSha256,
      manifestSha256: instance.manifestSha256,
      componentId: instance.componentId,
      role: instance.role,
      artifactSha256: instance.artifactSha256,
      migrationSetSha256: instance.migrationSetSha256,
      configSchemaSha256: instance.configSchemaSha256,
      protocolSetSha256: instance.protocolSetSha256,
      taskQueueSha256,
      failureConverterSha256,
      dependencyEvidenceSha256,
      healthState: observation.healthState,
      reasonCode: observation.reasonCode,
    };
    const path = `/api/jobs/internal/managed-cloud/runtime-instances/${instance.runtimeInstanceId}/heartbeats`;
    const value = await this.request(path, input);
    return parseHeartbeat(value, input, instance);
  }

  async originalSourceVerifierStartup(
    instance: ManagedCloudRuntimeInstance,
    expectedDegradedHeartbeatSequence: number,
  ): Promise<ManagedCloudOriginalSourceVerifierStartup> {
    if (
      instance.role !== "original_source_verifier" ||
      instance.componentId !== "jobs-workflows"
    ) {
      throw new Error(
        "Managed-cloud original-source verifier identity is invalid",
      );
    }
    assertManagedCloudRuntimeMeasurement(this.config);
    const degradedSequence = positiveInteger(
      expectedDegradedHeartbeatSequence,
      "degraded heartbeat sequence",
    );
    const input: ManagedCloudOriginalSourceVerifierStartupRequest = {
      version: 1,
      audience: ORIGINAL_SOURCE_VERIFIER_STARTUP_AUDIENCE,
      startupRequestId: managedCloudOriginalSourceVerifierStartupRequestId(
        instance,
        degradedSequence,
      ),
      runtimeInstanceId: instance.runtimeInstanceId,
      workerId: instance.workerId,
      sessionToken: this.config.sessionToken,
      instanceEpoch: instance.instanceEpoch,
      expectedDegradedHeartbeatSequence: degradedSequence,
      runtimeIdentitySha256: instance.runtimeIdentitySha256,
      observedHeadRevision: instance.headRevision,
      observedTransitionSha256: instance.transitionSha256,
      activationSha256: instance.activationSha256,
      manifestSha256: instance.manifestSha256,
      componentId: instance.componentId,
      role: instance.role,
      artifactSha256: instance.artifactSha256,
      migrationSetSha256: instance.migrationSetSha256,
      configSchemaSha256: instance.configSchemaSha256,
      protocolSetSha256: instance.protocolSetSha256,
      taskQueueSha256: instance.taskQueueSha256,
      failureConverterSha256: instance.failureConverterSha256,
      dependencyEvidenceSha256: instance.dependencyEvidenceSha256,
    };
    const path =
      `/api/jobs/internal/managed-cloud/runtime-instances/${instance.runtimeInstanceId}` +
      "/original-source-verifier-startup";
    const value = await this.request(path, input);
    return parseOriginalSourceVerifierStartup(value, input);
  }

  async registerSourceVerifierBoot(
    instance: ManagedCloudRuntimeInstance,
  ): Promise<SourceVerifierBootRegistrationResult> {
    requireSourceVerifierIdentity(instance);
    assertManagedCloudRuntimeMeasurement(this.config);
    const path = `/api/jobs/internal/managed-cloud/runtime-instances/${instance.runtimeInstanceId}/source-verifier-boot-registration`;
    const value = await this.request(path, {
      registrationRequestId: sourceVerifierBootRegistrationRequestId(),
      bootId: sourceVerifierBootId(),
      runtimeInstanceId: instance.runtimeInstanceId,
      instanceEpoch: instance.instanceEpoch,
      workerId: instance.workerId,
      runtimeIdentitySha256: instance.runtimeIdentitySha256,
      activationSha256: instance.activationSha256,
      manifestSha256: instance.manifestSha256,
      componentId: instance.componentId,
      role: instance.role,
    });
    return parseSourceVerifierBootRegistration(value);
  }

  async sourceVerifierBootStartup(
    instance: ManagedCloudRuntimeInstance,
    bootGeneration: number,
    expectedDegradedHeartbeatSequence: number,
    probeResult: string,
  ): Promise<SourceVerifierBootStartupResult> {
    requireSourceVerifierIdentity(instance);
    assertManagedCloudRuntimeMeasurement(this.config);
    const degradedSequence = positiveInteger(
      expectedDegradedHeartbeatSequence,
      "degraded heartbeat sequence",
    );
    const path = `/api/jobs/internal/managed-cloud/runtime-instances/${instance.runtimeInstanceId}/source-verifier-boot-startup`;
    const value = await this.request(path, {
      startupRequestId: sourceVerifierBootStartupRequestId(),
      bootId: sourceVerifierBootId(),
      runtimeInstanceId: instance.runtimeInstanceId,
      instanceEpoch: instance.instanceEpoch,
      bootGeneration,
      workerId: instance.workerId,
      degradedHeartbeatSequence: degradedSequence,
      probeResult,
    });
    return parseSourceVerifierBootStartup(value);
  }

  private async request(path: string, input: unknown): Promise<unknown> {
    const body = JSON.stringify(input);
    const headers = createJobsWorkerAuthHeaders({
      signingKey: this.config.signingKey,
      workerId: this.config.workerId,
      method: "POST",
      path,
      body,
      origin: this.config.apiOrigin,
    });
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    let response: Response;
    try {
      response = await this.fetcher(`${this.config.apiOrigin}${path}`, {
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
      throw new Error(
        controller.signal.aborted
          ? "Managed-cloud runtime API timed out"
          : "Managed-cloud runtime API unavailable",
      );
    } finally {
      clearTimeout(timer);
    }
    if (
      response.status !== 200 ||
      response.headers.get("content-type") !== "application/json" ||
      response.headers.get("cache-control") !== "no-store" ||
      response.headers.get("x-content-type-options") !== "nosniff"
    ) {
      throw new Error(
        "Managed-cloud runtime API response is not authoritative",
      );
    }
    const text = await response.text();
    if (Buffer.byteLength(text, "utf8") > MAX_RESPONSE_BYTES) {
      throw new Error("Managed-cloud runtime API response is too large");
    }
    try {
      return JSON.parse(text) as unknown;
    } catch {
      throw new Error("Managed-cloud runtime API response is invalid");
    }
  }
}

export function managedCloudOriginalSourceVerifierStartupRequestId(
  instance: ManagedCloudRuntimeInstance,
  expectedDegradedHeartbeatSequence: number,
): string {
  if (
    instance.role !== "original_source_verifier" ||
    instance.componentId !== "jobs-workflows"
  ) {
    throw new Error(
      "Managed-cloud original-source verifier identity is invalid",
    );
  }
  const sequence = positiveInteger(
    expectedDegradedHeartbeatSequence,
    "degraded heartbeat sequence",
  );
  const hash = createHash("sha256").update(
    "bluey-jobs-managed-cloud-original-source-verifier-startup-request-v1\0",
  );
  for (const value of [
    instance.runtimeInstanceId,
    instance.workerId,
    instance.runtimeIdentitySha256,
    String(instance.instanceEpoch),
    String(sequence),
    String(instance.headRevision),
    instance.transitionSha256,
    instance.activationSha256,
    instance.manifestSha256,
    instance.componentId,
    instance.role,
    instance.artifactSha256,
    instance.migrationSetSha256,
    instance.configSchemaSha256,
    instance.protocolSetSha256,
    instance.taskQueueSha256,
    instance.failureConverterSha256,
    instance.dependencyEvidenceSha256,
  ]) {
    hash.update(value, "utf8").update("\0");
  }
  return `osv-startup-${hash.digest("hex")}`;
}

export async function runManagedCloudRuntimeReporter(
  api: ManagedCloudRuntimeApi,
  observation: (
    instance: ManagedCloudRuntimeInstance,
  ) => Promise<ManagedCloudRuntimeObservation>,
  heartbeatIntervalMs: number,
  signal: AbortSignal,
  options: {
    attempts?: number;
    retryDelayMs?: number;
    sleep?: (milliseconds: number, signal: AbortSignal) => Promise<void>;
    onReady?: (instance: ManagedCloudRuntimeInstance) => void;
    onReadinessChanged?: (
      ready: boolean,
      instance: ManagedCloudRuntimeInstance,
    ) => void;
  } = {},
): Promise<void> {
  const attempts = boundedInteger(options.attempts ?? 3, 1, 5);
  const retryDelayMs = boundedInteger(options.retryDelayMs ?? 250, 0, 5_000);
  const sleep = options.sleep ?? abortableSleep;
  const instance = await claimManagedCloudRuntimeInstance(api, signal, {
    attempts,
    retryDelayMs,
    sleep,
  });
  await runManagedCloudRuntimeHeartbeats(
    api,
    instance,
    observation,
    heartbeatIntervalMs,
    signal,
    { ...options, attempts, retryDelayMs, sleep },
  );
}

export async function claimManagedCloudRuntimeInstance(
  api: ManagedCloudRuntimeApi,
  signal: AbortSignal,
  options: {
    attempts?: number;
    retryDelayMs?: number;
    sleep?: (milliseconds: number, signal: AbortSignal) => Promise<void>;
  } = {},
): Promise<ManagedCloudRuntimeInstance> {
  const attempts = boundedInteger(options.attempts ?? 3, 1, 5);
  const retryDelayMs = boundedInteger(options.retryDelayMs ?? 250, 0, 5_000);
  const sleep = options.sleep ?? abortableSleep;
  return retryExact(() => api.claim(), attempts, retryDelayMs, sleep, signal);
}

export async function runManagedCloudRuntimeHeartbeats(
  api: ManagedCloudRuntimeApi,
  instance: ManagedCloudRuntimeInstance,
  observation: (
    instance: ManagedCloudRuntimeInstance,
  ) => Promise<ManagedCloudRuntimeObservation>,
  heartbeatIntervalMs: number,
  signal: AbortSignal,
  options: {
    attempts?: number;
    retryDelayMs?: number;
    sleep?: (milliseconds: number, signal: AbortSignal) => Promise<void>;
    onReady?: (instance: ManagedCloudRuntimeInstance) => void;
    onReadinessChanged?: (
      ready: boolean,
      instance: ManagedCloudRuntimeInstance,
    ) => void;
  } = {},
): Promise<void> {
  const attempts = boundedInteger(options.attempts ?? 3, 1, 5);
  const retryDelayMs = boundedInteger(options.retryDelayMs ?? 250, 0, 5_000);
  const sleep = options.sleep ?? abortableSleep;
  let sequence = instance.nextHeartbeatSequence;
  let announced = false;
  while (!signal.aborted) {
    let heartbeat: ManagedCloudRuntimeHeartbeat;
    try {
      const exactObservation = await observation(instance);
      heartbeat = await retryExact(
        () => api.heartbeat(instance, sequence, exactObservation),
        attempts,
        retryDelayMs,
        sleep,
        signal,
      );
    } catch (error) {
      options.onReadinessChanged?.(false, instance);
      throw error;
    }
    const ready = heartbeat.healthState === "ready" && !heartbeat.replayed;
    options.onReadinessChanged?.(ready, instance);
    if (ready && !announced) {
      options.onReady?.(instance);
      announced = true;
    }
    sequence += 1;
    await sleep(heartbeatIntervalMs, signal);
  }
}

async function retryExact<T>(
  operation: () => Promise<T>,
  attempts: number,
  retryDelayMs: number,
  sleep: (milliseconds: number, signal: AbortSignal) => Promise<void>,
  signal: AbortSignal,
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= attempts && !signal.aborted; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      if (attempt < attempts) await sleep(retryDelayMs, signal);
    }
  }
  if (signal.aborted) throw new Error("Managed-cloud runtime reporter stopped");
  throw lastError;
}

function abortableSleep(
  milliseconds: number,
  signal: AbortSignal,
): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve();
    const timer = setTimeout(done, milliseconds);
    signal.addEventListener("abort", done, { once: true });
    function done(): void {
      clearTimeout(timer);
      signal.removeEventListener("abort", done);
      resolve();
    }
  });
}

function parseInstance(
  value: unknown,
  config: ManagedCloudRuntimeConfig,
): ManagedCloudRuntimeInstance {
  const row = exactRecord(value, [
    "activationExpiresAtMs",
    "activationSha256",
    "artifactSha256",
    "claimedAtMs",
    "componentId",
    "configSchemaSha256",
    "failureConverterSha256",
    "grantId",
    "headRevision",
    "dependencyEvidenceSha256",
    "instanceEpoch",
    "manifestSha256",
    "migrationSetSha256",
    "nextHeartbeatSequence",
    "protocolSetSha256",
    "replayed",
    "role",
    "runtimeIdentitySha256",
    "runtimeInstanceId",
    "scope",
    "taskQueueSha256",
    "transitionSha256",
    "workerId",
  ]);
  const scope = parseScope(row.scope);
  const role = runtimeRole(row.role);
  const instance: ManagedCloudRuntimeInstance = {
    grantId: routeId(row.grantId, "grant"),
    runtimeInstanceId: routeId(row.runtimeInstanceId, "runtime instance"),
    runtimeIdentitySha256: requiredSha256(
      row.runtimeIdentitySha256,
      "runtime identity",
    ),
    workerId: workerId(row.workerId),
    scope,
    activationSha256: requiredSha256(row.activationSha256, "activation"),
    activationExpiresAtMs: positiveInteger(
      row.activationExpiresAtMs,
      "activation expiry",
    ),
    manifestSha256: requiredSha256(row.manifestSha256, "manifest"),
    componentId: tokenId(row.componentId, "component"),
    role,
    headRevision: positiveInteger(row.headRevision, "head revision"),
    transitionSha256: requiredSha256(row.transitionSha256, "transition"),
    artifactSha256: requiredSha256(row.artifactSha256, "artifact"),
    configSchemaSha256: requiredSha256(row.configSchemaSha256, "config schema"),
    migrationSetSha256: requiredSha256(row.migrationSetSha256, "migration set"),
    protocolSetSha256: requiredSha256(row.protocolSetSha256, "protocol set"),
    taskQueueSha256: requiredSha256(row.taskQueueSha256, "task queue"),
    failureConverterSha256: requiredSha256(
      row.failureConverterSha256,
      "failure converter",
    ),
    dependencyEvidenceSha256: requiredSha256(
      row.dependencyEvidenceSha256,
      "dependency evidence",
    ),
    instanceEpoch: positiveInteger(row.instanceEpoch, "instance epoch"),
    nextHeartbeatSequence: positiveInteger(
      row.nextHeartbeatSequence,
      "next heartbeat sequence",
    ),
    claimedAtMs: positiveInteger(row.claimedAtMs, "claim time"),
    replayed: booleanValue(row.replayed, "claim replay"),
  };
  if (
    instance.grantId !== config.grantId ||
    instance.runtimeInstanceId !== config.runtimeInstanceId ||
    instance.runtimeIdentitySha256 !== config.runtimeIdentitySha256 ||
    instance.workerId !== config.workerId ||
    instance.componentId !== config.componentId ||
    instance.role !== config.expectedRole ||
    instance.configSchemaSha256 !== config.configSchemaSha256 ||
    instance.migrationSetSha256 !== config.migrationSetSha256 ||
    instance.protocolSetSha256 !== config.protocolSetSha256 ||
    JSON.stringify(instance.scope) !== JSON.stringify(config.scope)
  ) {
    throw new Error("Managed-cloud runtime claim identity is inconsistent");
  }
  if (
    instance.dependencyEvidenceSha256 !==
    managedCloudDependencyEvidenceSha256(instance)
  ) {
    throw new Error(
      "Managed-cloud runtime dependency evidence is inconsistent",
    );
  }
  return instance;
}

function parseHeartbeat(
  value: unknown,
  input: Record<string, unknown>,
  instance: ManagedCloudRuntimeInstance,
): ManagedCloudRuntimeHeartbeat {
  const row = exactRecord(value, [
    "activationSha256",
    "artifactSha256",
    "componentId",
    "configSchemaSha256",
    "dependencyEvidenceSha256",
    "failureConverterSha256",
    "healthState",
    "heartbeatAtMs",
    "heartbeatSequence",
    "instanceEpoch",
    "manifestSha256",
    "migrationSetSha256",
    "observedHeadRevision",
    "observedTransitionSha256",
    "protocolSetSha256",
    "reasonCode",
    "replayed",
    "role",
    "runtimeInstanceId",
    "taskQueueSha256",
    "workerId",
  ]);
  const parsed = {
    runtimeInstanceId: routeId(row.runtimeInstanceId, "runtime instance"),
    workerId: workerId(row.workerId),
    instanceEpoch: positiveInteger(row.instanceEpoch, "instance epoch"),
    heartbeatSequence: positiveInteger(
      row.heartbeatSequence,
      "heartbeat sequence",
    ),
    observedHeadRevision: positiveInteger(
      row.observedHeadRevision,
      "head revision",
    ),
    observedTransitionSha256: requiredSha256(
      row.observedTransitionSha256,
      "transition",
    ),
    activationSha256: requiredSha256(row.activationSha256, "activation"),
    manifestSha256: requiredSha256(row.manifestSha256, "manifest"),
    componentId: tokenId(row.componentId, "component"),
    role: runtimeRole(row.role),
    artifactSha256: requiredSha256(row.artifactSha256, "artifact"),
    migrationSetSha256: requiredSha256(row.migrationSetSha256, "migration set"),
    configSchemaSha256: requiredSha256(row.configSchemaSha256, "config schema"),
    protocolSetSha256: requiredSha256(row.protocolSetSha256, "protocol set"),
    taskQueueSha256: requiredSha256(row.taskQueueSha256, "task queue"),
    failureConverterSha256: requiredSha256(
      row.failureConverterSha256,
      "failure converter",
    ),
    dependencyEvidenceSha256: requiredSha256(
      row.dependencyEvidenceSha256,
      "dependency evidence",
    ),
    healthState: healthState(row.healthState),
    reasonCode: reasonCode(row.reasonCode),
    heartbeatAtMs: positiveInteger(row.heartbeatAtMs, "heartbeat time"),
    replayed: booleanValue(row.replayed, "heartbeat replay"),
  } satisfies ManagedCloudRuntimeHeartbeat;
  for (const [key, expected] of Object.entries(input)) {
    if (
      key !== "sessionToken" &&
      parsed[key as keyof typeof parsed] !== expected
    ) {
      throw new Error(
        "Managed-cloud heartbeat response identity is inconsistent",
      );
    }
  }
  if (parsed.instanceEpoch !== instance.instanceEpoch) {
    throw new Error("Managed-cloud heartbeat instance fence is inconsistent");
  }
  return parsed;
}

function parseOriginalSourceVerifierStartup(
  value: unknown,
  input: ManagedCloudOriginalSourceVerifierStartupRequest,
): ManagedCloudOriginalSourceVerifierStartup {
  const row = exactRecord(value, [
    "activationSha256",
    "artifactSha256",
    "audience",
    "componentId",
    "configSchemaSha256",
    "degradedHeartbeatSequence",
    "dependencyEvidenceSha256",
    "failureConverterSha256",
    "instanceEpoch",
    "manifestSha256",
    "migrationSetSha256",
    "observedHeadRevision",
    "observedTransitionSha256",
    "probeResult",
    "protocolSetSha256",
    "readyHeartbeatSequence",
    "role",
    "runtimeIdentitySha256",
    "runtimeInstanceId",
    "startupAtMs",
    "startupAuthoritySha256",
    "startupRequestId",
    "taskQueueSha256",
    "version",
    "workerId",
  ]);
  if (
    row.version !== 1 ||
    row.audience !== ORIGINAL_SOURCE_VERIFIER_STARTUP_AUDIENCE
  ) {
    throw new Error(
      "Managed-cloud original-source verifier startup version is invalid",
    );
  }
  const role = runtimeRole(row.role);
  if (role !== "original_source_verifier") {
    throw new Error(
      "Managed-cloud original-source verifier startup role is invalid",
    );
  }
  const probeResult = originalSourceVerifierProbeResult(row.probeResult);
  const parsed: ManagedCloudOriginalSourceVerifierStartup = {
    version: 1,
    audience: ORIGINAL_SOURCE_VERIFIER_STARTUP_AUDIENCE,
    startupRequestId: tokenId(row.startupRequestId, "startup request"),
    runtimeInstanceId: routeId(row.runtimeInstanceId, "runtime instance"),
    workerId: workerId(row.workerId),
    instanceEpoch: positiveInteger(row.instanceEpoch, "instance epoch"),
    runtimeIdentitySha256: requiredSha256(
      row.runtimeIdentitySha256,
      "runtime identity",
    ),
    observedHeadRevision: positiveInteger(
      row.observedHeadRevision,
      "head revision",
    ),
    observedTransitionSha256: requiredSha256(
      row.observedTransitionSha256,
      "transition",
    ),
    activationSha256: requiredSha256(row.activationSha256, "activation"),
    manifestSha256: requiredSha256(row.manifestSha256, "manifest"),
    componentId: tokenId(row.componentId, "component"),
    role,
    artifactSha256: requiredSha256(row.artifactSha256, "artifact"),
    migrationSetSha256: requiredSha256(row.migrationSetSha256, "migration set"),
    configSchemaSha256: requiredSha256(row.configSchemaSha256, "config schema"),
    protocolSetSha256: requiredSha256(row.protocolSetSha256, "protocol set"),
    taskQueueSha256: requiredSha256(row.taskQueueSha256, "task queue"),
    failureConverterSha256: requiredSha256(
      row.failureConverterSha256,
      "failure converter",
    ),
    dependencyEvidenceSha256: requiredSha256(
      row.dependencyEvidenceSha256,
      "dependency evidence",
    ),
    degradedHeartbeatSequence: positiveInteger(
      row.degradedHeartbeatSequence,
      "degraded heartbeat sequence",
    ),
    readyHeartbeatSequence: positiveInteger(
      row.readyHeartbeatSequence,
      "ready heartbeat sequence",
    ),
    probeResult,
    startupAuthoritySha256: requiredSha256(
      row.startupAuthoritySha256,
      "startup authority",
    ),
    startupAtMs: positiveInteger(row.startupAtMs, "startup time"),
  };
  for (const [key, expected] of Object.entries(input)) {
    if (
      key !== "sessionToken" &&
      key !== "expectedDegradedHeartbeatSequence" &&
      parsed[key as keyof typeof parsed] !== expected
    ) {
      throw new Error(
        "Managed-cloud original-source verifier startup identity is inconsistent",
      );
    }
  }
  if (
    parsed.degradedHeartbeatSequence !==
      input.expectedDegradedHeartbeatSequence ||
    parsed.readyHeartbeatSequence !==
      input.expectedDegradedHeartbeatSequence + 1
  ) {
    throw new Error(
      "Managed-cloud original-source verifier startup sequence is inconsistent",
    );
  }
  return parsed;
}

function exactRecord(value: unknown, keys: string[]): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Managed-cloud runtime API response is invalid");
  }
  const row = value as Record<string, unknown>;
  const actual = Object.keys(row).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new Error("Managed-cloud runtime API response shape is invalid");
  }
  return row;
}

function parseScope(value: unknown): ManagedCloudScope {
  const row = exactRecord(value, ["channel", "environment", "region"]);
  const environment = row.environment;
  const channel = row.channel;
  const region = row.region;
  if (
    (environment !== "staging" && environment !== "production") ||
    (channel !== "shadow" && channel !== "canary" && channel !== "general") ||
    typeof region !== "string" ||
    region.length > 64 ||
    !/^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/.test(region)
  ) {
    throw new Error("Managed-cloud runtime scope is invalid");
  }
  return { environment, region, channel };
}

function runtimeRole(value: unknown): ManagedCloudRuntimeRole {
  const roles: ManagedCloudRuntimeRole[] = [
    "discovery_worker",
    "global_discovery_worker",
    "jobs_api",
    "managed_runner",
    "original_source_verifier",
    "workflow_command_dispatcher",
    "workflow_cleanup_dispatcher",
    "workflow_gateway",
    "workflow_worker",
  ];
  if (
    typeof value !== "string" ||
    !roles.includes(value as ManagedCloudRuntimeRole)
  ) {
    throw new Error("Managed-cloud runtime role is invalid");
  }
  return value as ManagedCloudRuntimeRole;
}

function routeId(value: unknown, label: string): string {
  if (typeof value !== "string" || !ROUTE_ID.test(value)) {
    throw new Error(`Managed-cloud ${label} ID is invalid`);
  }
  return value;
}

function workerId(value: unknown): string {
  if (typeof value !== "string" || !ROUTE_ID.test(value)) {
    throw new Error("Managed-cloud worker ID is invalid");
  }
  return value;
}

function tokenId(value: unknown, label: string): string {
  if (typeof value !== "string" || !TOKEN_ID.test(value)) {
    throw new Error(`Managed-cloud ${label} ID is invalid`);
  }
  return value;
}

function requiredSha256(value: unknown, label: string): string {
  if (typeof value !== "string" || !SHA256.test(value)) {
    throw new Error(`Managed-cloud ${label} SHA-256 is invalid`);
  }
  return value;
}

function positiveInteger(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 1) {
    throw new Error(`Managed-cloud ${label} is invalid`);
  }
  return value as number;
}

function booleanValue(value: unknown, label: string): boolean {
  if (typeof value !== "boolean")
    throw new Error(`Managed-cloud ${label} is invalid`);
  return value;
}

function healthState(value: unknown): ManagedCloudHealth["healthState"] {
  if (value !== "ready" && value !== "degraded" && value !== "draining") {
    throw new Error("Managed-cloud health state is invalid");
  }
  return value;
}

function reasonCode(value: unknown): ManagedCloudHealth["reasonCode"] {
  if (
    value === null ||
    [
      "artifact_mismatch",
      "config_mismatch",
      "dependency_unavailable",
      "draining",
      "head_mismatch",
      "migration_mismatch",
      "probe_failed",
      "protocol_mismatch",
      "startup",
    ].includes(value as string)
  ) {
    return value as ManagedCloudHealth["reasonCode"];
  }
  throw new Error("Managed-cloud health reason is invalid");
}

function originalSourceVerifierProbeResult(
  value: unknown,
): ManagedCloudOriginalSourceVerifierProbeResult {
  if (value !== "idle" && value !== "leaseable") {
    throw new Error(
      "Managed-cloud original-source verifier startup probe is invalid",
    );
  }
  return value;
}

function boundedInteger(
  value: number,
  minimum: number,
  maximum: number,
): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error("Managed-cloud runtime timeout is invalid");
  }
  return value;
}
