// Transport/coherence only. Expected issuance is not signed-context proof, and
// neither a parsed receipt nor this client grants business/readiness permission.
import { Buffer } from "node:buffer";
import { createJobsWorkerAuthHeaders } from "./worker-auth.js";
import { assertManagedCloudRuntimeMeasurement, type ManagedCloudRuntimeConfig } from "./managed-cloud-runtime.js";
import {
  deriveManagedCloudProvisioningDependencyEvidenceSha256,
  deriveManagedCloudProvisioningObservationSha256,
  deriveManagedCloudProvisioningSessionToken,
  parseManagedCloudProvisioningClaimRequest,
  parseManagedCloudProvisioningClaimResponse,
  parseManagedCloudProvisioningHeartbeatRequest,
  parseManagedCloudProvisioningHeartbeatResponse,
  provisioningCanonicalEqual,
  provisioningCanonicalJsonBytes,
  type ManagedCloudProvisioningClaimResponse,
  type ManagedCloudProvisioningHeartbeatRequest,
  type ManagedCloudProvisioningHeartbeatResponse,
} from "./managed-cloud-provisioning-protocol.js";

export type ManagedCloudProvisioningExpectedIssuance = Readonly<Pick<ManagedCloudProvisioningClaimResponse,
  "authorityBinding" | "grantId" | "runtimeInstanceId" | "workerId" | "runtimeIdentitySha256" |
  "scope" | "componentId" | "role" | "headRevision" | "transitionSha256" | "activationSha256" |
  "activationExpiresAtMs" | "manifestSha256" | "artifactSha256" | "configSchemaSha256" |
  "migrationSetSha256" | "protocolSetSha256" | "taskQueueSha256" | "failureConverterSha256">>;
export type ManagedCloudProvisioningRuntimeObservation = Readonly<Pick<ManagedCloudProvisioningHeartbeatRequest,
  "lifecycleState" | "acknowledgedLatchSha256" | "dependencyProbe" | "queueObservation" |
  "healthState" | "reasonCode" | "bootId" | "bootGeneration">>;
type RuntimeFetch = (input: string, init: RequestInit) => Promise<Response>;
type ErrorCode = "invalid_configuration" | "measurement_changed" | "busy" | "claim_required" |
  "invalid_request" | "binding_mismatch" | "replay_mismatch" | "sequence_mismatch" |
  "transport_failed" | "timeout" | "invalid_response";
export class ManagedCloudProvisioningRuntimeClientError extends Error {
  constructor(readonly code: ErrorCode) { super(`Managed-cloud provisioning client: ${code}`); }
}
const EXPECTED_KEYS = ["authorityBinding", "grantId", "runtimeInstanceId", "workerId", "runtimeIdentitySha256",
  "scope", "componentId", "role", "headRevision", "transitionSha256", "activationSha256",
  "activationExpiresAtMs", "manifestSha256", "artifactSha256", "configSchemaSha256", "migrationSetSha256",
  "protocolSetSha256", "taskQueueSha256", "failureConverterSha256"] as const;
const OBSERVATION_KEYS = ["lifecycleState", "acknowledgedLatchSha256", "dependencyProbe", "queueObservation",
  "healthState", "reasonCode"];
const MAX_BYTES = 64 * 1024;
function fail(code: ErrorCode): never {
  throw new ManagedCloudProvisioningRuntimeClientError(code);
}

export class ManagedCloudProvisioningRuntimeApiClient {
  private readonly config: ManagedCloudRuntimeConfig;
  private readonly expected: ManagedCloudProvisioningExpectedIssuance;
  private readonly dependency: string;
  private readonly fetcher: RuntimeFetch;
  private readonly timeoutMs: number;
  private readonly claimBody: string;
  private claiming = false;
  private instance?: ManagedCloudProvisioningClaimResponse;
  private cursor?: number;
  private latch: string | null = null;
  private last?: { sequence: number; body: string; receipt?: ManagedCloudProvisioningHeartbeatResponse };

