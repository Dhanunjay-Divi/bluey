import { pathToFileURL } from "node:url";

import { OriginalSourceVerifier } from "@bluey/jobs-automation/original-source-verification";
import { managedCloudRuntimeConfig } from "@bluey/jobs-automation/managed-cloud-runtime";
import {
  ManagedCloudRuntimeApiClient,
  claimManagedCloudRuntimeInstance,
  managedCloudReadyObservation,
  runManagedCloudRuntimeHeartbeats,
  type ManagedCloudOriginalSourceVerifierStartup,
  type ManagedCloudOriginalSourceVerifierStartupApi,
  type ManagedCloudRuntimeInstance,
  type ManagedCloudRuntimeObservation,
} from "@bluey/jobs-automation/managed-cloud-runtime-client";

import { OriginalSourceVerificationApiClient } from "./original-source-verification-api.js";
import {
  DEFAULT_ORIGINAL_SOURCE_LEASE_HEARTBEAT_INTERVAL_MS,
  DEFAULT_ORIGINAL_SOURCE_POLL_INTERVAL_MS,
  OriginalSourceVerifierRuntime,
  originalSourceVerifierBinding,
} from "./original-source-verification-runtime.js";

export async function runOriginalSourceVerifierFromEnvironment(): Promise<void> {
  const runtimeConfig = managedCloudRuntimeConfig("original_source_verifier");
  if (!runtimeConfig) {
    throw new Error(
      "Original-source verifier requires managed-cloud runtime authority",
    );
  }
  const configuredWorkerId =
    process.env.BLUEY_JOBS_ORIGINAL_SOURCE_VERIFIER_WORKER_ID;
  if (configuredWorkerId && configuredWorkerId !== runtimeConfig.workerId) {
    throw new Error(
      "Managed-cloud worker ID must equal the original-source verifier worker ID",
    );
  }

  const controller = new AbortController();
  process.once("SIGINT", () => controller.abort());
  process.once("SIGTERM", () => controller.abort());
  const namespace = requiredEnvironment("TEMPORAL_NAMESPACE");
  const taskQueue =
    process.env.BLUEY_JOBS_TASK_QUEUE || "bluey-jobs-applications";
  const runtimeApi = new ManagedCloudRuntimeApiClient(runtimeConfig);
  const runtimeInstance = await claimManagedCloudRuntimeInstance(
    runtimeApi,
    controller.signal,
  );
  const observation = await managedCloudReadyObservation(
    runtimeInstance,
    namespace,
    taskQueue,
    new URL("./failure-converter.js", import.meta.url),
  );
  const { reportingInstance, startup } =
    await establishOriginalSourceVerifierStartup(
      runtimeApi,
      runtimeInstance,
      observation,
    );
  const worker = new OriginalSourceVerifierRuntime({
    api: new OriginalSourceVerificationApiClient({
      origin: runtimeConfig.apiOrigin,
      signingKey: runtimeConfig.signingKey,
      binding: originalSourceVerifierBinding(
        runtimeInstance,
        runtimeConfig.sessionToken,
      ),
      requestTimeoutMs: environmentInteger(
        "BLUEY_JOBS_ORIGINAL_SOURCE_API_TIMEOUT_MS",
        process.env.BLUEY_JOBS_ORIGINAL_SOURCE_API_TIMEOUT_MS,
        10_000,
      ),
    }),
    verifier: new OriginalSourceVerifier({
      timeoutMs: environmentInteger(
        "BLUEY_JOBS_ORIGINAL_SOURCE_PROVIDER_TIMEOUT_MS",
        process.env.BLUEY_JOBS_ORIGINAL_SOURCE_PROVIDER_TIMEOUT_MS,
        8_000,
      ),
    }),
    workerRuntimeIdentitySha256: runtimeInstance.runtimeIdentitySha256,
    pollIntervalMs: environmentInteger(
      "BLUEY_JOBS_ORIGINAL_SOURCE_POLL_MS",
      process.env.BLUEY_JOBS_ORIGINAL_SOURCE_POLL_MS,
      DEFAULT_ORIGINAL_SOURCE_POLL_INTERVAL_MS,
    ),
    leaseHeartbeatIntervalMs: environmentInteger(
      "BLUEY_JOBS_ORIGINAL_SOURCE_LEASE_HEARTBEAT_MS",
      process.env.BLUEY_JOBS_ORIGINAL_SOURCE_LEASE_HEARTBEAT_MS,
      DEFAULT_ORIGINAL_SOURCE_LEASE_HEARTBEAT_INTERVAL_MS,
    ),
  });
  await runOriginalSourceVerifierLifecycle(worker, controller, startup, () =>
    runManagedCloudRuntimeHeartbeats(
      runtimeApi,
      reportingInstance,
      async (instance) => {
        if (!worker.managedCloudReady()) {
          throw new Error("Original-source verifier dependency probe is stale");
        }
        return managedCloudReadyObservation(
          instance,
          namespace,
          taskQueue,
          new URL("./failure-converter.js", import.meta.url),
        );
      },
      runtimeConfig.heartbeatIntervalMs,
      controller.signal,
    ),
  );
}

