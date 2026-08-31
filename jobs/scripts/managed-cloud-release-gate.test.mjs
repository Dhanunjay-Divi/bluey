import assert from "node:assert/strict";
import {
  generateKeyPairSync,
  sign as signBytes,
} from "node:crypto";
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  canonicalJsonBytes,
  createRuntimeMeasurementFromFilesystem,
  deriveManagedCloudCanaryCheckIds,
  deriveManagedCloudRequiredRuntimePaths,
  deriveRequiredRuntimeRoles,
  deriveManagedCloudRuntimeIdentitySha256,
  deriveManagedCloudTaskQueueSha256,
  inspectOciImageArchive,
  inspectStaticBundleArchive,
  MANAGED_CLOUD_AUDIENCES,
  MANAGED_CLOUD_BASE_CAPABILITIES,
  MANAGED_CLOUD_BASE_RUNTIME_ROLES,
  MANAGED_CLOUD_COMPONENTS,
  MANAGED_CLOUD_PROTOCOL_IDS,
  MANAGED_CLOUD_SUCCESSOR_PROTOCOL_IDS,
  MANAGED_CLOUD_RUNTIME_CONTRACTS,
  sha256,
  validateManagedCloudActivationEvidence,
  validateManagedCloudActivationWindow,
  validateManagedCloudInventoryAttachments,
  validateManagedCloudReleaseContracts,
  validateManagedCloudRollbackSuccessor,
  validateManagedCloudRuntimeMeasurement,
  validateManagedCloudTrustBundle,
  validateManagedCloudWorkflowContract,
  validateReleaseManifest,
  verifyManagedCloudSignatureSet,
} from "./managed-cloud-release-gate.mjs";

const SOURCE_COMMIT = "a".repeat(40);
const RELEASE_ID = "managed-cloud-611-001";
const SAFE_PATH =
  "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const PROTOCOL_VERSIONS = new Map([
  ["ats_certification", 1],
  ["execution_lease", 1],
  ["gateway_command", 3],
  ["managed_cloud_release", 1],
  ["object_evidence", 1],
  ["runner_checkpoint", 2],
  ["runner_profile_snapshot", 1],
  ["runner_result", 2],
  ["runtime_heartbeat", 1],
  ["source_verification", 1],
  ["workflow_cleanup", 3],
  ["workflow_command", 2],
]);
const CANARY_CHECK_IDS = [
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
].sort();
const SOURCE_VERIFICATION_PROTOCOL_PATHS = [
  "jobs/automation/src/original-source-verification.ts",
  "jobs/automation/src/worker-auth.ts",
  "jobs/workflows/src/original-source-verification-api.ts",
  "jobs/workflows/src/original-source-verification-runtime.ts",
  "jobs/workflows/src/original-source-verifier.ts",
  "server/src/api/jobs_original_source_verifications.rs",
  "server/src/api/jobs_worker_auth.rs",
  "server/src/db/mod.rs",
  "server/src/db/jobs/eligibility.rs",
  "server/src/db/jobs/operational_holds.rs",
  "server/src/db/jobs/original_source_verification.rs",
  "server/src/jobs_ats_target.rs",
];

async function temporaryDirectory(t, label) {
  const path = await mkdtemp(join(tmpdir(), "bluey-" + label + "-"));
  t.after(() => rm(path, { force: true, recursive: true }));
  return path;
}

async function writeCanonical(path, value) {
  await mkdir(join(path, ".."), { recursive: true });
  await writeFile(path, canonicalJsonBytes(value));
}

function octal(value, width) {
  return value.toString(8).padStart(width - 1, "0") + "\0";
}

function tarArchive(entries) {
  const blocks = [];
  for (const item of entries) {
    const body = Buffer.from(item.body ?? "");
    const type = item.type ?? "0";
    const header = Buffer.alloc(512);
    const name = item.name;
    if (Buffer.byteLength(name) > 100) throw new Error("fixture path too long");
    header.write(name, 0, 100, "utf8");
    header.write(octal(item.mode ?? (type === "5" ? 0o555 : 0o444), 8), 100);
    header.write(octal(item.uid ?? 0, 8), 108);
    header.write(octal(item.gid ?? 0, 8), 116);
    header.write(octal(type === "0" ? body.length : 0, 12), 124);
    header.write(octal(item.mtime ?? 0, 12), 136);
    header.fill(32, 148, 156);
    header.write(type, 156, 1, "ascii");
    if (item.linkTarget) header.write(item.linkTarget, 157, 100, "utf8");
    header.write("ustar\0", 257, 6, "binary");
    header.write("00", 263, 2, "ascii");
    let checksum = 0;
    for (const byte of header) checksum += byte;
    header.write(octal(checksum, 8), 148, 8, "ascii");
    blocks.push(header);
    if (type === "0") {
      blocks.push(body);
      const padding = (512 - (body.length % 512)) % 512;
      if (padding > 0) blocks.push(Buffer.alloc(padding));
    }
  }
  blocks.push(Buffer.alloc(1024));
  return Buffer.concat(blocks);
}

function descriptor(bytes, mediaType) {
  return {
    digest: "sha256:" + sha256(bytes),
    mediaType,
    size: bytes.length,
  };
}

function apiRuntime(overrides = {}) {
  return {
    Cmd: [],
    Entrypoint: ["/usr/local/bin/bluey-jobs-api"],
    Env: [
      "BLUEY_JOBS_API_HOST=0.0.0.0",
      "BLUEY_JOBS_API_PORT=8081",
      "PATH=" + SAFE_PATH,
    ],
    ExposedPorts: { "8081/tcp": {} },
    Labels: {},
    User: "65532:65532",
    WorkingDir: "/app",
    ...overrides,
  };
}

function runnerRuntime(overrides = {}) {
  return {
    Cmd: ["/usr/local/bin/node", "runner/dist/server.js"],
    Entrypoint: [],
    Env: [
      "BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner",
      "NODE_ENV=production",
      "PATH=" + SAFE_PATH,
      "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright",
    ],
    ExposedPorts: { "8091/tcp": {} },
    Labels: {},
    User: "pwuser",
    WorkingDir: "/app",
    ...overrides,
  };
}

async function writeOciFixture(path, {
  extraBlob,
  layers = [
    [
      { name: "app", type: "5" },
      {
        body: "api-binary\n",
        mode: 0o555,
        name: "usr/local/bin/bluey-jobs-api",
      },
    ],
  ],
  runtime = apiRuntime(),
} = {}) {
  const exactLayers = layers.map((entries) => [...entries]);
  if (!exactLayers.flat().some(
    (entry) => entry.name ===
      "app/.bluey/managed-cloud-runtime-measurement.json",
  )) {
    exactLayers[0].push({
      body: "{}\n",
      name: "app/.bluey/managed-cloud-runtime-measurement.json",
    });
  }
  const layerBytes = exactLayers.map((entries) => tarArchive(entries));
  const layerDescriptors = layerBytes.map((bytes) =>
    descriptor(bytes, "application/vnd.oci.image.layer.v1.tar"),
  );
  const configBytes = Buffer.from(
    JSON.stringify({
      architecture: "amd64",
      config: runtime,
      os: "linux",
      rootfs: {
        diff_ids: layerDescriptors.map((item) => item.digest),
        type: "layers",
      },
    }),
  );
  const configDescriptor = descriptor(
    configBytes,
    "application/vnd.oci.image.config.v1+json",
  );
  const manifestBytes = Buffer.from(
    JSON.stringify({
      config: configDescriptor,
      layers: layerDescriptors,
      schemaVersion: 2,
    }),
  );
  const manifestDescriptor = descriptor(
    manifestBytes,
    "application/vnd.oci.image.manifest.v1+json",
  );
  const indexBytes = Buffer.from(
    JSON.stringify({ manifests: [manifestDescriptor], schemaVersion: 2 }),
  );
  const outerEntries = [
    { body: JSON.stringify({ imageLayoutVersion: "1.0.0" }), name: "oci-layout" },
    { body: indexBytes, name: "index.json" },
    {
      body: manifestBytes,
      name: "blobs/sha256/" + manifestDescriptor.digest.slice(7),
    },
    {
      body: configBytes,
      name: "blobs/sha256/" + configDescriptor.digest.slice(7),
    },
    ...layerBytes.map((bytes, index) => ({
      body: bytes,
      name: "blobs/sha256/" + layerDescriptors[index].digest.slice(7),
    })),
  ];
  if (extraBlob) {
    outerEntries.push({
      body: extraBlob,
      name: "blobs/sha256/" + sha256(Buffer.from(extraBlob)),
    });
  }
  await writeFile(path, tarArchive(outerEntries));
  return manifestDescriptor.digest.slice(7);
}

