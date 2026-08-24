import {
  createHash,
  createPublicKey,
  verify as verifySignature,
} from "node:crypto";
import { createReadStream } from "node:fs";
import {
  copyFile,
  cp,
  lstat,
  mkdir,
  open,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import {
  basename,
  dirname,
  join,
  posix,
  relative,
  resolve,
  sep,
} from "node:path";
import { pathToFileURL } from "node:url";

export const MANAGED_CLOUD_AUDIENCES = Object.freeze({
  activation: "bluey-jobs-managed-cloud-activation-v1",
  authorization: "bluey-jobs-managed-cloud-authorization-v1",
  builderPolicy: "bluey-jobs-managed-cloud-builder-policy-v1",
  candidate: "bluey-jobs-managed-cloud-candidate-v1",
  canaryEvidence: "bluey-jobs-managed-cloud-canary-evidence-v1",
  cleanupEvidence: "bluey-jobs-managed-cloud-cleanup-evidence-v1",
  cohort: "bluey-jobs-managed-cloud-cohort-v1",
  componentInventory: "bluey-jobs-managed-cloud-component-inventory-v1",
  configContract: "bluey-jobs-managed-cloud-config-contract-v1",
  contentInventory: "bluey-jobs-managed-cloud-content-inventory-v1",
  failureConverterEvidence:
    "bluey-jobs-managed-cloud-failure-converter-evidence-v1",
  manifest: "bluey-jobs-managed-cloud-release-v1",
  migrationContract: "bluey-jobs-managed-cloud-migration-contract-v1",
  promotion: "bluey-jobs-managed-cloud-promotion-v1",
  portalReadbackEvidence:
    "bluey-jobs-managed-cloud-portal-readback-evidence-v1",
  provenance: "bluey-jobs-managed-cloud-provenance-v1",
  protocolContract: "bluey-jobs-managed-cloud-protocol-contract-v1",
  rollback: "bluey-jobs-managed-cloud-rollback-v1",
  rootAnchor: "bluey-jobs-managed-cloud-root-anchor-v1",
  runtimeMeasurement:
    "bluey-jobs-managed-cloud-runtime-measurement-v1",
  runnerFleetEvidence: "bluey-jobs-managed-cloud-runner-fleet-evidence-v1",
  sbom: "bluey-jobs-managed-cloud-file-sbom-v1",
  signatureSet: "bluey-jobs-managed-cloud-signature-set-v1",
  testEvidence: "bluey-jobs-managed-cloud-test-evidence-v1",
  taskQueueEvidence: "bluey-jobs-managed-cloud-task-queue-evidence-v1",
  temporalEvidence: "bluey-jobs-managed-cloud-temporal-evidence-v1",
  trustBundle: "bluey-jobs-managed-cloud-trust-bundle-v1",
  trustPolicy: "bluey-jobs-managed-cloud-trust-policy-v1",
  treeInventory: "bluey-jobs-managed-cloud-static-tree-inventory-v1",
  storageEvidence: "bluey-jobs-managed-cloud-storage-evidence-v1",
  verificationEvidence:
    "bluey-jobs-managed-cloud-verification-evidence-v1",
});

export const MANAGED_CLOUD_COMPONENTS = Object.freeze([
  Object.freeze({
    architecture: "x86_64",
    artifactKind: "oci_image",
    componentId: "jobs-api",
    platform: "linux",
  }),
  Object.freeze({
    architecture: "wasm",
    artifactKind: "static_bundle",
    componentId: "jobs-portal",
    platform: "web",
  }),
  Object.freeze({
    architecture: "x86_64",
    artifactKind: "oci_image",
    componentId: "jobs-runner",
    platform: "linux",
  }),
  Object.freeze({
    architecture: "x86_64",
    artifactKind: "oci_image",
    componentId: "jobs-workflows",
    platform: "linux",
  }),
]);

export const MANAGED_CLOUD_BASE_CAPABILITIES = Object.freeze([
  Object.freeze({
    capability: "jobs_api",
    componentId: "jobs-api",
  }),
  Object.freeze({
    capability: "workflow_cleanup_dispatcher",
    componentId: "jobs-api",
  }),
  Object.freeze({
    componentId: "jobs-api",
    capability: "workflow_command_dispatcher",
  }),
  Object.freeze({
    capability: "portal_static",
    componentId: "jobs-portal",
  }),
  Object.freeze({
    capability: "managed_runner",
    componentId: "jobs-runner",
  }),
  Object.freeze({
    capability: "workflow_gateway",
    componentId: "jobs-workflows",
  }),
  Object.freeze({
    capability: "workflow_worker",
    componentId: "jobs-workflows",
  }),
]);

export const MANAGED_CLOUD_BASE_RUNTIME_ROLES = Object.freeze([
  "jobs_api",
  "managed_runner",
  "workflow_cleanup_dispatcher",
  "workflow_command_dispatcher",
  "workflow_gateway",
  "workflow_worker",
]);

export const MANAGED_CLOUD_RESERVED_CAPABILITIES = Object.freeze([
  "discovery_worker",
  "global_discovery_worker",
  "original_source_verifier",
]);

export const MANAGED_CLOUD_RUNTIME_CONTRACTS = Object.freeze({
  "jobs-api": Object.freeze({
    cmd: Object.freeze([]),
    entrypoint: Object.freeze(["/usr/local/bin/bluey-jobs-api"]),
    exposedPorts: Object.freeze(["8081/tcp"]),
    requiredEnvironment: Object.freeze([
      "BLUEY_JOBS_API_HOST=0.0.0.0",
      "BLUEY_JOBS_API_PORT=8081",
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    ]),
    requiredPaths: Object.freeze([
      "app/.bluey/managed-cloud-runtime-measurement.json",
      "usr/local/bin/bluey-jobs-api",
    ]),
    runtimeUser: "65532:65532",
    workingDirectory: "/app",
  }),
  "jobs-runner": Object.freeze({
    cmd: Object.freeze(["/usr/local/bin/node", "runner/dist/server.js"]),
    entrypoint: Object.freeze([]),
    exposedPorts: Object.freeze(["8091/tcp"]),
    requiredEnvironment: Object.freeze([
      "BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner",
      "NODE_ENV=production",
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
      "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright",
    ]),
    requiredPaths: Object.freeze([
      "app/.bluey/managed-cloud-runtime-measurement.json",
      "app/runner/dist/native/bluey_jobs_runner_native_storage.node",
      "app/runner/dist/server.js",
      "usr/local/bin/node",
    ]),
    runtimeUser: "pwuser",
    workingDirectory: "/app",
  }),
  "jobs-workflows": Object.freeze({
    cmd: Object.freeze(["/usr/local/bin/node", "workflows/dist/worker.js"]),
    entrypoint: Object.freeze([]),
    exposedPorts: Object.freeze([]),
    requiredEnvironment: Object.freeze([
      "NODE_ENV=production",
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    ]),
    requiredPaths: Object.freeze([
      "app/.bluey/managed-cloud-runtime-measurement.json",
      "app/workflows/dist/discovery-worker.js",
      "app/workflows/dist/failure-converter.js",
      "app/workflows/dist/gateway.js",
      "app/workflows/dist/global-discovery-worker.js",
      "app/workflows/dist/worker.js",
      "usr/local/bin/node",
    ]),
    runtimeUser: "node",
    workingDirectory: "/app",
  }),
});

export const MANAGED_CLOUD_PROTOCOL_IDS = Object.freeze([
  "ats_certification",
  "execution_lease",
  "gateway_command",
  "managed_cloud_release",
  "object_evidence",
  "runner_checkpoint",
  "runner_profile_snapshot",
  "runner_result",
  "runtime_heartbeat",
  "workflow_cleanup",
  "workflow_command",
]);

const MANAGED_CLOUD_BASE_CANARY_CHECK_IDS = Object.freeze([
  "artifact-registry-readback",
  "jobs-api-readiness",
  "jobs-api-runtime-artifact-attestation",
  "jobs-portal-readback",
  "jobs-workflows-runtime-artifact-attestation",
  "managed-runner-readiness",
  "managed-runner-runtime-artifact-attestation",
  "migration-contract-readback",
  "protocol-contract-readback",
  "read-only-rootfs-policy",
  "workflow-cleanup-dispatcher-readiness",
  "workflow-command-dispatcher-readiness",
  "workflow-gateway-readiness",
  "workflow-worker-readiness",
]);

const HEX_64 = /^[0-9a-f]{64}$/;
const SHA256_DIGEST = /^sha256:[0-9a-f]{64}$/;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/;
const SAFE_ID = /^[a-z0-9][a-z0-9._:-]{2,127}$/;
const BASE64URL = /^[A-Za-z0-9_-]+$/;
const IMAGE_REFERENCE =
  /^[a-z0-9][a-z0-9._:-]*(?:\/[a-z0-9][a-z0-9._-]*)+@sha256:[0-9a-f]{64}$/;
const MAX_JSON_BYTES = 16 * 1024 * 1024;
const MAX_SIGNED_OBJECT_BYTES = 128 * 1024;
const MAX_INVENTORY_ATTACHMENT_BYTES = 12 * 1024 * 1024;
const MAX_INVENTORY_ATTACHMENT_TRANSPORT_BYTES =
  16 * 1024 * 1024 + 1_024;
const MAX_INVENTORY_ENTRIES = 100_000;
const MAX_ACTIVATION_EVIDENCE_FILE_BYTES = 128 * 1024;
const MAX_ACTIVATION_EVIDENCE_TOTAL_BYTES = 1024 * 1024;
const MAX_RUNTIME_MEASUREMENT_FILES = 512;
const MAX_SAFE_GENERATION = Number.MAX_SAFE_INTEGER;
const RUNTIME_MEASUREMENT_PATH =
  "app/.bluey/managed-cloud-runtime-measurement.json";
const RUNTIME_IDENTITY_DOMAIN =
  "bluey-jobs-managed-cloud-runtime-identity-v1\0";
const SQLITE_MIGRATION_HEAD =
  "055_jobs_managed_cloud_release_authority.sql";
const POSTGRES_MIGRATION_HEAD =
  "033_jobs_managed_cloud_release_authority.sql";
const JOBS_LOCK_EVIDENCE_PATH = "evidence/build/jobs-package-lock.json";
const SERVER_LOCK_EVIDENCE_PATH = "evidence/build/server-cargo-lock";
const REQUIRED_CANDIDATE_FILES = Object.freeze([
  "builder-policy.json",
  "candidate-set.json",
  "component-inventory.json",
  "config-contract.json",
  "inventory-attachments.json",
  "migration-contract.json",
  "protocol-contract.json",
  "release-manifest.json",
  "test-evidence.json",
  "verification-evidence.json",
]);
const REQUIRED_AUTHORIZED_FILES = Object.freeze([
  "authorization-set.json",
  "candidate",
  "release-signatures.json",
  "trust-bundle.json",
]);
const REQUIRED_PROMOTION_FILES = Object.freeze([
  "activation-signatures.json",
  "activation.json",
  "authorized-candidate",
  "cohort-signatures.json",
  "cohort.json",
  "evidence",
  "promotion-set.json",
  "published",
  "readback",
]);
const REQUIRED_ROLLBACK_FILES = Object.freeze([
  "from-promotion",
  "rollback-signatures.json",
  "rollback-set.json",
  "rollback.json",
  "target-promotion",
]);

const PROTOCOL_SPECS = Object.freeze([
  Object.freeze({
    id: "ats_certification",
    sourcePaths: Object.freeze([
      "jobs/automation/src/certified-submit-form.ts",
      "server/src/db/jobs/ats_certification_authority.rs",
    ]),
    version: 1,
  }),
  Object.freeze({
    id: "execution_lease",
    sourcePaths: Object.freeze([
      "jobs/runner/src/execution-lease.ts",
      "server/src/db/jobs/execution_leases.rs",
    ]),
    version: 1,
  }),
  Object.freeze({
    id: "gateway_command",
    sourcePaths: Object.freeze([
      "jobs/workflows/src/contracts.ts",
      "jobs/workflows/src/gateway-service.ts",
      "jobs/workflows/src/gateway.ts",
      "server/src/jobs_workflow_dispatch.rs",
    ]),
    version: 3,
  }),
  Object.freeze({
    id: "managed_cloud_release",
    sourcePaths: Object.freeze([
      "jobs/automation/src/managed-cloud-execution.ts",
      "jobs/automation/src/managed-cloud-runtime-client.ts",
      "jobs/automation/src/managed-cloud-runtime.ts",
      "jobs/workflows/src/contracts.ts",
      "jobs/workflows/src/gateway-cleanup-service.ts",
      "jobs/workflows/src/gateway-service.ts",
      "jobs/workflows/src/workflows.ts",
      "server/src/db/jobs/managed_cloud_release_authority.rs",
      "server/src/db/jobs/workflow_commands.rs",
      "server/src/jobs_workflow_dispatch.rs",
    ]),
    version: 1,
  }),
  Object.freeze({
    id: "object_evidence",
    sourcePaths: Object.freeze([
      "server/src/db/object_uploads.rs",
      "server/src/db/jobs/evidence.rs",
    ]),
    version: 1,
  }),
  Object.freeze({
    id: "runner_checkpoint",
    sourcePaths: Object.freeze([
      "jobs/runner/src/run-checkpoint-store.ts",
      "server/src/db/jobs/execution_leases.rs",
    ]),
    version: 2,
  }),
  Object.freeze({
    id: "runner_profile_snapshot",
    sourcePaths: Object.freeze([
      "jobs/runner/src/profile-snapshot-client.ts",
      "server/src/db/jobs/browser_profile_snapshots.rs",
    ]),
    version: 1,
  }),
  Object.freeze({
    id: "runner_result",
    sourcePaths: Object.freeze([
      "jobs/runner/src/result-store.ts",
      "jobs/runner/src/submitted-result-recovery.ts",
      "server/src/db/jobs/local_runner.rs",
    ]),
    version: 2,
  }),
  Object.freeze({
    id: "runtime_heartbeat",
    sourcePaths: Object.freeze([
      "jobs/automation/src/managed-cloud-runtime-client.ts",
      "jobs/automation/src/managed-cloud-runtime.ts",
      "server/src/jobs_managed_cloud_runtime.rs",
      "server/src/api/jobs_worker_auth.rs",
    ]),
    version: 1,
  }),
  Object.freeze({
    id: "workflow_cleanup",
    sourcePaths: Object.freeze([
      "jobs/workflows/src/gateway-cleanup-service.ts",
      "jobs/workflows/src/gateway.ts",
      "server/src/jobs_workflow_cleanup.rs",
    ]),
    version: 3,
  }),
  Object.freeze({
    id: "workflow_command",
    sourcePaths: Object.freeze([
      "jobs/workflows/src/contracts.ts",
      "jobs/workflows/src/gateway-service.ts",
      "jobs/workflows/src/workflows.ts",
      "server/src/db/jobs/workflow_commands.rs",
      "jobs/workflows/src/gateway.ts",
      "server/src/jobs_workflow_dispatch.rs",
    ]),
    version: 2,
  }),
]);

const CONFIG_ROLE_ROOTS = Object.freeze([
  Object.freeze({
    paths: Object.freeze(["server/src"]),
    role: "jobs_api",
  }),
  Object.freeze({
    paths: Object.freeze(["jobs/automation/src", "jobs/workflows/src"]),
    role: "jobs_workflows",
  }),
  Object.freeze({
    paths: Object.freeze(["jobs/automation/src", "jobs/runner/src"]),
    role: "managed_runner",
  }),
  Object.freeze({
    paths: Object.freeze(["jobs/portal/src", "jobs/portal/vite.config.ts"]),
    role: "portal_static",
  }),
]);

const MANAGED_CLOUD_SHARED_CONFIG_ROLES = Object.freeze([
  "jobs_api",
  "jobs_workflows",
  "managed_runner",
]);
const MANAGED_CLOUD_WORKER_CONFIG_ROLES = Object.freeze([
  "jobs_workflows",
  "managed_runner",
]);
const MANAGED_CLOUD_JOBS_API_CONFIG_ROLES = Object.freeze(["jobs_api"]);
const MANAGED_CLOUD_JOBS_API_AND_WORKFLOWS_CONFIG_ROLES = Object.freeze([
  "jobs_api",
  "jobs_workflows",
]);
const MANAGED_CLOUD_REQUIRED_CONFIG_SPECS = new Map([
  [
    "BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN",
    Object.freeze({
      classification: "configuration",
      requiredRoles: MANAGED_CLOUD_SHARED_CONFIG_ROLES,
      valueType: "https_url",
    }),
  ],
  ...["CHANNEL", "ENVIRONMENT", "REGION"].map((suffix) => [
    "BLUEY_JOBS_MANAGED_CLOUD_" + suffix,
    Object.freeze({
      classification: "configuration",
      requiredRoles: MANAGED_CLOUD_SHARED_CONFIG_ROLES,
      valueType: "canonical_id",
    }),
  ]),
  [
    "BLUEY_JOBS_MANAGED_CLOUD_HEARTBEAT_SECONDS",
    Object.freeze({
      classification: "configuration",
      requiredRoles: MANAGED_CLOUD_SHARED_CONFIG_ROLES,
      valueType: "positive_integer",
    }),
  ],
  [
    "BLUEY_JOBS_WORKER_SIGNING_KEY",
    Object.freeze({
      classification: "secret",
      requiredRoles: MANAGED_CLOUD_SHARED_CONFIG_ROLES,
      valueType: "secret_bytes",
    }),
  ],
  ...[
    "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
    "BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED",
    "BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED",
  ].map((name) => [
    name,
    Object.freeze({
      classification: "deny_only_authority",
      requiredRoles: MANAGED_CLOUD_JOBS_API_CONFIG_ROLES,
      valueType: "boolean_true",
    }),
  ]),
  [
    "BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED",
    Object.freeze({
      classification: "deny_only_authority",
      requiredRoles: MANAGED_CLOUD_JOBS_API_AND_WORKFLOWS_CONFIG_ROLES,
      valueType: "boolean_true",
    }),
  ],
  [
    "BLUEY_JOBS_WORKFLOW_ORIGIN",
    Object.freeze({
      classification: "configuration",
      requiredRoles: MANAGED_CLOUD_JOBS_API_CONFIG_ROLES,
      valueType: "https_url",
    }),
  ],
  [
    "BLUEY_JOBS_WORKFLOW_TOKEN",
    Object.freeze({
      classification: "secret",
      requiredRoles: MANAGED_CLOUD_JOBS_API_AND_WORKFLOWS_CONFIG_ROLES,
      valueType: "secret_bytes",
    }),
  ],
  [
    "BLUEY_JOBS_WORKFLOW_NAMESPACE",
    Object.freeze({
      classification: "configuration",
      requiredRoles: MANAGED_CLOUD_JOBS_API_AND_WORKFLOWS_CONFIG_ROLES,
      valueType: "canonical_id",
    }),
  ],
  [
    "BLUEY_JOBS_WORKFLOW_CLEANUP_VISIBILITY_CUTOFF_MS",
    Object.freeze({
      classification: "configuration",
      requiredRoles: MANAGED_CLOUD_JOBS_API_CONFIG_ROLES,
      valueType: "positive_integer",
    }),
  ],
  [
    "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED",
    Object.freeze({
      classification: "deny_only_authority",
      requiredRoles: MANAGED_CLOUD_SHARED_CONFIG_ROLES,
      valueType: "boolean_true",
    }),
  ],
  ...[
    ["RUNTIME_GRANT_ID", "canonical_id", "configuration"],
    ["RUNTIME_GRANT_TOKEN", "secret_bytes", "secret"],
    ["RUNTIME_INSTANCE_ID", "canonical_id", "configuration"],
    ["WORKER_ID", "canonical_id", "configuration"],
  ].map(([suffix, valueType, classification]) => [
    "BLUEY_JOBS_MANAGED_CLOUD_" + suffix,
    Object.freeze({
      classification,
      requiredRoles: MANAGED_CLOUD_WORKER_CONFIG_ROLES,
      valueType,
    }),
  ]),
  ...[
    "JOBS_API",
    "WORKFLOW_CLEANUP_DISPATCHER",
    "WORKFLOW_COMMAND_DISPATCHER",
  ].flatMap((role) =>
    [
      ["GRANT_ID", "canonical_id", "configuration"],
      ["GRANT_TOKEN", "secret_bytes", "secret"],
      ["RUNTIME_INSTANCE_ID", "canonical_id", "configuration"],
      ["WORKER_ID", "canonical_id", "configuration"],
    ].map(([suffix, valueType, classification]) => [
      "BLUEY_JOBS_MANAGED_CLOUD_" + role + "_" + suffix,
      Object.freeze({
        classification,
        requiredRoles: Object.freeze(["jobs_api"]),
        valueType,
      }),
    ]),
  ),
]);
const MANAGED_CLOUD_FORBIDDEN_CONFIG_SUFFIX = "_RUNTIME_IDENTITY_SHA256";

const BUILDER_POLICY_PATHS = Object.freeze([
  ".github/workflows/jobs-managed-cloud-release.yml",
  "jobs/.dockerignore",
  "jobs/automation/package.json",
  "jobs/package-lock.json",
  "jobs/package.json",
  "jobs/portal/package.json",
  "jobs/runner/Dockerfile",
  "jobs/runner/native-storage/Cargo.lock",
  "jobs/runner/native-storage/Cargo.toml",
  "jobs/runner/package.json",
  "jobs/scripts/managed-cloud-release-gate.mjs",
  "jobs/workflows/Dockerfile",
  "jobs/workflows/package.json",
  "server/Cargo.lock",
  "server/Dockerfile.jobs",
  "server/Dockerfile.jobs.dockerignore",
]);

export class ManagedCloudReleaseGateError extends Error {
  constructor(message = "Managed cloud release gate rejected the input") {
    super(message);
    this.name = "ManagedCloudReleaseGateError";
  }
}

function fail(message) {
  throw new ManagedCloudReleaseGateError(message);
}

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    fail(label + " must be an object");
  }
  return value;
}

function requireExactKeys(value, expected, label) {
  requireObject(value, label);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    fail(label + " must contain exactly: " + wanted.join(", "));
  }
}

function requireArray(value, label, minimum = 0) {
  if (!Array.isArray(value) || value.length < minimum) {
    fail(label + " must be an array with at least " + minimum + " entries");
  }
  return value;
}

function requireBoundedArray(value, label, minimum, maximum) {
  const array = requireArray(value, label, minimum);
  if (array.length > maximum) {
    fail(label + " must contain at most " + maximum + " entries");
  }
  return array;
}

function requireSignedObjectSize(value, label) {
  if (canonicalJsonBytes(value).length > MAX_SIGNED_OBJECT_BYTES) {
    fail(label + " exceeds the 128 KiB signed-object limit");
  }
}

function requireString(value, label) {
  if (typeof value !== "string" || value.trim() !== value || value.length === 0) {
    fail(label + " must be a nonempty canonical string");
  }
  return value;
}

function requireSafeId(value, label) {
  const id = requireString(value, label);
  if (!SAFE_ID.test(id)) {
    fail(label + " is not a safe identifier");
  }
  return id;
}

function requireHex64(value, label) {
  if (typeof value !== "string" || !HEX_64.test(value)) {
    fail(label + " must be 64 lowercase hexadecimal characters");
  }
  return value;
}

function requireSourceCommit(value) {
  if (typeof value !== "string" || !SOURCE_COMMIT.test(value)) {
    fail("sourceCommit must be 40 lowercase hexadecimal characters");
  }
  return value;
}

function requireDigest(value, label) {
  if (typeof value !== "string" || !SHA256_DIGEST.test(value)) {
    fail(label + " must be an immutable sha256 digest");
  }
  return value;
}

function requireInteger(value, label, minimum = 0) {
  if (
    !Number.isSafeInteger(value) ||
    value < minimum ||
    value > MAX_SAFE_GENERATION
  ) {
    fail(label + " must be a safe integer at least " + minimum);
  }
  return value;
}

function requireBoolean(value, label) {
  if (typeof value !== "boolean") {
    fail(label + " must be a boolean");
  }
  return value;
}

function requireEnum(value, values, label) {
  if (!values.includes(value)) {
    fail(label + " must be exactly one of: " + values.join(", "));
  }
  return value;
}

function requireSortedUniqueStrings(values, label) {
  requireArray(values, label);
  const copy = values.map((value, index) =>
    requireString(value, label + "[" + index + "]"),
  );
  const sorted = [...copy].sort();
  if (
    new Set(copy).size !== copy.length ||
    copy.some((value, index) => value !== sorted[index])
  ) {
    fail(label + " must be sorted and unique");
  }
  return copy;
}

function compareUtf8(left, right) {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function canonicalize(value) {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (value === null || typeof value === "boolean" || typeof value === "string") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || Object.is(value, -0)) {
      fail("Canonical JSON permits only safe non-negative-zero integers");
    }
    return value;
  }
  if (value && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype) {
    const result = {};
    const keys = Object.keys(value).sort(compareUtf8);
    for (const key of keys) {
      result[key] = canonicalize(value[key]);
    }
    return result;
  }
  fail("Canonical JSON permits only primitives, arrays, and plain objects");
}

export function canonicalJsonBytes(value) {
  return Buffer.from(JSON.stringify(canonicalize(value)) + "\n", "utf8");
}

export function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function measurementRoots(componentId) {
  if (componentId === "jobs-api") {
    return ["usr/local/bin/bluey-jobs-api"];
  }
  if (componentId === "jobs-workflows") {
    return ["app/automation", "app/workflows", "usr/local/bin/node"];
  }
  if (componentId === "jobs-runner") {
    return [
      "app/automation",
      "app/runner",
      "ms-playwright",
      "usr/local/bin/node",
    ];
  }
  fail("Runtime measurement supports only OCI runtime components");
}

