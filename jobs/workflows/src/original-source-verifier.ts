import { pathToFileURL } from "node:url";

import { OriginalSourceVerifier } from "@bluey/jobs-automation/original-source-verification";
import { managedCloudRuntimeConfig } from "@bluey/jobs-automation/managed-cloud-runtime";
import {
  ManagedCloudRuntimeApiClient,
  claimManagedCloudRuntimeInstance,
  managedCloudReadyObservation,
  runManagedCloudRuntimeHeartbeats,
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
    throw new Error("Original-source verifier requires managed-cloud runtime authority");
  }
  const configuredWorkerId = process.env.BLUEY_JOBS_ORIGINAL_SOURCE_VERIFIER_WORKER_ID;
  if (configuredWorkerId && configuredWorkerId !== runtimeConfig.workerId) {
    throw new Error("Managed-cloud worker ID must equal the original-source verifier worker ID");
  }

  const controller = new AbortController();
  process.once("SIGINT", () => controller.abort());
  process.once("SIGTERM", () => controller.abort());
  const namespace = requiredEnvironment("TEMPORAL_NAMESPACE");
  const taskQueue = process.env.BLUEY_JOBS_TASK_QUEUE || "bluey-jobs-applications";
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
  const firstHeartbeat = await runtimeApi.heartbeat(
    runtimeInstance,
    runtimeInstance.nextHeartbeatSequence,
    observation,
  );
  if (firstHeartbeat.healthState !== "ready" || firstHeartbeat.replayed) {
    throw new Error("Original-source verifier did not establish fresh runtime readiness");
  }
  const reportingInstance = {
    ...runtimeInstance,
    nextHeartbeatSequence: firstHeartbeat.heartbeatSequence + 1,
  };
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
  await runOriginalSourceVerifierLifecycle(
    worker,
    controller,
    () => runManagedCloudRuntimeHeartbeats(
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

export async function runOriginalSourceVerifierLifecycle(
  worker: OriginalSourceVerifierRuntime,
  controller: AbortController,
  runRuntimeHeartbeats: () => Promise<void>,
): Promise<void> {
  // The managed heartbeat loop probes immediately. Establish one successful
  // lease poll first so startup cannot publish readiness before the verifier
  // dependency itself has been exercised.
  await worker.establishReadiness();
  const workerRun = worker.run(controller.signal);
  try {
    await Promise.race([
      workerRun,
      runRuntimeHeartbeats(),
    ]);
  } finally {
    controller.abort();
    worker.stop();
    await workerRun.catch(() => undefined);
  }
}

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (typeof value !== "string" || value.length === 0 || value !== value.trim()) {
    throw new Error(`${name} is required for managed-cloud runtime evidence`);
  }
  return value;
}

function environmentInteger(name: string, value: string | undefined, fallback: number): number {
  if (value === undefined || value.trim() === "") return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  void runOriginalSourceVerifierFromEnvironment().catch(() => {
    process.stderr.write(`${JSON.stringify({
      event: "original_source_verifier_fatal",
      error_code: "startup_failed",
    })}\n`);
    process.exitCode = 1;
  });
}