function releaseFixture() {
  const configSchemaSha256 = "c".repeat(64);
  const digestCharacters = ["1", "2", "3", "4"];
  const components = MANAGED_CLOUD_COMPONENTS.map((expected, index) => {
    const artifactSha256 = digestCharacters[index].repeat(64);
    return {
      architecture: expected.architecture,
      artifactKind: expected.artifactKind,
      artifactRef:
        expected.artifactKind === "oci_image"
          ? "registry.example/bluey/" + expected.componentId +
            "@sha256:" + artifactSha256
          : "https://artifacts.example/releases/" + RELEASE_ID + "/" +
            artifactSha256 + ".tar",
      artifactSha256,
      buildId: "build-" + expected.componentId,
      componentId: expected.componentId,
      configSchemaSha256,
      platform: expected.platform,
      provenanceSha256: (index + 5).toString(16).repeat(64),
      sbomSha256: (index + 9).toString(16).repeat(64),
      sourceCommit: SOURCE_COMMIT,
    };
  });
  const capabilities = MANAGED_CLOUD_BASE_CAPABILITIES.map((item) => ({
    capability: item.capability,
    componentId: item.componentId,
  }));
  const featureAuthority = {
    cloudDistribution: true,
    directDiscovery: false,
    globalDiscovery: false,
    sourceVerification: false,
    workflowCleanup: true,
    workflowCommandDispatch: true,
  };
  return {
    audience: MANAGED_CLOUD_AUDIENCES.manifest,
    capabilities,
    components,
    componentSetSha256: sha256(canonicalJsonBytes({
      audience: MANAGED_CLOUD_AUDIENCES.componentInventory,
      capabilities,
      components,
      version: 1,
    })),
    configSchemaSha256,
    featureAuthority,
    featureAuthoritySha256: sha256(canonicalJsonBytes(featureAuthority)),
    manifestGeneration: 611,
    manifestId: "manifest-611-001",
    migrationSetSha256: "d".repeat(64),
    postgresMigrationHead: "034_jobs_canonical_taxonomy_authority.sql",
    protocolSetSha256: "e".repeat(64),
    protocols: MANAGED_CLOUD_PROTOCOL_IDS.map((protocolId, index) => ({
      protocolId,
      protocolVersion: PROTOCOL_VERSIONS.get(protocolId),
      schemaSha256: (index % 10).toString(16).repeat(64),
    })),
    publishedAtMs: 1_786_700_000_000,
    releaseId: RELEASE_ID,
    releaseSequence: 611001,
    sourceCommit: SOURCE_COMMIT,
    sqliteMigrationHead: "056_jobs_canonical_taxonomy_authority.sql",
    verificationEvidenceSha256: "f".repeat(64),
    version: 1,
  };
}

function successorReleaseFixture() {
  const manifest = releaseFixture();
  manifest.version = 2;
  manifest.audience = MANAGED_CLOUD_AUDIENCES.manifestV2;
  manifest.sqliteMigrationHead =
    "057_jobs_original_source_verification_authority.sql";
  manifest.postgresMigrationHead =
    "035_jobs_original_source_verification_authority.sql";
  manifest.featureAuthority = {
    ...manifest.featureAuthority,
    sourceVerification: true,
  };
  manifest.featureAuthoritySha256 = sha256(
    canonicalJsonBytes(manifest.featureAuthority),
  );
  manifest.capabilities = [
    ...manifest.capabilities,
    {
      capability: "original_source_verifier",
      componentId: "jobs-workflows",
    },
  ].sort((left, right) =>
    left.componentId.localeCompare(right.componentId, "en") ||
    left.capability.localeCompare(right.capability, "en"),
  );
  manifest.protocols = MANAGED_CLOUD_SUCCESSOR_PROTOCOL_IDS.map(
    (protocolId, index) => ({
      protocolId,
      protocolVersion: PROTOCOL_VERSIONS.get(protocolId),
      schemaSha256: ((index + 1) % 10).toString(16).repeat(64),
    }),
  );
  manifest.componentSetSha256 = sha256(canonicalJsonBytes({
    audience: MANAGED_CLOUD_AUDIENCES.componentInventory,
    capabilities: manifest.capabilities,
    components: manifest.components,
    version: 1,
  }));
  return manifest;
}

function sourceFile(path, fill = "a") {
  return { path, sha256: fill.repeat(64), sizeBytes: 1 };
}

function sourceSet(paths, fill = "a") {
  const files = [...paths].sort().map((path) => sourceFile(path, fill));
  return { files, setSha256: sha256(canonicalJsonBytes(files)) };
}