function expectedMeasurementPaths(componentId, inventory) {
  return inventory.entries
    .filter(
      (entry) =>
        entry.type === "file" &&
        entry.sizeBytes > 0 &&
        entry.path !== RUNTIME_MEASUREMENT_PATH &&
        measurementPathSelected(componentId, entry.path),
    )
    .map((entry) => entry.path)
    .sort(compareUtf8);
}

function measurementPathSelected(componentId, path) {
  return measurementRoots(componentId).some(
    (root) => path === root || path.startsWith(root + "/"),
  );
}

function isManagedChromiumExecutablePath(path) {
  return /^ms-playwright\/chromium_headless_shell-[0-9]+\/[a-z0-9._-]+\/chrome-headless-shell$/
    .test(path);
}

function expectedRuntimeRolesForComponent(componentId, capabilities) {
  const roles = capabilities
    .filter(
      (capability) =>
        capability.componentId === componentId &&
        capability.capability !== "portal_static",
    )
    .map((capability) => capability.capability)
    .sort(compareUtf8);
  if (componentId === "jobs-portal") {
    if (roles.length !== 0) {
      fail("Static portal cannot claim a runtime identity");
    }
    return roles;
  }
  if (roles.length < 1) {
    fail(componentId + " must claim at least one runtime role");
  }
  return roles;
}

function runtimeMeasurementDocument({
  buildId,
  componentId,
  configSchemaSha256,
  inventory,
  migrationSetSha256,
  protocolSetSha256,
  roles,
  sourceCommit,
}) {
  if (componentId === "jobs-portal") {
    fail("Static portal cannot contain a runtime measurement");
  }
  const inventoryEntries = new Map(
    inventory.entries.map((entry) => [entry.path, entry]),
  );
  const measuredPaths = expectedMeasurementPaths(componentId, inventory);
  if (measuredPaths.length < 1 || measuredPaths.length > MAX_RUNTIME_MEASUREMENT_FILES) {
    fail(componentId + " runtime measurement file set is invalid");
  }
  const measuredFiles = measuredPaths.map((path) => {
    const entry = inventoryEntries.get(path);
    if (!entry || entry.type !== "file" || entry.sizeBytes < 1) {
      fail(componentId + " runtime measurement contains an invalid file");
    }
    return { path, sha256: entry.sha256 };
  });
  return {
    audience: MANAGED_CLOUD_AUDIENCES.runtimeMeasurement,
    buildId: requireSafeId(buildId, componentId + ".measurement.buildId"),
    componentId,
    configSchemaSha256: requireHex64(
      configSchemaSha256,
      componentId + ".measurement.configSchemaSha256",
    ),
    measuredFiles,
    migrationSetSha256: requireHex64(
      migrationSetSha256,
      componentId + ".measurement.migrationSetSha256",
    ),
    protocolSetSha256: requireHex64(
      protocolSetSha256,
      componentId + ".measurement.protocolSetSha256",
    ),
    roles: requireSortedUniqueStrings(
      roles,
      componentId + ".measurement.roles",
    ),
    sourceCommit: requireSourceCommit(sourceCommit),
    version: 1,
  };
}

export function deriveManagedCloudRuntimeIdentitySha256(
  measurementSha256,
  componentId,
  role,
) {
  requireHex64(measurementSha256, "runtime measurement SHA-256");
  requireSafeId(componentId, "runtime measurement componentId");
  requireEnum(role, [
    "discovery_worker",
    "global_discovery_worker",
    "jobs_api",
    "managed_runner",
    "workflow_cleanup_dispatcher",
    "workflow_command_dispatcher",
    "workflow_gateway",
    "workflow_worker",
  ], "runtime measurement role");
  return sha256(
    Buffer.concat([
      Buffer.from(RUNTIME_IDENTITY_DOMAIN, "utf8"),
      Buffer.from(measurementSha256, "ascii"),
      Buffer.from("\0", "utf8"),
      Buffer.from(componentId, "utf8"),
      Buffer.from("\0", "utf8"),
      Buffer.from(role, "utf8"),
    ]),
  );
}

export function validateManagedCloudRuntimeMeasurement(
  measurement,
  measurementSha256,
  runtimeIdentities,
  context,
) {
  requireExactKeys(
    measurement,
    [
      "audience",
      "buildId",
      "componentId",
      "configSchemaSha256",
      "measuredFiles",
      "migrationSetSha256",
      "protocolSetSha256",
      "roles",
      "sourceCommit",
      "version",
    ],
    context.componentId + " runtime measurement",
  );
  const expected = runtimeMeasurementDocument(context);
  const bytes = canonicalJsonBytes(measurement);
  if (
    bytes.length > MAX_SIGNED_OBJECT_BYTES ||
    bytes.compare(canonicalJsonBytes(expected)) !== 0 ||
    measurementSha256 !== sha256(bytes)
  ) {
    fail(context.componentId + " runtime measurement is not exact");
  }
  const embedded = context.inventory.entries.find(
    (entry) => entry.path === RUNTIME_MEASUREMENT_PATH,
  );
  if (
    !embedded ||
    embedded.type !== "file" ||
    embedded.sizeBytes !== bytes.length ||
    embedded.sha256 !== measurementSha256
  ) {
    fail(context.componentId + " embedded runtime measurement bytes do not match");
  }
  const identities = requireArray(
    runtimeIdentities,
    context.componentId + ".runtimeIdentities",
    expected.roles.length,
  );
  if (identities.length !== expected.roles.length) {
    fail(context.componentId + " runtime identity set is incomplete");
  }
  for (const [index, role] of expected.roles.entries()) {
    const identity = requireObject(
      identities[index],
      context.componentId + ".runtimeIdentities[" + index + "]",
    );
    requireExactKeys(
      identity,
      ["role", "runtimeIdentitySha256"],
      context.componentId + ".runtimeIdentities[" + index + "]",
    );
    if (
      identity.role !== role ||
      identity.runtimeIdentitySha256 !==
        deriveManagedCloudRuntimeIdentitySha256(
          measurementSha256,
          context.componentId,
          role,
        )
    ) {
      fail(context.componentId + " runtime identity is not locally derived");
    }
  }
  return measurement;
}

function validateContentInventoryDocument(inventory, componentId) {
  requireExactKeys(
    inventory,
    [
      "artifactKind",
      "artifactSha256",
      "audience",
      "componentId",
      "entries",
      "runtime",
      "version",
    ],
    componentId + " content inventory",
  );
  const expected = MANAGED_CLOUD_COMPONENTS.find(
    (component) => component.componentId === componentId,
  );
  if (
    !expected ||
    inventory.version !== 1 ||
    inventory.audience !== MANAGED_CLOUD_AUDIENCES.contentInventory ||
    inventory.componentId !== componentId ||
    inventory.artifactKind !== expected.artifactKind
  ) {
    fail("Inventory attachment does not bind its component identity");
  }
  requireHex64(inventory.artifactSha256, componentId + ".artifactSha256");
  const entries = requireBoundedArray(
    inventory.entries,
    componentId + ".entries",
    1,
    MAX_INVENTORY_ENTRIES,
  );
  const paths = [];
  for (const [index, entry] of entries.entries()) {
    requireObject(entry, componentId + ".entries[" + index + "]");
    const path = requireSafeRelativePath(
      entry.path,
      componentId + ".entries[" + index + "].path",
    );
    paths.push(path);
    if (typeof entry.mode !== "string" || !/^[0-7]{4}$/.test(entry.mode)) {
      fail(componentId + " inventory entry mode is invalid");
    }
    if (entry.type === "directory") {
      requireExactKeys(entry, ["mode", "path", "type"], path);
      continue;
    }
    if (entry.type === "file") {
      requireExactKeys(
        entry,
        ["mode", "path", "sha256", "sizeBytes", "type"],
        path,
      );
      requireHex64(entry.sha256, path + ".sha256");
      requireInteger(entry.sizeBytes, path + ".sizeBytes");
      continue;
    }
    if (entry.type === "symlink" && expected.artifactKind === "oci_image") {
      requireExactKeys(
        entry,
        ["mode", "path", "resolvedTarget", "target", "type"],
        path,
      );
      requireString(entry.target, path + ".target");
      requireSafeRelativePath(entry.resolvedTarget, path + ".resolvedTarget");
      continue;
    }
    fail(componentId + " inventory entry type is invalid");
  }
  if (
    new Set(paths).size !== paths.length ||
    paths.some(
      (path, index) => index > 0 && compareUtf8(paths[index - 1], path) >= 0,
    )
  ) {
    fail(componentId + " inventory paths must be UTF-8 sorted and unique");
  }
  const runtimeContract = MANAGED_CLOUD_RUNTIME_CONTRACTS[componentId];
  if (!runtimeContract) {
    if (inventory.runtime !== null) {
      fail("Static inventory must not contain an OCI runtime contract");
    }
    const index = entries.find((entry) => entry.path === "index.html");
    if (!index || index.type !== "file" || index.sizeBytes < 1) {
      fail("Static inventory must bind a nonempty regular index.html");
    }
    if (
      entries.some(
        (entry) =>
          entry.path.endsWith(".map") ||
          /(^|\/)(?:src|test|tests|node_modules)(\/|$)/.test(entry.path),
      )
    ) {
      fail("Static inventory contains development or dependency content");
    }
    return inventory;
  }
  requireExactKeys(
    inventory.runtime,
    [
      "cmd",
      "environment",
      "entrypoint",
      "exposedPorts",
      "user",
      "workingDirectory",
    ],
    componentId + " inventory runtime",
  );
  const expectedRuntime = {
    cmd: [...runtimeContract.cmd],
    environment: [...runtimeContract.requiredEnvironment].sort(compareUtf8),
    entrypoint: [...runtimeContract.entrypoint],
    exposedPorts: [...runtimeContract.exposedPorts],
    user: runtimeContract.runtimeUser,
    workingDirectory: runtimeContract.workingDirectory,
  };
  if (
    canonicalJsonBytes(inventory.runtime).compare(
      canonicalJsonBytes(expectedRuntime),
    ) !== 0
  ) {
    fail(componentId + " inventory runtime contract is not exact");
  }
  const inventoryMap = new Map(entries.map((entry) => [entry.path, entry]));
  validateContainerSymlinkGraph(inventoryMap);
  requireClosedRuntimeContent(componentId, entries);
  return inventory;
}

export function validateManagedCloudInventoryAttachments(attachments) {
  const componentIds = MANAGED_CLOUD_COMPONENTS.map(
    (component) => component.componentId,
  );
  requireExactKeys(
    attachments,
    componentIds,
    "inventoryAttachments",
  );
  const decoded = new Map();
  let totalBytes = 0;
  for (const componentId of componentIds) {
    const encoded = requireString(
      attachments[componentId],
      "inventoryAttachments." + componentId,
    );
    if (!BASE64URL.test(encoded)) {
      fail("Inventory attachment must be unpadded canonical base64url");
    }
    const bytes = Buffer.from(encoded, "base64url");
    if (bytes.toString("base64url") !== encoded) {
      fail("Inventory attachment is not canonical base64url");
    }
    totalBytes += bytes.length;
    if (totalBytes > MAX_INVENTORY_ATTACHMENT_BYTES) {
      fail("Decoded inventory attachments exceed 12 MiB");
    }
    let inventory;
    try {
      inventory = JSON.parse(bytes.toString("utf8"));
    } catch {
      fail("Inventory attachment is not valid JSON");
    }
    if (!bytes.equals(canonicalJsonBytes(inventory))) {
      fail("Inventory attachment bytes are not canonical JSON");
    }
    validateContentInventoryDocument(inventory, componentId);
    decoded.set(componentId, { bytes, inventory });
  }
  return { decoded, totalBytes };
}

async function hashFile(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

async function readJson(
  path,
  label = basename(path),
  maximumBytes = MAX_JSON_BYTES,
) {
  const bytes = await readFile(path);
  if (bytes.length < 3 || bytes.length > maximumBytes) {
    fail(label + " has an invalid byte length");
  }
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch {
    fail(label + " must be valid JSON");
  }
  return { bytes, value };
}

async function readCanonicalJson(
  path,
  label = basename(path),
  maximumBytes = MAX_JSON_BYTES,
) {
  const parsed = await readJson(path, label, maximumBytes);
  if (!parsed.bytes.equals(canonicalJsonBytes(parsed.value))) {
    fail(label + " must use canonical JSON encoding");
  }
  return parsed;
}

async function writeCanonicalJson(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, canonicalJsonBytes(value), { flag: "wx", mode: 0o444 });
}

async function requireRegularFile(path, label = path) {
  let metadata;
  try {
    metadata = await lstat(path);
  } catch {
    fail(label + " must exist");
  }
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.nlink !== 1
  ) {
    fail(label + " must be one regular, non-linked file");
  }
  return metadata;
}

async function requireEmptyOutputDirectory(path) {
  await mkdir(path, { recursive: true, mode: 0o700 });
  const entries = await readdir(path);
  if (entries.length !== 0) {
    fail("Output directory must be empty");
  }
}

function requireSafeRelativePath(path, label = "path") {
  const value = requireString(path, label);
  if (
    value !== value.normalize("NFC") ||
    value.startsWith("/") ||
    value.includes("\\") ||
    value.split("/").some((part) => part === "" || part === "." || part === "..") ||
    /[\u0000-\u001f\u007f\ud800-\udfff\ufffd]/.test(value)
  ) {
    fail(label + " must be a canonical relative path");
  }
  return value;
}

async function collectTreeFiles(root) {
  const rootPath = resolve(root);
  const files = [];
  const seenInodes = new Set();

  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => compareUtf8(left.name, right.name));
    for (const entry of entries) {
      const path = join(directory, entry.name);
      const metadata = await lstat(path);
      const relativePath = relative(rootPath, path).split(sep).join("/");
      requireSafeRelativePath(relativePath);
      if (metadata.isSymbolicLink()) {
        fail("Trees may not contain symbolic links: " + relativePath);
      }
      if (metadata.isDirectory()) {
        await visit(path);
        continue;
      }
      if (!metadata.isFile()) {
        fail("Trees may contain only directories and regular files");
      }
      const inode = metadata.dev + ":" + metadata.ino;
      if (metadata.nlink !== 1 || seenInodes.has(inode)) {
        fail("Trees may not contain hard-linked files: " + relativePath);
      }
      seenInodes.add(inode);
      files.push({ metadata, path, relativePath });
      if (files.length > MAX_INVENTORY_ENTRIES) {
        fail("Tree contains too many files");
      }
    }
  }

  await visit(rootPath);
  return files;
}

export async function createStaticTreeInventory(root, rootName) {
  const files = await collectTreeFiles(root);
  if (files.length === 0) {
    fail("Static tree must contain at least one file");
  }
  const entries = [];
  for (const file of files) {
    entries.push({
      mode: (file.metadata.mode & 0o777).toString(8).padStart(4, "0"),
      path: file.relativePath,
      sha256: await hashFile(file.path),
      sizeBytes: file.metadata.size,
      type: "file",
    });
  }
  entries.sort((left, right) => compareUtf8(left.path, right.path));
  return {
    audience: MANAGED_CLOUD_AUDIENCES.treeInventory,
    entries,
    rootName: requireSafeId(rootName, "rootName"),
    schemaVersion: 1,
  };
}

async function createRuntimeMeasurementFromFilesystem({
  buildId,
  componentId,
  configSchemaSha256,
  migrationSetSha256,
  protocolSetSha256,
  roles,
  root,
  sourceCommit,
}) {
  const rootPath = resolve(root);
  const entries = [];
  const seen = new Set();

  async function visit(path) {
    const metadata = await lstat(path).catch(() => undefined);
    if (!metadata) {
      fail("Runtime measurement root is missing: " + path);
    }
    if (metadata.isSymbolicLink()) {
      return;
    }
    if (metadata.isDirectory()) {
      const children = await readdir(path, { withFileTypes: true });
      children.sort((left, right) => compareUtf8(left.name, right.name));
      for (const child of children) {
        await visit(join(path, child.name));
      }
      return;
    }
    if (!metadata.isFile()) {
      fail("Runtime measurement roots may contain only files and directories");
    }
    const relativePath = relative(rootPath, path).split(sep).join("/");
    requireSafeRelativePath(relativePath, "runtime measurement path");
    if (
      relativePath === RUNTIME_MEASUREMENT_PATH ||
      metadata.size === 0 ||
      !measurementPathSelected(componentId, relativePath)
    ) {
      return;
    }
    if (seen.has(relativePath)) {
      fail("Runtime measurement contains a duplicate path");
    }
    seen.add(relativePath);
    entries.push({
      path: relativePath,
      sha256: await hashFile(path),
      sizeBytes: metadata.size,
      type: "file",
    });
    if (entries.length > MAX_RUNTIME_MEASUREMENT_FILES) {
      fail("Runtime measurement contains too many files");
    }
  }

  for (const relativePath of measurementRoots(componentId)) {
    await visit(resolve(rootPath, relativePath));
  }
  entries.sort((left, right) => compareUtf8(left.path, right.path));
  return runtimeMeasurementDocument({
    buildId,
    componentId,
    configSchemaSha256,
    inventory: { entries },
    migrationSetSha256,
    protocolSetSha256,
    roles,
    sourceCommit,
  });
}

function tarText(buffer, start, length) {
  const end = buffer.indexOf(0, start);
  const finish = end >= start && end < start + length ? end : start + length;
  return buffer.subarray(start, finish).toString("utf8");
}

function tarInteger(buffer, start, length, label) {
  if ((buffer[start] & 0x80) !== 0) {
    fail("Base-256 tar integers are not allowed for " + label);
  }
  const value = buffer
    .subarray(start, start + length)
    .toString("ascii")
    .replace(/\0.*$/s, "")
    .trim();
  if (value === "") {
    return 0;
  }
  if (!/^[0-7]+$/.test(value)) {
    fail("Tar " + label + " must be canonical octal");
  }
  return requireInteger(Number.parseInt(value, 8), "tar " + label);
}

function tarChecksum(header) {
  let total = 0;
  for (let index = 0; index < header.length; index += 1) {
    total += index >= 148 && index < 156 ? 32 : header[index];
  }
  return total;
}

async function readTarEntries(
  archivePath,
  { length, start = 0, allowLinks = false } = {},
) {
  const metadata = await requireRegularFile(archivePath, "tar archive");
  const archiveLength = length ?? metadata.size - start;
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(archiveLength) ||
    start < 0 ||
    archiveLength < 1024 ||
    archiveLength % 512 !== 0 ||
    start + archiveLength > metadata.size
  ) {
    fail("Tar archive bounds are invalid");
  }
  const handle = await open(archivePath, "r");
  const entries = [];
  const names = new Set();
  let offset = 0;
  let zeroBlocks = 0;
  try {
    while (offset + 512 <= archiveLength) {
      const header = Buffer.alloc(512);
      const read = await handle.read(header, 0, 512, start + offset);
      if (read.bytesRead !== 512) {
        fail("Tar header is truncated");
      }
      if (header.every((byte) => byte === 0)) {
        zeroBlocks += 1;
        offset += 512;
        if (zeroBlocks === 2) {
          break;
        }
        continue;
      }
      if (zeroBlocks !== 0) {
        fail("Tar archive has a nonterminal zero block");
      }
      if (
        tarText(header, 257, 6) !== "ustar" ||
        header.subarray(263, 265).toString("ascii") !== "00"
      ) {
        fail("Tar entry must use canonical POSIX ustar magic and version");
      }
      const expectedChecksum = tarInteger(header, 148, 8, "checksum");
      if (expectedChecksum !== tarChecksum(header)) {
        fail("Tar header checksum is invalid");
      }
      const name = tarText(header, 0, 100);
      const prefix = tarText(header, 345, 155);
      let path = prefix ? prefix + "/" + name : name;
      const type = String.fromCharCode(header[156] || 48);
      if (type === "5" && path.endsWith("/")) {
        path = path.slice(0, -1);
      }
      requireSafeRelativePath(path, "tar path");
      if (names.has(path)) {
        fail("Tar archive contains a duplicate path: " + path);
      }
      names.add(path);
      const sizeBytes = tarInteger(header, 124, 12, "size");
      const mode = tarInteger(header, 100, 8, "mode");
      const uid = tarInteger(header, 108, 8, "uid");
      const gid = tarInteger(header, 116, 8, "gid");
      const mtime = tarInteger(header, 136, 12, "mtime");
      const linkTarget = tarText(header, 157, 100);
      const allowedTypes = allowLinks ? ["0", "1", "2", "5"] : ["0", "5"];
      if (!allowedTypes.includes(type)) {
        fail("Tar archive contains unsupported entry type " + type);
      }
      if (type !== "0" && sizeBytes !== 0) {
        fail("Non-file tar entries must have zero content size");
      }
      if ((type === "1" || type === "2") && linkTarget.length === 0) {
        fail("Tar link target must be nonempty");
      }
      const paddedSize = Math.ceil(sizeBytes / 512) * 512;
      if (offset + 512 + paddedSize > archiveLength) {
        fail("Tar entry content is truncated");
      }
      entries.push({
        dataOffset: start + offset + 512,
        gid,
        linkTarget,
        mode,
        mtime,
        path,
        sizeBytes,
        type,
        uid,
      });
      const paddingLength = paddedSize - sizeBytes;
      if (paddingLength > 0) {
        const padding = Buffer.alloc(paddingLength);
        const paddingRead = await handle.read(
          padding,
          0,
          paddingLength,
          start + offset + 512 + sizeBytes,
        );
        if (
          paddingRead.bytesRead !== paddingLength ||
          padding.some((byte) => byte !== 0)
        ) {
          fail("Tar entry padding must be all zero");
        }
      }
      offset += 512 + paddedSize;
      if (entries.length > MAX_INVENTORY_ENTRIES) {
        fail("Tar archive contains too many entries");
      }
    }
  } finally {
    await handle.close();
  }
  if (zeroBlocks !== 2) {
    fail("Tar archive must end with two zero blocks");
  }
  while (offset < archiveLength) {
    const lengthToRead = Math.min(64 * 1024, archiveLength - offset);
    const trailing = Buffer.alloc(lengthToRead);
    const trailingHandle = await open(archivePath, "r");
    try {
      const trailingRead = await trailingHandle.read(
        trailing,
        0,
        lengthToRead,
        start + offset,
      );
      if (
        trailingRead.bytesRead !== lengthToRead ||
        trailing.some((byte) => byte !== 0)
      ) {
        fail("Tar archive contains nonzero bytes after its terminator");
      }
    } finally {
      await trailingHandle.close();
    }
    offset += lengthToRead;
  }
  return entries;
}

async function readArchiveSlice(path, entry, maximum = MAX_JSON_BYTES) {
  if (entry.type !== "0" || entry.sizeBytes < 2 || entry.sizeBytes > maximum) {
    fail(entry.path + " is not a bounded regular file");
  }
  const handle = await open(path, "r");
  try {
    const bytes = Buffer.alloc(entry.sizeBytes);
    const read = await handle.read(bytes, 0, entry.sizeBytes, entry.dataOffset);
    if (read.bytesRead !== entry.sizeBytes) {
      fail(entry.path + " is truncated");
    }
    return bytes;
  } finally {
    await handle.close();
  }
}

