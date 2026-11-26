import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { managedCloudRuntimeConfig } from "../src/managed-cloud-runtime.js";
import { ManagedCloudProvisioningRuntimeApiClient,
  type ManagedCloudProvisioningExpectedIssuance,
  type ManagedCloudProvisioningRuntimeObservation } from "../src/managed-cloud-provisioning-runtime-client.js";
import { deriveManagedCloudProvisioningDependencyEvidenceSha256,
  deriveManagedCloudProvisioningObservationSha256,
  deriveManagedCloudProvisioningSessionToken,
  parseManagedCloudProvisioningHeartbeatRequest,
  provisioningCanonicalJsonBytes,
  type ManagedCloudProvisioningClaimResponse,
  type ManagedCloudProvisioningHeartbeatResponse } from "../src/managed-cloud-provisioning-protocol.js";

const ROOT = mkdtempSync(join(tmpdir(), "bluey-provisioning-client-"));
const MEASUREMENT_PATH = join(ROOT, "app/.bluey/managed-cloud-runtime-measurement.json");
const FILES = { "app/workflows/dist/gateway.js": "synthetic gateway\n", "usr/local/bin/node": "synthetic node\n" };
for (const [path, bytes] of Object.entries(FILES)) {
  mkdirSync(join(ROOT, path, ".."), { recursive: true });
  writeFileSync(join(ROOT, path), bytes);
}
mkdirSync(join(ROOT, "app/.bluey"), { recursive: true });
mkdirSync(join(ROOT, "app/automation"), { recursive: true });
writeFileSync(MEASUREMENT_PATH, provisioningCanonicalJsonBytes({ version: 1,
  audience: "bluey-jobs-managed-cloud-runtime-measurement-v1", buildId: "synthetic-provisioning-client",
  componentId: "jobs-workflows", sourceCommit: "a".repeat(40),
  roles: ["original_source_verifier", "workflow_gateway", "workflow_worker"],
  configSchemaSha256: "1".repeat(64), migrationSetSha256: "2".repeat(64), protocolSetSha256: "3".repeat(64),
  measuredFiles: Object.entries(FILES).map(([path, bytes]) => ({ path,
    sha256: createHash("sha256").update(bytes).digest("hex") })),
}));
const ENV = { BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED: "true",
  BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN: "https://jobs-api.internal",
  BLUEY_JOBS_WORKER_SIGNING_KEY: "synthetic-worker-signing-key-0123456789",
  BLUEY_JOBS_MANAGED_CLOUD_WORKER_ID: "worker-provisioning-1234567890",
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_ID: "grant-provisioning-1234567890",
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_TOKEN: "A".repeat(43),
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_INSTANCE_ID: "instance-provisioning-1234567890",
  BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT: "staging", BLUEY_JOBS_MANAGED_CLOUD_REGION: "us-east-1",
  BLUEY_JOBS_MANAGED_CLOUD_CHANNEL: "canary" };
const CONFIG = managedCloudRuntimeConfig("workflow_gateway", ENV,
  { measurementPath: MEASUREMENT_PATH, rootPath: ROOT })!;
afterAll(() => rmSync(ROOT, { recursive: true, force: true }));
const EXPECTED: ManagedCloudProvisioningExpectedIssuance = {
  authorityBinding: { authorityKind: "provisioning_v4", purpose: "nonproduction_synthetic",
    contextSha256: "c".repeat(64), queueGenerationId: "queue-provisioning-1234567890" },
  grantId: CONFIG.grantId, runtimeInstanceId: CONFIG.runtimeInstanceId, workerId: CONFIG.workerId,
  runtimeIdentitySha256: CONFIG.runtimeIdentitySha256, scope: CONFIG.scope, role: "workflow_gateway",
  componentId: "jobs-workflows", headRevision: 7, transitionSha256: "4".repeat(64),
  activationSha256: "5".repeat(64), activationExpiresAtMs: 1_900_000_000_000,
  manifestSha256: "6".repeat(64), artifactSha256: "7".repeat(64),
  configSchemaSha256: CONFIG.configSchemaSha256, migrationSetSha256: CONFIG.migrationSetSha256,
  protocolSetSha256: CONFIG.protocolSetSha256, taskQueueSha256: "8".repeat(64), failureConverterSha256: "9".repeat(64),
};
const DEPENDENCY = deriveManagedCloudProvisioningDependencyEvidenceSha256({ role: EXPECTED.role,
  authorityBinding: EXPECTED.authorityBinding, activationSha256: EXPECTED.activationSha256,
  manifestSha256: EXPECTED.manifestSha256, componentId: EXPECTED.componentId, artifactSha256: EXPECTED.artifactSha256,
  taskQueueSha256: EXPECTED.taskQueueSha256, failureConverterSha256: EXPECTED.failureConverterSha256 });
