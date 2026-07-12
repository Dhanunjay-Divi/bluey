import { createHash, createHmac, randomBytes } from "node:crypto";

const SIGNATURE_VERSION = "bluey-jobs-worker-v1";
const SIGNATURE_AUDIENCE = "bluey-jobs-api";
const SAFE_IDENTIFIER = /^[A-Za-z0-9._:-]+$/;

export type JobsWorkerScope =
  | "application-state"
  | "discovery"
  | "execution"
  | "intervention"
  | "receipt"
  | "run-events";

export interface JobsWorkerAuthInput {
  signingKey: string;
  workerId: string;
  method: string;
  path: string;
  body?: string | Uint8Array;
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
  const canonical = [
    SIGNATURE_VERSION,
    String(timestamp),
    nonce,
    input.workerId,
    SIGNATURE_AUDIENCE,
    scope,
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
  };
}

export function jobsWorkerScope(method: string, path: string): JobsWorkerScope | undefined {
  if (method.toUpperCase() !== "POST" || !path.startsWith("/api/jobs/internal/")) return undefined;
  if (path.includes("/execution-leases/")) return "execution";
  if (path.includes("/discovery/")) return "discovery";
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