async function hashArchiveSlice(path, entry) {
  const hash = createHash("sha256");
  if (entry.sizeBytes === 0) {
    return hash.digest("hex");
  }
  const stream = createReadStream(path, {
    end: entry.dataOffset + entry.sizeBytes - 1,
    start: entry.dataOffset,
  });
  for await (const chunk of stream) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

async function parseArchiveJson(path, entry, label) {
  const bytes = await readArchiveSlice(path, entry);
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch {
    fail(label + " must contain valid JSON");
  }
  return value;
}

function requireOciDescriptor(value, label) {
  requireObject(value, label);
  for (const key of ["digest", "mediaType", "size"]) {
    if (!(key in value)) {
      fail(label + " is missing " + key);
    }
  }
  requireDigest(value.digest, label + ".digest");
  requireString(value.mediaType, label + ".mediaType");
  requireInteger(value.size, label + ".size", 1);
  return value;
}

function normalizeContainerPath(path) {
  const value = path.startsWith("./") ? path.slice(2) : path;
  return requireSafeRelativePath(value, "container path");
}

function removeContainerPath(entries, target) {
  entries.delete(target);
  for (const path of entries.keys()) {
    if (path.startsWith(target + "/")) {
      entries.delete(path);
    }
  }
}

function requireUnblockedContainerAncestors(entries, path) {
  const parts = path.split("/");
  for (let index = 1; index < parts.length; index += 1) {
    const ancestor = parts.slice(0, index).join("/");
    const value = entries.get(ancestor);
    if (value && value.type !== "directory") {
      fail("OCI layer writes beneath a non-directory ancestor: " + ancestor);
    }
  }
}

function resolveContainerLinkTarget(path, target) {
  if (/[\u0000-\u001f\u007f\ufffd]/.test(target) || target.length === 0) {
    fail("OCI symbolic link target is invalid");
  }
  const normalized = target.startsWith("/")
    ? posix.normalize(target).slice(1)
    : posix.normalize(posix.join(posix.dirname(path), target));
  if (
    normalized === "" ||
    normalized === "." ||
    normalized === ".." ||
    normalized.startsWith("../")
  ) {
    fail("OCI symbolic link escapes the image root");
  }
  return requireSafeRelativePath(normalized, "resolved symlink target");
}

function validateContainerSymlinkGraph(inventory) {
  for (const entry of inventory.values()) {
    if (entry.type !== "symlink") {
      continue;
    }
    const visited = new Set([entry.path]);
    let target = entry.resolvedTarget;
    for (;;) {
      const resolved = inventory.get(target);
      if (!resolved) {
        fail("OCI symbolic link target is missing: " + entry.path);
      }
      if (resolved.type !== "symlink") {
        break;
      }
      if (visited.has(resolved.path)) {
        fail("OCI symbolic link graph contains a cycle");
      }
      visited.add(resolved.path);
      target = resolved.resolvedTarget;
    }
  }
}

async function applyOciLayer(archivePath, layerEntry, inventory) {
  const entries = await readTarEntries(archivePath, {
    allowLinks: true,
    length: layerEntry.sizeBytes,
    start: layerEntry.dataOffset,
  });
  const layerEntries = entries.map((entry) => ({
    ...entry,
    path: normalizeContainerPath(entry.path),
  }));
  for (const entry of layerEntries) {
    const path = entry.path;
    const name = basename(path);
    if (name === ".wh..wh..opq") {
      const parent = dirname(path) === "." ? "" : dirname(path);
      for (const existing of inventory.keys()) {
        if (parent === "" || existing.startsWith(parent + "/")) {
          inventory.delete(existing);
        }
      }
      continue;
    }
    if (name.startsWith(".wh.")) {
      const targetName = name.slice(4);
      if (!targetName) {
        fail("OCI whiteout target is invalid");
      }
      const parent = dirname(path) === "." ? "" : dirname(path);
      removeContainerPath(
        inventory,
        parent ? parent + "/" + targetName : targetName,
      );
      continue;
    }
  }
  for (const entry of layerEntries) {
    const path = entry.path;
    const name = basename(path);
    if (name === ".wh..wh..opq" || name.startsWith(".wh.")) {
      continue;
    }
    const mode = entry.mode.toString(8).padStart(4, "0");
    requireUnblockedContainerAncestors(inventory, path);
    if (entry.type === "5") {
      const existing = inventory.get(path);
      if (existing && existing.type !== "directory") {
        fail("OCI layer replaces a non-directory without an explicit whiteout");
      }
      inventory.set(path, { mode, path, type: "directory" });
      continue;
    }
    removeContainerPath(inventory, path);
    if (entry.type === "2") {
      inventory.set(path, {
        mode,
        path,
        resolvedTarget: resolveContainerLinkTarget(path, entry.linkTarget),
        target: entry.linkTarget,
        type: "symlink",
      });
      continue;
    }
    if (entry.type === "1") {
      const target = normalizeContainerPath(entry.linkTarget);
      const existing = inventory.get(target);
      if (!existing || existing.type !== "file") {
        fail("OCI hard link must target an earlier regular file");
      }
      inventory.set(path, { ...existing, mode, path });
      continue;
    }
    inventory.set(path, {
      mode,
      path,
      sha256: await hashArchiveSlice(archivePath, entry),
      sizeBytes: entry.sizeBytes,
      type: "file",
    });
  }
}

function requireClosedRuntimeContent(componentId, entries) {
  const contract = MANAGED_CLOUD_RUNTIME_CONTRACTS[componentId];
  if (!contract) {
    fail("Unknown OCI runtime component");
  }
  const byPath = new Map(entries.map((entry) => [entry.path, entry]));
  const paths = new Set(byPath.keys());
  for (const path of contract.requiredPaths) {
    const entry = byPath.get(path);
    if (!entry || entry.type !== "file" || entry.sizeBytes < 1) {
      fail(
        componentId + " runtime is missing regular file or it is empty: " + path,
      );
    }
    if (
      (path === "usr/local/bin/bluey-jobs-api" ||
        path === "usr/local/bin/node") &&
      (Number.parseInt(entry.mode, 8) & 0o111) === 0
    ) {
      fail(componentId + " required runtime executable is not executable");
    }
  }
  for (const path of paths) {
    if (
      path.startsWith("app/") &&
      (/(^|\/)(?:src|test|tests|__tests__)(\/|$)/.test(path) ||
        /\.(?:map|ts|tsx)$/.test(path) ||
        /(?:^|\/)(?:Dockerfile|package-lock\.json)$/.test(path))
    ) {
      fail(componentId + " runtime contains development content: " + path);
    }
    if (
      /original[-_]source[-_]verifier|source-verifier/i.test(path)
    ) {
      fail("Phase 611 must not claim a source-verifier runtime entrypoint");
    }
  }
  if (componentId === "jobs-runner") {
    const browserEntries = entries.filter((entry) =>
      entry.path.startsWith("ms-playwright/"),
    );
    if (browserEntries.some((entry) => {
      const topLevel = entry.path.split("/")[1];
      return !/^(?:chromium_headless_shell|ffmpeg)-[0-9]+$/.test(topLevel);
    })) {
      fail("Managed runner may contain only Chromium headless shell and ffmpeg");
    }
    const chromium = entries.filter(
      (entry) =>
        entry.type === "file" &&
        isManagedChromiumExecutablePath(entry.path),
    );
    if (
      chromium.length < 1 ||
      chromium.some(
        (entry) => (Number.parseInt(entry.mode, 8) & 0o111) === 0,
      )
    ) {
      fail(
        "Managed runner image must contain executable Playwright Chromium headless shell",
      );
    }
    if (
      [...paths].some((path) =>
        /ms-playwright\/(?:firefox|webkit)[^/]*\//.test(path),
      )
    ) {
      fail("Managed runner image may not contain unused Firefox or WebKit");
    }
  }
}

function validateOciRuntimeConfig(runtime, imageConfig, componentId, contract) {
  if (imageConfig.os !== "linux" || imageConfig.architecture !== "amd64") {
    fail(componentId + " OCI config must be exactly linux/amd64");
  }
  const allowedRuntimeKeys = new Set([
    "Cmd",
    "Entrypoint",
    "Env",
    "ExposedPorts",
    "Healthcheck",
    "Labels",
    "OnBuild",
    "Shell",
    "StopSignal",
    "User",
    "Volumes",
    "WorkingDir",
  ]);
  if (Object.keys(runtime).some((key) => !allowedRuntimeKeys.has(key))) {
    fail(componentId + " OCI runtime config contains an unknown key");
  }
  const environment = runtime.Env ?? [];
  if (!Array.isArray(environment)) {
    fail(componentId + " OCI Env must be an array");
  }
  const allowedEnvironment = new Set([
    "BLUEY_JOBS_API_HOST",
    "BLUEY_JOBS_API_PORT",
    "BLUEY_JOBS_RUNNER_DATA",
    "LANG",
    "LC_ALL",
    "NODE_ENV",
    "NODE_VERSION",
    "PATH",
    "PLAYWRIGHT_BROWSERS_PATH",
    "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD",
    "YARN_VERSION",
  ]);
  const dangerousEnvironment = new Set([
    "BASH_ENV",
    "ENV",
    "LD_AUDIT",
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "NODE_OPTIONS",
    "NODE_PATH",
    "PYTHONPATH",
    "RUSTC_WRAPPER",
  ]);
  const values = new Map();
  for (const item of environment) {
    const value = requireString(item, componentId + " OCI environment");
    if (
      value.length > 4_096 ||
      /[\u0000-\u001f\u007f\ufffd]/.test(value)
    ) {
      fail(componentId + " OCI environment value is unsafe or oversized");
    }
    const separator = value.indexOf("=");
    const name = separator > 0 ? value.slice(0, separator) : "";
    if (
      !/^[A-Z][A-Z0-9_]*$/.test(name) ||
      values.has(name) ||
      dangerousEnvironment.has(name) ||
      !allowedEnvironment.has(name)
    ) {
      fail(componentId + " OCI environment is duplicated, unsafe, or unknown");
    }
    values.set(name, value);
  }
  for (const required of contract.requiredEnvironment) {
    const name = required.slice(0, required.indexOf("="));
    if (values.get(name) !== required) {
      fail(componentId + " OCI environment is missing " + required);
    }
  }
  if (
    environment.length !== contract.requiredEnvironment.length ||
    environment.some(
      (value, index) => value !== contract.requiredEnvironment[index],
    )
  ) {
    fail(componentId + " OCI environment is not the exact closed set");
  }
  if ((runtime.WorkingDir ?? "") !== contract.workingDirectory) {
    fail(componentId + " OCI working directory is not exact");
  }
  for (const key of ["Healthcheck", "OnBuild", "Shell", "StopSignal", "Volumes"]) {
    const value = runtime[key];
    if (
      value !== undefined &&
      value !== null &&
      !(Array.isArray(value) && value.length === 0) &&
      !(
        value &&
        typeof value === "object" &&
        !Array.isArray(value) &&
        Object.keys(value).length === 0
      )
    ) {
      fail(componentId + " OCI runtime contains unsupported " + key);
    }
  }
  const exposedPorts = Object.keys(runtime.ExposedPorts ?? {}).sort();
  if (JSON.stringify(exposedPorts) !== JSON.stringify(contract.exposedPorts)) {
    fail(componentId + " OCI exposed-port set is not exact");
  }
  const labels = runtime.Labels ?? {};
  requireObject(labels, componentId + " OCI labels");
  for (const [name, value] of Object.entries(labels)) {
    if (
      ![
        "org.opencontainers.image.created",
        "org.opencontainers.image.revision",
        "org.opencontainers.image.source",
      ].includes(name) ||
      typeof value !== "string" ||
      /[\u0000-\u001f\u007f\ufffd]/.test(value)
    ) {
      fail(componentId + " OCI labels contain an unknown or unsafe value");
    }
  }
}

export async function inspectOciImageArchive(archivePath, componentId) {
  const contract = MANAGED_CLOUD_RUNTIME_CONTRACTS[componentId];
  if (!contract) {
    fail("OCI inventory supports only managed cloud runtime components");
  }
  const tarEntries = await readTarEntries(archivePath);
  const files = new Map();
  for (const entry of tarEntries) {
    if (entry.type === "5") {
      continue;
    }
    if (
      ![
        "index.json",
        "oci-layout",
      ].includes(entry.path) &&
      !/^blobs\/sha256\/[0-9a-f]{64}$/.test(entry.path)
    ) {
      fail("OCI layout contains an unknown path: " + entry.path);
    }
    files.set(entry.path, entry);
  }
  for (const required of ["index.json", "oci-layout"]) {
    if (!files.has(required)) {
      fail("OCI layout is missing " + required);
    }
  }
  const layout = await parseArchiveJson(
    archivePath,
    files.get("oci-layout"),
    "oci-layout",
  );
  requireExactKeys(layout, ["imageLayoutVersion"], "oci-layout");
  if (layout.imageLayoutVersion !== "1.0.0") {
    fail("OCI layout version must be 1.0.0");
  }
  for (const [path, entry] of files) {
    if (!path.startsWith("blobs/sha256/")) {
      continue;
    }
    if (
      entry.sizeBytes < 1 ||
      (await hashArchiveSlice(archivePath, entry)) !== basename(path)
    ) {
      fail("OCI blob does not match its content digest: " + path);
    }
  }
  const index = await parseArchiveJson(
    archivePath,
    files.get("index.json"),
    "OCI index",
  );
  requireObject(index, "OCI index");
  if (
    index.schemaVersion !== 2 ||
    !Array.isArray(index.manifests) ||
    index.manifests.length !== 1
  ) {
    fail("OCI index must contain exactly one image manifest");
  }
  const imageDescriptor = requireOciDescriptor(
    index.manifests[0],
    "OCI image descriptor",
  );
  if (
    imageDescriptor.mediaType !==
    "application/vnd.oci.image.manifest.v1+json"
  ) {
    fail("OCI candidate must use one OCI image manifest");
  }
  const manifestEntry = files.get(
    "blobs/sha256/" + imageDescriptor.digest.slice(7),
  );
  if (!manifestEntry || manifestEntry.sizeBytes !== imageDescriptor.size) {
    fail("OCI image manifest descriptor is not size-bound");
  }
  const manifest = await parseArchiveJson(
    archivePath,
    manifestEntry,
    "OCI image manifest",
  );
  requireObject(manifest, "OCI image manifest");
  if (
    manifest.schemaVersion !== 2 ||
    !Array.isArray(manifest.layers) ||
    manifest.layers.length < 1
  ) {
    fail("OCI image manifest is invalid");
  }
  const configDescriptor = requireOciDescriptor(
    manifest.config,
    "OCI config descriptor",
  );
  if (configDescriptor.mediaType !== "application/vnd.oci.image.config.v1+json") {
    fail("OCI image config media type is invalid");
  }
  const configEntry = files.get(
    "blobs/sha256/" + configDescriptor.digest.slice(7),
  );
  if (!configEntry || configEntry.sizeBytes !== configDescriptor.size) {
    fail("OCI image config descriptor is not size-bound");
  }
  const imageConfig = await parseArchiveJson(
    archivePath,
    configEntry,
    "OCI image config",
  );
  requireObject(imageConfig, "OCI image config");
  const allowedImageConfigKeys = new Set([
    "architecture",
    "config",
    "created",
    "history",
    "os",
    "rootfs",
  ]);
  if (Object.keys(imageConfig).some((key) => !allowedImageConfigKeys.has(key))) {
    fail("OCI image config contains an unknown top-level key");
  }
  requireExactKeys(imageConfig.rootfs, ["diff_ids", "type"], "OCI rootfs");
  if (
    imageConfig.rootfs.type !== "layers" ||
    !Array.isArray(imageConfig.rootfs.diff_ids) ||
    JSON.stringify(imageConfig.rootfs.diff_ids) !==
      JSON.stringify(manifest.layers.map((layer) => layer.digest))
  ) {
    fail("OCI config rootfs does not bind the exact uncompressed layers");
  }
  const runtime = requireObject(imageConfig.config, "OCI runtime config");
  const user = runtime.User ?? "";
  const entrypoint = runtime.Entrypoint ?? [];
  const cmd = runtime.Cmd ?? [];
  const workingDirectory = runtime.WorkingDir ?? "";
  validateOciRuntimeConfig(runtime, imageConfig, componentId, contract);
  if (
    user !== contract.runtimeUser ||
    JSON.stringify(entrypoint) !== JSON.stringify(contract.entrypoint) ||
    JSON.stringify(cmd) !== JSON.stringify(contract.cmd)
  ) {
    fail(componentId + " OCI runtime user or entrypoint is not canonical");
  }
  const inventory = new Map();
  const selectedBlobPaths = new Set([
    "blobs/sha256/" + imageDescriptor.digest.slice(7),
    "blobs/sha256/" + configDescriptor.digest.slice(7),
    ...manifest.layers.map(
      (descriptor) => "blobs/sha256/" + descriptor.digest.slice(7),
    ),
  ]);
  const actualBlobPaths = [...files.keys()].filter((path) =>
    path.startsWith("blobs/sha256/"),
  );
  if (
    actualBlobPaths.length !== selectedBlobPaths.size ||
    actualBlobPaths.some((path) => !selectedBlobPaths.has(path))
  ) {
    fail("OCI layout contains an unreferenced or duplicate-selected blob");
  }
  for (const [index, descriptor] of manifest.layers.entries()) {
    requireOciDescriptor(descriptor, "OCI layer[" + index + "]");
    if (descriptor.mediaType !== "application/vnd.oci.image.layer.v1.tar") {
      fail("OCI verification requires deterministic uncompressed layers");
    }
    const layerEntry = files.get(
      "blobs/sha256/" + descriptor.digest.slice(7),
    );
    if (!layerEntry || layerEntry.sizeBytes !== descriptor.size) {
      fail("OCI layer descriptor is not size-bound");
    }
    await applyOciLayer(archivePath, layerEntry, inventory);
  }
  validateContainerSymlinkGraph(inventory);
  const entries = [...inventory.values()].sort((left, right) =>
    compareUtf8(left.path, right.path),
  );
  requireClosedRuntimeContent(componentId, entries);
  return {
    artifactKind: "oci_image",
    artifactSha256: imageDescriptor.digest.slice(7),
    audience: MANAGED_CLOUD_AUDIENCES.contentInventory,
    componentId,
    entries,
    runtime: {
      cmd,
      environment: [...(runtime.Env ?? [])].sort(compareUtf8),
      entrypoint,
      exposedPorts: Object.keys(runtime.ExposedPorts ?? {}).sort(compareUtf8),
      user,
      workingDirectory,
    },
    version: 1,
  };
}

export async function inspectStaticBundleArchive(
  archivePath,
  componentId = "jobs-portal",
) {
  if (componentId !== "jobs-portal") {
    fail("Static bundle inventory is reserved for jobs-portal");
  }
  const entries = await readTarEntries(archivePath);
  const inventory = [];
  for (const entry of entries) {
    if (entry.uid !== 0 || entry.gid !== 0 || entry.mtime !== 0) {
      fail("Static bundle tar metadata must be normalized to uid/gid/mtime zero");
    }
    if (entry.type === "5") {
      inventory.push({
        mode: entry.mode.toString(8).padStart(4, "0"),
        path: entry.path,
        type: "directory",
      });
      continue;
    }
    if (
      entry.path.endsWith(".map") ||
      /(^|\/)(?:src|test|tests|node_modules)(\/|$)/.test(entry.path)
    ) {
      fail("Static bundle contains source, test, map, or dependency content");
    }
    inventory.push({
      mode: entry.mode.toString(8).padStart(4, "0"),
      path: entry.path,
      sha256: await hashArchiveSlice(archivePath, entry),
      sizeBytes: entry.sizeBytes,
      type: "file",
    });
  }
  inventory.sort((left, right) => compareUtf8(left.path, right.path));
  const index = inventory.find((entry) => entry.path === "index.html");
  if (!index || index.type !== "file" || index.sizeBytes < 1) {
    fail("Jobs portal static bundle must contain a nonempty regular index.html");
  }
  return {
    artifactKind: "static_bundle",
    artifactSha256: await hashFile(archivePath),
    audience: MANAGED_CLOUD_AUDIENCES.contentInventory,
    componentId,
    entries: inventory,
    runtime: null,
    version: 1,
  };
}

export async function inspectPreparedArtifact(
  archivePath,
  componentId,
  artifactKind,
) {
  return artifactKind === "oci_image"
    ? inspectOciImageArchive(archivePath, componentId)
    : inspectStaticBundleArchive(archivePath, componentId);
}

async function listSourceFiles(root, requestedPath) {
  const absolute = resolve(root, requestedPath);
  const metadata = await lstat(absolute).catch(() => undefined);
  if (!metadata) {
    fail("Required contract source is missing: " + requestedPath);
  }
  if (metadata.isFile()) {
    return [requestedPath];
  }
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    fail("Contract source must be a file or directory: " + requestedPath);
  }
  const result = [];
  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name, "en"));
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(path);
      } else if (entry.isFile()) {
        const relativePath = relative(root, path).split(sep).join("/");
        if (/\.(?:rs|ts|tsx|json|toml|sql|yml|yaml)$/.test(relativePath)) {
          result.push(relativePath);
        }
      } else {
        fail("Contract sources may not contain links or special files");
      }
    }
  }
  await visit(absolute);
  return result;
}

async function createSourceSet(root, paths) {
  const expanded = [];
  for (const path of paths) {
    expanded.push(...(await listSourceFiles(root, path)));
  }
  const unique = [...new Set(expanded)].sort();
  const files = [];
  for (const path of unique) {
    const absolute = resolve(root, path);
    const metadata = await requireRegularFile(absolute, path);
    files.push({
      path,
      sha256: await hashFile(absolute),
      sizeBytes: metadata.size,
    });
  }
  return {
    files,
    setSha256: sha256(canonicalJsonBytes(files)),
  };
}

function validateSourceFileSet(
  files,
  label,
  { expectedPaths, pathAllowed } = {},
) {
  const entries = requireBoundedArray(
    files,
    label + ".files",
    1,
    MAX_INVENTORY_ENTRIES,
  );
  const paths = [];
  for (const [index, file] of entries.entries()) {
    requireExactKeys(
      file,
      ["path", "sha256", "sizeBytes"],
      label + ".files[" + index + "]",
    );
    const path = requireSafeRelativePath(
      file.path,
      label + ".files[" + index + "].path",
    );
    paths.push(path);
    requireHex64(file.sha256, path + ".sha256");
    requireInteger(file.sizeBytes, path + ".sizeBytes", 1);
    if (pathAllowed && !pathAllowed(path)) {
      fail(label + " contains an out-of-scope source path: " + path);
    }
  }
  const sorted = [...paths].sort();
  if (
    new Set(paths).size !== paths.length ||
    paths.some((path, index) => path !== sorted[index])
  ) {
    fail(label + " source paths must be sorted and unique");
  }
  if (
    expectedPaths &&
    (paths.length !== expectedPaths.length ||
      paths.some((path, index) => path !== [...expectedPaths].sort()[index]))
  ) {
    fail(label + " does not bind its exact required source paths");
  }
  return entries;
}

function validateBuilderPolicyContract(builderPolicy) {
  requireExactKeys(
    builderPolicy,
    [
      "audience",
      "buildNetwork",
      "candidateBuildIsolation",
      "files",
      "immutableBaseImageRequired",
      "noRebuildStages",
      "rootlessRuntimeRequired",
      "schemaVersion",
      "setSha256",
    ],
    "builder policy",
  );
  if (
    builderPolicy.schemaVersion !== 1 ||
    builderPolicy.audience !== MANAGED_CLOUD_AUDIENCES.builderPolicy ||
    builderPolicy.buildNetwork !== "dependency-fetch-only" ||
    builderPolicy.candidateBuildIsolation !== "credential-free" ||
    builderPolicy.immutableBaseImageRequired !== true ||
    builderPolicy.rootlessRuntimeRequired !== true ||
    JSON.stringify(builderPolicy.noRebuildStages) !==
      JSON.stringify(["authorize", "promote", "rollback", "verify"])
  ) {
    fail("Builder policy does not enforce the closed build-once contract");
  }
  const files = validateSourceFileSet(builderPolicy.files, "builder policy", {
    expectedPaths: BUILDER_POLICY_PATHS,
  });
  requireHex64(builderPolicy.setSha256, "builder policy setSha256");
  if (builderPolicy.setSha256 !== sha256(canonicalJsonBytes(files))) {
    fail("Builder policy source-set digest is invalid");
  }
  return files;
}

function validateMigrationContract(migrationContract) {
  requireExactKeys(
    migrationContract,
    ["audience", "paritySha256", "postgres", "schemaVersion", "sqlite"],
    "migration contract",
  );
  if (
    migrationContract.schemaVersion !== 1 ||
    migrationContract.audience !== MANAGED_CLOUD_AUDIENCES.migrationContract
  ) {
    fail("Migration contract audience or version is invalid");
  }
  const dialects = {};
  for (const [name, expectedHead] of [
    ["postgres", POSTGRES_MIGRATION_HEAD],
    ["sqlite", SQLITE_MIGRATION_HEAD],
  ]) {
    const dialect = requireObject(
      migrationContract[name],
      name + " migration contract",
    );
    requireExactKeys(
      dialect,
      ["files", "head", "name", "setSha256"],
      name + " migration contract",
    );
    if (dialect.name !== name || dialect.head !== expectedHead) {
      fail(name + " migration contract has the wrong identity or head");
    }
    const prefix = "infra/" + name + "/server-runtime/";
    const files = validateSourceFileSet(dialect.files, name + " migrations", {
      pathAllowed: (path) => path.startsWith(prefix) && path.endsWith(".sql"),
    });
    requireHex64(dialect.setSha256, name + " migration setSha256");
    if (
      dialect.setSha256 !== sha256(canonicalJsonBytes(files)) ||
      basename(files.at(-1).path) !== expectedHead
    ) {
      fail(name + " migration source set or head is invalid");
    }
    dialects[name] = files;
  }
  requireHex64(migrationContract.paritySha256, "migration paritySha256");
  const expectedParitySha256 = sha256(
    canonicalJsonBytes({
      postgres: dialects.postgres.map((file) => basename(file.path)),
      sqlite: dialects.sqlite.map((file) => basename(file.path)),
    }),
  );
  if (migrationContract.paritySha256 !== expectedParitySha256) {
    fail("Migration parity digest does not bind both exact filename sets");
  }
  return migrationContract;
}

function validateProtocolContract(protocolContract, releaseProtocols) {
  requireExactKeys(
    protocolContract,
    ["audience", "protocols", "schemaVersion"],
    "protocol contract",
  );
  if (
    protocolContract.schemaVersion !== 1 ||
    protocolContract.audience !== MANAGED_CLOUD_AUDIENCES.protocolContract
  ) {
    fail("Protocol contract audience or version is invalid");
  }
  const protocols = requireArray(
    protocolContract.protocols,
    "protocol contract protocols",
    PROTOCOL_SPECS.length,
  );
  if (protocols.length !== PROTOCOL_SPECS.length) {
    fail("Protocol contract must contain the exact Phase 611 protocol set");
  }
  const projected = [];
  for (const [index, protocol] of protocols.entries()) {
    const spec = PROTOCOL_SPECS[index];
    requireExactKeys(
      protocol,
      [
        "protocolId",
        "protocolVersion",
        "schemaSha256",
        "sourceFiles",
      ],
      "protocol contract protocols[" + index + "]",
    );
    if (
      protocol.protocolId !== spec.id ||
      protocol.protocolVersion !== spec.version
    ) {
      fail("Protocol contract identity/version set is not exact");
    }
    const sourceFiles = validateSourceFileSet(
      protocol.sourceFiles,
      protocol.protocolId + " protocol",
      { expectedPaths: spec.sourcePaths },
    );
    requireHex64(
      protocol.schemaSha256,
      protocol.protocolId + ".schemaSha256",
    );
    if (protocol.schemaSha256 !== sha256(canonicalJsonBytes(sourceFiles))) {
      fail(protocol.protocolId + " protocol source-set digest is invalid");
    }
    projected.push({
      protocolId: protocol.protocolId,
      protocolVersion: protocol.protocolVersion,
      schemaSha256: protocol.schemaSha256,
    });
  }
  if (
    releaseProtocols &&
    canonicalJsonBytes(projected).compare(canonicalJsonBytes(releaseProtocols)) !==
      0
  ) {
    fail("Release protocol rows do not project from the full protocol contract");
  }
  return protocolContract;
}

