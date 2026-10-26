import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import {
  ManagedCloudRuntimeApiClient,
  managedCloudDependencyEvidenceSha256,
  managedCloudFailureConverterSha256,
  managedCloudOriginalSourceVerifierStartupRequestId,
  managedCloudTaskQueueSha256,
  runManagedCloudRuntimeReporter,
  type ManagedCloudRuntimeApi,
  type ManagedCloudRuntimeFetch,
} from "../src/managed-cloud-runtime-client.js";
import { managedCloudRuntimeConfig } from "../src/managed-cloud-runtime.js";

const DIGESTS = Array.from({ length: 11 }, (_, index) =>
  (index + 1).toString(16).repeat(64),
);

const ENV: NodeJS.ProcessEnv = {
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED: "true",
  BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN: "https://jobs-api.internal",
  BLUEY_JOBS_WORKER_SIGNING_KEY: "worker-signing-key-0123456789abcdef",
  BLUEY_JOBS_MANAGED_CLOUD_WORKER_ID: "workflow-gateway-runtime",
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_ID: "cloud-runtime-grant-1234567890",
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_TOKEN: "A".repeat(43),
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_INSTANCE_ID:
    "cloud-runtime-instance-1234567890",
  BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT: "staging",
  BLUEY_JOBS_MANAGED_CLOUD_REGION: "us-east-1",
  BLUEY_JOBS_MANAGED_CLOUD_CHANNEL: "shadow",
};

const MEASUREMENT_ROOT = mkdtempSync(join(tmpdir(), "bluey-runtime-client-"));
const MEASURED_PATH = "app/workflows/dist/gateway.js";
const MEASURED_BYTES = Buffer.from("gateway-runtime\n");
const NODE_PATH = "usr/local/bin/node";
const NODE_BYTES = Buffer.from("node-runtime\n");
mkdirSync(join(MEASUREMENT_ROOT, "app/.bluey"), { recursive: true });
mkdirSync(join(MEASUREMENT_ROOT, "app/automation"), { recursive: true });
mkdirSync(join(MEASUREMENT_ROOT, "app/workflows/dist"), { recursive: true });
mkdirSync(join(MEASUREMENT_ROOT, "usr/local/bin"), { recursive: true });
writeFileSync(join(MEASUREMENT_ROOT, MEASURED_PATH), MEASURED_BYTES);
writeFileSync(join(MEASUREMENT_ROOT, NODE_PATH), NODE_BYTES);
const MEASUREMENT_PATH = join(
  MEASUREMENT_ROOT,
  "app/.bluey/managed-cloud-runtime-measurement.json",
);
writeFileSync(
  MEASUREMENT_PATH,
  JSON.stringify({
    audience: "bluey-jobs-managed-cloud-runtime-measurement-v1",
    buildId: "managed-cloud-611-jobs-workflows",
    componentId: "jobs-workflows",
    configSchemaSha256: DIGESTS[5],
    measuredFiles: [
      {
        path: MEASURED_PATH,
        sha256: createHash("sha256").update(MEASURED_BYTES).digest("hex"),
      },
      {
        path: NODE_PATH,
        sha256: createHash("sha256").update(NODE_BYTES).digest("hex"),
      },
    ],
    migrationSetSha256: DIGESTS[6],
    protocolSetSha256: DIGESTS[7],
    roles: ["original_source_verifier", "workflow_gateway", "workflow_worker"],
    sourceCommit: "a".repeat(40),
    version: 1,
  }) + "\n",
);
const CONFIG = managedCloudRuntimeConfig("workflow_gateway", ENV, {
  measurementPath: MEASUREMENT_PATH,
  rootPath: MEASUREMENT_ROOT,
})!;
const STARTUP_CONFIG = managedCloudRuntimeConfig(
  "original_source_verifier",
  ENV,
  {
    measurementPath: MEASUREMENT_PATH,
    rootPath: MEASUREMENT_ROOT,
  },
)!;
afterAll(() => rmSync(MEASUREMENT_ROOT, { force: true, recursive: true }));