const CLAIM: ManagedCloudProvisioningClaimResponse = { ...EXPECTED, version: 2, dependencyEvidenceSha256: DEPENDENCY,
  instanceEpoch: 3, claimedAtMs: 1_800_000_000_000, nextHeartbeatSequence: 1, replayed: false, servingLatchSha256: null };
const OBSERVATION: ManagedCloudProvisioningRuntimeObservation = { lifecycleState: "parked",
  acknowledgedLatchSha256: null, healthState: "degraded", reasonCode: "startup", queueObservation: null,
  dependencyProbe: { version: 1, kind: "workflow_gateway", listenerState: "listening",
    temporalNamespace: { endpointId: "temporal-endpoint-1234567890", transport: "tls", authority: "temporal.test:7233",
      namespace: "synthetic", namespaceId: "synthetic-namespace-id", state: "registered" } } };
const HEADERS = { "content-type": "application/json", "cache-control": "no-store", "x-content-type-options": "nosniff" };
const response = (value: unknown): Response => new Response(JSON.stringify(value), { headers: HEADERS });
function heartbeat(body: string, servingLatchSha256: string | null = null,
  heartbeatAtMs = 1_800_000_000_001): ManagedCloudProvisioningHeartbeatResponse {
  const request = parseManagedCloudProvisioningHeartbeatRequest(JSON.parse(body));
  const row = { ...request } as Record<string, unknown>;
  delete row.sessionToken; delete row.bootId; delete row.bootGeneration;
  return { ...row, instanceEpoch: 3, heartbeatAtMs, replayed: false,
    observationSha256: deriveManagedCloudProvisioningObservationSha256({ grantId: CONFIG.grantId,
      heartbeat: request, heartbeatAtMs, instanceEpoch: 3, servingLatchSha256 }),
    servingLatchSha256 } as ManagedCloudProvisioningHeartbeatResponse;
}
function client(fetcher: (url: string, init: RequestInit) => Promise<Response>) {
  const { sessionToken: _legacy, ...config } = CONFIG;
  return new ManagedCloudProvisioningRuntimeApiClient({ config, expected: EXPECTED, fetch: fetcher });
}
function different(value: unknown): unknown {
  if (typeof value === "number") return value + 1;
  if (typeof value === "string") return value === "workflow_gateway" ? "workflow_worker"
    : value === "jobs-workflows" ? "jobs-api" : value === "startup" ? "probe_failed"
      : value === "degraded" ? "ready" : value === "parked" ? "operational"
        : value === "0".repeat(64) ? "1".repeat(64) : /^[a-f0-9]{64}$/.test(value) ? "0".repeat(64) : value + "x";
  if (value === null) return "e".repeat(64);
  if (typeof value === "object") return { ...value, extra: true };
  return true;
}