function validateConfigContract(configContract) {
  requireExactKeys(
    configContract,
    ["audience", "roleSources", "schemaVersion", "variables"],
    "config contract",
  );
  if (
    configContract.schemaVersion !== 1 ||
    configContract.audience !== MANAGED_CLOUD_AUDIENCES.configContract
  ) {
    fail("Config contract audience or version is invalid");
  }
  const expectedRoles = [...CONFIG_ROLE_ROOTS]
    .sort((left, right) => left.role.localeCompare(right.role, "en"));
  const roleSources = requireArray(
    configContract.roleSources,
    "config contract roleSources",
    expectedRoles.length,
  );
  if (roleSources.length !== expectedRoles.length) {
    fail("Config contract must bind the exact component role set");
  }
  for (const [index, value] of roleSources.entries()) {
    const expected = expectedRoles[index];
    requireExactKeys(
      value,
      ["role", "sourceFiles", "sourceSetSha256"],
      "config contract roleSources[" + index + "]",
    );
    if (value.role !== expected.role) {
      fail("Config contract role source set is not exact or sorted");
    }
    const sourceFiles = validateSourceFileSet(
      value.sourceFiles,
      value.role + " config sources",
      {
        pathAllowed: (path) =>
          expected.paths.some((root) =>
            root.endsWith("/src") ? path.startsWith(root + "/") : path === root,
          ),
      },
    );
    requireHex64(value.sourceSetSha256, value.role + ".sourceSetSha256");
    if (value.sourceSetSha256 !== sha256(canonicalJsonBytes(sourceFiles))) {
      fail(value.role + " config source-set digest is invalid");
    }
  }
  const allowedRoles = new Set(expectedRoles.map((value) => value.role));
  const variableNames = [];
  for (const [index, variable] of requireBoundedArray(
    configContract.variables,
    "config contract variables",
    1,
    4_096,
  ).entries()) {
    requireExactKeys(
      variable,
      ["classification", "name", "required", "roles", "valueType"],
      "config contract variables[" + index + "]",
    );
    const name = requireString(variable.name, "config variable name");
    if (!/^BLUEY_[A-Z0-9_]+$/.test(name)) {
      fail("Config contract variable name is invalid");
    }
    if (
      name.startsWith("BLUEY_JOBS_MANAGED_CLOUD_") &&
      name.endsWith(MANAGED_CLOUD_FORBIDDEN_CONFIG_SUFFIX)
    ) {
      fail("Runtime identity must be measured locally and cannot be configured");
    }
    variableNames.push(name);
    const requiredSpec = MANAGED_CLOUD_REQUIRED_CONFIG_SPECS.get(name);
    const expectedClassification =
      requiredSpec?.classification ?? classifyConfigVariable(name);
    if (variable.classification !== expectedClassification) {
      fail("Config variable classification is not derived from its name");
    }
    requireBoolean(variable.required, name + ".required");
    requireEnum(
      variable.valueType,
      [
        "boolean_true",
        "canonical_id",
        "https_url",
        "positive_integer",
        "secret_bytes",
        "sha256_hex",
        "string",
      ],
      name + ".valueType",
    );
    if (
      variable.required !== Boolean(requiredSpec) ||
      variable.valueType !== (requiredSpec?.valueType ?? "string")
    ) {
      fail("Config variable required/type authority is not exact");
    }
    const roles = requireSortedUniqueStrings(
      variable.roles,
      name + ".roles",
    );
    if (roles.length < 1 || roles.some((role) => !allowedRoles.has(role))) {
      fail("Config variable roles are empty or outside the closed role set");
    }
    if (
      requiredSpec &&
      requiredSpec.requiredRoles.some((role) => !roles.includes(role))
    ) {
      fail("Required managed-cloud config is missing an owning runtime role");
    }
  }
  if (
    new Set(variableNames).size !== variableNames.length ||
    variableNames.some(
      (name, index) => name !== [...variableNames].sort()[index],
    )
  ) {
    fail("Config variable names must be sorted and unique");
  }
  if (
    [...MANAGED_CLOUD_REQUIRED_CONFIG_SPECS.keys()].some(
      (name) => !variableNames.includes(name),
    )
  ) {
    fail("Config contract omits required managed-cloud runtime configuration");
  }
  return configContract;
}

export function validateManagedCloudReleaseContracts({
  builderPolicy,
  configContract,
  manifest,
  migrationContract,
  protocolContract,
}) {
  validateBuilderPolicyContract(builderPolicy);
  validateConfigContract(configContract);
  validateMigrationContract(migrationContract);
  validateProtocolContract(protocolContract, manifest.protocols);
  if (
    sha256(canonicalJsonBytes(configContract)) !== manifest.configSchemaSha256 ||
    sha256(canonicalJsonBytes(migrationContract)) !==
      manifest.migrationSetSha256 ||
    sha256(canonicalJsonBytes(protocolContract)) !== manifest.protocolSetSha256 ||
    migrationContract.sqlite.head !== manifest.sqliteMigrationHead ||
    migrationContract.postgres.head !== manifest.postgresMigrationHead
  ) {
    fail("Release manifest does not bind the exact managed-cloud contracts");
  }
  return true;
}

function classifyConfigVariable(name) {
  if (/(?:KEY|PASSWORD|SECRET|TOKEN|CREDENTIAL|PRIVATE)/.test(name)) {
    return "secret";
  }
  if (/(?:ENABLED|ALLOW|DISABLE)/.test(name)) {
    return "deny_only_authority";
  }
  return "configuration";
}

export async function createManagedCloudContracts(repoRoot, outputDirectory) {
  const root = resolve(repoRoot);
  const output = resolve(outputDirectory);
  await requireEmptyOutputDirectory(output);

  const builderSources = await createSourceSet(root, BUILDER_POLICY_PATHS);
  const builderPolicy = {
    audience: MANAGED_CLOUD_AUDIENCES.builderPolicy,
    buildNetwork: "dependency-fetch-only",
    candidateBuildIsolation: "credential-free",
    files: builderSources.files,
    immutableBaseImageRequired: true,
    noRebuildStages: ["authorize", "promote", "rollback", "verify"],
    rootlessRuntimeRequired: true,
    schemaVersion: 1,
    setSha256: builderSources.setSha256,
  };

  const protocols = [];
  for (const spec of PROTOCOL_SPECS) {
    const sources = await createSourceSet(root, spec.sourcePaths);
    protocols.push({
      protocolId: spec.id,
      protocolVersion: spec.version,
      schemaSha256: sources.setSha256,
      sourceFiles: sources.files,
    });
  }
  protocols.sort((left, right) =>
    left.protocolId.localeCompare(right.protocolId, "en"),
  );
  const protocolContract = {
    audience: MANAGED_CLOUD_AUDIENCES.protocolContract,
    protocols,
    schemaVersion: 1,
  };

  const sqliteFiles = await listSourceFiles(
    root,
    "infra/sqlite/server-runtime",
  );
  const postgresFiles = await listSourceFiles(
    root,
    "infra/postgres/server-runtime",
  );
  async function migrationDialect(name, paths) {
    const sqlPaths = paths.filter((path) => path.endsWith(".sql")).sort();
    const sourceSet = await createSourceSet(root, sqlPaths);
    if (sourceSet.files.length === 0) {
      fail(name + " migration contract is empty");
    }
    return {
      files: sourceSet.files,
      head: basename(sourceSet.files.at(-1).path),
      name,
      setSha256: sourceSet.setSha256,
    };
  }
  const sqlite = await migrationDialect("sqlite", sqliteFiles);
  const postgres = await migrationDialect("postgres", postgresFiles);
  const migrationContract = {
    audience: MANAGED_CLOUD_AUDIENCES.migrationContract,
    paritySha256: sha256(
      canonicalJsonBytes({
        postgres: postgres.files.map((file) => basename(file.path)),
        sqlite: sqlite.files.map((file) => basename(file.path)),
      }),
    ),
    postgres,
    schemaVersion: 1,
    sqlite,
  };

  const variables = new Map();
  const roleSources = [];
  for (const roleRoot of CONFIG_ROLE_ROOTS) {
    const sources = await createSourceSet(root, roleRoot.paths);
    roleSources.push({
      role: roleRoot.role,
      sourceFiles: sources.files,
      sourceSetSha256: sources.setSha256,
    });
    for (const file of sources.files) {
      const text = await readFile(resolve(root, file.path), "utf8");
      for (const match of text.matchAll(/\bBLUEY_[A-Z0-9_]+\b/g)) {
        const roles = variables.get(match[0]) ?? new Set();
        roles.add(roleRoot.role);
        variables.set(match[0], roles);
      }
    }
  }
  const configContract = {
    audience: MANAGED_CLOUD_AUDIENCES.configContract,
    roleSources: roleSources.sort((left, right) =>
      left.role.localeCompare(right.role, "en"),
    ),
    schemaVersion: 1,
    variables: [...variables.entries()]
      .sort(([left], [right]) => left.localeCompare(right, "en"))
      .map(([name, roles]) => ({
        classification:
          MANAGED_CLOUD_REQUIRED_CONFIG_SPECS.get(name)?.classification ??
          classifyConfigVariable(name),
        name,
        required: MANAGED_CLOUD_REQUIRED_CONFIG_SPECS.has(name),
        roles: [...roles].sort(),
        valueType:
          MANAGED_CLOUD_REQUIRED_CONFIG_SPECS.get(name)?.valueType ?? "string",
      })),
  };

  await writeCanonicalJson(join(output, "builder-policy.json"), builderPolicy);
  await writeCanonicalJson(join(output, "config-contract.json"), configContract);
  await writeCanonicalJson(
    join(output, "migration-contract.json"),
    migrationContract,
  );
  await writeCanonicalJson(
    join(output, "protocol-contract.json"),
    protocolContract,
  );

  return {
    builderPolicy,
    configContract,
    migrationContract,
    protocolContract,
  };
}

function requireFeatureAuthority(featureAuthority) {
  requireExactKeys(
    featureAuthority,
    [
      "cloudDistribution",
      "directDiscovery",
      "globalDiscovery",
      "sourceVerification",
      "workflowCleanup",
      "workflowCommandDispatch",
    ],
    "featureAuthority",
  );
  for (const key of Object.keys(featureAuthority)) {
    requireBoolean(featureAuthority[key], "featureAuthority." + key);
  }
  if (
    !featureAuthority.cloudDistribution ||
    !featureAuthority.workflowCommandDispatch ||
    !featureAuthority.workflowCleanup
  ) {
    fail("Managed cloud base authorities must be enabled atomically");
  }
  if (
    featureAuthority.directDiscovery ||
    featureAuthority.globalDiscovery ||
    featureAuthority.sourceVerification
  ) {
    fail(
      "Phase 611 reserves conditional discovery and source verification without enabling them",
    );
  }
  return featureAuthority;
}

function expectedCapabilities(featureAuthority) {
  requireFeatureAuthority(featureAuthority);
  const capabilities = MANAGED_CLOUD_BASE_CAPABILITIES.map((entry) => ({
    capability: entry.capability,
    componentId: entry.componentId,
  }));
  if (featureAuthority.directDiscovery) {
    capabilities.push({
      capability: "discovery_worker",
      componentId: "jobs-workflows",
    });
  }
  if (featureAuthority.globalDiscovery) {
    capabilities.push({
      capability: "global_discovery_worker",
      componentId: "jobs-workflows",
    });
  }
  return capabilities.sort((left, right) => {
    const componentOrder = left.componentId.localeCompare(
      right.componentId,
      "en",
    );
    return componentOrder || left.capability.localeCompare(right.capability, "en");
  });
}

function requireCapabilities(capabilities, featureAuthority) {
  const expected = expectedCapabilities(featureAuthority);
  requireArray(capabilities, "capabilities", expected.length);
  if (capabilities.length !== expected.length) {
    fail("Release capability count does not match signed feature authority");
  }
  for (const [index, capability] of capabilities.entries()) {
    requireExactKeys(
      capability,
      ["capability", "componentId"],
      "capabilities[" + index + "]",
    );
    if (
      capability.componentId !== expected[index].componentId ||
      capability.capability !== expected[index].capability
    ) {
      fail("Release capability mapping is not the exact closed mapping");
    }
  }
  return capabilities;
}

function requireImmutableArtifactReference(component, releaseId) {
  if (component.artifactKind === "oci_image") {
    if (
      !IMAGE_REFERENCE.test(component.artifactRef) ||
      !component.artifactRef.endsWith("@sha256:" + component.artifactSha256)
    ) {
      fail(component.componentId + " must use its exact immutable OCI digest");
    }
    return;
  }
  let url;
  try {
    url = new URL(component.artifactRef);
  } catch {
    fail("Portal artifactRef must be an immutable HTTPS URL");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    !url.pathname.split("/").includes(releaseId) ||
    !url.pathname.split("/").some(
      (segment) =>
        segment === component.artifactSha256 ||
        segment === component.artifactSha256 + ".tar",
    )
  ) {
    fail("Portal artifactRef must be content-addressed and release-bound");
  }
}

function requireReleaseComponents(
  components,
  releaseId,
  sourceCommit,
  configSchemaSha256,
) {
  requireArray(components, "components", MANAGED_CLOUD_COMPONENTS.length);
  if (components.length !== MANAGED_CLOUD_COMPONENTS.length) {
    fail("Release must contain exactly four artifact components");
  }
  for (const [index, component] of components.entries()) {
    requireExactKeys(
      component,
      [
        "architecture",
        "artifactKind",
        "artifactRef",
        "artifactSha256",
        "buildId",
        "componentId",
        "configSchemaSha256",
        "platform",
        "provenanceSha256",
        "sbomSha256",
        "sourceCommit",
      ],
      "components[" + index + "]",
    );
    const expected = MANAGED_CLOUD_COMPONENTS[index];
    for (const key of [
      "architecture",
      "artifactKind",
      "componentId",
      "platform",
    ]) {
      if (component[key] !== expected[key]) {
        fail("Release component inventory is not exact or sorted");
      }
    }
    requireSafeId(component.buildId, component.componentId + ".buildId");
    if (
      component.sourceCommit !== sourceCommit ||
      component.configSchemaSha256 !== configSchemaSha256
    ) {
      fail(component.componentId + " source or config binding is inconsistent");
    }
    for (const key of [
      "artifactSha256",
      "configSchemaSha256",
      "provenanceSha256",
      "sbomSha256",
    ]) {
      requireHex64(component[key], component.componentId + "." + key);
    }
    requireImmutableArtifactReference(component, releaseId);
  }
  return components;
}

function requireProtocols(protocols) {
  requireArray(protocols, "protocols", MANAGED_CLOUD_PROTOCOL_IDS.length);
  if (protocols.length !== MANAGED_CLOUD_PROTOCOL_IDS.length) {
    fail(
      "Release must bind exactly " +
        MANAGED_CLOUD_PROTOCOL_IDS.length +
        " Phase 611 protocols",
    );
  }
  for (const [index, protocol] of protocols.entries()) {
    requireExactKeys(
      protocol,
      ["protocolId", "protocolVersion", "schemaSha256"],
      "protocols[" + index + "]",
    );
    if (
      protocol.protocolId !== MANAGED_CLOUD_PROTOCOL_IDS[index] ||
      protocol.protocolVersion !== PROTOCOL_SPECS[index].version
    ) {
      fail("Release protocols must use the exact sorted Phase 611 set");
    }
    requireInteger(protocol.protocolVersion, "protocolVersion", 1);
    requireHex64(protocol.schemaSha256, "protocol.schemaSha256");
  }
  return protocols;
}

function componentInventoryDocument(components, capabilities) {
  return {
    audience: MANAGED_CLOUD_AUDIENCES.componentInventory,
    capabilities,
    components,
    version: 1,
  };
}

export function validateReleaseManifest(manifest) {
  requireSignedObjectSize(manifest, "release manifest");
  requireExactKeys(
    manifest,
    [
      "audience",
      "capabilities",
      "components",
      "componentSetSha256",
      "configSchemaSha256",
      "featureAuthority",
      "featureAuthoritySha256",
      "manifestGeneration",
      "manifestId",
      "migrationSetSha256",
      "postgresMigrationHead",
      "protocolSetSha256",
      "protocols",
      "publishedAtMs",
      "releaseId",
      "releaseSequence",
      "sourceCommit",
      "sqliteMigrationHead",
      "verificationEvidenceSha256",
      "version",
    ],
    "release manifest",
  );
  if (
    manifest.version !== 1 ||
    manifest.audience !== MANAGED_CLOUD_AUDIENCES.manifest
  ) {
    fail("Release audience or version is invalid");
  }
  requireSafeId(manifest.manifestId, "manifestId");
  requireInteger(manifest.manifestGeneration, "manifestGeneration", 1);
  const releaseId = requireSafeId(manifest.releaseId, "releaseId");
  requireInteger(manifest.releaseSequence, "releaseSequence", 1);
  if (!SOURCE_COMMIT.test(manifest.sourceCommit)) {
    fail("sourceCommit must be 40 lowercase hexadecimal characters");
  }
  requireInteger(manifest.publishedAtMs, "publishedAtMs");
  requireString(manifest.sqliteMigrationHead, "sqliteMigrationHead");
  requireString(manifest.postgresMigrationHead, "postgresMigrationHead");
  if (
    manifest.sqliteMigrationHead !== SQLITE_MIGRATION_HEAD ||
    manifest.postgresMigrationHead !== POSTGRES_MIGRATION_HEAD
  ) {
    fail("Release migration heads must be the exact current SQL filenames");
  }
  for (const key of [
    "componentSetSha256",
    "configSchemaSha256",
    "featureAuthoritySha256",
    "migrationSetSha256",
    "protocolSetSha256",
    "verificationEvidenceSha256",
  ]) {
    requireHex64(manifest[key], key);
  }
  requireFeatureAuthority(manifest.featureAuthority);
  if (
    sha256(canonicalJsonBytes(manifest.featureAuthority)) !==
    manifest.featureAuthoritySha256
  ) {
    fail("featureAuthoritySha256 does not bind featureAuthority");
  }
  requireReleaseComponents(
    manifest.components,
    releaseId,
    manifest.sourceCommit,
    manifest.configSchemaSha256,
  );
  requireCapabilities(manifest.capabilities, manifest.featureAuthority);
  requireProtocols(manifest.protocols);
  if (
    sha256(
      canonicalJsonBytes(
        componentInventoryDocument(
          manifest.components,
          manifest.capabilities,
        ),
      ),
    ) !== manifest.componentSetSha256
  ) {
    fail("componentSetSha256 does not bind the exact component/capability set");
  }
  return manifest;
}

function validateFileSbom(sbom, componentId, inventory) {
  requireExactKeys(
    sbom,
    [
      "artifactSha256",
      "audience",
      "componentId",
      "contentInventorySha256",
      "files",
      "version",
    ],
    componentId + " SBOM",
  );
  if (
    sbom.version !== 1 ||
    sbom.audience !== MANAGED_CLOUD_AUDIENCES.sbom ||
    sbom.componentId !== componentId ||
    sbom.artifactSha256 !== inventory.artifactSha256 ||
    sbom.contentInventorySha256 !== sha256(canonicalJsonBytes(inventory))
  ) {
    fail(componentId + " SBOM does not bind the artifact inventory");
  }
  const expectedFiles = inventory.entries
    .filter((entry) => entry.type === "file")
    .map((entry) => ({
      path: entry.path,
      sha256: entry.sha256,
      sizeBytes: entry.sizeBytes,
    }));
  if (
    canonicalJsonBytes(sbom.files).compare(canonicalJsonBytes(expectedFiles)) !== 0
  ) {
    fail(componentId + " SBOM must exhaustively match inventory files");
  }
}

export function createFileSbom(inventory) {
  requireObject(inventory, "content inventory");
  requireSafeId(inventory.componentId, "inventory.componentId");
  requireHex64(inventory.artifactSha256, "inventory.artifactSha256");
  requireArray(inventory.entries, "inventory.entries", 1);
  return {
    artifactSha256: inventory.artifactSha256,
    audience: MANAGED_CLOUD_AUDIENCES.sbom,
    componentId: inventory.componentId,
    contentInventorySha256: sha256(canonicalJsonBytes(inventory)),
    files: inventory.entries
      .filter((entry) => entry.type === "file")
      .map((entry) => ({
        path: entry.path,
        sha256: entry.sha256,
        sizeBytes: entry.sizeBytes,
      })),
    version: 1,
  };
}

export function createArtifactProvenance({
  artifactSha256,
  buildId,
  builderPolicySha256,
  componentId,
  sourceCommit,
  sourceDateEpoch,
}) {
  const provenance = {
    artifactSha256,
    audience: MANAGED_CLOUD_AUDIENCES.provenance,
    buildId,
    builderPolicySha256,
    componentId,
    sourceCommit,
    sourceDateEpoch,
    version: 1,
  };
  validateProvenance(provenance, provenance);
  return provenance;
}

function validateProvenance(
  provenance,
  {
    artifactSha256,
    buildId,
    builderPolicySha256,
    componentId,
    sourceCommit,
    sourceDateEpoch,
  },
) {
  requireExactKeys(
    provenance,
    [
      "artifactSha256",
      "audience",
      "buildId",
      "builderPolicySha256",
      "componentId",
      "sourceCommit",
      "sourceDateEpoch",
      "version",
    ],
    componentId + " provenance",
  );
  const expected = {
    artifactSha256,
    audience: MANAGED_CLOUD_AUDIENCES.provenance,
    buildId,
    builderPolicySha256,
    componentId,
    sourceCommit,
    sourceDateEpoch,
    version: 1,
  };
  if (canonicalJsonBytes(provenance).compare(canonicalJsonBytes(expected)) !== 0) {
    fail(componentId + " provenance is not exact");
  }
}

function validateTestEvidence(testEvidence, sourceCommit) {
  requireExactKeys(
    testEvidence,
    ["audience", "checks", "sourceCommit", "version"],
    "test evidence",
  );
  if (
    testEvidence.version !== 1 ||
    testEvidence.audience !== MANAGED_CLOUD_AUDIENCES.testEvidence ||
    testEvidence.sourceCommit !== sourceCommit
  ) {
    fail("Test evidence does not bind the candidate source");
  }
  const checkIds = [];
  for (const [index, check] of requireArray(
    testEvidence.checks,
    "test evidence checks",
    1,
  ).entries()) {
    requireExactKeys(
      check,
      ["checkId", "evidenceSha256", "status"],
      "test evidence check[" + index + "]",
    );
    checkIds.push(requireSafeId(check.checkId, "checkId"));
    requireHex64(check.evidenceSha256, "test evidence digest");
    if (check.status !== "pass") {
      fail("Every candidate test evidence check must pass");
    }
  }
  if (
    new Set(checkIds).size !== checkIds.length ||
    checkIds.some((id, index) => id !== [...checkIds].sort()[index])
  ) {
    fail("Test evidence check IDs must be sorted and unique");
  }
}