function releaseContractFixture() {
  const builderPaths = [
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
  ];
  const builderSources = sourceSet(builderPaths);
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
  const protocolSources = new Map([
    ["ats_certification", [
      "jobs/automation/src/certified-submit-form.ts",
      "server/src/db/jobs/ats_certification_authority.rs",
    ]],
    ["execution_lease", [
      "jobs/runner/src/execution-lease.ts",
      "server/src/db/jobs/execution_leases.rs",
    ]],
    ["gateway_command", [
      "jobs/workflows/src/contracts.ts",
      "jobs/workflows/src/gateway-service.ts",
      "jobs/workflows/src/gateway.ts",
      "server/src/jobs_workflow_dispatch.rs",
    ]],
    ["managed_cloud_release", [
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
    ]],
    ["object_evidence", [
      "server/src/db/object_uploads.rs",
      "server/src/db/jobs/evidence.rs",
    ]],
    ["runner_checkpoint", [
      "jobs/runner/src/run-checkpoint-store.ts",
      "server/src/db/jobs/execution_leases.rs",
    ]],
    ["runner_profile_snapshot", [
      "jobs/runner/src/profile-snapshot-client.ts",
      "server/src/db/jobs/browser_profile_snapshots.rs",
    ]],
    ["runner_result", [
      "jobs/runner/src/result-store.ts",
      "jobs/runner/src/submitted-result-recovery.ts",
      "server/src/db/jobs/local_runner.rs",
    ]],
    ["runtime_heartbeat", [
      "jobs/automation/src/managed-cloud-runtime-client.ts",
      "jobs/automation/src/managed-cloud-runtime.ts",
      "server/src/api/jobs_worker_auth.rs",
      "server/src/jobs_managed_cloud_runtime.rs",
    ]],
    ["workflow_cleanup", [
      "jobs/workflows/src/gateway-cleanup-service.ts",
      "jobs/workflows/src/gateway.ts",
      "server/src/jobs_workflow_cleanup.rs",
    ]],
    ["workflow_command", [
      "jobs/workflows/src/contracts.ts",
      "jobs/workflows/src/gateway-service.ts",
      "jobs/workflows/src/gateway.ts",
      "jobs/workflows/src/workflows.ts",
      "server/src/db/jobs/workflow_commands.rs",
      "server/src/jobs_workflow_dispatch.rs",
    ]],
  ]);
  const protocols = MANAGED_CLOUD_PROTOCOL_IDS.map((protocolId, index) => {
    const sources = sourceSet(protocolSources.get(protocolId),
      ((index + 1) % 10).toString(16));
    return {
      protocolId,
      protocolVersion: PROTOCOL_VERSIONS.get(protocolId),
      schemaSha256: sources.setSha256,
      sourceFiles: sources.files,
    };
  });
  const protocolContract = {
    audience: MANAGED_CLOUD_AUDIENCES.protocolContract,
    protocols,
    schemaVersion: 1,
  };
  const migrationFiles = {
    postgres: [sourceFile(
      "infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql",
      "b",
    )],
    sqlite: [sourceFile(
      "infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql",
      "c",
    )],
  };
  const migrationContract = {
    audience: MANAGED_CLOUD_AUDIENCES.migrationContract,
    paritySha256: sha256(canonicalJsonBytes({
      postgres: ["034_jobs_canonical_taxonomy_authority.sql"],
      sqlite: ["056_jobs_canonical_taxonomy_authority.sql"],
    })),
    postgres: {
      files: migrationFiles.postgres,
      head: "034_jobs_canonical_taxonomy_authority.sql",
      name: "postgres",
      setSha256: sha256(canonicalJsonBytes(migrationFiles.postgres)),
    },
    schemaVersion: 1,
    sqlite: {
      files: migrationFiles.sqlite,
      head: "056_jobs_canonical_taxonomy_authority.sql",
      name: "sqlite",
      setSha256: sha256(canonicalJsonBytes(migrationFiles.sqlite)),
    },
  };
  const roleSourcePaths = new Map([
    ["jobs_api", ["server/src/config.rs"]],
    ["jobs_workflows", ["jobs/workflows/src/worker.ts"]],
    ["managed_runner", ["jobs/runner/src/server.ts"]],
    ["portal_static", ["jobs/portal/vite.config.ts"]],
  ]);
  const roleSources = [...roleSourcePaths].map(([role, paths], index) => {
    const sources = sourceSet(paths, (index + 4).toString(16));
    return { role, sourceFiles: sources.files, sourceSetSha256: sources.setSha256 };
  });
  const sharedRoles = ["jobs_api", "jobs_workflows", "managed_runner"];
  const jobsApiRoles = ["jobs_api"];
  const jobsApiAndWorkflowsRoles = ["jobs_api", "jobs_workflows"];
  const variables = [
    [
      "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
      "deny_only_authority",
      jobsApiRoles,
      "boolean_true",
    ],
    ["BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN", "configuration", sharedRoles, "https_url"],
    ["BLUEY_JOBS_MANAGED_CLOUD_CHANNEL", "configuration", sharedRoles, "canonical_id"],
    ["BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT", "configuration", sharedRoles, "canonical_id"],
    ["BLUEY_JOBS_MANAGED_CLOUD_HEARTBEAT_SECONDS", "configuration", sharedRoles, "positive_integer"],
    ["BLUEY_JOBS_MANAGED_CLOUD_REGION", "configuration", sharedRoles, "canonical_id"],
    ["BLUEY_JOBS_WORKER_SIGNING_KEY", "secret", sharedRoles, "secret_bytes"],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED",
      "deny_only_authority",
      sharedRoles,
      "boolean_true",
    ],
    [
      "BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED",
      "deny_only_authority",
      jobsApiAndWorkflowsRoles,
      "boolean_true",
    ],
    [
      "BLUEY_JOBS_WORKFLOW_CLEANUP_VISIBILITY_CUTOFF_MS",
      "configuration",
      jobsApiRoles,
      "positive_integer",
    ],
    [
      "BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED",
      "deny_only_authority",
      jobsApiRoles,
      "boolean_true",
    ],
    [
      "BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED",
      "deny_only_authority",
      jobsApiRoles,
      "boolean_true",
    ],
    [
      "BLUEY_JOBS_WORKFLOW_NAMESPACE",
      "configuration",
      jobsApiAndWorkflowsRoles,
      "canonical_id",
    ],
    [
      "BLUEY_JOBS_WORKFLOW_ORIGIN",
      "configuration",
      jobsApiRoles,
      "https_url",
    ],
    [
      "BLUEY_JOBS_WORKFLOW_TOKEN",
      "secret",
      jobsApiAndWorkflowsRoles,
      "secret_bytes",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_ID",
      "configuration",
      ["jobs_workflows", "managed_runner"],
      "canonical_id",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_TOKEN",
      "secret",
      ["jobs_workflows", "managed_runner"],
      "secret_bytes",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_INSTANCE_ID",
      "configuration",
      ["jobs_workflows", "managed_runner"],
      "canonical_id",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_WORKER_ID",
      "configuration",
      ["jobs_workflows", "managed_runner"],
      "canonical_id",
    ],
    ...[
      "JOBS_API",
      "WORKFLOW_CLEANUP_DISPATCHER",
      "WORKFLOW_COMMAND_DISPATCHER",
    ].flatMap((role) => [
      [`BLUEY_JOBS_MANAGED_CLOUD_${role}_GRANT_ID`, "configuration", ["jobs_api"], "canonical_id"],
      [`BLUEY_JOBS_MANAGED_CLOUD_${role}_GRANT_TOKEN`, "secret", ["jobs_api"], "secret_bytes"],
      [`BLUEY_JOBS_MANAGED_CLOUD_${role}_RUNTIME_INSTANCE_ID`, "configuration", ["jobs_api"], "canonical_id"],
      [`BLUEY_JOBS_MANAGED_CLOUD_${role}_WORKER_ID`, "configuration", ["jobs_api"], "canonical_id"],
    ]),
  ].map(([name, classification, roles, valueType]) => ({
    classification,
    name,
    required: true,
    roles,
    valueType,
  })).sort((left, right) => left.name.localeCompare(right.name));
  const configContract = {
    audience: MANAGED_CLOUD_AUDIENCES.configContract,
    roleSources,
    schemaVersion: 1,
    variables,
  };
  const manifest = {
    configSchemaSha256: sha256(canonicalJsonBytes(configContract)),
    migrationSetSha256: sha256(canonicalJsonBytes(migrationContract)),
    postgresMigrationHead: migrationContract.postgres.head,
    protocolSetSha256: sha256(canonicalJsonBytes(protocolContract)),
    protocols: protocols.map(({ protocolId, protocolVersion, schemaSha256 }) => ({
      protocolId,
      protocolVersion,
      schemaSha256,
    })),
    sqliteMigrationHead: migrationContract.sqlite.head,
  };
  return {
    builderPolicy,
    configContract,
    manifest,
    migrationContract,
    protocolContract,
  };
}

function successorReleaseContractFixture() {
  const fixture = releaseContractFixture();
  const sources = sourceSet(SOURCE_VERIFICATION_PROTOCOL_PATHS, "d");
  fixture.protocolContract.schemaVersion = 2;
  fixture.protocolContract.protocols.splice(9, 0, {
    protocolId: "source_verification",
    protocolVersion: 1,
    schemaSha256: sources.setSha256,
    sourceFiles: sources.files,
  });
  fixture.migrationContract.sqlite = {
    files: [sourceFile(
      "infra/sqlite/server-runtime/057_jobs_original_source_verification_authority.sql",
      "e",
    )],
    head: "057_jobs_original_source_verification_authority.sql",
    name: "sqlite",
    setSha256: "",
  };
  fixture.migrationContract.postgres = {
    files: [sourceFile(
      "infra/postgres/server-runtime/035_jobs_original_source_verification_authority.sql",
      "f",
    )],
    head: "035_jobs_original_source_verification_authority.sql",
    name: "postgres",
    setSha256: "",
  };
  for (const dialect of ["postgres", "sqlite"]) {
    fixture.migrationContract[dialect].setSha256 = sha256(
      canonicalJsonBytes(fixture.migrationContract[dialect].files),
    );
  }
  fixture.migrationContract.paritySha256 = sha256(canonicalJsonBytes({
    postgres: [fixture.migrationContract.postgres.head],
    sqlite: [fixture.migrationContract.sqlite.head],
  }));
  fixture.manifest = {
    ...fixture.manifest,
    version: 2,
    migrationSetSha256: sha256(canonicalJsonBytes(fixture.migrationContract)),
    postgresMigrationHead: fixture.migrationContract.postgres.head,
    protocolSetSha256: sha256(canonicalJsonBytes(fixture.protocolContract)),
    protocols: fixture.protocolContract.protocols.map(
      ({ protocolId, protocolVersion, schemaSha256 }) => ({
        protocolId,
        protocolVersion,
        schemaSha256,
      }),
    ),
    sqliteMigrationHead: fixture.migrationContract.sqlite.head,
  };
  return fixture;
}

function rawPublicKey(pair) {
  return pair.publicKey.export({ format: "jwk" }).x;
}

function trustKey(keyId, role, pair, nowMs, state = "active") {
  return {
    keyId,
    maximumTrustGeneration: 10,
    minimumTrustGeneration: 1,
    publicKey: rawPublicKey(pair),
    role,
    state,
    validFromMs: nowMs - 60_000,
    validUntilMs: nowMs + 60_000,
  };
}

function signedSet({
  id,
  keys,
  role,
  signedAtMs,
  targetAudience,
  targetBytes,
  trustGeneration = 1,
}) {
  const unsigned = {
    audience: MANAGED_CLOUD_AUDIENCES.signatureSet,
    role,
    signatureSetId: id,
    signedAtMs,
    targetAudience,
    targetSha256: sha256(targetBytes),
    trustGeneration,
    version: 1,
  };
  const payload = canonicalJsonBytes(unsigned);
  return {
    ...unsigned,
    signatures: keys
      .map(({ keyId, pair }) => ({
        keyId,
        signature: signBytes(null, payload, pair.privateKey).toString("base64url"),
      }))
      .sort((left, right) => left.keyId.localeCompare(right.keyId)),
  };
}

