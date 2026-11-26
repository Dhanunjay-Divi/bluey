import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { generateKeyPairSync, sign as signBytes } from "node:crypto";
import {
  access,
  copyFile,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, posix } from "node:path";
import { Readable } from "node:stream";
import test from "node:test";
import { promisify } from "node:util";
import {
  assembleManagedCloudCandidate,
  attestManagedCloudArtifactPublication,
  authorizeManagedCloudCandidate,
  canonicalJsonBytes,
  captureManagedCloudPortalManifestStream,
  captureManagedCloudPublicationApiStream,
  createArtifactProvenance,
  createFileSbom,
  createManagedCloudCiEvidence,
  createManagedCloudArtifactPublicationPlan,
  createManagedCloudRollback,
  createManagedCloudContracts,
  createManagedCloudPublicationWorkflowEvidence,
  selectManagedCloudPublicationSource,
  createManagedCloudPublicationPreflightDocument,
  createManagedCloudPublicationSetDocument,
  promoteManagedCloudCandidate,
  createRuntimeMeasurementFromFilesystem,
  deriveManagedCloudCanaryCheckIds,
  deriveManagedCloudPortalManifestCreatedAt,
  deriveManagedCloudRequiredRuntimePaths,
  deriveRequiredRuntimeRoles,
  deriveManagedCloudRuntimeIdentitySha256,
  deriveManagedRunnerAutomationBundleSha256,
  deriveManagedCloudTaskQueueSha256,
  inspectOciImageArtifact,
  inspectOciImageArchive,
  inspectManagedCloudArtifactPublicationReadback,
  inspectStaticBundleArchive,
  validateTarRejectionDiagnostic,
  MANAGED_CLOUD_AUDIENCES,
  MANAGED_CLOUD_BASE_CAPABILITIES,
  MANAGED_CLOUD_BASE_RUNTIME_ROLES,
  MANAGED_CLOUD_COMPONENTS,
  MANAGED_CLOUD_PROTOCOL_IDS,
  MANAGED_CLOUD_RELEASE_V3_PROTOCOL_IDS,
  MANAGED_CLOUD_SUCCESSOR_PROTOCOL_IDS,
  MANAGED_CLOUD_RUNTIME_CONTRACTS,
  sha256,
  stageManagedCloudPortalArtifactReadback,
  validateManagedCloudActivationEvidence,
  validateManagedCloudActivationWindow,
  validateAuthorizedManagedCloudCandidate,
  validateManagedCloudInventoryAttachments,
  validateManagedCloudPublicationAdapter,
  validateManagedCloudPublicationBuilderPolicy,
  validateManagedCloudPublicationPlanShape,
  validateManagedCloudPublicationPreflightDocument,
  validateManagedCloudPublicationRuntimeIdentity,
  validateManagedCloudPublicationSetShape,
  validateManagedCloudPromotion,
  validateManagedCloudReleaseContracts,
  validateManagedCloudRollbackSuccessor,
  validateManagedCloudRollback,
  validateManagedCloudRuntimeMeasurement,
  validateManagedCloudTestEvidence,
  validateManagedCloudTrustBundle,
  validateManagedCloudWorkflowContract,
  validateReleaseManifest,
  verifyManagedCloudSignatureSet,
} from "./managed-cloud-release-gate.mjs";