describe("provisioning v2 transport coherence (not admission authority)", () => {
  it("uses measured files, independent expected issuance, explicit routes and derived session", async () => {
    const calls: Array<{ url: string; init: RequestInit }> = [];
    const api = client(async (url, init) => { calls.push({ url, init });
      return response(url.endsWith("/claim") ? CLAIM : heartbeat(String(init.body))); });
    expect(await api.claim()).toEqual(CLAIM);
    await api.heartbeat(1, structuredClone(OBSERVATION));
    expect(calls.map((call) => new URL(call.url).pathname)).toEqual([
      `/api/jobs/internal/managed-cloud/v2/runtime-grants/${CONFIG.grantId}/claim`,
      `/api/jobs/internal/managed-cloud/v2/runtime-instances/${CONFIG.runtimeInstanceId}/heartbeats`]);
    for (const { init } of calls) {
      expect(init.redirect).toBe("error"); expect(init.credentials).toBe("omit");
      expect(JSON.parse(String(init.body)).sessionToken).toBe(deriveManagedCloudProvisioningSessionToken(
        CONFIG.grantToken, CONFIG.grantId, CONFIG.runtimeInstanceId, EXPECTED.authorityBinding));
      expect(init.headers).toHaveProperty("x-bluey-jobs-worker-scope", "managed-cloud-runtime");
    }
  });
  it.each(Object.keys(EXPECTED))("rejects claim expected-field drift: %s", async (key) => {
    const api = client(async () => response({ ...CLAIM, [key]: different(CLAIM[key as keyof typeof CLAIM]) }));
    await expect(api.claim()).rejects.toThrow();
  });
  it("rejects dependency drift and response extra fields", async () => {
    for (const value of [{ ...CLAIM, dependencyEvidenceSha256: "0".repeat(64) }, { ...CLAIM, private: "forbidden" }]) {
      await expect(client(async () => response(value)).claim()).rejects.toThrow();
    }
  });
  it.each(["claimedAtMs", "instanceEpoch", "servingLatchSha256"] as const)("pins claim snapshot %s", async (key) => {
    let calls = 0;
    const api = client(async () => response(calls++ === 0 ? CLAIM : { ...CLAIM, replayed: true,
      [key]: different(CLAIM[key]) }));
    await api.claim(); await expect(api.claim()).rejects.toThrow();
  });
  it("allows only claim replay flag/current cursor to change", async () => {
    let calls = 0;
    const api = client(async () => response({ ...CLAIM, replayed: calls++ > 0, nextHeartbeatSequence: calls }));
    await api.claim(); expect((await api.claim()).nextHeartbeatSequence).toBe(2);
  });
  it.each(["workerId", "runtimeInstanceId", "role", "componentId", "activationSha256", "manifestSha256",
    "artifactSha256", "configSchemaSha256", "migrationSetSha256", "protocolSetSha256", "taskQueueSha256",
    "failureConverterSha256", "dependencyEvidenceSha256", "observedHeadRevision", "observedTransitionSha256",
    "heartbeatSequence", "healthState", "reasonCode", "lifecycleState", "acknowledgedLatchSha256",
    "authorityBinding", "dependencyProbe", "queueObservation", "instanceEpoch"])("rejects heartbeat drift: %s", async (key) => {
    const api = client(async (url, init) => {
      if (url.endsWith("/claim")) return response(CLAIM);
      const receipt = heartbeat(String(init.body));
      return response({ ...receipt, [key]: different(receipt[key as keyof typeof receipt]) });
    });
    await api.claim(); await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow();
  });
  it("retries exact pending bytes only and never automatically resends", async () => {
    const bodies: string[] = []; let failed = false;
    const api = client(async (url, init) => {
      if (url.endsWith("/claim")) return response(CLAIM);
      bodies.push(String(init.body));
      if (!failed) { failed = true; throw new Error("private synthetic failure"); }
      return response(heartbeat(String(init.body)));
    });
    await api.claim(); await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow("transport_failed");
    expect(bodies).toHaveLength(1);
    await expect(api.heartbeat(2, OBSERVATION)).rejects.toThrow("sequence_mismatch");
    await expect(api.heartbeat(1, { ...OBSERVATION, reasonCode: "probe_failed" })).rejects.toThrow("replay_mismatch");
    await api.heartbeat(1, OBSERVATION); expect(bodies[0]).toBe(bodies[1]);
  });
  it.each(["heartbeatAtMs", "observationSha256", "servingLatchSha256"] as const)("rejects heartbeat replay change %s", async (key) => {
    let calls = 0;
    const api = client(async (url, init) => {
      if (url.endsWith("/claim")) return response(CLAIM);
      const receipt = heartbeat(String(init.body));
      if (calls++ === 0) return response(receipt);
      const changed = key === "heartbeatAtMs" ? heartbeat(String(init.body), null, receipt.heartbeatAtMs + 1)
        : key === "servingLatchSha256" ? heartbeat(String(init.body), "e".repeat(64))
          : { ...receipt, observationSha256: "0".repeat(64) };
      return response({ ...changed, replayed: true });
    });
    await api.claim(); await api.heartbeat(1, OBSERVATION);
    await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow();
  });
  it.each(["observationSha256", "heartbeatAtMs", "servingLatchSha256"] as const)(
    "rejects an incoherent first heartbeat digest after %s drift and permits exact recovery", async (key) => {
      const bodies: string[] = [];
      const api = client(async (url, init) => {
        if (url.endsWith("/claim")) return response(CLAIM);
        bodies.push(String(init.body));
        const receipt = heartbeat(String(init.body));
        return response(bodies.length === 1 ? { ...receipt, [key]: different(receipt[key]) } : receipt);
      });
      await api.claim();
      await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow("binding_mismatch");
      await expect(api.heartbeat(2, OBSERVATION)).rejects.toThrow("sequence_mismatch");
      expect((await api.heartbeat(1, OBSERVATION)).heartbeatSequence).toBe(1);
      expect(bodies).toHaveLength(2);
      expect(bodies[0]).toBe(bodies[1]);
    },
  );
  it("allows replay flag only and learns a latch only on new accepted sequence", async () => {
    let calls = 0;
    const api = client(async (url, init) => {
      if (url.endsWith("/claim")) return response(CLAIM);
      const sequence = (JSON.parse(String(init.body)) as { heartbeatSequence: number }).heartbeatSequence;
      return response({ ...heartbeat(String(init.body), sequence > 1 ? "e".repeat(64) : null),
        replayed: calls++ === 1 });
    });
    await api.claim(); await api.heartbeat(1, OBSERVATION); await api.heartbeat(1, structuredClone(OBSERVATION));
    await api.heartbeat(2, OBSERVATION);
    await api.heartbeat(3, { ...OBSERVATION, lifecycleState: "latch_acknowledged", acknowledgedLatchSha256: "e".repeat(64) });
  });
  it("rejects unclaimed heartbeat and unknown/unpaired observation fields before HTTP", async () => {
    let calls = 0; const api = client(async () => { calls++; return response(CLAIM); });
    await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow("claim_required");
    await api.claim();
    for (const bad of [{ ...OBSERVATION, private: "forbidden" }, { ...OBSERVATION, bootId: "boot-12345678901234567890" },
      { ...OBSERVATION, bootId: "boot-12345678901234567890", bootGeneration: 1 }]) {
      await expect(api.heartbeat(1, bad)).rejects.toThrow("invalid_request");
    }
    expect(calls).toBe(1);
  });
  it("accepts ordinary JSON spacing/key order but rejects nested escaped duplicate keys", async () => {
    const ordinary = JSON.stringify(Object.fromEntries(Object.entries(CLAIM).reverse()), null, 2);
    expect(await client(async () => new Response(ordinary, { headers: HEADERS })).claim()).toEqual(CLAIM);
    const duplicate = JSON.stringify(CLAIM).replace('"queueGenerationId":', '"queueGenerationId":"unused", "\\u0071ueueGenerationId":');
    await expect(client(async () => new Response(duplicate, { headers: HEADERS })).claim()).rejects.toThrow("invalid_response");
  });
  it.each(["depth", "size", "encoding", "keys", "nodes"])("bounds raw response %s", async (kind) => {
    const bytes = kind === "encoding" ? Uint8Array.of(0xff) : Buffer.from(kind === "depth" ? "[".repeat(18) + "0" + "]".repeat(18)
      : kind === "size" ? " ".repeat(65537) : kind === "keys" ? JSON.stringify(Object.fromEntries(Array.from({ length: 129 }, (_, i) => [i, 0])))
        : JSON.stringify(Array(4097).fill(0)));
    await expect(client(async () => new Response(bytes, { headers: HEADERS })).claim()).rejects.toThrow();
  });
  it("cancels oversized streamed bodies without exposing body/errors", async () => {
    let canceled = false;
    const api = client(async () => new Response(new ReadableStream<Uint8Array>({
      start(controller) { controller.enqueue(Buffer.alloc(65537, 65)); }, cancel() { canceled = true; },
    }), { headers: HEADERS }));
    await expect(api.claim()).rejects.toThrow("invalid_response"); expect(canceled).toBe(true);
  });
  it.each(["http://jobs-api.internal", "https://jobs-api.internal/", "https://user@jobs-api.internal",
    "https://jobs-api.internal/path", "https://jobs-api.internal?x=1", "https://jobs-api.internal#x"])("rejects noncanonical origin %s", (apiOrigin) => {
    expect(() => new ManagedCloudProvisioningRuntimeApiClient({ config: { ...CONFIG, apiOrigin }, expected: EXPECTED })).toThrow("invalid_configuration");
  });
  it("checks expected/config coherence and actual files again before claim", async () => {
    expect(() => new ManagedCloudProvisioningRuntimeApiClient({ config: { ...CONFIG, workerId: "different-worker-1234567890" },
      expected: EXPECTED })).toThrow("invalid_configuration");
    let calls = 0; const api = client(async () => { calls++; return response(CLAIM); });
    writeFileSync(join(ROOT, "app/workflows/dist/gateway.js"), "tampered");
    try { await expect(api.claim()).rejects.toThrow("measurement_changed"); expect(calls).toBe(0); }
    finally { writeFileSync(join(ROOT, "app/workflows/dist/gateway.js"), FILES["app/workflows/dist/gateway.js"]); }
  });
  it("copies expected inputs and serializes one operation at a time", async () => {
    let resolve!: (response: Response) => void;
    const expected = { ...EXPECTED, scope: { ...EXPECTED.scope } };
    const api = new ManagedCloudProvisioningRuntimeApiClient({ config: CONFIG, expected,
      fetch: async () => new Promise<Response>((done) => { resolve = done; }) });
    expected.scope.region = "changed";
    const pending = api.claim(); await expect(api.claim()).rejects.toThrow("busy");
    resolve(response(CLAIM)); expect(await pending).toEqual(CLAIM);
  });
  it("compares valid nested probe/binding drift structurally, not by object identity", async () => {
    for (const field of ["dependencyProbe", "authorityBinding"] as const) {
      const api = client(async (url, init) => {
        if (url.endsWith("/claim")) return response(CLAIM);
        const receipt = heartbeat(String(init.body));
        const changed = field === "authorityBinding"
          ? { ...receipt.authorityBinding, queueGenerationId: "different-queue-1234567890" }
          : { ...OBSERVATION.dependencyProbe, temporalNamespace: {
            endpointId: "temporal-endpoint-1234567890", transport: "tls", authority: "other.test:7233",
            namespace: "synthetic", namespaceId: "synthetic-namespace-id", state: "registered" } };
        return response({ ...receipt, [field]: changed });
      });
      await api.claim(); await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow("binding_mismatch");
    }
  });
  it("accepts paired generation-aware OSV request-only boot fields without demanding response echoes", async () => {
    const config = managedCloudRuntimeConfig("original_source_verifier", ENV,
      { measurementPath: MEASUREMENT_PATH, rootPath: ROOT })!;
    const expected = { ...EXPECTED, role: "original_source_verifier" as const,
      runtimeIdentitySha256: config.runtimeIdentitySha256 };
    const dependency = deriveManagedCloudProvisioningDependencyEvidenceSha256({ role: expected.role,
      authorityBinding: expected.authorityBinding, activationSha256: expected.activationSha256,
      manifestSha256: expected.manifestSha256, componentId: expected.componentId, artifactSha256: expected.artifactSha256,
      taskQueueSha256: expected.taskQueueSha256, failureConverterSha256: expected.failureConverterSha256 });
    const api = new ManagedCloudProvisioningRuntimeApiClient({ config, expected, fetch: async (url, init) =>
      response(url.endsWith("/claim") ? { ...CLAIM, ...expected, dependencyEvidenceSha256: dependency }
        : heartbeat(String(init.body))) });
    await api.claim();
    const bootId = "boot:/._-12345678901234567890";
    const receipt = await api.heartbeat(1, { ...OBSERVATION, bootId, bootGeneration: 2,
      dependencyProbe: { version: 1, kind: "original_source_verifier", loopState: "parked", controlProgress: "fresh",
        bootStartup: { bootId, bootGeneration: 2, startupRequestId: "startup:/._-12345678901234567890", parkedHeartbeatSequence: 1 } } });
    expect(receipt).not.toHaveProperty("bootId"); expect(receipt).not.toHaveProperty("sessionToken");
  });
  it("rejects OSV boot fields added to the closed response", async () => {
    const api = client(async (url, init) => response(url.endsWith("/claim") ? CLAIM : {
      ...heartbeat(String(init.body)), bootId: "boot-12345678901234567890", bootGeneration: 1 }));
    await api.claim();
    await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow("invalid_response");
  });
  it.each(["fetch", "body"])("bounds timeout across %s and keeps finite private diagnostics", async (stage) => {
    let canceled = false;
    const api = new ManagedCloudProvisioningRuntimeApiClient({ config: CONFIG, expected: EXPECTED, timeoutMs: 1,
      fetch: async () => stage === "fetch" ? new Promise<Response>(() => undefined)
        : new Response(new ReadableStream<Uint8Array>({ cancel() { canceled = true; } }), { headers: HEADERS }) });
    await expect(api.claim()).rejects.toThrow("timeout");
    if (stage === "body") expect(canceled).toBe(true);
  });
  it.each(["status", "content-type", "cache-control", "x-content-type-options"])("rejects invalid transport metadata %s", async (field) => {
    const headers: Record<string, string> = { ...HEADERS };
    if (field !== "status") headers[field] = "invalid";
    await expect(client(async () => new Response(JSON.stringify(CLAIM), { headers,
      status: field === "status" ? 409 : 200 })).claim()).rejects.toThrow("invalid_response");
  });
  it("rejects missing/extra expected issuance and canonical non-roundtripping grant tokens", () => {
    const { manifestSha256: _missing, ...missing } = EXPECTED;
    for (const expected of [missing, { ...EXPECTED, extra: "forbidden" }]) {
      expect(() => new ManagedCloudProvisioningRuntimeApiClient({ config: CONFIG,
        expected: expected as ManagedCloudProvisioningExpectedIssuance })).toThrow("invalid_configuration");
    }
    expect(() => new ManagedCloudProvisioningRuntimeApiClient({ config: { ...CONFIG, grantToken: "A".repeat(42) + "B" },
      expected: EXPECTED })).toThrow("invalid_configuration");
  });
  it("compares worker queue observations structurally and refuses each valid nested drift", async () => {
    const config = managedCloudRuntimeConfig("workflow_worker", ENV,
      { measurementPath: MEASUREMENT_PATH, rootPath: ROOT })!;
    const expected = { ...EXPECTED, role: "workflow_worker" as const, runtimeIdentitySha256: config.runtimeIdentitySha256 };
    const dependency = deriveManagedCloudProvisioningDependencyEvidenceSha256({ role: expected.role,
      authorityBinding: expected.authorityBinding, activationSha256: expected.activationSha256,
      manifestSha256: expected.manifestSha256, componentId: expected.componentId, artifactSha256: expected.artifactSha256,
      taskQueueSha256: expected.taskQueueSha256, failureConverterSha256: expected.failureConverterSha256 });
    const observation: ManagedCloudProvisioningRuntimeObservation = { ...OBSERVATION,
      dependencyProbe: { version: 1, kind: "workflow_worker", workerState: "running", temporalNamespace: null },
      queueObservation: { pollerIdentitySha256: "a".repeat(64), backlogCount: 0, deliveredTaskCount: 0 } };
    for (const field of [null, "pollerIdentitySha256", "backlogCount", "deliveredTaskCount"] as const) {
      const api = new ManagedCloudProvisioningRuntimeApiClient({ config, expected, fetch: async (url, init) => {
        if (url.endsWith("/claim")) return response({ ...CLAIM, ...expected, dependencyEvidenceSha256: dependency });
        const receipt = heartbeat(String(init.body));
        return response(field === null ? receipt : { ...receipt, queueObservation: { ...observation.queueObservation,
          [field]: field === "pollerIdentitySha256" ? "b".repeat(64) : 1 } });
      } });
      await api.claim();
      if (field === null) expect((await api.heartbeat(1, observation)).queueObservation).toEqual(observation.queueObservation);
      else await expect(api.heartbeat(1, observation)).rejects.toThrow("binding_mismatch");
    }
  });
  it("remeasures actual files before the first heartbeat, not just before claim", async () => {
    let calls = 0;
    const api = client(async () => { calls++; return response(CLAIM); });
    await api.claim();
    writeFileSync(join(ROOT, "app/workflows/dist/gateway.js"), "tampered before first heartbeat");
    try { await expect(api.heartbeat(1, OBSERVATION)).rejects.toThrow("measurement_changed"); expect(calls).toBe(1); }
    finally { writeFileSync(join(ROOT, "app/workflows/dist/gateway.js"), FILES["app/workflows/dist/gateway.js"]); }
  });
});