  constructor(options: Readonly<{
    config: Omit<ManagedCloudRuntimeConfig, "sessionToken">;
    expected: ManagedCloudProvisioningExpectedIssuance;
    fetch?: RuntimeFetch;
    timeoutMs?: number;
  }>) {
    try {
      const origin = new URL(options.config.apiOrigin);
      if (origin.protocol !== "https:" || origin.origin !== options.config.apiOrigin
        || origin.username || origin.password) fail("invalid_configuration");
      this.timeoutMs = options.timeoutMs ?? 5_000;
      if (!Number.isSafeInteger(this.timeoutMs) || this.timeoutMs < 1 || this.timeoutMs > 30_000) {
        fail("invalid_configuration");
      }
      const expected = clone(options.expected);
      exactKeys(expected, EXPECTED_KEYS);
      this.dependency = deriveManagedCloudProvisioningDependencyEvidenceSha256({
        role: expected.role, activationSha256: expected.activationSha256, manifestSha256: expected.manifestSha256,
        componentId: expected.componentId, artifactSha256: expected.artifactSha256,
        taskQueueSha256: expected.taskQueueSha256, failureConverterSha256: expected.failureConverterSha256,
        authorityBinding: expected.authorityBinding,
      });
      // Reuse the closed response grammar solely to validate all expected fields.
      parseManagedCloudProvisioningClaimResponse({ ...expected, version: 2, dependencyEvidenceSha256: this.dependency,
        instanceEpoch: 1, nextHeartbeatSequence: 1, claimedAtMs: 1, servingLatchSha256: null, replayed: false });
      this.expected = deepFreeze(expected);
      const config = options.config;
      for (const key of ["grantId", "runtimeInstanceId", "workerId", "runtimeIdentitySha256", "componentId",
        "configSchemaSha256", "migrationSetSha256", "protocolSetSha256", "scope"] as const) {
        if (!provisioningCanonicalEqual(config[key], expected[key])) fail("invalid_configuration");
      }
      if (config.expectedRole !== expected.role) fail("invalid_configuration");
      const sessionToken = deriveManagedCloudProvisioningSessionToken(config.grantToken, config.grantId,
        config.runtimeInstanceId, expected.authorityBinding);
      this.config = Object.freeze({ ...config, sessionToken, scope: Object.freeze({ ...config.scope }),
        measurementOptions: Object.freeze({ ...config.measurementOptions }) });
      this.claimBody = JSON.stringify(parseManagedCloudProvisioningClaimRequest({ version: 2,
        authorityBinding: expected.authorityBinding, grantId: config.grantId, grantToken: config.grantToken,
        workerId: config.workerId, runtimeInstanceId: config.runtimeInstanceId,
        runtimeIdentitySha256: config.runtimeIdentitySha256, sessionToken }));
      this.fetcher = options.fetch ?? globalThis.fetch;
    } catch { fail("invalid_configuration"); }
  }

  async claim(): Promise<ManagedCloudProvisioningClaimResponse> {
    return this.exclusive(async () => {
      this.measure();
      const receipt = parseResponse(parseManagedCloudProvisioningClaimResponse,
        await this.request(`/api/jobs/internal/managed-cloud/v2/runtime-grants/${this.config.grantId}/claim`, this.claimBody));
      for (const key of EXPECTED_KEYS) {
        if (!provisioningCanonicalEqual(receipt[key], this.expected[key])) fail("binding_mismatch");
      }
      if (receipt.dependencyEvidenceSha256 !== this.dependency) fail("binding_mismatch");
      if (this.instance) {
        if (!provisioningCanonicalEqual(claimSnapshot(this.instance), claimSnapshot(receipt))) fail("replay_mismatch");
        if (receipt.nextHeartbeatSequence < this.cursor!) fail("sequence_mismatch");
      }
      this.instance ??= deepFreeze(receipt);
      this.cursor = receipt.nextHeartbeatSequence;
      // A claim replay cannot discover a newer latch; the immutable snapshot above enforces this.
      if (!this.last) this.latch = receipt.servingLatchSha256;
      return deepFreeze(receipt);
    });
  }