async function descriptorBuildToComponent(
  build,
  {
    builderPolicySha256,
    capabilities,
    configSchemaSha256,
    migrationSetSha256,
    protocolSetSha256,
    releaseId,
    sourceCommit,
    sourceDateEpoch,
  },
) {
  requireExactKeys(
    build,
    [
      "architecture",
      "artifactKind",
      "artifactRef",
      "buildId",
      "candidateFile",
      "componentId",
      "contentInventoryFile",
      "platform",
      "provenanceFile",
      "sbomFile",
    ],
    "build descriptor",
  );
  const componentId = requireSafeId(build.componentId, "componentId");
  const expected = MANAGED_CLOUD_COMPONENTS.find(
    (component) => component.componentId === componentId,
  );
  if (
    !expected ||
    build.artifactKind !== expected.artifactKind ||
    build.platform !== expected.platform ||
    build.architecture !== expected.architecture
  ) {
    fail("Build descriptor does not match a closed managed cloud component");
  }
  const candidateFile = resolve(requireString(build.candidateFile, "candidateFile"));
  const contentInventoryFile = resolve(
    requireString(build.contentInventoryFile, "contentInventoryFile"),
  );
  const provenanceFile = resolve(
    requireString(build.provenanceFile, "provenanceFile"),
  );
  const sbomFile = resolve(requireString(build.sbomFile, "sbomFile"));
  for (const path of [
    candidateFile,
    contentInventoryFile,
    provenanceFile,
    sbomFile,
  ]) {
    await requireRegularFile(path);
  }
  const inspected = await inspectPreparedArtifact(
    candidateFile,
    componentId,
    build.artifactKind,
  );
  const inventory = (
    await readCanonicalJson(contentInventoryFile, componentId + " inventory")
  ).value;
  if (
    canonicalJsonBytes(inventory).compare(canonicalJsonBytes(inspected)) !== 0
  ) {
    fail(componentId + " inventory does not match inspected artifact bytes");
  }
  const sbom = (
    await readCanonicalJson(sbomFile, componentId + " SBOM")
  ).value;
  validateFileSbom(sbom, componentId, inventory);
  const provenance = (
    await readCanonicalJson(provenanceFile, componentId + " provenance")
  ).value;
  const buildId = requireSafeId(build.buildId, componentId + ".buildId");
  validateProvenance(provenance, {
    artifactSha256: inventory.artifactSha256,
    buildId,
    builderPolicySha256,
    componentId,
    sourceCommit,
    sourceDateEpoch,
  });
  const component = {
    architecture: expected.architecture,
    artifactKind: expected.artifactKind,
    artifactRef: requireString(build.artifactRef, "artifactRef"),
    artifactSha256: inventory.artifactSha256,
    buildId,
    componentId,
    configSchemaSha256,
    platform: expected.platform,
    provenanceSha256: await hashFile(provenanceFile),
    sbomSha256: await hashFile(sbomFile),
    sourceCommit,
  };
  requireImmutableArtifactReference(component, releaseId);
  const runtimeContract =
    MANAGED_CLOUD_RUNTIME_CONTRACTS[componentId] ?? {
      cmd: [],
      entrypoint: [],
      requiredPaths: ["index.html"],
      runtimePath: null,
      runtimeUser: "static",
    };
  const runtimePath = runtimeContract.requiredEnvironment
    ?.find((entry) => entry.startsWith("PATH="))
    ?.slice("PATH=".length) ?? runtimeContract.runtimePath;
  const requiredPaths = [...runtimeContract.requiredPaths];
  if (componentId === "jobs-runner") {
    requiredPaths.push(
      ...inventory.entries
        .filter(
          (entry) =>
            entry.type === "file" &&
            isManagedChromiumExecutablePath(entry.path),
        )
        .map((entry) => entry.path),
    );
  }
  requiredPaths.sort();
  const runtimeRoles = expectedRuntimeRolesForComponent(
    componentId,
    capabilities,
  );
  const runtimeMeasurement = runtimeContract
    ? runtimeMeasurementDocument({
        buildId,
        componentId,
        configSchemaSha256,
        inventory,
        migrationSetSha256,
        protocolSetSha256,
        roles: runtimeRoles,
        sourceCommit,
      })
    : null;
  const runtimeMeasurementSha256 = runtimeMeasurement
    ? sha256(canonicalJsonBytes(runtimeMeasurement))
    : null;
  const runtimeIdentities = runtimeMeasurement
    ? runtimeRoles.map((role) => ({
        role,
        runtimeIdentitySha256: deriveManagedCloudRuntimeIdentitySha256(
          runtimeMeasurementSha256,
          componentId,
          role,
        ),
      }))
    : [];
  if (runtimeMeasurement) {
    validateManagedCloudRuntimeMeasurement(
      runtimeMeasurement,
      runtimeMeasurementSha256,
      runtimeIdentities,
      {
        buildId,
        componentId,
        configSchemaSha256,
        inventory,
        migrationSetSha256,
        protocolSetSha256,
        roles: runtimeRoles,
        sourceCommit,
      },
    );
  }
  return {
    candidateFile,
    component,
    contentInventoryFile,
    evidence: {
      artifactSha256: component.artifactSha256,
      cmd: [...runtimeContract.cmd],
      componentId,
      contentInventorySha256: await hashFile(contentInventoryFile),
      entrypoint: [...runtimeContract.entrypoint],
      provenanceSha256: component.provenanceSha256,
      requiredPaths,
      runtimeIdentities,
      runtimeMeasurement,
      runtimeMeasurementSha256,
      runtimePath,
      runtimeUser: runtimeContract.runtimeUser,
      sbomSha256: component.sbomSha256,
    },
    provenanceFile,
    sbomFile,
  };
}

async function copyBoundPayload(source, destination, path, payloads) {
  const metadata = await requireRegularFile(source, path);
  await mkdir(dirname(destination), { recursive: true });
  await copyFile(source, destination, 0);
  payloads.push({
    path,
    sha256: await hashFile(destination),
    sizeBytes: metadata.size,
  });
}

export async function assembleManagedCloudCandidate({
  contractsDirectory,
  descriptorFile,
  outputDirectory,
  repoRoot,
}) {
  const contractsRoot = resolve(contractsDirectory);
  const output = resolve(outputDirectory);
  const root = resolve(repoRoot);
  await requireEmptyOutputDirectory(output);
  const { value: descriptor } = await readJson(descriptorFile, "release descriptor");
  requireExactKeys(
    descriptor,
    [
      "audience",
      "builds",
      "featureAuthority",
      "manifestGeneration",
      "manifestId",
      "publishedAtMs",
      "releaseId",
      "releaseSequence",
      "sourceCommit",
      "sourceDateEpoch",
      "testEvidenceFile",
      "version",
    ],
    "release descriptor",
  );
  if (
    descriptor.version !== 1 ||
    descriptor.audience !== "bluey-jobs-managed-cloud-release-descriptor-v1"
  ) {
    fail("Release descriptor audience or version is invalid");
  }
  const releaseId = requireSafeId(descriptor.releaseId, "releaseId");
  const sourceCommit = descriptor.sourceCommit;
  if (!SOURCE_COMMIT.test(sourceCommit)) {
    fail("Descriptor sourceCommit is invalid");
  }
  const sourceDateEpoch = requireInteger(
    descriptor.sourceDateEpoch,
    "sourceDateEpoch",
  );
  const featureAuthority = requireFeatureAuthority(descriptor.featureAuthority);
  const contractNames = [
    "builder-policy.json",
    "config-contract.json",
    "migration-contract.json",
    "protocol-contract.json",
  ];
  const contracts = {};
  for (const name of contractNames) {
    const source = join(contractsRoot, name);
    await readCanonicalJson(source, name);
    await copyFile(source, join(output, name), 0);
    contracts[name] = await hashFile(source);
  }
  const jobsLockPath = join(root, "jobs/package-lock.json");
  const serverLockPath = join(root, "server/Cargo.lock");
  await requireRegularFile(jobsLockPath);
  await requireRegularFile(serverLockPath);
  const migrationContract = (
    await readCanonicalJson(
      join(output, "migration-contract.json"),
      "migration contract",
    )
  ).value;
  requireObject(migrationContract.sqlite, "SQLite migration contract");
  requireObject(migrationContract.postgres, "Postgres migration contract");
  const protocolContract = (
    await readCanonicalJson(
      join(output, "protocol-contract.json"),
      "protocol contract",
    )
  ).value;
  if (
    protocolContract.version !== 1 &&
    protocolContract.schemaVersion !== 1
  ) {
    fail("Protocol contract version is invalid");
  }
  const protocols = requireProtocols(
    requireArray(
      protocolContract.protocols,
      "protocol contract protocols",
      MANAGED_CLOUD_PROTOCOL_IDS.length,
    )
      .map((protocol) => ({
        protocolId: protocol.protocolId,
        protocolVersion: protocol.protocolVersion,
        schemaSha256: protocol.schemaSha256,
      }))
      .sort((left, right) =>
        left.protocolId.localeCompare(right.protocolId, "en"),
      ),
  );
  const capabilities = expectedCapabilities(featureAuthority);
  const buildRecords = await Promise.all(
    requireArray(descriptor.builds, "descriptor.builds", 4).map((build) =>
      descriptorBuildToComponent(build, {
        builderPolicySha256: contracts["builder-policy.json"],
        capabilities,
        configSchemaSha256: contracts["config-contract.json"],
        migrationSetSha256: contracts["migration-contract.json"],
        protocolSetSha256: contracts["protocol-contract.json"],
        releaseId,
        sourceCommit,
        sourceDateEpoch,
      }),
    ),
  );
  buildRecords.sort((left, right) =>
    left.component.componentId.localeCompare(
      right.component.componentId,
      "en",
    ),
  );
  const components = buildRecords.map((record) => record.component);
  const inventoryAttachments = {};
  for (const record of buildRecords) {
    inventoryAttachments[record.component.componentId] = (
      await readFile(record.contentInventoryFile)
    ).toString("base64url");
  }
  validateManagedCloudInventoryAttachments(inventoryAttachments);
  await writeCanonicalJson(
    join(output, "inventory-attachments.json"),
    inventoryAttachments,
  );
  const componentInventory = componentInventoryDocument(
    components,
    capabilities,
  );
  await writeCanonicalJson(
    join(output, "component-inventory.json"),
    componentInventory,
  );
  const testEvidenceFile = resolve(
    requireString(descriptor.testEvidenceFile, "testEvidenceFile"),
  );
  const { value: testEvidence } = await readCanonicalJson(
    testEvidenceFile,
    "test evidence",
  );
  validateTestEvidence(testEvidence, sourceCommit);
  await copyFile(testEvidenceFile, join(output, "test-evidence.json"), 0);
  const verificationEvidence = {
    audience: MANAGED_CLOUD_AUDIENCES.verificationEvidence,
    builderPolicySha256: contracts["builder-policy.json"],
    components: buildRecords.map((record) => record.evidence),
    configContractSha256: contracts["config-contract.json"],
    jobsLockSha256: await hashFile(jobsLockPath),
    migrationContractSha256: contracts["migration-contract.json"],
    protocolContractSha256: contracts["protocol-contract.json"],
    serverLockSha256: await hashFile(serverLockPath),
    sourceCommit,
    testEvidenceSha256: await hashFile(testEvidenceFile),
    version: 1,
  };
  requireSignedObjectSize(verificationEvidence, "verification evidence");
  await writeCanonicalJson(
    join(output, "verification-evidence.json"),
    verificationEvidence,
  );
  const manifest = {
    audience: MANAGED_CLOUD_AUDIENCES.manifest,
    capabilities,
    components,
    componentSetSha256: await hashFile(join(output, "component-inventory.json")),
    configSchemaSha256: contracts["config-contract.json"],
    featureAuthority,
    featureAuthoritySha256: sha256(canonicalJsonBytes(featureAuthority)),
    manifestGeneration: requireInteger(
      descriptor.manifestGeneration,
      "manifestGeneration",
      1,
    ),
    manifestId: requireSafeId(descriptor.manifestId, "manifestId"),
    migrationSetSha256: contracts["migration-contract.json"],
    postgresMigrationHead: requireString(
      migrationContract.postgres.head,
      "postgresMigrationHead",
    ),
    protocolSetSha256: contracts["protocol-contract.json"],
    protocols,
    publishedAtMs: requireInteger(descriptor.publishedAtMs, "publishedAtMs"),
    releaseId,
    releaseSequence: requireInteger(
      descriptor.releaseSequence,
      "releaseSequence",
      1,
    ),
    sourceCommit,
    sqliteMigrationHead: requireString(
      migrationContract.sqlite.head,
      "sqliteMigrationHead",
    ),
    verificationEvidenceSha256: await hashFile(
      join(output, "verification-evidence.json"),
    ),
    version: 1,
  };
  validateReleaseManifest(manifest);
  await writeCanonicalJson(join(output, "release-manifest.json"), manifest);

  const payloads = [];
  for (const record of buildRecords) {
    const id = record.component.componentId;
    const extension =
      record.component.artifactKind === "oci_image" ? ".oci.tar" : ".tar";
    await copyBoundPayload(
      record.candidateFile,
      join(output, "payloads", id + extension),
      "payloads/" + id + extension,
      payloads,
    );
    for (const [kind, source] of [
      ["content-inventory", record.contentInventoryFile],
      ["provenance", record.provenanceFile],
      ["sbom", record.sbomFile],
    ]) {
      const path = "evidence/" + id + "." + kind + ".json";
      await copyBoundPayload(source, join(output, path), path, payloads);
    }
  }
  await copyBoundPayload(
    jobsLockPath,
    join(output, JOBS_LOCK_EVIDENCE_PATH),
    JOBS_LOCK_EVIDENCE_PATH,
    payloads,
  );
  await copyBoundPayload(
    serverLockPath,
    join(output, SERVER_LOCK_EVIDENCE_PATH),
    SERVER_LOCK_EVIDENCE_PATH,
    payloads,
  );
  payloads.sort((left, right) => left.path.localeCompare(right.path, "en"));
  const candidateSet = {
    audience: MANAGED_CLOUD_AUDIENCES.candidate,
    candidateId: manifest.manifestId,
    manifestSha256: await hashFile(join(output, "release-manifest.json")),
    payloads,
    releaseId,
    sourceCommit: manifest.sourceCommit,
    verificationEvidenceSha256: manifest.verificationEvidenceSha256,
    version: 1,
  };
  await writeCanonicalJson(join(output, "candidate-set.json"), candidateSet);
  await validateManagedCloudCandidate(output);
  return candidateSet;
}

function validateCandidateSetShape(candidate) {
  requireExactKeys(
    candidate,
    [
      "audience",
      "candidateId",
      "manifestSha256",
      "payloads",
      "releaseId",
      "sourceCommit",
      "verificationEvidenceSha256",
      "version",
    ],
    "candidate set",
  );
  if (
    candidate.version !== 1 ||
    candidate.audience !== MANAGED_CLOUD_AUDIENCES.candidate
  ) {
    fail("Candidate set audience or schema version is invalid");
  }
  requireSafeId(candidate.candidateId, "candidateId");
  requireSafeId(candidate.releaseId, "releaseId");
  if (!SOURCE_COMMIT.test(candidate.sourceCommit)) {
    fail("Candidate sourceCommit is invalid");
  }
  for (const key of [
    "manifestSha256",
    "verificationEvidenceSha256",
  ]) {
    requireHex64(candidate[key], "candidate." + key);
  }
  requireArray(candidate.payloads, "candidate.payloads", 18);
  if (candidate.payloads.length !== 18) {
    fail("Candidate set must bind exactly 18 artifact and evidence payloads");
  }
  const paths = [];
  for (const [index, payload] of candidate.payloads.entries()) {
    requireExactKeys(
      payload,
      ["path", "sha256", "sizeBytes"],
      "candidate.payloads[" + index + "]",
    );
    paths.push(requireSafeRelativePath(payload.path));
    requireHex64(payload.sha256, payload.path + ".sha256");
    requireInteger(payload.sizeBytes, payload.path + ".sizeBytes", 1);
  }
  if (
    new Set(paths).size !== paths.length ||
    paths.some((path, index) => path !== [...paths].sort()[index])
  ) {
    fail("Candidate payload paths must be sorted and unique");
  }
}

async function requireExactTopLevel(root, expected) {
  const entries = await readdir(root);
  entries.sort();
  const wanted = [...expected].sort();
  if (
    entries.length !== wanted.length ||
    entries.some((entry, index) => entry !== wanted[index])
  ) {
    fail("Directory contains an unexpected or missing top-level entry");
  }
}

export async function validateManagedCloudCandidate(candidateDirectory) {
  const root = resolve(candidateDirectory);
  await requireExactTopLevel(root, [
    ...REQUIRED_CANDIDATE_FILES,
    "evidence",
    "payloads",
  ]);
  const { value: manifest } = await readCanonicalJson(
    join(root, "release-manifest.json"),
  );
  validateReleaseManifest(manifest);
  const { value: candidate } = await readCanonicalJson(
    join(root, "candidate-set.json"),
  );
  validateCandidateSetShape(candidate);
  if (
    candidate.releaseId !== manifest.releaseId ||
    candidate.sourceCommit !== manifest.sourceCommit ||
    candidate.manifestSha256 !==
      (await hashFile(join(root, "release-manifest.json")))
  ) {
    fail("Candidate identity does not bind the release manifest");
  }
  const { value: builderPolicy } = await readCanonicalJson(
    join(root, "builder-policy.json"),
    "builder policy",
  );
  const boundContracts = {};
  const contractBindings = [
    ["component-inventory.json", "componentSetSha256"],
    ["config-contract.json", "configSchemaSha256"],
    ["migration-contract.json", "migrationSetSha256"],
    ["protocol-contract.json", "protocolSetSha256"],
    ["verification-evidence.json", "verificationEvidenceSha256"],
  ];
  for (const [name, key] of contractBindings) {
    const { value } = await readCanonicalJson(join(root, name), name);
    boundContracts[name] = value;
    const digest = await hashFile(join(root, name));
    if (digest !== manifest[key]) {
      fail(name + " does not match its release manifest binding");
    }
  }
  validateManagedCloudReleaseContracts({
    builderPolicy,
    configContract: boundContracts["config-contract.json"],
    manifest,
    migrationContract: boundContracts["migration-contract.json"],
    protocolContract: boundContracts["protocol-contract.json"],
  });
  if (candidate.verificationEvidenceSha256 !== manifest.verificationEvidenceSha256) {
    fail("Candidate does not bind release verification evidence");
  }
  const requiredCandidatePayloadPaths = new Set([
    JOBS_LOCK_EVIDENCE_PATH,
    SERVER_LOCK_EVIDENCE_PATH,
    ...manifest.components.flatMap((component) => {
      const extension =
        component.artifactKind === "oci_image" ? ".oci.tar" : ".tar";
      return [
        "evidence/" + component.componentId + ".content-inventory.json",
        "evidence/" + component.componentId + ".provenance.json",
        "evidence/" + component.componentId + ".sbom.json",
        "payloads/" + component.componentId + extension,
      ];
    }),
  ]);
  const candidatePayloadPaths = new Set(
    candidate.payloads.map((payload) => payload.path),
  );
  if (
    candidatePayloadPaths.size !== requiredCandidatePayloadPaths.size ||
    [...requiredCandidatePayloadPaths].some(
      (path) => !candidatePayloadPaths.has(path),
    )
  ) {
    fail("Candidate payload set is not the exact closed release payload set");
  }
  const { value: componentInventory } = await readCanonicalJson(
    join(root, "component-inventory.json"),
  );
  if (
    canonicalJsonBytes(componentInventory).compare(
      canonicalJsonBytes(
        componentInventoryDocument(
          manifest.components,
          manifest.capabilities,
        ),
      ),
    ) !== 0
  ) {
    fail("Component inventory does not match the release");
  }
  const { value: verificationEvidence } = await readCanonicalJson(
    join(root, "verification-evidence.json"),
    "verification evidence",
    MAX_SIGNED_OBJECT_BYTES,
  );
  const { value: inventoryAttachments } = await readCanonicalJson(
    join(root, "inventory-attachments.json"),
    "inventory attachments",
    MAX_INVENTORY_ATTACHMENT_TRANSPORT_BYTES,
  );
  const validatedAttachments =
    validateManagedCloudInventoryAttachments(inventoryAttachments);
  requireExactKeys(
    verificationEvidence,
    [
      "audience",
      "builderPolicySha256",
      "components",
      "configContractSha256",
      "jobsLockSha256",
      "migrationContractSha256",
      "protocolContractSha256",
      "serverLockSha256",
      "sourceCommit",
      "testEvidenceSha256",
      "version",
    ],
    "verification evidence",
  );
  if (
    verificationEvidence.version !== 1 ||
    verificationEvidence.audience !==
      MANAGED_CLOUD_AUDIENCES.verificationEvidence ||
    verificationEvidence.sourceCommit !== manifest.sourceCommit ||
    verificationEvidence.configContractSha256 !==
      manifest.configSchemaSha256 ||
    verificationEvidence.migrationContractSha256 !==
      manifest.migrationSetSha256 ||
    verificationEvidence.protocolContractSha256 !==
      manifest.protocolSetSha256 ||
    verificationEvidence.testEvidenceSha256 !==
      (await hashFile(join(root, "test-evidence.json"))) ||
    verificationEvidence.builderPolicySha256 !==
      (await hashFile(join(root, "builder-policy.json"))) ||
    verificationEvidence.jobsLockSha256 !==
      (await hashFile(join(root, JOBS_LOCK_EVIDENCE_PATH))) ||
    verificationEvidence.serverLockSha256 !==
      (await hashFile(join(root, SERVER_LOCK_EVIDENCE_PATH)))
  ) {
    fail("Verification evidence does not bind the release contracts");
  }
  const { value: testEvidence } = await readCanonicalJson(
    join(root, "test-evidence.json"),
  );
  validateTestEvidence(testEvidence, manifest.sourceCommit);
  if (verificationEvidence.components.length !== manifest.components.length) {
    fail("Verification evidence component set is incomplete");
  }
  for (const [index, component] of manifest.components.entries()) {
    const evidence = verificationEvidence.components[index];
    requireExactKeys(
      evidence,
      [
        "artifactSha256",
        "cmd",
        "componentId",
        "contentInventorySha256",
        "entrypoint",
        "provenanceSha256",
        "requiredPaths",
        "runtimeIdentities",
        "runtimeMeasurement",
        "runtimeMeasurementSha256",
        "runtimePath",
        "runtimeUser",
        "sbomSha256",
      ],
      "verification component evidence",
    );
    if (
      evidence.componentId !== component.componentId ||
      evidence.artifactSha256 !== component.artifactSha256 ||
      evidence.sbomSha256 !== component.sbomSha256 ||
      evidence.provenanceSha256 !== component.provenanceSha256
    ) {
      fail("Verification component evidence does not bind the manifest");
    }
    const runtimeContract =
      MANAGED_CLOUD_RUNTIME_CONTRACTS[component.componentId];
    if (runtimeContract) {
      requireObject(
        evidence.runtimeMeasurement,
        component.componentId + ".runtimeMeasurement",
      );
      requireHex64(
        evidence.runtimeMeasurementSha256,
        component.componentId + ".runtimeMeasurementSha256",
      );
    } else if (
      evidence.runtimeMeasurement !== null ||
      evidence.runtimeMeasurementSha256 !== null ||
      !Array.isArray(evidence.runtimeIdentities) ||
      evidence.runtimeIdentities.length !== 0
    ) {
      fail("Static portal must not claim runtime measurement authority");
    }
    const expectedRuntimePath = runtimeContract
      ? runtimeContract.requiredEnvironment
          .find((entry) => entry.startsWith("PATH="))
          ?.slice("PATH=".length)
      : null;
    const baseRequiredPaths = runtimeContract?.requiredPaths ?? ["index.html"];
    const evidenceRequiredPaths = requireSortedUniqueStrings(
      evidence.requiredPaths,
      component.componentId + ".requiredPaths",
    );
    const runnerChromiumPaths = evidenceRequiredPaths.filter((path) =>
      isManagedChromiumExecutablePath(path),
    );
    if (
      evidence.runtimePath !== expectedRuntimePath ||
      evidence.runtimeUser !== (runtimeContract?.runtimeUser ?? "static") ||
      JSON.stringify(evidence.entrypoint) !==
        JSON.stringify(runtimeContract?.entrypoint ?? []) ||
      JSON.stringify(evidence.cmd) !==
        JSON.stringify(runtimeContract?.cmd ?? []) ||
      baseRequiredPaths.some((path) => !evidenceRequiredPaths.includes(path)) ||
      (component.componentId === "jobs-runner"
        ? runnerChromiumPaths.length < 1 ||
          evidenceRequiredPaths.length !==
            baseRequiredPaths.length + runnerChromiumPaths.length
        : JSON.stringify(evidenceRequiredPaths) !==
          JSON.stringify([...baseRequiredPaths].sort()))
    ) {
      fail(component.componentId + " runtime evidence is not exact");
    }
    const extension =
      component.artifactKind === "oci_image" ? ".oci.tar" : ".tar";
    const payloadPath = join(
      root,
      "payloads",
      component.componentId + extension,
    );
    const inventoryPath = join(
      root,
      "evidence",
      component.componentId + ".content-inventory.json",
    );
    const sbomPath = join(
      root,
      "evidence",
      component.componentId + ".sbom.json",
    );
    const provenancePath = join(
      root,
      "evidence",
      component.componentId + ".provenance.json",
    );
    const inspected = await inspectPreparedArtifact(
      payloadPath,
      component.componentId,
      component.artifactKind,
    );
    const { value: inventory } = await readCanonicalJson(inventoryPath);
    const attachedInventory = validatedAttachments.decoded.get(
      component.componentId,
    );
    if (
      canonicalJsonBytes(inspected).compare(canonicalJsonBytes(inventory)) !== 0 ||
      !attachedInventory?.bytes.equals(await readFile(inventoryPath)) ||
      evidence.contentInventorySha256 !== (await hashFile(inventoryPath)) ||
      inspected.artifactSha256 !== component.artifactSha256
    ) {
      fail(component.componentId + " payload/inventory relationship is invalid");
    }
    if (runtimeContract) {
      const runtimeRoles = expectedRuntimeRolesForComponent(
        component.componentId,
        manifest.capabilities,
      );
      validateManagedCloudRuntimeMeasurement(
        evidence.runtimeMeasurement,
        evidence.runtimeMeasurementSha256,
        evidence.runtimeIdentities,
        {
          buildId: component.buildId,
          componentId: component.componentId,
          configSchemaSha256: manifest.configSchemaSha256,
          inventory,
          migrationSetSha256: manifest.migrationSetSha256,
          protocolSetSha256: manifest.protocolSetSha256,
          roles: runtimeRoles,
          sourceCommit: manifest.sourceCommit,
        },
      );
    }
    const { value: sbom } = await readCanonicalJson(sbomPath);
    validateFileSbom(sbom, component.componentId, inventory);
    const { value: provenance } = await readCanonicalJson(provenancePath);
    validateProvenance(provenance, {
      artifactSha256: component.artifactSha256,
      buildId: component.buildId,
      builderPolicySha256: verificationEvidence.builderPolicySha256,
      componentId: component.componentId,
      sourceCommit: manifest.sourceCommit,
      sourceDateEpoch: requireInteger(
        provenance.sourceDateEpoch,
        component.componentId + " provenance sourceDateEpoch",
      ),
    });
    if (
      (await hashFile(sbomPath)) !== component.sbomSha256 ||
      (await hashFile(provenancePath)) !== component.provenanceSha256
    ) {
      fail(component.componentId + " SBOM/provenance relationship is invalid");
    }
  }
  const expectedPayloadPaths = new Set(candidate.payloads.map((item) => item.path));
  const treeFiles = await collectTreeFiles(root);
  const actualPayloadPaths = treeFiles
    .map((file) => file.relativePath)
    .filter((path) => path.startsWith("evidence/") || path.startsWith("payloads/"));
  if (
    actualPayloadPaths.length !== expectedPayloadPaths.size ||
    actualPayloadPaths.some((path) => !expectedPayloadPaths.has(path))
  ) {
    fail("Candidate contains unbound payload or evidence files");
  }
  for (const payload of candidate.payloads) {
    const path = join(root, payload.path);
    const metadata = await requireRegularFile(path, payload.path);
    if (
      metadata.size !== payload.sizeBytes ||
      (await hashFile(path)) !== payload.sha256
    ) {
      fail(payload.path + " does not match the candidate binding");
    }
  }
  return { candidate, manifest };
}

