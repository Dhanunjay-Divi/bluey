import { createHash } from "node:crypto";
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import {
  assertManagedCloudRuntimeMeasurement,
  managedCloudRuntimeClaimRequest,
  managedCloudRuntimeConfig,
  measureManagedCloudRuntimeIdentity,
  type ManagedCloudRuntimeMeasurement,
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

function runnerMeasurementFixture() {
  const root = mkdtempSync(join(tmpdir(), "bluey-runner-bundle-"));
  const files: Record<string, Buffer> = {
    "app/automation/dist/index.js": Buffer.from("automation-runtime\n"),
    "app/automation/dist/secondary.js": Buffer.from("automation-secondary\n"),
    "app/automation/package.json": Buffer.from('{"name":"synthetic-automation"}\n'),
    "app/runner/dist/server.js": Buffer.from("runner-runtime\n"),
    "ms-playwright/chromium_headless_shell-123/chrome-headless-shell-linux64/chrome-headless-shell":
      Buffer.from("chromium-runtime\n"),
    "usr/local/bin/node": Buffer.from("node-runtime\n"),
  };
  for (const [path, bytes] of Object.entries(files)) {
    mkdirSync(join(root, path.slice(0, path.lastIndexOf("/"))), { recursive: true });
    writeFileSync(join(root, path), bytes);
  }
  const measurementPath = join(root, "app/.bluey/managed-cloud-runtime-measurement.json");
  mkdirSync(join(root, "app/.bluey"), { recursive: true });
  const measurement: ManagedCloudRuntimeMeasurement = {
    audience: "bluey-jobs-managed-cloud-runtime-measurement-v1",
    buildId: "managed-cloud-639-jobs-runner",
    componentId: "jobs-runner",
    configSchemaSha256: "c".repeat(64),
    measuredFiles: [],
    migrationSetSha256: "d".repeat(64),
    protocolSetSha256: "e".repeat(64),
    roles: ["managed_runner"],
    sourceCommit: "a".repeat(40),
    version: 1,
  };
  const refresh = () => {
    measurement.measuredFiles = Object.keys(files).sort((left, right) =>
      Buffer.compare(Buffer.from(left), Buffer.from(right)),
    ).map((path) => ({ path, sha256: createHash("sha256").update(files[path]).digest("hex") }));
    writeFileSync(measurementPath, JSON.stringify(measurement) + "\n");
  };
  refresh();
  return { root, files, measurement, measurementPath, refresh,
    options: { rootPath: root, measurementPath } };
}

describe("managed runner automation bundle binding", () => {
  it("derives the separate canonical LF domain from all actual automation files", () => {
    const fixture = runnerMeasurementFixture();
    try {
      const entries = fixture.measurement.measuredFiles.filter((entry) =>
        entry.path.startsWith("app/automation/"));
      const expected = createHash("sha256").update(JSON.stringify({
        audience: "bluey-jobs-managed-runner-automation-bundle-v1", entries, version: 1,
      }) + "\n").digest("hex");
      expect(entries).toHaveLength(3);
      expect(measureManagedCloudRuntimeIdentity("managed_runner", fixture.options)
        .automationBundleSha256).toBe(expected);
      expect(managedCloudRuntimeConfig("managed_runner", BASE_ENV, fixture.options)
        ?.automationBundleSha256).toBe(expected);
      expect(measureManagedCloudRuntimeIdentity("workflow_gateway", MEASUREMENT_OPTIONS)
        .automationBundleSha256).toBeUndefined();
    } finally { rmSync(fixture.root, { recursive: true, force: true }); }
  });

  it("changes for coherently rehashed automation bytes but not runner-only bytes", () => {
    const fixture = runnerMeasurementFixture();
    try {
      const measure = () => measureManagedCloudRuntimeIdentity("managed_runner", fixture.options);
      const before = measure();
      const runnerPath = "app/runner/dist/server.js";
      fixture.files[runnerPath] = Buffer.from("runner-successor\n");
      writeFileSync(join(fixture.root, runnerPath), fixture.files[runnerPath]);
      fixture.refresh();
      expect(measure().automationBundleSha256).toBe(before.automationBundleSha256);
      expect(measure().runtimeIdentitySha256).not.toBe(before.runtimeIdentitySha256);
      let digest = measure().automationBundleSha256;
      for (const path of ["app/automation/package.json", "app/automation/dist/index.js",
        "app/automation/dist/secondary.js"]) {
        fixture.files[path] = Buffer.concat([fixture.files[path], Buffer.from("changed\n")]);
        writeFileSync(join(fixture.root, path), fixture.files[path]);
        fixture.refresh();
        const changed = measure().automationBundleSha256;
        expect(changed).not.toBe(digest);
        digest = changed;
      }
    } finally { rmSync(fixture.root, { recursive: true, force: true }); }
  });

  it("rejects either required automation file even after coherent inventory removal", () => {
    for (const path of ["app/automation/package.json", "app/automation/dist/index.js"]) {
      const fixture = runnerMeasurementFixture();
      try {
        rmSync(join(fixture.root, path));
        delete fixture.files[path];
        fixture.refresh();
        expect(() => measureManagedCloudRuntimeIdentity("managed_runner", fixture.options))
          .toThrow("automation bundle is incomplete");
      } finally { rmSync(fixture.root, { recursive: true, force: true }); }
    }
  });

  it("rejects tampered, unmeasured and wrong-role runner contents", () => {
    for (const mutation of ["tamper", "unmeasured", "role"] as const) {
      const fixture = runnerMeasurementFixture();
      try {
        if (mutation === "tamper") {
          writeFileSync(join(fixture.root, "app/automation/dist/index.js"), "tampered\n");
        } else if (mutation === "unmeasured") {
          writeFileSync(join(fixture.root, "app/automation/dist/unmeasured.js"), "extra\n");
        } else {
          fixture.measurement.roles = ["managed_runner", "workflow_worker"];
          fixture.refresh();
        }
        expect(() => measureManagedCloudRuntimeIdentity("managed_runner", fixture.options)).toThrow();
      } finally { rmSync(fixture.root, { recursive: true, force: true }); }
    }
  });

  it("rechecks the derived bundle field instead of accepting a configured digest", () => {
    const fixture = runnerMeasurementFixture();
    try {
      const config = managedCloudRuntimeConfig("managed_runner", BASE_ENV, fixture.options)!;
      expect(() => assertManagedCloudRuntimeMeasurement(config)).not.toThrow();
      expect(() => assertManagedCloudRuntimeMeasurement({ ...config,
        automationBundleSha256: "f".repeat(64) })).toThrow("measurement changed");
      expect(() => assertManagedCloudRuntimeMeasurement({ ...config,
        automationBundleSha256: undefined })).toThrow("measurement changed");
    } finally { rmSync(fixture.root, { recursive: true, force: true }); }
  });
});

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

  it("accepts the measured successor verifier only in jobs-workflows", () => {
    const successor = {
      ...MEASUREMENT,
      roles: [
        "original_source_verifier",
        "workflow_gateway",
        "workflow_worker",
      ],
    };
    writeFileSync(MEASUREMENT_PATH, JSON.stringify(successor) + "\n");
    const verifier = measureManagedCloudRuntimeIdentity(
      "original_source_verifier",
      MEASUREMENT_OPTIONS,
    );
    const worker = measureManagedCloudRuntimeIdentity(
      "workflow_worker",
      MEASUREMENT_OPTIONS,
    );
    expect(verifier.componentId).toBe("jobs-workflows");
    expect(verifier.runtimeIdentitySha256).not.toBe(worker.runtimeIdentitySha256);
    writeFileSync(MEASUREMENT_PATH, JSON.stringify(MEASUREMENT) + "\n");
  });

  it("measures the normalized regular managed runner Node binary", () => {
    const root = mkdtempSync(join(tmpdir(), "bluey-runner-measurement-"));
    const files = [
      ["app/automation/dist/index.js", Buffer.from("automation-runtime\n")],
      ["app/automation/package.json", Buffer.from('{"name":"synthetic-automation"}\n')],
      ["app/runner/dist/server.js", Buffer.from("runner-runtime\n")],
      [
        "ms-playwright/chromium_headless_shell-123/" +
          "chrome-headless-shell-linux64/chrome-headless-shell",
        Buffer.from("chromium-runtime\n"),
      ],
      ["usr/local/bin/node", Buffer.from("node-runtime\n")],
    ] as const;
    try {
      for (const [path, bytes] of files) {
        mkdirSync(join(root, path.slice(0, path.lastIndexOf("/"))), {
          recursive: true,
        });
        writeFileSync(join(root, path), bytes);
      }
      const measuredFiles = files.map(([path, bytes]) => ({
        path,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      }));
      const measurementPath = join(
        root,
        "app/.bluey/managed-cloud-runtime-measurement.json",
      );
      mkdirSync(join(root, "app/.bluey"), { recursive: true });
      writeFileSync(measurementPath, JSON.stringify({
        audience: "bluey-jobs-managed-cloud-runtime-measurement-v1",
        buildId: "managed-cloud-611-jobs-runner",
        componentId: "jobs-runner",
        configSchemaSha256: "c".repeat(64),
        measuredFiles,
        migrationSetSha256: "d".repeat(64),
        protocolSetSha256: "e".repeat(64),
        roles: ["managed_runner"],
        sourceCommit: "a".repeat(40),
        version: 1,
      }) + "\n");

      expect(measureManagedCloudRuntimeIdentity("managed_runner", {
        measurementPath,
        rootPath: root,
      })).toMatchObject({ componentId: "jobs-runner" });
      rmSync(join(root, "usr/local/bin/node"));
      expect(() => measureManagedCloudRuntimeIdentity("managed_runner", {
        measurementPath,
        rootPath: root,
      })).toThrow("measured runtime root is missing");
      mkdirSync(join(root, "usr/bin"), { recursive: true });
      writeFileSync(join(root, "usr/bin/node-target"), "node-runtime\n");
      symlinkSync(
        join(root, "usr/bin/node-target"),
        join(root, "usr/local/bin/node"),
      );
      expect(() => measureManagedCloudRuntimeIdentity("managed_runner", {
        measurementPath,
        rootPath: root,
      })).toThrow("measured runtime roots cannot contain symbolic links");
    } finally {
      rmSync(root, { force: true, recursive: true });
    }
  });

  it("accepts exactly 512 measured files and rejects 513", () => {
    const measuredFiles = Array.from({ length: 510 }, (_, index) => {
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