  async heartbeat(sequence: number, observation: ManagedCloudProvisioningRuntimeObservation):
  Promise<ManagedCloudProvisioningHeartbeatResponse> {
    return this.exclusive(async () => {
      if (!this.instance) fail("claim_required");
      if (!this.last) this.measure();
      let request: ManagedCloudProvisioningHeartbeatRequest;
      try {
        const observed = clone(observation);
        const boot = Object.hasOwn(observed, "bootId") || Object.hasOwn(observed, "bootGeneration");
        exactKeys(observed, boot ? [...OBSERVATION_KEYS, "bootId", "bootGeneration"] : OBSERVATION_KEYS);
        const e = this.expected;
        request = parseManagedCloudProvisioningHeartbeatRequest({ ...observed, version: 2,
          authorityBinding: e.authorityBinding, runtimeInstanceId: e.runtimeInstanceId, workerId: e.workerId,
          sessionToken: this.config.sessionToken, role: e.role, componentId: e.componentId,
          heartbeatSequence: sequence, observedHeadRevision: e.headRevision, observedTransitionSha256: e.transitionSha256,
          activationSha256: e.activationSha256, manifestSha256: e.manifestSha256, artifactSha256: e.artifactSha256,
          configSchemaSha256: e.configSchemaSha256, migrationSetSha256: e.migrationSetSha256,
          protocolSetSha256: e.protocolSetSha256, taskQueueSha256: e.taskQueueSha256,
          failureConverterSha256: e.failureConverterSha256, dependencyEvidenceSha256: this.dependency });
      } catch { fail("invalid_request"); }
      const body = JSON.stringify(request);
      const replay = this.last?.sequence === sequence;
      if (replay && this.last!.body !== body) fail("replay_mismatch");
      if (!replay && (this.last && !this.last.receipt || sequence !== this.cursor)) fail("sequence_mismatch");
      if (request.acknowledgedLatchSha256 !== null && request.acknowledgedLatchSha256 !== this.latch) {
        fail("binding_mismatch");
      }
      if (!replay) this.last = { sequence, body };
      const receipt = parseResponse(parseManagedCloudProvisioningHeartbeatResponse,
        await this.request(`/api/jobs/internal/managed-cloud/v2/runtime-instances/${this.config.runtimeInstanceId}/heartbeats`, body));
      for (const key of Object.keys(request) as (keyof ManagedCloudProvisioningHeartbeatRequest)[]) {
        if (key === "sessionToken" || key === "bootId" || key === "bootGeneration") continue;
        if (!provisioningCanonicalEqual(request[key], receipt[key as keyof typeof receipt])) fail("binding_mismatch");
      }
      if (receipt.instanceEpoch !== this.instance.instanceEpoch) fail("binding_mismatch");
      if (receipt.observationSha256 !== deriveManagedCloudProvisioningObservationSha256({
        grantId: this.expected.grantId, heartbeat: request, heartbeatAtMs: receipt.heartbeatAtMs,
        instanceEpoch: receipt.instanceEpoch, servingLatchSha256: receipt.servingLatchSha256,
      })) fail("binding_mismatch");
      if (this.last!.receipt && !provisioningCanonicalEqual(heartbeatSnapshot(this.last!.receipt), heartbeatSnapshot(receipt))) {
        fail("replay_mismatch");
      }
      if ((this.latch !== null && receipt.servingLatchSha256 !== this.latch)
        || (request.acknowledgedLatchSha256 !== null && receipt.servingLatchSha256 !== request.acknowledgedLatchSha256)) {
        fail("binding_mismatch");
      }
      if (!Number.isSafeInteger(sequence + 1)) fail("sequence_mismatch");
      this.last!.receipt = deepFreeze(receipt);
      this.cursor = Math.max(this.cursor!, sequence + 1);
      this.latch = receipt.servingLatchSha256;
      return this.last!.receipt;
    });
  }

  private measure(): void {
    try { assertManagedCloudRuntimeMeasurement(this.config); } catch { fail("measurement_changed"); }
  }
  private async exclusive<T>(operation: () => Promise<T>): Promise<T> {
    if (this.claiming) fail("busy");
    this.claiming = true;
    try { return await operation(); } finally { this.claiming = false; }
  }
  private async request(path: string, body: string): Promise<unknown> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
    try {
      const response = await abortable(this.fetcher(this.config.apiOrigin + path, { method: "POST", body,
        redirect: "error", credentials: "omit", signal: controller.signal,
        headers: { ...createJobsWorkerAuthHeaders({ signingKey: this.config.signingKey, workerId: this.config.workerId,
          method: "POST", path, body, origin: this.config.apiOrigin }), "content-type": "application/json" } }), controller.signal);
      if (response.body) reader = response.body.getReader();
      if (response.status !== 200 || response.headers.get("content-type") !== "application/json"
        || response.headers.get("cache-control") !== "no-store"
        || response.headers.get("x-content-type-options") !== "nosniff" || !reader) fail("invalid_response");
      const chunks: Uint8Array[] = [];
      let size = 0;
      while (true) {
        const next = await abortable(reader.read(), controller.signal);
        if (next.done) break;
        size += next.value.byteLength;
        if (size > MAX_BYTES) fail("invalid_response");
        chunks.push(next.value);
      }
      let text: string;
      try { text = new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks)); }
      catch { return fail("invalid_response"); }
      return ordinaryJson(text);
    } catch (error) {
      if (controller.signal.aborted) fail("timeout");
      if (error instanceof ManagedCloudProvisioningRuntimeClientError) throw error;
      fail("transport_failed");
    } finally {
      clearTimeout(timer);
      // Never wait indefinitely for a custom/misbehaving stream's cancel hook.
      if (reader) { void reader.cancel().catch(() => undefined); reader.releaseLock(); }
    }
  }
}