const INSTANCE = {
  grantId: "cloud-runtime-grant-1234567890",
  runtimeInstanceId: "cloud-runtime-instance-1234567890",
  runtimeIdentitySha256: CONFIG.runtimeIdentitySha256,
  workerId: "workflow-gateway-runtime",
  scope: { environment: "staging", region: "us-east-1", channel: "shadow" },
  activationSha256: DIGESTS[1],
  activationExpiresAtMs: 1_800_000_000_000,
  manifestSha256: DIGESTS[2],
  componentId: "jobs-workflows",
  role: "workflow_gateway",
  headRevision: 7,
  transitionSha256: DIGESTS[3],
  artifactSha256: DIGESTS[4],
  configSchemaSha256: DIGESTS[5],
  migrationSetSha256: DIGESTS[6],
  protocolSetSha256: DIGESTS[7],
  taskQueueSha256: DIGESTS[8],
  failureConverterSha256: DIGESTS[9],
  dependencyEvidenceSha256: "",
  instanceEpoch: 3,
  nextHeartbeatSequence: 1,
  claimedAtMs: 1_750_000_000_000,
  replayed: false,
};
INSTANCE.dependencyEvidenceSha256 = managedCloudDependencyEvidenceSha256(
  INSTANCE as never,
);
const STARTUP_INSTANCE = {
  ...INSTANCE,
  runtimeIdentitySha256: STARTUP_CONFIG.runtimeIdentitySha256,
  role: "original_source_verifier",
};
STARTUP_INSTANCE.dependencyEvidenceSha256 =
  managedCloudDependencyEvidenceSha256(STARTUP_INSTANCE as never);

