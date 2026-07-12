import { hostname } from "node:os";
import { pathToFileURL } from "node:url";
import { DiscoveryApiClient } from "./discovery-api.js";
import { DiscoveryWorkerRuntime } from "./discovery-runtime.js";

export function discoveryWorkerFromEnvironment(): DiscoveryWorkerRuntime {
  const origin = process.env.BLUEY_JOBS_API_ORIGIN || "http://127.0.0.1:8080";
  const token = process.env.BLUEY_JOBS_WORKER_TOKEN || "";
  const workerId = process.env.BLUEY_JOBS_DISCOVERY_WORKER_ID
    || `discovery-${hostname()}-${process.pid}`;
  const pollIntervalMs = environmentInteger(
    "BLUEY_JOBS_DISCOVERY_POLL_MS",
    process.env.BLUEY_JOBS_DISCOVERY_POLL_MS,
    5_000,
  );

  return new DiscoveryWorkerRuntime({
    api: new DiscoveryApiClient({ origin, token, workerId }),
    pollIntervalMs,
  });
}

export async function runDiscoveryWorkerFromEnvironment(): Promise<void> {
  const worker = discoveryWorkerFromEnvironment();
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
  void runDiscoveryWorkerFromEnvironment().catch(() => {
    process.stderr.write(`${JSON.stringify({ event: "discovery_worker_fatal", error_code: "startup_failed" })}\n`);
    process.exitCode = 1;
  });
}