function trustFixture() {
  const nowMs = Date.now();
  const pairs = Object.fromEntries(
    [
      "anchor-only",
      "general",
      "incident",
      "policy-only",
      "promotion",
      "release-active",
      "release-revoked",
      "root-shared",
    ].map((name) => [name, generateKeyPairSync("ed25519")]),
  );
  const policy = {
    audience: MANAGED_CLOUD_AUDIENCES.trustPolicy,
    expiresAtMs: nowMs + 30_000,
    issuedAtMs: nowMs - 1_000,
    keys: [
      trustKey("general-key", "general_promotion", pairs.general, nowMs),
      trustKey("incident-key", "incident", pairs.incident, nowMs),
      trustKey("policy-only", "root", pairs["policy-only"], nowMs),
      trustKey("promotion-key", "promotion", pairs.promotion, nowMs),
      trustKey("release-active", "release", pairs["release-active"], nowMs),
      trustKey(
        "release-revoked",
        "release",
        pairs["release-revoked"],
        nowMs,
        "revoked",
      ),
      trustKey("root-shared", "root", pairs["root-shared"], nowMs),
    ].sort((left, right) => left.keyId.localeCompare(right.keyId)),
    policyId: "policy-generation-001",
    predecessorPolicySha256: null,
    roles: [
      { role: "general_promotion", threshold: 1 },
      { role: "incident", threshold: 1 },
      { role: "promotion", threshold: 1 },
      { role: "release", threshold: 1 },
      { role: "root", threshold: 2 },
    ],
    trustGeneration: 1,
    validFromMs: nowMs - 10_000,
    version: 1,
  };
  const rootAnchor = {
    audience: MANAGED_CLOUD_AUDIENCES.rootAnchor,
    keys: [
      {
        keyId: "anchor-only",
        publicKey: rawPublicKey(pairs["anchor-only"]),
        validFromMs: nowMs - 60_000,
        validUntilMs: nowMs + 60_000,
      },
      {
        keyId: "root-shared",
        publicKey: rawPublicKey(pairs["root-shared"]),
        validFromMs: nowMs - 60_000,
        validUntilMs: nowMs + 60_000,
      },
    ],
    threshold: 2,
    version: 1,
  };
  const policyBytes = canonicalJsonBytes(policy);
  const authorization = signedSet({
    id: "policy-root-signatures-001",
    keys: [
      { keyId: "anchor-only", pair: pairs["anchor-only"] },
      { keyId: "policy-only", pair: pairs["policy-only"] },
      { keyId: "root-shared", pair: pairs["root-shared"] },
    ],
    role: "root",
    signedAtMs: policy.issuedAtMs,
    targetAudience: MANAGED_CLOUD_AUDIENCES.trustPolicy,
    targetBytes: policyBytes,
  });
  return {
    pairs,
    policy,
    rootAnchor,
    rootAnchorSha256: sha256(canonicalJsonBytes(rootAnchor)),
    trustBundle: {
      audience: MANAGED_CLOUD_AUDIENCES.trustBundle,
      policies: [{ authorization, policy }],
      rootAnchor,
      version: 1,
    },
  };
}

test("canonical JSON is recursively UTF-8-key-sorted and integer-only", () => {
  const value = {
    z: "é",
    a: { β: 2, a: -7 },
    n: 0,
    array: [{ b: true, a: null }, "雪"],
  };
  const expected =
    '{"a":{"a":-7,"β":2},"array":[{"a":null,"b":true},"雪"],"n":0,"z":"é"}\n';
  assert.equal(canonicalJsonBytes(value).toString("utf8"), expected);
  assert.equal(
    sha256(canonicalJsonBytes(value)),
    "e0e886e66d3fea4d9571caa85b779ff5a7a25f41138d6ce936244f148867d2ee",
  );
  assert.throws(() => canonicalJsonBytes({ value: -0 }), /safe .*integers/);
  assert.throws(() => canonicalJsonBytes({ value: 1.5 }), /safe .*integers/);
  assert.throws(
    () => canonicalJsonBytes({ value: Number.MAX_SAFE_INTEGER + 1 }),
    /safe .*integers/,
  );
});

test("release-v1 validates the exact artifacts, capabilities, migrations, and protocols", () => {
  const manifest = releaseFixture();
  assert.equal(validateReleaseManifest(manifest), manifest);
  assert.throws(
    () => validateReleaseManifest({
      ...manifest,
      capabilities: manifest.capabilities.map((item) =>
        item.capability === "portal_static"
          ? { ...item, capability: "jobs_portal" }
          : item,
      ),
    }),
    /capability mapping/,
  );
  assert.throws(
    () => validateReleaseManifest({
      ...manifest,
      protocols: manifest.protocols.map((item) =>
        item.protocolId === "runner_result"
          ? { ...item, protocolVersion: 1 }
          : item,
      ),
    }),
    /protocols/,
  );
  assert.throws(
    () => validateReleaseManifest({
      ...manifest,
      sqliteMigrationHead: "056_jobs_canonical_taxonomy_authority",
    }),
    /migration heads/,
  );
  assert.throws(
    () => validateReleaseManifest({
      ...manifest,
      manifestId: "m".repeat(128 * 1024),
    }),
    /128 KiB/,
  );
  for (const feature of ["directDiscovery", "globalDiscovery"]) {
    assert.throws(
      () => validateReleaseManifest({
        ...manifest,
        featureAuthority: {
          ...manifest.featureAuthority,
          [feature]: true,
        },
      }),
      /reserves conditional discovery/,
    );
  }
  const portal = manifest.components.find((item) => item.componentId === "jobs-portal");
  assert.throws(
    () => validateReleaseManifest({
      ...manifest,
      components: manifest.components.map((item) =>
        item.componentId === "jobs-portal"
          ? {
              ...item,
              artifactRef:
                "https://artifacts.example/releases/" + RELEASE_ID + "/portal.tar",
            }
          : item,
      ),
    }),
    /content-addressed/,
  );
  assert.match(portal.artifactRef, new RegExp(portal.artifactSha256));
});

test("release-v2 binds source verification capability, protocol, role, and entrypoint", () => {
  const manifest = successorReleaseFixture();
  assert.equal(validateReleaseManifest(manifest), manifest);
  assert.deepEqual(deriveRequiredRuntimeRoles(manifest.featureAuthority), [
    ...MANAGED_CLOUD_BASE_RUNTIME_ROLES,
    "original_source_verifier",
  ].sort());
  assert.deepEqual(
    deriveManagedCloudCanaryCheckIds(manifest.featureAuthority),
    [...CANARY_CHECK_IDS, "original-source-verifier-readiness"].sort(),
  );
  assert.deepEqual(
    deriveManagedCloudRequiredRuntimePaths(
      "jobs-workflows",
      manifest.capabilities,
    ).filter((path) => path.includes("original-source")),
    ["app/workflows/dist/original-source-verifier.js"],
  );
  assert.doesNotThrow(() => deriveManagedCloudRuntimeIdentitySha256(
    "a".repeat(64),
    "jobs-workflows",
    "original_source_verifier",
  ));

  const versionOneEnabled = releaseFixture();
  versionOneEnabled.featureAuthority.sourceVerification = true;
  const versionTwoDisabled = structuredClone(manifest);
  versionTwoDisabled.featureAuthority.sourceVerification = false;
  for (const invalid of [versionOneEnabled, versionTwoDisabled]) {
    invalid.featureAuthoritySha256 = sha256(
      canonicalJsonBytes(invalid.featureAuthority),
    );
    assert.throws(() => validateReleaseManifest(invalid), /v1=false or v2=true/);
  }

  const missingCapability = structuredClone(manifest);
  missingCapability.capabilities = missingCapability.capabilities.filter(
    (entry) => entry.capability !== "original_source_verifier",
  );
  assert.throws(
    () => validateReleaseManifest(missingCapability),
    /capabilit/,
  );

  const wrongProtocol = structuredClone(manifest);
  wrongProtocol.protocols.find(
    (entry) => entry.protocolId === "source_verification",
  ).protocolVersion = 2;
  assert.throws(
    () => validateReleaseManifest(wrongProtocol),
    /exact sorted versioned set/,
  );

  const contracts = successorReleaseContractFixture();
  assert.equal(validateManagedCloudReleaseContracts(contracts), true);
  contracts.protocolContract.protocols.find(
    (entry) => entry.protocolId === "source_verification",
  ).sourceFiles.pop();
  assert.throws(
    () => validateManagedCloudReleaseContracts(contracts),
    /required source paths/,
  );
});

