import { createHash, createHmac } from "node:crypto";
import { describe, expect, it } from "vitest";
import { workerAuthHeaders, workerScope } from "../src/worker-auth.js";

const signingKey = "0123456789abcdef0123456789abcdef";

describe("Jobs worker request signing", () => {
  it("binds the signature to worker, time, nonce, scope, method, and path", () => {
    const path = "/api/jobs/internal/discovery/lease";
    const body = JSON.stringify({ source: "greenhouse" });
    const headers = workerAuthHeaders({
      signingKey,
      workerId: "discovery-1",
      method: "POST",
      path,
      body,
      nowSeconds: 1_750_000_000,
      nonce: "abcdef0123456789abcdef0123456789",
    });
    const canonical = [
      "bluey-jobs-worker-v1",
      "1750000000",
      "abcdef0123456789abcdef0123456789",
      "discovery-1",
      "bluey-jobs-api",
      "discovery",
      "POST",
      path,
      createHash("sha256").update(body).digest("hex"),
    ].join("\n");

    expect(headers["x-bluey-jobs-worker-signature"]).toBe(
      createHmac("sha256", signingKey).update(canonical).digest("hex"),
    );
    expect(headers["x-bluey-jobs-worker-scope"]).toBe("discovery");
    expect(headers["x-bluey-jobs-worker-audience"]).toBe("bluey-jobs-api");
  });

  it("rejects public and unscoped paths", () => {
    expect(() => workerScope("GET", "/api/jobs/workspace")).toThrow("not signable");
    expect(() => workerScope("POST", "/api/jobs/internal/unknown")).toThrow("no permitted scope");
  });
});
