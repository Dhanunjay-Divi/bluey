import { createHash } from "node:crypto";

import {
  OriginalSourceVerifier,
  type OriginalSourceVerificationObservation,
  type OriginalSourceVerificationSubject,
} from "@bluey/jobs-automation/original-source-verification";
import {
  runManagedCloudRuntimeHeartbeats,
  type ManagedCloudOriginalSourceVerifierStartupApi,
  type ManagedCloudRuntimeHeartbeat,
  type ManagedCloudRuntimeInstance,
  type ManagedCloudRuntimeObservation,
} from "@bluey/jobs-automation/managed-cloud-runtime-client";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  OriginalSourceVerificationApiClient,
  type OriginalSourceVerificationApiFetch,
  type OriginalSourceVerificationHeartbeat,
  type OriginalSourceVerificationLease,
  type OriginalSourceVerificationTerminal,
  type OriginalSourceVerificationWorkerApi,
  type OriginalSourceVerifierBinding,
} from "../src/original-source-verification-api.js";
import {
  OriginalSourceVerifierRuntime,
  originalSourceVerifierBinding,
  terminalRequestId,
} from "../src/original-source-verification-runtime.js";
import {
  establishOriginalSourceVerifierStartup,
  runOriginalSourceVerifierFromEnvironment,
  runOriginalSourceVerifierLifecycle,
} from "../src/original-source-verifier.js";

const SIGNING_KEY = "original-source-signing-key-0123456789abcdef";
const SUBJECT = originalSourceSubject();
const CANONICAL_SUBJECT = JSON.stringify(SUBJECT);
const SUBJECT_SHA256 = createHash("sha256")
  .update(CANONICAL_SUBJECT)
  .digest("hex");
const RUNTIME_IDENTITY_SHA256 = "1".repeat(64);
const BINDING: OriginalSourceVerifierBinding = {
  worker_id: "original-source-verifier-test",
  runtime_instance_id: "original-source-runtime-instance-123",
  runtime_instance_epoch: 7,
  runtime_authority_sha256: "a".repeat(64),
  runtime_session_token: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE",
};
const MANAGED_INSTANCE: ManagedCloudRuntimeInstance = {
  grantId: "original-source-runtime-grant-123",
  runtimeInstanceId: BINDING.runtime_instance_id,
  runtimeIdentitySha256: RUNTIME_IDENTITY_SHA256,
  workerId: BINDING.worker_id,
  scope: { environment: "staging", region: "us-east-1", channel: "shadow" },
  activationSha256: "2".repeat(64),
  activationExpiresAtMs: 1_800_000_000_000,
  manifestSha256: "3".repeat(64),
  componentId: "jobs-workflows",
  role: "original_source_verifier",
  headRevision: 4,
  transitionSha256: "5".repeat(64),
  artifactSha256: "6".repeat(64),
  configSchemaSha256: "7".repeat(64),
  migrationSetSha256: "8".repeat(64),
  protocolSetSha256: "9".repeat(64),
  taskQueueSha256: "a".repeat(64),
  failureConverterSha256: "b".repeat(64),
  dependencyEvidenceSha256: "c".repeat(64),
  instanceEpoch: BINDING.runtime_instance_epoch,
  nextHeartbeatSequence: 11,
  claimedAtMs: 1_750_000_000_000,
  replayed: false,
};
const MANAGED_OBSERVATION: ManagedCloudRuntimeObservation = {
  taskQueueSha256: MANAGED_INSTANCE.taskQueueSha256,
  failureConverterSha256: MANAGED_INSTANCE.failureConverterSha256,
  dependencyEvidenceSha256: MANAGED_INSTANCE.dependencyEvidenceSha256,
  healthState: "ready",
  reasonCode: null,
};

afterEach(() => vi.unstubAllEnvs());

function originalSourceSubject(): OriginalSourceVerificationSubject {
  return {
    schema_version: 1,
    canonical_job_id: "canonical-job-acme-platform",
    employer_id: "employer-acme",
    original_url: "https://boards.greenhouse.io/acme/jobs/job-1",
    provider_family: "greenhouse",
    provider_record_id: "greenhouse:acme:job-1",
    provider_target: {
      host: "boards.greenhouse.io",
      tenant: "acme",
      job: "job-1",
      variant: "greenhouse_public",
    },
    expected: {
      availability_status: "active",
      company: "Acme",
      compensation: "",
      description: "Build reliable systems.",
      employment_type: "full_time",
      location: "Austin, TX",
      posted_at_ms: Date.parse("2026-08-25T12:00:00Z"),
      title: "Platform Engineer",
      workplace: "hybrid",
    },
  };
}