async function abortable<T>(operation: Promise<T>, signal: AbortSignal): Promise<T> {
  if (signal.aborted) return fail("timeout");
  let abort: (() => void) | undefined;
  try {
    return await Promise.race([operation, new Promise<never>((_resolve, reject) => {
      abort = () => reject(new ManagedCloudProvisioningRuntimeClientError("timeout"));
      signal.addEventListener("abort", abort, { once: true });
    })]);
  } finally { if (abort) signal.removeEventListener("abort", abort); }
}

function claimSnapshot(value: ManagedCloudProvisioningClaimResponse): unknown {
  const { replayed: _replayed, nextHeartbeatSequence: _cursor, ...snapshot } = value;
  return snapshot;
}
function heartbeatSnapshot(value: ManagedCloudProvisioningHeartbeatResponse): unknown {
  const { replayed: _replayed, ...snapshot } = value;
  return snapshot;
}
function parseResponse<T>(parser: (value: unknown) => T, value: unknown): T {
  try { return parser(value); } catch { return fail("invalid_response"); }
}
function clone<T>(value: T): T {
  return JSON.parse(Buffer.from(provisioningCanonicalJsonBytes(value)).toString("utf8")) as T;
}
function deepFreeze<T>(value: T): T {
  if (value && typeof value === "object") {
    Object.values(value).forEach(deepFreeze);
    Object.freeze(value);
  }
  return value;
}
function exactKeys(value: object, keys: readonly string[]): void {
  if (!provisioningCanonicalEqual(Object.keys(value).sort(), [...keys].sort())) fail("invalid_request");
}

// Ordinary HTTP JSON need not have canonical spacing/order. Keep raw text until
// escaped-key duplicate detection has completed; JSON.parse alone cannot do it.
function ordinaryJson(text: string): unknown {
  let cursor = 0;
  let nodes = 0;
  const bad = (): never => fail("invalid_response");
  const whitespace = (): void => { while (/[\x20\t\r\n]/.test(text[cursor] ?? "x")) cursor += 1; };
  const string = (): string => {
    const start = cursor++;
    if (text[start] !== '"') bad();
    while (cursor < text.length) {
      const character = text[cursor++];
      if (character === '"') {
        try { return JSON.parse(text.slice(start, cursor)) as string; } catch { return bad(); }
      }
      if (character === "\\") cursor += 1;
    }
    return bad();
  };
  const value = (depth: number): void => {
    if (++nodes > 4096 || depth > 16) bad();
    whitespace();
    if (text[cursor] === "{") {
      cursor += 1; whitespace();
      const keys = new Set<string>();
      if (text[cursor] === "}") { cursor += 1; return; }
      while (cursor < text.length) {
        whitespace();
        const key = string();
        if (Buffer.byteLength(key) > 256 || keys.has(key) || keys.size >= 128) bad();
        keys.add(key); whitespace();
        if (text[cursor++] !== ":") bad();
        value(depth + 1); whitespace();
        const delimiter = text[cursor++];
        if (delimiter === "}") return;
        if (delimiter !== ",") bad();
      }
      bad();
    } else if (text[cursor] === "[") {
      cursor += 1; whitespace();
      if (text[cursor] === "]") { cursor += 1; return; }
      while (cursor < text.length) {
        value(depth + 1); whitespace();
        const delimiter = text[cursor++];
        if (delimiter === "]") return;
        if (delimiter !== ",") bad();
      }
      bad();
    } else if (text[cursor] === '"') { string(); }
    else {
      const match = /^(?:true|false|null|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)/.exec(text.slice(cursor));
      if (!match) bad();
      cursor += match![0].length;
    }
  };
  value(0); whitespace();
  if (cursor !== text.length) bad();
  try { return JSON.parse(text) as unknown; } catch { return bad(); }
}
