import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import {
  managedCloudRuntimeClaimRequest,
  managedCloudRuntimeConfig,
  measureManagedCloudRuntimeIdentity,
} from "../src/managed-cloud-runtime.js";

const MEASUREMENT_ROOT = mkdtempSync(join(tmpdir(), "bluey-runtime-measurement-"));
const MEASURED_PATH = "app/workflows/dist/gateway.js";
const MEASURED_BYTES = Buffer.from("gateway-runtime\n");
const NODE_PATH = "usr/local/bin/node";
const NODE_BYTES = Buffer.from("node-runtime\n");
mkdirSync(join(MEASUREMENT_ROOT, "app/.bluey"), { recursive: true });
mkdirSync(join(MEASUREMENT_ROOT, "app/automation"), { recursive: true });
mkdirSync(join(MEASUREMENT_ROOT, "app/workflows/dist"), { recursive: true });
mkdirSync(join(MEASUREMENT_ROOT, "usr/local/bin"), { recursive: true });
writeFileSync(join(MEASUREMENT_ROOT, MEASURED_PATH), MEASURED_BYTES);
writeFileSync(join(MEASUREMENT_ROOT, NODE_PATH), NODE_BYTES);
const MEASUREMENT = {
  audience: "bluey-jobs-managed-cloud-runtime-measurement-v1",
  buildId: "managed-cloud-611-jobs-workflows",
  componentId: "jobs-workflows",
  configSchemaSha256: "c".repeat(64),
  measuredFiles: [{
    path: MEASURED_PATH,
    sha256: createHash("sha256").update(MEASURED_BYTES).digest("hex"),
  }, {
    path: NODE_PATH,
    sha256: createHash("sha256").update(NODE_BYTES).digest("hex"),
  }],
  migrationSetSha256: "d".repeat(64),
  protocolSetSha256: "e".repeat(64),
  roles: ["workflow_gateway", "workflow_worker"],
  sourceCommit: "a".repeat(40),
  version: 1,
};
const MEASUREMENT_PATH = join(
  MEASUREMENT_ROOT,
  "app/.bluey/managed-cloud-runtime-measurement.json",
);
writeFileSync(MEASUREMENT_PATH, JSON.stringify(MEASUREMENT) + "\n");
const MEASUREMENT_OPTIONS = {
  measurementPath: MEASUREMENT_PATH,
  rootPath: MEASUREMENT_ROOT,
};
afterAll(() => rmSync(MEASUREMENT_ROOT, { force: true, recursive: true }));

const BASE_ENV: NodeJS.ProcessEnv = {
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED: "true",
  BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN: "https://jobs-api.internal",
  BLUEY_JOBS_WORKER_SIGNING_KEY: "worker-signing-key-0123456789abcdef",
  BLUEY_JOBS_MANAGED_CLOUD_WORKER_ID: "workflow-gateway-runtime",
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_ID: "cloud-runtime-grant-1234567890",
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_TOKEN: "A".repeat(43),
  BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_INSTANCE_ID: "cloud-runtime-instance-1234567890",
  BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT: "staging",
  BLUEY_JOBS_MANAGED_CLOUD_REGION: "us-east-1",
  BLUEY_JOBS_MANAGED_CLOUD_CHANNEL: "shadow",
};