function lease(): OriginalSourceVerificationLease {
  return {
    assignment_id: "source-verification-assignment-123",
    subject_sha256: SUBJECT_SHA256,
    canonical_subject_json: CANONICAL_SUBJECT,
    attempt_id: "source-verification-attempt-12345",
    fence: 3,
    lease_token: "lease-token-original-source-test",
    lease_expires_at_ms: 1_800_000_060_000,
    hard_deadline_at_ms: 1_800_000_600_000,
    heartbeat_sequence: 0,
  };
}

function closedObservation(): OriginalSourceVerificationObservation & {
  worker_runtime_identity_sha256: string;
} {
  return {
    assurance: "original_verified",
    result: "closed",
    evidence_sha256: "c".repeat(64),
    error_code: null,
    requested_url:
      "https://boards-api.greenhouse.io/v1/boards/acme/jobs/job-1?content=true",
    canonical_observed_url: null,
    canonical_application_url: null,
    application_domain: null,
    retrieval_status: "not_found",
    http_status: 404,
    http_semantics_digest: "d".repeat(64),
    redirect_chain_digest: "e".repeat(64),
    headers_digest: "f".repeat(64),
    content_digest: "0".repeat(64),
    parser_version: "greenhouse.original_source.v1",
    parser_digest: "9".repeat(64),
    worker_runtime_identity_sha256: RUNTIME_IDENTITY_SHA256,
    provider_record_id: null,
    company: null,
    title: null,
    location: null,
    workplace: null,
    description: null,
    compensation: null,
    employment_type: null,
    posted_at_ms: null,
    mismatched_fields: [],
  };
}

function heartbeat(sequence: number): OriginalSourceVerificationHeartbeat {
  return {
    lease_expires_at_ms: 1_800_000_060_000,
    hard_deadline_at_ms: 1_800_000_600_000,
    heartbeat_sequence: sequence,
    replayed: false,
  };
}

function managedHeartbeat(
  sequence: number,
  healthState: "ready" | "degraded",
  reasonCode: "startup" | "probe_failed" | null,
  replayed = false,
): ManagedCloudRuntimeHeartbeat {
  return {
    runtimeInstanceId: MANAGED_INSTANCE.runtimeInstanceId,
    workerId: MANAGED_INSTANCE.workerId,
    instanceEpoch: MANAGED_INSTANCE.instanceEpoch,
    heartbeatSequence: sequence,
    observedHeadRevision: MANAGED_INSTANCE.headRevision,
    observedTransitionSha256: MANAGED_INSTANCE.transitionSha256,
    activationSha256: MANAGED_INSTANCE.activationSha256,
    manifestSha256: MANAGED_INSTANCE.manifestSha256,
    componentId: MANAGED_INSTANCE.componentId,
    role: "original_source_verifier" as const,
    artifactSha256: MANAGED_INSTANCE.artifactSha256,
    migrationSetSha256: MANAGED_INSTANCE.migrationSetSha256,
    configSchemaSha256: MANAGED_INSTANCE.configSchemaSha256,
    protocolSetSha256: MANAGED_INSTANCE.protocolSetSha256,
    taskQueueSha256: MANAGED_INSTANCE.taskQueueSha256,
    failureConverterSha256: MANAGED_INSTANCE.failureConverterSha256,
    dependencyEvidenceSha256: MANAGED_INSTANCE.dependencyEvidenceSha256,
    healthState,
    reasonCode,
    heartbeatAtMs: 1_750_000_001_000 + sequence,
    replayed,
  };
}

function managedStartup() {
  return {
    startupRequestId: "startboot-" + "d".repeat(32),
    bootId: "boot-" + "e".repeat(32),
    runtimeInstanceId: MANAGED_INSTANCE.runtimeInstanceId,
    workerId: MANAGED_INSTANCE.workerId,
    instanceEpoch: MANAGED_INSTANCE.instanceEpoch,
    bootGeneration: 7,
    degradedHeartbeatSequence: MANAGED_INSTANCE.nextHeartbeatSequence,
    readyHeartbeatSequence: MANAGED_INSTANCE.nextHeartbeatSequence + 1,
    probeResult: "idle" as const,
    protocolId: "source_verifier_boot" as const,
    protocolVersion: 1 as const,
    startupAuthoritySha256: "e".repeat(64),
    startupAtMs: 1_750_000_002_000,
  };
}

