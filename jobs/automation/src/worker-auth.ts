import { createHash, createHmac, randomBytes } from "node:crypto";

const SIGNATURE_VERSION = "bluey-jobs-worker-v1";
const MANAGED_CLOUD_SIGNATURE_VERSION = "bluey-jobs-worker-v2";
const SIGNATURE_AUDIENCE = "bluey-jobs-api";
const SAFE_IDENTIFIER = /^[A-Za-z0-9._:-]+$/;
const ORIGINAL_SOURCE_ASSIGNMENT_ROUTE =
  /^\/api\/jobs\/internal\/original-source-verifications\/[A-Za-z0-9_-]{20,128}\/(?:heartbeat|complete|fail)$/;

export type JobsWorkerScope =
  | "application-state"
  | "discovery"
  | "execution"
  | "intervention"
  | "managed-cloud-runtime"
  | "original-source-verification"
  | "receipt"
  | "runner-volume"
  | "run-events"
  | "workflow-command-execution"
  | "workflow-command-materialize";

export interface JobsWorkerAuthInput {
  signingKey: string;
  workerId: string;
  method: string;
  path: string;
  body?: string | Uint8Array;
  origin?: string;
  timestamp?: number;
  nonce?: string;
}

export function createJobsWorkerAuthHeaders(input: JobsWorkerAuthInput): Record<string, string> {
  const method = input.method.toUpperCase();
  const scope = jobsWorkerScope(method, input.path);
  if (!scope || input.path.includes("?") || input.path.includes("#")) {
    throw new Error("Bluey Jobs worker request path is not signable");
  }
  assertIdentifier(input.workerId, 3, 128, "worker ID");
  if (Buffer.byteLength(input.signingKey, "utf8") < 32) {
    throw new Error("BLUEY_JOBS_WORKER_SIGNING_KEY must contain at least 32 bytes");
  }

  const timestamp = input.timestamp ?? Math.floor(Date.now() / 1_000);
  if (!Number.isSafeInteger(timestamp) || timestamp < 0) {
    throw new Error("Bluey Jobs worker timestamp is invalid");
  }
  const nonce = input.nonce ?? randomBytes(16).toString("hex");
  assertIdentifier(nonce, 24, 128, "nonce");

  const contentSha256 = createHash("sha256").update(input.body ?? "").digest("hex");
  const managedCloudOrigin = scope === "managed-cloud-runtime"
    ? normalizeManagedCloudWorkerOrigin(input.origin)
    : undefined;
  if (scope !== "managed-cloud-runtime" && input.origin !== undefined) {
    throw new Error("Bluey Jobs worker origin is only valid for managed-cloud runtime requests");
  }
  const canonical = [
    managedCloudOrigin ? MANAGED_CLOUD_SIGNATURE_VERSION : SIGNATURE_VERSION,
    String(timestamp),
    nonce,
    input.workerId,
    SIGNATURE_AUDIENCE,
    scope,
    ...(managedCloudOrigin ? [managedCloudOrigin] : []),
    method,
    input.path,
    contentSha256,
  ].join("\n");
  const signature = createHmac("sha256", input.signingKey).update(canonical).digest("hex");

  return {
    "x-bluey-jobs-worker-id": input.workerId,
    "x-bluey-jobs-worker-timestamp": String(timestamp),
    "x-bluey-jobs-worker-nonce": nonce,
    "x-bluey-jobs-worker-audience": SIGNATURE_AUDIENCE,
    "x-bluey-jobs-worker-scope": scope,
    "x-bluey-jobs-worker-content-sha256": contentSha256,
    "x-bluey-jobs-worker-signature": signature,
    ...(managedCloudOrigin
      ? { "x-bluey-jobs-worker-origin": managedCloudOrigin }
      : {}),
  };
}

export function normalizeManagedCloudWorkerOrigin(value: string | undefined): string {
  if (typeof value !== "string" || value.length === 0 || value !== value.trim()) {
    throw new Error("Bluey Jobs managed-cloud worker origin is required");
  }
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("Bluey Jobs managed-cloud worker origin is invalid");
  }
  const loopbackHttp = url.protocol === "http:"
    && ["127.0.0.1", "localhost", "::1"].includes(url.hostname);
  if ((url.protocol !== "https:" && !loopbackHttp)
    || url.username
    || url.password
    || url.search
    || url.hash
    || !["", "/"].includes(url.pathname)) {
    throw new Error("Bluey Jobs managed-cloud worker origin is invalid");
  }
  url.pathname = "";
  return url.toString().replace(/\/$/, "");
}

export function jobsWorkerScope(method: string, path: string): JobsWorkerScope | undefined {
  if (method.toUpperCase() !== "POST" || !path.startsWith("/api/jobs/internal/")) return undefined;
  if (/^\/api\/jobs\/internal\/workflow-commands\/[A-Za-z0-9_-]{20,128}\/materialize$/.test(path)) {
    return "workflow-command-materialize";
  }
  const workflowExecution =
    /^\/api\/jobs\/internal\/workflow-commands\/[A-Za-z0-9_-]{20,128}\/(?:finalize|intervention\/prepare)$/;
  const interventionPublish =
    /^\/api\/jobs\/internal\/workflow-commands\/[A-Za-z0-9_-]{20,128}\/intervention\/[A-Za-z0-9_-]{20,128}\/publish$/;
  if (workflowExecution.test(path) || interventionPublish.test(path)) {
    return "workflow-command-execution";
  }
  if (/^\/api\/jobs\/internal\/managed-cloud\/runtime-grants\/[A-Za-z0-9_-]{20,128}\/claim$/.test(path)
    || /^\/api\/jobs\/internal\/managed-cloud\/runtime-instances\/[A-Za-z0-9_-]{20,128}\/heartbeats$/.test(path)) {
    return "managed-cloud-runtime";
  }
  if (path === "/api/jobs/internal/original-source-verifications/lease"
    || ORIGINAL_SOURCE_ASSIGNMENT_ROUTE.test(path)) {
    return "original-source-verification";
  }
  if (path.includes("/runner-volumes/")) return "runner-volume";
  if (path.includes("/execution-leases/")) return "execution";
  if (path.includes("/discovery/") || path.includes("/global-discovery/")) return "discovery";
  if (path.endsWith("/receipt")) return "receipt";
  if (path.endsWith("/interventions")) return "intervention";
  if (path.endsWith("/state")) return "application-state";
  if (path.endsWith("/events")) return "run-events";
  return undefined;
}

function assertIdentifier(value: string, minimum: number, maximum: number, label: string): void {
  const bytes = Buffer.byteLength(value, "utf8");
  if (bytes < minimum || bytes > maximum || !SAFE_IDENTIFIER.test(value)) {
    throw new Error(`Bluey Jobs worker ${label} is invalid`);
  }
}