test("release contracts prove full source sets without configured runtime identity", () => {
  const fixture = releaseContractFixture();
  assert.equal(validateManagedCloudReleaseContracts(fixture), true);

  const wrongProtocol = structuredClone(fixture);
  wrongProtocol.protocolContract.protocols[0].sourceFiles[0].sha256 =
    "f".repeat(64);
  assert.throws(
    () => validateManagedCloudReleaseContracts(wrongProtocol),
    /protocol source-set digest/,
  );

  const rebuildAuthorize = structuredClone(fixture);
  rebuildAuthorize.builderPolicy.noRebuildStages = [
    "promote",
    "rollback",
    "verify",
  ];
  assert.throws(
    () => validateManagedCloudReleaseContracts(rebuildAuthorize),
    /build-once contract/,
  );

  const optionalGrant = structuredClone(fixture);
  optionalGrant.configContract.variables.find((variable) =>
    variable.name.endsWith("JOBS_API_GRANT_TOKEN"),
  ).required = false;
  assert.throws(
    () => validateManagedCloudReleaseContracts(optionalGrant),
    /required\/type authority/,
  );

  const apiRuntimeDisabled = structuredClone(fixture);
  apiRuntimeDisabled.configContract.variables.find((variable) =>
    variable.name === "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED"
  ).roles = ["jobs_workflows", "managed_runner"];
  assert.throws(
    () => validateManagedCloudReleaseContracts(apiRuntimeDisabled),
    /missing an owning runtime role/,
  );

  const reconciliationOptional = structuredClone(fixture);
  reconciliationOptional.configContract.variables.find((variable) =>
    variable.name === "BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED"
  ).required = false;
  assert.throws(
    () => validateManagedCloudReleaseContracts(reconciliationOptional),
    /required\/type authority/,
  );

  const configuredIdentity = structuredClone(fixture);
  configuredIdentity.configContract.variables.push({
    classification: "configuration",
    name: "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_GATEWAY_RUNTIME_IDENTITY_SHA256",
    required: false,
    roles: ["jobs_workflows"],
    valueType: "string",
  });
  assert.throws(
    () => validateManagedCloudReleaseContracts(configuredIdentity),
    /measured locally/,
  );

  const suffixlessMigration = structuredClone(fixture);
  suffixlessMigration.migrationContract.sqlite.head =
    "056_jobs_canonical_taxonomy_authority";
  assert.throws(
    () => validateManagedCloudReleaseContracts(suffixlessMigration),
    /wrong identity or head/,
  );
});

test("runtime identity is role-separated and derived from measured artifact bytes", () => {
  const measurementSha256 = sha256(Buffer.from("measurement\n"));
  const gateway = deriveManagedCloudRuntimeIdentitySha256(
    measurementSha256,
    "jobs-workflows",
    "workflow_gateway",
  );
  const worker = deriveManagedCloudRuntimeIdentitySha256(
    measurementSha256,
    "jobs-workflows",
    "workflow_worker",
  );
  assert.notEqual(gateway, worker);
  assert.equal(
    gateway,
    sha256(Buffer.concat([
      Buffer.from("bluey-jobs-managed-cloud-runtime-identity-v1\0"),
      Buffer.from(measurementSha256),
      Buffer.from("\0jobs-workflows\0workflow_gateway"),
    ])),
  );
});

test("runtime measurement accepts 512 exact files and rejects 513", () => {
  const measuredFiles = Array.from({ length: 511 }, (_, index) => ({
    path: `app/workflows/dist/measured-${index.toString().padStart(3, "0")}.js`,
    sha256: index.toString(16).padStart(64, "0"),
  })).concat({
    path: "usr/local/bin/node",
    sha256: "f".repeat(64),
  });
  const measurement = {
    audience: MANAGED_CLOUD_AUDIENCES.runtimeMeasurement,
    buildId: "build-jobs-workflows",
    componentId: "jobs-workflows",
    configSchemaSha256: "c".repeat(64),
    measuredFiles,
    migrationSetSha256: "d".repeat(64),
    protocolSetSha256: "e".repeat(64),
    roles: ["workflow_gateway", "workflow_worker"],
    sourceCommit: SOURCE_COMMIT,
    version: 1,
  };
  const measurementSha256 = sha256(canonicalJsonBytes(measurement));
  const inventory = {
    entries: [
      {
        path: "app/.bluey/managed-cloud-runtime-measurement.json",
        sha256: measurementSha256,
        sizeBytes: canonicalJsonBytes(measurement).length,
        type: "file",
      },
      ...measuredFiles.map((file) => ({ ...file, sizeBytes: 1, type: "file" })),
    ].sort((left, right) => Buffer.compare(
      Buffer.from(left.path),
      Buffer.from(right.path),
    )),
  };
  const runtimeIdentities = measurement.roles.map((role) => ({
    role,
    runtimeIdentitySha256: deriveManagedCloudRuntimeIdentitySha256(
      measurementSha256,
      "jobs-workflows",
      role,
    ),
  }));
  const context = {
    buildId: measurement.buildId,
    componentId: measurement.componentId,
    configSchemaSha256: measurement.configSchemaSha256,
    inventory,
    migrationSetSha256: measurement.migrationSetSha256,
    protocolSetSha256: measurement.protocolSetSha256,
    roles: measurement.roles,
    sourceCommit: measurement.sourceCommit,
  };
  assert.equal(
    validateManagedCloudRuntimeMeasurement(
      measurement,
      measurementSha256,
      runtimeIdentities,
      context,
    ),
    measurement,
  );
  const overflow = {
    ...measurement,
    measuredFiles: [
      ...measuredFiles,
      { path: "app/workflows/dist/overflow.js", sha256: "f".repeat(64) },
    ],
  };
  assert.throws(
    () => validateManagedCloudRuntimeMeasurement(
      overflow,
      sha256(canonicalJsonBytes(overflow)),
      runtimeIdentities,
      {
        ...context,
        inventory: {
          entries: [
            ...inventory.entries,
            {
              path: "app/workflows/dist/overflow.js",
              sha256: "f".repeat(64),
              sizeBytes: 1,
              type: "file",
            },
          ],
        },
      },
    ),
    /file set is invalid/,
  );
});

test("filesystem runtime measurement rejects a symbolic-link root", async (t) => {
  const root = await temporaryDirectory(t, "runtime-measurement-symlink");
  await mkdir(join(root, "opt"), { recursive: true });
  await mkdir(join(root, "usr/local/bin"), { recursive: true });
  await writeFile(join(root, "opt/bluey-jobs-api"), "api-runtime\n");
  await symlink(
    join(root, "opt/bluey-jobs-api"),
    join(root, "usr/local/bin/bluey-jobs-api"),
  );
  await assert.rejects(
    () => createRuntimeMeasurementFromFilesystem({
      buildId: "managed-cloud-611-jobs-api",
      componentId: "jobs-api",
      configSchemaSha256: "c".repeat(64),
      migrationSetSha256: "d".repeat(64),
      protocolSetSha256: "e".repeat(64),
      roles: [
        "jobs_api",
        "workflow_cleanup_dispatcher",
        "workflow_command_dispatcher",
      ],
      root,
      sourceCommit: SOURCE_COMMIT,
    }),
    /runtime measurement roots may not contain symbolic links/,
  );
});

test("task queue and failure-converter bytes retain shared digest goldens", () => {
  assert.equal(
    deriveManagedCloudTaskQueueSha256(
      "bluey-prod",
      "bluey-jobs-applications",
    ),
    "1551b746f88eded4598f4e0817382253d97bd1d8aa8bcc74b4f0339bd118cfb0",
  );
  assert.equal(
    sha256(Buffer.from("converter\n")),
    "4b57c07fe3edb9cb6068615fb27d286931d8d6d547c0e4de3426fcc81ed17aa0",
  );
  assert.throws(
    () => deriveManagedCloudTaskQueueSha256(" bluey-prod", "queue"),
    /canonical string/,
  );
  assert.doesNotThrow(() =>
    deriveManagedCloudTaskQueueSha256("n".repeat(240), "queue"),
  );
  assert.throws(
    () => deriveManagedCloudTaskQueueSha256("n".repeat(241), "queue"),
    /identity bytes/,
  );
  assert.doesNotThrow(() =>
    deriveManagedCloudTaskQueueSha256("é".repeat(120), "queue"),
  );
  assert.throws(
    () => deriveManagedCloudTaskQueueSha256("é".repeat(121), "queue"),
    /identity bytes/,
  );
});