const TRUST_ROLES = Object.freeze([
  "general_promotion",
  "incident",
  "promotion",
  "release",
  "root",
]);
const SIGNED_TARGET_AUDIENCES = Object.freeze([
  MANAGED_CLOUD_AUDIENCES.activation,
  MANAGED_CLOUD_AUDIENCES.cohort,
  MANAGED_CLOUD_AUDIENCES.manifest,
  MANAGED_CLOUD_AUDIENCES.rollback,
  MANAGED_CLOUD_AUDIENCES.trustPolicy,
]);

function requireCanonicalBase64url(value, byteLength, label) {
  const encoded = requireString(value, label);
  if (!BASE64URL.test(encoded)) {
    fail(label + " must be unpadded base64url");
  }
  const bytes = Buffer.from(encoded, "base64url");
  if (
    bytes.length !== byteLength ||
    bytes.toString("base64url") !== encoded
  ) {
    fail(label + " must be canonical base64url with exact length");
  }
  return bytes;
}

function publicKeyFromRawBase64url(value, label) {
  const raw = requireCanonicalBase64url(value, 32, label);
  const prefix = Buffer.from("302a300506032b6570032100", "hex");
  try {
    const key = createPublicKey({
      format: "der",
      key: Buffer.concat([prefix, raw]),
      type: "spki",
    });
    if (key.asymmetricKeyType !== "ed25519") {
      fail(label + " must encode an Ed25519 public key");
    }
    return key;
  } catch {
    fail(label + " must encode an Ed25519 public key");
  }
}

function validateRootAnchor(rootAnchor) {
  requireSignedObjectSize(rootAnchor, "root anchor");
  requireExactKeys(
    rootAnchor,
    ["audience", "keys", "threshold", "version"],
    "root anchor",
  );
  if (
    rootAnchor.version !== 1 ||
    rootAnchor.audience !== MANAGED_CLOUD_AUDIENCES.rootAnchor
  ) {
    fail("Root anchor audience or version is invalid");
  }
  requireInteger(rootAnchor.threshold, "root anchor threshold", 1);
  const keyIds = [];
  const keys = new Map();
  for (const [index, key] of requireBoundedArray(
    rootAnchor.keys,
    "root anchor keys",
    1,
    32,
  ).entries()) {
    requireExactKeys(
      key,
      ["keyId", "publicKey", "validFromMs", "validUntilMs"],
      "root anchor key[" + index + "]",
    );
    const keyId = requireSafeId(key.keyId, "root anchor keyId");
    keyIds.push(keyId);
    requireInteger(key.validFromMs, "root key validFromMs");
    requireInteger(key.validUntilMs, "root key validUntilMs", 1);
    if (key.validUntilMs <= key.validFromMs) {
      fail("Root key validity window is invalid");
    }
    keys.set(keyId, {
      keyId,
      publicKey: publicKeyFromRawBase64url(
        key.publicKey,
        "root key publicKey",
      ),
      validFromMs: key.validFromMs,
      validUntilMs: key.validUntilMs,
    });
  }
  if (
    rootAnchor.threshold > keys.size ||
    new Set(keyIds).size !== keyIds.length ||
    keyIds.some((id, index) => id !== [...keyIds].sort()[index])
  ) {
    fail("Root anchor keys must be sorted, unique, and meet threshold");
  }
  return { keys, threshold: rootAnchor.threshold };
}

export function validateManagedCloudTrustPolicy(policy) {
  requireSignedObjectSize(policy, "trust policy");
  requireExactKeys(
    policy,
    [
      "audience",
      "expiresAtMs",
      "issuedAtMs",
      "keys",
      "policyId",
      "predecessorPolicySha256",
      "roles",
      "trustGeneration",
      "validFromMs",
      "version",
    ],
    "trust policy",
  );
  if (
    policy.version !== 1 ||
    policy.audience !== MANAGED_CLOUD_AUDIENCES.trustPolicy
  ) {
    fail("Trust policy audience or version is invalid");
  }
  requireSafeId(policy.policyId, "policyId");
  requireInteger(policy.trustGeneration, "trustGeneration", 1);
  requireInteger(policy.issuedAtMs, "policy.issuedAtMs");
  requireInteger(policy.validFromMs, "policy.validFromMs");
  requireInteger(policy.expiresAtMs, "policy.expiresAtMs", 1);
  if (
    policy.validFromMs > policy.issuedAtMs ||
    policy.expiresAtMs <= policy.issuedAtMs
  ) {
    fail("Trust policy validity window is invalid");
  }
  if (policy.trustGeneration === 1) {
    if (policy.predecessorPolicySha256 !== null) {
      fail("Generation-one trust policy must not have a predecessor");
    }
  } else {
    requireHex64(
      policy.predecessorPolicySha256,
      "predecessorPolicySha256",
    );
  }
  const thresholds = new Map();
  for (const [index, role] of requireArray(
    policy.roles,
    "trust policy roles",
    TRUST_ROLES.length,
  ).entries()) {
    requireExactKeys(
      role,
      ["role", "threshold"],
      "trust policy role[" + index + "]",
    );
    if (role.role !== TRUST_ROLES[index]) {
      fail("Trust policy roles must be the exact sorted role set");
    }
    thresholds.set(
      role.role,
      requireInteger(role.threshold, role.role + " threshold", 1),
    );
  }
  if (policy.roles.length !== TRUST_ROLES.length) {
    fail("Trust policy role set is incomplete");
  }
  const keys = new Map();
  const keyIds = [];
  for (const [index, key] of requireBoundedArray(
    policy.keys,
    "trust policy keys",
    TRUST_ROLES.length,
    64,
  ).entries()) {
    requireExactKeys(
      key,
      [
        "keyId",
        "maximumTrustGeneration",
        "minimumTrustGeneration",
        "publicKey",
        "role",
        "state",
        "validFromMs",
        "validUntilMs",
      ],
      "trust policy key[" + index + "]",
    );
    const keyId = requireSafeId(key.keyId, "keyId");
    keyIds.push(keyId);
    requireEnum(key.role, TRUST_ROLES, "key role");
    requireEnum(key.state, ["active", "retired", "revoked"], "key state");
    requireInteger(key.validFromMs, "key.validFromMs");
    requireInteger(key.validUntilMs, "key.validUntilMs", 1);
    requireInteger(
      key.minimumTrustGeneration,
      "key.minimumTrustGeneration",
      1,
    );
    requireInteger(
      key.maximumTrustGeneration,
      "key.maximumTrustGeneration",
      1,
    );
    if (
      key.validUntilMs <= key.validFromMs ||
      key.maximumTrustGeneration < key.minimumTrustGeneration
    ) {
      fail("Trust key validity or generation window is invalid");
    }
    keys.set(keyId, {
      ...key,
      publicKeyObject: publicKeyFromRawBase64url(
        key.publicKey,
        "trust key publicKey",
      ),
    });
  }
  if (
    new Set(keyIds).size !== keyIds.length ||
    keyIds.some((id, index) => id !== [...keyIds].sort()[index])
  ) {
    fail("Trust policy key IDs must be sorted and unique");
  }
  for (const role of TRUST_ROLES) {
    const activeCount = [...keys.values()].filter(
      (key) =>
        key.role === role &&
        key.state === "active" &&
        policy.trustGeneration >= key.minimumTrustGeneration &&
        policy.trustGeneration <= key.maximumTrustGeneration &&
        policy.issuedAtMs >= key.validFromMs &&
        policy.issuedAtMs < key.validUntilMs,
    ).length;
    if (thresholds.get(role) > activeCount) {
      fail(role + " threshold exceeds its active key count");
    }
  }
  return { keys, thresholds };
}

function validateSignatureSetShape(signatureSet) {
  requireSignedObjectSize(signatureSet, "signature set");
  requireExactKeys(
    signatureSet,
    [
      "audience",
      "role",
      "signatureSetId",
      "signatures",
      "signedAtMs",
      "targetAudience",
      "targetSha256",
      "trustGeneration",
      "version",
    ],
    "signature set",
  );
  if (
    signatureSet.version !== 1 ||
    signatureSet.audience !== MANAGED_CLOUD_AUDIENCES.signatureSet
  ) {
    fail("Signature set audience or version is invalid");
  }
  requireSafeId(signatureSet.signatureSetId, "signatureSetId");
  requireInteger(signatureSet.trustGeneration, "signature trustGeneration", 1);
  requireEnum(signatureSet.role, TRUST_ROLES, "signature role");
  requireEnum(
    signatureSet.targetAudience,
    SIGNED_TARGET_AUDIENCES,
    "signature targetAudience",
  );
  requireHex64(signatureSet.targetSha256, "signature targetSha256");
  requireInteger(signatureSet.signedAtMs, "signature signedAtMs");
  const keyIds = [];
  for (const [index, signature] of requireBoundedArray(
    signatureSet.signatures,
    "signatures",
    1,
    32,
  ).entries()) {
    requireExactKeys(
      signature,
      ["keyId", "signature"],
      "signature[" + index + "]",
    );
    keyIds.push(requireSafeId(signature.keyId, "signature keyId"));
    requireCanonicalBase64url(signature.signature, 64, "signature");
  }
  if (
    new Set(keyIds).size !== keyIds.length ||
    keyIds.some((id, index) => id !== [...keyIds].sort()[index])
  ) {
    fail("Signature key IDs must be sorted and unique");
  }
  return signatureSet;
}

function signaturePayload(signatureSet) {
  return canonicalJsonBytes({
    audience: signatureSet.audience,
    role: signatureSet.role,
    signatureSetId: signatureSet.signatureSetId,
    signedAtMs: signatureSet.signedAtMs,
    targetAudience: signatureSet.targetAudience,
    targetSha256: signatureSet.targetSha256,
    trustGeneration: signatureSet.trustGeneration,
    version: signatureSet.version,
  });
}

function verifySignatureSetWithKeys({
  expectedRole,
  expectedTargetAudience,
  expectedTargetBytes,
  expectedTrustGeneration,
  keys,
  signatureSet,
  threshold,
}) {
  validateSignatureSetShape(signatureSet);
  const nowMs = Date.now();
  if (
    signatureSet.role !== expectedRole ||
    signatureSet.targetAudience !== expectedTargetAudience ||
    signatureSet.targetSha256 !== sha256(expectedTargetBytes) ||
    signatureSet.trustGeneration !== expectedTrustGeneration ||
    signatureSet.signedAtMs > nowMs
  ) {
    fail("Signature set does not bind the expected role, target, generation, and time");
  }
  const payload = signaturePayload(signatureSet);
  const valid = new Set();
  for (const signature of signatureSet.signatures) {
    const key = keys.get(signature.keyId);
    if (!key) {
      fail("Signature set contains an untrusted key");
    }
    if (
      key.role &&
      (key.role !== expectedRole ||
        key.state !== "active" ||
        expectedTrustGeneration < key.minimumTrustGeneration ||
        expectedTrustGeneration > key.maximumTrustGeneration)
    ) {
      fail("Signature key role, state, or generation is invalid");
    }
    const validFromMs = key.validFromMs;
    const validUntilMs = key.validUntilMs;
    if (
      signatureSet.signedAtMs < validFromMs ||
      signatureSet.signedAtMs >= validUntilMs
    ) {
      fail("Signature key is outside its signed-time validity window");
    }
    if (
      !verifySignature(
        null,
        payload,
        key.publicKeyObject ?? key.publicKey,
        Buffer.from(signature.signature, "base64url"),
      )
    ) {
      fail("Ed25519 signature verification failed");
    }
    valid.add(signature.keyId);
  }
  if (valid.size < threshold) {
    fail("Role-separated signature threshold was not met");
  }
  return [...valid].sort();
}

function rootKeyEligible(key, trustGeneration, signedAtMs) {
  return (
    (!key.role ||
      (key.role === "root" &&
        key.state === "active" &&
        trustGeneration >= key.minimumTrustGeneration &&
        trustGeneration <= key.maximumTrustGeneration)) &&
    signedAtMs >= key.validFromMs &&
    signedAtMs < key.validUntilMs
  );
}

function verifyUnionRootSignatureSet({
  expectedTargetBytes,
  expectedTrustGeneration,
  signatureSet,
  sides,
}) {
  validateSignatureSetShape(signatureSet);
  if (
    signatureSet.role !== "root" ||
    signatureSet.targetAudience !== MANAGED_CLOUD_AUDIENCES.trustPolicy ||
    signatureSet.targetSha256 !== sha256(expectedTargetBytes) ||
    signatureSet.trustGeneration !== expectedTrustGeneration ||
    signatureSet.signedAtMs > Date.now()
  ) {
    fail("Root signature set does not bind the policy and trust generation");
  }
  const payload = signaturePayload(signatureSet);
  const counts = sides.map(() => new Set());
  for (const signature of signatureSet.signatures) {
    let eligibleAnywhere = false;
    let verifiedAnywhere = false;
    for (const [sideIndex, side] of sides.entries()) {
      const key = side.keys.get(signature.keyId);
      if (
        !key ||
        !rootKeyEligible(
          key,
          side.trustGeneration ?? expectedTrustGeneration,
          signatureSet.signedAtMs,
        )
      ) {
        continue;
      }
      eligibleAnywhere = true;
      if (
        verifySignature(
          null,
          payload,
          key.publicKeyObject ?? key.publicKey,
          Buffer.from(signature.signature, "base64url"),
        )
      ) {
        verifiedAnywhere = true;
        counts[sideIndex].add(signature.keyId);
      }
    }
    if (!eligibleAnywhere) {
      fail("Root signature set contains a signer ineligible on every trust side");
    }
    if (!verifiedAnywhere) {
      fail("Root signature does not verify under any eligible trust-side key");
    }
  }
  for (const [index, side] of sides.entries()) {
    if (counts[index].size < side.threshold) {
      fail(side.label + " root signature threshold was not met");
    }
  }
  return [...new Set(counts.flatMap((count) => [...count]))].sort();
}

export function verifyManagedCloudSignatureSet({
  expectedRole,
  expectedTargetAudience,
  expectedTargetBytes,
  policy,
  signatureSet,
}) {
  const validatedPolicy = validateManagedCloudTrustPolicy(policy);
  const nowMs = Date.now();
  if (
    nowMs < policy.validFromMs ||
    nowMs >= policy.expiresAtMs ||
    signatureSet.signedAtMs < policy.validFromMs ||
    signatureSet.signedAtMs >= policy.expiresAtMs
  ) {
    fail("Trust policy is not valid for this verification");
  }
  return verifySignatureSetWithKeys({
    expectedRole,
    expectedTargetAudience,
    expectedTargetBytes,
    expectedTrustGeneration: policy.trustGeneration,
    keys: validatedPolicy.keys,
    signatureSet,
    threshold: validatedPolicy.thresholds.get(expectedRole),
  });
}

export function validateManagedCloudTrustBundle(
  trustBundle,
  expectedRootAnchorSha256,
) {
  requireExactKeys(
    trustBundle,
    ["audience", "policies", "rootAnchor", "version"],
    "trust bundle",
  );
  if (
    trustBundle.version !== 1 ||
    trustBundle.audience !== MANAGED_CLOUD_AUDIENCES.trustBundle
  ) {
    fail("Trust bundle audience or version is invalid");
  }
  requireHex64(expectedRootAnchorSha256, "expectedRootAnchorSha256");
  if (
    sha256(canonicalJsonBytes(trustBundle.rootAnchor)) !==
    expectedRootAnchorSha256
  ) {
    fail("Trust bundle root anchor is not the independently pinned anchor");
  }
  const rootAnchor = validateRootAnchor(trustBundle.rootAnchor);
  let previous;
  for (const [index, entry] of requireArray(
    trustBundle.policies,
    "trust bundle policies",
    1,
  ).entries()) {
    requireExactKeys(entry, ["authorization", "policy"], "trust entry");
    const policy = entry.policy;
    const validated = validateManagedCloudTrustPolicy(policy);
    if (policy.trustGeneration !== index + 1) {
      fail("Trust bundle generations must be contiguous and sorted");
    }
    validateSignatureSetShape(entry.authorization);
    if (
      entry.authorization.signedAtMs < policy.issuedAtMs ||
      entry.authorization.signedAtMs < policy.validFromMs ||
      entry.authorization.signedAtMs >= policy.expiresAtMs ||
      (previous &&
        (entry.authorization.signedAtMs < previous.policy.validFromMs ||
          entry.authorization.signedAtMs >= previous.policy.expiresAtMs))
    ) {
      fail(
        "Trust-policy authorization must be inside every required policy window",
      );
    }
    const policyBytes = canonicalJsonBytes(policy);
    if (index === 0) {
      const rootKeys = new Map(
        [...rootAnchor.keys.entries()].map(([keyId, key]) => [
          keyId,
          {
            ...key,
            publicKeyObject: key.publicKey,
          },
        ]),
      );
      const successorRootKeys = new Map(
        [...validated.keys].filter(
          ([, key]) => key.role === "root" && key.state === "active",
        ),
      );
      verifyUnionRootSignatureSet({
        expectedTargetBytes: policyBytes,
        expectedTrustGeneration: 1,
        signatureSet: entry.authorization,
        sides: [
          {
            keys: rootKeys,
            label: "pinned anchor",
            threshold: rootAnchor.threshold,
            trustGeneration: 1,
          },
          {
            keys: successorRootKeys,
            label: "successor policy",
            threshold: validated.thresholds.get("root"),
            trustGeneration: policy.trustGeneration,
          },
        ],
      });
    } else {
      if (
        policy.predecessorPolicySha256 !==
        sha256(canonicalJsonBytes(previous.policy))
      ) {
        fail("Trust rotation does not bind its exact predecessor");
      }
      const previousRootKeys = new Map(
        [...previous.validated.keys].filter(
          ([, key]) => key.role === "root" && key.state === "active",
        ),
      );
      const nextRootKeys = new Map(
        [...validated.keys].filter(
          ([, key]) => key.role === "root" && key.state === "active",
        ),
      );
      verifyUnionRootSignatureSet({
        expectedTargetBytes: policyBytes,
        expectedTrustGeneration: policy.trustGeneration,
        signatureSet: entry.authorization,
        sides: [
          {
            keys: previousRootKeys,
            label: "predecessor policy",
            threshold: previous.validated.thresholds.get("root"),
            trustGeneration: previous.policy.trustGeneration,
          },
          {
            keys: nextRootKeys,
            label: "successor policy",
            threshold: validated.thresholds.get("root"),
            trustGeneration: policy.trustGeneration,
          },
        ],
      });
    }
    previous = { policy, validated };
  }
  const nowMs = Date.now();
  if (
    nowMs < previous.policy.validFromMs ||
    nowMs >= previous.policy.expiresAtMs
  ) {
    fail("Active trust policy is outside its validity window");
  }
  return previous.policy;
}

export async function authorizeManagedCloudCandidate({
  authorizationId,
  candidateDirectory,
  expectedRootAnchorSha256,
  outputDirectory,
  signatureSetFile,
  trustBundleFile,
}) {
  const output = resolve(outputDirectory);
  await requireEmptyOutputDirectory(output);
  const validated = await validateManagedCloudCandidate(candidateDirectory);
  const { value: trustBundle } = await readCanonicalJson(
    trustBundleFile,
    "trust bundle",
  );
  const policy = validateManagedCloudTrustBundle(
    trustBundle,
    expectedRootAnchorSha256,
  );
  const { value: signatureSet } = await readCanonicalJson(
    signatureSetFile,
    "release signatures",
  );
  const manifestBytes = await readFile(
    join(resolve(candidateDirectory), "release-manifest.json"),
  );
  const signerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: "release",
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.manifest,
    expectedTargetBytes: manifestBytes,
    policy,
    signatureSet,
  });
  if (signatureSet.signedAtMs < validated.manifest.publishedAtMs) {
    fail("Release signature cannot predate manifest publication");
  }
  await cp(resolve(candidateDirectory), join(output, "candidate"), {
    errorOnExist: true,
    force: false,
    recursive: true,
  });
  await copyFile(signatureSetFile, join(output, "release-signatures.json"), 0);
  await copyFile(trustBundleFile, join(output, "trust-bundle.json"), 0);
  const authorization = {
    audience: MANAGED_CLOUD_AUDIENCES.authorization,
    authorizationId: requireSafeId(authorizationId, "authorizationId"),
    authorizedAtMs: Date.now(),
    candidateSetSha256: await hashFile(
      join(resolve(candidateDirectory), "candidate-set.json"),
    ),
    manifestSha256: sha256(manifestBytes),
    policySha256: sha256(canonicalJsonBytes(policy)),
    releaseId: validated.manifest.releaseId,
    releaseSignatureSetSha256: await hashFile(signatureSetFile),
    signerKeyIds,
    trustBundleSha256: await hashFile(trustBundleFile),
    trustGeneration: policy.trustGeneration,
    version: 1,
  };
  await writeCanonicalJson(
    join(output, "authorization-set.json"),
    authorization,
  );
  return authorization;
}

export async function validateAuthorizedManagedCloudCandidate(
  authorizedDirectory,
  expectedRootAnchorSha256,
) {
  const root = resolve(authorizedDirectory);
  await requireExactTopLevel(root, REQUIRED_AUTHORIZED_FILES);
  const validated = await validateManagedCloudCandidate(join(root, "candidate"));
  const { value: authorization } = await readCanonicalJson(
    join(root, "authorization-set.json"),
  );
  requireExactKeys(
    authorization,
    [
      "audience",
      "authorizationId",
      "authorizedAtMs",
      "candidateSetSha256",
      "manifestSha256",
      "policySha256",
      "releaseId",
      "releaseSignatureSetSha256",
      "signerKeyIds",
      "trustBundleSha256",
      "trustGeneration",
      "version",
    ],
    "authorization set",
  );
  if (
    authorization.version !== 1 ||
    authorization.audience !== MANAGED_CLOUD_AUDIENCES.authorization ||
    authorization.releaseId !== validated.manifest.releaseId ||
    authorization.manifestSha256 !== validated.candidate.manifestSha256 ||
    authorization.candidateSetSha256 !==
      (await hashFile(join(root, "candidate/candidate-set.json")))
  ) {
    fail("Authorization set does not bind the candidate");
  }
  requireSafeId(authorization.authorizationId, "authorizationId");
  requireInteger(authorization.authorizedAtMs, "authorizedAtMs");
  requireSortedUniqueStrings(authorization.signerKeyIds, "signerKeyIds");
  const { value: trustBundle } = await readCanonicalJson(
    join(root, "trust-bundle.json"),
  );
  const policy = validateManagedCloudTrustBundle(
    trustBundle,
    expectedRootAnchorSha256,
  );
  const { value: signatureSet } = await readCanonicalJson(
    join(root, "release-signatures.json"),
  );
  const manifestBytes = await readFile(
    join(root, "candidate/release-manifest.json"),
  );
  const signerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: "release",
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.manifest,
    expectedTargetBytes: manifestBytes,
    policy,
    signatureSet,
  });
  if (signatureSet.signedAtMs < validated.manifest.publishedAtMs) {
    fail("Release signature cannot predate manifest publication");
  }
  if (
    authorization.authorizedAtMs < signatureSet.signedAtMs ||
    authorization.authorizedAtMs > Date.now() ||
    authorization.authorizedAtMs >= policy.expiresAtMs
  ) {
    fail("Authorization receipt time is outside its signed policy window");
  }
  const bindings = [
    ["manifestSha256", sha256(manifestBytes)],
    ["policySha256", sha256(canonicalJsonBytes(policy))],
    [
      "releaseSignatureSetSha256",
      await hashFile(join(root, "release-signatures.json")),
    ],
    ["trustBundleSha256", await hashFile(join(root, "trust-bundle.json"))],
  ];
  for (const [key, expected] of bindings) {
    if (authorization[key] !== expected) {
      fail("Authorization readback does not bind " + key);
    }
  }
  if (
    authorization.trustGeneration !== policy.trustGeneration ||
    JSON.stringify(authorization.signerKeyIds) !==
      JSON.stringify(signerKeyIds)
  ) {
    fail("Authorization readback signer or trust generation is invalid");
  }
  return {
    ...validated,
    authorization,
    policy,
    root,
    signatureSet,
    trustBundle,
  };
}