describe("managed-cloud runtime configuration", () => {
  it("is exact-true and otherwise has no deployment authority", () => {
    expect(managedCloudRuntimeConfig("workflow_gateway", {})).toBeUndefined();
    expect(managedCloudRuntimeConfig("workflow_gateway", {
      ...BASE_ENV,
      BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED: "1",
    })).toBeUndefined();
  });

  it("builds the closed one-time grant claim without artifact self-assertions", () => {
    const config = managedCloudRuntimeConfig(
      "workflow_gateway",
      BASE_ENV,
      MEASUREMENT_OPTIONS,
    );
    expect(config).toMatchObject({
      apiOrigin: "https://jobs-api.internal",
      expectedRole: "workflow_gateway",
      heartbeatIntervalMs: 5_000,
      scope: { environment: "staging", region: "us-east-1", channel: "shadow" },
    });
    expect(managedCloudRuntimeClaimRequest(config!)).toEqual({
      grantId: "cloud-runtime-grant-1234567890",
      grantToken: "A".repeat(43),
      workerId: "workflow-gateway-runtime",
      sessionToken: config!.sessionToken,
      runtimeInstanceId: "cloud-runtime-instance-1234567890",
      runtimeIdentitySha256: config!.runtimeIdentitySha256,
    });
    expect(config!.sessionToken)
      .toBe("0IRP57ngXPzC59kCx1RDz3YzR5srjKmnSnR3B2joUFY");
  });

  it("rejects unsafe origins, malformed grants, digests, and intervals", () => {
    for (const overrides of [
      { BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN: "http://jobs-api.internal" },
      { BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN: "https://user@jobs-api.internal" },
      {
        BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN: "http://127.0.0.1:8080",
        NODE_ENV: "production",
      },
      { BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_TOKEN: "short" },
      { BLUEY_JOBS_MANAGED_CLOUD_WORKER_ID: "invalid.worker.runtime" },
      { BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_IDENTITY_SHA256: "b".repeat(64) },
      {
        BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_GATEWAY_RUNTIME_IDENTITY_SHA256:
          "b".repeat(64),
      },
      { BLUEY_JOBS_MANAGED_CLOUD_REGION: "US-EAST-1" },
      { BLUEY_JOBS_MANAGED_CLOUD_CHANNEL: "beta" },
      { BLUEY_JOBS_MANAGED_CLOUD_HEARTBEAT_SECONDS: "61" },
    ]) {
      expect(() => managedCloudRuntimeConfig(
        "workflow_gateway",
        { ...BASE_ENV, ...overrides },
        MEASUREMENT_OPTIONS,
      )).toThrow();
    }
  });

  it("rehashes measured files and role-separates the local identity", () => {
    const gateway = measureManagedCloudRuntimeIdentity(
      "workflow_gateway",
      MEASUREMENT_OPTIONS,
    );
    const worker = measureManagedCloudRuntimeIdentity(
      "workflow_worker",
      MEASUREMENT_OPTIONS,
    );
    expect(gateway.runtimeIdentitySha256).not.toBe(worker.runtimeIdentitySha256);
    writeFileSync(join(MEASUREMENT_ROOT, MEASURED_PATH), "tampered\n");
    expect(() => measureManagedCloudRuntimeIdentity(
      "workflow_gateway",
      MEASUREMENT_OPTIONS,
    )).toThrow("measured file digest is invalid");
    writeFileSync(join(MEASUREMENT_ROOT, MEASURED_PATH), MEASURED_BYTES);
    const unmeasuredPath = join(
      MEASUREMENT_ROOT,
      "app/workflows/dist/unmeasured.js",
    );
    writeFileSync(unmeasuredPath, "unmeasured-runtime\n");
    expect(() => measureManagedCloudRuntimeIdentity(
      "workflow_gateway",
      MEASUREMENT_OPTIONS,
    )).toThrow("measured file set changed");
    rmSync(unmeasuredPath);
  });

  it("accepts exactly 256 measured files and rejects 257", () => {
    const measuredFiles = Array.from({ length: 254 }, (_, index) => {
      const path = `app/workflows/dist/measured-${index.toString().padStart(3, "0")}.js`;
      const bytes = Buffer.from(`measured-${index}\n`);
      writeFileSync(join(MEASUREMENT_ROOT, path), bytes);
      return {
        path,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      };
    }).concat(MEASUREMENT.measuredFiles).sort((left, right) =>
      Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)),
    );
    writeFileSync(
      MEASUREMENT_PATH,
      JSON.stringify({ ...MEASUREMENT, measuredFiles }) + "\n",
    );
    expect(() => measureManagedCloudRuntimeIdentity(
      "workflow_gateway",
      MEASUREMENT_OPTIONS,
    )).not.toThrow();
    writeFileSync(
      MEASUREMENT_PATH,
      JSON.stringify({
        ...MEASUREMENT,
        measuredFiles: [
          ...measuredFiles,
          { path: "app/workflows/dist/overflow.js", sha256: "f".repeat(64) },
        ],
      }) + "\n",
    );
    expect(() => measureManagedCloudRuntimeIdentity(
      "workflow_gateway",
      MEASUREMENT_OPTIONS,
    )).toThrow("measured file set is invalid");
    writeFileSync(MEASUREMENT_PATH, JSON.stringify(MEASUREMENT) + "\n");
  });
});