function bootRegistration() {
  return {
    registrationRequestId: "regboot-" + "d".repeat(32),
    bootId: "boot-" + "e".repeat(32),
    runtimeInstanceId: MANAGED_INSTANCE.runtimeInstanceId,
    instanceEpoch: MANAGED_INSTANCE.instanceEpoch,
    workerId: MANAGED_INSTANCE.workerId,
    runtimeIdentitySha256: MANAGED_INSTANCE.runtimeIdentitySha256,
    activationSha256: MANAGED_INSTANCE.activationSha256,
    manifestSha256: MANAGED_INSTANCE.manifestSha256,
    componentId: MANAGED_INSTANCE.componentId,
    role: "original_source_verifier" as const,
    bootGeneration: 7,
    protocolId: "source_verifier_boot" as const,
    protocolVersion: 1 as const,
    registeredAtMs: 1_750_000_002_000,
  };
}

function terminal(replayed = false): OriginalSourceVerificationTerminal {
  return {
    assignment_id: lease().assignment_id,
    state: "idle",
    replayed,
    receipt_sha256: "b".repeat(64),
    head: null,
  };
}

function authoritativeResponse(value: unknown, status = 200): Response {
  return new Response(status === 204 ? null : JSON.stringify(value), {
    status,
    headers:
      status === 204
        ? {}
        : {
            "content-type": "application/json",
            "cache-control": "no-store",
            "x-content-type-options": "nosniff",
          },
  });
}

