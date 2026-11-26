import { NativeConnection, Worker } from "@temporalio/worker";
import * as activities from "./activities.js";
import { managedCloudRuntimeConfig } from "@bluey/jobs-automation/managed-cloud-runtime";
import {
  ManagedCloudRuntimeApiClient,
  claimManagedCloudRuntimeInstance,
  managedCloudReadyObservation,
  runManagedCloudRuntimeHeartbeats,
} from "@bluey/jobs-automation/managed-cloud-runtime-client";

async function run(): Promise<void> {
  const runtimeConfig = managedCloudRuntimeConfig("workflow_worker");
  const address = process.env.TEMPORAL_ADDRESS;
  const namespace = process.env.TEMPORAL_NAMESPACE;
  if (!address || !namespace) throw new Error("TEMPORAL_ADDRESS and TEMPORAL_NAMESPACE are required");
  const controller = new AbortController();
  const stop = (): void => controller.abort();
  const runtimeApi = runtimeConfig
    ? new ManagedCloudRuntimeApiClient(runtimeConfig)
    : undefined;
  if (runtimeConfig) {
    process.once("SIGINT", stop);
    process.once("SIGTERM", stop);
  }
  const runtimeInstance = runtimeApi
    ? await claimManagedCloudRuntimeInstance(runtimeApi, controller.signal)
    : undefined;
  const connection = await NativeConnection.connect({
    address,
    tls: process.env.TEMPORAL_TLS === "false" ? false : true,
    apiKey: process.env.TEMPORAL_API_KEY,
  });
  const taskQueue = process.env.BLUEY_JOBS_TASK_QUEUE || "bluey-jobs-applications";
  const failureConverterPath = new URL("./failure-converter.js", import.meta.url).pathname;
  const worker = await Worker.create({
    connection,
    namespace,
    taskQueue,
    workflowsPath: new URL("./workflows.js", import.meta.url).pathname,
    activities,
    dataConverter: {
      failureConverterPath,
    },
  });
  if (!runtimeConfig) {
    await worker.run();
    return;
  }

  let workerRun: Promise<void> | undefined;
  try {
    if (!runtimeApi || !runtimeInstance) {
      throw new Error("Managed-cloud workflow runtime identity is unavailable");
    }
    workerRun = worker.run();
    await waitForWorkerRunning(worker, controller.signal);
    await Promise.race([
      workerRun,
      runManagedCloudRuntimeHeartbeats(
        runtimeApi,
        runtimeInstance,
        async (instance) => {
          if (worker.getState() !== "RUNNING") {
            throw new Error("Temporal worker is not running");
          }
          await connection.workflowService.describeNamespace({ namespace });
          return managedCloudReadyObservation(
            instance,
            namespace,
            taskQueue,
            failureConverterPath,
          );
        },
        runtimeConfig.heartbeatIntervalMs,
        controller.signal,
      ),
    ]);
  } finally {
    controller.abort();
    process.off("SIGINT", stop);
    process.off("SIGTERM", stop);
    await worker.shutdown();
    await workerRun?.catch(() => undefined);
    await connection.close();
  }
}

async function waitForWorkerRunning(worker: Worker, signal: AbortSignal): Promise<void> {
  for (let attempts = 0; attempts < 500 && !signal.aborted; attempts += 1) {
    const state = worker.getState();
    if (state === "RUNNING") return;
    if (state === "FAILED" || state === "STOPPED") {
      throw new Error("Temporal worker stopped before managed-cloud readiness");
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("Temporal worker did not reach managed-cloud readiness");
}

void run();