test("inventory import transport is exact, canonical, and capped at 12 MiB", () => {
  const attachments = Object.fromEntries(
    MANAGED_CLOUD_COMPONENTS.map((component) => {
      const runtime = MANAGED_CLOUD_RUNTIME_CONTRACTS[component.componentId];
      const requiredPaths = runtime
        ? [
          ...runtime.requiredPaths,
          ...(component.componentId === "jobs-runner"
            ? [
              "ms-playwright/chromium_headless_shell-1/" +
                "chrome-headless-shell-linux64/chrome-headless-shell",
            ]
            : []),
        ]
        : ["index.html"];
      const entries = requiredPaths.map((path) => ({
        mode:
          path === "usr/local/bin/node" ||
          path === "usr/local/bin/bluey-jobs-api" ||
          path.endsWith("/chrome-headless-shell")
            ? "0555"
            : "0444",
        path,
        sha256: "1".repeat(64),
        sizeBytes: 1,
        type: "file",
      })).sort((left, right) => Buffer.compare(
        Buffer.from(left.path),
        Buffer.from(right.path),
      ));
      const inventory = {
        artifactKind: component.artifactKind,
        artifactSha256: "1".repeat(64),
        audience: MANAGED_CLOUD_AUDIENCES.contentInventory,
        componentId: component.componentId,
        entries,
        runtime: runtime
          ? {
            cmd: [...runtime.cmd],
            environment: [...runtime.requiredEnvironment].sort(),
            entrypoint: [...runtime.entrypoint],
            exposedPorts: [...runtime.exposedPorts],
            user: runtime.runtimeUser,
            workingDirectory: runtime.workingDirectory,
          }
          : null,
        version: 1,
      };
      return [
        component.componentId,
        canonicalJsonBytes(inventory).toString("base64url"),
      ];
    }),
  );
  const validated = validateManagedCloudInventoryAttachments(attachments);
  assert.equal(validated.decoded.size, 4);
  assert.ok(validated.totalBytes > 0);
  assert.throws(
    () => validateManagedCloudInventoryAttachments({
      ...attachments,
      "jobs-api": attachments["jobs-api"] + "=",
    }),
    /base64url/,
  );
  assert.throws(
    () => validateManagedCloudInventoryAttachments({
      ...attachments,
      "jobs-api": attachments["jobs-runner"],
    }),
    /component identity/,
  );
  const oversized = canonicalJsonBytes({
    audience: MANAGED_CLOUD_AUDIENCES.contentInventory,
    componentId: "jobs-api",
    entries: [],
    padding: "x".repeat(12 * 1024 * 1024),
    version: 1,
  }).toString("base64url");
  assert.throws(
    () => validateManagedCloudInventoryAttachments({
      ...attachments,
      "jobs-api": oversized,
    }),
    /exceed 12 MiB/,
  );
});

test("union root authorization satisfies overlapping anchor and policy thresholds", () => {
  const fixture = trustFixture();
  const active = validateManagedCloudTrustBundle(
    fixture.trustBundle,
    fixture.rootAnchorSha256,
  );
  assert.equal(active.policyId, fixture.policy.policyId);
  const targetBytes = Buffer.from("release-target\n");
  const releaseSignatures = signedSet({
    id: "release-signatures-001",
    keys: [{ keyId: "release-active", pair: fixture.pairs["release-active"] }],
    role: "release",
    signedAtMs: fixture.policy.issuedAtMs + 1,
    targetAudience: MANAGED_CLOUD_AUDIENCES.manifest,
    targetBytes,
  });
  assert.deepEqual(
    verifyManagedCloudSignatureSet({
      expectedRole: "release",
      expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.manifest,
      expectedTargetBytes: targetBytes,
      policy: active,
      signatureSet: releaseSignatures,
    }),
    ["release-active"],
  );
  const revokedExtra = signedSet({
    id: "release-signatures-revoked-extra",
    keys: [
      { keyId: "release-active", pair: fixture.pairs["release-active"] },
      { keyId: "release-revoked", pair: fixture.pairs["release-revoked"] },
    ],
    role: "release",
    signedAtMs: fixture.policy.issuedAtMs + 1,
    targetAudience: MANAGED_CLOUD_AUDIENCES.manifest,
    targetBytes,
  });
  assert.throws(
    () => verifyManagedCloudSignatureSet({
      expectedRole: "release",
      expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.manifest,
      expectedTargetBytes: targetBytes,
      policy: active,
      signatureSet: revokedExtra,
    }),
    /role, state, or generation/,
  );

  const tooManySignatures = {
    ...releaseSignatures,
    signatures: Array.from({ length: 33 }, (_, index) => ({
      keyId: "release-overflow-" + index.toString().padStart(2, "0"),
      signature: releaseSignatures.signatures[0].signature,
    })),
  };
  assert.throws(
    () => verifyManagedCloudSignatureSet({
      expectedRole: "release",
      expectedTargetAudience: MANAGED_CLOUD_AUDIENCES.manifest,
      expectedTargetBytes: targetBytes,
      policy: active,
      signatureSet: tooManySignatures,
    }),
    /at most 32/,
  );

  const rootOverflow = structuredClone(fixture.trustBundle);
  rootOverflow.rootAnchor.keys = Array.from({ length: 33 }, (_, index) => ({
    ...fixture.rootAnchor.keys[0],
    keyId: "anchor-overflow-" + index.toString().padStart(2, "0"),
  }));
  assert.throws(
    () => validateManagedCloudTrustBundle(
      rootOverflow,
      sha256(canonicalJsonBytes(rootOverflow.rootAnchor)),
    ),
    /at most 32/,
  );

  const preIssued = structuredClone(fixture.trustBundle);
  preIssued.policies[0].authorization.signedAtMs =
    preIssued.policies[0].policy.issuedAtMs - 1;
  assert.throws(
    () => validateManagedCloudTrustBundle(
      preIssued,
      fixture.rootAnchorSha256,
    ),
    /inside every required policy window/,
  );
});

test("trust rotation rejects authorization after predecessor policy expiry", () => {
  const fixture = trustFixture();
  const nowMs = Date.now();
  const predecessor = structuredClone(fixture.policy);
  predecessor.issuedAtMs = nowMs - 4_000;
  predecessor.validFromMs = nowMs - 10_000;
  predecessor.expiresAtMs = nowMs - 2_000;
  const predecessorBytes = canonicalJsonBytes(predecessor);
  const predecessorAuthorization = signedSet({
    id: "predecessor-root-signatures",
    keys: [
      { keyId: "anchor-only", pair: fixture.pairs["anchor-only"] },
      { keyId: "policy-only", pair: fixture.pairs["policy-only"] },
      { keyId: "root-shared", pair: fixture.pairs["root-shared"] },
    ],
    role: "root",
    signedAtMs: predecessor.issuedAtMs,
    targetAudience: MANAGED_CLOUD_AUDIENCES.trustPolicy,
    targetBytes: predecessorBytes,
  });
  const successor = {
    ...structuredClone(predecessor),
    expiresAtMs: nowMs + 30_000,
    issuedAtMs: nowMs - 1_000,
    policyId: "policy-generation-002",
    predecessorPolicySha256: sha256(predecessorBytes),
    trustGeneration: 2,
    validFromMs: nowMs - 1_500,
  };
  const successorBytes = canonicalJsonBytes(successor);
  const successorAuthorization = signedSet({
    id: "successor-root-signatures",
    keys: [
      { keyId: "policy-only", pair: fixture.pairs["policy-only"] },
      { keyId: "root-shared", pair: fixture.pairs["root-shared"] },
    ],
    role: "root",
    signedAtMs: successor.issuedAtMs,
    targetAudience: MANAGED_CLOUD_AUDIENCES.trustPolicy,
    targetBytes: successorBytes,
    trustGeneration: 2,
  });
  assert.throws(
    () => validateManagedCloudTrustBundle(
      {
        ...fixture.trustBundle,
        policies: [
          { authorization: predecessorAuthorization, policy: predecessor },
          { authorization: successorAuthorization, policy: successor },
        ],
      },
      fixture.rootAnchorSha256,
    ),
    /inside every required policy window/,
  );
});

test("trust rotation evaluates predecessor keys at predecessor generation", () => {
  const fixture = trustFixture();
  const nowMs = Date.now();
  const predecessor = structuredClone(fixture.policy);
  predecessor.issuedAtMs = nowMs - 3_000;
  predecessor.validFromMs = nowMs - 10_000;
  predecessor.expiresAtMs = nowMs + 10_000;
  predecessor.keys = predecessor.keys.map((key) => ({
    ...key,
    maximumTrustGeneration: 1,
  }));
  const predecessorBytes = canonicalJsonBytes(predecessor);
  const predecessorAuthorization = signedSet({
    id: "predecessor-generation-one-signatures",
    keys: [
      { keyId: "anchor-only", pair: fixture.pairs["anchor-only"] },
      { keyId: "policy-only", pair: fixture.pairs["policy-only"] },
      { keyId: "root-shared", pair: fixture.pairs["root-shared"] },
    ],
    role: "root",
    signedAtMs: predecessor.issuedAtMs,
    targetAudience: MANAGED_CLOUD_AUDIENCES.trustPolicy,
    targetBytes: predecessorBytes,
  });
  const successor = {
    ...structuredClone(predecessor),
    expiresAtMs: nowMs + 30_000,
    issuedAtMs: nowMs - 1_000,
    keys: predecessor.keys.map((key) => ({
      ...key,
      maximumTrustGeneration: 2,
    })),
    policyId: "policy-generation-002",
    predecessorPolicySha256: sha256(predecessorBytes),
    trustGeneration: 2,
    validFromMs: nowMs - 2_000,
  };
  const successorBytes = canonicalJsonBytes(successor);
  const successorAuthorization = signedSet({
    id: "successor-generation-two-signatures",
    keys: [
      { keyId: "policy-only", pair: fixture.pairs["policy-only"] },
      { keyId: "root-shared", pair: fixture.pairs["root-shared"] },
    ],
    role: "root",
    signedAtMs: successor.issuedAtMs,
    targetAudience: MANAGED_CLOUD_AUDIENCES.trustPolicy,
    targetBytes: successorBytes,
    trustGeneration: 2,
  });
  const active = validateManagedCloudTrustBundle(
    {
      ...fixture.trustBundle,
      policies: [
        { authorization: predecessorAuthorization, policy: predecessor },
        { authorization: successorAuthorization, policy: successor },
      ],
    },
    fixture.rootAnchorSha256,
  );
  assert.equal(active.trustGeneration, 2);
});

