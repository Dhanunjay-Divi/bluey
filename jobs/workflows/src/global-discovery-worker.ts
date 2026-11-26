import { hostname, tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { GlobalDiscoveryApiClient } from "./global-discovery-api.js";
import { managedCloudRuntimeConfig } from "@bluey/jobs-automation/managed-cloud-runtime";
import {
  ManagedCloudRuntimeApiClient,
  claimManagedCloudRuntimeInstance,
  managedCloudReadyObservation,
  runManagedCloudRuntimeHeartbeats,
} from "@bluey/jobs-automation/managed-cloud-runtime-client";
import {
  DEFAULT_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS,
  DEFAULT_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS,
  DEFAULT_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES,
  DEFAULT_GLOBAL_DISCOVERY_POLL_INTERVAL_MS,
  DEFAULT_GLOBAL_DISCOVERY_RUN_INTERVAL_MS,
  GlobalDiscoveryWorkerRuntime,
  parseGlobalDiscoverySourceFamilies,
} from "./global-discovery-runtime.js";

export function globalDiscoveryWorkerFromEnvironment(): GlobalDiscoveryWorkerRuntime {
  const runtimeConfig = managedCloudRuntimeConfig("global_discovery_worker");
  const origin = process.env.BLUEY_JOBS_API_ORIGIN || "http://127.0.0.1:8081";
  const signingKey = process.env.BLUEY_JOBS_WORKER_SIGNING_KEY || "";
  const workerId = process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_WORKER_ID
    || `global-discovery-${hostname()}-${process.pid}`;
  if (runtimeConfig && runtimeConfig.workerId !== workerId) {
    throw new Error("Managed-cloud worker ID must equal the global discovery worker ID");
  }
  const stagingDirectory = process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_STAGING_DIR
    || path.join(tmpdir(), "bluey-jobs-global-discovery");
  const sourceFamilies = parseGlobalDiscoverySourceFamilies(
    process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_SOURCE_FAMILIES,
  );
  if (!sourceFamilies || sourceFamilies.length === 0) {
    throw new Error(
      "BLUEY_JOBS_GLOBAL_DISCOVERY_SOURCE_FAMILIES is required; apply an approved rollout wave",
    );
  }

  return new GlobalDiscoveryWorkerRuntime({
    api: new GlobalDiscoveryApiClient({ origin, signingKey, workerId }),
    stagingDirectory,
    sourceFamilies,
    pollIntervalMs: environmentInteger(
      "BLUEY_JOBS_GLOBAL_DISCOVERY_POLL_MS",
      process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_POLL_MS,
      DEFAULT_GLOBAL_DISCOVERY_POLL_INTERVAL_MS,
    ),
    runIntervalMs: environmentInteger(
      "BLUEY_JOBS_GLOBAL_DISCOVERY_RUN_INTERVAL_MS",
      process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_RUN_INTERVAL_MS,
      DEFAULT_GLOBAL_DISCOVERY_RUN_INTERVAL_MS,
    ),
    manifestRefreshMs: environmentInteger(
      "BLUEY_JOBS_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS",
      process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS,
      DEFAULT_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS,
    ),
    artifactTimeoutMs: environmentInteger(
      "BLUEY_JOBS_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS",
      process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS,
      DEFAULT_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS,
    ),
    maxArtifactBytes: environmentInteger(
      "BLUEY_JOBS_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES",
      process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES,
      DEFAULT_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES,
    ),
  });
}

export async function runGlobalDiscoveryWorkerFromEnvironment(): Promise<void> {
  const runtimeConfig = managedCloudRuntimeConfig("global_discovery_worker");
  const worker = globalDiscoveryWorkerFromEnvironment();
  const controller = new AbortController();
  process.once("SIGINT", () => controller.abort());
  process.once("SIGTERM", () => controller.abort());
  if (!runtimeConfig) {
    await worker.run(controller.signal);
    return;
  }
  const namespace = requiredEnvironment("TEMPORAL_NAMESPACE");
  const taskQueue = process.env.BLUEY_JOBS_TASK_QUEUE || "bluey-jobs-applications";
  const runtimeApi = new ManagedCloudRuntimeApiClient(runtimeConfig);
  const runtimeInstance = await claimManagedCloudRuntimeInstance(
    runtimeApi,
    controller.signal,
  );
  const workerRun = worker.run(controller.signal);
  await waitForGlobalDiscoveryReady(worker, controller.signal);
  try {
    await Promise.race([
      workerRun,
      runManagedCloudRuntimeHeartbeats(
        runtimeApi,
        runtimeInstance,
        async (instance) => {
          if (!worker.managedCloudReady()) {
            throw new Error("Global discovery worker dependency probe is stale");
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
    ]);
  } finally {
    controller.abort();
    await workerRun.catch(() => undefined);
  }
}

async function waitForGlobalDiscoveryReady(
  worker: GlobalDiscoveryWorkerRuntime,
  signal: AbortSignal,
): Promise<void> {
  for (let attempts = 0; attempts < 500 && !signal.aborted; attempts += 1) {
    if (worker.managedCloudReady()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("Global discovery worker did not reach managed-cloud readiness");
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
  if (!Number.isInteger(parsed)) throw new Error(`${name} must be an integer`);
  return parsed;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  void runGlobalDiscoveryWorkerFromEnvironment().catch(() => {
    process.stderr.write(`${JSON.stringify({
      event: "global_discovery_worker_fatal",
      error_code: "startup_failed",
    })}\n`);
    process.exitCode = 1;
  });
}
