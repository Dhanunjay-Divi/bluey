import { createHash, createHmac, randomBytes } from "node:crypto";

const VERSION = "bluey-jobs-worker-v1";
const AUDIENCE = "bluey-jobs-api";
const SAFE_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;

export interface WorkerAuthInput {
  signingKey: string;
  workerId: string;
  method: string;
  path: string;
  body?: string;
  nowSeconds?: number;
  nonce?: string;
}

export function workerAuthHeaders(input: WorkerAuthInput): Record<string, string> {
  if (Buffer.byteLength(input.signingKey) < 32) {
    throw new Error("BLUEY_JOBS_WORKER_SIGNING_KEY must be at least 32 bytes");
  }
  if (!SAFE_ID.test(input.workerId)) throw new Error("Jobs worker ID is invalid");
  const scope = workerScope(input.method, input.path);
  const timestamp = input.nowSeconds ?? Math.floor(Date.now() / 1_000);
  if (!Number.isSafeInteger(timestamp) || timestamp <= 0) throw new Error("Jobs worker timestamp is invalid");
  const nonce = input.nonce ?? randomBytes(24).toString("hex");
  if (!SAFE_ID.test(nonce) || nonce.length < 24) throw new Error("Jobs worker nonce is invalid");
  const method = input.method.toUpperCase();
  const contentSha256 = createHash("sha256").update(input.body ?? "").digest("hex");
  const canonical = `${VERSION}\n${timestamp}\n${nonce}\n${input.workerId}\n${AUDIENCE}\n${scope}\n${method}\n${input.path}\n${contentSha256}`;
  const signature = createHmac("sha256", input.signingKey).update(canonical).digest("hex");
  return {
    "x-bluey-jobs-worker-id": input.workerId,
    "x-bluey-jobs-worker-timestamp": String(timestamp),
    "x-bluey-jobs-worker-nonce": nonce,
    "x-bluey-jobs-worker-audience": AUDIENCE,
    "x-bluey-jobs-worker-scope": scope,
    "x-bluey-jobs-worker-content-sha256": contentSha256,
    "x-bluey-jobs-worker-signature": signature,
  };
}

export function workerScope(method: string, path: string): string {
  if (method.toUpperCase() !== "POST" || !path.startsWith("/api/jobs/internal/")) {
    throw new Error("Jobs worker path is not signable");
  }
  if (path.includes("/execution-leases/")) return "execution";
  if (path.includes("/discovery/")) return "discovery";
  if (path.endsWith("/receipt")) return "receipt";
  if (path.endsWith("/interventions")) return "intervention";
  if (path.endsWith("/state")) return "application-state";
  if (path.endsWith("/events")) return "run-events";
  throw new Error("Jobs worker path has no permitted scope");
}