test("static bundle inspection rejects ambiguous archives and non-file index", async (t) => {
  const root = await temporaryDirectory(t, "static-gate");
  const good = join(root, "portal.tar");
  await writeFile(good, tarArchive([{ body: "<main>Bluey</main>\n", name: "index.html" }]));
  const inspected = await inspectStaticBundleArchive(good);
  assert.equal(inspected.entries[0].path, "index.html");

  const trailing = join(root, "trailing.tar");
  const trailingBytes = Buffer.concat([await readFile(good), Buffer.alloc(512)]);
  trailingBytes[trailingBytes.length - 1] = 1;
  await writeFile(trailing, trailingBytes);
  await assert.rejects(() => inspectStaticBundleArchive(trailing), /nonzero bytes/);

  const badMagic = join(root, "bad-magic.tar");
  const badMagicBytes = Buffer.from(await readFile(good));
  badMagicBytes[257] = "x".charCodeAt(0);
  await writeFile(badMagic, badMagicBytes);
  await assert.rejects(() => inspectStaticBundleArchive(badMagic), /ustar/);

  const directoryIndex = join(root, "directory-index.tar");
  await writeFile(
    directoryIndex,
    tarArchive([{ name: "index.html/", type: "5" }]),
  );
  await assert.rejects(
    () => inspectStaticBundleArchive(directoryIndex),
    /nonempty regular index.html/,
  );
});

test("OCI inspection proves closed blobs, absolute runtime, and safe overlays", async (t) => {
  const root = await temporaryDirectory(t, "oci-gate");
  const good = join(root, "good.oci.tar");
  await writeOciFixture(good, {
    layers: [
      [
        { name: "app", type: "5" },
        { name: "app/cache", type: "5" },
        { body: "old", name: "app/cache/old" },
        {
          body: "api-binary\n",
          mode: 0o555,
          name: "usr/local/bin/bluey-jobs-api",
        },
      ],
      [
        { body: "new", name: "app/cache/new" },
        { name: "app/cache/.wh..wh..opq" },
      ],
    ],
  });
  const inspected = await inspectOciImageArchive(good, "jobs-api");
  assert.ok(inspected.entries.some((entry) => entry.path === "app/cache/new"));
  assert.ok(!inspected.entries.some((entry) => entry.path === "app/cache/old"));

  const hostilePath = join(root, "hostile-path.oci.tar");
  await writeOciFixture(hostilePath, {
    runtime: apiRuntime({
      Env: [
        "BLUEY_JOBS_API_HOST=0.0.0.0",
        "BLUEY_JOBS_API_PORT=8081",
        "PATH=/app/evil",
      ],
    }),
  });
  await assert.rejects(
    () => inspectOciImageArchive(hostilePath, "jobs-api"),
    /missing PATH=/,
  );

  const extraEnvironment = join(root, "extra-environment.oci.tar");
  await writeOciFixture(extraEnvironment, {
    runtime: apiRuntime({
      Env: [...apiRuntime().Env, "NODE_VERSION=24.0.0"],
    }),
  });
  await assert.rejects(
    () => inspectOciImageArchive(extraEnvironment, "jobs-api"),
    /exact closed set/,
  );

  const symlinkBinary = join(root, "symlink-binary.oci.tar");
  await writeOciFixture(symlinkBinary, {
    layers: [[
      { name: "app", type: "5" },
      { body: "api", mode: 0o555, name: "app/api" },
      {
        linkTarget: "/app/api",
        mode: 0o555,
        name: "usr/local/bin/bluey-jobs-api",
        type: "2",
      },
    ]],
  });
  await assert.rejects(
    () => inspectOciImageArchive(symlinkBinary, "jobs-api"),
    /runtime measurement roots may not contain symbolic links/,
  );

  const extraBlob = join(root, "extra-blob.oci.tar");
  await writeOciFixture(extraBlob, { extraBlob: "unreferenced secret\n" });
  await assert.rejects(
    () => inspectOciImageArchive(extraBlob, "jobs-api"),
    /unreferenced/,
  );

  const runnerPaths = [
    { body: "automation\n", name: "app/automation/dist/index.js" },
    { body: "runner\n", name: "app/runner/dist/server.js" },
    {
      body: "native-addon\n",
      mode: 0o555,
      name: "app/runner/dist/native/bluey_jobs_runner_native_storage.node",
    },
    { body: "node\n", mode: 0o555, name: "usr/local/bin/node" },
  ];
  const headlessExecutable =
    "ms-playwright/chromium_headless_shell-1/" +
    "chrome-headless-shell-linux64/chrome-headless-shell";
  const headlessRunner = join(root, "headless-runner.oci.tar");
  await writeOciFixture(headlessRunner, {
    layers: [[
      ...runnerPaths,
      { body: "headless\n", mode: 0o555, name: headlessExecutable },
      { body: "ffmpeg\n", mode: 0o555, name: "ms-playwright/ffmpeg-1/ffmpeg" },
    ]],
    runtime: runnerRuntime(),
  });

  const symlinkedMeasuredRunner = join(root, "symlinked-measured-runner.oci.tar");
  await writeOciFixture(symlinkedMeasuredRunner, {
    layers: [[
      ...runnerPaths,
      { body: "headless\n", mode: 0o555, name: headlessExecutable },
      { body: "shared-runtime\n", name: "opt/shared-runtime.js" },
      {
        linkTarget: "/opt/shared-runtime.js",
        name: "app/automation/dist/linked-runtime.js",
        type: "2",
      },
    ]],
    runtime: runnerRuntime(),
  });
  await assert.rejects(
    () => inspectOciImageArchive(symlinkedMeasuredRunner, "jobs-runner"),
    /runtime measurement roots may not contain symbolic links/,
  );
  const runnerInventory = await inspectOciImageArchive(
    headlessRunner,
    "jobs-runner",
  );
  assert.ok(runnerInventory.entries.some(
    (entry) => entry.path === headlessExecutable,
  ));

  const fullChromiumRunner = join(root, "full-chromium-runner.oci.tar");
  await writeOciFixture(fullChromiumRunner, {
    layers: [[
      ...runnerPaths,
      {
        body: "chrome\n",
        mode: 0o555,
        name: "ms-playwright/chromium-1/chrome-linux/chrome",
      },
      { body: "headless\n", mode: 0o555, name: headlessExecutable },
      { body: "ffmpeg\n", mode: 0o555, name: "ms-playwright/ffmpeg-1/ffmpeg" },
    ]],
    runtime: runnerRuntime(),
  });
  await assert.rejects(
    () => inspectOciImageArchive(fullChromiumRunner, "jobs-runner"),
    /only Chromium headless shell and ffmpeg/,
  );

  const nonExecutableRunner = join(root, "non-executable-runner.oci.tar");
  await writeOciFixture(nonExecutableRunner, {
    layers: [[
      ...runnerPaths,
      { body: "headless\n", mode: 0o444, name: headlessExecutable },
      { body: "ffmpeg\n", mode: 0o555, name: "ms-playwright/ffmpeg-1/ffmpeg" },
    ]],
    runtime: runnerRuntime(),
  });
  await assert.rejects(
    () => inspectOciImageArchive(nonExecutableRunner, "jobs-runner"),
    /executable Playwright Chromium headless shell/,
  );

  const unknownConfig = join(root, "unknown-config.oci.tar");
  await writeOciFixture(unknownConfig, {
    runtime: apiRuntime({ AttachStdin: true }),
  });
  await assert.rejects(
    () => inspectOciImageArchive(unknownConfig, "jobs-api"),
    /unknown key/,
  );
});

