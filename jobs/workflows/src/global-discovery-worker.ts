import { hostname, tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { GlobalDiscoveryApiClient } from "./global-discovery-api.js";
import {
  DEFAULT_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS,
  DEFAULT_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS,
  DEFAULT_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES,
  DEFAULT_GLOBAL_DISCOVERY_POLL_INTERVAL_MS,
  DEFAULT_GLOBAL_DISCOVERY_RUN_INTERVAL_MS,
  GlobalDiscoveryWorkerRuntime,
} from "./global-discovery-runtime.js";

export function globalDiscoveryWorkerFromEnvironment(): GlobalDiscoveryWorkerRuntime {
  const origin = process.env.BLUEY_JOBS_API_ORIGIN || "http://127.0.0.1:8081";
  const signingKey = process.env.BLUEY_JOBS_WORKER_SIGNING_KEY || "";
  const workerId = process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_WORKER_ID
    || `global-discovery-${hostname()}-${process.pid}`;
  const stagingDirectory = process.env.BLUEY_JOBS_GLOBAL_DISCOVERY_STAGING_DIR
    || path.join(tmpdir(), "bluey-jobs-global-discovery");

  return new GlobalDiscoveryWorkerRuntime({
    api: new GlobalDiscoveryApiClient({ origin, signingKey, workerId }),
    stagingDirectory,
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
  const worker = globalDiscoveryWorkerFromEnvironment();
  const controller = new AbortController();
  process.once("SIGINT", () => controller.abort());
  process.once("SIGTERM", () => controller.abort());
  await worker.run(controller.signal);
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