const execFileAsync = promisify(execFile);

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
  ["source_verifier_startup", 1],
  ["source_verifier_boot", 1],
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
const SOURCE_VERIFIER_STARTUP_PROTOCOL_PATHS = [
  "infra/postgres/server-runtime/040_jobs_original_source_verifier_startup_authority.sql",
  "infra/sqlite/server-runtime/062_jobs_original_source_verifier_startup_authority.sql",
  "jobs/automation/src/managed-cloud-runtime-client.ts",
  "jobs/automation/src/worker-auth.ts",
  "jobs/workflows/src/original-source-verification-runtime.ts",
  "jobs/workflows/src/original-source-verifier.ts",
  "server/src/api/jobs_managed_cloud_releases.rs",
  "server/src/api/jobs_worker_auth.rs",
  "server/src/db/jobs/managed_cloud_release_authority.rs",
  "server/src/db/jobs/original_source_verification.rs",
  "server/src/db/mod.rs",
];
const SOURCE_VERIFIER_BOOT_PROTOCOL_PATHS = [
  "infra/postgres/server-runtime/041_jobs_source_verifier_boot_generation_authority.sql",
  "infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql",
  "infra/sqlite/server-runtime/063_jobs_source_verifier_boot_generation_authority.sql",
  "infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql",
  "jobs/automation/src/managed-cloud-runtime-client.ts",
  "jobs/automation/src/worker-auth.ts",
  "jobs/workflows/src/original-source-verifier.ts",
  "server/src/api/jobs.rs",
  "server/src/api/jobs_worker_auth.rs",
  "server/src/db/jobs/managed_cloud_release_authority.rs",
  "server/src/db/jobs/source_verifier_boot.rs",
  "server/src/db/mod.rs",
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
    header.write(octal(["0", "x", "g"].includes(type) ? body.length : 0, 12), 124);
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
    if (["0", "x", "g"].includes(type)) {
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

function portalArtifactManifest(portalBytes, overrides = {}) {
  const manifest = {
    annotations: {
      "org.opencontainers.image.created": "2026-08-31T00:00:00Z",
    },
    artifactType: "application/vnd.bluey.jobs.portal.v1",
    config: {
      data: "e30=",
      digest:
        "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
      mediaType: "application/vnd.oci.empty.v1+json",
      size: 2,
    },
    layers: [
      {
        annotations: { "org.opencontainers.image.title": "jobs-portal.tar" },
        digest: "sha256:" + sha256(portalBytes),
        mediaType: "application/vnd.bluey.jobs.portal.tar",
        size: portalBytes.length,
      },
    ],
    mediaType: "application/vnd.oci.image.manifest.v1+json",
    schemaVersion: 2,
  };
  return { ...manifest, ...overrides };
}

function publicationWorkflowArtifacts({
  attempt = 1,
  branch = "main",
  repositoryId = 611,
  runId = 2001,
} = {}) {
  return {
    artifacts: [
      {
        digest: "sha256:" + "b".repeat(64),
        expired: false,
        id: 4001,
        name: "jobs-managed-cloud-publication-" + runId + "-" + attempt,
        size_in_bytes: 4096,
        workflow_run: {
          head_branch: branch,
          head_repository_id: repositoryId,
          head_sha: SOURCE_COMMIT,
          id: runId,
          repository_id: repositoryId,
        },
      },
    ],
    total_count: 1,
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

function workflowsRuntime(overrides = {}) {
  return {
    Cmd: ["/usr/local/bin/node", "workflows/dist/worker.js"],
    Entrypoint: [],
    Env: ["NODE_ENV=production", "PATH=" + SAFE_PATH],
    ExposedPorts: {},
    Labels: {},
    User: "node",
    WorkingDir: "/app",
    ...overrides,
  };
}

async function writeOciFixture(
  path,
  {
    extraBlob,
    rawLayers,
    indexReferenceName,
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
  } = {},
) {
  const exactLayers = layers.map((entries) => [...entries]);
  if (
    !exactLayers
      .flat()
      .some(
        (entry) =>
          entry.name === "app/.bluey/managed-cloud-runtime-measurement.json",
      )
  ) {
    exactLayers[0].push({
      body: "{}\n",
      name: "app/.bluey/managed-cloud-runtime-measurement.json",
    });
  }
  const layerBytes = rawLayers ?? exactLayers.map((entries) => tarArchive(entries));
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
  const indexDescriptor = indexReferenceName
    ? {
        ...manifestDescriptor,
        annotations: {
          "org.opencontainers.image.ref.name": indexReferenceName,
        },
      }
    : manifestDescriptor;
  const indexBytes = Buffer.from(
    JSON.stringify({ manifests: [indexDescriptor], schemaVersion: 2 }),
  );
  const outerEntries = [
    {
      body: JSON.stringify({ imageLayoutVersion: "1.0.0" }),
      name: "oci-layout",
    },
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
  return {
    imageConfigSha256: configDescriptor.digest.slice(7),
    imageManifestSha256: manifestDescriptor.digest.slice(7),
  };
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
          ? "registry.example/bluey/" +
            expected.componentId +
            "@sha256:" +
            artifactSha256
          : "https://artifacts.example/releases/" +
            RELEASE_ID +
            "/" +
            artifactSha256 +
            ".tar",
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
    componentSetSha256: sha256(
      canonicalJsonBytes({
        audience: MANAGED_CLOUD_AUDIENCES.componentInventory,
        capabilities,
        components,
        version: 1,
      }),
    ),
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
  ].sort(
    (left, right) =>
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
  manifest.componentSetSha256 = sha256(
    canonicalJsonBytes({
      audience: MANAGED_CLOUD_AUDIENCES.componentInventory,
      capabilities: manifest.capabilities,
      components: manifest.components,
      version: 1,
    }),
  );
  return manifest;
}

function releaseV3Fixture() {
  const manifest = successorReleaseFixture();
  manifest.version = 3;
  manifest.audience = MANAGED_CLOUD_AUDIENCES.manifestV3;
  manifest.sqliteMigrationHead =
    "065_jobs_source_verifier_heartbeat_boot_bindings.sql";
  manifest.postgresMigrationHead =
    "043_jobs_source_verifier_heartbeat_boot_bindings.sql";
  manifest.featureAuthority = {
    ...manifest.featureAuthority,
    directDiscovery: true,
    globalDiscovery: true,
  };
  manifest.featureAuthoritySha256 = sha256(
    canonicalJsonBytes(manifest.featureAuthority),
  );
  manifest.capabilities = [
    ...manifest.capabilities,
    {
      capability: "discovery_worker",
      componentId: "jobs-workflows",
    },
    {
      capability: "global_discovery_worker",
      componentId: "jobs-workflows",
    },
  ].sort(
    (left, right) =>
      left.componentId.localeCompare(right.componentId, "en") ||
      left.capability.localeCompare(right.capability, "en"),
  );
  manifest.protocols = MANAGED_CLOUD_RELEASE_V3_PROTOCOL_IDS.map(
    (protocolId, index) => ({
      protocolId,
      protocolVersion: PROTOCOL_VERSIONS.get(protocolId),
      schemaSha256: ((index + 2) % 10).toString(16).repeat(64),
    }),
  );
  manifest.componentSetSha256 = sha256(
    canonicalJsonBytes({
      audience: MANAGED_CLOUD_AUDIENCES.componentInventory,
      capabilities: manifest.capabilities,
      components: manifest.components,
      version: 1,
    }),
  );
  return manifest;
}

function releaseV4Fixture() {
  const manifest = releaseV3Fixture();
  manifest.version = 4;
  manifest.audience = MANAGED_CLOUD_AUDIENCES.manifestV4;
  manifest.sqliteMigrationHead =
    "073_jobs_managed_cloud_release_v4_audiences.sql";
  manifest.postgresMigrationHead =
    "052_jobs_managed_cloud_release_v4_audiences.sql";
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
    [
      "ats_certification",
      [
        "jobs/automation/src/certified-submit-form.ts",
        "server/src/db/jobs/ats_certification_authority.rs",
      ],
    ],
    [
      "execution_lease",
      [
        "jobs/runner/src/execution-lease.ts",
        "server/src/db/jobs/execution_leases.rs",
      ],
    ],
    [
      "gateway_command",
      [
        "jobs/workflows/src/contracts.ts",
        "jobs/workflows/src/gateway-service.ts",
        "jobs/workflows/src/gateway.ts",
        "server/src/jobs_workflow_dispatch.rs",
      ],
    ],
    [
      "managed_cloud_release",
      [
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
      ],
    ],
    [
      "object_evidence",
      ["server/src/db/object_uploads.rs", "server/src/db/jobs/evidence.rs"],
    ],
    [
      "runner_checkpoint",
      [
        "jobs/runner/src/run-checkpoint-store.ts",
        "server/src/db/jobs/execution_leases.rs",
      ],
    ],
    [
      "runner_profile_snapshot",
      [
        "jobs/runner/src/profile-snapshot-client.ts",
        "server/src/db/jobs/browser_profile_snapshots.rs",
      ],
    ],
    [
      "runner_result",
      [
        "jobs/runner/src/result-store.ts",
        "jobs/runner/src/submitted-result-recovery.ts",
        "server/src/db/jobs/local_runner.rs",
      ],
    ],
    [
      "runtime_heartbeat",
      [
        "jobs/automation/src/managed-cloud-runtime-client.ts",
        "jobs/automation/src/managed-cloud-runtime.ts",
        "server/src/api/jobs_worker_auth.rs",
        "server/src/jobs_managed_cloud_runtime.rs",
      ],
    ],
    [
      "workflow_cleanup",
      [
        "jobs/workflows/src/gateway-cleanup-service.ts",
        "jobs/workflows/src/gateway.ts",
        "server/src/jobs_workflow_cleanup.rs",
      ],
    ],
    [
      "workflow_command",
      [
        "jobs/workflows/src/contracts.ts",
        "jobs/workflows/src/gateway-service.ts",
        "jobs/workflows/src/gateway.ts",
        "jobs/workflows/src/workflows.ts",
        "server/src/db/jobs/workflow_commands.rs",
        "server/src/jobs_workflow_dispatch.rs",
      ],
    ],
  ]);
  const protocols = MANAGED_CLOUD_PROTOCOL_IDS.map((protocolId, index) => {
    const sources = sourceSet(
      protocolSources.get(protocolId),
      ((index + 1) % 10).toString(16),
    );
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
    postgres: [
      sourceFile(
        "infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql",
        "b",
      ),
    ],
    sqlite: [
      sourceFile(
        "infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql",
        "c",
      ),
    ],
  };
  const migrationContract = {
    audience: MANAGED_CLOUD_AUDIENCES.migrationContract,
    paritySha256: sha256(
      canonicalJsonBytes({
        postgres: ["034_jobs_canonical_taxonomy_authority.sql"],
        sqlite: ["056_jobs_canonical_taxonomy_authority.sql"],
      }),
    ),
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
    return {
      role,
      sourceFiles: sources.files,
      sourceSetSha256: sources.setSha256,
    };
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
    [
      "BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN",
      "configuration",
      sharedRoles,
      "https_url",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_CHANNEL",
      "configuration",
      sharedRoles,
      "canonical_id",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT",
      "configuration",
      sharedRoles,
      "canonical_id",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_HEARTBEAT_SECONDS",
      "configuration",
      sharedRoles,
      "positive_integer",
    ],
    [
      "BLUEY_JOBS_MANAGED_CLOUD_REGION",
      "configuration",
      sharedRoles,
      "canonical_id",
    ],
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
    ["BLUEY_JOBS_WORKFLOW_ORIGIN", "configuration", jobsApiRoles, "https_url"],
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
      [
        `BLUEY_JOBS_MANAGED_CLOUD_${role}_GRANT_ID`,
        "configuration",
        ["jobs_api"],
        "canonical_id",
      ],
      [
        `BLUEY_JOBS_MANAGED_CLOUD_${role}_GRANT_TOKEN`,
        "secret",
        ["jobs_api"],
        "secret_bytes",
      ],
      [
        `BLUEY_JOBS_MANAGED_CLOUD_${role}_RUNTIME_INSTANCE_ID`,
        "configuration",
        ["jobs_api"],
        "canonical_id",
      ],
      [
        `BLUEY_JOBS_MANAGED_CLOUD_${role}_WORKER_ID`,
        "configuration",
        ["jobs_api"],
        "canonical_id",
      ],
    ]),
  ]
    .map(([name, classification, roles, valueType]) => ({
      classification,
      name,
      required: true,
      roles,
      valueType,
    }))
    .sort((left, right) => left.name.localeCompare(right.name));
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
    protocols: protocols.map(
      ({ protocolId, protocolVersion, schemaSha256 }) => ({
        protocolId,
        protocolVersion,
        schemaSha256,
      }),
    ),
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
  fixture.builderPolicy = {
    ...fixture.builderPolicy,
    audience: MANAGED_CLOUD_AUDIENCES.builderPolicyV2,
    noRebuildStages: ["authorize", "promote", "publish", "rollback", "verify"],
    schemaVersion: 2,
  };
  const sources = sourceSet(SOURCE_VERIFICATION_PROTOCOL_PATHS, "d");
  fixture.protocolContract.schemaVersion = 2;
  fixture.protocolContract.protocols.splice(9, 0, {
    protocolId: "source_verification",
    protocolVersion: 1,
    schemaSha256: sources.setSha256,
    sourceFiles: sources.files,
  });
  fixture.migrationContract.sqlite = {
    files: [
      sourceFile(
        "infra/sqlite/server-runtime/057_jobs_original_source_verification_authority.sql",
        "e",
      ),
    ],
    head: "057_jobs_original_source_verification_authority.sql",
    name: "sqlite",
    setSha256: "",
  };
  fixture.migrationContract.postgres = {
    files: [
      sourceFile(
        "infra/postgres/server-runtime/035_jobs_original_source_verification_authority.sql",
        "f",
      ),
    ],
    head: "035_jobs_original_source_verification_authority.sql",
    name: "postgres",
    setSha256: "",
  };
  for (const dialect of ["postgres", "sqlite"]) {
    fixture.migrationContract[dialect].setSha256 = sha256(
      canonicalJsonBytes(fixture.migrationContract[dialect].files),
    );
  }
  fixture.migrationContract.paritySha256 = sha256(
    canonicalJsonBytes({
      postgres: [fixture.migrationContract.postgres.head],
      sqlite: [fixture.migrationContract.sqlite.head],
    }),
  );
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

function releaseV3ContractFixture() {
  const fixture = successorReleaseContractFixture();
  const startupSources = sourceSet(SOURCE_VERIFIER_STARTUP_PROTOCOL_PATHS, "1");
  const bootSources = sourceSet(SOURCE_VERIFIER_BOOT_PROTOCOL_PATHS, "2");
  fixture.protocolContract.schemaVersion = 3;
  fixture.protocolContract.protocols.splice(10, 0, {
    protocolId: "source_verifier_startup",
    protocolVersion: 1,
    schemaSha256: startupSources.setSha256,
    sourceFiles: startupSources.files,
  });
  fixture.protocolContract.protocols.splice(11, 0, {
    protocolId: "source_verifier_boot",
    protocolVersion: 1,
    schemaSha256: bootSources.setSha256,
    sourceFiles: bootSources.files,
  });
  fixture.migrationContract.sqlite = {
    files: [
      sourceFile(
        "infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql",
        "3",
      ),
    ],
    head: "065_jobs_source_verifier_heartbeat_boot_bindings.sql",
    name: "sqlite",
    setSha256: "",
  };
  fixture.migrationContract.postgres = {
    files: [
      sourceFile(
        "infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql",
        "4",
      ),
    ],
    head: "043_jobs_source_verifier_heartbeat_boot_bindings.sql",
    name: "postgres",
    setSha256: "",
  };
  for (const dialect of ["postgres", "sqlite"]) {
    fixture.migrationContract[dialect].setSha256 = sha256(
      canonicalJsonBytes(fixture.migrationContract[dialect].files),
    );
  }
  fixture.migrationContract.paritySha256 = sha256(
    canonicalJsonBytes({
      postgres: [fixture.migrationContract.postgres.head],
      sqlite: [fixture.migrationContract.sqlite.head],
    }),
  );
  fixture.manifest = {
    ...fixture.manifest,
    version: 3,
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

const V4_MEASUREMENT_HELPERS = [
  "jobs/scripts/check-managed-cloud-authorization-origin.mjs",
  "jobs/scripts/check-managed-cloud-observability-policy.mjs",
];

function releaseV4ContractFixture() {
  const fixture = releaseV3ContractFixture();
  const files = [
    ...fixture.builderPolicy.files,
    ...V4_MEASUREMENT_HELPERS.map((path) => sourceFile(path, "b")),
  ].sort((left, right) => left.path < right.path ? -1 : Number(left.path > right.path));
  fixture.builderPolicy = {
    ...fixture.builderPolicy,
    audience: MANAGED_CLOUD_AUDIENCES.builderPolicyV3,
    files,
    schemaVersion: 3,
    setSha256: sha256(canonicalJsonBytes(files)),
  };
  fixture.protocolContract.schemaVersion = 4;
  const migrationPaths = {
    sqlite: [
      "066_jobs_attempt_employer_identity.sql",
      "067_jobs_provisioned_runtime_startup.sql",
      "068_jobs_provisioned_runtime_observations.sql",
      "069_jobs_ats_storage_fences.sql",
      "070_jobs_application_answer_revisions.sql",
      "071_jobs_workday_reconciliation_holds.sql",
      "072_jobs_mcp_read_grants.sql",
      "073_jobs_managed_cloud_release_v4_audiences.sql",
    ],
    postgres: [
      "044_jobs_attempt_employer_identity.sql",
      "045_jobs_provisioned_runtime_startup.sql",
      "046_jobs_provisioned_runtime_observations.sql",
      "047_jobs_ats_storage_fences.sql",
      "048_jobs_application_answer_revisions.sql",
      "049_jobs_process_attachment_signature_audience.sql",
      "050_jobs_workday_reconciliation_holds.sql",
      "051_jobs_mcp_read_grants.sql",
      "052_jobs_managed_cloud_release_v4_audiences.sql",
    ],
  };
  for (const [dialect, paths] of Object.entries(migrationPaths)) {
    const files = paths.map((path, index) =>
      sourceFile(
        `infra/${dialect}/server-runtime/${path}`,
        ((index + (dialect === "sqlite" ? 1 : 2)) % 10).toString(16),
      ),
    );
    fixture.migrationContract[dialect] = {
      files,
      head: paths.at(-1),
      name: dialect,
      setSha256: sha256(canonicalJsonBytes(files)),
    };
  }
  fixture.migrationContract.paritySha256 = sha256(
    canonicalJsonBytes({
      postgres: migrationPaths.postgres,
      sqlite: migrationPaths.sqlite,
    }),
  );
  fixture.manifest = {
    ...fixture.manifest,
    version: 4,
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
        signature: signBytes(null, payload, pair.privateKey).toString(
          "base64url",
        ),
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

async function fileSha256(path) {
  return sha256(await readFile(path));
}

function runtimeMeasurementFixture({
  buildId,
  componentId,
  contractHashes,
  entries,
  roles,
}) {
  const roots =
    componentId === "jobs-api"
      ? ["usr/local/bin/bluey-jobs-api"]
      : componentId === "jobs-workflows"
        ? ["app/automation", "app/workflows", "usr/local/bin/node"]
        : [
            "app/automation",
            "app/runner",
            "ms-playwright",
            "usr/local/bin/node",
          ];
  const measuredFiles = entries
    .filter(
      (entry) =>
        (entry.type ?? "0") === "0" &&
        Buffer.byteLength(entry.body ?? "") > 0 &&
        roots.some(
          (root) => entry.name === root || entry.name.startsWith(root + "/"),
        ),
    )
    .map((entry) => ({
      path: entry.name,
      sha256: sha256(Buffer.from(entry.body ?? "")),
    }))
    .sort((left, right) => left.path.localeCompare(right.path));
  return {
    audience: MANAGED_CLOUD_AUDIENCES.runtimeMeasurement,
    buildId,
    componentId,
    configSchemaSha256: contractHashes.config,
    measuredFiles,
    migrationSetSha256: contractHashes.migration,
    protocolSetSha256: contractHashes.protocol,
    roles: [...roles].sort(),
    sourceCommit: SOURCE_COMMIT,
    version: 1,
  };
}

async function createAuthorizedCandidateFixture({
  authorizationId,
  checkPolicyRefusals = false,
  contractVersion = 2,
  label,
  releaseSequence,
  root,
  trust,
}) {
  const repoRoot = join(import.meta.dirname, "../..");
  const fixtureRoot = join(root, label);
  const contracts = join(fixtureRoot, "contracts");
  await createManagedCloudContracts(repoRoot, contracts, contractVersion);
  const contractHashes = {
    builder: await fileSha256(join(contracts, "builder-policy.json")),
    config: await fileSha256(join(contracts, "config-contract.json")),
    migration: await fileSha256(join(contracts, "migration-contract.json")),
    protocol: await fileSha256(join(contracts, "protocol-contract.json")),
  };
  const sourceDateEpoch = Math.floor((Date.now() - 2_000) / 1_000);
  const artifactRoot = join(fixtureRoot, "artifacts");
  await mkdir(artifactRoot, { recursive: true });
  const specs = [
    {
      componentId: "jobs-api",
      entries: [
        {
          body: "api-binary\n",
          mode: 0o555,
          name: "usr/local/bin/bluey-jobs-api",
        },
      ],
      roles: [
        "jobs_api",
        "workflow_cleanup_dispatcher",
        "workflow_command_dispatcher",
      ],
      runtime: apiRuntime(),
    },
    {
      componentId: "jobs-runner",
      entries: [
        { body: "automation\n", name: "app/automation/dist/index.js" },
        { body: "runner\n", name: "app/runner/dist/server.js" },
        {
          body: "native-addon\n",
          mode: 0o555,
          name: "app/runner/dist/native/bluey_jobs_runner_native_storage.node",
        },
        { body: "node\n", mode: 0o555, name: "usr/local/bin/node" },
        {
          body: "headless\n",
          mode: 0o555,
          name:
            "ms-playwright/chromium_headless_shell-1/" +
            "chrome-headless-shell-linux64/chrome-headless-shell",
        },
        {
          body: "ffmpeg\n",
          mode: 0o555,
          name: "ms-playwright/ffmpeg-1/ffmpeg",
        },
      ],
      roles: ["managed_runner"],
      runtime: runnerRuntime(),
    },
    {
      componentId: "jobs-workflows",
      entries: [
        { body: "automation\n", name: "app/automation/dist/index.js" },
        { body: "discovery\n", name: "app/workflows/dist/discovery-worker.js" },
        { body: "failure\n", name: "app/workflows/dist/failure-converter.js" },
        { body: "gateway\n", name: "app/workflows/dist/gateway.js" },
        {
          body: "global-discovery\n",
          name: "app/workflows/dist/global-discovery-worker.js",
        },
        {
          body: "verifier\n",
          name: "app/workflows/dist/original-source-verifier.js",
        },
        { body: "worker\n", name: "app/workflows/dist/worker.js" },
        { body: "node\n", mode: 0o555, name: "usr/local/bin/node" },
      ],
      roles: [
        ...(contractVersion >= 3
          ? ["discovery_worker", "global_discovery_worker"]
          : []),
        "original_source_verifier",
        "workflow_gateway",
        "workflow_worker",
      ],
      runtime: workflowsRuntime(),
    },
  ];
  const builds = [];
  const releaseId = `managed-cloud-${label}`;
  for (const spec of specs) {
    const buildId = `${releaseId}-${spec.componentId}`;
    const measurement = runtimeMeasurementFixture({
      buildId,
      componentId: spec.componentId,
      contractHashes,
      entries: spec.entries,
      roles: spec.roles,
    });
    const artifact = join(artifactRoot, spec.componentId + ".oci.tar");
    await writeOciFixture(artifact, {
      layers: [
        [
          ...spec.entries,
          {
            body: canonicalJsonBytes(measurement),
            name: "app/.bluey/managed-cloud-runtime-measurement.json",
          },
        ],
      ],
      runtime: spec.runtime,
    });
    const inventory = await inspectOciImageArchive(artifact, spec.componentId);
    const inventoryFile = join(
      artifactRoot,
      spec.componentId + ".inventory.json",
    );
    const sbomFile = join(artifactRoot, spec.componentId + ".sbom.json");
    const provenanceFile = join(
      artifactRoot,
      spec.componentId + ".provenance.json",
    );
    await writeCanonical(inventoryFile, inventory);
    await writeCanonical(sbomFile, createFileSbom(inventory));
    await writeCanonical(
      provenanceFile,
      createArtifactProvenance({
        artifactSha256: inventory.artifactSha256,
        buildId,
        builderPolicySha256: contractHashes.builder,
        componentId: spec.componentId,
        sourceCommit: SOURCE_COMMIT,
        sourceDateEpoch,
      }),
    );
    builds.push({
      architecture: "x86_64",
      artifactKind: "oci_image",
      artifactRef:
        `ghcr.io/dhanunjay-divi/bluey-jobs-${spec.componentId.slice(5)}` +
        `@sha256:${inventory.artifactSha256}`,
      buildId,
      candidateFile: artifact,
      componentId: spec.componentId,
      contentInventoryFile: inventoryFile,
      platform: "linux",
      provenanceFile,
      sbomFile,
    });
  }
  const portalArtifact = join(artifactRoot, "jobs-portal.tar");
  await writeFile(
    portalArtifact,
    tarArchive([{ body: "<main>Bluey Jobs</main>\n", name: "index.html" }]),
  );
  const portalInventory = await inspectStaticBundleArchive(
    portalArtifact,
    "jobs-portal",
  );
  const portalInventoryFile = join(artifactRoot, "jobs-portal.inventory.json");
  const portalSbomFile = join(artifactRoot, "jobs-portal.sbom.json");
  const portalProvenanceFile = join(
    artifactRoot,
    "jobs-portal.provenance.json",
  );
  const portalBuildId = `${releaseId}-jobs-portal`;
  await writeCanonical(portalInventoryFile, portalInventory);
  await writeCanonical(portalSbomFile, createFileSbom(portalInventory));
  await writeCanonical(
    portalProvenanceFile,
    createArtifactProvenance({
      artifactSha256: portalInventory.artifactSha256,
      buildId: portalBuildId,
      builderPolicySha256: contractHashes.builder,
      componentId: "jobs-portal",
      sourceCommit: SOURCE_COMMIT,
      sourceDateEpoch,
    }),
  );
  builds.push({
    architecture: "wasm",
    artifactKind: "static_bundle",
    artifactRef:
      `https://artifacts.bluey.sh/jobs/releases/${releaseId}/` +
      `${portalInventory.artifactSha256}.tar`,
    buildId: portalBuildId,
    candidateFile: portalArtifact,
    componentId: "jobs-portal",
    contentInventoryFile: portalInventoryFile,
    platform: "web",
    provenanceFile: portalProvenanceFile,
    sbomFile: portalSbomFile,
  });
  const repository = "Dhanunjay-Divi/bluey";
  const repositoryRecord = { full_name: repository, id: 611 };
  const run = (id, path) => ({
    conclusion: "success",
    event: "push",
    head_branch: "main",
    head_sha: SOURCE_COMMIT,
    id,
    path,
    repository: repositoryRecord,
    run_attempt: 1,
    status: "completed",
  });
  const testEvidenceFile = join(fixtureRoot, "test-evidence.json");
  await writeCanonical(
    testEvidenceFile,
    createManagedCloudCiEvidence({
      defaultBranch: "main",
      jobsCiRun: run(1001, ".github/workflows/jobs-ci.yml@main"),
      observabilityRun: run(1002, ".github/workflows/observability-policy.yml"),
      releaseContractSelfTestSha256: "b".repeat(64),
      repository,
      sourceCommit: SOURCE_COMMIT,
    }),
  );
  const descriptorFile = join(fixtureRoot, "release-descriptor.json");
  const publishedAtMs = Date.now() - 1_000;
  await writeCanonical(descriptorFile, {
    audience: `bluey-jobs-managed-cloud-release-descriptor-v${contractVersion}`,
    builds,
    featureAuthority: {
      cloudDistribution: true,
      directDiscovery: contractVersion >= 3,
      globalDiscovery: contractVersion >= 3,
      sourceVerification: true,
      workflowCleanup: true,
      workflowCommandDispatch: true,
    },
    manifestGeneration: releaseSequence,
    manifestId: `manifest-${label}`,
    publishedAtMs,
    releaseId,
    releaseSequence,
    sourceCommit: SOURCE_COMMIT,
    sourceDateEpoch,
    testEvidenceFile,
    version: contractVersion,
  });
  const candidate = join(fixtureRoot, "candidate");
  await assembleManagedCloudCandidate({
    contractsDirectory: contracts,
    descriptorFile,
    outputDirectory: candidate,
    repoRoot,
  });
  const manifestFile = join(candidate, "release-manifest.json");
  const releaseSignaturesFile = join(fixtureRoot, "release-signatures.json");
  await writeCanonical(
    releaseSignaturesFile,
    signedSet({
      id: `release-signatures-${label}`,
      keys: [{ keyId: "release-active", pair: trust.pairs["release-active"] }],
      role: "release",
      signedAtMs: Date.now(),
      targetAudience:
        contractVersion === 4
          ? MANAGED_CLOUD_AUDIENCES.manifestV4
          : contractVersion === 3
            ? MANAGED_CLOUD_AUDIENCES.manifestV3
            : MANAGED_CLOUD_AUDIENCES.manifestV2,
      targetBytes: await readFile(manifestFile),
    }),
  );
  const trustBundleFile = join(fixtureRoot, "trust-bundle.json");
  await writeCanonical(trustBundleFile, trust.trustBundle);
  const authorized = join(fixtureRoot, "authorized");
  const policyBytes = [Buffer.from("fixture workflow\n"), Buffer.from("fixture checker\n")];
  const policyPaths = [".github/workflows/observability-policy.yml", "scripts/analyze-tracing-calls.py"];
  const policyModes = ["100644", "100755"];
  const policyObjects = ["1".repeat(40), "2".repeat(40)];
  const policyFiles = policyPaths.map((path, index) => ({
    path, mode: policyModes[index], sha256: sha256(policyBytes[index]),
  }));
  const observabilityPolicyRepo = join(fixtureRoot, "fixture-policy-source");
  const readFixturePolicyGit = (repo, args) => {
    assert.equal(repo, observabilityPolicyRepo);
    if (args[0] === "ls-tree") {
      assert.deepEqual(args, ["ls-tree", "-z", SOURCE_COMMIT, "--", ...policyPaths]);
      return Buffer.from(policyPaths.map((path, index) =>
        policyModes[index] + " blob " + policyObjects[index] + "\t" + path + "\0").join(""));
    }
    if (args[1] === "-t") {
      assert.deepEqual(args, ["cat-file", "-t", SOURCE_COMMIT]);
      return Buffer.from("commit\n");
    }
    assert.deepEqual(args.slice(0, 2), ["cat-file", "blob"]);
    const index = policyObjects.indexOf(args[2]);
    assert(index >= 0);
    return policyBytes[index];
  };
  const authorizationOptions = {
    approvedObservabilityPolicySha256: sha256(JSON.stringify({
      domain: "bluey-jobs-managed-cloud-observability-policy-v1", files: policyFiles,
    })),
    authorizationId: authorizationId ?? `authorization-${label}`,
    candidateDirectory: candidate,
    expectedCandidateSetSha256: sha256(await readFile(join(candidate, "candidate-set.json"))),
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    observabilityPolicyRepo,
    outputDirectory: authorized,
    signatureSetFile: releaseSignaturesFile,
    trustBundleFile,
  };
  if (checkPolicyRefusals) {
    await assert.rejects(() => authorizeManagedCloudCandidate({
      ...authorizationOptions,
      expectedCandidateSetSha256: "0".repeat(64),
      signatureSetFile: join(fixtureRoot, "must-not-read-drifted-signatures"),
    }, readFixturePolicyGit), /candidate differs from the verified intake/);
    await assert.rejects(() => access(authorized), { code: "ENOENT" });
    for (const [approvedObservabilityPolicySha256, reason] of [
      [null, /independently approved Observability Policy SHA-256 is required/],
      ["0".repeat(64), /does not match independently approved bytes/],
    ]) {
      await assert.rejects(() => authorizeManagedCloudCandidate({
        ...authorizationOptions,
        approvedObservabilityPolicySha256,
        signatureSetFile: join(fixtureRoot, "must-not-read-signatures"),
        trustBundleFile: join(fixtureRoot, "must-not-read-trust"),
      }, readFixturePolicyGit), reason);
      await assert.rejects(() => access(authorized), { code: "ENOENT" });
    }
  }
  await authorizeManagedCloudCandidate(authorizationOptions, readFixturePolicyGit);
  return { authorized, fixtureRoot, releaseId };
}

const taxonomyPath = "jobs/taxonomy/canonical-v1.json";

test("Jobs API Docker taxonomy closure includes the Rust registry before Cargo", async () => {
  const paths = [
    "server/Dockerfile.jobs", "server/Dockerfile.jobs.dockerignore",
    "server/src/jobs_taxonomy.rs", taxonomyPath,
  ];
  const [dockerfile, dockerignore, rust, taxonomy] = await Promise.all(paths.map(
    (path) => readFile(new URL(`../../${path}`, import.meta.url), "utf8"),
  ));
  assert.ok(taxonomy.length > 0);
  const copy = `COPY ${taxonomyPath} ./jobs/taxonomy/`;
  const check = (source, ignore, include) => {
    const registry = /const REGISTRY_BYTES:\s*&\[u8\]\s*=\s*include_bytes!\("([^"]+)"\);/
      .exec(include)?.[1];
    assert.ok(registry);
    assert.equal(posix.join("server/src", registry), taxonomyPath);
    const rules = ignore.split(/\r?\n/).map((line) => line.trim())
      .filter((line) => line && !line.startsWith("#"));
    assert.deepEqual(rules, [
      "**", "!Cargo.toml", "!Cargo.lock", "!crates/", "!crates/**", "!infra/", "!infra/**",
      "!jobs/", "jobs/**", "!jobs/taxonomy/", "jobs/taxonomy/**", `!${taxonomyPath}`,
      "!server/", "!server/Cargo.toml", "!server/Cargo.lock", "!server/Dockerfile.jobs",
      "!server/Dockerfile.jobs.dockerignore", "!server/src/", "!server/src/**",
    ]);
    const lines = source.split(/\r?\n/).map((line) => line.trim());
    const root = lines.indexOf("WORKDIR /build");
    const placement = lines.indexOf(copy);
    const build = lines.findIndex((line) =>
      line.startsWith("RUN cargo build --locked --release --manifest-path server/Cargo.toml "));
    assert.deepEqual(lines.filter((line) => line.startsWith("COPY jobs/")), [copy]);
    assert.ok(root >= 0 && root < placement && placement < build);
  };
  check(dockerfile, dockerignore, rust);
  for (const ignore of [
    dockerignore.replace("!jobs/taxonomy/\n", ""),
    dockerignore.replace(`!${taxonomyPath}`, "!jobs/taxonomy/**"),
    dockerignore + "!**/package.json\n",
  ]) {
    assert.throws(() => check(dockerfile, ignore, rust), { name: "AssertionError" });
  }
  for (const source of [
    dockerfile.replace(copy, `${copy}wrong`),
    dockerfile.replace(`${copy}\n`, "") + `\n${copy}\n`,
  ]) {
    assert.throws(() => check(source, dockerignore, rust), { name: "AssertionError" });
  }
  assert.throws(() => check(dockerfile, dockerignore, rust.replace(taxonomyPath, "jobs/package.json")),
    { name: "AssertionError" });
});

test("current release contracts refuse a historical migration cutoff", async (t) => {
  const root = await temporaryDirectory(t, "current-migration-contracts");
  const repo = join(import.meta.dirname, "../..");
  const gate = join(import.meta.dirname, "managed-cloud-release-gate.mjs");
  const legacy = await createManagedCloudContracts(repo, join(root, "legacy"), 3);
  const sources = [
    ...legacy.builderPolicy.files,
    ...legacy.configContract.roleSources.flatMap((role) => role.sourceFiles),
    ...legacy.protocolContract.protocols.flatMap((protocol) => protocol.sourceFiles),
    ...legacy.migrationContract.sqlite.files,
    ...legacy.migrationContract.postgres.files,
  ];
  const fixture = join(root, "source-at-v3-heads");
  for (const path of new Set(sources.map((source) => source.path))) {
    const dest = join(fixture, path);
    await mkdir(dirname(dest), { recursive: true });
    await copyFile(join(repo, path), dest);
  }
  const current = (out) => execFileAsync(process.execPath, [
    gate, "contracts-current", "--repo", fixture, "--out", out, "--version", "3",
  ]);
  const accepted = join(root, "accepted");
  await current(accepted);
  for (const name of await readdir(accepted)) {
    assert.deepEqual(await readFile(join(accepted, name)), await readFile(join(root, "legacy", name)));
  }

  for (const [dialect, successor] of [
    ["sqlite", "066_jobs_attempt_employer_identity.sql"],
    ["postgres", "044_jobs_attempt_employer_identity.sql"],
  ]) {
    const path = join(fixture, "infra", dialect, "server-runtime", successor);
    const refused = join(root, `refused-${dialect}`);
    await writeFile(path, "-- synthetic future migration\n", { flag: "wx" });
    await assert.rejects(current(refused),
      new RegExp(`${dialect} has runtime migrations after release-v3 head`));
    assert.deepEqual(await readdir(refused), []);
    await rm(path);
  }
});

test("managed release workflow requires the current migration contract generator", async (t) => {
  const workflow = join(import.meta.dirname, "../../.github/workflows/jobs-managed-cloud-release.yml");
  assert.equal(await validateManagedCloudWorkflowContract(workflow), true);
  const root = await temporaryDirectory(t, "current-migration-workflow");
  const downgraded = join(root, "jobs-managed-cloud-release.yml");
  const text = await readFile(workflow, "utf8");
  await writeFile(downgraded, text.replace("managed-cloud-release-gate.mjs contracts-current",
    "managed-cloud-release-gate.mjs contracts"));
  await assert.rejects(() => validateManagedCloudWorkflowContract(downgraded),
    /Workflow must build the exact release-v4/);
});

test("protected authorization requires the approved source policy before signatures or output", async (t) => {
  const root = await temporaryDirectory(t, "managed-policy-authorize");
  const trust = trustFixture();
  const { authorized, fixtureRoot } = await createAuthorizedCandidateFixture({
    root, label: "approved-policy", releaseSequence: 103, contractVersion: 3,
    authorizationId: "authorization-901-2", trust, checkPolicyRefusals: true,
  });
  const receiptPath = join(authorized, "authorization-set.json");
  const { authorization: receipt } = await validateAuthorizedManagedCloudCandidate(authorized, trust.rootAnchorSha256);
  assert.equal(receipt.version, 2);
  assert.equal(receipt.sourceCommit, SOURCE_COMMIT);
  assert.match(receipt.observabilityPolicySha256, /^[0-9a-f]{64}$/);
  const python = process.env.BLUEY_JOBS_ARCHIVE_TEST_PYTHON || "python3";
  const archive = join(root, "produced-authorization.zip");
  const destination = join(root, "authorized-intake");
  await mkdir(destination, { mode: 0o700 });
  const pack = (source, output) => execFileAsync(python, ["-I", "-S", "-B", "-c", [
    "from pathlib import Path",
    "import sys, zipfile",
    "root, output = map(Path, sys.argv[1:])",
    "with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED) as bundle:",
    "    for item in sorted(root.rglob('*')):",
    "        if item.is_file(): bundle.write(item, item.relative_to(root).as_posix())",
  ].join("\n"), source, output]);
  await pack(authorized, archive);
  const archiveBytes = await readFile(archive);
  const workflowCommit = "e".repeat(40);
  const runPath = join(root, "authorization-run.json");
  const jobsPath = join(root, "authorization-jobs.json");
  const artifactsPath = join(root, "authorization-artifacts.json");
  const complete = { status: "completed", conclusion: "success" };
  const repository = "Dhanunjay-Divi/bluey";
  await writeCanonical(runPath, {
    ...complete, id: 901, run_attempt: 2, event: "workflow_dispatch",
    head_sha: workflowCommit, head_branch: "main",
    path: ".github/workflows/jobs-managed-cloud-release.yml@main",
    repository: { id: 611, full_name: repository },
  });
  await writeCanonical(jobsPath, {
    total_count: 1,
    jobs: [{
      ...complete, id: 902, run_id: 901, head_sha: workflowCommit, name: "authorize",
      steps: [
        "Check out only the verifier",
        "Read the verified candidate source commit",
        "Check out candidate policy files as data only",
        "Verify the protected Observability policy before handling release signatures",
        "Reverify and authorize with protected trust material",
        "Upload authorized bytes",
      ].map((name, number) => ({ ...complete, number: number + 2, name })),
    }],
  });
  await writeCanonical(artifactsPath, {
    total_count: 1,
    artifacts: [{
      id: 903, name: "jobs-managed-cloud-authorized-901-2",
      digest: "sha256:" + sha256(archiveBytes), size_in_bytes: archiveBytes.byteLength, expired: false,
      workflow_run: { id: 901, repository_id: 611, head_repository_id: 611,
        head_sha: workflowCommit, head_branch: "main" },
    }],
  });
  const gate = join(import.meta.dirname, "managed-cloud-release-gate.mjs");
  const metadata = [
    "--artifact-max-bytes", String(8 * 1024 * 1024), "--run", runPath, "--jobs", jobsPath,
    "--artifacts", artifactsPath, "--default-branch", "main", "--repository", repository,
    "--repository-id", "611", "--run-id", "901", "--run-attempt", "2",
    "--authorization-workflow-commit", workflowCommit, "--candidate-source-commit", SOURCE_COMMIT,
  ];
  const stageOptions = (archivePath, pin, artifactId = "903") => [
    "--archive", archivePath, "--parent", destination, "--max-unpacked-bytes", String(8 * 1024 * 1024),
    "--downloaded-artifact-id", artifactId,
    "--reserve-bytes", "0", "--root-anchor-sha256", trust.rootAnchorSha256,
    "--approved-observability-policy-sha256", pin,
  ];
  const selected = await execFileAsync(process.execPath, [gate, "authorization-artifact-select", ...metadata]);
  assert.equal(selected.stderr, "");
  assert.equal(JSON.parse(selected.stdout).artifactId, 903);
  const intake = await execFileAsync(process.execPath, [
    gate, "authorization-intake", ...metadata, ...stageOptions(archive, receipt.observabilityPolicySha256),
  ]);
  const staged = JSON.parse(intake.stdout);
  assert.equal(intake.stderr, "");
  assert.equal(staged.artifactId, 903);
  assert.equal(staged.authorizationWorkflowCommit, workflowCommit);
  assert.equal(staged.candidateSourceCommit, SOURCE_COMMIT);
  assert.equal(staged.artifactDigest, "sha256:" + sha256(archiveBytes));
  assert.equal(staged.authorizationSetSha256, sha256(await readFile(receiptPath)));
  assert.equal(receipt.authorizationId, "authorization-901-2");
  assert.equal(staged.metadataSha256.jobs, sha256(await readFile(jobsPath)));
  assert.deepEqual(
    (await validateAuthorizedManagedCloudCandidate(staged.root, trust.rootAnchorSha256)).authorization,
    receipt,
  );
  const expectCliRefusal = async (args, reason) => {
    await assert.rejects(() => execFileAsync(process.execPath, [gate, ...args]), (error) => {
      assert.equal(error.code, 1);
      assert.equal(error.stdout, "");
      assert.match(error.stderr, reason);
      return true;
    });
  };
  const wrongAttempt = [...metadata];
  wrongAttempt[wrongAttempt.indexOf("--run-attempt") + 1] = "1";
  await expectCliRefusal(["authorization-intake", ...wrongAttempt, ...stageOptions(archive, receipt.observabilityPolicySha256)],
    /successful exact-source workflow attempt/);
  await expectCliRefusal(["authorization-intake", ...metadata, ...stageOptions(archive, receipt.observabilityPolicySha256, "904")],
    /artifact ID no longer matches/);
  const changedZip = join(root, "different-archive.zip");
  const changedBytes = Buffer.from(archiveBytes);
  changedBytes[0] ^= 1;
  await writeFile(changedZip, changedBytes);
  await expectCliRefusal(["authorization-intake", ...metadata, ...stageOptions(changedZip, receipt.observabilityPolicySha256)],
    /closed stager/);
  await expectCliRefusal(["authorization-intake", ...metadata, ...stageOptions(archive, "0".repeat(64))],
    /does not bind the protected approved policy and source/);
  const otherRun = join(root, "other-authorization-run.json");
  const runEvidence = JSON.parse(await readFile(runPath, "utf8"));
  await writeCanonical(otherRun, { ...runEvidence, repository: { id: 611, full_name: "fixture/other" } });
  const wrongRepository = [...metadata];
  wrongRepository[wrongRepository.indexOf("--run") + 1] = otherRun;
  wrongRepository[wrongRepository.indexOf("--repository") + 1] = "fixture/other";
  await expectCliRefusal([
    "authorization-intake", ...wrongRepository, ...stageOptions(archive, receipt.observabilityPolicySha256),
  ], /Authorized candidate CI evidence does not bind the selected repository/);
  await expectCliRefusal(["authorization-artifact-select", ...metadata, "--run-id", "901"],
    /Duplicate authorization intake option/);
  assert.deepEqual(await readdir(destination), [basename(dirname(staged.root))],
    "only the accepted intake may remain; wrong-policy extraction must be removed");
  const unsignedArchive = join(root, "candidate-producer.zip");
  const candidateDestination = join(root, "candidate-intake");
  const candidateRoot = join(fixtureRoot, "candidate");
  await mkdir(candidateDestination, { mode: 0o700 });
  await pack(candidateRoot, unsignedArchive);
  const candidateBytes = await readFile(unsignedArchive);
  const candidateRunPath = join(root, "candidate-run.json");
  const candidateJobsPath = join(root, "candidate-jobs.json");
  const candidateArtifactsPath = join(root, "candidate-artifacts.json");
  await writeCanonical(candidateRunPath, { ...runEvidence, id: 811, run_attempt: 3, head_sha: SOURCE_COMMIT });
  const candidateJob = (name, id, labels) => ({
    ...complete, name, id, run_id: 811, head_sha: SOURCE_COMMIT,
    steps: labels.map((label, index) => ({ ...complete, name: label, number: index + 2 })),
  });
  const producerJobs = [
    candidateJob("candidate", 812, [
      "Bind successful exact-tip CI instead of synthetic candidate evidence",
      "Build each component exactly once", "Assemble the deterministic release candidate",
      "Upload the only candidate bytes",
    ]),
    candidateJob("verify", 813, ["Fetch candidate bytes from the build job", "Independently inspect the downloaded bytes"]),
  ];
  await writeCanonical(candidateJobsPath, { total_count: producerJobs.length, jobs: producerJobs });
  await writeCanonical(candidateArtifactsPath, {
    total_count: 1, artifacts: [{
      id: 814, name: "jobs-managed-cloud-candidate-811-3",
      size_in_bytes: candidateBytes.length, digest: "sha256:" + sha256(candidateBytes), expired: false,
      workflow_run: { id: 811, repository_id: 611, head_repository_id: 611,
        head_sha: SOURCE_COMMIT, head_branch: "main" },
    }],
  });
  const candidateMetadata = [
    "--artifact-max-bytes", String(8 * 1024 * 1024),
    "--run", candidateRunPath, "--jobs", candidateJobsPath, "--artifacts", candidateArtifactsPath,
    "--default-branch", "main", "--repository", repository, "--repository-id", "611",
    "--run-id", "811", "--run-attempt", "3",
  ];
  const verifiedCandidate = await execFileAsync(process.execPath, [
    gate, "candidate-artifact-select", ...candidateMetadata,
  ]);
  assert.equal(JSON.parse(verifiedCandidate.stdout).artifactId, 814);
  assert.equal(verifiedCandidate.stderr, "");
  const candidateStage = [
    "--archive", unsignedArchive, "--downloaded-artifact-id", "814", "--parent", candidateDestination,
    "--max-unpacked-bytes", String(8 * 1024 * 1024), "--reserve-bytes", "0",
  ];
  const candidateResult = await execFileAsync(process.execPath, [
    gate, "candidate-intake", ...candidateMetadata, ...candidateStage,
  ]);
  const candidateProof = JSON.parse(candidateResult.stdout);
  assert.equal(candidateResult.stderr, "");
  assert.equal(candidateProof.candidateSourceCommit, SOURCE_COMMIT);
  assert.equal(candidateProof.candidateSetSha256, sha256(await readFile(join(candidateRoot, "candidate-set.json"))));
  assert.equal(candidateProof.candidateVerifyJobId, 813);
  assert.equal(sha256(await readFile(join(candidateProof.root, "candidate-set.json"))), candidateProof.candidateSetSha256);
  const skippedVerifier = join(root, "candidate-skipped-jobs.json");
  await writeCanonical(skippedVerifier, {
    total_count: 2, jobs: [producerJobs[0], { ...producerJobs[1], conclusion: "skipped" }],
  });
  const skipped = [...candidateMetadata];
  skipped[skipped.indexOf("--jobs") + 1] = skippedVerifier;
  await expectCliRefusal(["candidate-intake", ...skipped, ...candidateStage], /candidate verify job did not succeed/);
  assert.deepEqual(await readdir(candidateDestination), [basename(dirname(candidateProof.root))]);
  const { stdout, stderr } = await execFileAsync(process.execPath, [
    join(import.meta.dirname, "managed-cloud-release-gate.mjs"),
    "verify", "--candidate", join(fixtureRoot, "candidate"), "--print-source-commit", "true",
  ]);
  assert.equal(stdout, SOURCE_COMMIT + "\n");
  assert.equal(stderr, "");
  const replaceFixtureReceipt = async (value) => {
    const replacement = join(authorized, "authorization-set.test-replacement.json");
    await writeCanonical(replacement, value);
    await rename(replacement, receiptPath);
  };
  await replaceFixtureReceipt({ ...receipt, sourceCommit: "3".repeat(40) });
  await assert.rejects(() => validateAuthorizedManagedCloudCandidate(authorized, trust.rootAnchorSha256),
    /Observability policy does not bind the candidate source/);
  const { observabilityPolicySha256, sourceCommit, ...legacy } = receipt;
  assert(observabilityPolicySha256 && sourceCommit);
  await replaceFixtureReceipt({
    ...legacy, audience: MANAGED_CLOUD_AUDIENCES.authorization, version: 1,
  });
  assert.equal((await validateAuthorizedManagedCloudCandidate(authorized, trust.rootAnchorSha256)).authorization.version, 1);
  const planPath = join(root, "unqualified-publication-plan.json");
  await assert.rejects(() => createManagedCloudArtifactPublicationPlan({
    authorizedDirectory: authorized, expectedRootAnchorSha256: trust.rootAnchorSha256, outputFile: planPath,
    registryNamespace: "ghcr.io/dhanunjay-divi/bluey-jobs", portalArtifactOrigin: "https://artifacts.bluey.sh",
  }), /requires a policy-bound authorization receipt/);
  await assert.rejects(() => access(planPath), { code: "ENOENT" });
});

async function createRemotePromotionFixture({
  activationGeneration,
  channelSequence,
  contractVersion = 2,
  expectedHeadRevision,
  expectedTransitionSha256,
  label,
  predecessorActivationSha256,
  releaseSequence,
  root,
  trust,
}) {
  const candidateFixture = await createAuthorizedCandidateFixture({
    contractVersion,
    label,
    releaseSequence,
    root,
    trust,
  });
  const { authorized, fixtureRoot } = candidateFixture;
  const manifest = JSON.parse(
    await readFile(join(authorized, "candidate/release-manifest.json"), "utf8"),
  );
  const candidateSet = JSON.parse(
    await readFile(join(authorized, "candidate/candidate-set.json"), "utf8"),
  );
  const authorization = JSON.parse(
    await readFile(join(authorized, "authorization-set.json"), "utf8"),
  );
  const nowMs = Date.now();
  const scope = {
    channel: "shadow",
    environment: "staging",
    region: "us-east-1",
  };
  const cohort = {
    accountIdSha256s: [],
    approvalRef: "test-only-shadow-approval",
    audience: MANAGED_CLOUD_AUDIENCES.cohort,
    cohortGeneration: activationGeneration,
    cohortId: `cohort-${label}`,
    expiresAtMs: nowMs + 20_000,
    issuedAtMs: nowMs - 3_000,
    notBeforeMs: nowMs - 500,
    rolloutMode: "none",
    scope,
    trustGeneration: 1,
    version: 1,
  };
  const cohortFile = join(fixtureRoot, "cohort.json");
  await writeCanonical(cohortFile, cohort);
  const cohortSignaturesFile = join(fixtureRoot, "cohort-signatures.json");
  await writeCanonical(
    cohortSignaturesFile,
    signedSet({
      id: `cohort-signatures-${label}`,
      keys: [{ keyId: "promotion-key", pair: trust.pairs.promotion }],
      role: "promotion",
      signedAtMs: nowMs - 400,
      targetAudience: MANAGED_CLOUD_AUDIENCES.cohort,
      targetBytes: await readFile(cohortFile),
    }),
  );
  const activationTimes = {
    expiresAtMs: nowMs + 10_000,
    issuedAtMs: nowMs - 1_000,
    notBeforeMs: nowMs - 400,
    portalReadbackAtMs: nowMs - 2_000,
  };
  const evidence = join(fixtureRoot, "activation-evidence");
  await mkdir(evidence);
  const evidenceEnvelope = {
    expiresAtMs: nowMs + 20_000,
    manifestSha256: candidateSet.manifestSha256,
    observedAtMs: activationTimes.portalReadbackAtMs,
    scope,
    status: "pass",
    version: 1,
  };
  const workflowsInventory = JSON.parse(
    await readFile(
      join(
        authorized,
        "candidate/evidence/jobs-workflows.content-inventory.json",
      ),
      "utf8",
    ),
  );
  const converterSha256 = workflowsInventory.entries.find(
    (entry) => entry.path === "app/workflows/dist/failure-converter.js",
  ).sha256;
  const portal = manifest.components.find(
    (component) => component.componentId === "jobs-portal",
  );
  const taskQueueSha256 = deriveManagedCloudTaskQueueSha256(
    "bluey-test",
    "bluey-jobs-applications",
  );
  const evidenceDocuments = {
    "canary.json": {
      ...evidenceEnvelope,
      audience: MANAGED_CLOUD_AUDIENCES.canaryEvidence,
      checks: deriveManagedCloudCanaryCheckIds(manifest.featureAuthority).map(
        (checkId) => ({
          checkId,
          evidenceSha256: "1".repeat(64),
          status: "pass",
        }),
      ),
    },
    "cleanup-authority.json": {
      ...evidenceEnvelope,
      audience: MANAGED_CLOUD_AUDIENCES.cleanupEvidence,
      cleanupAuthoritySha256: "2".repeat(64),
      dispatcherReady: true,
    },
    "failure-converter.json": {
      ...evidenceEnvelope,
      audience: MANAGED_CLOUD_AUDIENCES.failureConverterEvidence,
      deployedFileSha256: converterSha256,
    },
    "portal-readback.json": {
      ...evidenceEnvelope,
      artifactRef: portal.artifactRef,
      audience: MANAGED_CLOUD_AUDIENCES.portalReadbackEvidenceV2,
      portalArtifactSha256: portal.artifactSha256,
      readbackSha256: portal.artifactSha256,
    },
    "runner-fleet.json": {
      ...evidenceEnvelope,
      audience: MANAGED_CLOUD_AUDIENCES.runnerFleetEvidence,
      fleetIdSha256: "3".repeat(64),
      readyInstances: 1,
      requiredInstances: 1,
    },
    "storage-config.json": {
      ...evidenceEnvelope,
      audience: MANAGED_CLOUD_AUDIENCES.storageEvidence,
      readProbeSha256: "4".repeat(64),
      storageConfigSha256: "5".repeat(64),
      writeProbeSha256: "6".repeat(64),
    },
    "task-queue.json": {
      ...evidenceEnvelope,
      audience: MANAGED_CLOUD_AUDIENCES.taskQueueEvidence,
      namespace: "bluey-test",
      taskQueue: "bluey-jobs-applications",
      taskQueueSha256,
    },
    "temporal-namespace.json": {
      ...evidenceEnvelope,
      audience: MANAGED_CLOUD_AUDIENCES.temporalEvidence,
      gatewayReady: true,
      namespace: "bluey-test",
      namespaceSha256: sha256(Buffer.from("bluey-test")),
      workflowWorkerReady: true,
    },
  };
  for (const [name, value] of Object.entries(evidenceDocuments)) {
    await writeCanonical(join(evidence, name), value);
  }
  const activation = {
    activationGeneration,
    activationId: `activation-${label}`,
    audience:
      contractVersion === 4
        ? MANAGED_CLOUD_AUDIENCES.activationV4
        : contractVersion === 3
          ? MANAGED_CLOUD_AUDIENCES.activationV3
          : MANAGED_CLOUD_AUDIENCES.activationV2,
    canaryEvidenceSha256: await fileSha256(join(evidence, "canary.json")),
    channelSequence,
    cleanupAuthoritySha256: "2".repeat(64),
    cohortSha256: await fileSha256(cohortFile),
    cohortSignatureSetSha256: await fileSha256(cohortSignaturesFile),
    expectedHeadRevision,
    expectedTransitionSha256,
    expiresAtMs: activationTimes.expiresAtMs,
    failureConverterSha256: converterSha256,
    featureAuthority: manifest.featureAuthority,
    featureAuthoritySha256: manifest.featureAuthoritySha256,
    heartbeatTtlMs: 5_000,
    issuedAtMs: activationTimes.issuedAtMs,
    manifestSha256: candidateSet.manifestSha256,
    manifestSignatureSetSha256: authorization.releaseSignatureSetSha256,
    maximumDailyAdmissions: 0,
    maximumInflight: 0,
    notBeforeMs: activationTimes.notBeforeMs,
    portalReadbackAtMs: activationTimes.portalReadbackAtMs,
    portalReadbackEvidenceSha256: await fileSha256(
      join(evidence, "portal-readback.json"),
    ),
    portalReadbackTtlMs: 60_000,
    predecessorActivationSha256,
    recoveryAcceptances: [],
    runnerFleetEvidenceSha256: await fileSha256(
      join(evidence, "runner-fleet.json"),
    ),
    scope,
    storageConfigSha256: "5".repeat(64),
    taskQueueSha256,
    temporalNamespaceSha256: sha256(Buffer.from("bluey-test")),
    trustGeneration: 1,
    version: contractVersion,
  };
  const activationFile = join(fixtureRoot, "activation.json");
  await writeCanonical(activationFile, activation);
  const activationSignaturesFile = join(
    fixtureRoot,
    "activation-signatures.json",
  );
  await writeCanonical(
    activationSignaturesFile,
    signedSet({
      id: `activation-signatures-${label}`,
      keys: [{ keyId: "promotion-key", pair: trust.pairs.promotion }],
      role: "promotion",
      signedAtMs: nowMs,
      targetAudience:
        contractVersion === 4
          ? MANAGED_CLOUD_AUDIENCES.activationV4
          : contractVersion === 3
            ? MANAGED_CLOUD_AUDIENCES.activationV3
            : MANAGED_CLOUD_AUDIENCES.activationV2,
      targetBytes: await readFile(activationFile),
    }),
  );
  const publication = join(fixtureRoot, "publication");
  const publicationPlanFile = join(publication, "publication-plan.json");
  const plan = await createManagedCloudArtifactPublicationPlan({
    authorizedDirectory: authorized,
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    outputFile: publicationPlanFile,
    portalArtifactOrigin: "https://artifacts.bluey.sh/jobs/releases",
    registryNamespace: "ghcr.io/dhanunjay-divi/bluey-jobs",
  });
  const publicationReadback = join(fixtureRoot, "publication-readback");
  await mkdir(publicationReadback);
  for (const component of MANAGED_CLOUD_COMPONENTS) {
    const extension =
      component.artifactKind === "oci_image" ? ".oci.tar" : ".tar";
    await copyFile(
      join(authorized, "candidate/payloads", component.componentId + extension),
      join(publicationReadback, component.componentId + extension),
    );
  }
  const portalPlan = plan.artifacts.find(
    (artifact) => artifact.componentId === "jobs-portal",
  );
  const portalReadbackBytes = await readFile(
    join(publicationReadback, "jobs-portal.tar"),
  );
  const portalManifestBytes = canonicalJsonBytes(
    portalArtifactManifest(portalReadbackBytes, {
      annotations: {
        "org.opencontainers.image.created": plan.portalManifestCreatedAt,
      },
    }),
  );
  await writeFile(
    join(publicationReadback, "jobs-portal.oci-manifest.json"),
    portalManifestBytes,
  );
  const portalRegistryRef =
    portalPlan.publicationRepository + "@sha256:" + sha256(portalManifestBytes);
  const workflowRunId = 20_000 + releaseSequence;
  const workflowRunAttempt = 1;
  const publicationSetFile = join(publication, "publication-set.json");
  const publicationSet = await attestManagedCloudArtifactPublication({
    authorizedDirectory: authorized,
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    outputFile: publicationSetFile,
    planFile: publicationPlanFile,
    portalRegistryRef,
    readbackDirectory: publicationReadback,
    workflowRunAttempt,
    workflowRunId,
  });
  const run = {
    conclusion: "success",
    event: "workflow_dispatch",
    head_branch: "main",
    head_sha: SOURCE_COMMIT,
    id: workflowRunId,
    path: ".github/workflows/jobs-managed-cloud-release.yml@main",
    repository: { full_name: "Dhanunjay-Divi/bluey", id: 611 },
    run_attempt: workflowRunAttempt,
    status: "completed",
  };
  const jobs = {
    jobs: [
      {
        conclusion: "success",
        head_sha: SOURCE_COMMIT,
        id: workflowRunId + 1,
        name: "publish",
        run_attempt: workflowRunAttempt,
        run_id: workflowRunId,
        status: "completed",
        steps: [
          {
            conclusion: "success",
            name: "Upload the remote publication receipt only",
            status: "completed",
          },
        ],
      },
    ],
    total_count: 1,
  };
  const workflowEvidence = createManagedCloudPublicationWorkflowEvidence({
    artifacts: publicationWorkflowArtifacts({ runId: workflowRunId }),
    defaultBranch: "main",
    jobs,
    plan,
    publication: publicationSet,
    run,
  });
  const workflowEvidenceFile = join(
    fixtureRoot,
    "publication-workflow-evidence.json",
  );
  await writeCanonical(workflowEvidenceFile, workflowEvidence);
  const preflightFile = join(fixtureRoot, "publication-preflight.json");
  await writeCanonical(
    preflightFile,
    createManagedCloudPublicationPreflightDocument({
      plan,
      publication: publicationSet,
      publicationPlanSha256: await fileSha256(publicationPlanFile),
      publicationSetSha256: await fileSha256(publicationSetFile),
      workflowEvidence,
    }),
  );
  const promotion = join(fixtureRoot, "promotion-v2");
  const promotionSet = await promoteManagedCloudCandidate({
    activationFile,
    activationSignatureSetFile: activationSignaturesFile,
    authorizedDirectory: authorized,
    cohortFile,
    cohortSignatureSetFile: cohortSignaturesFile,
    evidenceDirectory: evidence,
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    outputDirectory: promotion,
    publicationDirectory: publication,
    publicationPreflightFile: preflightFile,
    publicationReadbackDirectory: publicationReadback,
    promotionId: `promotion-${label}`,
  });
  return {
    activationFile,
    authorized,
    fixtureRoot,
    promotion,
    promotionSet,
  };
}

async function createHistoricalPromotionFixture(remoteFixture, output) {
  await mkdir(output);
  for (const name of [
    "activation-signatures.json",
    "activation.json",
    "cohort-signatures.json",
    "cohort.json",
  ]) {
    await copyFile(join(remoteFixture.promotion, name), join(output, name));
  }
  await cp(
    join(remoteFixture.promotion, "authorized-candidate"),
    join(output, "authorized-candidate"),
    { recursive: true },
  );
  await cp(
    join(remoteFixture.promotion, "evidence"),
    join(output, "evidence"),
    {
      recursive: true,
    },
  );
  const candidateSet = JSON.parse(
    await readFile(
      join(output, "authorized-candidate/candidate/candidate-set.json"),
      "utf8",
    ),
  );
  const payloads = candidateSet.payloads.filter((payload) =>
    payload.path.startsWith("payloads/"),
  );
  for (const rootName of ["published", "readback"]) {
    for (const payload of payloads) {
      const target = join(output, rootName, payload.path);
      await mkdir(join(target, ".."), { recursive: true });
      await copyFile(
        join(output, "authorized-candidate/candidate", payload.path),
        target,
      );
    }
  }
  const remoteSet = remoteFixture.promotionSet;
  const historicalSet = {
    activationSha256: remoteSet.activationSha256,
    activationSignatureSetSha256: remoteSet.activationSignatureSetSha256,
    activationSignerKeyIds: remoteSet.activationSignerKeyIds,
    adapter: "local_fixture",
    audience: MANAGED_CLOUD_AUDIENCES.promotion,
    authorizationSetSha256: remoteSet.authorizationSetSha256,
    cohortSha256: remoteSet.cohortSha256,
    cohortSignatureSetSha256: remoteSet.cohortSignatureSetSha256,
    cohortSignerKeyIds: remoteSet.cohortSignerKeyIds,
    manifestSha256: remoteSet.manifestSha256,
    promotedAtMs: remoteSet.promotedAtMs,
    promotionId: remoteSet.promotionId + "-historical",
    publishedPayloads: payloads,
    readbackPayloads: payloads,
    releaseSignatureSetSha256: remoteSet.releaseSignatureSetSha256,
    trustGeneration: remoteSet.trustGeneration,
    version: 1,
  };
  await writeCanonical(join(output, "promotion-set.json"), historicalSet);
  return historicalSet;
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
    () =>
      validateReleaseManifest({
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
    () =>
      validateReleaseManifest({
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
    () =>
      validateReleaseManifest({
        ...manifest,
        sqliteMigrationHead: "056_jobs_canonical_taxonomy_authority",
      }),
    /migration heads/,
  );
  assert.throws(
    () =>
      validateReleaseManifest({
        ...manifest,
        manifestId: "m".repeat(128 * 1024),
      }),
    /128 KiB/,
  );
  for (const feature of ["directDiscovery", "globalDiscovery"]) {
    assert.throws(
      () =>
        validateReleaseManifest({
          ...manifest,
          featureAuthority: {
            ...manifest.featureAuthority,
            [feature]: true,
          },
        }),
      /reserves conditional discovery/,
    );
  }
  const portal = manifest.components.find(
    (item) => item.componentId === "jobs-portal",
  );
  assert.throws(
    () =>
      validateReleaseManifest({
        ...manifest,
        components: manifest.components.map((item) =>
          item.componentId === "jobs-portal"
            ? {
                ...item,
                artifactRef:
                  "https://artifacts.example/releases/" +
                  RELEASE_ID +
                  "/portal.tar",
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
  assert.deepEqual(
    deriveRequiredRuntimeRoles(manifest.featureAuthority),
    [...MANAGED_CLOUD_BASE_RUNTIME_ROLES, "original_source_verifier"].sort(),
  );
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
  assert.doesNotThrow(() =>
    deriveManagedCloudRuntimeIdentitySha256(
      "a".repeat(64),
      "jobs-workflows",
      "original_source_verifier",
    ),
  );

  const versionOneEnabled = releaseFixture();
  versionOneEnabled.featureAuthority.sourceVerification = true;
  const versionTwoDisabled = structuredClone(manifest);
  versionTwoDisabled.featureAuthority.sourceVerification = false;
  for (const invalid of [versionOneEnabled, versionTwoDisabled]) {
    invalid.featureAuthoritySha256 = sha256(
      canonicalJsonBytes(invalid.featureAuthority),
    );
    assert.throws(
      () => validateReleaseManifest(invalid),
      /v1=false or v2=true/,
    );
  }

  const missingCapability = structuredClone(manifest);
  missingCapability.capabilities = missingCapability.capabilities.filter(
    (entry) => entry.capability !== "original_source_verifier",
  );
  assert.throws(() => validateReleaseManifest(missingCapability), /capabilit/);

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
  contracts.protocolContract.protocols
    .find((entry) => entry.protocolId === "source_verification")
    .sourceFiles.pop();
  assert.throws(
    () => validateManagedCloudReleaseContracts(contracts),
    /required source paths/,
  );
});

test("release-v1 and v2 preserve their historical canonical fixture bytes", () => {
  const releaseV1 = releaseFixture();
  const releaseV2 = successorReleaseFixture();
  assert.equal(
    sha256(canonicalJsonBytes(releaseV1)),
    "2c8598c721b40595ba3bf9f72d8873f3eb8a86fb2f559775966b041c5cec2848",
  );
  assert.equal(
    sha256(canonicalJsonBytes(releaseV2)),
    "e4eaa40b864b15c4c6e8135c3c2dc1601f10b3ab3a11ae413b50dcd6a6614a34",
  );
  assert.equal(
    sha256(canonicalJsonBytes(releaseContractFixture())),
    "0ff2cd59129ca3c0089a9841276a2e3b888b51f8d02b7c256ed37113911bd59a",
  );
  assert.equal(
    sha256(canonicalJsonBytes(successorReleaseContractFixture())),
    "52455ce26c1129a24619c2fdb1c12e7d25d0c53cce16ebd838757b9cdf1f1083",
  );
  assert.deepEqual(
    [releaseV1, releaseV2].map((manifest) => ({
      audience: manifest.audience,
      featureAuthority: manifest.featureAuthority,
      postgresMigrationHead: manifest.postgresMigrationHead,
      protocolOrdinals: manifest.protocols.map((entry) => entry.protocolId),
      sqliteMigrationHead: manifest.sqliteMigrationHead,
      version: manifest.version,
    })),
    [
      {
        audience: MANAGED_CLOUD_AUDIENCES.manifest,
        featureAuthority: {
          cloudDistribution: true,
          directDiscovery: false,
          globalDiscovery: false,
          sourceVerification: false,
          workflowCleanup: true,
          workflowCommandDispatch: true,
        },
        postgresMigrationHead: "034_jobs_canonical_taxonomy_authority.sql",
        protocolOrdinals: MANAGED_CLOUD_PROTOCOL_IDS,
        sqliteMigrationHead: "056_jobs_canonical_taxonomy_authority.sql",
        version: 1,
      },
      {
        audience: MANAGED_CLOUD_AUDIENCES.manifestV2,
        featureAuthority: {
          cloudDistribution: true,
          directDiscovery: false,
          globalDiscovery: false,
          sourceVerification: true,
          workflowCleanup: true,
          workflowCommandDispatch: true,
        },
        postgresMigrationHead:
          "035_jobs_original_source_verification_authority.sql",
        protocolOrdinals: MANAGED_CLOUD_SUCCESSOR_PROTOCOL_IDS,
        sqliteMigrationHead:
          "057_jobs_original_source_verification_authority.sql",
        version: 2,
      },
    ],
  );
});

test("release-v3 exactly binds discovery, verifier startup, migrations, and nine roles", () => {
  const manifest = releaseV3Fixture();
  assert.equal(validateReleaseManifest(manifest), manifest);
  assert.equal(manifest.audience, "bluey-jobs-managed-cloud-release-v3");
  assert.deepEqual(MANAGED_CLOUD_PROTOCOL_IDS, [
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
  assert.deepEqual(MANAGED_CLOUD_SUCCESSOR_PROTOCOL_IDS, [
    ...MANAGED_CLOUD_PROTOCOL_IDS.slice(0, 9),
    "source_verification",
    ...MANAGED_CLOUD_PROTOCOL_IDS.slice(9),
  ]);
  assert.deepEqual(MANAGED_CLOUD_RELEASE_V3_PROTOCOL_IDS, [
    ...MANAGED_CLOUD_SUCCESSOR_PROTOCOL_IDS.slice(0, 10),
    "source_verifier_startup",
    "source_verifier_boot",
    ...MANAGED_CLOUD_SUCCESSOR_PROTOCOL_IDS.slice(10),
  ]);
  assert.deepEqual(
    deriveRequiredRuntimeRoles(manifest.featureAuthority),
    [
      ...MANAGED_CLOUD_BASE_RUNTIME_ROLES,
      "discovery_worker",
      "global_discovery_worker",
      "original_source_verifier",
    ].sort(),
  );
  assert.deepEqual(
    manifest.capabilities.filter(
      (entry) => entry.componentId === "jobs-workflows",
    ),
    [
      "discovery_worker",
      "global_discovery_worker",
      "original_source_verifier",
      "workflow_gateway",
      "workflow_worker",
    ].map((capability) => ({ capability, componentId: "jobs-workflows" })),
  );
  assert.deepEqual(
    deriveManagedCloudCanaryCheckIds(manifest.featureAuthority),
    [
      ...CANARY_CHECK_IDS,
      "discovery-worker-readiness",
      "global-discovery-worker-readiness",
      "original-source-verifier-readiness",
    ].sort(),
  );
  assert.deepEqual(
    deriveManagedCloudRequiredRuntimePaths(
      "jobs-workflows",
      manifest.capabilities,
    ),
    [
      "app/.bluey/managed-cloud-runtime-measurement.json",
      "app/workflows/dist/discovery-worker.js",
      "app/workflows/dist/failure-converter.js",
      "app/workflows/dist/gateway.js",
      "app/workflows/dist/global-discovery-worker.js",
      "app/workflows/dist/original-source-verifier.js",
      "app/workflows/dist/worker.js",
      "usr/local/bin/node",
    ],
  );

  const contracts = releaseV3ContractFixture();
  assert.equal(validateManagedCloudReleaseContracts(contracts), true);
  assert.equal(contracts.builderPolicy.schemaVersion, 2);
  assert.equal(
    validateManagedCloudPublicationBuilderPolicy(contracts.builderPolicy, 3),
    true,
  );
  assert.deepEqual(
    contracts.protocolContract.protocols
      .find((entry) => entry.protocolId === "source_verifier_startup")
      .sourceFiles.map((entry) => entry.path),
    SOURCE_VERIFIER_STARTUP_PROTOCOL_PATHS,
  );
  assert.deepEqual(
    contracts.protocolContract.protocols
      .find((entry) => entry.protocolId === "source_verifier_boot")
      .sourceFiles.map((entry) => entry.path),
    SOURCE_VERIFIER_BOOT_PROTOCOL_PATHS,
  );

  for (const [label, mutate, expected] of [
    [
      "v2 audience",
      (value) => {
        value.audience = MANAGED_CLOUD_AUDIENCES.manifestV2;
      },
      /audience or version/,
    ],
    [
      "v2 migration heads",
      (value) => {
        value.sqliteMigrationHead =
          "057_jobs_original_source_verification_authority.sql";
      },
      /migration heads/,
    ],
    [
      "v2 postgres migration head",
      (value) => {
        value.postgresMigrationHead =
          "035_jobs_original_source_verification_authority.sql";
      },
      /migration heads/,
    ],
    [
      "disabled direct discovery",
      (value) => {
        value.featureAuthority.directDiscovery = false;
        value.featureAuthoritySha256 = sha256(
          canonicalJsonBytes(value.featureAuthority),
        );
      },
      /Release-v3 requires direct and global discovery/,
    ],
    [
      "disabled global discovery",
      (value) => {
        value.featureAuthority.globalDiscovery = false;
        value.featureAuthoritySha256 = sha256(
          canonicalJsonBytes(value.featureAuthority),
        );
      },
      /Release-v3 requires direct and global discovery/,
    ],
    [
      "disabled source verification",
      (value) => {
        value.featureAuthority.sourceVerification = false;
        value.featureAuthoritySha256 = sha256(
          canonicalJsonBytes(value.featureAuthority),
        );
      },
      /release v3 also requires true/,
    ],
    [
      "missing global discovery capability",
      (value) => {
        value.capabilities = value.capabilities.filter(
          (entry) => entry.capability !== "global_discovery_worker",
        );
      },
      /capabilit/,
    ],
    [
      "startup protocol omission",
      (value) => {
        value.protocols = value.protocols.filter(
          (entry) => entry.protocolId !== "source_verifier_startup",
        );
      },
      /bind exactly|protocol/,
    ],
    [
      "boot protocol omission",
      (value) => {
        value.protocols = value.protocols.filter(
          (entry) => entry.protocolId !== "source_verifier_boot",
        );
      },
      /bind exactly|protocol/,
    ],
  ]) {
    const mutated = structuredClone(manifest);
    mutate(mutated);
    assert.throws(
      () => validateReleaseManifest(mutated),
      expected,
      `${label} must not cross the v3 contract boundary`,
    );
  }

  const v2WithStartup = successorReleaseFixture();
  v2WithStartup.protocols.splice(10, 0, {
    protocolId: "source_verifier_startup",
    protocolVersion: 1,
    schemaSha256: "a".repeat(64),
  });
  assert.throws(
    () => validateReleaseManifest(v2WithStartup),
    /bind exactly|protocol/,
  );

  const wrongStartupSources = releaseV3ContractFixture();
  wrongStartupSources.protocolContract.protocols
    .find((entry) => entry.protocolId === "source_verifier_startup")
    .sourceFiles.pop();
  assert.throws(
    () => validateManagedCloudReleaseContracts(wrongStartupSources),
    /required source paths/,
  );

  const wrongBootSources = releaseV3ContractFixture();
  wrongBootSources.protocolContract.protocols
    .find((entry) => entry.protocolId === "source_verifier_boot")
    .sourceFiles.pop();
  assert.throws(
    () => validateManagedCloudReleaseContracts(wrongBootSources),
    /required source paths/,
  );

  const missingStartupSigner = releaseV3ContractFixture();
  const startupSignerSources =
    missingStartupSigner.protocolContract.protocols.find(
      (entry) => entry.protocolId === "source_verifier_startup",
    ).sourceFiles;
  const startupSignerIndex = startupSignerSources.findIndex(
    (entry) => entry.path === "jobs/automation/src/worker-auth.ts",
  );
  assert.notEqual(startupSignerIndex, -1, "startup signer fixture must exist");
  startupSignerSources.splice(startupSignerIndex, 1);
  assert.throws(
    () => validateManagedCloudReleaseContracts(missingStartupSigner),
    /required source paths/,
  );

  const wrongStartupVersion = releaseV3ContractFixture();
  wrongStartupVersion.protocolContract.protocols.find(
    (entry) => entry.protocolId === "source_verifier_startup",
  ).protocolVersion = 2;
  assert.throws(
    () => validateManagedCloudReleaseContracts(wrongStartupVersion),
    /identity\/version set is not exact/,
  );

  const wrongBootVersion = releaseV3ContractFixture();
  wrongBootVersion.protocolContract.protocols.find(
    (entry) => entry.protocolId === "source_verifier_boot",
  ).protocolVersion = 2;
  assert.throws(
    () => validateManagedCloudReleaseContracts(wrongBootVersion),
    /identity\/version set is not exact/,
  );

  const v2ProtocolContract = releaseV3ContractFixture();
  v2ProtocolContract.protocolContract.schemaVersion = 2;
  assert.throws(
    () => validateManagedCloudReleaseContracts(v2ProtocolContract),
    /one exact version/,
  );

  const builderV3 = releaseV3ContractFixture();
  builderV3.builderPolicy.schemaVersion = 3;
  assert.throws(
    () => validateManagedCloudReleaseContracts(builderV3),
    /build-once contract/,
  );
  const legacyBuilderV3 = releaseV3ContractFixture();
  legacyBuilderV3.builderPolicy = structuredClone(
    releaseContractFixture().builderPolicy,
  );
  assert.throws(
    () => validateManagedCloudReleaseContracts(legacyBuilderV3),
    /requires builder policy generation v2/,
  );
});

test("release-v4 binds distinct domains, exact qualified migrations and the builder closure", () => {
  const manifest = releaseV4Fixture();
  assert.equal(validateReleaseManifest(manifest), manifest);
  assert.equal(manifest.audience, "bluey-jobs-managed-cloud-release-v4");
  assert.equal(manifest.sqliteMigrationHead, "073_jobs_managed_cloud_release_v4_audiences.sql");
  assert.equal(manifest.postgresMigrationHead, "052_jobs_managed_cloud_release_v4_audiences.sql");
  assert.deepEqual(manifest.protocols, releaseV3Fixture().protocols);
  assert.deepEqual(manifest.capabilities, releaseV3Fixture().capabilities);
  assert.deepEqual(manifest.featureAuthority, releaseV3Fixture().featureAuthority);
  assert.deepEqual(
    deriveRequiredRuntimeRoles(manifest.featureAuthority),
    [
      ...MANAGED_CLOUD_BASE_RUNTIME_ROLES,
      "discovery_worker",
      "global_discovery_worker",
      "original_source_verifier",
    ].sort(),
  );

  const contracts = releaseV4ContractFixture();
  assert.equal(validateManagedCloudReleaseContracts(contracts), true);
  assert.equal(contracts.builderPolicy.schemaVersion, 3);
  assert.equal(contracts.builderPolicy.audience, MANAGED_CLOUD_AUDIENCES.builderPolicyV3);
  assert.deepEqual(
    contracts.protocolContract.protocols,
    releaseV3ContractFixture().protocolContract.protocols,
  );
  assert.deepEqual(
    contracts.migrationContract.sqlite.files.map((file) => basename(file.path)),
    [
      "066_jobs_attempt_employer_identity.sql",
      "067_jobs_provisioned_runtime_startup.sql",
      "068_jobs_provisioned_runtime_observations.sql",
      "069_jobs_ats_storage_fences.sql",
      "070_jobs_application_answer_revisions.sql",
      "071_jobs_workday_reconciliation_holds.sql",
      "072_jobs_mcp_read_grants.sql",
      "073_jobs_managed_cloud_release_v4_audiences.sql",
    ],
  );
  assert.deepEqual(
    contracts.migrationContract.postgres.files.map((file) => basename(file.path)),
    [
      "044_jobs_attempt_employer_identity.sql",
      "045_jobs_provisioned_runtime_startup.sql",
      "046_jobs_provisioned_runtime_observations.sql",
      "047_jobs_ats_storage_fences.sql",
      "048_jobs_application_answer_revisions.sql",
      "049_jobs_process_attachment_signature_audience.sql",
      "050_jobs_workday_reconciliation_holds.sql",
      "051_jobs_mcp_read_grants.sql",
      "052_jobs_managed_cloud_release_v4_audiences.sql",
    ],
  );

  for (const [label, mutate, expected] of [
    [
      "v3 manifest audience",
      (value) => {
        value.audience = MANAGED_CLOUD_AUDIENCES.manifestV3;
      },
      /audience or version/,
    ],
    [
      "v3 SQLite migration head",
      (value) => {
        value.sqliteMigrationHead =
          "065_jobs_source_verifier_heartbeat_boot_bindings.sql";
      },
      /migration heads/,
    ],
    [
      "v3 PostgreSQL migration head",
      (value) => {
        value.postgresMigrationHead =
          "043_jobs_source_verifier_heartbeat_boot_bindings.sql";
      },
      /migration heads/,
    ],
  ]) {
    const invalid = structuredClone(manifest);
    mutate(invalid);
    assert.throws(() => validateReleaseManifest(invalid), expected, label);
  }

  const mixedVersion = releaseV4ContractFixture();
  mixedVersion.protocolContract.schemaVersion = 3;
  assert.throws(
    () => validateManagedCloudReleaseContracts(mixedVersion),
    /one exact version/,
  );

  const rebindMigrations = (fixture) => {
    for (const dialect of ["postgres", "sqlite"]) {
      fixture.migrationContract[dialect].setSha256 = sha256(
        canonicalJsonBytes(fixture.migrationContract[dialect].files),
      );
    }
    fixture.migrationContract.paritySha256 = sha256(
      canonicalJsonBytes({
        postgres: fixture.migrationContract.postgres.files.map((file) =>
          basename(file.path),
        ),
        sqlite: fixture.migrationContract.sqlite.files.map((file) =>
          basename(file.path),
        ),
      }),
    );
    fixture.manifest.migrationSetSha256 = sha256(
      canonicalJsonBytes(fixture.migrationContract),
    );
  };
  const missing = releaseV4ContractFixture();
  missing.migrationContract.sqlite.files.splice(3, 1);
  rebindMigrations(missing);
  assert.throws(
    () => validateManagedCloudReleaseContracts(missing),
    /release-v4 migration suffix is incomplete or out of order/,
  );

  const extra = releaseV4ContractFixture();
  extra.migrationContract.postgres.files.splice(
    -1,
    0,
    sourceFile(
      "infra/postgres/server-runtime/051a_unqualified_extra.sql",
      "d",
    ),
  );
  rebindMigrations(extra);
  assert.throws(
    () => validateManagedCloudReleaseContracts(extra),
    /release-v4 migration suffix is incomplete or out of order/,
  );
});

test("release-v4 builder policy rejects downgraded, crossed and incomplete input closures", () => {
  const current = releaseV4ContractFixture();
  const legacy = releaseV3ContractFixture();
  assert.equal(validateManagedCloudPublicationBuilderPolicy(current.builderPolicy, 4), true);
  assert.throws(
    () => validateManagedCloudReleaseContracts({ ...current, builderPolicy: legacy.builderPolicy }),
    /Release-v4 requires builder policy generation v3/,
  );
  assert.throws(
    () => validateManagedCloudReleaseContracts({ ...legacy, builderPolicy: current.builderPolicy }),
    /Release-v3 requires builder policy generation v2/,
  );
  assert.throws(
    () => validateManagedCloudReleaseContracts({
      ...successorReleaseContractFixture(), builderPolicy: current.builderPolicy,
    }),
    /Release-v2 requires a historical builder policy generation/,
  );
  assert.throws(
    () => validateManagedCloudPublicationBuilderPolicy(legacy.builderPolicy, 4),
    /Release-v4 publication requires builder policy v3/,
  );
  assert.throws(
    () => validateManagedCloudPublicationBuilderPolicy(current.builderPolicy, 3),
    /Release-v3 publication requires builder policy v2/,
  );
  assert.throws(
    () => validateManagedCloudPublicationBuilderPolicy(current.builderPolicy),
    /requires release-v2 or later/,
  );
  for (const path of V4_MEASUREMENT_HELPERS) {
    const missing = structuredClone(current);
    missing.builderPolicy.files = missing.builderPolicy.files.filter((entry) => entry.path !== path);
    missing.builderPolicy.setSha256 = sha256(canonicalJsonBytes(missing.builderPolicy.files));
    assert.throws(() => validateManagedCloudReleaseContracts(missing), /exact required source paths/);
    const tampered = structuredClone(current);
    const entry = tampered.builderPolicy.files.find((item) => item.path === path);
    entry.sha256 = "f".repeat(64);
    assert.throws(() => validateManagedCloudReleaseContracts(tampered), /source-set digest is invalid/);
  }
  const extra = structuredClone(current);
  extra.builderPolicy.files.push(sourceFile("z-unsanctioned-in-build.js", "c"));
  extra.builderPolicy.setSha256 = sha256(canonicalJsonBytes(extra.builderPolicy.files));
  assert.throws(() => validateManagedCloudReleaseContracts(extra), /exact required source paths/);
});

test("release contracts prove full source sets without configured runtime identity", () => {
  const fixture = releaseContractFixture();
  assert.equal(validateManagedCloudReleaseContracts(fixture), true);
  assert.throws(
    () => validateManagedCloudPublicationBuilderPolicy(fixture.builderPolicy, 2),
    /requires builder policy v2/,
  );

  const successor = successorReleaseContractFixture();
  assert.equal(validateManagedCloudReleaseContracts(successor), true);
  assert.equal(
    validateManagedCloudPublicationBuilderPolicy(successor.builderPolicy, 2),
    true,
  );
  const historicalSuccessor = structuredClone(successor);
  historicalSuccessor.builderPolicy = structuredClone(fixture.builderPolicy);
  assert.equal(validateManagedCloudReleaseContracts(historicalSuccessor), true);
  assert.throws(
    () =>
      validateManagedCloudPublicationBuilderPolicy(
        historicalSuccessor.builderPolicy,
        2,
      ),
    /requires builder policy v2/,
  );

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
  apiRuntimeDisabled.configContract.variables.find(
    (variable) => variable.name === "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED",
  ).roles = ["jobs_workflows", "managed_runner"];
  assert.throws(
    () => validateManagedCloudReleaseContracts(apiRuntimeDisabled),
    /missing an owning runtime role/,
  );

  const reconciliationOptional = structuredClone(fixture);
  reconciliationOptional.configContract.variables.find(
    (variable) =>
      variable.name === "BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED",
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
    sha256(
      Buffer.concat([
        Buffer.from("bluey-jobs-managed-cloud-runtime-identity-v1\0"),
        Buffer.from(measurementSha256),
        Buffer.from("\0jobs-workflows\0workflow_gateway"),
      ]),
    ),
  );
});

test("release-v2 runtime measurement binds the exact verifier role and entrypoint bytes", () => {
  const roles = [
    "original_source_verifier",
    "workflow_gateway",
    "workflow_worker",
  ];
  const measuredFiles = [
    {
      path: "app/automation/dist/original-source-verification.js",
      sha256: "1".repeat(64),
    },
    {
      path: "app/workflows/dist/failure-converter.js",
      sha256: "2".repeat(64),
    },
    {
      path: "app/workflows/dist/original-source-verifier.js",
      sha256: "3".repeat(64),
    },
    {
      path: "app/workflows/dist/worker.js",
      sha256: "4".repeat(64),
    },
    { path: "usr/local/bin/node", sha256: "5".repeat(64) },
  ];
  const measurement = {
    audience: MANAGED_CLOUD_AUDIENCES.runtimeMeasurement,
    buildId: "build-jobs-workflows-v2",
    componentId: "jobs-workflows",
    configSchemaSha256: "c".repeat(64),
    measuredFiles,
    migrationSetSha256: "d".repeat(64),
    protocolSetSha256: "e".repeat(64),
    roles,
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
    ].sort((left, right) =>
      Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)),
    ),
  };
  const runtimeIdentities = roles.map((role) => ({
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
    roles,
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
  assert.deepEqual(
    deriveManagedCloudRequiredRuntimePaths(
      "jobs-workflows",
      successorReleaseFixture().capabilities,
    ).filter((path) => path.includes("original-source-verifier")),
    ["app/workflows/dist/original-source-verifier.js"],
  );

  const missingVerifierRole = roles.slice(1);
  assert.throws(
    () =>
      validateManagedCloudRuntimeMeasurement(
        { ...measurement, roles: missingVerifierRole },
        sha256(
          canonicalJsonBytes({ ...measurement, roles: missingVerifierRole }),
        ),
        runtimeIdentities.slice(1),
        context,
      ),
    /not exact/,
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
    ].sort((left, right) =>
      Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)),
    ),
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
    () =>
      validateManagedCloudRuntimeMeasurement(
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

test("filesystem runtime measurement accepts 128 KiB and rejects one extra byte", async (t) => {
  const root = await temporaryDirectory(t, "runtime-measurement-bytes");
  const directory = "app/workflows/dist/" + "p".repeat(120);
  const nodePath = "usr/local/bin/node";
  const contents = "fixture runtime\n";
  const fileSha256 = sha256(contents);
  const fixtureFiles = Array.from({ length: 500 }, (_, index) => ({
    path: directory + "/" + index.toString().padStart(3, "0") + ".js",
    sha256: fileSha256,
  }));
  const input = {
    buildId: "build-jobs-workflows",
    componentId: "jobs-workflows",
    configSchemaSha256: "c".repeat(64),
    migrationSetSha256: "d".repeat(64),
    protocolSetSha256: "e".repeat(64),
    roles: ["workflow_gateway", "workflow_worker"],
    sourceCommit: SOURCE_COMMIT,
  };
  const document = {
    ...input,
    audience: MANAGED_CLOUD_AUDIENCES.runtimeMeasurement,
    measuredFiles: [...fixtureFiles, { path: nodePath, sha256: fileSha256 }],
    version: 1,
  };
  const maximum = 128 * 1024;
  let remaining = maximum - canonicalJsonBytes(document).length;
  assert.ok(remaining > 0);
  for (const file of fixtureFiles) {
    const extra = Math.min(remaining, 200);
    file.path = file.path.slice(0, -3) + "x".repeat(extra) + ".js";
    remaining -= extra;
  }
  assert.equal(remaining, 0);
  assert.equal(canonicalJsonBytes(document).length, maximum);
  await mkdir(join(root, directory), { recursive: true });
  await mkdir(join(root, "app/automation"), { recursive: true });
  await mkdir(join(root, "usr/local/bin"), { recursive: true });
  for (const file of document.measuredFiles) {
    await writeFile(join(root, file.path), contents);
  }
  const measured = await createRuntimeMeasurementFromFilesystem({ ...input, root });
  assert.deepEqual(canonicalJsonBytes(measured), canonicalJsonBytes(document));

  const lastPath = fixtureFiles.at(-1).path;
  await rename(join(root, lastPath), join(root, lastPath.slice(0, -3) + "x.js"));
  await assert.rejects(
    () => createRuntimeMeasurementFromFilesystem({ ...input, root }),
    /runtime measurement exceeds 128 KiB/,
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
    () =>
      createRuntimeMeasurementFromFilesystem({
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
    deriveManagedCloudTaskQueueSha256("bluey-prod", "bluey-jobs-applications"),
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
      const entries = requiredPaths
        .map((path) => ({
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
        }))
        .sort((left, right) =>
          Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)),
        );
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
    () =>
      validateManagedCloudInventoryAttachments({
        ...attachments,
        "jobs-api": attachments["jobs-api"] + "=",
      }),
    /base64url/,
  );
  assert.throws(
    () =>
      validateManagedCloudInventoryAttachments({
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
    () =>
      validateManagedCloudInventoryAttachments({
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
    () =>
      verifyManagedCloudSignatureSet({
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
    () =>
      verifyManagedCloudSignatureSet({
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
    () =>
      validateManagedCloudTrustBundle(
        rootOverflow,
        sha256(canonicalJsonBytes(rootOverflow.rootAnchor)),
      ),
    /at most 32/,
  );

  const preIssued = structuredClone(fixture.trustBundle);
  preIssued.policies[0].authorization.signedAtMs =
    preIssued.policies[0].policy.issuedAtMs - 1;
  assert.throws(
    () => validateManagedCloudTrustBundle(preIssued, fixture.rootAnchorSha256),
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
    () =>
      validateManagedCloudTrustBundle(
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
  await writeFile(
    good,
    tarArchive([{ body: "<main>Bluey</main>\n", name: "index.html" }]),
  );
  const inspected = await inspectStaticBundleArchive(good);
  assert.equal(inspected.entries[0].path, "index.html");

  const trailing = join(root, "trailing.tar");
  const trailingBytes = Buffer.concat([
    await readFile(good),
    Buffer.alloc(512),
  ]);
  trailingBytes[trailingBytes.length - 1] = 1;
  await writeFile(trailing, trailingBytes);
  await assert.rejects(
    () => inspectStaticBundleArchive(trailing),
    /nonzero bytes/,
  );

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
  const fixtureIdentity = await writeOciFixture(good, {
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
  const artifact = await inspectOciImageArtifact(good, "jobs-api");
  const inspected = await inspectOciImageArchive(good, "jobs-api");
  assert.deepEqual(Object.keys(artifact).sort(), ["imageConfigSha256", "inventory"]);
  assert.equal(artifact.imageConfigSha256, fixtureIdentity.imageConfigSha256);
  assert.equal(artifact.inventory.artifactSha256, fixtureIdentity.imageManifestSha256);
  assert.deepEqual(artifact.inventory, inspected);
  assert.ok(inspected.entries.some((entry) => entry.path === "app/cache/new"));
  assert.ok(!inspected.entries.some((entry) => entry.path === "app/cache/old"));

  const tamperedConfig = join(root, "tampered-config.oci.tar");
  const tamperedBytes = Buffer.from(await readFile(good));
  const architecture = Buffer.from('"architecture":"amd64"');
  const architectureOffset = tamperedBytes.indexOf(architecture);
  assert.ok(architectureOffset >= 0);
  tamperedBytes.set(Buffer.from('"architecture":"arm64"'), architectureOffset);
  await writeFile(tamperedConfig, tamperedBytes);
  await assert.rejects(
    () => inspectOciImageArtifact(tamperedConfig, "jobs-api"),
    /content digest/,
  );

  const tamperedRuntime = join(root, "tampered-runtime-config.oci.tar");
  await writeOciFixture(tamperedRuntime, {
    runtime: apiRuntime({ AttachStdin: true }),
  });
  await assert.rejects(
    () => inspectOciImageArtifact(tamperedRuntime, "jobs-api"),
    /unknown key/,
  );

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
    layers: [
      [
        { name: "app", type: "5" },
        { body: "api", mode: 0o555, name: "app/api" },
        {
          linkTarget: "/app/api",
          mode: 0o555,
          name: "usr/local/bin/bluey-jobs-api",
          type: "2",
        },
      ],
    ],
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

  const workflowsPaths = [
    { body: "discovery\n", name: "app/workflows/dist/discovery-worker.js" },
    { body: "failure\n", name: "app/workflows/dist/failure-converter.js" },
    { body: "gateway\n", name: "app/workflows/dist/gateway.js" },
    {
      body: "global-discovery\n",
      name: "app/workflows/dist/global-discovery-worker.js",
    },
    { body: "worker\n", name: "app/workflows/dist/worker.js" },
    { body: "node\n", mode: 0o555, name: "usr/local/bin/node" },
  ];
  const exactVerifier = join(root, "exact-source-verifier.oci.tar");
  await writeOciFixture(exactVerifier, {
    layers: [
      [
        ...workflowsPaths,
        {
          body: "original-source-verifier\n",
          name: "app/workflows/dist/original-source-verifier.js",
        },
      ],
    ],
    runtime: workflowsRuntime(),
  });
  const exactVerifierInventory = await inspectOciImageArchive(
    exactVerifier,
    "jobs-workflows",
  );
  assert.ok(
    exactVerifierInventory.entries.some(
      (entry) =>
        entry.path === "app/workflows/dist/original-source-verifier.js" &&
        entry.type === "file" &&
        entry.sizeBytes > 0,
    ),
  );

  for (const [label, verifierPath] of [
    ["renamed", "app/workflows/dist/original_source_verifier.js"],
    ["alias", "app/workflows/dist/source-verifier.js"],
  ]) {
    const fixture = join(root, `${label}-source-verifier.oci.tar`);
    await writeOciFixture(fixture, {
      layers: [
        [...workflowsPaths, { body: "source-verifier\n", name: verifierPath }],
      ],
      runtime: workflowsRuntime(),
    });
    await assert.rejects(
      () => inspectOciImageArchive(fixture, "jobs-workflows"),
      /unexpected source-verifier path/,
    );
  }

  const emptyVerifier = join(root, "empty-source-verifier.oci.tar");
  await writeOciFixture(emptyVerifier, {
    layers: [
      [
        ...workflowsPaths,
        { body: "", name: "app/workflows/dist/original-source-verifier.js" },
      ],
    ],
    runtime: workflowsRuntime(),
  });
  await assert.rejects(
    () => inspectOciImageArchive(emptyVerifier, "jobs-workflows"),
    /nonempty regular file/,
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
    layers: [
      [
        ...runnerPaths,
        { body: "headless\n", mode: 0o555, name: headlessExecutable },
        {
          body: "ffmpeg\n",
          mode: 0o555,
          name: "ms-playwright/ffmpeg-1/ffmpeg",
        },
      ],
    ],
    runtime: runnerRuntime(),
  });

  const symlinkedMeasuredRunner = join(
    root,
    "symlinked-measured-runner.oci.tar",
  );
  await writeOciFixture(symlinkedMeasuredRunner, {
    layers: [
      [
        ...runnerPaths,
        { body: "headless\n", mode: 0o555, name: headlessExecutable },
        { body: "shared-runtime\n", name: "opt/shared-runtime.js" },
        {
          linkTarget: "/opt/shared-runtime.js",
          name: "app/automation/dist/linked-runtime.js",
          type: "2",
        },
      ],
    ],
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
  assert.ok(
    runnerInventory.entries.some((entry) => entry.path === headlessExecutable),
  );

  const fullChromiumRunner = join(root, "full-chromium-runner.oci.tar");
  await writeOciFixture(fullChromiumRunner, {
    layers: [
      [
        ...runnerPaths,
        {
          body: "chrome\n",
          mode: 0o555,
          name: "ms-playwright/chromium-1/chrome-linux/chrome",
        },
        { body: "headless\n", mode: 0o555, name: headlessExecutable },
        {
          body: "ffmpeg\n",
          mode: 0o555,
          name: "ms-playwright/ffmpeg-1/ffmpeg",
        },
      ],
    ],
    runtime: runnerRuntime(),
  });
  await assert.rejects(
    () => inspectOciImageArchive(fullChromiumRunner, "jobs-runner"),
    /only Chromium headless shell and ffmpeg/,
  );

  const nonExecutableRunner = join(root, "non-executable-runner.oci.tar");
  await writeOciFixture(nonExecutableRunner, {
    layers: [
      [
        ...runnerPaths,
        { body: "headless\n", mode: 0o444, name: headlessExecutable },
        {
          body: "ffmpeg\n",
          mode: 0o555,
          name: "ms-playwright/ffmpeg-1/ffmpeg",
        },
      ],
    ],
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

async function writeRuntimeConfigFixture(path, componentId, runtime) {
  const entries = MANAGED_CLOUD_RUNTIME_CONTRACTS[componentId].requiredPaths.map(
    (name) => ({
      body: name.endsWith("managed-cloud-runtime-measurement.json")
        ? "{}\n"
        : "synthetic-runtime-only\n",
      mode: name.startsWith("usr/local/bin/") || name.endsWith(".node")
        ? 0o555
        : 0o444,
      name,
    }),
  );
  if (componentId === "jobs-runner") {
    entries.push(
      { body: "automation\n", name: "app/automation/dist/index.js" },
      {
        body: "headless\n",
        mode: 0o555,
        name: "ms-playwright/chromium_headless_shell-1/" +
          "chrome-headless-shell-linux64/chrome-headless-shell",
      },
      { body: "ffmpeg\n", mode: 0o555, name: "ms-playwright/ffmpeg-1/ffmpeg" },
    );
  }
  return writeOciFixture(path, { layers: [entries], runtime });
}

test("OCI exact environment sets admit builder order without changing normalized runtime authority", async (t) => {
  const root = await temporaryDirectory(t, "oci-env-set-positive");
  for (const [componentId, factory] of [
    ["jobs-api", apiRuntime], ["jobs-runner", runnerRuntime], ["jobs-workflows", workflowsRuntime],
  ]) {
    const baseline = factory();
    const variants = [baseline.Env, [...baseline.Env].reverse(), [
      ...baseline.Env.filter((value) => value.startsWith("PATH=")),
      ...baseline.Env.filter((value) => !value.startsWith("PATH=")),
    ]];
    const inspected = [];
    for (const [index, environment] of variants.entries()) {
      const archive = join(root, `${componentId}-${index}.oci.tar`);
      const identity = await writeRuntimeConfigFixture(archive, componentId,
        factory({ Env: environment }));
      const artifact = await inspectOciImageArtifact(archive, componentId);
      assert.equal(artifact.imageConfigSha256, identity.imageConfigSha256);
      assert.deepEqual(artifact.inventory, await inspectOciImageArchive(archive, componentId));
      assert.deepEqual(artifact.inventory.runtime.environment, [...baseline.Env].sort());
      inspected.push(artifact);
    }
    for (const artifact of inspected.slice(1)) {
      assert.deepEqual(artifact.inventory.runtime, inspected[0].inventory.runtime);
    }
    // Exact config bytes remain hash-bound even though effective environment
    // normalization deliberately ignores the serialized array order.
    assert.notEqual(inspected[0].imageConfigSha256, inspected[1].imageConfigSha256);
  }
});

test("OCI reordered environment sets still reject extras duplicates changed values and unsafe entries", async (t) => {
  const root = await temporaryDirectory(t, "oci-env-set-negative");
  for (const [componentId, factory] of [
    ["jobs-api", apiRuntime], ["jobs-runner", runnerRuntime], ["jobs-workflows", workflowsRuntime],
  ]) {
    const environment = [...factory().Env].reverse();
    const cases = [
      ["extra-known", [...environment, "LANG=C"], /not the exact closed set/],
      ["extra-other-component", [...environment, componentId === "jobs-api"
        ? "BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner" : "BLUEY_JOBS_API_PORT=8081"],
      /not the exact closed set/],
      ["duplicate", [...environment, environment[0]], /duplicated, unsafe, or unknown/],
      ["wrong-value", environment.map((value, index) => index === 0 ? value + "wrong" : value),
      /environment is missing/],
      ["missing", environment.slice(1), /environment is missing/],
      ["unknown", [...environment, "PRIVATE_UNKNOWN=PRIVATE_VALUE"], /duplicated, unsafe, or unknown/],
      ["dangerous", [...environment, "NODE_OPTIONS=--inspect"], /duplicated, unsafe, or unknown/],
      ["control", [...environment, "LANG=C\nPRIVATE_VALUE"], /unsafe or oversized/],
      ["oversized", [...environment, "LANG=" + "x".repeat(4096)], /unsafe or oversized/],
      ["object", { Env: environment }, /Env must be an array/],
    ];
    for (const [label, Env, expected] of cases) {
      const archive = join(root, `${componentId}-${label}.oci.tar`);
      await writeRuntimeConfigFixture(archive, componentId, factory({ Env }));
      for (const inspect of [inspectOciImageArtifact, inspectOciImageArchive]) {
        await assert.rejects(() => inspect(archive, componentId), expected);
      }
    }
  }
});

test("OCI Linux command runtimes admit absent or true ArgsEscaped without changing normalized contracts", async (t) => {
  const root = await temporaryDirectory(t, "oci-args-escaped-positive");
  for (const [componentId, factory] of [
    ["jobs-runner", runnerRuntime],
    ["jobs-workflows", workflowsRuntime],
  ]) {
    const baseline = factory();
    assert.equal(Object.hasOwn(baseline, "ArgsEscaped"), false);
    const inspected = [];
    for (const [label, runtime] of [
      ["absent", baseline],
      ["true", factory({ ArgsEscaped: true })],
    ]) {
      assert.deepEqual(Object.keys(runtime).sort(), [
        ...Object.keys(baseline), ...(label === "true" ? ["ArgsEscaped"] : []),
      ].sort());
      assert.equal(Object.hasOwn(runtime, "ArgsEscaped"), label === "true");
      if (label === "true") {
        assert.equal(typeof runtime.ArgsEscaped, "boolean");
        assert.equal(runtime.ArgsEscaped, true);
      }
      assert.deepEqual(runtime.Cmd, MANAGED_CLOUD_RUNTIME_CONTRACTS[componentId].cmd);
      assert.deepEqual(runtime.Entrypoint, MANAGED_CLOUD_RUNTIME_CONTRACTS[componentId].entrypoint);
      const archive = join(root, `${componentId}-${label}.oci.tar`);
      const identity = await writeRuntimeConfigFixture(archive, componentId, runtime);
      const artifact = await inspectOciImageArtifact(archive, componentId);
      const inventory = await inspectOciImageArchive(archive, componentId);
      assert.equal(artifact.imageConfigSha256, identity.imageConfigSha256);
      assert.equal(inventory.artifactSha256, identity.imageManifestSha256);
      assert.deepEqual(artifact.inventory, inventory);
      assert.deepEqual(Object.keys(inventory.runtime).sort(), [
        "cmd", "entrypoint", "environment", "exposedPorts", "user", "workingDirectory",
      ]);
      assert.deepEqual(inventory.runtime.cmd, baseline.Cmd);
      assert.deepEqual(inventory.runtime.entrypoint, baseline.Entrypoint);
      inspected.push(artifact);
    }
    assert.notEqual(inspected[0].imageConfigSha256, inspected[1].imageConfigSha256);
    assert.deepEqual(inspected[0].inventory.runtime, inspected[1].inventory.runtime);
  }
});

test("OCI command runtimes reject non-true ArgsEscaped and command or unknown-key bypasses", async (t) => {
  const root = await temporaryDirectory(t, "oci-args-escaped-negative");
  for (const [componentId, factory] of [
    ["jobs-runner", runnerRuntime],
    ["jobs-workflows", workflowsRuntime],
  ]) {
    for (const [label, value] of [
      ["false", false], ["null", null], ["string", "true"],
      ["number", 1], ["object", {}], ["array", []],
    ]) {
      const archive = join(root, `${componentId}-${label}.oci.tar`);
      await writeRuntimeConfigFixture(archive, componentId, factory({ ArgsEscaped: value }));
      for (const inspect of [inspectOciImageArtifact, inspectOciImageArchive]) {
        await assert.rejects(() => inspect(archive, componentId), /OCI ArgsEscaped is not admitted/);
      }
    }
    for (const [label, overrides, expected] of [
      ["unknown-key", { AttachStdin: true }, /OCI runtime config contains an unknown key/],
      ["cmd", { Cmd: ["/bin/sh", "-c", "id"] }, /OCI runtime user or entrypoint is not canonical/],
      ["entrypoint", { Entrypoint: ["/bin/sh"] }, /OCI runtime user or entrypoint is not canonical/],
    ]) {
      const archive = join(root, `${componentId}-${label}.oci.tar`);
      await writeRuntimeConfigFixture(archive, componentId, factory({ ArgsEscaped: true, ...overrides }));
      for (const inspect of [inspectOciImageArtifact, inspectOciImageArchive]) {
        await assert.rejects(() => inspect(archive, componentId), expected);
      }
    }
  }
});

test("OCI jobs-api rejects every ArgsEscaped presence while its historical absent config stays valid", async (t) => {
  const root = await temporaryDirectory(t, "oci-api-args-escaped");
  const absent = join(root, "api-absent.oci.tar");
  await writeRuntimeConfigFixture(absent, "jobs-api", apiRuntime());
  assert.deepEqual((await inspectOciImageArtifact(absent, "jobs-api")).inventory,
    await inspectOciImageArchive(absent, "jobs-api"));
  for (const [label, value] of [
    ["true", true], ["false", false], ["null", null], ["string", "true"],
    ["number", 1], ["object", {}], ["array", []],
  ]) {
    const archive = join(root, `api-${label}.oci.tar`);
    await writeRuntimeConfigFixture(archive, "jobs-api", apiRuntime({ ArgsEscaped: value }));
    for (const inspect of [inspectOciImageArtifact, inspectOciImageArchive]) {
      await assert.rejects(() => inspect(archive, "jobs-api"), /OCI ArgsEscaped is not admitted/);
    }
  }
});

test("portal manifest transport admits 32768 bytes and leaves no 32769-byte partial", async (t) => {
  const root = await temporaryDirectory(t, "portal-manifest-capture");
  const exactPath = join(root, "exact.json");
  const exactBytes = Buffer.alloc(32 * 1024, 0x20);
  exactBytes.set(Buffer.from("{}\n"));
  assert.deepEqual(
    await captureManagedCloudPortalManifestStream({
      input: Readable.from([
        exactBytes.subarray(0, 17),
        exactBytes.subarray(17),
      ]),
      outputFile: exactPath,
    }),
    { sizeBytes: 32 * 1024 },
  );
  assert.deepEqual(await readFile(exactPath), exactBytes);

  const oversizedPath = join(root, "oversized.json");
  await assert.rejects(
    () =>
      captureManagedCloudPortalManifestStream({
        input: Readable.from([Buffer.alloc(32 * 1024), Buffer.from("x")]),
        outputFile: oversizedPath,
      }),
    /exceeds its 32768-byte limit/,
  );
  await assert.rejects(() => readFile(oversizedPath), /ENOENT/);
  await assert.rejects(() => readFile(oversizedPath + ".partial"), /ENOENT/);
});

test("publication API captures enforce every fixed transport cap atomically", async (t) => {
  const root = await temporaryDirectory(t, "publication-api-capture");
  for (const [kind, maximumBytes] of [
    ["run", 128 * 1024],
    ["jobs", 1024 * 1024],
    ["artifacts", 1024 * 1024],
  ]) {
    const exactPath = join(root, kind + "-exact.json");
    const exact = Buffer.alloc(maximumBytes, 0x20);
    exact.set(Buffer.from("{}\n"));
    assert.deepEqual(
      await captureManagedCloudPublicationApiStream({
        input: Readable.from([exact]),
        kind,
        outputFile: exactPath,
      }),
      { sizeBytes: maximumBytes },
    );
    assert.deepEqual(await readFile(exactPath), exact);
    const oversizedPath = join(root, kind + "-oversized.json");
    await assert.rejects(
      () =>
        captureManagedCloudPublicationApiStream({
          input: Readable.from([Buffer.alloc(maximumBytes), Buffer.from("x")]),
          kind,
          outputFile: oversizedPath,
        }),
      new RegExp(`exceeds its ${maximumBytes}-byte limit`),
    );
    await assert.rejects(() => readFile(oversizedPath), /ENOENT/);
    await assert.rejects(() => readFile(oversizedPath + ".partial"), /ENOENT/);
  }
});

test("failed API producer removes a completed run capture and its partial", async (t) => {
  const root = await temporaryDirectory(t, "failed-api-producer");
  const output = join(root, "run.json");
  const gateScript = join(
    import.meta.dirname,
    "managed-cloud-release-gate.mjs",
  );
  const shell = [
    "set -o pipefail",
    'if ! { printf "{}\\n"; exit 23; } | "$1" "$2" publication-api-capture \\',
    '  --kind run --out "$3"',
    "then",
    '  rm -f -- "$3" "$3.partial"',
    "  exit 1",
    "fi",
    "exit 99",
  ].join("\\n");
  await assert.rejects(
    () =>
      execFileAsync("bash", [
        "-c",
        shell,
        "bluey-capture-test",
        process.execPath,
        gateScript,
        output,
      ]),
    /Command failed/,
  );
  await assert.rejects(() => readFile(output), /ENOENT/);
  await assert.rejects(() => readFile(output + ".partial"), /ENOENT/);
});

test("remote publication readback binds OCI digests, portal bytes, and runner native bytes", async (t) => {
  const root = await temporaryDirectory(t, "publication-readback");
  const readback = join(root, "readback");
  await mkdir(readback);
  const apiPath = join(readback, "jobs-api.oci.tar");
  await writeOciFixture(apiPath, { indexReferenceName: "publication-run-1" });
  const workflowsPath = join(readback, "jobs-workflows.oci.tar");
  await writeOciFixture(workflowsPath, {
    indexReferenceName: "publication-run-1",
    layers: [
      [
        { body: "automation\n", name: "app/automation/dist/index.js" },
        { body: "discovery\n", name: "app/workflows/dist/discovery-worker.js" },
        { body: "failure\n", name: "app/workflows/dist/failure-converter.js" },
        { body: "gateway\n", name: "app/workflows/dist/gateway.js" },
        {
          body: "global-discovery\n",
          name: "app/workflows/dist/global-discovery-worker.js",
        },
        {
          body: "source-verifier\n",
          name: "app/workflows/dist/original-source-verifier.js",
        },
        { body: "worker\n", name: "app/workflows/dist/worker.js" },
        { body: "node\n", mode: 0o555, name: "usr/local/bin/node" },
      ],
    ],
    runtime: workflowsRuntime(),
  });
  const runnerPath = join(readback, "jobs-runner.oci.tar");
  await writeOciFixture(runnerPath, {
    indexReferenceName: "publication-run-1",
    layers: [
      [
        { body: "automation\n", name: "app/automation/dist/index.js" },
        { body: "runner\n", name: "app/runner/dist/server.js" },
        {
          body: "native-addon\n",
          mode: 0o555,
          name: "app/runner/dist/native/bluey_jobs_runner_native_storage.node",
        },
        { body: "node\n", mode: 0o555, name: "usr/local/bin/node" },
        {
          body: "headless\n",
          mode: 0o555,
          name:
            "ms-playwright/chromium_headless_shell-1/" +
            "chrome-headless-shell-linux64/chrome-headless-shell",
        },
        {
          body: "ffmpeg\n",
          mode: 0o555,
          name: "ms-playwright/ffmpeg-1/ffmpeg",
        },
      ],
    ],
    runtime: runnerRuntime(),
  });
  const portalPath = join(readback, "jobs-portal.tar");
  await writeFile(
    portalPath,
    tarArchive([{ body: "<main>Bluey Jobs</main>\n", name: "index.html" }]),
  );
  const paths = new Map([
    ["jobs-api", apiPath],
    ["jobs-portal", portalPath],
    ["jobs-runner", runnerPath],
    ["jobs-workflows", workflowsPath],
  ]);
  const artifacts = [];
  for (const component of MANAGED_CLOUD_COMPONENTS) {
    const path = paths.get(component.componentId);
    const inventory =
      component.artifactKind === "oci_image"
        ? await inspectOciImageArchive(path, component.componentId)
        : await inspectStaticBundleArchive(path, component.componentId);
    const bytes = await readFile(path);
    const repository =
      "ghcr.io/dhanunjay-divi/bluey-jobs-" +
      component.componentId.slice("jobs-".length);
    artifacts.push({
      artifactKind: component.artifactKind,
      artifactSha256: inventory.artifactSha256,
      candidatePayloadPath:
        "payloads/" +
        component.componentId +
        (component.artifactKind === "oci_image" ? ".oci.tar" : ".tar"),
      candidatePayloadSha256: sha256(bytes),
      candidatePayloadSizeBytes: bytes.length,
      componentId: component.componentId,
      signedArtifactRef:
        component.artifactKind === "oci_image"
          ? repository + "@sha256:" + inventory.artifactSha256
          : "https://artifacts.bluey.sh/jobs/releases/" +
            RELEASE_ID +
            "/" +
            inventory.artifactSha256 +
            ".tar",
      publicationRepository: repository,
      publicationTagRef: repository + ":" + RELEASE_ID,
    });
  }
  const portalBytes = await readFile(portalPath);
  const portalArtifact = artifacts.find(
    (artifact) => artifact.componentId === "jobs-portal",
  );
  const portalManifest = portalArtifactManifest(portalBytes);
  const portalManifestBytes = canonicalJsonBytes(portalManifest);
  const portalManifestPath = join(readback, "jobs-portal.oci-manifest.json");
  await writeFile(portalManifestPath, portalManifestBytes);
  const portalRegistryRef =
    portalArtifact.publicationRepository +
    "@sha256:" +
    sha256(portalManifestBytes);
  const plan = {
    adapter: "remote_registry_readback_v1",
    artifacts,
    audience: MANAGED_CLOUD_AUDIENCES.publicationPlan,
    authorizationSetSha256: "a".repeat(64),
    candidateSetSha256: "b".repeat(64),
    manifestSha256: "c".repeat(64),
    nativeArtifact: {},
    portalArtifactOrigin: "https://artifacts.bluey.sh/jobs/releases",
    portalManifestCreatedAt: "2026-08-31T00:00:00Z",
    registryNamespace: "ghcr.io/dhanunjay-divi/bluey-jobs",
    releaseId: RELEASE_ID,
    sourceCommit: SOURCE_COMMIT,
    sourceDefaultBranch: "main",
    sourceRepository: "Dhanunjay-Divi/bluey",
    sourceRepositoryId: 611,
    supplyChainStatus: {},
    version: 1,
  };
  const portalDownload = join(root, "portal-download");
  await mkdir(portalDownload);
  await writeFile(join(portalDownload, "jobs-portal.tar"), portalBytes);
  const stagedPortal = join(root, "staged-portal.tar");
  assert.equal(
    await stageManagedCloudPortalArtifactReadback({
      downloadDirectory: portalDownload,
      manifestFile: portalManifestPath,
      outputFile: stagedPortal,
      plan,
      portalRegistryRef,
    }),
    true,
  );
  assert.deepEqual(await readFile(stagedPortal), portalBytes);
  const records = await inspectManagedCloudArtifactPublicationReadback({
    plan,
    portalRegistryRef,
    readbackDirectory: readback,
  });
  assert.equal(records.length, 4);
  assert.equal(
    records.find((item) => item.componentId === "jobs-runner")
      .publishedManifestDigest,
    "sha256:" +
      artifacts.find((item) => item.componentId === "jobs-runner")
        .artifactSha256,
  );
  const portalRecord = records.find(
    (item) => item.componentId === "jobs-portal",
  );
  assert.match(portalRecord.signedArtifactRef, /^https:\/\//);
  assert.equal(portalRecord.publishedRegistryRef, portalRegistryRef);
  await assert.rejects(
    () =>
      inspectManagedCloudArtifactPublicationReadback({
        plan,
        portalRegistryRef:
          "ghcr.io/other/bluey-jobs-portal@sha256:" +
          portalRegistryRef.slice(portalRegistryRef.indexOf("sha256:") + 7),
        readbackDirectory: readback,
      }),
    /controlled repository/,
  );
  assert.throws(
    () => validateManagedCloudPublicationAdapter("local_fixture"),
    /test-only/,
  );
  for (const [label, mutate, expected] of [
    [
      "extra-layer",
      (manifest) => ({
        ...manifest,
        layers: [...manifest.layers, manifest.layers[0]],
      }),
      /exactly one layer/,
    ],
    [
      "layer-digest",
      (manifest) => ({
        ...manifest,
        layers: [{ ...manifest.layers[0], digest: "sha256:" + "f".repeat(64) }],
      }),
      /exact candidate tar/,
    ],
    [
      "layer-size",
      (manifest) => ({
        ...manifest,
        layers: [{ ...manifest.layers[0], size: manifest.layers[0].size + 1 }],
      }),
      /exact candidate tar/,
    ],
    [
      "layer-media-type",
      (manifest) => ({
        ...manifest,
        layers: [
          { ...manifest.layers[0], mediaType: "application/octet-stream" },
        ],
      }),
      /exact candidate tar/,
    ],
    [
      "layer-title",
      (manifest) => ({
        ...manifest,
        layers: [
          {
            ...manifest.layers[0],
            annotations: { "org.opencontainers.image.title": "other.tar" },
          },
        ],
      }),
      /exact candidate tar/,
    ],
    [
      "artifact-type",
      (manifest) => ({ ...manifest, artifactType: "application/octet-stream" }),
      /manifest or artifact type/,
    ],
    [
      "created",
      (manifest) => ({
        ...manifest,
        annotations: {
          "org.opencontainers.image.created": "2026-08-31T00:00:01Z",
        },
      }),
      /deterministic plan value/,
    ],
  ]) {
    const mutatedBytes = canonicalJsonBytes(
      mutate(structuredClone(portalManifest)),
    );
    await writeFile(portalManifestPath, mutatedBytes);
    const mutatedRef =
      portalArtifact.publicationRepository + "@sha256:" + sha256(mutatedBytes);
    await assert.rejects(
      () =>
        inspectManagedCloudArtifactPublicationReadback({
          plan,
          portalRegistryRef: mutatedRef,
          readbackDirectory: readback,
        }),
      expected,
      label,
    );
  }
  await writeFile(portalManifestPath, portalManifestBytes);
  const planFile = join(root, "publication-plan.json");
  await writeCanonical(planFile, plan);
  const gateScript = join(
    import.meta.dirname,
    "managed-cloud-release-gate.mjs",
  );
  await execFileAsync(process.execPath, [
    gateScript,
    "publication-portal-manifest",
    "--plan",
    planFile,
    "--portal-registry-ref",
    portalRegistryRef,
    "--manifest",
    portalManifestPath,
  ]);
  const oversizedManifest = Buffer.alloc(32 * 1024 + 1, 0x20);
  await writeFile(portalManifestPath, oversizedManifest);
  await assert.rejects(
    () =>
      execFileAsync(process.execPath, [
        gateScript,
        "publication-portal-manifest",
        "--plan",
        planFile,
        "--portal-registry-ref",
        portalArtifact.publicationRepository +
          "@sha256:" +
          sha256(oversizedManifest),
        "--manifest",
        portalManifestPath,
      ]),
    /invalid byte length/,
  );
  await writeFile(portalManifestPath, portalManifestBytes);
  await writeFile(join(portalDownload, "unexpected"), "extra\n");
  await assert.rejects(
    () =>
      stageManagedCloudPortalArtifactReadback({
        downloadDirectory: portalDownload,
        manifestFile: portalManifestPath,
        outputFile: join(root, "extra-output.tar"),
        plan,
        portalRegistryRef,
      }),
    /exact flat regular-file set/,
  );
  await rm(join(portalDownload, "unexpected"));
  await mkdir(join(portalDownload, "unexpected-directory"));
  await assert.rejects(
    () =>
      stageManagedCloudPortalArtifactReadback({
        downloadDirectory: portalDownload,
        manifestFile: portalManifestPath,
        outputFile: join(root, "directory-output.tar"),
        plan,
        portalRegistryRef,
      }),
    /exact flat regular-file set/,
  );
  await rm(join(portalDownload, "unexpected-directory"), { recursive: true });
  await writeFile(join(readback, "unexpected"), "not remote authority\n");
  await assert.rejects(
    () =>
      inspectManagedCloudArtifactPublicationReadback({
        plan,
        portalRegistryRef,
        readbackDirectory: readback,
      }),
    /exact flat regular-file set/,
  );
  await rm(join(readback, "unexpected"));
  await mkdir(join(readback, "unexpected-directory"));
  await assert.rejects(
    () =>
      inspectManagedCloudArtifactPublicationReadback({
        plan,
        portalRegistryRef,
        readbackDirectory: readback,
      }),
    /exact flat regular-file set/,
  );
});

test("publication plan and generated receipt share fail-closed schema keys", () => {
  const plan = {
    adapter: "remote_registry_readback_v1",
    artifacts: [],
    audience: MANAGED_CLOUD_AUDIENCES.publicationPlan,
    authorizationSetSha256: "a".repeat(64),
    candidateSetSha256: "b".repeat(64),
    manifestSha256: "c".repeat(64),
    nativeArtifact: {},
    portalArtifactOrigin: "https://artifacts.bluey.sh/jobs/releases",
    portalManifestCreatedAt: "2026-08-31T00:00:00Z",
    registryNamespace: "ghcr.io/dhanunjay-divi/bluey-jobs",
    releaseId: RELEASE_ID,
    sourceCommit: SOURCE_COMMIT,
    sourceDefaultBranch: "main",
    sourceRepository: "Dhanunjay-Divi/bluey",
    sourceRepositoryId: 611,
    supplyChainStatus: {},
    version: 1,
  };
  assert.equal(validateManagedCloudPublicationPlanShape(plan), true);
  assert.equal(
    validateManagedCloudPublicationRuntimeIdentity({
      defaultBranch: plan.sourceDefaultBranch,
      plan,
      repository: plan.sourceRepository,
      repositoryId: String(plan.sourceRepositoryId),
      sourceCommit: plan.sourceCommit,
    }),
    true,
  );
  for (const mutation of [
    { defaultBranch: "other" },
    { repository: "other/bluey" },
    { repositoryId: "612" },
    { sourceCommit: "f".repeat(40) },
  ]) {
    assert.throws(
      () =>
        validateManagedCloudPublicationRuntimeIdentity({
          defaultBranch: plan.sourceDefaultBranch,
          plan,
          repository: plan.sourceRepository,
          repositoryId: String(plan.sourceRepositoryId),
          sourceCommit: plan.sourceCommit,
          ...mutation,
        }),
      /exact GitHub runtime identity/,
    );
  }
  const slashBranchPlan = {
    ...plan,
    sourceDefaultBranch: "release/stable",
  };
  assert.equal(
    validateManagedCloudPublicationRuntimeIdentity({
      defaultBranch: "release/stable",
      plan: slashBranchPlan,
      repository: slashBranchPlan.sourceRepository,
      repositoryId: String(slashBranchPlan.sourceRepositoryId),
      sourceCommit: slashBranchPlan.sourceCommit,
    }),
    true,
  );
  const publication = createManagedCloudPublicationSetDocument({
    artifacts: [],
    attestedAtMs: Date.now(),
    plan,
    publicationPlanSha256: "d".repeat(64),
    workflowRunAttempt: 1,
    workflowRunId: 2001,
  });
  assert.equal(validateManagedCloudPublicationSetShape(publication), true);
  assert.throws(
    () =>
      validateManagedCloudPublicationSetShape({
        ...publication,
        publishedAtMs: publication.attestedAtMs,
      }),
    /must contain exactly/,
  );
});

test("publication preflight is canonical, bounded, and retry-stable", () => {
  const portalManifestCreatedAt =
    deriveManagedCloudPortalManifestCreatedAt(1_788_134_400);
  assert.equal(portalManifestCreatedAt, "2026-08-31T00:00:00Z");
  assert.equal(
    deriveManagedCloudPortalManifestCreatedAt(1_788_134_400),
    portalManifestCreatedAt,
  );
  const retryPortalBytes = Buffer.from("retry-stable-portal\n");
  const manifestForAttempt = ({ observedAtMs, sourceDateEpoch }) => {
    assert.ok(Number.isSafeInteger(observedAtMs));
    return portalArtifactManifest(retryPortalBytes, {
      annotations: {
        "org.opencontainers.image.created":
          deriveManagedCloudPortalManifestCreatedAt(sourceDateEpoch),
      },
    });
  };
  const firstAttempt = manifestForAttempt({
    observedAtMs: 1_788_134_410_000,
    sourceDateEpoch: 1_788_134_400,
  });
  const retryAttempt = manifestForAttempt({
    observedAtMs: 1_788_220_800_000,
    sourceDateEpoch: 1_788_134_400,
  });
  const changedSourceAttempt = manifestForAttempt({
    observedAtMs: 1_788_220_800_000,
    sourceDateEpoch: 1_788_134_401,
  });
  assert.equal(
    sha256(canonicalJsonBytes(firstAttempt)),
    sha256(canonicalJsonBytes(retryAttempt)),
  );
  assert.notEqual(
    sha256(canonicalJsonBytes(firstAttempt)),
    sha256(canonicalJsonBytes(changedSourceAttempt)),
  );
  const registryNamespace = "ghcr.io/dhanunjay-divi/bluey-jobs";
  const digestCharacters = ["1", "2", "3", "4"];
  const artifacts = MANAGED_CLOUD_COMPONENTS.map((component, index) => {
    const artifactSha256 = digestCharacters[index].repeat(64);
    const publicationRepository =
      registryNamespace + "-" + component.componentId.slice("jobs-".length);
    return {
      artifactKind: component.artifactKind,
      artifactSha256,
      candidatePayloadPath:
        "payloads/" +
        component.componentId +
        (component.artifactKind === "oci_image" ? ".oci.tar" : ".tar"),
      candidatePayloadSha256: String(index + 5).repeat(64),
      candidatePayloadSizeBytes: 10_000 + index,
      componentId: component.componentId,
      publicationRepository,
      publicationTagRef: publicationRepository + ":" + RELEASE_ID,
      signedArtifactRef:
        component.artifactKind === "oci_image"
          ? publicationRepository + "@sha256:" + artifactSha256
          : "https://artifacts.bluey.sh/jobs/releases/" +
            RELEASE_ID +
            "/" +
            artifactSha256 +
            ".tar",
    };
  });
  const plan = {
    adapter: "remote_registry_readback_v1",
    artifacts,
    audience: MANAGED_CLOUD_AUDIENCES.publicationPlan,
    authorizationSetSha256: "a".repeat(64),
    candidateSetSha256: "b".repeat(64),
    manifestSha256: "c".repeat(64),
    nativeArtifact: {},
    portalArtifactOrigin: "https://artifacts.bluey.sh/jobs/releases",
    portalManifestCreatedAt,
    registryNamespace,
    releaseId: RELEASE_ID,
    sourceCommit: SOURCE_COMMIT,
    sourceDefaultBranch: "release/stable",
    sourceRepository: "Dhanunjay-Divi/bluey",
    sourceRepositoryId: 611,
    supplyChainStatus: {},
    version: 1,
  };
  const publicationArtifacts = artifacts.map((artifact, index) => {
    const publishedManifestDigest =
      artifact.artifactKind === "oci_image"
        ? "sha256:" + artifact.artifactSha256
        : "sha256:" + ["9", "a", "b", "c"][index].repeat(64);
    return {
      componentId: artifact.componentId,
      publishedManifestDigest,
      publishedRegistryRef:
        artifact.publicationRepository + "@" + publishedManifestDigest,
    };
  });
  const publication = createManagedCloudPublicationSetDocument({
    artifacts: publicationArtifacts,
    attestedAtMs: Date.now(),
    plan,
    publicationPlanSha256: "d".repeat(64),
    workflowRunAttempt: 1,
    workflowRunId: 2001,
  });
  const run = {
    conclusion: "success",
    event: "workflow_dispatch",
    head_branch: "release/stable",
    head_sha: SOURCE_COMMIT,
    id: 2001,
    path: ".github/workflows/jobs-managed-cloud-release.yml@release/stable",
    repository: {
      full_name: plan.sourceRepository,
      id: plan.sourceRepositoryId,
    },
    run_attempt: 1,
    status: "completed",
  };
  const jobs = {
    jobs: [
      {
        conclusion: "success",
        head_sha: SOURCE_COMMIT,
        id: 3001,
        name: "publish",
        run_attempt: 1,
        run_id: 2001,
        status: "completed",
        steps: [
          {
            conclusion: "success",
            name: "Upload the remote publication receipt only",
            status: "completed",
          },
        ],
      },
    ],
    total_count: 1,
  };
  const workflowEvidence = createManagedCloudPublicationWorkflowEvidence({
    artifacts: publicationWorkflowArtifacts({
      branch: "release/stable",
      repositoryId: plan.sourceRepositoryId,
    }),
    defaultBranch: "release/stable",
    jobs,
    plan,
    publication,
    run,
  });
  const preflight = createManagedCloudPublicationPreflightDocument({
    plan,
    publication,
    publicationPlanSha256: "d".repeat(64),
    publicationSetSha256: "e".repeat(64),
    workflowEvidence,
  });
  assert.deepEqual(
    createManagedCloudPublicationPreflightDocument({
      plan,
      publication,
      publicationPlanSha256: "d".repeat(64),
      publicationSetSha256: "e".repeat(64),
      workflowEvidence,
    }),
    preflight,
  );
  assert.equal(
    validateManagedCloudPublicationPreflightDocument(preflight).defaultBranch,
    "release/stable",
  );
  const wrongRef = structuredClone(preflight);
  wrongRef.artifacts[0].publishedRegistryRef =
    "ghcr.io/other/jobs-api@" + wrongRef.artifacts[0].publishedManifestDigest;
  assert.throws(
    () => validateManagedCloudPublicationPreflightDocument(wrongRef),
    /outside its exact authority/,
  );
  const wrongRun = structuredClone(preflight);
  wrongRun.workflowEvidence.runId = 2002;
  wrongRun.workflowEvidence.artifactName =
    "jobs-managed-cloud-publication-2002-1";
  assert.throws(
    () =>
      validateManagedCloudPublicationPreflightDocument(wrongRun, {
        plan,
        publication,
      }),
    /exact publication set/,
  );
  const oversized = structuredClone(preflight);
  oversized.repository = "R".repeat(128 * 1024);
  oversized.workflowEvidence.repository = oversized.repository;
  assert.throws(
    () => validateManagedCloudPublicationPreflightDocument(oversized),
    /exceeds the 128 KiB canonical limit/,
  );
});

test("release-v1 assembly rejects a three-role workflows verifier image", async (t) => {
  const root = await temporaryDirectory(t, "v1-verifier-assembly");
  const repoRoot = join(import.meta.dirname, "../..");
  const contracts = join(root, "contracts");
  await createManagedCloudContracts(repoRoot, contracts, 1);
  const buildId = "build-jobs-workflows-v1-verifier";
  const sourceDateEpoch = 1_786_700_000;
  const contractSha256 = async (name) =>
    sha256(await readFile(join(contracts, name)));
  const measurement = {
    audience: MANAGED_CLOUD_AUDIENCES.runtimeMeasurement,
    buildId,
    componentId: "jobs-workflows",
    configSchemaSha256: await contractSha256("config-contract.json"),
    measuredFiles: [],
    migrationSetSha256: await contractSha256("migration-contract.json"),
    protocolSetSha256: await contractSha256("protocol-contract.json"),
    roles: ["original_source_verifier", "workflow_gateway", "workflow_worker"],
    sourceCommit: SOURCE_COMMIT,
    version: 1,
  };
  const artifact = join(root, "jobs-workflows.oci.tar");
  await writeOciFixture(artifact, {
    layers: [
      [
        {
          body: canonicalJsonBytes(measurement),
          name: "app/.bluey/managed-cloud-runtime-measurement.json",
        },
        { body: "discovery\n", name: "app/workflows/dist/discovery-worker.js" },
        { body: "failure\n", name: "app/workflows/dist/failure-converter.js" },
        { body: "gateway\n", name: "app/workflows/dist/gateway.js" },
        {
          body: "global-discovery\n",
          name: "app/workflows/dist/global-discovery-worker.js",
        },
        {
          body: "original-source-verifier\n",
          name: "app/workflows/dist/original-source-verifier.js",
        },
        { body: "worker\n", name: "app/workflows/dist/worker.js" },
        { body: "node\n", mode: 0o555, name: "usr/local/bin/node" },
      ],
    ],
    runtime: workflowsRuntime(),
  });
  const inventory = await inspectOciImageArchive(artifact, "jobs-workflows");
  const inventoryFile = join(root, "jobs-workflows.inventory.json");
  const sbomFile = join(root, "jobs-workflows.sbom.json");
  const provenanceFile = join(root, "jobs-workflows.provenance.json");
  await writeCanonical(inventoryFile, inventory);
  await writeCanonical(sbomFile, createFileSbom(inventory));
  await writeCanonical(
    provenanceFile,
    createArtifactProvenance({
      artifactSha256: inventory.artifactSha256,
      buildId,
      builderPolicySha256: await contractSha256("builder-policy.json"),
      componentId: "jobs-workflows",
      sourceCommit: SOURCE_COMMIT,
      sourceDateEpoch,
    }),
  );
  const build = {
    architecture: "x86_64",
    artifactKind: "oci_image",
    artifactRef:
      "registry.example/bluey/jobs-workflows@sha256:" +
      inventory.artifactSha256,
    buildId,
    candidateFile: artifact,
    componentId: "jobs-workflows",
    contentInventoryFile: inventoryFile,
    platform: "linux",
    provenanceFile,
    sbomFile,
  };
  const descriptorFile = join(root, "release-descriptor-v1.json");
  await writeCanonical(descriptorFile, {
    audience: "bluey-jobs-managed-cloud-release-descriptor-v1",
    builds: [build, build, build, build],
    featureAuthority: {
      cloudDistribution: true,
      directDiscovery: false,
      globalDiscovery: false,
      sourceVerification: false,
      workflowCleanup: true,
      workflowCommandDispatch: true,
    },
    manifestGeneration: 611,
    manifestId: "manifest-v1-verifier-regression",
    publishedAtMs: 1_786_700_000_000,
    releaseId: RELEASE_ID,
    releaseSequence: 611001,
    sourceCommit: SOURCE_COMMIT,
    sourceDateEpoch,
    testEvidenceFile: join(root, "unused-test-evidence.json"),
    version: 1,
  });
  await assert.rejects(
    () =>
      assembleManagedCloudCandidate({
        contractsDirectory: contracts,
        descriptorFile,
        outputDirectory: join(root, "candidate"),
        repoRoot,
      }),
    /source-verifier entrypoint does not match release authority/,
  );
});

test("release-v2 activation evidence requires verifier readiness and exact readback", async (t) => {
  const root = await temporaryDirectory(t, "activation-evidence");
  const evidenceRoot = join(root, "evidence");
  await mkdir(evidenceRoot);
  const manifest = successorReleaseFixture();
  const manifestSha256 = sha256(canonicalJsonBytes(manifest));
  const nowMs = Date.now();
  const scope = {
    channel: "shadow",
    environment: "staging",
    region: "us-east-1",
  };
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
    entries: [
      {
        path: "app/workflows/dist/failure-converter.js",
        sha256: converterSha256,
        type: "file",
      },
    ],
  });
  const portal = manifest.components.find(
    (item) => item.componentId === "jobs-portal",
  );
  const taskQueueSha256 = deriveManagedCloudTaskQueueSha256(
    "bluey-prod",
    "bluey-jobs-applications",
  );
  const documents = {
    "canary.json": {
      ...common,
      audience: MANAGED_CLOUD_AUDIENCES.canaryEvidence,
      checks: deriveManagedCloudCanaryCheckIds(manifest.featureAuthority).map(
        (checkId) => ({
          checkId,
          evidenceSha256: "1".repeat(64),
          status: "pass",
        }),
      ),
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
      artifactRef: portal.artifactRef,
      audience: MANAGED_CLOUD_AUDIENCES.portalReadbackEvidenceV2,
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
    checks: documents["canary.json"].checks.filter(
      (check) => check.checkId !== "original-source-verifier-readiness",
    ),
  });
  await assert.rejects(
    () => validateManagedCloudActivationEvidence(args),
    /at least 15 entries/,
  );
  await writeCanonical(
    join(evidenceRoot, "canary.json"),
    documents["canary.json"],
  );

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
  await writeCanonical(join(evidenceRoot, "portal-readback.json"), {
    ...documents["portal-readback.json"],
    artifactRef: documents["portal-readback.json"].artifactRef + ".wrong",
  });
  await assert.rejects(
    () => validateManagedCloudActivationEvidence(args),
    /signed portal URL and artifact/,
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

test("promotion and rollback authority preserves v1 readback and emits only v2", async (t) => {
  const root = await temporaryDirectory(t, "promotion-generations");
  const trust = trustFixture();
  const from = await createRemotePromotionFixture({
    activationGeneration: 1,
    channelSequence: 1,
    expectedHeadRevision: 0,
    expectedTransitionSha256: null,
    label: "remote-from",
    predecessorActivationSha256: null,
    releaseSequence: 200,
    root,
    trust,
  });
  assert.equal(from.promotionSet.version, 2);
  assert.equal(from.promotionSet.audience, MANAGED_CLOUD_AUDIENCES.promotionV2);
  assert.equal(
    (
      await validateManagedCloudPromotion(
        from.promotion,
        trust.rootAnchorSha256,
      )
    ).promotionGeneration,
    2,
  );
  const historicalFrom = join(root, "historical-from");
  await createHistoricalPromotionFixture(from, historicalFrom);
  assert.equal(
    (
      await validateManagedCloudPromotion(
        historicalFrom,
        trust.rootAnchorSha256,
      )
    ).promotionGeneration,
    1,
  );

  const to = await createRemotePromotionFixture({
    activationGeneration: 2,
    channelSequence: 2,
    expectedHeadRevision: 1,
    expectedTransitionSha256: "7".repeat(64),
    label: "remote-target",
    predecessorActivationSha256: await fileSha256(from.activationFile),
    releaseSequence: 100,
    root,
    trust,
  });
  const historicalTo = join(root, "historical-target");
  await createHistoricalPromotionFixture(to, historicalTo);
  const rollbackDocument = (version, audience, rollbackId) => ({
    audience,
    evidenceSha256: "8".repeat(64),
    expectedHeadRevision: 1,
    expectedTransitionSha256: "7".repeat(64),
    fromActivationSha256: from.promotionSet.activationSha256,
    fromManifestSha256: from.promotionSet.manifestSha256,
    issuedAtMs: Date.now(),
    reasonRef: "test-only-release-regression",
    rollbackGeneration: 1,
    rollbackId,
    scope: { channel: "shadow", environment: "staging", region: "us-east-1" },
    toActivationSha256: to.promotionSet.activationSha256,
    toManifestSha256: to.promotionSet.manifestSha256,
    trustGeneration: 1,
    version,
  });
  const rollbackV2File = join(root, "rollback-v2.json");
  const rollbackV2 = rollbackDocument(
    2,
    MANAGED_CLOUD_AUDIENCES.rollbackV2,
    "rollback-v2-test",
  );
  await writeCanonical(rollbackV2File, rollbackV2);
  const rollbackV2Signatures = join(root, "rollback-v2-signatures.json");
  await writeCanonical(
    rollbackV2Signatures,
    signedSet({
      id: "rollback-v2-signatures",
      keys: [{ keyId: "promotion-key", pair: trust.pairs.promotion }],
      role: "promotion",
      signedAtMs: rollbackV2.issuedAtMs,
      targetAudience: MANAGED_CLOUD_AUDIENCES.rollbackV2,
      targetBytes: await readFile(rollbackV2File),
    }),
  );
  const rollbackV2Root = join(root, "rollback-v2");
  const emittedRollback = await createManagedCloudRollback({
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    fromPromotionDirectory: from.promotion,
    outputDirectory: rollbackV2Root,
    rollbackFile: rollbackV2File,
    rollbackSignatureSetFile: rollbackV2Signatures,
    toPromotionDirectory: to.promotion,
  });
  assert.equal(emittedRollback.version, 2);
  assert.equal(emittedRollback.audience, MANAGED_CLOUD_AUDIENCES.rollbackSetV2);
  assert.equal(
    (await validateManagedCloudRollback(rollbackV2Root, trust.rootAnchorSha256))
      .rollbackSet.version,
    2,
  );
  await assert.rejects(
    () =>
      createManagedCloudRollback({
        expectedRootAnchorSha256: trust.rootAnchorSha256,
        fromPromotionDirectory: historicalFrom,
        outputDirectory: join(root, "forbidden-v1-create"),
        rollbackFile: rollbackV2File,
        rollbackSignatureSetFile: rollbackV2Signatures,
        toPromotionDirectory: historicalTo,
      }),
    /requires two remote promotion generation-v2 inputs/,
  );

  const rollbackV1 = rollbackDocument(
    1,
    MANAGED_CLOUD_AUDIENCES.rollback,
    "rollback-v1-historical",
  );
  const rollbackV1Root = join(root, "rollback-v1-historical");
  await mkdir(rollbackV1Root);
  await cp(historicalFrom, join(rollbackV1Root, "from-promotion"), {
    recursive: true,
  });
  await cp(historicalTo, join(rollbackV1Root, "target-promotion"), {
    recursive: true,
  });
  const rollbackV1File = join(rollbackV1Root, "rollback.json");
  await writeCanonical(rollbackV1File, rollbackV1);
  const rollbackV1Signatures = join(rollbackV1Root, "rollback-signatures.json");
  await writeCanonical(
    rollbackV1Signatures,
    signedSet({
      id: "rollback-v1-signatures",
      keys: [{ keyId: "promotion-key", pair: trust.pairs.promotion }],
      role: "promotion",
      signedAtMs: rollbackV1.issuedAtMs,
      targetAudience: MANAGED_CLOUD_AUDIENCES.rollback,
      targetBytes: await readFile(rollbackV1File),
    }),
  );
  await writeCanonical(join(rollbackV1Root, "rollback-set.json"), {
    audience: MANAGED_CLOUD_AUDIENCES.rollbackSet,
    fromPromotionSha256: await fileSha256(
      join(rollbackV1Root, "from-promotion/promotion-set.json"),
    ),
    rollbackSha256: await fileSha256(rollbackV1File),
    rollbackSignatureSetSha256: await fileSha256(rollbackV1Signatures),
    signerKeyIds: ["promotion-key"],
    targetPromotionSha256: await fileSha256(
      join(rollbackV1Root, "target-promotion/promotion-set.json"),
    ),
    version: 1,
  });
  assert.equal(
    (await validateManagedCloudRollback(rollbackV1Root, trust.rootAnchorSha256))
      .rollbackSet.version,
    1,
  );

  const mixedRollback = join(root, "mixed-rollback");
  await cp(rollbackV1Root, mixedRollback, { recursive: true });
  await rm(join(mixedRollback, "target-promotion"), { recursive: true });
  await cp(to.promotion, join(mixedRollback, "target-promotion"), {
    recursive: true,
  });
  await assert.rejects(
    () => validateManagedCloudRollback(mixedRollback, trust.rootAnchorSha256),
    /cannot mix historical and remote promotion generations/,
  );

  const crossedV1 = join(root, "crossed-v1-tree");
  await cp(from.promotion, crossedV1, { recursive: true });
  await rm(join(crossedV1, "promotion-set.json"));
  await copyFile(
    join(historicalFrom, "promotion-set.json"),
    join(crossedV1, "promotion-set.json"),
  );
  await assert.rejects(
    () => validateManagedCloudPromotion(crossedV1, trust.rootAnchorSha256),
    /unexpected or missing top-level entry/,
  );
  const crossedV2 = join(root, "crossed-v2-tree");
  await cp(historicalFrom, crossedV2, { recursive: true });
  await rm(join(crossedV2, "promotion-set.json"));
  await copyFile(
    join(from.promotion, "promotion-set.json"),
    join(crossedV2, "promotion-set.json"),
  );
  await assert.rejects(
    () => validateManagedCloudPromotion(crossedV2, trust.rootAnchorSha256),
    /unexpected or missing top-level entry/,
  );
});

test("release-v3 publication, promotion, and rollback retain generation-v2 schemas", async (t) => {
  const root = await temporaryDirectory(t, "release-v3-publication");
  const trust = trustFixture();
  const from = await createRemotePromotionFixture({
    activationGeneration: 3,
    channelSequence: 3,
    contractVersion: 3,
    expectedHeadRevision: 0,
    expectedTransitionSha256: null,
    label: "release-v3-from",
    predecessorActivationSha256: null,
    releaseSequence: 300,
    root,
    trust,
  });
  const validatedFrom = await validateManagedCloudPromotion(
    from.promotion,
    trust.rootAnchorSha256,
  );
  assert.equal(validatedFrom.authorized.manifest.version, 3);
  assert.equal(
    validatedFrom.authorized.manifest.audience,
    MANAGED_CLOUD_AUDIENCES.manifestV3,
  );
  assert.equal(validatedFrom.activation.version, 3);
  assert.equal(
    validatedFrom.activation.audience,
    MANAGED_CLOUD_AUDIENCES.activationV3,
  );
  assert.equal(validatedFrom.promotion.version, 2);
  assert.equal(
    validatedFrom.promotion.audience,
    MANAGED_CLOUD_AUDIENCES.promotionV2,
  );
  assert.equal(validatedFrom.promotionGeneration, 2);
  const builderPolicy = JSON.parse(
    await readFile(
      join(from.authorized, "candidate/builder-policy.json"),
      "utf8",
    ),
  );
  const testEvidence = JSON.parse(
    await readFile(
      join(from.authorized, "candidate/test-evidence.json"),
      "utf8",
    ),
  );
  const publicationPlan = JSON.parse(
    await readFile(
      join(from.fixtureRoot, "publication/publication-plan.json"),
      "utf8",
    ),
  );
  const publicationSet = JSON.parse(
    await readFile(
      join(from.fixtureRoot, "publication/publication-set.json"),
      "utf8",
    ),
  );
  const portalReadback = JSON.parse(
    await readFile(
      join(from.fixtureRoot, "activation-evidence/portal-readback.json"),
      "utf8",
    ),
  );
  assert.equal(builderPolicy.schemaVersion, 2);
  assert.equal(builderPolicy.audience, MANAGED_CLOUD_AUDIENCES.builderPolicyV2);
  assert.equal(testEvidence.version, 2);
  assert.equal(testEvidence.audience, MANAGED_CLOUD_AUDIENCES.testEvidenceV2);
  assert.equal(publicationPlan.version, 1);
  assert.equal(
    publicationPlan.audience,
    MANAGED_CLOUD_AUDIENCES.publicationPlan,
  );
  assert.equal(publicationSet.version, 1);
  assert.equal(publicationSet.audience, MANAGED_CLOUD_AUDIENCES.publicationSet);
  assert.equal(portalReadback.version, 1);
  assert.equal(
    portalReadback.audience,
    MANAGED_CLOUD_AUDIENCES.portalReadbackEvidenceV2,
  );

  const to = await createRemotePromotionFixture({
    activationGeneration: 4,
    channelSequence: 4,
    contractVersion: 3,
    expectedHeadRevision: 3,
    expectedTransitionSha256: "9".repeat(64),
    label: "release-v3-target",
    predecessorActivationSha256: await fileSha256(from.activationFile),
    releaseSequence: 250,
    root,
    trust,
  });
  const rollbackFile = join(root, "release-v3-rollback.json");
  const rollback = {
    audience: MANAGED_CLOUD_AUDIENCES.rollbackV2,
    evidenceSha256: "8".repeat(64),
    expectedHeadRevision: 3,
    expectedTransitionSha256: "9".repeat(64),
    fromActivationSha256: from.promotionSet.activationSha256,
    fromManifestSha256: from.promotionSet.manifestSha256,
    issuedAtMs: Date.now(),
    reasonRef: "test-only-release-v3-regression",
    rollbackGeneration: 2,
    rollbackId: "rollback-release-v3",
    scope: { channel: "shadow", environment: "staging", region: "us-east-1" },
    toActivationSha256: to.promotionSet.activationSha256,
    toManifestSha256: to.promotionSet.manifestSha256,
    trustGeneration: 1,
    version: 2,
  };
  await writeCanonical(rollbackFile, rollback);
  const signaturesFile = join(root, "release-v3-rollback-signatures.json");
  await writeCanonical(
    signaturesFile,
    signedSet({
      id: "release-v3-rollback-signatures",
      keys: [{ keyId: "promotion-key", pair: trust.pairs.promotion }],
      role: "promotion",
      signedAtMs: rollback.issuedAtMs,
      targetAudience: MANAGED_CLOUD_AUDIENCES.rollbackV2,
      targetBytes: await readFile(rollbackFile),
    }),
  );
  const rollbackRoot = join(root, "release-v3-rollback");
  const rollbackSet = await createManagedCloudRollback({
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    fromPromotionDirectory: from.promotion,
    outputDirectory: rollbackRoot,
    rollbackFile,
    rollbackSignatureSetFile: signaturesFile,
    toPromotionDirectory: to.promotion,
  });
  assert.equal(rollbackSet.version, 2);
  assert.equal(rollbackSet.audience, MANAGED_CLOUD_AUDIENCES.rollbackSetV2);
  assert.equal(
    (await validateManagedCloudRollback(rollbackRoot, trust.rootAnchorSha256))
      .rollbackSet.version,
    2,
  );
});

test("release-v4 descriptor and activation use v4 domains with v2 downstream schemas", async (t) => {
  const root = await temporaryDirectory(t, "release-v4-domains");
  const trust = trustFixture();
  const fixture = await createRemotePromotionFixture({
    activationGeneration: 4,
    channelSequence: 4,
    contractVersion: 4,
    expectedHeadRevision: 0,
    expectedTransitionSha256: null,
    label: "release-v4-domains",
    predecessorActivationSha256: null,
    releaseSequence: 400,
    root,
    trust,
  });
  const descriptor = JSON.parse(
    await readFile(join(fixture.fixtureRoot, "release-descriptor.json"), "utf8"),
  );
  assert.equal(
    descriptor.audience,
    "bluey-jobs-managed-cloud-release-descriptor-v4",
  );
  assert.equal(descriptor.version, 4);

  const validated = await validateManagedCloudPromotion(
    fixture.promotion,
    trust.rootAnchorSha256,
  );
  assert.equal(validated.authorized.manifest.version, 4);
  assert.equal(
    validated.authorized.manifest.audience,
    MANAGED_CLOUD_AUDIENCES.manifestV4,
  );
  assert.equal(validated.activation.version, 4);
  assert.equal(
    validated.activation.audience,
    MANAGED_CLOUD_AUDIENCES.activationV4,
  );
  assert.equal(validated.promotion.version, 2);
  assert.equal(validated.promotion.audience, MANAGED_CLOUD_AUDIENCES.promotionV2);
  const builderPolicy = JSON.parse(
    await readFile(
      join(fixture.authorized, "candidate/builder-policy.json"),
      "utf8",
    ),
  );
  const testEvidence = JSON.parse(
    await readFile(
      join(fixture.authorized, "candidate/test-evidence.json"),
      "utf8",
    ),
  );
  assert.equal(builderPolicy.schemaVersion, 3);
  assert.equal(builderPolicy.audience, MANAGED_CLOUD_AUDIENCES.builderPolicyV3);
  assert.equal(testEvidence.version, 2);
  assert.equal(testEvidence.audience, MANAGED_CLOUD_AUDIENCES.testEvidenceV2);

  const target = await createRemotePromotionFixture({
    activationGeneration: 5,
    channelSequence: 5,
    contractVersion: 4,
    expectedHeadRevision: 4,
    expectedTransitionSha256: "9".repeat(64),
    label: "release-v4-rollback-target",
    predecessorActivationSha256: await fileSha256(fixture.activationFile),
    releaseSequence: 390,
    root,
    trust,
  });
  const rollbackFile = join(root, "release-v4-rollback.json");
  const rollback = {
    audience: MANAGED_CLOUD_AUDIENCES.rollbackV2,
    evidenceSha256: "8".repeat(64),
    expectedHeadRevision: 4,
    expectedTransitionSha256: "9".repeat(64),
    fromActivationSha256: fixture.promotionSet.activationSha256,
    fromManifestSha256: fixture.promotionSet.manifestSha256,
    issuedAtMs: Date.now(),
    reasonRef: "test-only-release-v4-regression",
    rollbackGeneration: 2,
    rollbackId: "rollback-release-v4",
    scope: { channel: "shadow", environment: "staging", region: "us-east-1" },
    toActivationSha256: target.promotionSet.activationSha256,
    toManifestSha256: target.promotionSet.manifestSha256,
    trustGeneration: 1,
    version: 2,
  };
  await writeCanonical(rollbackFile, rollback);
  const signaturesFile = join(root, "release-v4-rollback-signatures.json");
  await writeCanonical(
    signaturesFile,
    signedSet({
      id: "release-v4-rollback-signatures",
      keys: [{ keyId: "promotion-key", pair: trust.pairs.promotion }],
      role: "promotion",
      signedAtMs: rollback.issuedAtMs,
      targetAudience: MANAGED_CLOUD_AUDIENCES.rollbackV2,
      targetBytes: await readFile(rollbackFile),
    }),
  );
  const rollbackRoot = join(root, "release-v4-rollback");
  const rollbackSet = await createManagedCloudRollback({
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    fromPromotionDirectory: fixture.promotion,
    outputDirectory: rollbackRoot,
    rollbackFile,
    rollbackSignatureSetFile: signaturesFile,
    toPromotionDirectory: target.promotion,
  });
  assert.equal(rollbackSet.version, 2);
  assert.equal(rollbackSet.audience, MANAGED_CLOUD_AUDIENCES.rollbackSetV2);
  assert.equal(
    (await validateManagedCloudRollback(rollbackRoot, trust.rootAnchorSha256))
      .rollbackSet.version,
    2,
  );
});

test("remote rollback refuses a separately signed v3 target after a v4 release", async (t) => {
  const root = await temporaryDirectory(t, "v4-v3-rollback-refusal");
  const trust = trustFixture();
  const from = await createRemotePromotionFixture({
    root, trust, label: "current-v4", contractVersion: 4, releaseSequence: 400,
    activationGeneration: 4, channelSequence: 4, expectedHeadRevision: 0,
    expectedTransitionSha256: null, predecessorActivationSha256: null,
  });
  const to = await createRemotePromotionFixture({
    root, trust, label: "older-v3", contractVersion: 3, releaseSequence: 390,
    activationGeneration: 5, channelSequence: 5, expectedHeadRevision: 4,
    expectedTransitionSha256: "9".repeat(64),
    predecessorActivationSha256: from.promotionSet.activationSha256,
  });
  // Each promotion verifies on its own. It is their pairing that is invalid.
  for (const [item, version] of [[from, 4], [to, 3]]) {
    const checked = await validateManagedCloudPromotion(item.promotion, trust.rootAnchorSha256);
    assert.equal(checked.authorized.manifest.version, version);
  }
  const request = {
    audience: MANAGED_CLOUD_AUDIENCES.rollbackV2,
    evidenceSha256: "8".repeat(64),
    expectedHeadRevision: 4,
    expectedTransitionSha256: "9".repeat(64),
    fromActivationSha256: from.promotionSet.activationSha256,
    fromManifestSha256: from.promotionSet.manifestSha256,
    issuedAtMs: Date.now(),
    reasonRef: "test-only-cross-migration-refusal",
    rollbackGeneration: 2,
    rollbackId: "rollback-incompatible-migrations",
    scope: { channel: "shadow", environment: "staging", region: "us-east-1" },
    toActivationSha256: to.promotionSet.activationSha256,
    toManifestSha256: to.promotionSet.manifestSha256,
    trustGeneration: 1,
    version: 2,
  };
  const rollbackFile = join(root, "rollback.json");
  const signaturesFile = join(root, "rollback-signatures.json");
  await writeCanonical(rollbackFile, request);
  await writeCanonical(signaturesFile, signedSet({
    id: "rollback-incompatible-migrations-signed",
    keys: [{ keyId: "promotion-key", pair: trust.pairs.promotion }],
    role: "promotion",
    signedAtMs: request.issuedAtMs,
    targetAudience: MANAGED_CLOUD_AUDIENCES.rollbackV2,
    targetBytes: await readFile(rollbackFile),
  }));
  const outputDirectory = join(root, "rejected-rollback");
  await assert.rejects(() => createManagedCloudRollback({
    expectedRootAnchorSha256: trust.rootAnchorSha256,
    fromPromotionDirectory: from.promotion,
    toPromotionDirectory: to.promotion,
    outputDirectory, rollbackFile, rollbackSignatureSetFile: signaturesFile,
  }), /same signed migration contract/);
  assert.deepEqual(await readdir(outputDirectory), []);
});

test("rollback uses a monotonic successor activation over older verified bytes", () => {
  const nowMs = Date.now();
  const scope = {
    channel: "canary",
    environment: "production",
    region: "us-east-1",
  };
  const currentPolicy = { trustGeneration: 2, policyId: "current" };
  const from = {
    activation: { activationGeneration: 10, channelSequence: 20, scope },
    authorized: { manifest: { releaseSequence: 200 }, policy: currentPolicy },
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
      policy: currentPolicy,
      trustBundle: { policies: [{}, { policy: currentPolicy }] },
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
    () =>
      validateManagedCloudRollbackSuccessor(rollback, from, {
        ...to,
        activation: {
          ...to.activation,
          predecessorActivationSha256: "6".repeat(64),
        },
      }),
    /successor activation over older release bytes/,
  );
  assert.throws(
    () =>
      validateManagedCloudRollbackSuccessor(rollback, from, {
        ...to,
        authorized: {
          ...to.authorized,
          manifest: { releaseSequence: 201 },
        },
      }),
    /successor activation over older release bytes/,
  );
  const rotatedPolicy = { trustGeneration: 3, policyId: "rotated" };
  const successor = (policy, ...policies) => ({
    ...to,
    authorized: {
      ...to.authorized,
      policy,
      trustBundle: { policies: policies.map((entry) => ({ policy: entry })) },
    },
  });
  const olderPolicy = { trustGeneration: 1, policyId: "older" };
  const fork = { ...currentPolicy, policyId: "fork" };
  assert.deepEqual(
    validateManagedCloudRollbackSuccessor(
      { ...rollback, trustGeneration: 3 }, from, successor(rotatedPolicy, olderPolicy, currentPolicy, rotatedPolicy),
    ),
    scope,
  );
  assert.throws(
    () => validateManagedCloudRollbackSuccessor(
      { ...rollback, trustGeneration: 1 }, from, successor(olderPolicy, olderPolicy),
    ),
    /rollback trust lineage/i,
  );
  assert.throws(
    () => validateManagedCloudRollbackSuccessor(rollback, from, successor(fork, olderPolicy, fork)),
    /rollback trust lineage/i,
  );
  assert.throws(
    () => validateManagedCloudRollbackSuccessor(
      { ...rollback, trustGeneration: 3 }, from, successor(rotatedPolicy, olderPolicy, fork, rotatedPolicy),
    ),
    /rollback trust lineage/i,
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
    () =>
      validateManagedCloudActivationWindow(
        activation,
        { ...cohort, expiresAtMs: activation.expiresAtMs - 1 },
        policy,
      ),
    /validity window/,
  );
  assert.throws(
    () =>
      validateManagedCloudActivationWindow(activation, cohort, {
        ...policy,
        expiresAtMs: activation.expiresAtMs - 1,
      }),
    /validity window/,
  );
  assert.throws(
    () =>
      validateManagedCloudActivationWindow(
        { ...activation, portalReadbackTtlMs: 32_999 },
        cohort,
        policy,
      ),
    /validity window/,
  );
});

test("release-v2 CI evidence requires two successful exact-tip default-branch runs", () => {
  const repository = "Dhanunjay-Divi/bluey";
  const repositoryRecord = {
    full_name: repository,
    id: 611,
  };
  const run = (id, path) => ({
    conclusion: "success",
    event: "push",
    head_branch: "main",
    head_sha: SOURCE_COMMIT,
    id,
    path,
    repository: repositoryRecord,
    run_attempt: 1,
    status: "completed",
  });
  const jobsCiRun = run(1001, ".github/workflows/jobs-ci.yml@main");
  const observabilityRun = run(
    1002,
    ".github/workflows/observability-policy.yml",
  );
  const evidence = createManagedCloudCiEvidence({
    defaultBranch: "main",
    jobsCiRun,
    observabilityRun,
    releaseContractSelfTestSha256: "b".repeat(64),
    repository,
    sourceCommit: SOURCE_COMMIT,
  });
  assert.equal(evidence.version, 2);
  assert.equal(evidence.defaultBranch, "main");
  assert.equal("default_branch" in repositoryRecord, false);
  assert.equal(evidence.audience, MANAGED_CLOUD_AUDIENCES.testEvidenceV2);
  assert.equal(
    validateManagedCloudTestEvidence(evidence, SOURCE_COMMIT, 3),
    evidence,
  );
  assert.throws(
    () => validateManagedCloudTestEvidence(evidence, SOURCE_COMMIT, 1),
    /must contain exactly|does not bind the candidate source/,
  );
  assert.deepEqual(
    evidence.ciRuns.map((item) => item.checkId),
    ["jobs-ci", "observability-policy"],
  );
  assert.throws(
    () =>
      createManagedCloudCiEvidence({
        defaultBranch: "main",
        jobsCiRun: { ...jobsCiRun, event: "pull_request" },
        observabilityRun,
        releaseContractSelfTestSha256: "b".repeat(64),
        repository,
        sourceCommit: SOURCE_COMMIT,
      }),
    /exact-tip default-branch push run/,
  );
  assert.throws(
    () =>
      createManagedCloudCiEvidence({
        defaultBranch: "main",
        jobsCiRun,
        observabilityRun: { ...observabilityRun, head_sha: "c".repeat(40) },
        releaseContractSelfTestSha256: "b".repeat(64),
        repository,
        sourceCommit: SOURCE_COMMIT,
      }),
    /exact-tip default-branch push run/,
  );
  const storedBranchDrift = structuredClone(evidence);
  storedBranchDrift.ciRuns[0].headBranch = "other";
  assert.throws(
    () => validateManagedCloudTestEvidence(storedBranchDrift, SOURCE_COMMIT, 2),
    /exact-tip CI authority/,
  );
  const storedDefaultBranchDrift = structuredClone(evidence);
  storedDefaultBranchDrift.defaultBranch = "other";
  assert.throws(
    () =>
      validateManagedCloudTestEvidence(
        storedDefaultBranchDrift,
        SOURCE_COMMIT,
        2,
      ),
    /exact-tip CI authority/,
  );
  const slashBranchRun = (source) => ({
    ...source,
    head_branch: "release/main",
    path: source.path.replace(/@main$/, "") + "@release/main",
  });
  const slashBranchEvidence = createManagedCloudCiEvidence({
    defaultBranch: "release/main",
    jobsCiRun: slashBranchRun(jobsCiRun),
    observabilityRun: slashBranchRun(observabilityRun),
    releaseContractSelfTestSha256: "b".repeat(64),
    repository,
    sourceCommit: SOURCE_COMMIT,
  });
  assert.equal(
    validateManagedCloudTestEvidence(slashBranchEvidence, SOURCE_COMMIT, 2),
    slashBranchEvidence,
  );
  assert.throws(
    () =>
      createManagedCloudCiEvidence({
        defaultBranch: "release..main",
        jobsCiRun,
        observabilityRun,
        releaseContractSelfTestSha256: "b".repeat(64),
        repository,
        sourceCommit: SOURCE_COMMIT,
      }),
    /canonical Git branch name/,
  );
  assert.throws(
    () =>
      createManagedCloudCiEvidence({
        defaultBranch: "release/main",
        jobsCiRun,
        observabilityRun,
        releaseContractSelfTestSha256: "b".repeat(64),
        repository,
        sourceCommit: SOURCE_COMMIT,
      }),
    /exact-tip default-branch push run/,
  );
});

test("promotion binds the successful exact-source publication workflow job", () => {
  const repository = "Dhanunjay-Divi/bluey";
  const plan = {
    sourceCommit: SOURCE_COMMIT,
    sourceDefaultBranch: "main",
    sourceRepository: repository,
    sourceRepositoryId: 611,
  };
  const publication = { workflowRunAttempt: 1, workflowRunId: 2001 };
  const run = {
    conclusion: "success",
    event: "workflow_dispatch",
    head_branch: "main",
    head_sha: SOURCE_COMMIT,
    id: 2001,
    path: ".github/workflows/jobs-managed-cloud-release.yml@main",
    repository: {
      full_name: repository,
      id: 611,
    },
    run_attempt: 1,
    status: "completed",
  };
  const publishJob = {
    conclusion: "success",
    head_sha: SOURCE_COMMIT,
    id: 3001,
    name: "publish",
    run_attempt: 1,
    run_id: 2001,
    status: "completed",
    steps: [
      {
        conclusion: "success",
        name: "Upload the remote publication receipt only",
        status: "completed",
      },
    ],
  };
  const jobs = { jobs: [publishJob], total_count: 1 };
  const artifacts = publicationWorkflowArtifacts();
  const evidence = createManagedCloudPublicationWorkflowEvidence({
    artifacts,
    defaultBranch: "main",
    jobs,
    plan,
    publication,
    run,
  });
  assert.equal(evidence.runId, 2001);
  assert.equal(evidence.jobId, 3001);
  assert.equal(evidence.artifactId, 4001);
  assert.equal(evidence.artifactName, "jobs-managed-cloud-publication-2001-1");
  assert.equal("default_branch" in run.repository, false);
  assert.equal(
    evidence.environmentProtectionStatus,
    "workflow_declared_settings_not_attested",
  );
  assert.throws(
    () =>
      createManagedCloudPublicationWorkflowEvidence({
        artifacts,
        defaultBranch: "main",
        jobs,
        plan,
        publication,
        run: { ...run, head_sha: "c".repeat(40) },
      }),
    /successful exact-source workflow dispatch/,
  );
  assert.throws(
    () =>
      createManagedCloudPublicationWorkflowEvidence({
        artifacts: {
          artifacts: [
            ...artifacts.artifacts,
            { ...artifacts.artifacts[0], id: 4002 },
          ],
          total_count: 2,
        },
        defaultBranch: "main",
        jobs,
        plan,
        publication,
        run,
      }),
    /exactly one attempt-qualified receipt/,
  );
  assert.throws(
    () =>
      createManagedCloudPublicationWorkflowEvidence({
        artifacts: {
          artifacts: [
            {
              ...artifacts.artifacts[0],
              size_in_bytes: 1024 * 1024 + 1,
            },
          ],
          total_count: 1,
        },
        defaultBranch: "main",
        jobs,
        plan,
        publication,
        run,
      }),
    /exceeds the 1 MiB receipt limit/,
  );
  assert.throws(
    () =>
      createManagedCloudPublicationWorkflowEvidence({
        artifacts,
        defaultBranch: "main",
        jobs: {
          jobs: [{ ...publishJob, conclusion: "failure" }],
          total_count: 1,
        },
        plan,
        publication,
        run,
      }),
    /did not complete successfully/,
  );
  assert.throws(
    () =>
      createManagedCloudPublicationWorkflowEvidence({
        artifacts,
        defaultBranch: "main",
        jobs: {
          jobs: [{ ...publishJob, run_attempt: 2 }],
          total_count: 1,
        },
        plan,
        publication,
        run,
      }),
    /did not complete successfully/,
  );
  assert.throws(
    () =>
      createManagedCloudPublicationWorkflowEvidence({
        artifacts,
        defaultBranch: "other",
        jobs,
        plan,
        publication,
        run,
      }),
    /successful exact-source workflow dispatch/,
  );
});

function mutateMarkedSection(workflowText, marker, mutate) {
  const begin = `# RELEASE-GATE: ${marker}-BEGIN`;
  const end = `# RELEASE-GATE: ${marker}-END`;
  const start = workflowText.indexOf(begin) + begin.length;
  const finish = workflowText.indexOf(end);
  const section = workflowText.slice(start, finish);
  const mutatedSection = mutate(section);
  assert.notEqual(mutatedSection, section, `${marker} mutation must alter its section`);
  return workflowText.slice(0, start) + mutatedSection + workflowText.slice(finish);
}

test("promotion derives the earlier source only from its exact successful publication run", () => {
  const identity = {
    defaultBranch: "main", repository: "Dhanunjay-Divi/bluey", repositoryId: 611, runAttempt: 2, runId: 9001,
  };
  const run = {
    conclusion: "success", event: "workflow_dispatch", head_branch: "main", head_sha: SOURCE_COMMIT,
    id: 9001, path: ".github/workflows/jobs-managed-cloud-release.yml@main",
    repository: { id: 611, full_name: identity.repository }, run_attempt: 2, status: "completed",
  };
  assert.equal(selectManagedCloudPublicationSource({ ...identity, run }), SOURCE_COMMIT);
  for (const patch of [
    { head_branch: "other" }, { path: ".github/workflows/unrelated.yml@main" }, { run_attempt: 1 },
    { head_repository: { id: 612 } }, { event: "push" }, { conclusion: "skipped" }, { id: 9002 },
  ]) {
    assert.throws(() => selectManagedCloudPublicationSource({ ...identity, run: { ...run, ...patch } }),
      /exact successful same-repository default-branch attempt/);
  }
  assert.throws(() => selectManagedCloudPublicationSource({ ...identity, run: { ...run, head_sha: "HEAD" } }),
    /sourceCommit must be 40 lowercase hexadecimal/);
});

test("protected authorization workflow refuses weakened source and policy bindings", async (t) => {
  const workflow = new URL("../../.github/workflows/jobs-managed-cloud-release.yml", import.meta.url);
  const text = await readFile(workflow, "utf8");
  const root = await temporaryDirectory(t, "authorization-policy-workflow");
  const variants = [
    ["verifier-ref", "ref: ${{ github.workflow_sha }}", "ref: ${{ github.sha }}"],
    ["unverified-source", "intake-managed-cloud-authorized.sh candidate", "intake-managed-cloud-authorized.sh"],
    ["mutable-source-ref", "ref: ${{ steps.candidate-source.outputs.commit }}", "ref: main"],
    ["persisted-credentials", "persist-credentials: false", "persist-credentials: true"],
    ["expanded-policy-checkout", "/scripts/analyze-tracing-calls.py", "/scripts/"],
    ["checkout-head-unchecked", ' = "$POLICY_SOURCE_COMMIT"', ' != "$POLICY_SOURCE_COMMIT"'],
    ["unprotected-pin", "secrets.BLUEY_JOBS_OBSERVABILITY_POLICY_SHA256", "inputs.policy_sha256"],
    ["skip-policy-step", "      - name: Verify the protected Observability policy before handling release signatures\n",
      "      - name: Verify the protected Observability policy before handling release signatures\n        if: false\n"],
    ["wrong-policy-repository", '--observability-policy-repo "$GITHUB_WORKSPACE/candidate-policy"',
      '--observability-policy-repo "$GITHUB_WORKSPACE"'],
    ["legacy-authorization-id", 'authorization-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT', 'authorization-$GITHUB_RUN_ID'],
    ["unchecked-candidate-set", "--expected-candidate-set-sha256", "--unchecked-set-sha256"],
  ];
  for (const [label, expected, replacement] of variants) {
    const mutated = mutateMarkedSection(text, "AUTHORIZE-NO-REBUILD", (section) =>
      section.replace(expected, replacement));
    assert.notEqual(mutated, text, label);
    const path = join(root, label + ".yml");
    await writeFile(path, mutated);
    await assert.rejects(() => validateManagedCloudWorkflowContract(path),
      /Authorization must bind the exact approved Observability Policy before consuming signatures/);
  }
  const reordered = mutateMarkedSection(text, "AUTHORIZE-NO-REBUILD", (section) => {
    const start = section.indexOf("      - name: Verify the protected Observability policy");
    const next = section.indexOf("      - name: Reverify and authorize", start);
    const upload = section.indexOf("      - name: Upload authorized bytes", next);
    assert(start >= 0 && next > start && upload > next);
    return section.slice(0, start) + section.slice(next, upload)
      + section.slice(start, next) + section.slice(upload);
  });
  const path = join(root, "late-policy.yml");
  await writeFile(path, reordered);
  await assert.rejects(() => validateManagedCloudWorkflowContract(path),
    /Authorization must bind the exact approved Observability Policy before consuming signatures/);
});

test("both managed publication consumers require the exact staged authorization", async (t) => {
  const workflow = new URL("../../.github/workflows/jobs-managed-cloud-release.yml", import.meta.url);
  const text = await readFile(workflow, "utf8");
  const root = await temporaryDirectory(t, "managed-authorization-consumers");
  for (const [index, [marker, exact, changed]] of [
    ["AUTHORIZE-NO-REBUILD", "name: jobs-managed-cloud-authorized-${{ github.run_id }}-${{ github.run_attempt }}",
      "name: jobs-managed-cloud-authorized"],
    ["PUBLISH-REMOTE", "ref: ${{ github.workflow_sha }}", "ref: main"],
    ["PUBLISH-REMOTE", "secrets.BLUEY_JOBS_OBSERVABILITY_POLICY_SHA256", "inputs.unreviewed_policy_sha"],
    ["PUBLISH-REMOTE", 'test "$AUTHORIZATION_WORKFLOW_COMMIT" = "$CANDIDATE_SOURCE_COMMIT"', "true"],
    ["PUBLISH-REMOTE", "bash jobs/scripts/intake-managed-cloud-authorized.sh", "true"],
    ["PUBLISH-REMOTE", "steps.intake-authorization.outputs.root", "inputs.unchecked_archive"],
    ["PROMOTE-NO-REBUILD", "publication-source-commit", "verify"],
    ["PROMOTE-NO-REBUILD", 'AUTHORIZATION_WORKFLOW_COMMIT="$source_commit"', 'AUTHORIZATION_WORKFLOW_COMMIT="$GITHUB_SHA"'],
    ["PROMOTE-NO-REBUILD", '            --authorized "$root"', '            --authorized authorized'],
    ["PROMOTE-NO-REBUILD", "steps.select-publication.outputs.authorized-root", "inputs.unchecked_archive"],
  ].entries()) {
    const path = join(root, "consumer-" + index + ".yml");
    await writeFile(path, mutateMarkedSection(text, marker, (section) => section.replace(exact, changed)));
    await assert.rejects(() => validateManagedCloudWorkflowContract(path),
      /must intake one exact protected authorization before use/);
  }
});

test("candidate evidence keeps verification on the build attempt", async (t) => {
  const text = await readFile(new URL("../../.github/workflows/jobs-managed-cloud-release.yml", import.meta.url), "utf8");
  const root = await temporaryDirectory(t, "candidate-producer-contract");
  for (const [index, [marker, exact, changed]] of [
    ["CANDIDATE-BUILD", "name: jobs-managed-cloud-candidate-${{ github.run_id }}-${{ github.run_attempt }}",
      "name: jobs-managed-cloud-candidate"],
    ["VERIFY-NO-REBUILD", "needs: candidate", "needs: []"],
    ["VERIFY-NO-REBUILD", "name: jobs-managed-cloud-candidate-${{ github.run_id }}-${{ github.run_attempt }}",
      "name: jobs-managed-cloud-candidate-${{ github.run_id }}"],
    ["VERIFY-NO-REBUILD", "          path: candidate", "          run-id: 1\n          path: candidate"],
  ].entries()) {
    const path = join(root, "candidate-producer-" + index + ".yml");
    await writeFile(path, mutateMarkedSection(text, marker, (section) => section.replace(exact, changed)));
    await assert.rejects(() => validateManagedCloudWorkflowContract(path),
      /must bind the exact run attempt/);
  }
});

test("every managed release job requires GitHub's protected-ref boolean", async (t) => {
  const workflow = new URL("../../.github/workflows/jobs-managed-cloud-release.yml", import.meta.url);
  const text = await readFile(workflow, "utf8");
  const root = await temporaryDirectory(t, "protected-release-ref");
  assert.equal(await validateManagedCloudWorkflowContract(workflow), true);
  const guard = "      github.ref_protected == true &&\n";
  const cases = [
    ["CANDIDATE-BUILD", ""],
    ["VERIFY-NO-REBUILD", ""],
    ["AUTHORIZE-NO-REBUILD", ""],
    ["PUBLISH-REMOTE", ""],
    ["PROMOTE-NO-REBUILD", ""],
    ["ROLLBACK-NO-REBUILD", ""],
    ["PUBLISH-REMOTE", "      github.ref_protected == false &&\n"],
    ["PUBLISH-REMOTE", "      github.ref_protected == 'true' &&\n"],
    ["PUBLISH-REMOTE", "      github.ref_protected == true ||\n"],
    ["PUBLISH-REMOTE", "      true &&\n"],
  ];
  for (const [index, [marker, weakened]] of cases.entries()) {
    const path = join(root, `protected-ref-${index}.yml`);
    const mutant = mutateMarkedSection(text, marker, (section) => {
      assert.ok(section.includes(guard), marker);
      return section.replace(guard, weakened);
    });
    await writeFile(path, mutant);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /must require the protected default branch for exactly its release operation/,
      marker,
    );
  }
});

test("release workflow statically proves build-once and protected authorization", async (t) => {
  const workflow = new URL(
    "../../.github/workflows/jobs-managed-cloud-release.yml",
    import.meta.url,
  );
  assert.equal(await validateManagedCloudWorkflowContract(workflow), true);
  const text = await readFile(workflow, "utf8");
  assert.match(text, /directDiscovery: true/);
  assert.match(text, /globalDiscovery: true/);
  assert.match(text, /sourceVerification: true/);
  assert.match(text, /bluey-jobs-managed-cloud-release-descriptor-v4/);
  assert.match(
    text,
    new RegExp(
      "BLUEY_JOBS_WORKFLOW_RUNTIME_ROLES=discovery_worker," +
        "global_discovery_worker,original_source_verifier," +
        "workflow_gateway,workflow_worker",
    ),
  );
  const workflowsDockerfile = await readFile(
    new URL("../workflows/Dockerfile", import.meta.url),
    "utf8",
  );
  const exactWorkflowRoles =
    "discovery_worker,global_discovery_worker,original_source_verifier," +
    "workflow_gateway,workflow_worker";
  assert.deepEqual(
    [
      ...workflowsDockerfile.matchAll(
        /^ARG BLUEY_JOBS_WORKFLOW_RUNTIME_ROLES=([^\r\n]+)$/gm,
      ),
    ].map((match) => match[1]),
    [exactWorkflowRoles, exactWorkflowRoles],
  );
  assert.deepEqual(
    [...text.matchAll(/BLUEY_JOBS_WORKFLOW_RUNTIME_ROLES=([a-z_,]+)/g)].map(
      (match) => match[1],
    ),
    [exactWorkflowRoles],
  );
  assert.equal(
    text.split(
      "uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020",
    ).length - 1,
    6,
  );
  assert.equal(text.split("node-version: 22.23.2").length - 1, 6);
  assert.equal(
    text.split('test "$(node --version)" = "v22.23.2"').length - 1,
    6,
  );
  assert.equal(text.split("github.ref_type == 'branch'").length - 1, 6);
  assert.equal(
    text.split("github.ref_name == github.event.repository.default_branch")
      .length - 1,
    6,
  );
  assert.equal(text.split("actions: read").length - 1, 6);
  assert.equal(text.split("contents: read").length - 1, 7);
  assert.equal(text.split("packages: write").length - 1, 1);
  assert.match(text, /environment: jobs-managed-cloud-artifact-publication/);
  assert.match(text, /publication-attest/);
  assert.match(text, /publication-preflight/);
  assert.match(text, /publication-portal-manifest/);
  assert.match(text, /publication-verify/);
  assert.match(
    text,
    /--annotation "org\.opencontainers\.image\.created=\$PORTAL_MANIFEST_CREATED_AT"/,
  );
  assert.doesNotMatch(text, /`ghcr\.io\/bluey\//);
  assert.doesNotMatch(text, /local_fixture/);
  assert.doesNotMatch(text, /directDiscovery: false/);
  assert.doesNotMatch(text, /globalDiscovery: false/);
  assert.doesNotMatch(text, /sourceVerification: false/);
  assert.doesNotMatch(text, /bluey-jobs-managed-cloud-release-descriptor-v1/);
  assert.doesNotMatch(text, /bluey-jobs-managed-cloud-release-descriptor-v2/);
  for (const contract of Object.values(MANAGED_CLOUD_RUNTIME_CONTRACTS)) {
    assert.ok(
      contract.requiredEnvironment.some((item) => item === "PATH=" + SAFE_PATH),
    );
  }

  const root = await temporaryDirectory(t, "managed-cloud-v3-workflow-guard");
  const mutateNamedStep = (section, name, mutate) => {
    const marker = `      - name: ${name}`;
    const start = section.indexOf(marker);
    assert.ok(start >= 0, `${name} step must exist before mutation`);
    assert.equal(
      section.indexOf(marker, start + marker.length),
      -1,
      `${name} step must be unambiguous before mutation`,
    );
    const next = section.indexOf("\n      - ", start + marker.length);
    const finish = next < 0 ? section.length : next;
    const step = section.slice(start, finish);
    const mutatedStep = mutate(step);
    assert.notEqual(
      mutatedStep,
      step,
      `${name} mutation must alter only that step`,
    );
    return section.slice(0, start) + mutatedStep + section.slice(finish);
  };
  const permissionContracts = new Map([
    ["CANDIDATE-BUILD", ["actions: read", "contents: read"]],
    ["VERIFY-NO-REBUILD", ["actions: read", "contents: read"]],
    ["AUTHORIZE-NO-REBUILD", ["actions: read", "contents: read"]],
    ["PUBLISH-REMOTE", ["actions: read", "contents: read", "packages: write"]],
    [
      "PROMOTE-NO-REBUILD",
      ["actions: read", "contents: read", "packages: read"],
    ],
    ["ROLLBACK-NO-REBUILD", ["actions: read", "contents: read"]],
  ]);
  for (const [marker, permissions] of permissionContracts) {
    for (const permission of permissions) {
      const mutated = mutateMarkedSection(text, marker, (section) =>
        section.replace(
          `      ${permission}`,
          `      ${permission.split(":")[0]}: none`,
        ),
      );
      const label = permission.replace(/[: ]/g, "-");
      const path = join(
        root,
        `${marker.toLowerCase()}-${label}-permission.yml`,
      );
      await writeFile(path, mutated);
      await assert.rejects(
        () => validateManagedCloudWorkflowContract(path),
        /exact least-privilege job-local permissions/,
      );
    }
  }
  for (const marker of [
    "CANDIDATE-BUILD",
    "VERIFY-NO-REBUILD",
    "AUTHORIZE-NO-REBUILD",
    "ROLLBACK-NO-REBUILD",
  ]) {
    const mutated = mutateMarkedSection(text, marker, (section) =>
      section.replace(
        "      contents: read",
        "      contents: read\n      packages: read",
      ),
    );
    const path = join(root, `${marker.toLowerCase()}-unexpected-packages.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /exact least-privilege job-local permissions/,
    );
  }
  for (const [label, replacement] of [
    ["changed-contents", "permissions:\n  contents: none"],
    ["widened-actions", "permissions:\n  actions: read\n  contents: read"],
    ["widened-packages", "permissions:\n  contents: read\n  packages: read"],
  ]) {
    const mutated = text.replace("permissions:\n  contents: read", replacement);
    assert.notEqual(
      mutated,
      text,
      `${label} mutation must alter global permissions`,
    );
    const path = join(root, `${label}-global-permissions.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /Global workflow permissions must be contents: read only/,
    );
  }
  for (const [label, original, replacement] of [
    ["contract", "--version 4", "--version 3"],
    ["descriptor", "release-descriptor-v4", "release-descriptor-v3"],
    ["descriptor-version", "version: 4", "version: 3"],
    ["self-test", "release-gate-tests-v4", "release-gate-tests-v3"],
    ["contract-suffix", "--version 4", "--version 40"],
    ["descriptor-version-suffix", "version: 4", "version: 40"],
    ["self-test-suffix", "release-gate-tests-v4", "release-gate-tests-v40"],
    ["duplicate-descriptor-version", "            version: 4,", "            version: 4,\n            version: 3,"],
    ["feature", "sourceVerification: true", "sourceVerification: false"],
    [
      "runtime-role",
      exactWorkflowRoles,
      "global_discovery_worker,original_source_verifier,workflow_gateway,workflow_worker",
    ],
    [
      "runtime-role-extra",
      exactWorkflowRoles,
      exactWorkflowRoles + ",unexpected_worker",
    ],
    ["direct-discovery", "directDiscovery: true", "directDiscovery: false"],
    ["global-discovery", "globalDiscovery: true", "globalDiscovery: false"],
  ]) {
    const mutated = text.replace(original, replacement);
    assert.notEqual(
      mutated,
      text,
      `${label} mutation must alter the workflow fixture`,
    );
    const path = join(root, `${label}.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /exact release-v4 discovery and source-verifier candidate/,
    );
  }

  for (const [label, runId, output] of [
    ["jobs-ci", "$JOBS_CI_RUN_ID", "release/prepared/jobs-ci-run.json"],
    [
      "observability",
      "$OBSERVABILITY_RUN_ID",
      "release/prepared/observability-run.json",
    ],
  ]) {
    const endpoint = `gh api "/repos/$GITHUB_REPOSITORY/actions/runs/${runId}"`;
    const directRedirect = mutateMarkedSection(
      text,
      "CANDIDATE-BUILD",
      (section) =>
        section.replace(
          `${endpoint} \\\n` +
            "            | node jobs/scripts/managed-cloud-release-gate.mjs " +
            "publication-api-capture \\\n" +
            `                --kind run --out ${output}`,
          `${endpoint} > ${output}`,
        ),
    );
    assert.notEqual(
      directRedirect,
      text,
      `${label} direct-redirection mutation must alter the workflow fixture`,
    );
    const directRedirectPath = join(root, `${label}-direct-redirect.yml`);
    await writeFile(directRedirectPath, directRedirect);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(directRedirectPath),
      /exactly two atomic 128 KiB captures|fail closed without final or partial output/,
    );

    const bypassCapture = mutateMarkedSection(
      text,
      "CANDIDATE-BUILD",
      (section) =>
        section.replace(
          `--kind run --out ${output}`,
          `--kind run-unbounded --out ${output}`,
        ),
    );
    assert.notEqual(
      bypassCapture,
      text,
      `${label} capture-bypass mutation must alter the workflow fixture`,
    );
    const bypassCapturePath = join(root, `${label}-capture-bypass.yml`);
    await writeFile(bypassCapturePath, bypassCapture);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(bypassCapturePath),
      /exactly two atomic 128 KiB captures|fail closed without final or partial output/,
    );

    const missingCleanup = mutateMarkedSection(
      text,
      "CANDIDATE-BUILD",
      (section) => section.replace(`${output}.partial`, `${output}.unchecked`),
    );
    assert.notEqual(
      missingCleanup,
      text,
      `${label} missing-cleanup mutation must alter the workflow fixture`,
    );
    const missingCleanupPath = join(root, `${label}-missing-cleanup.yml`);
    await writeFile(missingCleanupPath, missingCleanup);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(missingCleanupPath),
      /fail closed without final or partial output/,
    );
  }

  const missingCandidateCapturePipefail = mutateMarkedSection(
    text,
    "CANDIDATE-BUILD",
    (section) =>
      mutateNamedStep(
        section,
        "Bind successful exact-tip CI instead of synthetic candidate evidence",
        (step) =>
          step.replace(
            "        run: |\n          set -euo pipefail\n",
            "        run: |\n          set -eu\n",
          ),
      ),
  );
  const missingCandidateCapturePipefailPath = join(
    root,
    "candidate-capture-missing-pipefail.yml",
  );
  await writeFile(
    missingCandidateCapturePipefailPath,
    missingCandidateCapturePipefail,
  );
  await assert.rejects(
    () =>
      validateManagedCloudWorkflowContract(missingCandidateCapturePipefailPath),
    /exact-tip CI capture step must enable pipefail/,
  );

  const interpolatedPublicationRun = text.replace(
    '--workflow-run-id "$PUBLICATION_RUN_ID"',
    '--workflow-run-id "${{ github.event.inputs.publication_run_id }}"',
  );
  assert.notEqual(interpolatedPublicationRun, text);
  const interpolatedPublicationRunPath = join(
    root,
    "interpolated-publication-run.yml",
  );
  await writeFile(interpolatedPublicationRunPath, interpolatedPublicationRun);
  await assert.rejects(
    () => validateManagedCloudWorkflowContract(interpolatedPublicationRunPath),
    /Promotion must independently read back/,
  );
  const missingPublicationJobsApi = text.replace(
    '/attempts/$PUBLICATION_RUN_ATTEMPT/jobs?per_page=100"',
    '/attempts/$PUBLICATION_RUN_ATTEMPT/jobs"',
  );
  assert.notEqual(missingPublicationJobsApi, text);
  const missingPublicationJobsApiPath = join(
    root,
    "missing-publication-jobs-api.yml",
  );
  await writeFile(missingPublicationJobsApiPath, missingPublicationJobsApi);
  await assert.rejects(
    () => validateManagedCloudWorkflowContract(missingPublicationJobsApiPath),
    /Promotion must independently read back/,
  );
  for (const [label, original, replacement] of [
    [
      "unsafe-publication-run-input",
      "Number.isSafeInteger(Number(value))",
      "Number.isInteger(Number(value))",
    ],
    ["latest-publication-attempt", '/attempts/$PUBLICATION_RUN_ATTEMPT"', '"'],
    [
      "unchecked-publication-artifact",
      "publication-artifact-select",
      "publication-artifact-unchecked",
    ],
    [
      "unbounded-publication-run-api",
      "publication-api-capture",
      "publication-api-unbounded",
    ],
    [
      "missing-publication-api-cleanup",
      "rm -f -- publication-workflow-run.json publication-workflow-run.json.partial",
      "true # missing bounded-capture cleanup",
    ],
    [
      "receipt-download-by-name",
      "artifact-ids: ${{ steps.select-publication.outputs.artifact-id }}",
      "name: jobs-managed-cloud-publication",
    ],
    [
      "unbound-publication-evidence",
      "--publication-workflow-evidence publication-workflow-evidence.json",
      "--publication-workflow-evidence unchecked.json",
    ],
  ]) {
    const mutated = mutateMarkedSection(text, "PROMOTE-NO-REBUILD", (section) =>
      section.replace(original, replacement),
    );
    assert.notEqual(
      mutated,
      text,
      `${label} mutation must alter the workflow fixture`,
    );
    const path = join(root, `${label}.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /Promotion must independently read back|Promotion preflight must pass/,
    );
  }

  const staticPublicationName = text.replace(
    "name: jobs-managed-cloud-publication-${{ github.run_id }}-${{ github.run_attempt }}",
    "name: jobs-managed-cloud-publication",
  );
  assert.notEqual(staticPublicationName, text);
  const staticPublicationNamePath = join(root, "static-publication-name.yml");
  await writeFile(staticPublicationNamePath, staticPublicationName);
  await assert.rejects(
    () => validateManagedCloudWorkflowContract(staticPublicationNamePath),
    /protected package writer and immutable readback/,
  );

  for (const [label, original, replacement] of [
    ["package-writer", "packages: write", "packages: read"],
    ["remote-readback", "oras cp --to-oci-layout", "oras cp"],
    [
      "oci-layout-source",
      '"publication-source/$component@sha256:$DIGEST" "$TAG_REF"',
      '"authorized/candidate/payloads/$component.oci.tar@sha256:$DIGEST" "$TAG_REF"',
    ],
    [
      "controlled-namespace",
      "vars.BLUEY_JOBS_ARTIFACT_NAMESPACE",
      "'ghcr.io/bluey/jobs'",
    ],
  ]) {
    const mutated = text.replace(original, replacement);
    assert.notEqual(
      mutated,
      text,
      `${label} mutation must alter the workflow fixture`,
    );
    const path = join(root, `${label}.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /controlled namespaces|protected package writer|immutable readback|exact least-privilege job-local permissions/,
    );
  }

  for (const [label, marker, replacement] of [
    ["authorized", '--authorized "$AUTHORIZED_ROOT"', "--authorized unchecked"],
    [
      "root-anchor-sha256",
      '--root-anchor-sha256 "$ROOT_ANCHOR_SHA256"',
      '--root-anchor-sha256 "$MUTATED_ROOT_ANCHOR_SHA256"',
    ],
    [
      "default-branch",
      '--default-branch "$DEFAULT_BRANCH"',
      '--default-branch "$MUTATED_DEFAULT_BRANCH"',
    ],
    [
      "source-commit",
      '--source-commit "$GITHUB_SHA"',
      '--source-commit "$MUTATED_SHA"',
    ],
    [
      "repository",
      '--repository "$GITHUB_REPOSITORY"',
      '--repository "$MUTATED_REPOSITORY"',
    ],
    [
      "repository-id",
      '--repository-id "$GITHUB_REPOSITORY_ID"',
      '--repository-id "$MUTATED_REPOSITORY_ID"',
    ],
  ]) {
    const mutated = mutateMarkedSection(text, "PUBLISH-REMOTE", (section) =>
      section.replaceAll(marker, replacement),
    );
    const path = join(root, `${label}-runtime-identity.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /protected package writer and immutable readback/,
    );
  }
  for (const marker of ["PUBLISH-REMOTE", "PROMOTE-NO-REBUILD"]) {
    const unboundedManifest = mutateMarkedSection(text, marker, (section) =>
      section.replace("publication-portal-capture", "publication-portal-tee"),
    );
    const unboundedManifestPath = join(
      root,
      `${marker.toLowerCase()}-unbounded-portal-manifest.yml`,
    );
    await writeFile(unboundedManifestPath, unboundedManifest);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(unboundedManifestPath),
      /exact portal OCI manifest and one-file pull/,
    );
    const uncheckedPortal = mutateMarkedSection(text, marker, (section) =>
      section.replace(
        "publication-portal-readback",
        "publication-portal-unchecked",
      ),
    );
    const uncheckedPortalPath = join(
      root,
      `${marker.toLowerCase()}-unchecked-portal-readback.yml`,
    );
    await writeFile(uncheckedPortalPath, uncheckedPortal);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(uncheckedPortalPath),
      /exact portal OCI manifest and one-file pull/,
    );
    const uncheckedManifest = mutateMarkedSection(text, marker, (section) =>
      section.replace(
        "publication-portal-manifest",
        "portal-manifest-unchecked",
      ),
    );
    const uncheckedManifestPath = join(
      root,
      `${marker.toLowerCase()}-unchecked-portal-manifest.yml`,
    );
    await writeFile(uncheckedManifestPath, uncheckedManifest);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(uncheckedManifestPath),
      /exact portal OCI manifest and one-file pull/,
    );
    const lateManifest = mutateMarkedSection(text, marker, (section) => {
      const commandStart =
        "\n          node jobs/scripts/managed-cloud-release-gate.mjs \\\n" +
        "            publication-portal-manifest \\\n";
      const commandEnd =
        "            --manifest " +
        "publication-readback/jobs-portal.oci-manifest.json\n";
      const start = section.indexOf(commandStart);
      const finish = section.indexOf(commandEnd, start) + commandEnd.length;
      assert.ok(
        start >= 0 && finish >= commandEnd.length,
        `${marker} must contain manifest-only validation`,
      );
      const block = section.slice(start, finish);
      return section
        .replace(block, "")
        .replace(
          'oras pull "$PORTAL_REGISTRY_REF" --output portal-download',
          'oras pull "$PORTAL_REGISTRY_REF" --output portal-download' + block,
        );
    });
    const lateManifestPath = join(
      root,
      `${marker.toLowerCase()}-late-portal-manifest.yml`,
    );
    await writeFile(lateManifestPath, lateManifest);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(lateManifestPath),
      /exact portal OCI manifest and one-file pull/,
    );
  }
  const missingCreatedAnnotation = mutateMarkedSection(
    text,
    "PUBLISH-REMOTE",
    (section) =>
      section.replace(
        '--annotation "org.opencontainers.image.created=$PORTAL_MANIFEST_CREATED_AT"',
        '--annotation "org.opencontainers.image.created=$UNBOUND_NOW"',
      ),
  );
  const missingCreatedAnnotationPath = join(
    root,
    "unbound-portal-created-annotation.yml",
  );
  await writeFile(missingCreatedAnnotationPath, missingCreatedAnnotation);
  await assert.rejects(
    () => validateManagedCloudWorkflowContract(missingCreatedAnnotationPath),
    /protected package writer and immutable readback/,
  );
  const rawPromotionReceipt = mutateMarkedSection(
    text,
    "PROMOTE-NO-REBUILD",
    (section) =>
      section.replace(
        'const preflight=require("./publication-preflight.json");',
        'const preflight=require("./publication/publication-plan.json");',
      ),
  );
  const rawPromotionReceiptPath = join(root, "raw-promotion-receipt.yml");
  await writeFile(rawPromotionReceiptPath, rawPromotionReceipt);
  await assert.rejects(
    () => validateManagedCloudWorkflowContract(rawPromotionReceiptPath),
    /Promotion must independently read back/,
  );
  const missingPromotionPreflight = mutateMarkedSection(
    text,
    "PROMOTE-NO-REBUILD",
    (section) =>
      section.replace("publication-preflight \\", "publication-unchecked \\"),
  );
  const missingPromotionPreflightPath = join(
    root,
    "missing-promotion-preflight.yml",
  );
  await writeFile(missingPromotionPreflightPath, missingPromotionPreflight);
  await assert.rejects(
    () => validateManagedCloudWorkflowContract(missingPromotionPreflightPath),
    /Promotion must independently read back|Promotion preflight must pass/,
  );
  for (const [marker, expected] of [
    [
      "CANDIDATE-BUILD",
      /controlled namespaces and successful exact-tip CI evidence/,
    ],
    ["PROMOTE-NO-REBUILD", /Promotion must independently read back/],
  ]) {
    const missingDefaultBranchInput = mutateMarkedSection(
      text,
      marker,
      (section) =>
        section.replaceAll(
          '--default-branch "$DEFAULT_BRANCH"',
          "--default-branch main",
        ),
    );
    const missingDefaultBranchInputPath = join(
      root,
      `${marker.toLowerCase()}-missing-default-branch-input.yml`,
    );
    await writeFile(missingDefaultBranchInputPath, missingDefaultBranchInput);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(missingDefaultBranchInputPath),
      expected,
    );
  }

  for (const [label, original, replacement] of [
    [
      "node-setup-omission",
      "uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020",
      "# setup-node intentionally omitted by the negative fixture",
    ],
    ["node-wrong-patch", "node-version: 22.23.2", "node-version: 22.23.1"],
  ]) {
    const mutated = text.replace(original, replacement);
    assert.notEqual(
      mutated,
      text,
      `${label} mutation must alter the workflow fixture`,
    );
    const path = join(root, `${label}.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /default-branch-only and pin Node 22\.23\.2/,
    );
  }

  for (const [label, original, replacement] of [
    [
      "default-branch-omission",
      "github.ref_name == github.event.repository.default_branch",
      "true",
    ],
    [
      "default-branch-inversion",
      "github.ref_name == github.event.repository.default_branch",
      "github.ref_name != github.event.repository.default_branch",
    ],
  ]) {
    const mutated = text.replace(original, replacement);
    assert.notEqual(
      mutated,
      text,
      `${label} mutation must alter the workflow fixture`,
    );
    const path = join(root, `${label}.yml`);
    await writeFile(path, mutated);
    await assert.rejects(
      () => validateManagedCloudWorkflowContract(path),
      /default-branch-only and pin Node 22\.23\.2/,
    );
  }

  const nodeAuthorityBlock = [
    "      - name: Install the exact release Node interpreter",
    "        uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4",
    "        with:",
    "          node-version: 22.23.2",
    "      - name: Verify the exact release Node interpreter",
    "        run: |",
    "          set -euo pipefail",
    '          test "$(node --version)" = "v22.23.2"',
    "",
  ].join("\n");
  assert.ok(text.includes(nodeAuthorityBlock));
  const lateNodeAuthority = text
    .replace(nodeAuthorityBlock, "")
    .replace(
      "  # RELEASE-GATE: CANDIDATE-BUILD-END",
      nodeAuthorityBlock + "  # RELEASE-GATE: CANDIDATE-BUILD-END",
    );
  const latePath = join(root, "node-late-placement.yml");
  await writeFile(latePath, lateNodeAuthority);
  await assert.rejects(
    () => validateManagedCloudWorkflowContract(latePath),
    /default-branch-only and pin Node 22\.23\.2/,
  );
});

function runnerAutomationBundleEvidenceFixture() {
  const contract = MANAGED_CLOUD_RUNTIME_CONTRACTS["jobs-runner"];
  const paths = [
    "app/automation/dist/index.js",
    "app/automation/package.json",
    "app/runner/dist/native/bluey_jobs_runner_native_storage.node",
    "app/runner/dist/server.js",
    "ms-playwright/chromium_headless_shell-1228/chrome-headless-shell-linux64/chrome-headless-shell",
    "usr/local/bin/node",
  ];
  const measurement = {
    audience: MANAGED_CLOUD_AUDIENCES.runtimeMeasurement,
    buildId: "runner-639.0",
    componentId: "jobs-runner",
    configSchemaSha256: "a".repeat(64),
    measuredFiles: paths.map((path, index) => ({ path, sha256: String(index + 1).repeat(64) })),
    migrationSetSha256: "b".repeat(64),
    protocolSetSha256: "c".repeat(64),
    roles: ["managed_runner"],
    sourceCommit: SOURCE_COMMIT,
    version: 1,
  };
  const inventory = {
    artifactKind: "oci_image",
    artifactSha256: "d".repeat(64),
    audience: MANAGED_CLOUD_AUDIENCES.contentInventory,
    componentId: "jobs-runner",
    entries: measurement.measuredFiles.map((entry) => ({
      ...entry, type: "file", sizeBytes: 1,
      mode: entry.path.includes("native/") || entry.path.includes("ms-playwright/")
        || entry.path === "usr/local/bin/node" ? "0555" : "0444",
    })),
    runtime: {
      cmd: [...contract.cmd], entrypoint: [...contract.entrypoint],
      environment: [...contract.requiredEnvironment].sort(),
      exposedPorts: [...contract.exposedPorts], user: contract.runtimeUser,
      workingDirectory: contract.workingDirectory,
    },
    version: 1,
  };
  inventory.entries.unshift({ path: "app/.bluey/managed-cloud-runtime-measurement.json",
    type: "file", mode: "0444", sizeBytes: 1, sha256: "f".repeat(64) });
  const context = {
    buildId: measurement.buildId, componentId: measurement.componentId,
    configSchemaSha256: measurement.configSchemaSha256, inventory,
    migrationSetSha256: measurement.migrationSetSha256,
    protocolSetSha256: measurement.protocolSetSha256,
    roles: measurement.roles, sourceCommit: measurement.sourceCommit,
  };
  return refreshRunnerAutomationBundleEvidence({ measurement, context });
}

function refreshRunnerAutomationBundleEvidence(fixture) {
  const bytes = canonicalJsonBytes(fixture.measurement);
  fixture.measurementSha256 = sha256(bytes);
  const embedded = fixture.context.inventory.entries.find(
    (entry) => entry.path === "app/.bluey/managed-cloud-runtime-measurement.json",
  );
  embedded.sha256 = fixture.measurementSha256;
  embedded.sizeBytes = bytes.length;
  fixture.runtimeIdentities = [{ role: "managed_runner", runtimeIdentitySha256:
    deriveManagedCloudRuntimeIdentitySha256(fixture.measurementSha256, "jobs-runner", "managed_runner") }];
  return fixture;
}

function runnerAutomationBundleDigest(fixture) {
  return deriveManagedRunnerAutomationBundleSha256(fixture.measurement,
    fixture.measurementSha256, fixture.runtimeIdentities, fixture.context);
}

test("managed runner automation bundle uses a separate canonical v1 golden domain", () => {
  assert.equal(runnerAutomationBundleDigest(runnerAutomationBundleEvidenceFixture()),
    "167ed27a469d0d4a888d9c16f7ad3df3a6885dd0653defed72a755ed22b1b189");
});

test("runner bundle digest changes only for validated automation subtree bytes", () => {
  for (const [path, changed] of [["app/automation/dist/index.js", true],
    ["app/runner/dist/server.js", false]]) {
    const fixture = runnerAutomationBundleEvidenceFixture();
    fixture.measurement.measuredFiles.find((entry) => entry.path === path).sha256 = "9".repeat(64);
    fixture.context.inventory.entries.find((entry) => entry.path === path).sha256 = "9".repeat(64);
    refreshRunnerAutomationBundleEvidence(fixture);
    assert.equal(runnerAutomationBundleDigest(fixture)
      !== "167ed27a469d0d4a888d9c16f7ad3df3a6885dd0653defed72a755ed22b1b189", changed);
  }
});

test("runner bundle derivation requires exact component, role and embedded measurement", () => {
  for (const mutate of [
    (f) => { f.context.componentId = "jobs-workflows"; },
    (f) => { f.context.roles = ["managed_runner", "workflow_worker"]; },
    (f) => { f.measurement.measuredFiles[0].sha256 = "9".repeat(64); },
    (f) => { f.runtimeIdentities[0].runtimeIdentitySha256 = "9".repeat(64); },
    (f) => { f.context.inventory.entries[0].sha256 = "9".repeat(64); },
  ]) {
    const fixture = runnerAutomationBundleEvidenceFixture(); mutate(fixture);
    assert.throws(() => runnerAutomationBundleDigest(fixture));
  }
});

test("runner bundle cannot omit its package or executable module", () => {
  for (const path of ["app/automation/package.json", "app/automation/dist/index.js"]) {
    const fixture = runnerAutomationBundleEvidenceFixture();
    fixture.measurement.measuredFiles = fixture.measurement.measuredFiles.filter((entry) => entry.path !== path);
    fixture.context.inventory.entries = fixture.context.inventory.entries.filter((entry) => entry.path !== path);
    refreshRunnerAutomationBundleEvidence(fixture);
    assert.throws(() => runnerAutomationBundleDigest(fixture), /bundle is incomplete/);
  }
});

function paxRecord(key, value) {
  const payload = Buffer.concat([Buffer.from(`${key}=`), Buffer.from(value), Buffer.from("\n")]);
  let length = payload.length + 2;
  for (;;) {
    const next = payload.length + String(length).length + 1;
    if (next === length) return Buffer.concat([Buffer.from(`${length} `), payload]);
    length = next;
  }
}

for (const [kind, key, value] of [
  ["capability metadata", "SCHILY.xattr.security.capability", "synthetic-only"],
]) {
  test(`OCI layer rejects unsupported PAX ${kind} before runtime admission`, async (t) => {
    const directory = await temporaryDirectory(t, "pax-denial");
    const archive = join(directory, "synthetic-pax.oci.tar");
    const body = paxRecord(key, value);
    const layer = tarArchive([
      { name: "PaxHeaders/synthetic", type: "x", body },
      { name: "synthetic", body: "synthetic-only\n" },
    ]);
    // Actual retained Phase646 bytes contain capability metadata. It remains
    // unsupported; path support must never silently discard security semantics.
    await writeOciFixture(archive, { rawLayers: [layer] });
    await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"),
      (error) => {
        assert.equal(error.message, "Tar archive contains unsupported entry type x");
        const detail = validateTarRejectionDiagnostic(error.tarDiagnostic);
        assert.equal(detail.scope, "layer");
        assert.equal(detail.layerDigest, "sha256:" + sha256(layer));
        assert.equal(detail.layerSizeBytes, layer.length);
        assert.equal(detail.layerOrdinal, 1);
        assert.equal(detail.headerOrdinal, 1);
        assert.equal(detail.paxState, "parsed");
        assert.deepEqual(detail.paxKnownKeys, [key]);
        return true;
      });
  });
}

for (const scope of ["layout", "layer", "runtime-bundle"]) {
  test(`unsupported tar diagnostic identifies ${scope} without admitting PAX`, async (t) => {
    const directory = await temporaryDirectory(t, "pax-scope");
    const archive = join(directory, "synthetic.tar");
    const name = "PaxHeaders/private-secret-sentinel";
    const body = paxRecord("path", "private-secret-sentinel/application");
    const bytes = tarArchive([{ name: "earlier", body: "earlier\n" }, { name, type: "x", body }]);
    if (scope === "layer") {
      await writeOciFixture(archive, { rawLayers: [tarArchive([{ name: "first", body: "first\n" }]), bytes] });
    } else await writeFile(archive, bytes);
    const inspect = scope === "runtime-bundle"
      ? () => inspectStaticBundleArchive(archive)
      : () => inspectOciImageArchive(archive, "jobs-api");
    await assert.rejects(inspect, (error) => {
      assert.equal(error.message, "Tar archive contains unsupported entry type x");
      const detail = validateTarRejectionDiagnostic(error.tarDiagnostic);
      assert.equal(detail.scope, scope);
      assert.equal(detail.headerOrdinal, 2);
      assert.equal(detail.headerNameSha256, sha256(Buffer.from(name)));
      assert.equal(detail.headerNameBytes, Buffer.byteLength(name));
      assert.equal(detail.paxBodyBytes, body.length);
      assert.equal(detail.paxBodySha256, sha256(body));
      assert.equal(detail.pathOverride, true);
      if (scope === "layer") {
        assert.equal(detail.layerDigest, "sha256:" + sha256(bytes));
        assert.equal(detail.layerSizeBytes, bytes.length);
        assert.equal(detail.layerOrdinal, 2);
      } else {
        assert.equal(detail.layerDigest, null); assert.equal(detail.layerSizeBytes, null);
        assert.equal(detail.layerOrdinal, null);
      }
      assert.equal(JSON.stringify(detail).includes("private-secret-sentinel"), false);
      assert.ok(canonicalJsonBytes(detail).length <= 1536);
      return true;
    });
  });
}

const diagnosticPaxBodies = [
  ["unknown secret key and value", paxRecord("private-secret-key", "private-secret-value"), "parsed"],
  ["duplicate unknown keys", Buffer.concat([paxRecord("private-secret-key", "one"),
    paxRecord("private-secret-key", "two")]), "parsed"],
  ["canonical record cap", Buffer.concat(Array.from({ length: 128 }, () => paxRecord("uid", "1"))), "parsed"],
  ["record cap exceeded", Buffer.concat(Array.from({ length: 129 }, () => paxRecord("uid", "1"))), "malformed"],
  ["leading-zero length", Buffer.from("011 path=x\n"), "malformed"],
  ["signed length", Buffer.from("+10 path=x\n"), "malformed"],
  ["missing equal delimiter", Buffer.from("10 path x\n"), "malformed"],
  ["missing newline", Buffer.from("10 path=x!"), "malformed"],
  ["trailing bytes", Buffer.concat([paxRecord("path", "x"), Buffer.from("secret")]), "malformed"],
  ["embedded key control", paxRecord("private\nsecret", "value"), "malformed"],
  ["length out of body", Buffer.from("65536 path=private-secret-value\n"), "malformed"],
  ["invalid UTF8", Buffer.from([49, 49, 32, 112, 97, 116, 104, 61, 255, 10]), "invalid-utf8"],
  ["bounded large value", paxRecord("path", "private-secret-value".repeat(3000)), "parsed"],
  ["exact 64KiB body", paxRecord("path", "x".repeat(65524)), "parsed"],
  ["oversize body", Buffer.alloc(64 * 1024 + 1, 120), "oversize"],
];
for (const [name, body, state] of diagnosticPaxBodies) {
  test(`PAX diagnostic ${name} remains a redacted unsupported-type rejection`, async (t) => {
    const directory = await temporaryDirectory(t, "pax-body");
    const archive = join(directory, "synthetic.tar");
    await writeFile(archive, tarArchive([{ name: "PaxHeaders/private-secret-header", type: "x", body }]));
    await assert.rejects(() => inspectStaticBundleArchive(archive), (error) => {
      assert.equal(error.message, "Tar archive contains unsupported entry type x");
      const detail = validateTarRejectionDiagnostic(error.tarDiagnostic);
      assert.equal(detail.paxState, state); assert.equal(detail.paxBodyBytes, body.length);
      assert.equal(detail.paxBodySha256, state === "oversize" ? null : sha256(body));
      if (state !== "parsed") {
        assert.equal(detail.paxRecordCount, null); assert.deepEqual(detail.paxKnownKeys, []);
        assert.equal(detail.paxUnknownKeyCount, null); assert.equal(detail.paxKeySetSha256, null);
        assert.equal(detail.pathOverride, false); assert.equal(detail.ownershipOverride, false);
      } else if (name.includes("unknown")) {
        assert.equal(detail.paxUnknownKeyCount, 1);
        assert.equal(detail.paxKeySetSha256, sha256(canonicalJsonBytes(["private-secret-key"])));
      }
      assert.equal(JSON.stringify(detail).includes("private-secret"), false);
      assert.ok(canonicalJsonBytes(detail).length <= 1536);
      return true;
    });
  });
}

test("PAX diagnostics cover fixed override classes and bounded known-key output only", async (t) => {
  const keys = ["path", "linkpath", "size", "uid", "gid", "mtime", "atime", "ctime",
    "SCHILY.xattr.security.capability", "SCHILY.xattr.security.selinux",
    "SCHILY.xattr.trusted.overlay.opaque", "SCHILY.xattr.user.overlay.opaque",
    "GNU.sparse.map", "GNU.sparse.size", "GNU.sparse.realsize", "GNU.sparse.name",
    "GNU.sparse.major", "GNU.sparse.minor", "GNU.sparse.numblocks",
    "GNU.sparse.offset", "GNU.sparse.numbytes", "SCHILY.realsize"];
  const directory = await temporaryDirectory(t, "pax-keys"), archive = join(directory, "synthetic.tar");
  await writeFile(archive, tarArchive([{ name: "PaxHeaders/synthetic", type: "g",
    body: Buffer.concat(keys.map((key) => paxRecord(key, "private-secret-value"))) }]));
  await assert.rejects(() => inspectStaticBundleArchive(archive), (error) => {
    assert.equal(error.message, "Tar archive contains unsupported entry type g");
    const detail = validateTarRejectionDiagnostic(error.tarDiagnostic);
    assert.deepEqual(detail.paxKnownKeys, [...keys].sort());
    assert.equal(detail.paxUnknownKeyCount, 0); assert.equal(detail.paxRecordCount, keys.length);
    for (const key of ["pathOverride", "linkOverride", "sizeOverride", "ownershipOverride",
      "timeOverride", "xattrOverride", "sparseOverride"]) assert.equal(detail[key], true);
    assert.ok(canonicalJsonBytes(detail).length <= 1536);
    assert.equal(JSON.stringify(detail).includes("private-secret-value"), false);
    return true;
  });
});

test("truncated PAX bodies and other unsupported types preserve the exact original rejection", async (t) => {
  const directory = await temporaryDirectory(t, "pax-truncated");
  const truncated = tarArchive([{ name: "PaxHeaders/synthetic", type: "x", body: paxRecord("path", "value") }]);
  truncated.write(octal(4096, 12), 124); truncated.fill(32, 148, 156);
  let checksum = 0; for (const byte of truncated.subarray(0, 512)) checksum += byte;
  truncated.write(octal(checksum, 8), 148, 8, "ascii");
  for (const [type, bytes, state] of [["x", truncated, "out-of-bounds"],
    ["L", tarArchive([{ name: "synthetic", type: "L" }]), "not-pax"]]) {
    const archive = join(directory, `synthetic-${type}.tar`); await writeFile(archive, bytes);
    await assert.rejects(() => inspectStaticBundleArchive(archive), (error) => {
      assert.equal(error.message, "Tar archive contains unsupported entry type " + type);
      const detail = validateTarRejectionDiagnostic(error.tarDiagnostic);
      assert.equal(detail.paxState, state); assert.equal(detail.paxBodySha256, null);
      return true;
    });
  }
});

function paxLayer(entries) {
  return tarArchive([
    { name: "app", type: "5" },
    { name: "usr/local/bin/bluey-jobs-api", mode: 0o555, body: "synthetic-api\n" },
    { name: "app/.bluey/managed-cloud-runtime-measurement.json", body: "{}\n" },
    ...entries,
  ]);
}

function localPax(values, entry) {
  return [{ name: "PaxHeaders/synthetic", type: "x",
    body: Buffer.concat(Object.entries(values).map(([key, value]) => paxRecord(key, value))) }, entry];
}

test("OCI layer PAX preserves the observed UTF-8 CA regular file and absolute symlink chain", async (t) => {
  const root = await temporaryDirectory(t, "pax-ca-chain"), archive = join(root, "ca.oci.tar");
  const stem = "NetLock_Arany_=Class_Gold=_Főtanúsítvány";
  const certificate = `usr/share/ca-certificates/mozilla/${stem}.crt`;
  const pem = `etc/ssl/certs/${stem}.pem`, hashed = "etc/ssl/certs/988a38cb.0";
  await writeOciFixture(archive, { rawLayers: [paxLayer([
    ...localPax({ path: certificate }, { name: "certificate-stub", body: "synthetic-certificate\n" }),
    ...localPax({ path: pem, linkpath: "/" + certificate },
      { name: "pem-stub", type: "2", linkTarget: "placeholder" }),
    ...localPax({ linkpath: stem + ".pem" }, { name: hashed, type: "2", linkTarget: "placeholder" }),
    { name: "after-extension", body: "not-renamed\n" },
  ])] });
  for (const inspect of [inspectOciImageArchive,
    async (path, component) => (await inspectOciImageArtifact(path, component)).inventory]) {
    const inventory = await inspect(archive, "jobs-api"), byPath = new Map(inventory.entries.map(v => [v.path, v]));
    assert.equal(byPath.get(certificate).sha256, sha256(Buffer.from("synthetic-certificate\n")));
    assert.equal(byPath.get(pem).target, "/" + certificate); assert.equal(byPath.get(pem).resolvedTarget, certificate);
    assert.equal(byPath.get(hashed).target, stem + ".pem"); assert.equal(byPath.get(hashed).resolvedTarget, pem);
    assert(byPath.has("after-extension"));
    assert.equal(inventory.entries.some(v => v.path.includes("stub") || v.path.startsWith("PaxHeaders/")), false);
  }
});

test("OCI layer PAX accepts the exact bounded path value without normalization", async (t) => {
  const root = await temporaryDirectory(t, "pax-value-bound"), archive = join(root, "path.oci.tar");
  const path = Array(16).fill("x".repeat(250)).join("/") + "/" + "y".repeat(80);
  assert.equal(Buffer.byteLength(path), 4096);
  await writeOciFixture(archive, { rawLayers: [paxLayer(localPax({ path }, { name: "stub", body: "bounded" }))] });
  const inventory = await inspectOciImageArchive(archive, "jobs-api");
  assert.equal(inventory.entries.find(v => v.path === path).sha256, sha256(Buffer.from("bounded")));
});

for (const scope of ["layout", "runtime-bundle"]) {
  test(`complete local PAX names remain forbidden in ${scope}`, async (t) => {
    const root = await temporaryDirectory(t, "pax-wrong-scope"), archive = join(root, "bad.tar");
    await writeFile(archive, tarArchive(localPax({ path: "index.html" }, { name: "stub", body: "synthetic" })));
    await assert.rejects(() => scope === "layout" ? inspectOciImageArtifact(archive, "jobs-api")
      : inspectStaticBundleArchive(archive), error => {
      assert.equal(error.message, "Tar archive contains unsupported entry type x");
      assert.equal(validateTarRejectionDiagnostic(error.tarDiagnostic).scope, scope); return true;
    });
  });
}

for (const [kind, target, type, resolved] of [
  ["absolute image-root symlink", "/target", "2", "target"],
  ["relative parent symlink", "../target", "2", "target"],
  ["earlier-file hardlink", "target", "1", null],
]) {
  test(`OCI layer PAX keeps ${kind} semantics`, async (t) => {
    const root = await temporaryDirectory(t, "pax-links"), archive = join(root, "links.oci.tar");
    await writeOciFixture(archive, { rawLayers: [paxLayer([
      { name: "target", body: "target-bytes\n" },
      ...localPax({ path: "directory/alias", linkpath: target },
        { name: "stub", type, linkTarget: "target" }),
    ])] });
    const inventory = await inspectOciImageArchive(archive, "jobs-api");
    const entry = inventory.entries.find(v => v.path === "directory/alias");
    if (resolved) assert.equal(entry.resolvedTarget, resolved);
    else { assert.equal(entry.type, "file"); assert.equal(entry.sha256, sha256(Buffer.from("target-bytes\n"))); }
  });
}

test("OCI PAX path-only override resolves the existing parent-relative symlink at its effective path", async (t) => {
  const root = await temporaryDirectory(t, "pax-path-only-link"), archive = join(root, "link.oci.tar");
  await writeOciFixture(archive, { rawLayers: [paxLayer([
    { name: "target", body: "target" },
    ...localPax({ path: "directory/alias" }, { name: "stub", type: "2", linkTarget: "../target" }),
  ])] });
  const inventory = await inspectOciImageArchive(archive, "jobs-api");
  const link = inventory.entries.find(v => v.path === "directory/alias");
  assert.equal(link.target, "../target"); assert.equal(link.resolvedTarget, "target");
});

test("OCI layer PAX effective paths drive ordinary and opaque whiteouts", async (t) => {
  const root = await temporaryDirectory(t, "pax-whiteouts"), archive = join(root, "whiteouts.oci.tar");
  await writeOciFixture(archive, { rawLayers: [paxLayer([
    { name: "old/leaf", body: "old" }, { name: "directory/child", body: "old" },
  ]), tarArchive([
    ...localPax({ path: "old/.wh.leaf" }, { name: "whiteout-stub" }),
    ...localPax({ path: "directory/.wh..wh..opq" }, { name: "opaque-stub" }),
    { name: "directory/current", body: "current" },
  ])] });
  const paths = (await inspectOciImageArchive(archive, "jobs-api")).entries.map(v => v.path);
  assert(!paths.includes("old/leaf") && !paths.includes("directory/child"));
  assert(paths.includes("directory/current")); assert(!paths.some(v => v.includes("stub") || v.includes(".wh.")));
});

const paxSemanticRejections = [
  ["duplicate effective entry", [
    ...localPax({ path: "same" }, { name: "first-stub" }),
    ...localPax({ path: "same" }, { name: "second-stub" }),
  ], /duplicate path/],
  ["missing symbolic target", localPax({ linkpath: "missing" },
    { name: "alias", type: "2", linkTarget: "placeholder" }), /target is missing/],
  ["symbolic cycle", [
    ...localPax({ linkpath: "b" }, { name: "a", type: "2", linkTarget: "placeholder" }),
    ...localPax({ linkpath: "a" }, { name: "b", type: "2", linkTarget: "placeholder" }),
  ], /contains a cycle/],
  ["escaping relative symlink", localPax({ linkpath: "../escape" },
    { name: "alias", type: "2", linkTarget: "placeholder" }), /escapes the image root/],
  ["absolute hardlink", [{ name: "target", body: "bytes" },
    ...localPax({ linkpath: "/target" }, { name: "alias", type: "1", linkTarget: "target" })], /canonical relative path/],
  ["parent hardlink", localPax({ linkpath: "../target" },
    { name: "alias", type: "1", linkTarget: "target" }), /canonical relative path/],
  ["forward hardlink", [...localPax({ linkpath: "target" },
    { name: "alias", type: "1", linkTarget: "target" }), { name: "target", body: "later" }], /earlier regular file/],
  ["non-directory ancestor", [
    ...localPax({ path: "parent" }, { name: "parent-stub", body: "file" }),
    ...localPax({ path: "parent/child" }, { name: "child-stub" }),
  ], /non-directory ancestor/],
  ["symlink whiteout", localPax({ path: ".wh.target", linkpath: "target" },
    { name: "stub", type: "2", linkTarget: "target" }), /whiteout must be an empty regular file/],
  ["nonempty whiteout", localPax({ path: ".wh.target" }, { name: "stub", body: "hidden" }),
    /whiteout must be an empty regular file/],
];
for (const [name, entries, error] of paxSemanticRejections) {
  test(`OCI layer PAX rejects ${name}`, async (t) => {
    const root = await temporaryDirectory(t, "pax-semantic"), archive = join(root, "bad.oci.tar");
    await writeOciFixture(archive, { rawLayers: [paxLayer(entries)] });
    await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"), error);
  });
}

for (const path of ["/absolute", "../escape", "safe/../escape", "safe//file", "./file", "safe\\file"]) {
  test(`OCI layer PAX rejects noncanonical entry path ${JSON.stringify(path)}`, async (t) => {
    const root = await temporaryDirectory(t, "pax-path"), archive = join(root, "bad.oci.tar");
    await writeOciFixture(archive, { rawLayers: [paxLayer(localPax({ path }, { name: "safe-stub" }))] });
    await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"), /canonical relative path/);
  });
}

const malformedLocalPax = [
  ["empty body", Buffer.alloc(0)],
  ["empty path", paxRecord("path", "")],
  ["duplicate path", Buffer.concat([paxRecord("path", "one"), paxRecord("path", "two")])],
  ["duplicate linkpath", Buffer.concat([paxRecord("linkpath", "one"), paxRecord("linkpath", "two")])],
  ["unknown key", paxRecord("unknown", "private-value")],
  ...["size", "uid", "gid", "mtime", "atime", "ctime", "GNU.sparse.map", "SCHILY.xattr.user.test",
    "SCHILY.xattr.security.capability", "LIBARCHIVE.xattr.security.capability"]
    .map(key => [key + " override", Buffer.concat([paxRecord("path", "safe"), paxRecord(key, "1")])]),
  ["binary capability value", paxRecord("SCHILY.xattr.security.capability", Buffer.alloc(20, 255))],
  ["invalid UTF8", paxRecord("path", Buffer.from([255]))],
  ["NFD path", paxRecord("path", "certificate/Cafe\u0301.crt")],
  ["NFD linkpath", paxRecord("linkpath", "Cafe\u0301.crt")],
  ["NUL value", paxRecord("path", "safe\0hidden")],
  ["newline value", paxRecord("path", "safe\nhidden")],
  ["DEL value", paxRecord("path", "safe\x7fhidden")],
  ["C1 control value", paxRecord("path", "safe\u0085hidden")],
  ["empty linkpath", paxRecord("linkpath", "")],
  ["oversize value", paxRecord("path", "x".repeat(4097))],
  ["oversize UTF8 bytes", paxRecord("path", "ő".repeat(2049))],
  ["oversize body", Buffer.alloc(65537, 120)],
  ["leading-zero length", Buffer.from("011 path=x\n")],
  ["signed length", Buffer.from("+10 path=x\n")],
  ["missing delimiter", Buffer.from("10 path x\n")],
  ["missing newline", Buffer.from("10 path=x!")],
  ["declared length outside body", Buffer.from("65536 path=x\n")],
  ["trailing bytes", Buffer.concat([paxRecord("path", "safe"), Buffer.from("tail")])],
  ["high-bit decimal byte", (() => { const body = paxRecord("path", "safe"); body[0] |= 128; return body; })()],
  ["character-count length", (() => {
    const body = paxRecord("path", "certificate/ő.crt"); body.write(String(body.length - 1)); return body;
  })()],
];
for (const [name, body] of malformedLocalPax) {
  test(`OCI local PAX rejects ${name} with bounded redacted evidence`, async (t) => {
    const root = await temporaryDirectory(t, "pax-invalid"), archive = join(root, "bad.oci.tar");
    await writeOciFixture(archive, { rawLayers: [paxLayer([
      { name: "PaxHeaders/private-sentinel", type: "x", body }, { name: "safe-stub" },
    ])] });
    await assert.rejects(() => inspectOciImageArtifact(archive, "jobs-api"), error => {
      assert.equal(error.message, "Tar archive contains unsupported entry type x");
      const detail = validateTarRejectionDiagnostic(error.tarDiagnostic);
      assert.equal(detail.scope, "layer"); assert.equal(detail.headerOrdinal, 4);
      assert(!JSON.stringify(detail).includes("private-")); assert(canonicalJsonBytes(detail).length <= 1536);
      return true;
    });
  });
}

for (const type of ["0", "5"]) {
  test(`OCI layer PAX rejects linkpath on non-link type ${type}`, async (t) => {
    const root = await temporaryDirectory(t, "pax-nonlink"), archive = join(root, "bad.oci.tar");
    await writeOciFixture(archive, { rawLayers: [paxLayer(localPax({ linkpath: "target" }, { name: "stub", type }))] });
    await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"), /unsupported entry type x/);
  });
}

for (const [name, entries] of [
  ["consecutive local headers", [{ name: "PaxHeaders/one", type: "x", body: paxRecord("path", "one") },
    ...localPax({ path: "two" }, { name: "stub" })]],
  ["dangling local header", [{ name: "PaxHeaders/dangling", type: "x", body: paxRecord("path", "one") }]],
  ...["g", "L", "K", "3", "4", "6"].map(type => ["pending then type " + type, [
    { name: "PaxHeaders/one", type: "x", body: paxRecord("path", "one") }, { name: "unsupported", type }]]),
  ["global header", [{ name: "PaxHeaders/global", type: "g", body: paxRecord("path", "one") }, { name: "stub" }]],
]) {
  test(`OCI layer PAX rejects ${name}`, async (t) => {
    const root = await temporaryDirectory(t, "pax-scope"), archive = join(root, "bad.oci.tar");
    await writeOciFixture(archive, { rawLayers: [paxLayer(entries)] });
    await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"), /unsupported entry type/);
  });
}

function rewriteTarChecksum(bytes, at) {
  bytes.fill(32, at + 148, at + 156);
  let sum = 0; for (const byte of bytes.subarray(at, at + 512)) sum += byte;
  bytes.write(octal(sum, 8), at + 148, 8, "ascii");
}

for (const [name, mutate, error] of [
  ["extension checksum", bytes => { bytes[0] ^= 1; }, /checksum/],
  ["extension padding", bytes => { bytes[512 + paxRecord("path", "effective").length] = 1; }, /padding/],
  ["payload checksum", bytes => { bytes[1024] ^= 1; }, /checksum/],
  ["payload padding", bytes => { bytes[1537] = 1; }, /padding/],
  ["payload invalid UTF8", bytes => { bytes[1024] = 255; rewriteTarChecksum(bytes, 1024); }, /UTF-8/],
  ["payload escaping raw path", bytes => {
    bytes.fill(0, 1024, 1124); bytes.write("../escape", 1024); rewriteTarChecksum(bytes, 1024);
  }, /canonical relative path/],
  ["payload hidden name bytes", bytes => { bytes[1024 + 20] = 120; rewriteTarChecksum(bytes, 1024); }, /text field padding/],
  ["payload link field on regular", bytes => { bytes.write("hidden", 1024 + 157); rewriteTarChecksum(bytes, 1024); }, /non-link entry/],
  ["payload nonzero symlink size", bytes => {
    bytes[1024 + 156] = 50; bytes.write("target", 1024 + 157); rewriteTarChecksum(bytes, 1024);
  }, /Non-file tar entries/],
  ["payload size outside layer", bytes => { bytes.write(octal(65536, 12), 1024 + 124); rewriteTarChecksum(bytes, 1024); }, /truncated/],
]) {
  test(`OCI PAX cannot hide ${name}`, async (t) => {
    const root = await temporaryDirectory(t, "pax-header"), archive = join(root, "bad.oci.tar");
    const bytes = tarArchive(localPax({ path: "effective" }, { name: "stub", body: "x" })); mutate(bytes);
    await writeOciFixture(archive, { rawLayers: [bytes] });
    await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"), error);
  });
}

test("OCI PAX cumulative metadata cap is shared across layers", async (t) => {
  const root = await temporaryDirectory(t, "pax-total"), archive = join(root, "bad.oci.tar");
  const layer = batch => paxLayer(Array.from({ length: 1100 }, (_, i) => localPax({
    path: `metadata-${batch}/${i}/` + ("x".repeat(250) + "/").repeat(15) + "x".repeat(250),
  }, { name: "stub-" + i })).flat());
  await writeOciFixture(archive, { rawLayers: [layer(0), layer(1)] });
  await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"), /unsupported entry type x/);
});

test("OCI physical header cap counts across layers independently of surviving inventory", async (t) => {
  const root = await temporaryDirectory(t, "pax-headers"), archive = join(root, "bad.oci.tar");
  const entries = Array.from({ length: 50_000 }, (_, i) => ({ name: "header-" + i, type: "5" }));
  await writeOciFixture(archive, { rawLayers: [tarArchive(entries), tarArchive(entries.slice(0, -1).reverse()),
    tarArchive(localPax({ path: "effective" }, { name: "stub" }))] });
  await assert.rejects(() => inspectOciImageArchive(archive, "jobs-api"), /too many physical headers/);
});

const escapedSystemdUnits = [
  "usr/lib/systemd/system/system-systemd\\x2dcryptsetup.slice",
  "usr/lib/systemd/system/system-systemd\\x2dveritysetup.slice",
];

test("runner producer removes the observed PTP helper and two exact escaped units before measurement and flattening", async () => {
  const dockerfile = await readFile(new URL("../runner/Dockerfile", import.meta.url), "utf8");
  let prior = dockerfile.indexOf("AS runtime-rootfs");
  const argumentsToRemove = ["/usr/lib/x86_64-linux-gnu/gstreamer1.0/gstreamer-1.0/gst-ptp-helper",
    ...escapedSystemdUnits.map(path => "'/" + path + "'")];
  for (const path of argumentsToRemove) {
    const required = [`test -f ${path}`, `! test -L ${path}`, `rm -- ${path}`, `! test -e ${path}`, `! test -L ${path}`];
    for (const token of required) { const at = dockerfile.indexOf(token, prior + 1); assert(at > prior); prior = at; }
  }
  const unitRemovals = dockerfile.split("\n").filter(line => line.includes("rm --")
    && line.includes("/usr/lib/systemd/")).map(line => line.trim());
  assert.deepEqual(unitRemovals, escapedSystemdUnits.map(path => `&& rm -- '/${path}' ` + "\\"));
  assert(prior < dockerfile.indexOf(" runtime-measurement ", prior));
  assert(prior < dockerfile.indexOf("FROM scratch", prior));
  assert(!/rm[^\n]*(?:ca-certificates|ssl\/certs)/.test(dockerfile));
});

test("both OCI builders carry the exact measurement module closure before cleanup", async (t) => {
  const root = await temporaryDirectory(t, "measurement-image-inputs");
  const gate = "managed-cloud-release-gate.mjs";
  const helpers = [
    "check-managed-cloud-authorization-origin.mjs",
    "check-managed-cloud-observability-policy.mjs",
  ];
  const modules = [gate, ...helpers];
  const copy = [
    "COPY scripts/" + gate + " " + "\\",
    "    scripts/" + helpers[0] + " " + "\\",
    "    scripts/" + helpers[1] + " /tmp/",
  ].join("\n");
  const ignore = await readFile(new URL("../.dockerignore", import.meta.url), "utf8");
  assert.deepEqual(
    ignore.split("\n").filter((line) => line.startsWith("!scripts/")).sort(),
    ["!scripts/", ...modules.map((name) => "!scripts/" + name)].sort(),
  );
  for (const [name, invoke, roles, paths] of [
    ["runner", "/usr/local/bin/node", "managed_runner", [
      "app/automation/dist/index.js",
      "app/runner/dist/native/bluey_jobs_runner_native_storage.node",
      "app/runner/dist/server.js",
      "ms-playwright/chromium_headless_shell-1/linux-x64/chrome-headless-shell",
      "usr/local/bin/node",
    ]],
    ["workflows", "node", "discovery_worker,global_discovery_worker,original_source_verifier,workflow_gateway,workflow_worker", [
      "app/automation/dist/index.js",
      "app/workflows/dist/discovery-worker.js",
      "app/workflows/dist/failure-converter.js",
      "app/workflows/dist/gateway.js",
      "app/workflows/dist/global-discovery-worker.js",
      "app/workflows/dist/original-source-verifier.js",
      "app/workflows/dist/worker.js",
      "usr/local/bin/node",
    ]],
  ]) {
    const dockerfile = await readFile(new URL("../" + name + "/Dockerfile", import.meta.url), "utf8");
    const begin = dockerfile.indexOf("AS runtime-rootfs");
    const copyAt = dockerfile.indexOf(copy, begin);
    const measureAt = dockerfile.indexOf(invoke + " /tmp/" + gate + " runtime-measurement", copyAt);
    const cleanupAt = dockerfile.indexOf("/tmp/*", measureAt);
    const flattenAt = dockerfile.indexOf("FROM scratch", cleanupAt);
    assert(begin >= 0 && copyAt > begin && measureAt > copyAt && cleanupAt > measureAt && flattenAt > cleanupAt);
    assert.equal(dockerfile.split(copy).length - 1, 1);

    const runtimeRoot = join(root, name, "rootfs");
    const synthetic = "owned measurement-only fixture\n";
    for (const path of paths) {
      const dest = join(runtimeRoot, path);
      await mkdir(dirname(dest), { recursive: true });
      await writeFile(dest, synthetic);
    }
    const common = [
      "runtime-measurement", "--root", runtimeRoot, "--component", "jobs-" + name,
      "--build-id", "fixture-jobs-" + name, "--config-schema-sha256", "c".repeat(64),
      "--migration-set-sha256", "d".repeat(64), "--protocol-set-sha256", "e".repeat(64),
      "--roles", roles, "--source-commit", SOURCE_COMMIT,
    ];
    for (const [caseName, omitted] of [
      ["exact", []], ["original-gate-only", helpers],
      ...helpers.map((helper) => [helper, [helper]]),
    ]) {
      const location = join(root, name, caseName);
      await mkdir(location);
      for (const module of modules.filter((item) => !omitted.includes(item))) {
        await copyFile(join(import.meta.dirname, module), join(location, module));
      }
      const output = join(location, "measurement.json");
      const command = () => execFileAsync(process.execPath, [
        join(location, gate), ...common, "--out", output,
      ]);
      if (omitted.length > 0) {
        await assert.rejects(command, (error) =>
          error.code === 1 && error.stderr.includes("ERR_MODULE_NOT_FOUND") &&
          omitted.some((helper) => error.stderr.includes(helper)) && error.stdout === "");
        await assert.rejects(() => access(output), /ENOENT/);
      } else {
        await command();
        const measurement = JSON.parse(await readFile(output, "utf8"));
        assert.equal(measurement.componentId, "jobs-" + name);
        assert.equal(measurement.sourceCommit, SOURCE_COMMIT);
        assert.deepEqual(measurement.roles, roles.split(","));
        assert.deepEqual(measurement.measuredFiles,
          paths.map((path) => ({ path, sha256: sha256(synthetic) })));
      }
    }
  }
});

for (const format of ["raw", "PAX"]) {
  test(`runner OCI inventory rejects ${format} backslash systemd unit entries`, async (t) => {
    const root = await temporaryDirectory(t, "systemd-path-denial");
    for (const [index, path] of escapedSystemdUnits.entries()) {
      const archive = join(root, `runner-${index}.oci.tar`);
      const entries = format === "raw" ? [{ name: path, body: "synthetic-unit\n" }]
        : localPax({ path }, { name: "systemd-stub", body: "synthetic-unit\n" });
      await writeOciFixture(archive, { runtime: runnerRuntime(), rawLayers: [paxLayer(entries)] });
      for (const inspect of [inspectOciImageArchive, inspectOciImageArtifact]) {
        await assert.rejects(() => inspect(archive, "jobs-runner"), /tar path must be a canonical relative path/);
      }
    }
  });
}

test("runner final OCI inventory rejects reintroduced PTP helper without requiring PAX", async (t) => {
  const root = await temporaryDirectory(t, "ptp-denial"), archive = join(root, "runner.oci.tar");
  const required = MANAGED_CLOUD_RUNTIME_CONTRACTS["jobs-runner"].requiredPaths
    .map(name => ({ name, body: "synthetic-only\n", mode: 0o555 }));
  await writeOciFixture(archive, { runtime: runnerRuntime(), layers: [[...required,
    { name: "usr/lib/x86_64-linux-gnu/gstreamer1.0/gstreamer-1.0/gst-ptp-helper", body: "synthetic-helper" }]] });
  for (const inspect of [inspectOciImageArchive, inspectOciImageArtifact]) {
    await assert.rejects(() => inspect(archive, "jobs-runner"), /unused privileged PTP helper/);
  }
});

test("runner bundle rejects duplicate, unsorted, escaping or unmeasured inventory entries", () => {
  for (const mutate of [
    (f) => { f.context.inventory.entries.push({ ...f.context.inventory.entries[1] }); },
    (f) => { f.context.inventory.entries.reverse(); },
    (f) => { f.context.inventory.entries[1].path = "app/automation/../escape.js"; },
    (f) => { f.context.inventory.entries[1].extra = true; },
    (f) => { f.context.inventory.entries.splice(1, 0, { path: "app/automation/dist/extra.js",
      type: "file", mode: "0444", sizeBytes: 1, sha256: "9".repeat(64) }); },
  ]) {
    const fixture = runnerAutomationBundleEvidenceFixture(); mutate(fixture);
    assert.throws(() => runnerAutomationBundleDigest(fixture));
  }
});