export function deriveRequiredRuntimeRoles(featureAuthority) {
  requireFeatureAuthority(featureAuthority);
  const roles = [...MANAGED_CLOUD_BASE_RUNTIME_ROLES];
  if (featureAuthority.directDiscovery) {
    roles.push("discovery_worker");
  }
  if (featureAuthority.globalDiscovery) {
    roles.push("global_discovery_worker");
  }
  return roles.sort();
}

function validateScope(scope, label = "scope") {
  requireExactKeys(
    scope,
    ["channel", "environment", "region"],
    label,
  );
  requireEnum(scope.environment, ["production", "staging"], label + ".environment");
  requireSafeId(scope.region, label + ".region");
  requireEnum(scope.channel, ["canary", "general", "shadow"], label + ".channel");
  return scope;
}

function authorityRoleForChannel(channel) {
  return channel === "general" ? "general_promotion" : "promotion";
}

function validateCohort(cohort) {
  requireSignedObjectSize(cohort, "cohort");
  requireExactKeys(
    cohort,
    [
      "accountIdSha256s",
      "approvalRef",
      "audience",
      "cohortGeneration",
      "cohortId",
      "expiresAtMs",
      "issuedAtMs",
      "notBeforeMs",
      "rolloutMode",
      "scope",
      "trustGeneration",
      "version",
    ],
    "cohort",
  );
  if (
    cohort.version !== 1 ||
    cohort.audience !== MANAGED_CLOUD_AUDIENCES.cohort
  ) {
    fail("Cohort audience or version is invalid");
  }
  requireSafeId(cohort.cohortId, "cohortId");
  requireInteger(cohort.cohortGeneration, "cohortGeneration", 1);
  requireInteger(cohort.trustGeneration, "cohort.trustGeneration", 1);
  const scope = validateScope(cohort.scope, "cohort.scope");
  requireEnum(
    cohort.rolloutMode,
    ["all_eligible_accounts", "allowlist", "none"],
    "rolloutMode",
  );
  requireString(cohort.approvalRef, "approvalRef");
  requireInteger(cohort.issuedAtMs, "cohort.issuedAtMs");
  requireInteger(cohort.notBeforeMs, "cohort.notBeforeMs");
  requireInteger(cohort.expiresAtMs, "cohort.expiresAtMs", 1);
  if (
    cohort.notBeforeMs < cohort.issuedAtMs ||
    cohort.expiresAtMs <= cohort.notBeforeMs
  ) {
    fail("Cohort validity window is invalid");
  }
  const accounts = requireArray(cohort.accountIdSha256s, "accountIdSha256s");
  if (accounts.length > 512) {
    fail("Canary cohort may contain at most 512 account hashes");
  }
  accounts.forEach((value) => requireHex64(value, "accountIdSha256"));
  if (
    new Set(accounts).size !== accounts.length ||
    accounts.some((value, index) => value !== [...accounts].sort()[index])
  ) {
    fail("Cohort account hashes must be sorted and unique");
  }
  if (
    (scope.channel === "canary" &&
      (cohort.rolloutMode !== "allowlist" || accounts.length < 1)) ||
    (scope.channel === "general" &&
      (cohort.rolloutMode !== "all_eligible_accounts" ||
        accounts.length !== 0)) ||
    (scope.channel === "shadow" &&
      (cohort.rolloutMode !== "none" || accounts.length !== 0))
  ) {
    fail("Cohort rollout mode does not match its signed channel");
  }
  return cohort;
}

const ACTIVATION_EVIDENCE_FILES = Object.freeze([
  Object.freeze({
    field: "canaryEvidenceSha256",
    name: "canary.json",
  }),
  Object.freeze({
    field: "cleanupAuthoritySha256",
    name: "cleanup-authority.json",
  }),
  Object.freeze({
    field: "failureConverterSha256",
    name: "failure-converter.json",
  }),
  Object.freeze({
    field: "portalReadbackEvidenceSha256",
    name: "portal-readback.json",
  }),
  Object.freeze({
    field: "runnerFleetEvidenceSha256",
    name: "runner-fleet.json",
  }),
  Object.freeze({
    field: "storageConfigSha256",
    name: "storage-config.json",
  }),
  Object.freeze({
    field: "taskQueueSha256",
    name: "task-queue.json",
  }),
  Object.freeze({
    field: "temporalNamespaceSha256",
    name: "temporal-namespace.json",
  }),
]);

function requireTaskQueueIdentity(value, label) {
  const identity = requireString(value, label);
  const bytes = Buffer.from(identity, "utf8");
  if (
    identity.trim() !== identity ||
    bytes.length > 240 ||
    bytes.toString("utf8") !== identity ||
    /[\p{Cc}\ufffd]/u.test(identity)
  ) {
    fail(
      label +
        " must be a canonical string of at most 240 UTF-8 identity bytes",
    );
  }
  return identity;
}

export function deriveManagedCloudTaskQueueSha256(namespace, taskQueue) {
  const namespaceBytes = Buffer.from(
    requireTaskQueueIdentity(namespace, "namespace"),
    "utf8",
  );
  const taskQueueBytes = Buffer.from(
    requireTaskQueueIdentity(taskQueue, "taskQueue"),
    "utf8",
  );
  return sha256(
    Buffer.concat([
      Buffer.from("bluey-jobs-managed-cloud-task-queue-v1\0", "utf8"),
      namespaceBytes,
      Buffer.from([0]),
      taskQueueBytes,
    ]),
  );
}

function requireEvidenceEnvelope(
  value,
  audience,
  extraKeys,
  activation,
  manifestSha256,
  label,
) {
  requireExactKeys(
    value,
    [
      "audience",
      "expiresAtMs",
      "manifestSha256",
      "observedAtMs",
      "scope",
      "status",
      "version",
      ...extraKeys,
    ],
    label,
  );
  if (
    value.version !== 1 ||
    value.audience !== audience ||
    value.status !== "pass" ||
    value.manifestSha256 !== manifestSha256 ||
    canonicalJsonBytes(value.scope).compare(
      canonicalJsonBytes(activation.scope),
    ) !== 0
  ) {
    fail(label + " does not bind a passing activation scope and manifest");
  }
  validateScope(value.scope, label + ".scope");
  requireInteger(value.observedAtMs, label + ".observedAtMs");
  requireInteger(value.expiresAtMs, label + ".expiresAtMs", 1);
  const nowMs = Date.now();
  if (
    value.observedAtMs > activation.issuedAtMs ||
    value.expiresAtMs <= value.observedAtMs ||
    value.expiresAtMs - value.observedAtMs > 2_592_000_000 ||
    activation.expiresAtMs > value.expiresAtMs ||
    nowMs < value.observedAtMs ||
    nowMs >= value.expiresAtMs
  ) {
    fail(label + " is stale, future-dated, or does not cover activation");
  }
  return value;
}

function expectedCanaryCheckIds(featureAuthority) {
  const ids = [...MANAGED_CLOUD_BASE_CANARY_CHECK_IDS];
  if (featureAuthority.directDiscovery) {
    ids.push("discovery-worker-readiness");
  }
  if (featureAuthority.globalDiscovery) {
    ids.push("global-discovery-worker-readiness");
  }
  return ids.sort();
}

async function readActivationEvidence(
  evidenceDirectory,
  activation,
  authorized,
) {
  const root = resolve(evidenceDirectory);
  await requireExactTopLevel(
    root,
    ACTIVATION_EVIDENCE_FILES.map((entry) => entry.name),
  );
  const hashes = {};
  let taskQueueNamespace;
  let evidenceBytes = 0;
  for (const entry of ACTIVATION_EVIDENCE_FILES) {
    const path = join(root, entry.name);
    const metadata = await requireRegularFile(path, entry.name);
    evidenceBytes += metadata.size;
    if (
      metadata.size > MAX_ACTIVATION_EVIDENCE_FILE_BYTES ||
      evidenceBytes > MAX_ACTIVATION_EVIDENCE_TOTAL_BYTES
    ) {
      fail("Activation evidence exceeds its bounded transport size");
    }
    const { value } = await readCanonicalJson(
      path,
      entry.name,
      MAX_ACTIVATION_EVIDENCE_FILE_BYTES,
    );
    if (entry.field === "canaryEvidenceSha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.canaryEvidence,
        ["checks"],
        activation,
        authorized.candidate.manifestSha256,
        "canary evidence",
      );
      const expectedIds = expectedCanaryCheckIds(
        activation.featureAuthority,
      );
      const ids = [];
      for (const [index, check] of requireArray(
        value.checks,
        "canary checks",
        expectedIds.length,
      ).entries()) {
        requireExactKeys(
          check,
          ["checkId", "evidenceSha256", "status"],
          "canary check[" + index + "]",
        );
        ids.push(requireSafeId(check.checkId, "canary checkId"));
        requireHex64(check.evidenceSha256, "canary check evidenceSha256");
        if (check.status !== "pass") {
          fail("Every canary check must pass");
        }
      }
      if (JSON.stringify(ids) !== JSON.stringify(expectedIds)) {
        fail("Canary evidence does not contain the exact derived check set");
      }
      hashes[entry.field] = await hashFile(path);
      continue;
    }
    if (entry.field === "cleanupAuthoritySha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.cleanupEvidence,
        ["cleanupAuthoritySha256", "dispatcherReady"],
        activation,
        authorized.candidate.manifestSha256,
        "cleanup evidence",
      );
      requireHex64(
        value.cleanupAuthoritySha256,
        "cleanupAuthoritySha256",
      );
      if (value.dispatcherReady !== true) {
        fail("Cleanup evidence must prove a ready dispatcher");
      }
      hashes[entry.field] = value.cleanupAuthoritySha256;
      continue;
    }
    if (entry.field === "failureConverterSha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.failureConverterEvidence,
        ["deployedFileSha256"],
        activation,
        authorized.candidate.manifestSha256,
        "failure converter evidence",
      );
      const { value: workflowsInventory } = await readCanonicalJson(
        authorized.workflowsInventoryFile ??
          join(
            authorized.root,
            "candidate/evidence/jobs-workflows.content-inventory.json",
          ),
        "jobs-workflows content inventory",
      );
      const deployedConverter = requireArray(
        workflowsInventory.entries,
        "jobs-workflows inventory entries",
        1,
      ).find(
        (item) =>
          item.path === "app/workflows/dist/failure-converter.js" &&
          item.type === "file",
      );
      if (!deployedConverter) {
        fail("Workflows inventory omits the deployed failure converter");
      }
      const deployedFileSha256 = requireHex64(
        deployedConverter.sha256,
        "deployed failure converter SHA-256",
      );
      if (value.deployedFileSha256 !== deployedFileSha256) {
        fail("Failure converter evidence does not bind deployed raw bytes");
      }
      hashes[entry.field] = deployedFileSha256;
      continue;
    }
    if (entry.field === "portalReadbackEvidenceSha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.portalReadbackEvidence,
        ["portalArtifactSha256", "readbackSha256"],
        activation,
        authorized.candidate.manifestSha256,
        "portal readback evidence",
      );
      const portal = authorized.manifest.components.find(
        (component) => component.componentId === "jobs-portal",
      );
      if (value.portalArtifactSha256 !== portal.artifactSha256) {
        fail("Portal readback evidence does not bind portal artifact");
      }
      requireHex64(value.readbackSha256, "portal readbackSha256");
      if (value.readbackSha256 !== portal.artifactSha256) {
        fail("Portal readback bytes differ from the released artifact bytes");
      }
      if (value.observedAtMs !== activation.portalReadbackAtMs) {
        fail("Portal readback timestamp does not bind activation");
      }
      hashes[entry.field] = await hashFile(path);
      continue;
    }
    if (entry.field === "runnerFleetEvidenceSha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.runnerFleetEvidence,
        ["fleetIdSha256", "readyInstances", "requiredInstances"],
        activation,
        authorized.candidate.manifestSha256,
        "runner fleet evidence",
      );
      requireHex64(value.fleetIdSha256, "fleetIdSha256");
      requireInteger(value.requiredInstances, "requiredInstances", 1);
      requireInteger(value.readyInstances, "readyInstances", 1);
      if (value.readyInstances < value.requiredInstances) {
        fail("Runner fleet evidence has insufficient ready instances");
      }
      hashes[entry.field] = await hashFile(path);
      continue;
    }
    if (entry.field === "storageConfigSha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.storageEvidence,
        ["readProbeSha256", "storageConfigSha256", "writeProbeSha256"],
        activation,
        authorized.candidate.manifestSha256,
        "storage evidence",
      );
      for (const key of [
        "readProbeSha256",
        "storageConfigSha256",
        "writeProbeSha256",
      ]) {
        requireHex64(value[key], "storage evidence " + key);
      }
      hashes[entry.field] = value.storageConfigSha256;
      continue;
    }
    if (entry.field === "taskQueueSha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.taskQueueEvidence,
        ["namespace", "taskQueue", "taskQueueSha256"],
        activation,
        authorized.candidate.manifestSha256,
        "task queue evidence",
      );
      if (
        value.taskQueueSha256 !==
          deriveManagedCloudTaskQueueSha256(value.namespace, value.taskQueue)
      ) {
        fail("Task queue evidence is not canonical or correctly derived");
      }
      taskQueueNamespace = value.namespace;
      hashes[entry.field] = value.taskQueueSha256;
      continue;
    }
    if (entry.field === "temporalNamespaceSha256") {
      requireEvidenceEnvelope(
        value,
        MANAGED_CLOUD_AUDIENCES.temporalEvidence,
        [
          "gatewayReady",
          "namespace",
          "namespaceSha256",
          "workflowWorkerReady",
        ],
        activation,
        authorized.candidate.manifestSha256,
        "Temporal evidence",
      );
      const namespace = requireTaskQueueIdentity(
        value.namespace,
        "Temporal namespace",
      );
      if (
        namespace !== taskQueueNamespace ||
        value.namespaceSha256 !== sha256(Buffer.from(namespace, "utf8")) ||
        value.gatewayReady !== true ||
        value.workflowWorkerReady !== true
      ) {
        fail("Temporal evidence does not prove namespace/gateway/worker");
      }
      hashes[entry.field] = value.namespaceSha256;
      continue;
    }
    fail("Unknown activation evidence binding");
  }
  return hashes;
}

export async function validateManagedCloudActivationEvidence({
  activation,
  evidenceDirectory,
  manifest,
  manifestSha256,
  workflowsInventoryFile,
}) {
  requireSignedObjectSize(activation, "activation");
  validateReleaseManifest(manifest);
  requireHex64(manifestSha256, "manifestSha256");
  if (
    canonicalJsonBytes(activation.featureAuthority).compare(
      canonicalJsonBytes(manifest.featureAuthority),
    ) !== 0
  ) {
    fail("Activation evidence authority differs from the release authority");
  }
  return readActivationEvidence(evidenceDirectory, activation, {
    candidate: { manifestSha256 },
    manifest,
    workflowsInventoryFile: resolve(
      requireString(workflowsInventoryFile, "workflowsInventoryFile"),
    ),
  });
}

function validateActivation(
  activation,
  authorized,
  cohort,
  cohortSignatureSetSha256,
  evidenceHashes,
) {
  requireSignedObjectSize(activation, "activation");
  requireExactKeys(
    activation,
    [
      "activationGeneration",
      "activationId",
      "audience",
      "canaryEvidenceSha256",
      "channelSequence",
      "cleanupAuthoritySha256",
      "cohortSha256",
      "cohortSignatureSetSha256",
      "expectedHeadRevision",
      "expectedTransitionSha256",
      "expiresAtMs",
      "failureConverterSha256",
      "featureAuthority",
      "featureAuthoritySha256",
      "heartbeatTtlMs",
      "issuedAtMs",
      "manifestSha256",
      "manifestSignatureSetSha256",
      "maximumDailyAdmissions",
      "maximumInflight",
      "notBeforeMs",
      "portalReadbackAtMs",
      "portalReadbackEvidenceSha256",
      "portalReadbackTtlMs",
      "predecessorActivationSha256",
      "recoveryAcceptances",
      "runnerFleetEvidenceSha256",
      "scope",
      "storageConfigSha256",
      "taskQueueSha256",
      "temporalNamespaceSha256",
      "trustGeneration",
      "version",
    ],
    "activation",
  );
  if (
    activation.version !== 1 ||
    activation.audience !== MANAGED_CLOUD_AUDIENCES.activation
  ) {
    fail("Activation audience or version is invalid");
  }
  requireSafeId(activation.activationId, "activationId");
  requireInteger(activation.activationGeneration, "activationGeneration", 1);
  requireInteger(activation.trustGeneration, "activation.trustGeneration", 1);
  const scope = validateScope(activation.scope, "activation.scope");
  requireInteger(activation.channelSequence, "channelSequence", 1);
  requireInteger(activation.expectedHeadRevision, "expectedHeadRevision");
  if (activation.expectedHeadRevision === 0) {
    if (
      activation.expectedTransitionSha256 !== null ||
      activation.predecessorActivationSha256 !== null
    ) {
      fail("Initial activation must use null predecessor CAS bindings");
    }
  } else {
    requireHex64(
      activation.expectedTransitionSha256,
      "expectedTransitionSha256",
    );
    requireHex64(
      activation.predecessorActivationSha256,
      "predecessorActivationSha256",
    );
  }
  for (const key of [
    "canaryEvidenceSha256",
    "cleanupAuthoritySha256",
    "cohortSha256",
    "cohortSignatureSetSha256",
    "failureConverterSha256",
    "featureAuthoritySha256",
    "manifestSha256",
    "manifestSignatureSetSha256",
    "portalReadbackEvidenceSha256",
    "runnerFleetEvidenceSha256",
    "storageConfigSha256",
    "taskQueueSha256",
    "temporalNamespaceSha256",
  ]) {
    requireHex64(activation[key], "activation." + key);
  }
  requireFeatureAuthority(activation.featureAuthority);
  if (
    canonicalJsonBytes(activation.featureAuthority).compare(
      canonicalJsonBytes(authorized.manifest.featureAuthority),
    ) !== 0 ||
    activation.featureAuthoritySha256 !==
      authorized.manifest.featureAuthoritySha256
  ) {
    fail("Activation feature authority must exactly equal the release authority");
  }
  const bindings = [
    ["manifestSha256", authorized.candidate.manifestSha256],
    [
      "manifestSignatureSetSha256",
      authorized.authorization.releaseSignatureSetSha256,
    ],
    ["cohortSha256", sha256(canonicalJsonBytes(cohort))],
    ["cohortSignatureSetSha256", cohortSignatureSetSha256],
  ];
  for (const [key, expected] of bindings) {
    if (activation[key] !== expected) {
      fail("Activation does not bind " + key);
    }
  }
  for (const entry of ACTIVATION_EVIDENCE_FILES) {
    if (activation[entry.field] !== evidenceHashes[entry.field]) {
      fail("Activation does not bind " + entry.field);
    }
  }
  if (
    activation.trustGeneration !== authorized.policy.trustGeneration ||
    canonicalJsonBytes(scope).compare(canonicalJsonBytes(cohort.scope)) !== 0
  ) {
    fail("Activation trust generation or scope is inconsistent");
  }
  requireInteger(activation.maximumInflight, "maximumInflight");
  requireInteger(activation.maximumDailyAdmissions, "maximumDailyAdmissions");
  if (
    (scope.channel === "shadow" &&
      (activation.maximumInflight !== 0 ||
        activation.maximumDailyAdmissions !== 0)) ||
    (scope.channel !== "shadow" &&
      (activation.maximumInflight < 1 ||
        activation.maximumDailyAdmissions < 1))
  ) {
    fail("Activation admission caps do not match its channel");
  }
  requireInteger(activation.heartbeatTtlMs, "heartbeatTtlMs", 5_000);
  if (activation.heartbeatTtlMs > 300_000) {
    fail("Activation heartbeat TTL is out of range");
  }
  requireInteger(activation.portalReadbackAtMs, "portalReadbackAtMs");
  requireInteger(activation.portalReadbackTtlMs, "portalReadbackTtlMs", 60_000);
  if (activation.portalReadbackTtlMs > 2_592_000_000) {
    fail("Portal readback TTL is out of range");
  }
  requireInteger(activation.issuedAtMs, "activation.issuedAtMs");
  requireInteger(activation.notBeforeMs, "activation.notBeforeMs");
  requireInteger(activation.expiresAtMs, "activation.expiresAtMs", 1);
  validateManagedCloudActivationWindow(
    activation,
    cohort,
    authorized.policy,
  );
  const recoveryKeys = [];
  for (const [index, recovery] of requireBoundedArray(
    activation.recoveryAcceptances,
    "recoveryAcceptances",
    0,
    32,
  ).entries()) {
    requireExactKeys(
      recovery,
      ["activationSha256", "manifestSha256"],
      "recovery acceptance[" + index + "]",
    );
    requireHex64(recovery.activationSha256, "recovery activationSha256");
    requireHex64(recovery.manifestSha256, "recovery manifestSha256");
    recoveryKeys.push(
      recovery.activationSha256 + ":" + recovery.manifestSha256,
    );
  }
  if (
    new Set(recoveryKeys).size !== recoveryKeys.length ||
    recoveryKeys.some((key, index) => key !== [...recoveryKeys].sort()[index])
  ) {
    fail("Recovery acceptances must be sorted and unique");
  }
  deriveRequiredRuntimeRoles(activation.featureAuthority);
  return activation;
}

export function validateManagedCloudActivationWindow(
  activation,
  cohort,
  policy,
) {
  const nowMs = Date.now();
  if (
    activation.notBeforeMs < activation.issuedAtMs ||
    activation.expiresAtMs <= activation.notBeforeMs ||
    activation.portalReadbackAtMs > activation.issuedAtMs ||
    activation.portalReadbackAtMs >
      Number.MAX_SAFE_INTEGER - activation.portalReadbackTtlMs ||
    activation.expiresAtMs >
      activation.portalReadbackAtMs + activation.portalReadbackTtlMs ||
    activation.issuedAtMs < cohort.issuedAtMs ||
    activation.notBeforeMs < cohort.notBeforeMs ||
    activation.expiresAtMs > cohort.expiresAtMs ||
    activation.issuedAtMs < policy.validFromMs ||
    activation.expiresAtMs > policy.expiresAtMs ||
    nowMs < activation.notBeforeMs ||
    nowMs >= activation.expiresAtMs
  ) {
    fail("Activation or portal-readback validity window is invalid");
  }
  return true;
}

async function copyPublishedPayloads(candidateRoot, publishedRoot, readbackRoot) {
  await requireEmptyOutputDirectory(publishedRoot);
  await requireEmptyOutputDirectory(readbackRoot);
  const { value: candidateSet } = await readCanonicalJson(
    join(candidateRoot, "candidate-set.json"),
  );
  const payloads = candidateSet.payloads.filter((item) =>
    item.path.startsWith("payloads/"),
  );
  for (const payload of payloads) {
    const source = join(candidateRoot, payload.path);
    const published = join(publishedRoot, payload.path);
    const readback = join(readbackRoot, payload.path);
    await mkdir(dirname(published), { recursive: true });
    await mkdir(dirname(readback), { recursive: true });
    await copyFile(source, published, 0);
    await copyFile(published, readback, 0);
    for (const path of [published, readback]) {
      const metadata = await requireRegularFile(path);
      if (
        metadata.size !== payload.sizeBytes ||
        (await hashFile(path)) !== payload.sha256
      ) {
        fail("Immutable local adapter readback changed " + payload.path);
      }
    }
  }
  return payloads;
}

async function validatePublishedPayloads(root, candidateSet) {
  const expected = candidateSet.payloads.filter((item) =>
    item.path.startsWith("payloads/"),
  );
  const files = await collectTreeFiles(root);
  if (
    files.length !== expected.length ||
    files.some(
      (file, index) => file.relativePath !== expected[index].path,
    )
  ) {
    fail("Published/readback payload tree is not exact");
  }
  for (const [index, file] of files.entries()) {
    if (
      file.metadata.size !== expected[index].sizeBytes ||
      (await hashFile(file.path)) !== expected[index].sha256
    ) {
      fail("Published/readback payload digest changed");
    }
  }
  return expected;
}

