import { createHash, createHmac, randomBytes } from "node:crypto";

const VERSION = "bluey-jobs-worker-v1";
const AUDIENCE = "bluey-jobs-api";
const SAFE_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;

export function executionWorkerAuthHeaders(input: {
  signingKey: string;
  workerId: string;
  path: string;
  body?: string;
  nowSeconds?: number;
  nonce?: string;
}): Record<string, string> {
  if (Buffer.byteLength(input.signingKey) < 32 || Buffer.byteLength(input.signingKey) > 4_096) {
    throw new Error("worker signing key is invalid");
  }
  if (!SAFE_ID.test(input.workerId)) throw new Error("worker ID is invalid");
  const timestamp = input.nowSeconds ?? Math.floor(Date.now() / 1_000);
  const nonce = input.nonce ?? randomBytes(24).toString("hex");
  if (!Number.isSafeInteger(timestamp) || timestamp <= 0 || !SAFE_ID.test(nonce) || nonce.length < 24) {
    throw new Error("worker request metadata is invalid");
  }
  if (!input.path.startsWith("/api/jobs/internal/execution-leases/")) {
    throw new Error("worker path is outside the execution scope");
  }
  const contentSha256 = createHash("sha256").update(input.body ?? "").digest("hex");
  const canonical = `${VERSION}\n${timestamp}\n${nonce}\n${input.workerId}\n${AUDIENCE}\nexecution\nPOST\n${input.path}\n${contentSha256}`;
  return {
    "x-bluey-jobs-worker-id": input.workerId,
    "x-bluey-jobs-worker-timestamp": String(timestamp),
    "x-bluey-jobs-worker-nonce": nonce,
    "x-bluey-jobs-worker-audience": AUDIENCE,
    "x-bluey-jobs-worker-scope": "execution",
    "x-bluey-jobs-worker-content-sha256": contentSha256,
    "x-bluey-jobs-worker-signature": createHmac("sha256", input.signingKey)
      .update(canonical)
      .digest("hex"),
  };
}