describe("original-source verification API", () => {
  it("sends an exact nested runtime binding and signed verifier scope", async () => {
    const requests: Array<{
      url: string;
      init: RequestInit;
      body: Record<string, unknown>;
    }> = [];
    const fetcher: OriginalSourceVerificationApiFetch = async (
      input,
      init = {},
    ) => {
      const body = JSON.parse(String(init.body)) as Record<string, unknown>;
      requests.push({ url: String(input), init, body });
      if (requests.length === 1) return authoritativeResponse(lease());
      if (requests.length === 2) return authoritativeResponse(heartbeat(1));
      return authoritativeResponse(terminal());
    };
    const client = new OriginalSourceVerificationApiClient({
      origin: "https://jobs.internal",
      signingKey: SIGNING_KEY,
      binding: BINDING,
      fetch: fetcher,
    });
    const claimed = await client.lease();
    await client.heartbeat(claimed!, 1);
    await client.fail(
      claimed!,
      terminalRequestId(claimed!),
      "provider_unavailable",
      {
        ...closedObservation(),
        result: "indeterminate",
        error_code: "provider_unavailable",
      },
    );

    expect(requests[0]?.body).toEqual({ binding: BINDING });
    expect(requests[1]?.body).toMatchObject({
      binding: BINDING,
      assignment_id: claimed?.assignment_id,
      attempt_id: claimed?.attempt_id,
      fence: 3,
      heartbeat_sequence: 1,
    });
    expect(requests[2]?.body).toMatchObject({
      binding: BINDING,
      request_id: terminalRequestId(claimed!),
      error_code: "provider_unavailable",
    });
    const headers = new Headers(requests[0]?.init.headers);
    expect(headers.get("x-bluey-jobs-worker-scope")).toBe(
      "original-source-verification",
    );
    expect(headers.get("x-bluey-jobs-worker-id")).toBe(BINDING.worker_id);
    expect(requests.every((request) => request.init.redirect === "error")).toBe(
      true,
    );
  });

  it("propagates one validated boot identity while preserving legacy omission", async () => {
    const bodies: Record<string, unknown>[] = [];
    const fetcher: OriginalSourceVerificationApiFetch = async (_input, init = {}) => {
      bodies.push(JSON.parse(String(init.body)) as Record<string, unknown>);
      if (bodies.length === 1) return authoritativeResponse(lease());
      if (bodies.length === 2) return authoritativeResponse(heartbeat(1));
      return authoritativeResponse(terminal());
    };
    const client = new OriginalSourceVerificationApiClient({
      origin: "https://jobs.internal",
      signingKey: SIGNING_KEY,
      binding: {
        ...BINDING,
        boot_id: "boot-" + "d".repeat(32),
        boot_generation: 7,
      },
      fetch: fetcher,
    });
    const claimed = await client.lease();
    await client.heartbeat(claimed!, 1);
    await client.complete(claimed!, terminalRequestId(claimed!), closedObservation());
    const bootBinding = {
        ...BINDING,
        boot_id: "boot-" + "d".repeat(32),
        boot_generation: 7,
    };
    expect(bodies.map((body) => body.binding)).toEqual([bootBinding, bootBinding, bootBinding]);
    expect(BINDING).not.toHaveProperty("boot_id");
    expect(BINDING).not.toHaveProperty("boot_generation");
  });

  it("retries terminal response loss with byte-identical request identity", async () => {
    const bodies: string[] = [];
    let calls = 0;
    const client = new OriginalSourceVerificationApiClient({
      origin: "https://jobs.internal",
      signingKey: SIGNING_KEY,
      binding: BINDING,
      reportRetryMs: 0,
      sleep: async () => undefined,
      fetch: async (_input, init) => {
        calls += 1;
        bodies.push(String(init?.body));
        if (calls === 1) throw new Error("response lost");
        return authoritativeResponse(terminal(true));
      },
    });
    const observation = closedObservation();

    await expect(
      client.complete(lease(), terminalRequestId(lease()), observation),
    ).resolves.toMatchObject({ replayed: true });
    expect(bodies).toHaveLength(2);
    expect(bodies[0]).toBe(bodies[1]);
    expect(JSON.parse(bodies[0]!)).toMatchObject({
      binding: BINDING,
      assignment_id: lease().assignment_id,
      observation: { provider_record_id: null },
    });
  });

  it("rejects unbound lease bytes and non-authoritative response headers", async () => {
    const changed = {
      ...lease(),
      canonical_subject_json: CANONICAL_SUBJECT.replace(
        "Platform Engineer",
        "Data Engineer",
      ),
    };
    const digestMismatch = new OriginalSourceVerificationApiClient({
      origin: "https://jobs.internal",
      signingKey: SIGNING_KEY,
      binding: BINDING,
      fetch: async () => authoritativeResponse(changed),
    });
    await expect(digestMismatch.lease()).rejects.toThrow(/digest/);

    const missingHeaders = new OriginalSourceVerificationApiClient({
      origin: "https://jobs.internal",
      signingKey: SIGNING_KEY,
      binding: BINDING,
      fetch: async () =>
        new Response(JSON.stringify(lease()), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    });
    await expect(missingHeaders.lease()).rejects.toThrow(/authoritative/);
  });
});

describe("original-source verifier runtime", () => {
  it("heartbeats before provider I/O and immediately before fenced completion", async () => {
    const order: string[] = [];
    const api: OriginalSourceVerificationWorkerApi = {
      lease: async () => lease(),
      heartbeat: async (_lease, sequence) => {
        order.push(`heartbeat:${sequence}`);
        return heartbeat(sequence);
      },
      complete: async (_lease, _requestId, observation) => {
        order.push(`complete:${observation.result}`);
        return terminal();
      },
      fail: async () => {
        throw new Error("unexpected failure");
      },
    };
    const verifier = new OriginalSourceVerifier({
      fetch: async () => {
        order.push("provider-fetch");
        return new Response(
          JSON.stringify({
            id: "job-1",
            absolute_url: "https://boards.greenhouse.io/acme/jobs/job-1",
            title: "Platform Engineer",
            location: { name: "Austin, TX" },
            workplace_type: "hybrid",
            content: "Build reliable systems.",
            employment_type: "full_time",
            updated_at: "2026-08-25T12:00:00Z",
          }),
          {
            status: 200,
            headers: new Headers({ "content-type": "application/json" }),
          },
        );
      },
    });
    const runtime = new OriginalSourceVerifierRuntime({
      api,
      verifier,
      workerRuntimeIdentitySha256: RUNTIME_IDENTITY_SHA256,
    });

    await expect(runtime.pollOnce()).resolves.toBe("completed");
    expect(order).toEqual([
      "heartbeat:1",
      "provider-fetch",
      "heartbeat:2",
      "complete:open",
    ]);
  });

  it("drops provider bytes when a lease heartbeat fails", async () => {
    const completed = vi.fn();
    let heartbeats = 0;
    const api: OriginalSourceVerificationWorkerApi = {
      lease: async () => lease(),
      heartbeat: async (_lease, sequence) => {
        heartbeats += 1;
        if (heartbeats > 1) throw new Error("lease revoked");
        return heartbeat(sequence);
      },
      complete: completed,
      fail: vi.fn(),
    };
    const verifier = {
      verify: async (_subject: unknown, signal?: AbortSignal) =>
        new Promise<{ kind: "fail"; error_code: "unreachable" }>((resolve) => {
          signal?.addEventListener(
            "abort",
            () => resolve({ kind: "fail", error_code: "unreachable" }),
            { once: true },
          );
        }),
    } as unknown as OriginalSourceVerifier;
    const runtime = new OriginalSourceVerifierRuntime({
      api,
      verifier,
      workerRuntimeIdentitySha256: RUNTIME_IDENTITY_SHA256,
      leaseHeartbeatIntervalMs: 100,
      sleep: async () => undefined,
    });

    await expect(runtime.pollOnce()).resolves.toBe("lease_lost");
    expect(completed).not.toHaveBeenCalled();
    expect(api.fail).not.toHaveBeenCalled();
  });

  it("converts an unexpected provider exception into one fenced failure", async () => {
    const failed = vi.fn(async () => terminal());
    const api: OriginalSourceVerificationWorkerApi = {
      lease: async () => lease(),
      heartbeat: async (_lease, sequence) => heartbeat(sequence),
      complete: vi.fn(),
      fail: failed,
    };
    const verifier = {
      verify: async () => {
        throw new Error("provider adapter failed unexpectedly");
      },
    } as unknown as OriginalSourceVerifier;
    const runtime = new OriginalSourceVerifierRuntime({
      api,
      verifier,
      workerRuntimeIdentitySha256: RUNTIME_IDENTITY_SHA256,
    });

    await expect(runtime.pollOnce()).resolves.toBe("failed");
    expect(failed).toHaveBeenCalledWith(
      expect.objectContaining({ assignment_id: lease().assignment_id }),
      terminalRequestId(lease()),
      "unreachable",
      expect.objectContaining({
        error_code: "unreachable",
        retrieval_status: "unreachable",
        worker_runtime_identity_sha256: RUNTIME_IDENTITY_SHA256,
      }),
    );
    expect(api.complete).not.toHaveBeenCalled();
  });

  it("derives one exact release/runtime authority binding", () => {
    const instance = {
      role: "original_source_verifier",
      componentId: "jobs-workflows",
      workerId: BINDING.worker_id,
      runtimeInstanceId: BINDING.runtime_instance_id,
      runtimeIdentitySha256: RUNTIME_IDENTITY_SHA256,
      activationSha256: "2".repeat(64),
      manifestSha256: "3".repeat(64),
      headRevision: 4,
      transitionSha256: "5".repeat(64),
      instanceEpoch: BINDING.runtime_instance_epoch,
    } as never;
    const binding = originalSourceVerifierBinding(
      instance,
      BINDING.runtime_session_token,
    );

    expect(binding).toMatchObject({
      worker_id: BINDING.worker_id,
      runtime_instance_id: BINDING.runtime_instance_id,
      runtime_instance_epoch: 7,
      runtime_session_token: BINDING.runtime_session_token,
    });
    expect(binding.runtime_authority_sha256).toBe(
      "34f63a889abc29ec7e865e2c398f63530cb1308ce1c43f9a13d36e493c2351ea",
    );
    const bootBinding = originalSourceVerifierBinding(
      { ...instance, bootId: "boot-" + "d".repeat(32), bootGeneration: 7 } as never,
      BINDING.runtime_session_token,
    );
    expect(bootBinding).toMatchObject({
      boot_id: "boot-" + "d".repeat(32),
      boot_generation: 7,
    });
    expect(() =>
      originalSourceVerifierBinding(
        { ...instance, role: "workflow_worker" } as never,
        BINDING.runtime_session_token,
      ),
    ).toThrow(/identity/);
  });


  it("registers the server-assigned boot generation before the degraded heartbeat", async () => {
    const order: string[] = [];
    const runtimeApi: ManagedCloudOriginalSourceVerifierStartupApi & {
      registerSourceVerifierBoot: (
        instance: ManagedCloudRuntimeInstance,
      ) => Promise<{ bootGeneration: number }>;
    } = {
      claim: async () => MANAGED_INSTANCE,
      registerSourceVerifierBoot: async () => {
        order.push("register-boot");
        return bootRegistration();
      },
      heartbeat: async (_instance, sequence, observation) => {
        order.push(
          `heartbeat:${sequence}:${observation.healthState}:${String(observation.reasonCode)}`,
        );
        return managedHeartbeat(
          sequence,
          observation.healthState === "ready" ? "ready" : "degraded",
          observation.reasonCode === "startup" ||
            observation.reasonCode === "probe_failed"
            ? observation.reasonCode
            : null,
        );
      },
      sourceVerifierBootStartup: async (_instance, generation, sequence) => {
        order.push(`startup:${generation}:${sequence}`);
        return managedStartup();
      },
    };
    const established = await establishOriginalSourceVerifierStartup(
      runtimeApi,
      MANAGED_INSTANCE,
      MANAGED_OBSERVATION,
    );
    expect(order[0]).toBe("register-boot");
    expect(order[1]).toBe(
      `heartbeat:${MANAGED_INSTANCE.nextHeartbeatSequence}:degraded:startup`,
    );
    expect(established.bootGeneration).toBe(7);
  });

  it("rejects an invalid server-assigned boot generation", async () => {
    const runtimeApi: ManagedCloudOriginalSourceVerifierStartupApi & {
      registerSourceVerifierBoot: (
        instance: ManagedCloudRuntimeInstance,
      ) => Promise<{ bootGeneration: number }>;
    } = {
      claim: async () => MANAGED_INSTANCE,
      registerSourceVerifierBoot: async () => ({ bootGeneration: 0 }),
      heartbeat: async (_instance, sequence, observation) =>
        managedHeartbeat(
          sequence,
          observation.healthState === "ready" ? "ready" : "degraded",
          observation.reasonCode === "startup" ? "startup" : null,
        ),
      sourceVerifierBootStartup: async () => managedStartup(),
    };
    await expect(
      establishOriginalSourceVerifierStartup(
        runtimeApi,
        MANAGED_INSTANCE,
        MANAGED_OBSERVATION,
      ),
    ).rejects.toThrow(/boot generation is invalid/);
  });

  it("rejects a runtime api without complete boot authority", async () => {
    const order: string[] = [];
    const runtimeApi: ManagedCloudOriginalSourceVerifierStartupApi = {
      claim: async () => MANAGED_INSTANCE,
      heartbeat: async (_instance, sequence, observation) => {
        order.push(`heartbeat:${sequence}`);
        return managedHeartbeat(
          sequence,
          observation.healthState === "ready" ? "ready" : "degraded",
          observation.reasonCode === "startup" ? "startup" : null,
        );
      },
      originalSourceVerifierStartup: async () => managedStartup() as never,
    };
    await expect(
      establishOriginalSourceVerifierStartup(
        runtimeApi,
        MANAGED_INSTANCE,
        MANAGED_OBSERVATION,
      ),
    ).rejects.toThrow(/requires boot startup authority/);
    expect(order).toEqual([]);
  });

  it("orders degraded startup, one readiness CAS, one poll, then sequence N+2", async () => {
    const order: string[] = [];
    const controller = new AbortController();
    const runtimeApi: ManagedCloudOriginalSourceVerifierStartupApi = {
      claim: async () => {
        order.push("claim");
        return MANAGED_INSTANCE;
      },
      heartbeat: async (_instance, sequence, observation) => {
        order.push(
          `managed:${sequence}:${observation.healthState}:${String(observation.reasonCode)}`,
        );
        if (sequence === MANAGED_INSTANCE.nextHeartbeatSequence + 2)
          controller.abort();
        return managedHeartbeat(
          sequence,
          observation.healthState === "ready" ? "ready" : "degraded",
          observation.reasonCode === "startup" ||
            observation.reasonCode === "probe_failed"
            ? observation.reasonCode
            : null,
        );
      },
      registerSourceVerifierBoot: async () => bootRegistration(),
      sourceVerifierBootStartup: async (_instance, generation, sequence) => {
        order.push(`startup:${generation}:${sequence}`);
        return managedStartup();
      },
    };
    const claimedInstance = await runtimeApi.claim();
    const { reportingInstance } = await establishOriginalSourceVerifierStartup(
      runtimeApi,
      claimedInstance,
      MANAGED_OBSERVATION,
    );
    const workerApi: OriginalSourceVerificationWorkerApi = {
      lease: async () => {
        order.push("worker-poll");
        return null;
      },
      heartbeat: vi.fn(),
      complete: vi.fn(),
      fail: vi.fn(),
    };
    const runtime = new OriginalSourceVerifierRuntime({
      api: workerApi,
      workerRuntimeIdentitySha256: RUNTIME_IDENTITY_SHA256,
      pollIntervalMs: 250,
    });

    await runOriginalSourceVerifierLifecycle(
      runtime,
      controller,
      managedStartup(),
      () =>
        runManagedCloudRuntimeHeartbeats(
          runtimeApi,
          reportingInstance,
          async () => {
            expect(runtime.managedCloudReady()).toBe(true);
            return MANAGED_OBSERVATION;
          },
          5_000,
          controller.signal,
        ),
    );

    expect(order).toEqual([
      "claim",
      "managed:11:degraded:startup",
      "startup:7:11",
      "worker-poll",
      "managed:13:ready:null",
    ]);
  });

  it("reports one N+1 probe failure and starts no worker when startup fails", async () => {
    const order: string[] = [];
    const runtimeApi: ManagedCloudOriginalSourceVerifierStartupApi = {
      claim: async () => MANAGED_INSTANCE,
      heartbeat: async (_instance, sequence, observation) => {
        order.push(
          `managed:${sequence}:${observation.healthState}:${String(observation.reasonCode)}`,
        );
        return managedHeartbeat(
          sequence,
          "degraded",
          observation.reasonCode === "startup" ? "startup" : "probe_failed",
        );
      },
      registerSourceVerifierBoot: async () => bootRegistration(),
      sourceVerifierBootStartup: async (_instance, generation, sequence) => {
        order.push(`startup:${generation}:${sequence}`);
        throw new Error("probe rejected");
      },
    };
    const workerStarted = vi.fn();

    await expect(
      (async () => {
        await establishOriginalSourceVerifierStartup(
          runtimeApi,
          MANAGED_INSTANCE,
          MANAGED_OBSERVATION,
        );
        workerStarted();
      })(),
    ).rejects.toThrow("boot startup failed");
    expect(workerStarted).not.toHaveBeenCalled();
    expect(order).toEqual([
      "managed:11:degraded:startup",
      "startup:7:11",
    ]);
  });

  it("does not retry startup or its single failed-probe acknowledgement", async () => {
    const startup = vi.fn(async () => {
      throw new Error("startup unavailable");
    });
    const heartbeatCalls: number[] = [];
    const runtimeApi: ManagedCloudOriginalSourceVerifierStartupApi = {
      claim: async () => MANAGED_INSTANCE,
      heartbeat: async (_instance, sequence, observation) => {
        heartbeatCalls.push(sequence);
        if (observation.reasonCode === "probe_failed") {
          throw new Error("failure acknowledgement unavailable");
        }
        return managedHeartbeat(sequence, "degraded", "startup");
      },
      registerSourceVerifierBoot: async () => bootRegistration(),
      sourceVerifierBootStartup: startup,
    };

    await expect(
      establishOriginalSourceVerifierStartup(
        runtimeApi,
        MANAGED_INSTANCE,
        MANAGED_OBSERVATION,
      ),
    ).rejects.toThrow("boot startup failed");
    expect(startup).toHaveBeenCalledTimes(1);
    expect(heartbeatCalls).toEqual([11]);
  });

  it("starts one verifier poll alongside the immediate managed-runtime report", async () => {
    const api: OriginalSourceVerificationWorkerApi = {
      lease: vi.fn(async () => null),
      heartbeat: vi.fn(),
      complete: vi.fn(),
      fail: vi.fn(),
    };
    const runtime = new OriginalSourceVerifierRuntime({
      api,
      workerRuntimeIdentitySha256: RUNTIME_IDENTITY_SHA256,
      pollIntervalMs: 250,
    });
    const controller = new AbortController();
    const readinessAtImmediateProbe: boolean[] = [];

    await runOriginalSourceVerifierLifecycle(
      runtime,
      controller,
      managedStartup(),
      async () => {
        readinessAtImmediateProbe.push(runtime.managedCloudReady());
        controller.abort();
      },
    );

    expect(readinessAtImmediateProbe).toEqual([true]);
    expect(api.lease).toHaveBeenCalledTimes(1);
  });

  it("has no unmanaged legacy startup mode", async () => {
    vi.stubEnv("BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED", "false");
    await expect(runOriginalSourceVerifierFromEnvironment()).rejects.toThrow(
      /requires managed-cloud runtime authority/,
    );
  });
});