export async function promoteManagedCloudCandidate({
  activationFile,
  activationSignatureSetFile,
  authorizedDirectory,
  cohortFile,
  cohortSignatureSetFile,
  evidenceDirectory,
  expectedRootAnchorSha256,
  outputDirectory,
  promotionId,
}) {
  const output = resolve(outputDirectory);
  await requireEmptyOutputDirectory(output);
  const authorized = await validateAuthorizedManagedCloudCandidate(
    authorizedDirectory,
    expectedRootAnchorSha256,
  );
  const { value: cohort } = await readCanonicalJson(cohortFile, "cohort");
  validateCohort(cohort);
  if (cohort.trustGeneration !== authorized.policy.trustGeneration) {
    fail("Cohort trust generation does not match active policy");
  }
  const { value: cohortSignatures } = await readCanonicalJson(
    cohortSignatureSetFile,
    "cohort signatures",
  );
  const cohortSignerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: authorityRoleForChannel(cohort.scope.channel),
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.cohort,
    expectedTargetBytes: await readFile(cohortFile),
    policy: authorized.policy,
    signatureSet: cohortSignatures,
  });
  if (
    cohortSignatures.signedAtMs < cohort.issuedAtMs ||
    Date.now() < cohort.notBeforeMs ||
    Date.now() >= cohort.expiresAtMs
  ) {
    fail("Cohort signature or current validity window is invalid");
  }
  const { value: activation } = await readCanonicalJson(
    activationFile,
    "activation",
  );
  const evidenceHashes = await readActivationEvidence(
    evidenceDirectory,
    activation,
    authorized,
  );
  validateActivation(
    activation,
    authorized,
    cohort,
    await hashFile(cohortSignatureSetFile),
    evidenceHashes,
  );
  const { value: activationSignatures } = await readCanonicalJson(
    activationSignatureSetFile,
    "activation signatures",
  );
  const activationSignerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: authorityRoleForChannel(activation.scope.channel),
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.activation,
    expectedTargetBytes: await readFile(activationFile),
    policy: authorized.policy,
    signatureSet: activationSignatures,
  });
  if (activationSignatures.signedAtMs < activation.issuedAtMs) {
    fail("Activation signature cannot predate activation issuance");
  }
  await cp(resolve(authorizedDirectory), join(output, "authorized-candidate"), {
    errorOnExist: true,
    force: false,
    recursive: true,
  });
  await copyFile(activationFile, join(output, "activation.json"), 0);
  await copyFile(
    activationSignatureSetFile,
    join(output, "activation-signatures.json"),
    0,
  );
  await copyFile(cohortFile, join(output, "cohort.json"), 0);
  await copyFile(
    cohortSignatureSetFile,
    join(output, "cohort-signatures.json"),
    0,
  );
  await cp(resolve(evidenceDirectory), join(output, "evidence"), {
    errorOnExist: true,
    force: false,
    recursive: true,
  });
  const publishedPayloads = await copyPublishedPayloads(
    join(resolve(authorizedDirectory), "candidate"),
    join(output, "published"),
    join(output, "readback"),
  );
  const promotion = {
    activationSha256: await hashFile(activationFile),
    activationSignatureSetSha256: await hashFile(
      activationSignatureSetFile,
    ),
    activationSignerKeyIds,
    adapter: "local_fixture",
    audience: MANAGED_CLOUD_AUDIENCES.promotion,
    authorizationSetSha256: await hashFile(
      join(resolve(authorizedDirectory), "authorization-set.json"),
    ),
    cohortSha256: await hashFile(cohortFile),
    cohortSignatureSetSha256: await hashFile(cohortSignatureSetFile),
    cohortSignerKeyIds,
    manifestSha256: authorized.candidate.manifestSha256,
    promotedAtMs: Date.now(),
    promotionId: requireSafeId(promotionId, "promotionId"),
    publishedPayloads,
    readbackPayloads: publishedPayloads,
    releaseSignatureSetSha256:
      authorized.authorization.releaseSignatureSetSha256,
    trustGeneration: authorized.policy.trustGeneration,
    version: 1,
  };
  await writeCanonicalJson(join(output, "promotion-set.json"), promotion);
  return promotion;
}

export async function validateManagedCloudPromotion(
  promotionDirectory,
  expectedRootAnchorSha256,
) {
  const root = resolve(promotionDirectory);
  await requireExactTopLevel(root, REQUIRED_PROMOTION_FILES);
  const authorized = await validateAuthorizedManagedCloudCandidate(
    join(root, "authorized-candidate"),
    expectedRootAnchorSha256,
  );
  const { value: cohort } = await readCanonicalJson(join(root, "cohort.json"));
  validateCohort(cohort);
  if (cohort.trustGeneration !== authorized.policy.trustGeneration) {
    fail("Cohort trust generation does not match active policy");
  }
  const { value: cohortSignatures } = await readCanonicalJson(
    join(root, "cohort-signatures.json"),
  );
  const cohortSignerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: authorityRoleForChannel(cohort.scope.channel),
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.cohort,
    expectedTargetBytes: await readFile(join(root, "cohort.json")),
    policy: authorized.policy,
    signatureSet: cohortSignatures,
  });
  if (
    cohortSignatures.signedAtMs < cohort.issuedAtMs ||
    Date.now() < cohort.notBeforeMs ||
    Date.now() >= cohort.expiresAtMs
  ) {
    fail("Cohort signature or current validity window is invalid");
  }
  const { value: activation } = await readCanonicalJson(join(root, "activation.json"));
  const evidenceHashes = await readActivationEvidence(
    join(root, "evidence"),
    activation,
    authorized,
  );
  validateActivation(
    activation,
    authorized,
    cohort,
    await hashFile(join(root, "cohort-signatures.json")),
    evidenceHashes,
  );
  const { value: activationSignatures } = await readCanonicalJson(
    join(root, "activation-signatures.json"),
  );
  const activationSignerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: authorityRoleForChannel(activation.scope.channel),
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.activation,
    expectedTargetBytes: await readFile(join(root, "activation.json")),
    policy: authorized.policy,
    signatureSet: activationSignatures,
  });
  if (activationSignatures.signedAtMs < activation.issuedAtMs) {
    fail("Activation signature cannot predate activation issuance");
  }
  const { value: promotion } = await readCanonicalJson(
    join(root, "promotion-set.json"),
  );
  requireExactKeys(
    promotion,
    [
      "activationSha256",
      "activationSignatureSetSha256",
      "activationSignerKeyIds",
      "adapter",
      "audience",
      "authorizationSetSha256",
      "cohortSha256",
      "cohortSignatureSetSha256",
      "cohortSignerKeyIds",
      "manifestSha256",
      "promotedAtMs",
      "promotionId",
      "publishedPayloads",
      "readbackPayloads",
      "releaseSignatureSetSha256",
      "trustGeneration",
      "version",
    ],
    "promotion set",
  );
  requireSafeId(promotion.promotionId, "promotionId");
  requireInteger(promotion.promotedAtMs, "promotedAtMs");
  if (
    promotion.version !== 1 ||
    promotion.audience !== MANAGED_CLOUD_AUDIENCES.promotion ||
    promotion.adapter !== "local_fixture" ||
    promotion.manifestSha256 !== authorized.candidate.manifestSha256 ||
    promotion.activationSha256 !==
      (await hashFile(join(root, "activation.json"))) ||
    promotion.activationSignatureSetSha256 !==
      (await hashFile(join(root, "activation-signatures.json"))) ||
    promotion.cohortSha256 !==
      (await hashFile(join(root, "cohort.json"))) ||
    promotion.cohortSignatureSetSha256 !==
      (await hashFile(join(root, "cohort-signatures.json"))) ||
    promotion.authorizationSetSha256 !==
      (await hashFile(join(root, "authorized-candidate/authorization-set.json"))) ||
    promotion.releaseSignatureSetSha256 !==
      authorized.authorization.releaseSignatureSetSha256 ||
    promotion.trustGeneration !== authorized.policy.trustGeneration ||
    promotion.promotedAtMs < activation.issuedAtMs ||
    promotion.promotedAtMs > Date.now() ||
    JSON.stringify(promotion.activationSignerKeyIds) !==
      JSON.stringify(activationSignerKeyIds) ||
    JSON.stringify(promotion.cohortSignerKeyIds) !==
      JSON.stringify(cohortSignerKeyIds)
  ) {
    fail("Promotion set does not bind its exact authorized bytes");
  }
  const publishedPayloads = await validatePublishedPayloads(
    join(root, "published"),
    authorized.candidate,
  );
  const readbackPayloads = await validatePublishedPayloads(
    join(root, "readback"),
    authorized.candidate,
  );
  if (
    canonicalJsonBytes(promotion.publishedPayloads).compare(
      canonicalJsonBytes(publishedPayloads),
    ) !== 0 ||
    canonicalJsonBytes(promotion.readbackPayloads).compare(
      canonicalJsonBytes(readbackPayloads),
    ) !== 0
  ) {
    fail("Promotion receipt does not bind published and read-back bytes");
  }
  return { activation, authorized, promotion };
}

export function validateManagedCloudRollbackSuccessor(rollback, from, to) {
  const scope = validateScope(rollback.scope, "rollback.scope");
  if (
    canonicalJsonBytes(scope).compare(
      canonicalJsonBytes(from.activation.scope),
    ) !== 0 ||
    canonicalJsonBytes(scope).compare(
      canonicalJsonBytes(to.activation.scope),
    ) !== 0 ||
    to.activation.activationGeneration <=
      from.activation.activationGeneration ||
    to.activation.channelSequence <= from.activation.channelSequence ||
    to.activation.predecessorActivationSha256 !==
      from.promotion.activationSha256 ||
    rollback.expectedHeadRevision !== to.activation.expectedHeadRevision ||
    rollback.expectedTransitionSha256 !==
      to.activation.expectedTransitionSha256 ||
    to.authorized.manifest.releaseSequence >=
      from.authorized.manifest.releaseSequence ||
    to.promotion.manifestSha256 === from.promotion.manifestSha256 ||
    rollback.issuedAtMs < to.activation.issuedAtMs ||
    rollback.issuedAtMs > Date.now() ||
    rollback.trustGeneration !== to.authorized.policy.trustGeneration
  ) {
    fail(
      "Rollback must use a newly signed successor activation over older release bytes",
    );
  }
  return scope;
}

export async function createManagedCloudRollback({
  expectedRootAnchorSha256,
  fromPromotionDirectory,
  outputDirectory,
  rollbackFile,
  rollbackSignatureSetFile,
  toPromotionDirectory,
}) {
  const output = resolve(outputDirectory);
  await requireEmptyOutputDirectory(output);
  const from = await validateManagedCloudPromotion(
    fromPromotionDirectory,
    expectedRootAnchorSha256,
  );
  const to = await validateManagedCloudPromotion(
    toPromotionDirectory,
    expectedRootAnchorSha256,
  );
  const { value: rollback } = await readCanonicalJson(rollbackFile, "rollback");
  requireSignedObjectSize(rollback, "rollback");
  requireExactKeys(
    rollback,
    [
      "audience",
      "evidenceSha256",
      "expectedHeadRevision",
      "expectedTransitionSha256",
      "fromActivationSha256",
      "fromManifestSha256",
      "issuedAtMs",
      "reasonRef",
      "rollbackGeneration",
      "rollbackId",
      "scope",
      "toActivationSha256",
      "toManifestSha256",
      "trustGeneration",
      "version",
    ],
    "rollback",
  );
  if (
    rollback.version !== 1 ||
    rollback.audience !== MANAGED_CLOUD_AUDIENCES.rollback
  ) {
    fail("Rollback audience or schema version is invalid");
  }
  requireSafeId(rollback.rollbackId, "rollbackId");
  requireString(rollback.reasonRef, "reasonRef");
  requireInteger(rollback.rollbackGeneration, "rollbackGeneration", 1);
  requireInteger(rollback.trustGeneration, "rollback.trustGeneration", 1);
  requireInteger(rollback.issuedAtMs, "rollback.issuedAtMs");
  const scope = validateScope(rollback.scope, "rollback.scope");
  requireInteger(rollback.expectedHeadRevision, "expectedHeadRevision", 1);
  requireHex64(
    rollback.expectedTransitionSha256,
    "expectedTransitionSha256",
  );
  for (const key of [
    "fromActivationSha256",
    "fromManifestSha256",
    "toActivationSha256",
    "toManifestSha256",
    "evidenceSha256",
  ]) {
    requireHex64(rollback[key], "rollback." + key);
  }
  const expectedBindings = [
    ["fromActivationSha256", from.promotion.activationSha256],
    ["fromManifestSha256", from.promotion.manifestSha256],
    ["toActivationSha256", to.promotion.activationSha256],
    ["toManifestSha256", to.promotion.manifestSha256],
  ];
  for (const [key, expected] of expectedBindings) {
    if (rollback[key] !== expected) {
      fail("Rollback does not bind " + key);
    }
  }
  validateManagedCloudRollbackSuccessor(rollback, from, to);
  const { value: signatures } = await readCanonicalJson(
    rollbackSignatureSetFile,
    "rollback signatures",
  );
  const signerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: authorityRoleForChannel(scope.channel),
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.rollback,
    expectedTargetBytes: await readFile(rollbackFile),
    policy: to.authorized.policy,
    signatureSet: signatures,
  });
  if (signatures.signedAtMs < rollback.issuedAtMs) {
    fail("Rollback signature cannot predate rollback issuance");
  }
  await cp(resolve(fromPromotionDirectory), join(output, "from-promotion"), {
    errorOnExist: true,
    force: false,
    recursive: true,
  });
  await cp(resolve(toPromotionDirectory), join(output, "target-promotion"), {
    errorOnExist: true,
    force: false,
    recursive: true,
  });
  await copyFile(rollbackFile, join(output, "rollback.json"), 0);
  await copyFile(
    rollbackSignatureSetFile,
    join(output, "rollback-signatures.json"),
    0,
  );
  const rollbackSet = {
    audience: "bluey-jobs-managed-cloud-rollback-set-v1",
    fromPromotionSha256: await hashFile(
      join(resolve(fromPromotionDirectory), "promotion-set.json"),
    ),
    rollbackSha256: await hashFile(rollbackFile),
    rollbackSignatureSetSha256: await hashFile(rollbackSignatureSetFile),
    signerKeyIds,
    targetPromotionSha256: await hashFile(
      join(resolve(toPromotionDirectory), "promotion-set.json"),
    ),
    version: 1,
  };
  await writeCanonicalJson(join(output, "rollback-set.json"), rollbackSet);
  return rollbackSet;
}

export async function validateManagedCloudRollback(
  rollbackDirectory,
  expectedRootAnchorSha256,
) {
  const root = resolve(rollbackDirectory);
  await requireExactTopLevel(root, REQUIRED_ROLLBACK_FILES);
  const from = await validateManagedCloudPromotion(
    join(root, "from-promotion"),
    expectedRootAnchorSha256,
  );
  const to = await validateManagedCloudPromotion(
    join(root, "target-promotion"),
    expectedRootAnchorSha256,
  );
  const { value: rollback } = await readCanonicalJson(
    join(root, "rollback.json"),
  );
  requireSignedObjectSize(rollback, "rollback");
  requireExactKeys(
    rollback,
    [
      "audience",
      "evidenceSha256",
      "expectedHeadRevision",
      "expectedTransitionSha256",
      "fromActivationSha256",
      "fromManifestSha256",
      "issuedAtMs",
      "reasonRef",
      "rollbackGeneration",
      "rollbackId",
      "scope",
      "toActivationSha256",
      "toManifestSha256",
      "trustGeneration",
      "version",
    ],
    "rollback",
  );
  if (
    rollback.version !== 1 ||
    rollback.audience !== MANAGED_CLOUD_AUDIENCES.rollback
  ) {
    fail("Rollback audience or version is invalid");
  }
  requireSafeId(rollback.rollbackId, "rollbackId");
  requireInteger(rollback.rollbackGeneration, "rollbackGeneration", 1);
  requireInteger(rollback.trustGeneration, "rollback.trustGeneration", 1);
  requireInteger(rollback.expectedHeadRevision, "expectedHeadRevision", 1);
  requireInteger(rollback.issuedAtMs, "rollback.issuedAtMs");
  requireString(rollback.reasonRef, "rollback.reasonRef");
  for (const key of [
    "evidenceSha256",
    "expectedTransitionSha256",
    "fromActivationSha256",
    "fromManifestSha256",
    "toActivationSha256",
    "toManifestSha256",
  ]) {
    requireHex64(rollback[key], "rollback." + key);
  }
  const { value: signatures } = await readCanonicalJson(
    join(root, "rollback-signatures.json"),
  );
  const scope = validateScope(rollback.scope, "rollback.scope");
  if (
    rollback.fromActivationSha256 !== from.promotion.activationSha256 ||
    rollback.fromManifestSha256 !== from.promotion.manifestSha256 ||
    rollback.toActivationSha256 !== to.promotion.activationSha256 ||
    rollback.toManifestSha256 !== to.promotion.manifestSha256 ||
    rollback.trustGeneration !== to.authorized.policy.trustGeneration
  ) {
    fail("Rollback readback does not bind a successor activation over old bytes");
  }
  validateManagedCloudRollbackSuccessor(rollback, from, to);
  const signerKeyIds = verifyManagedCloudSignatureSet({
    expectedRole: authorityRoleForChannel(scope.channel),
    expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.rollback,
    expectedTargetBytes: await readFile(join(root, "rollback.json")),
    policy: to.authorized.policy,
    signatureSet: signatures,
  });
  if (signatures.signedAtMs < rollback.issuedAtMs) {
    fail("Rollback signature cannot predate rollback issuance");
  }
  const { value: rollbackSet } = await readCanonicalJson(
    join(root, "rollback-set.json"),
  );
  requireExactKeys(
    rollbackSet,
    [
      "audience",
      "fromPromotionSha256",
      "rollbackSha256",
      "rollbackSignatureSetSha256",
      "signerKeyIds",
      "targetPromotionSha256",
      "version",
    ],
    "rollback set",
  );
  if (
    rollbackSet.version !== 1 ||
    rollbackSet.audience !== "bluey-jobs-managed-cloud-rollback-set-v1" ||
    rollbackSet.fromPromotionSha256 !==
      (await hashFile(join(root, "from-promotion/promotion-set.json"))) ||
    rollbackSet.targetPromotionSha256 !==
      (await hashFile(join(root, "target-promotion/promotion-set.json"))) ||
    rollbackSet.rollbackSha256 !==
      (await hashFile(join(root, "rollback.json"))) ||
    rollbackSet.rollbackSignatureSetSha256 !==
      (await hashFile(join(root, "rollback-signatures.json"))) ||
    JSON.stringify(rollbackSet.signerKeyIds) !== JSON.stringify(signerKeyIds)
  ) {
    fail("Rollback set does not survive full authority readback");
  }
  return { from, rollback, rollbackSet, to };
}

function markedSection(text, marker) {
  const begin = "# RELEASE-GATE: " + marker + "-BEGIN";
  const end = "# RELEASE-GATE: " + marker + "-END";
  const start = text.indexOf(begin);
  const finish = text.indexOf(end);
  if (start < 0 || finish <= start || text.indexOf(begin, start + 1) >= 0) {
    fail("Release workflow is missing one unambiguous " + marker + " section");
  }
  return text.slice(start + begin.length, finish);
}

export async function validateManagedCloudWorkflowContract(workflowFile) {
  const text = await readFile(workflowFile, "utf8");
  for (const marker of [
    "CANDIDATE-BUILD",
    "VERIFY-NO-REBUILD",
    "AUTHORIZE-NO-REBUILD",
    "PROMOTE-NO-REBUILD",
    "ROLLBACK-NO-REBUILD",
  ]) {
    markedSection(text, marker);
  }
  const forbiddenCommands = [
    /\b(?:cargo\s+(?:build|install)|docker\s+(?:build|buildx))\b/i,
    /\b(?:npm\s+(?:ci|install|run\s+build)|pnpm\s+(?:install|build))\b/i,
    /\byarn\s+(?:install|build)\b/i,
  ];
  const containsForbiddenCommand = (value) =>
    forbiddenCommands.some((pattern) => pattern.test(value));
  const candidateBuildSection = markedSection(text, "CANDIDATE-BUILD");
  if (containsForbiddenCommand(text.replace(candidateBuildSection, ""))) {
    fail("Artifact installation/build commands must be confined to candidate build");
  }
  for (const marker of [
    "VERIFY-NO-REBUILD",
    "AUTHORIZE-NO-REBUILD",
    "PROMOTE-NO-REBUILD",
    "ROLLBACK-NO-REBUILD",
  ]) {
    if (containsForbiddenCommand(markedSection(text, marker))) {
      fail(marker + " must not install dependencies or rebuild artifacts");
    }
  }
  if (/\$\{\{\s*secrets\./.test(markedSection(text, "CANDIDATE-BUILD"))) {
    fail("Candidate build section must not receive repository secrets");
  }
  for (const match of text.matchAll(/uses:\s*([^\s#]+)/g)) {
    if (
      !/^(?:actions\/checkout|actions\/download-artifact|actions\/upload-artifact)@[0-9a-f]{40}$/.test(
        match[1],
      )
    ) {
      fail("Every GitHub Action must be pinned by full commit SHA");
    }
  }
  if (
    !text.includes("directDiscovery: false") ||
    !text.includes("globalDiscovery: false") ||
    !text.includes("sourceVerification: false") ||
    text.includes("original-source-verifier.js") ||
    text.includes("original_source_verifier.js")
  ) {
    fail("Workflow must reserve, but never claim, conditional Phase 612 workers");
  }
  if (
    !text.includes("vars.BLUEY_JOBS_MANAGED_CLOUD_ROOT_ANCHOR_SHA256") ||
    /inputs\.[A-Za-z0-9_]*root[A-Za-z0-9_]*anchor/i.test(text)
  ) {
    fail("Workflow must use the protected root-anchor digest, never caller input");
  }
  return true;
}

function parseCli(argv) {
  const [command, ...tokens] = argv;
  const options = {};
  for (let index = 0; index < tokens.length; index += 2) {
    const key = tokens[index];
    const value = tokens[index + 1];
    if (!key?.startsWith("--") || value === undefined || value.startsWith("--")) {
      fail("CLI options must use --name value pairs");
    }
    options[key.slice(2)] = value;
  }
  return { command, options };
}

function requireOption(options, name) {
  return requireString(options[name], "--" + name);
}

async function main(argv) {
  const { command, options } = parseCli(argv);
  if (command === "contracts") {
    await createManagedCloudContracts(
      requireOption(options, "repo"),
      requireOption(options, "out"),
    );
    return;
  }
  if (command === "inventory-tree") {
    const inventory = await createStaticTreeInventory(
      requireOption(options, "root"),
      requireOption(options, "root-name"),
    );
    await writeCanonicalJson(requireOption(options, "out"), inventory);
    return;
  }
  if (command === "runtime-measurement") {
    const measurement = await createRuntimeMeasurementFromFilesystem({
      buildId: requireOption(options, "build-id"),
      componentId: requireOption(options, "component"),
      configSchemaSha256: requireOption(options, "config-schema-sha256"),
      migrationSetSha256: requireOption(options, "migration-set-sha256"),
      protocolSetSha256: requireOption(options, "protocol-set-sha256"),
      roles: requireOption(options, "roles").split(","),
      root: requireOption(options, "root"),
      sourceCommit: requireOption(options, "source-commit"),
    });
    await writeCanonicalJson(requireOption(options, "out"), measurement);
    return;
  }
  if (command === "inspect-artifact") {
    const inventory = await inspectPreparedArtifact(
      requireOption(options, "file"),
      requireOption(options, "component"),
      requireOption(options, "kind"),
    );
    await writeCanonicalJson(requireOption(options, "out"), inventory);
    return;
  }
  if (command === "sbom") {
    const { value: inventory } = await readCanonicalJson(
      requireOption(options, "inventory"),
      "content inventory",
    );
    await writeCanonicalJson(
      requireOption(options, "out"),
      createFileSbom(inventory),
    );
    return;
  }
  if (command === "provenance") {
    const { value: inventory } = await readCanonicalJson(
      requireOption(options, "inventory"),
      "content inventory",
    );
    await writeCanonicalJson(
      requireOption(options, "out"),
      createArtifactProvenance({
        artifactSha256: inventory.artifactSha256,
        buildId: requireOption(options, "build-id"),
        builderPolicySha256: requireOption(
          options,
          "builder-policy-sha256",
        ),
        componentId: inventory.componentId,
        sourceCommit: requireOption(options, "source-commit"),
        sourceDateEpoch: Number(requireOption(options, "source-date-epoch")),
      }),
    );
    return;
  }
  if (command === "assemble") {
    await assembleManagedCloudCandidate({
      contractsDirectory: requireOption(options, "contracts"),
      descriptorFile: requireOption(options, "descriptor"),
      outputDirectory: requireOption(options, "out"),
      repoRoot: requireOption(options, "repo"),
    });
    return;
  }
  if (command === "verify") {
    await validateManagedCloudCandidate(requireOption(options, "candidate"));
    return;
  }
  if (command === "authorize") {
    await authorizeManagedCloudCandidate({
      authorizationId: requireOption(options, "authorization-id"),
      candidateDirectory: requireOption(options, "candidate"),
      expectedRootAnchorSha256: requireOption(
        options,
        "root-anchor-sha256",
      ),
      outputDirectory: requireOption(options, "out"),
      signatureSetFile: requireOption(options, "signatures"),
      trustBundleFile: requireOption(options, "trust-bundle"),
    });
    return;
  }
  if (command === "promote") {
    await promoteManagedCloudCandidate({
      activationFile: requireOption(options, "activation"),
      activationSignatureSetFile: requireOption(
        options,
        "activation-signatures",
      ),
      authorizedDirectory: requireOption(options, "authorized"),
      cohortFile: requireOption(options, "cohort"),
      cohortSignatureSetFile: requireOption(options, "cohort-signatures"),
      evidenceDirectory: requireOption(options, "evidence"),
      expectedRootAnchorSha256: requireOption(
        options,
        "root-anchor-sha256",
      ),
      outputDirectory: requireOption(options, "out"),
      promotionId: requireOption(options, "promotion-id"),
    });
    return;
  }
  if (command === "rollback") {
    await createManagedCloudRollback({
      expectedRootAnchorSha256: requireOption(
        options,
        "root-anchor-sha256",
      ),
      fromPromotionDirectory: requireOption(options, "from-promotion"),
      outputDirectory: requireOption(options, "out"),
      rollbackFile: requireOption(options, "rollback"),
      rollbackSignatureSetFile: requireOption(options, "rollback-signatures"),
      toPromotionDirectory: requireOption(options, "to-promotion"),
    });
    return;
  }
  if (command === "workflow") {
    await validateManagedCloudWorkflowContract(requireOption(options, "file"));
    return;
  }
  fail(
    "Usage: managed-cloud-release-gate.mjs " +
      "<contracts|inventory-tree|runtime-measurement|inspect-artifact|sbom|provenance|" +
      "assemble|verify|authorize|promote|rollback|workflow>",
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