export interface EstablishedOriginalSourceVerifierStartup {
  reportingInstance: ManagedCloudRuntimeInstance;
  startup: ManagedCloudOriginalSourceVerifierStartup;
  bootGeneration?: number;
}

export async function establishOriginalSourceVerifierStartup(
  runtimeApi: ManagedCloudOriginalSourceVerifierStartupApi,
  runtimeInstance: ManagedCloudRuntimeInstance,
  observation: ManagedCloudRuntimeObservation,
): Promise<EstablishedOriginalSourceVerifierStartup> {
  // Establish the server-assigned boot generation before any heartbeat so the
  // readiness transition can be fenced by generation rather than by instance
  // epoch alone. Registration is idempotent, so a retry inside this process
  // replays the same generation instead of allocating a new one.
  let bootGeneration: number | undefined;
  const bootCapableApi = runtimeApi as Partial<{
    registerSourceVerifierBoot: (
      instance: ManagedCloudRuntimeInstance,
    ) => Promise<{ bootGeneration: number }>;
  }>;
  if (typeof bootCapableApi.registerSourceVerifierBoot === "function") {
    const registration = await bootCapableApi.registerSourceVerifierBoot(
      runtimeInstance,
    );
    if (
      !Number.isSafeInteger(registration.bootGeneration) ||
      registration.bootGeneration < 1
    ) {
      throw new Error(
        "Original-source verifier boot generation is invalid",
      );
    }
    bootGeneration = registration.bootGeneration;
  }
  const degradedHeartbeatSequence = runtimeInstance.nextHeartbeatSequence;
  const degradedHeartbeat = await runtimeApi.heartbeat(
    runtimeInstance,
    degradedHeartbeatSequence,
    {
      ...observation,
      healthState: "degraded",
      reasonCode: "startup",
    },
  );
  if (
    degradedHeartbeat.replayed ||
    degradedHeartbeat.heartbeatSequence !== degradedHeartbeatSequence ||
    degradedHeartbeat.healthState !== "degraded" ||
    degradedHeartbeat.reasonCode !== "startup"
  ) {
    throw new Error(
      "Original-source verifier did not establish fresh degraded startup authority",
    );
  }

  let startup: ManagedCloudOriginalSourceVerifierStartup;
  try {
    startup = await runtimeApi.originalSourceVerifierStartup(
      runtimeInstance,
      degradedHeartbeatSequence,
    );
    if (
      startup.degradedHeartbeatSequence !== degradedHeartbeatSequence ||
      startup.readyHeartbeatSequence !== degradedHeartbeatSequence + 1
    ) {
      throw new Error(
        "Original-source verifier startup sequence is inconsistent",
      );
    }
  } catch (startupError) {
    try {
      const failedHeartbeat = await runtimeApi.heartbeat(
        runtimeInstance,
        degradedHeartbeatSequence + 1,
        {
          ...observation,
          healthState: "degraded",
          reasonCode: "probe_failed",
        },
      );
      if (
        failedHeartbeat.replayed ||
        failedHeartbeat.heartbeatSequence !== degradedHeartbeatSequence + 1 ||
        failedHeartbeat.healthState !== "degraded" ||
        failedHeartbeat.reasonCode !== "probe_failed"
      ) {
        throw new Error(
          "Original-source verifier probe failure was not freshly acknowledged",
        );
      }
    } catch (failureReportError) {
      throw new AggregateError(
        [startupError, failureReportError],
        "Original-source verifier startup and probe-failure acknowledgement failed",
      );
    }
    throw new Error("Original-source verifier startup probe failed", {
      cause: startupError,
    });
  }

  return {
    startup,
    bootGeneration,
    reportingInstance: {
      ...runtimeInstance,
      nextHeartbeatSequence: startup.readyHeartbeatSequence + 1,
    },
  };
}

export async function runOriginalSourceVerifierLifecycle(
  worker: OriginalSourceVerifierRuntime,
  controller: AbortController,
  startup: ManagedCloudOriginalSourceVerifierStartup,
  runRuntimeHeartbeats: () => Promise<void>,
): Promise<void> {
  // The transactional startup authority already performed the rollback-only
  // lease probe. Seed the local freshness fence before starting the single
  // polling owner and the recurring managed heartbeat owner together.
  worker.markManagedCloudStartupReady(startup.startupAuthoritySha256);
  const workerRun = worker.run(controller.signal);
  try {
    await Promise.race([workerRun, runRuntimeHeartbeats()]);
  } finally {
    controller.abort();
    worker.stop();
    await workerRun.catch(() => undefined);
  }
}

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value !== value.trim()
  ) {
    throw new Error(`${name} is required for managed-cloud runtime evidence`);
  }
  return value;
}

function environmentInteger(
  name: string,
  value: string | undefined,
  fallback: number,
): number {
  if (value === undefined || value.trim() === "") return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  void runOriginalSourceVerifierFromEnvironment().catch(() => {
    process.stderr.write(
      `${JSON.stringify({
        event: "original_source_verifier_fatal",
        error_code: "startup_failed",
      })}\n`,
    );
    process.exitCode = 1;
  });
}
