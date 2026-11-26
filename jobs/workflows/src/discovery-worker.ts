import { hostname } from "node:os";
import { pathToFileURL } from "node:url";
import { DiscoveryApiClient } from "./discovery-api.js";
import { DiscoveryWorkerRuntime } from "./discovery-runtime.js";
import { managedCloudRuntimeConfig } from "@bluey/jobs-automation/managed-cloud-runtime";
import {
  ManagedCloudRuntimeApiClient,
  claimManagedCloudRuntimeInstance,
  managedCloudReadyObservation,
  runManagedCloudRuntimeHeartbeats,
} from "@bluey/jobs-automation/managed-cloud-runtime-client";

export function discoveryWorkerFromEnvironment(): DiscoveryWorkerRuntime {
  const runtimeConfig = managedCloudRuntimeConfig("discovery_worker");
  const origin = process.env.BLUEY_JOBS_API_ORIGIN || "http://127.0.0.1:8080";
  const signingKey = process.env.BLUEY_JOBS_WORKER_SIGNING_KEY || "";
  const workerId = process.env.BLUEY_JOBS_DISCOVERY_WORKER_ID
    || `discovery-${hostname()}-${process.pid}`;
  if (runtimeConfig && runtimeConfig.workerId !== workerId) {
    throw new Error("Managed-cloud worker ID must equal the discovery worker ID");
  }
  const pollIntervalMs = environmentInteger(
    "BLUEY_JOBS_DISCOVERY_POLL_MS",
    process.env.BLUEY_JOBS_DISCOVERY_POLL_MS,
    5_000,
  );

  return new DiscoveryWorkerRuntime({
    api: new DiscoveryApiClient({ origin, signingKey, workerId }),
    pollIntervalMs,
  });
}

export async function runDiscoveryWorkerFromEnvironment(): Promise<void> {
  const runtimeConfig = managedCloudRuntimeConfig("discovery_worker");
  const worker = discoveryWorkerFromEnvironment();
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
  await waitForDiscoveryReady(worker, controller.signal);
  try {
    await Promise.race([
      workerRun,
      runManagedCloudRuntimeHeartbeats(
        runtimeApi,
        runtimeInstance,
        async (instance) => {
          if (!worker.managedCloudReady()) {
            throw new Error("Discovery worker dependency probe is stale");
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

async function waitForDiscoveryReady(
  worker: DiscoveryWorkerRuntime,
  signal: AbortSignal,
): Promise<void> {
  for (let attempts = 0; attempts < 500 && !signal.aborted; attempts += 1) {
    if (worker.managedCloudReady()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("Discovery worker did not reach managed-cloud readiness");
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
  void runDiscoveryWorkerFromEnvironment().catch(() => {
    process.stderr.write(`${JSON.stringify({ event: "discovery_worker_fatal", error_code: "startup_failed" })}\n`);
    process.exitCode = 1;
  });
}