describe("managed-cloud runtime API client", () => {
  it("derives exact task-queue and raw converter identities", () => {
    expect(
      managedCloudTaskQueueSha256("bluey-prod", "bluey-jobs-applications"),
    ).toBe("1551b746f88eded4598f4e0817382253d97bd1d8aa8bcc74b4f0339bd118cfb0");
    expect(
      managedCloudFailureConverterSha256(
        new TextEncoder().encode("converter\n"),
      ),
    ).toBe("4b57c07fe3edb9cb6068615fb27d286931d8d6d547c0e4de3426fcc81ed17aa0");
    expect(() => managedCloudTaskQueueSha256(" bluey-prod", "queue")).toThrow(
      "namespace is invalid",
    );
    expect(() =>
      managedCloudTaskQueueSha256("bluey-prod", "q".repeat(241)),
    ).toThrow("task queue is invalid");
    expect(() =>
      managedCloudTaskQueueSha256("bluey-prod", "é".repeat(121)),
    ).toThrow("task queue is invalid");
    expect(() =>
      managedCloudTaskQueueSha256("bluey\u0085prod", "queue"),
    ).toThrow("namespace is invalid");
    expect(() =>
      managedCloudTaskQueueSha256("bluey-prod", "bad\ufffdqueue"),
    ).toThrow("task queue is invalid");
    expect(() =>
      managedCloudTaskQueueSha256("bluey-prod", "bad\ud800queue"),
    ).toThrow("task queue is invalid");
    expect(managedCloudDependencyEvidenceSha256(INSTANCE as never)).toMatch(
      /^[0-9a-f]{64}$/,
    );
  });

  it("claims a DB-fenced instance and heartbeats the exact release identity", async () => {
    const requests: Array<{
      url: string;
      init: RequestInit;
      body: Record<string, unknown>;
    }> = [];
    const fetcher: ManagedCloudRuntimeFetch = async (url, init = {}) => {
      const body = JSON.parse(String(init.body)) as Record<string, unknown>;
      requests.push({ url: String(url), init, body });
      if (requests.length === 1) return authoritativeResponse(INSTANCE);
      const { sessionToken: _, ...echo } = body;
      return authoritativeResponse({
        ...echo,
        instanceEpoch: 3,
        heartbeatAtMs: 1_750_000_001_000,
        replayed: false,
      });
    };
    const client = new ManagedCloudRuntimeApiClient(CONFIG, { fetch: fetcher });
    const instance = await client.claim();
    const heartbeat = await client.heartbeat(instance, 1, {
      taskQueueSha256: DIGESTS[8],
      failureConverterSha256: DIGESTS[9],
      dependencyEvidenceSha256: INSTANCE.dependencyEvidenceSha256,
      healthState: "ready",
      reasonCode: null,
    });

    expect(instance.instanceEpoch).toBe(3);
    expect(heartbeat.heartbeatSequence).toBe(1);
    expect(requests[0]?.body).toMatchObject({
      grantId: "cloud-runtime-grant-1234567890",
      workerId: "workflow-gateway-runtime",
      runtimeInstanceId: "cloud-runtime-instance-1234567890",
    });
    const headers = new Headers(requests[0]?.init.headers);
    expect(headers.get("x-bluey-jobs-worker-origin")).toBe(
      "https://jobs-api.internal",
    );
    expect(headers.get("x-bluey-jobs-worker-scope")).toBe(
      "managed-cloud-runtime",
    );
  });

  it("starts the original-source verifier with one exact bound request", async () => {
    const requests: Array<{
      url: string;
      init: RequestInit;
      body: Record<string, unknown>;
    }> = [];
    const fetcher: ManagedCloudRuntimeFetch = async (url, init = {}) => {
      const body = JSON.parse(String(init.body)) as Record<string, unknown>;
      requests.push({ url: String(url), init, body });
      const {
        expectedDegradedHeartbeatSequence: _expected,
        sessionToken: _session,
        ...bindings
      } = body;
      return authoritativeResponse({
        ...bindings,
        degradedHeartbeatSequence: 1,
        readyHeartbeatSequence: 2,
        probeResult: "leaseable",
        startupAuthoritySha256: DIGESTS[10],
        startupAtMs: 1_750_000_002_000,
      });
    };
    const client = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      fetch: fetcher,
    });
    const startup = await client.originalSourceVerifierStartup(
      STARTUP_INSTANCE as never,
      1,
    );

    expect(requests).toHaveLength(1);
    expect(requests[0]?.url).toBe(
      "https://jobs-api.internal/api/jobs/internal/managed-cloud/runtime-instances/" +
        "cloud-runtime-instance-1234567890/original-source-verifier-startup",
    );
    expect(requests[0]?.body).toEqual({
      version: 1,
      audience: "bluey-jobs-managed-cloud-original-source-verifier-startup-v1",
      startupRequestId: managedCloudOriginalSourceVerifierStartupRequestId(
        STARTUP_INSTANCE as never,
        1,
      ),
      runtimeInstanceId: STARTUP_INSTANCE.runtimeInstanceId,
      workerId: STARTUP_INSTANCE.workerId,
      sessionToken: STARTUP_CONFIG.sessionToken,
      instanceEpoch: STARTUP_INSTANCE.instanceEpoch,
      expectedDegradedHeartbeatSequence: 1,
      runtimeIdentitySha256: STARTUP_INSTANCE.runtimeIdentitySha256,
      observedHeadRevision: STARTUP_INSTANCE.headRevision,
      observedTransitionSha256: STARTUP_INSTANCE.transitionSha256,
      activationSha256: STARTUP_INSTANCE.activationSha256,
      manifestSha256: STARTUP_INSTANCE.manifestSha256,
      componentId: STARTUP_INSTANCE.componentId,
      role: "original_source_verifier",
      artifactSha256: STARTUP_INSTANCE.artifactSha256,
      migrationSetSha256: STARTUP_INSTANCE.migrationSetSha256,
      configSchemaSha256: STARTUP_INSTANCE.configSchemaSha256,
      protocolSetSha256: STARTUP_INSTANCE.protocolSetSha256,
      taskQueueSha256: STARTUP_INSTANCE.taskQueueSha256,
      failureConverterSha256: STARTUP_INSTANCE.failureConverterSha256,
      dependencyEvidenceSha256: STARTUP_INSTANCE.dependencyEvidenceSha256,
    });
    expect(startup).toMatchObject({
      degradedHeartbeatSequence: 1,
      readyHeartbeatSequence: 2,
      probeResult: "leaseable",
      startupAuthoritySha256: DIGESTS[10],
    });
    expect(startup).not.toHaveProperty("sessionToken");
    expect(startup).not.toHaveProperty("replayed");
    expect(
      managedCloudOriginalSourceVerifierStartupRequestId(
        STARTUP_INSTANCE as never,
        1,
      ),
    ).toBe(
      "osv-startup-2d3bef522abee153ccb71b0784642b58f2b244448828ee4af4ae7b6b1759dbc6",
    );
    expect(
      managedCloudOriginalSourceVerifierStartupRequestId(
        STARTUP_INSTANCE as never,
        2,
      ),
    ).not.toBe(startup.startupRequestId);
    const headers = new Headers(requests[0]?.init.headers);
    expect(headers.get("x-bluey-jobs-worker-scope")).toBe(
      "managed-cloud-runtime",
    );
  });

  it("retries boot registration and startup with byte-identical bodies after response loss", async () => {
    const bodies: Record<string, unknown>[] = [];
    let registrationAttempts = 0;
    let startupAttempts = 0;
    const client = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      fetch: async (_url, init) => {
        const body = JSON.parse(String(init?.body)) as Record<string, unknown>;
        bodies.push(body);
        if ("registrationRequestId" in body) {
          registrationAttempts += 1;
          if (registrationAttempts === 1) throw new Error("response lost");
          const { sessionToken: _, ...response } = body;
          return authoritativeResponse({
            ...response,
            bootGeneration: "7",
            instanceEpoch: "3",
            protocolId: "source_verifier_boot",
            protocolVersion: "1",
            registeredAtMs: "1750000002000",
          });
        }
        startupAttempts += 1;
        if (startupAttempts === 1) throw new Error("response lost");
        const { sessionToken: _, ...response } = body;
        return authoritativeResponse({
          ...response,
          readyHeartbeatSequence: "2",
          probeResult: "idle",
          protocolId: "source_verifier_boot",
          protocolVersion: "1",
          startupAtMs: "1750000003000",
          bootGeneration: "7",
          instanceEpoch: "3",
          degradedHeartbeatSequence: "1",
        });
      },
    });

    const registration = await client.registerSourceVerifierBoot(
      STARTUP_INSTANCE as never,
    );
    const startup = await client.sourceVerifierBootStartup(
      STARTUP_INSTANCE as never,
      registration.bootGeneration,
      1,
    );

    expect(registrationAttempts).toBe(2);
    expect(startupAttempts).toBe(2);
    expect(JSON.stringify(bodies[0])).toBe(JSON.stringify(bodies[1]));
    expect(JSON.stringify(bodies[2])).toBe(JSON.stringify(bodies[3]));
    expect(bodies[0]).toMatchObject({
      sessionToken: STARTUP_CONFIG.sessionToken,
      bootId: expect.stringMatching(/^boot-/),
    });
    expect(bodies[2]).toMatchObject({
      sessionToken: STARTUP_CONFIG.sessionToken,
      bootId: bodies[0]?.bootId,
      bootGeneration: 7,
    });
    expect(startup).toMatchObject({
      bootGeneration: 7,
      degradedHeartbeatSequence: 1,
      readyHeartbeatSequence: 2,
      probeResult: "idle",
    });
  });

  it("rejects startup response extensions and changed sequence bindings", async () => {
    const responseFor = (
      body: Record<string, unknown>,
      changes: Record<string, unknown>,
    ): Response => {
      const {
        expectedDegradedHeartbeatSequence: _expected,
        sessionToken: _session,
        ...bindings
      } = body;
      return authoritativeResponse({
        ...bindings,
        degradedHeartbeatSequence: 1,
        readyHeartbeatSequence: 2,
        probeResult: "idle",
        startupAuthoritySha256: DIGESTS[10],
        startupAtMs: 1_750_000_002_000,
        ...changes,
      });
    };
    const replayExtension = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      fetch: async (_url, init) =>
        responseFor(JSON.parse(String(init?.body)) as Record<string, unknown>, {
          replayed: false,
        }),
    });
    await expect(
      replayExtension.originalSourceVerifierStartup(
        STARTUP_INSTANCE as never,
        1,
      ),
    ).rejects.toThrow("response shape is invalid");

    const requestOnlyExtension = new ManagedCloudRuntimeApiClient(
      STARTUP_CONFIG,
      {
        fetch: async (_url, init) =>
          responseFor(
            JSON.parse(String(init?.body)) as Record<string, unknown>,
            { expectedDegradedHeartbeatSequence: 1 },
          ),
      },
    );
    await expect(
      requestOnlyExtension.originalSourceVerifierStartup(
        STARTUP_INSTANCE as never,
        1,
      ),
    ).rejects.toThrow("response shape is invalid");

    const changedSequence = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      fetch: async (_url, init) =>
        responseFor(JSON.parse(String(init?.body)) as Record<string, unknown>, {
          readyHeartbeatSequence: 3,
        }),
    });
    await expect(
      changedSequence.originalSourceVerifierStartup(
        STARTUP_INSTANCE as never,
        1,
      ),
    ).rejects.toThrow("startup sequence is inconsistent");

    const changedIdentity = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      fetch: async (_url, init) =>
        responseFor(JSON.parse(String(init?.body)) as Record<string, unknown>, {
          manifestSha256: DIGESTS[0],
        }),
    });
    await expect(
      changedIdentity.originalSourceVerifierStartup(
        STARTUP_INSTANCE as never,
        1,
      ),
    ).rejects.toThrow("startup identity is inconsistent");

    const changedProbe = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      fetch: async (_url, init) =>
        responseFor(JSON.parse(String(init?.body)) as Record<string, unknown>, {
          probeResult: "unknown",
        }),
    });
    await expect(
      changedProbe.originalSourceVerifierStartup(STARTUP_INSTANCE as never, 1),
    ).rejects.toThrow("startup probe is invalid");
  });

  it("times out startup once without an implicit retry", async () => {
    let requests = 0;
    const client = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      timeoutMs: 100,
      fetch: async (_url, init) => {
        requests += 1;
        return new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener(
            "abort",
            () => reject(new Error("aborted")),
            { once: true },
          );
        });
      },
    });
    await expect(
      client.originalSourceVerifierStartup(STARTUP_INSTANCE as never, 1),
    ).rejects.toThrow("API timed out");
    expect(requests).toBe(1);
  });

  it("rejects unbound response headers and swapped release identity", async () => {
    const missingHeaders = new ManagedCloudRuntimeApiClient(CONFIG, {
      fetch: async () =>
        new Response(JSON.stringify(INSTANCE), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    });
    await expect(missingHeaders.claim()).rejects.toThrow("not authoritative");

    const swapped = new ManagedCloudRuntimeApiClient(CONFIG, {
      fetch: async () =>
        authoritativeResponse({ ...INSTANCE, role: "workflow_worker" }),
    });
    await expect(swapped.claim()).rejects.toThrow("identity is inconsistent");

    const swappedContract = new ManagedCloudRuntimeApiClient(CONFIG, {
      fetch: async () =>
        authoritativeResponse({
          ...INSTANCE,
          configSchemaSha256: DIGESTS[0],
        }),
    });
    await expect(swappedContract.claim()).rejects.toThrow(
      "identity is inconsistent",
    );
  });

  it("rehashes the runtime before the first heartbeat", async () => {
    let requests = 0;
    const client = new ManagedCloudRuntimeApiClient(CONFIG, {
      fetch: async () => {
        requests += 1;
        return authoritativeResponse(INSTANCE);
      },
    });
    const instance = await client.claim();
    writeFileSync(
      join(MEASUREMENT_ROOT, MEASURED_PATH),
      "tampered-after-claim\n",
    );
    await expect(
      client.heartbeat(instance, instance.nextHeartbeatSequence, {
        taskQueueSha256: DIGESTS[8],
        failureConverterSha256: DIGESTS[9],
        dependencyEvidenceSha256: INSTANCE.dependencyEvidenceSha256,
        healthState: "ready",
        reasonCode: null,
      }),
    ).rejects.toThrow("measured file digest is invalid");
    expect(requests).toBe(1);
    writeFileSync(join(MEASUREMENT_ROOT, MEASURED_PATH), MEASURED_BYTES);
  });

  it("rehashes the runtime again before the startup readiness transition", async () => {
    let requests = 0;
    const client = new ManagedCloudRuntimeApiClient(STARTUP_CONFIG, {
      fetch: async () => {
        requests += 1;
        return authoritativeResponse({});
      },
    });
    writeFileSync(
      join(MEASUREMENT_ROOT, MEASURED_PATH),
      "tampered-before-startup\n",
    );
    await expect(
      client.originalSourceVerifierStartup(STARTUP_INSTANCE as never, 1),
    ).rejects.toThrow("measured file digest is invalid");
    expect(requests).toBe(0);
    writeFileSync(join(MEASUREMENT_ROOT, MEASURED_PATH), MEASURED_BYTES);
  });

  it("retries response loss with the same DB sequence and advances only after receipt", async () => {
    const sequences: number[] = [];
    const controller = new AbortController();
    let attempts = 0;
    const api: ManagedCloudRuntimeApi = {
      claim: async () => INSTANCE as never,
      heartbeat: async (_instance, sequence) => {
        sequences.push(sequence);
        attempts += 1;
        if (attempts === 1) throw new Error("response lost");
        controller.abort();
        return {} as never;
      },
    };
    await runManagedCloudRuntimeReporter(
      api,
      async () => ({
        taskQueueSha256: DIGESTS[8],
        failureConverterSha256: DIGESTS[9],
        dependencyEvidenceSha256: INSTANCE.dependencyEvidenceSha256,
        healthState: "ready",
        reasonCode: null,
      }),
      5_000,
      controller.signal,
      { retryDelayMs: 0, sleep: async () => undefined },
    );
    expect(sequences).toEqual([1, 1]);
  });

  it("never announces readiness from a degraded or replayed heartbeat", async () => {
    const controller = new AbortController();
    const changes: boolean[] = [];
    const ready: string[] = [];
    let sequence = 0;
    const api: ManagedCloudRuntimeApi = {
      claim: async () => INSTANCE as never,
      heartbeat: async () => {
        sequence += 1;
        if (sequence === 2) controller.abort();
        return {
          healthState: sequence === 1 ? "degraded" : "ready",
          replayed: sequence === 2,
        } as never;
      },
    };
    await runManagedCloudRuntimeReporter(
      api,
      async () => ({
        taskQueueSha256: DIGESTS[8],
        failureConverterSha256: DIGESTS[9],
        dependencyEvidenceSha256: INSTANCE.dependencyEvidenceSha256,
        healthState: "ready",
        reasonCode: null,
      }),
      1,
      controller.signal,
      {
        sleep: async () => undefined,
        onReady: () => ready.push("ready"),
        onReadinessChanged: (value) => changes.push(value),
      },
    );
    expect(changes).toEqual([false, false]);
    expect(ready).toEqual([]);
  });
});

function authoritativeResponse(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: {
      "content-type": "application/json",
      "cache-control": "no-store",
      "x-content-type-options": "nosniff",
    },
  });
}