test("typed activation evidence cross-binds portal, converter, and Temporal queue", async (t) => {
  const root = await temporaryDirectory(t, "activation-evidence");
  const evidenceRoot = join(root, "evidence");
  await mkdir(evidenceRoot);
  const manifest = releaseFixture();
  const manifestSha256 = sha256(canonicalJsonBytes(manifest));
  const nowMs = Date.now();
  const scope = { channel: "shadow", environment: "staging", region: "us-east-1" };
  const activation = {
    expiresAtMs: nowMs + 30_000,
    featureAuthority: manifest.featureAuthority,
    issuedAtMs: nowMs - 1_000,
    portalReadbackAtMs: nowMs - 2_000,
    scope,
  };
  const common = {
    expiresAtMs: nowMs + 60_000,
    manifestSha256,
    observedAtMs: nowMs - 2_000,
    scope,
    status: "pass",
    version: 1,
  };
  const converterSha256 = sha256(Buffer.from("deployed converter\n"));
  const workflowsInventoryFile = join(root, "workflows-inventory.json");
  await writeCanonical(workflowsInventoryFile, {
    entries: [{
      path: "app/workflows/dist/failure-converter.js",
      sha256: converterSha256,
      type: "file",
    }],
  });
  const portal = manifest.components.find((item) => item.componentId === "jobs-portal");
  const taskQueueSha256 = deriveManagedCloudTaskQueueSha256(
    "bluey-prod",
    "bluey-jobs-applications",
  );
  const documents = {
    "canary.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.canaryEvidence,
      checks: CANARY_CHECK_IDS.map((checkId) => ({
        checkId,
        evidenceSha256: "1".repeat(64),
        status: "pass",
      })),
    },
    "cleanup-authority.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.cleanupEvidence,
      cleanupAuthoritySha256: "2".repeat(64),
      dispatcherReady: true,
    },
    "failure-converter.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.failureConverterEvidence,
      deployedFileSha256: converterSha256,
    },
    "portal-readback.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.portalReadbackEvidence,
      portalArtifactSha256: portal.artifactSha256,
      readbackSha256: portal.artifactSha256,
    },
    "runner-fleet.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.runnerFleetEvidence,
      fleetIdSha256: "3".repeat(64),
      readyInstances: 2,
      requiredInstances: 2,
    },
    "storage-config.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.storageEvidence,
      readProbeSha256: "4".repeat(64),
      storageConfigSha256: "5".repeat(64),
      writeProbeSha256: "6".repeat(64),
    },
    "task-queue.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.taskQueueEvidence,
      namespace: "bluey-prod",
      taskQueue: "bluey-jobs-applications",
      taskQueueSha256,
    },
    "temporal-namespace.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.temporalEvidence,
      gatewayReady: true,
      namespace: "bluey-prod",
      namespaceSha256: sha256(Buffer.from("bluey-prod")),
      workflowWorkerReady: true,
    },
  };
  for (const [name, value] of Object.entries(documents)) {
    await writeCanonical(join(evidenceRoot, name), value);
  }
  const args = {
    activation,
    evidenceDirectory: evidenceRoot,
    manifest,
    manifestSha256,
    workflowsInventoryFile,
  };
  const hashes = await validateManagedCloudActivationEvidence(args);
  assert.equal(hashes.failureConverterSha256, converterSha256);
  assert.equal(hashes.taskQueueSha256, taskQueueSha256);

  await writeCanonical(join(evidenceRoot, "canary.json"), {
    ...documents["canary.json"],
    checks: documents["canary.json"].checks.map((check) =>
      check.checkId === "read-only-rootfs-policy"
        ? { ...check, checkId: "runtime-rootfs-policy" }
        : check,
    ),
  });
  await assert.rejects(
    () => validateManagedCloudActivationEvidence(args),
    /exact derived check set/,
  );
  await writeCanonical(
    join(evidenceRoot, "canary.json"),
    documents["canary.json"],
  );

  await writeCanonical(join(evidenceRoot, "portal-readback.json"), {
    ...documents["portal-readback.json"],
    readbackSha256: "7".repeat(64),
  });
  await assert.rejects(
    () => validateManagedCloudActivationEvidence(args),
    /readback bytes differ/,
  );
  await writeCanonical(
    join(evidenceRoot, "portal-readback.json"),
    documents["portal-readback.json"],
  );
  await writeCanonical(join(evidenceRoot, "temporal-namespace.json"), {
    ...documents["temporal-namespace.json"],
    namespace: "other-namespace",
    namespaceSha256: sha256(Buffer.from("other-namespace")),
  });
  await assert.rejects(
    () => validateManagedCloudActivationEvidence(args),
    /Temporal evidence/,
  );
  await writeCanonical(join(evidenceRoot, "temporal-namespace.json"), {});
  await assert.rejects(
    () => validateManagedCloudActivationEvidence(args),
    /must contain exactly/,
  );
});

test("rollback uses a monotonic successor activation over older verified bytes", () => {
  const nowMs = Date.now();
  const scope = { channel: "canary", environment: "production", region: "us-east-1" };
  const from = {
    activation: { activationGeneration: 10, channelSequence: 20, scope },
    authorized: { manifest: { releaseSequence: 200 } },
    promotion: {
      activationSha256: "1".repeat(64),
      manifestSha256: "2".repeat(64),
    },
  };
  const to = {
    activation: {
      activationGeneration: 11,
      channelSequence: 21,
      expectedHeadRevision: 8,
      expectedTransitionSha256: "3".repeat(64),
      issuedAtMs: nowMs - 1_000,
      predecessorActivationSha256: from.promotion.activationSha256,
      scope,
    },
    authorized: {
      manifest: { releaseSequence: 100 },
      policy: { trustGeneration: 2 },
    },
    promotion: {
      activationSha256: "4".repeat(64),
      manifestSha256: "5".repeat(64),
    },
  };
  const rollback = {
    expectedHeadRevision: 8,
    expectedTransitionSha256: "3".repeat(64),
    issuedAtMs: nowMs - 500,
    scope,
    trustGeneration: 2,
  };
  assert.deepEqual(
    validateManagedCloudRollbackSuccessor(rollback, from, to),
    scope,
  );
  assert.throws(
    () => validateManagedCloudRollbackSuccessor(
      rollback,
      from,
      {
        ...to,
        activation: {
          ...to.activation,
          predecessorActivationSha256: "6".repeat(64),
        },
      },
    ),
    /successor activation over older release bytes/,
  );
  assert.throws(
    () => validateManagedCloudRollbackSuccessor(
      rollback,
      from,
      {
        ...to,
        authorized: {
          ...to.authorized,
          manifest: { releaseSequence: 201 },
        },
      },
    ),
    /successor activation over older release bytes/,
  );
});

test("activation window is contained by cohort, trust, and portal readback", () => {
  const nowMs = Date.now();
  const activation = {
    expiresAtMs: nowMs + 30_000,
    issuedAtMs: nowMs - 2_000,
    notBeforeMs: nowMs - 1_000,
    portalReadbackAtMs: nowMs - 3_000,
    portalReadbackTtlMs: 33_000,
  };
  const cohort = {
    expiresAtMs: activation.expiresAtMs,
    issuedAtMs: nowMs - 4_000,
    notBeforeMs: activation.notBeforeMs,
  };
  const policy = {
    expiresAtMs: activation.expiresAtMs,
    validFromMs: activation.issuedAtMs,
  };
  assert.equal(
    validateManagedCloudActivationWindow(activation, cohort, policy),
    true,
  );
  assert.throws(
    () => validateManagedCloudActivationWindow(
      activation,
      { ...cohort, expiresAtMs: activation.expiresAtMs - 1 },
      policy,
    ),
    /validity window/,
  );
  assert.throws(
    () => validateManagedCloudActivationWindow(
      activation,
      cohort,
      { ...policy, expiresAtMs: activation.expiresAtMs - 1 },
    ),
    /validity window/,
  );
  assert.throws(
    () => validateManagedCloudActivationWindow(
      { ...activation, portalReadbackTtlMs: 32_999 },
      cohort,
      policy,
    ),
    /validity window/,
  );
});

test("release workflow statically proves build-once and protected authorization", async () => {
  const workflow = new URL(
    "../../.github/workflows/jobs-managed-cloud-release.yml",
    import.meta.url,
  );
  assert.equal(await validateManagedCloudWorkflowContract(workflow), true);
  const text = await readFile(workflow, "utf8");
  assert.match(text, /directDiscovery: false/);
  assert.match(text, /globalDiscovery: false/);
  assert.match(text, /sourceVerification: false/);
  assert.doesNotMatch(text, /original.source.verifier/i);
  for (const contract of Object.values(MANAGED_CLOUD_RUNTIME_CONTRACTS)) {
    assert.ok(contract.requiredEnvironment.some((item) => item === "PATH=" + SAFE_PATH));
  }
});
