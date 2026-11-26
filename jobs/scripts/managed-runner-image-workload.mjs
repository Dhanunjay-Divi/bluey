// D1-only actual-image workload. The caller owns the disposable VM lifecycle.
import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, readdir, realpath, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { TextDecoder } from "node:util";
import { canonicalJsonBytes, createManagedCloudContracts }
  from "./managed-cloud-release-gate.mjs";
import { validateExpected, validateImageInspectionReport as validateInspection }
  from "./nonprod-managed-runner-image-inspect.mjs";
import { guestReaderArguments, validateReaderApproval, validateReaderProbe }
  from "./managed-runner-artifact-stream.mjs";

const BUILDKIT_IMAGE =
  "moby/buildkit@sha256:b651bf250c2805569f1db385ad1ea8fb73737dfc851de305f274ca02690fa93f";
const BUILDKIT_CONFIG_SHA256 =
  "2a2c53740b68d6a814cfed5af9f6d9a8eaec67a8546058c226a8251ad4680450";
// Docker's public `image load` contract supplies --input and --platform. Docker 29's
// containerd image store exposes the OCI manifest digest as image/container Image;
// the independent archive inspector supplies the separate image-config digest.
const OCI_IMAGE_MANIFEST_MEDIA_TYPE = "application/vnd.oci.image.manifest.v1+json";
const BUILDKIT_MANIFEST_SIZE = 2261;
const IMAGE_IDENTITY_PROTOCOL = "docker_containerd_oci_manifest_v1";
const ROOT = "/var/tmp/bluey-jobs-runner-image-p639";
const BUILDER = "p639-image";
const BUILDKIT_CONTAINER = "buildx_buildkit_p639-image0";
const CONTAINER = "p639-image-smoke";
const MODULE_DIRECTORY = dirname(fileURLToPath(import.meta.url));
const MAX_SOURCE_ARCHIVE = 64 * 1024 * 1024;
const MAX_FIXTURE_ARCHIVE = 8 * 1024 * 1024;
const MAX_REPORT = 2 * 1024 * 1024;
const EXPECTED_TREE_ENTRIES = 797;
const GIB = 1024 ** 3;
const SHA = /^(?!0{64}$)[a-f0-9]{64}$/;
const SOURCE_COMMIT = /^(?!0{40}$)[a-f0-9]{40}$/;
const PROGRESS_PHASES = Object.freeze(["inputs_prepared", "builder_started",
  "image_build_started", "image_build_finished", "image_inspection_started",
  "image_inspection_finished"]);
const check = (value, code) => { if (!value) throw new Error(code); };
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const expectedSourceCommitValue = (value) => typeof value === "string" && value.length === 40
  && SOURCE_COMMIT.test(value);
const OCI_ENVIRONMENT_NAMES = new Set(["BLUEY_JOBS_API_HOST", "BLUEY_JOBS_API_PORT",
  "BLUEY_JOBS_RUNNER_DATA", "LANG", "LC_ALL", "NODE_ENV", "NODE_VERSION", "PATH",
  "PLAYWRIGHT_BROWSERS_PATH", "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "YARN_VERSION"]);

export const selectedPaths = Object.freeze([
  "jobs", "server", "crates/cue-core", "infra", ".github", "Cargo.toml", "Cargo.lock",
  "scripts/run-bluey-tests.sh",
]);

function exactKeys(value, keys, code) {
  check(value && Object.getPrototypeOf(value) === Object.prototype, code);
  check(JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...keys].sort()), code);
  return value;
}

function selectedState(value) {
  return {
    Status: value?.Status ?? null,
    Running: value?.Running ?? null,
    ExitCode: value?.ExitCode ?? null,
    Error: value?.Error ?? null,
  };
}

export function selectEngineMetadata(value) {
  exactKeys(value, ["Architecture", "Driver", "DriverStatus", "Os", "ServerVersion"],
    "engine_metadata_shape");
  check(typeof value.ServerVersion === "string" && value.ServerVersion.length > 0
    && typeof value.Driver === "string" && value.Driver.length > 0
    && Array.isArray(value.DriverStatus) && typeof value.Os === "string" && value.Os.length > 0
    && typeof value.Architecture === "string" && value.Architecture.length > 0,
  "engine_metadata_shape");
  return Object.freeze({ ...value, DriverStatus: Object.freeze([...value.DriverStatus]) });
}

export function selectBuilderImageMetadata(value) {
  check(value && typeof value === "object", "builder_image_metadata_shape");
  return Object.freeze({
    Id: value.Id ?? null,
    Descriptor: value.Descriptor ?? null,
    RepoDigests: value.RepoDigests ?? null,
    RootFS: value.RootFS ?? null,
    Config: Object.freeze({ User: value.Config?.User ?? null }),
  });
}

export function selectBuilderContainerMetadata(value) {
  check(value && typeof value === "object", "builder_container_metadata_shape");
  return Object.freeze({
    Image: value.Image ?? null,
    ImageManifestDescriptor: value.ImageManifestDescriptor ?? null,
    State: Object.freeze(selectedState(value.State)),
    Config: Object.freeze({ Image: value.Config?.Image ?? null }),
  });
}

export function selectBuiltImageMetadata(value) {
  check(value && typeof value === "object", "built_image_metadata_shape");
  return Object.freeze({
    Id: value.Id ?? null,
    Descriptor: value.Descriptor ?? null,
    RepoDigests: value.RepoDigests ?? null,
    Os: value.Os ?? null,
    Architecture: value.Architecture ?? null,
    Config: Object.freeze({ User: value.Config?.User ?? null }),
  });
}

export function selectSmokeContainerMetadata(value) {
  check(value && typeof value === "object", "smoke_container_metadata_shape");
  const host = value.HostConfig ?? {};
  return Object.freeze({
    Image: value.Image ?? null,
    ImageManifestDescriptor: value.ImageManifestDescriptor ?? null,
    State: Object.freeze(selectedState(value.State)),
    Config: Object.freeze({ Image: value.Config?.Image ?? null, User: value.Config?.User ?? null }),
    HostConfig: Object.freeze({
      NetworkMode: host.NetworkMode ?? null,
      ReadonlyRootfs: host.ReadonlyRootfs ?? null,
      Privileged: host.Privileged ?? null,
      CapAdd: host.CapAdd ?? null,
      CapDrop: host.CapDrop ?? null,
      IpcMode: host.IpcMode ?? null,
      PidMode: host.PidMode ?? null,
      Devices: host.Devices ?? null,
      DeviceRequests: host.DeviceRequests ?? null,
      DeviceCgroupRules: host.DeviceCgroupRules ?? null,
      SecurityOpt: host.SecurityOpt ?? null,
      PublishAllPorts: host.PublishAllPorts ?? null,
      PortBindings: host.PortBindings ?? null,
      PidsLimit: host.PidsLimit ?? null,
      Memory: host.Memory ?? null,
      NanoCpus: host.NanoCpus ?? null,
      Tmpfs: host.Tmpfs ?? null,
    }),
    Mounts: Object.freeze((value.Mounts ?? []).map((mount) => Object.freeze({
      Type: mount.Type ?? null,
      Source: mount.Source ?? null,
      Destination: mount.Destination ?? null,
      RW: mount.RW ?? null,
    }))),
  });
}

export async function flushStagedImageEvidence(diagnostic, evidence) {
  check(typeof diagnostic === "function" && Array.isArray(evidence) && evidence.length > 0,
    "staged_evidence_shape");
  const bytes = canonicalJsonBytes({ evidence, version: 1 });
  check(bytes.length > 0 && bytes.length <= 64 * 1024, "staged_evidence_bound");
  await diagnostic(`Managed runner image workload evidence:\n${bytes.toString("utf8")}`);
  return Object.freeze({ bytes: bytes.length, sha256: sha256(bytes) });
}

async function emitPrivateDiagnostic(diagnostic, label, text) {
  const message = `${label}\n${text}`;
  check(Buffer.byteLength(message, "utf8") <= 64 * 1024, "private_diagnostic_bound");
  try { await diagnostic(message); }
  catch (error) { throw new Error("image_workload_diagnostic_failed", { cause: error }); }
}

function secretPath(path) {
  const parts = path.split("/");
  const name = parts.at(-1).toLowerCase();
  const environmentSecret = parts.some((part) => part === ".env"
    || (/^\.env\./i.test(part) && !/^\.env\.(?:example|sample|template)$/i.test(part)));
  return environmentSecret
    || /^(?:\.?credentials?|service-account)(?:\.(?:json|ya?ml|toml|ini|conf))?$/i.test(name)
    || /^(?:id_rsa|id_dsa|id_ecdsa|id_ed25519)(?:\.|$)/i.test(name)
    || /\.(?:key|pem|p12|pfx)$/i.test(name);
}

export function parseSelectedGitTree(bytes, expectedCount = EXPECTED_TREE_ENTRIES) {
  check(Buffer.isBuffer(bytes) && bytes.length > 0 && bytes.length <= MAX_SOURCE_ARCHIVE,
    "source_tree_bound");
  let text;
  try { text = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
  catch { throw new Error("source_tree_encoding"); }
  check(text.endsWith("\0"), "source_tree_termination");
  const records = text.slice(0, -1).split("\0");
  check(records.length === expectedCount, "source_tree_count");
  let previous = "";
  const paths = [];
  for (const record of records) {
    const match = /^(100644|100755) blob ([a-f0-9]{40})\t([^\0]+)$/.exec(record);
    check(match, "source_tree_entry_invalid");
    const path = match[3];
    const parts = path.split("/");
    check(!path.startsWith("/") && !path.includes("\\") && path === path.normalize("NFC")
      && !parts.some((part) => !part || part === "." || part === "..")
      && selectedPaths.some((selected) => path === selected || path.startsWith(`${selected}/`))
      && !secretPath(path), "source_tree_path_invalid");
    check(Buffer.compare(Buffer.from(previous), Buffer.from(path)) < 0,
      "source_tree_order_invalid");
    previous = path;
    paths.push(path);
  }
  return Object.freeze({ count: records.length, paths: Object.freeze(paths) });
}

export function dockerEnvironment() {
  return ["env", "-i", `HOME=${ROOT}/home`,
    "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    `DOCKER_CONFIG=${ROOT}/docker-config`, "docker"];
}

export function buildArguments(expected, epoch) {
  validateExpected(expected);
  check(/^nonprod-p639-[a-f0-9]{12}$/.test(expected.buildId)
    && /^[1-9][0-9]{8,11}$/.test(String(epoch)), "build_identity");
  const values = [
    ["SOURCE_DATE_EPOCH", String(epoch)],
    ["BLUEY_JOBS_BUILD_ID", expected.buildId],
    ["BLUEY_JOBS_SOURCE_COMMIT", expected.sourceCommit],
    ["BLUEY_JOBS_CONFIG_SCHEMA_SHA256", expected.configSchemaSha256],
    ["BLUEY_JOBS_MIGRATION_SET_SHA256", expected.migrationSetSha256],
    ["BLUEY_JOBS_PROTOCOL_SET_SHA256", expected.protocolSetSha256],
  ];
  check(values.every(([, value]) => value && !/^0+$/.test(value)), "build_argument_zero");
  return ["buildx", "build", "--builder", BUILDER, "--platform", "linux/amd64",
    "--provenance=false", "--sbom=false", "--output",
    `type=oci,dest=${ROOT}/fixture/runner.oci.tar,compression=uncompressed,force-compression=true`,
    ...values.flatMap(([name, value]) => ["--build-arg", `${name}=${value}`]),
    "-f", `${ROOT}/source/jobs/runner/Dockerfile`, `${ROOT}/source/jobs`];
}

export function validateDiskSpace(text) {
  check(typeof text === "string" && Buffer.byteLength(text) <= 8192, "disk_space_output");
  const rows = text.trim().split("\n").slice(1).map((line) => line.trim().split(/\s+/));
  check(rows.length === 3 && rows.every((row) => row.length === 6), "disk_space_shape");
  check(rows[0][0] !== rows[1][0] && rows[0].at(-1) === "/"
    && rows[1].at(-1) === "/var/lib/docker" && rows[2].at(-1) === "/var/lib/containerd"
    && rows[1][0] === rows[2][0] && rows[1][1] === rows[2][1], "disk_space_topology");
  check(rows.every((row) => row.slice(1, 4).every((value) => /^[0-9]+$/.test(value))),
    "disk_space_numbers");
  const totals = rows.map((row) => Number(row[1]) * 1024);
  const bytes = rows.map((row) => Number(row[3]) * 1024);
  check(totals.every(Number.isSafeInteger) && totals[0] > 6 * GIB && totals[0] <= 8 * GIB
    && totals[1] > 14 * GIB && totals[1] <= 16 * GIB,
  "disk_space_capacity");
  check(bytes.every((value, index) => Number.isSafeInteger(value) && value <= totals[index])
    && bytes[0] >= 4 * GIB && bytes[1] >= 12 * GIB && bytes[2] >= 12 * GIB,
    "disk_space_insufficient");
  return Object.freeze({ rootFreeBytes: bytes[0], dataFreeBytes: bytes[1],
    containerdFreeBytes: bytes[2] });
}

export function validateStorageMounts(texts) {
  check(Array.isArray(texts) && texts.length === 3, "storage_mount_shape");
  const targets = ["/", "/var/lib/docker", "/var/lib/containerd"];
  const roots = ["/", "/docker", "/containerd"];
  const mounts = texts.map((text, index) => {
    check(typeof text === "string" && Buffer.byteLength(text) <= 8192, "storage_mount_bound");
    let value;
    try { value = JSON.parse(text); } catch { throw new Error("storage_mount_json"); }
    exactKeys(value, ["filesystems"], "storage_mount_shape");
    check(Array.isArray(value.filesystems) && value.filesystems.length === 1,
      "storage_mount_shape");
    const row = value.filesystems[0];
    exactKeys(row, ["source", "target", "fstype", "fsroot", "maj:min"], "storage_mount_shape");
    check(typeof row.source === "string" && /^\/dev\/[a-zA-Z0-9/_-]+(?:\[\/[a-z]+\])?$/
      .test(row.source) && row.target === targets[index] && row.fstype === "ext4"
      && row.fsroot === roots[index] && /^[0-9]+:[0-9]+$/.test(row["maj:min"]),
    "storage_mount_identity");
    return Object.freeze({ ...row });
  });
  const device = (row) => row.source.split("[")[0];
  check(mounts[0]["maj:min"] !== mounts[1]["maj:min"]
    && mounts[1]["maj:min"] === mounts[2]["maj:min"]
    && device(mounts[0]) !== device(mounts[1]) && device(mounts[1]) === device(mounts[2]),
  "storage_mount_topology");
  return Object.freeze(mounts);
}

export function validateContainerdEngine(value) {
  selectEngineMetadata(value);
  check(/^29\.[0-9]+\.[0-9]+$/.test(value.ServerVersion) && value.Driver === "overlayfs"
    && value.Os === "linux" && value.Architecture === "x86_64"
    && JSON.stringify(value.DriverStatus) === JSON.stringify([
      ["driver-type", "io.containerd.snapshotter.v1"],
    ]), "containerd_engine_required");
  return value;
}

export function parseBuildkitDaemonVersion(text) {
  check(typeof text === "string" && Buffer.byteLength(text) <= 4096,
    "buildkit_daemon_version_output");
  const match = /^buildkitd github\.com\/moby\/buildkit (v0\.34\.1) ([a-f0-9]{40})\n$/.exec(text);
  check(match, "buildkit_daemon_version_required");
  return Object.freeze({ revision: match[2], version: match[1] });
}

function boundedAnnotations(value, code) {
  if (value === undefined) return null;
  check(value && Object.getPrototypeOf(value) === Object.prototype
    && Object.keys(value).length <= 16, code);
  for (const [name, annotation] of Object.entries(value)) {
    check(/^[A-Za-z0-9][A-Za-z0-9._/-]{0,127}$/.test(name)
      && typeof annotation === "string" && Buffer.byteLength(annotation) <= 1024, code);
  }
  return Object.freeze({ ...value });
}

function boundedPlatform(value, code) {
  if (value === undefined) return null;
  check(value && Object.getPrototypeOf(value) === Object.prototype, code);
  const allowed = new Set(["architecture", "os", "os.features", "os.version", "variant"]);
  check(Object.keys(value).every((key) => allowed.has(key))
    && value.os === "linux" && value.architecture === "amd64", code);
  for (const [name, item] of Object.entries(value)) {
    check(name === "os.features"
      ? Array.isArray(item) && item.every((feature) => typeof feature === "string"
        && Buffer.byteLength(feature) <= 128)
      : typeof item === "string" && Buffer.byteLength(item) <= 128, code);
  }
  return Object.freeze({ ...value });
}

export function parseOciArchiveIndex(text) {
  check(typeof text === "string" && Buffer.byteLength(text) > 0
    && Buffer.byteLength(text) <= 32 * 1024, "oci_index_bound");
  let value;
  try { value = JSON.parse(text); } catch { throw new Error("oci_index_json"); }
  check(value && Object.getPrototypeOf(value) === Object.prototype
    && value.schemaVersion === 2 && Array.isArray(value.manifests)
    && value.manifests.length === 1, "oci_index_shape");
  const allowedTop = new Set(["annotations", "manifests", "mediaType", "schemaVersion"]);
  check(Object.keys(value).every((key) => allowedTop.has(key)), "oci_index_shape");
  if (value.mediaType !== undefined) {
    check(value.mediaType === "application/vnd.oci.image.index.v1+json", "oci_index_media_type");
  }
  boundedAnnotations(value.annotations, "oci_index_annotations");
  const descriptor = value.manifests[0];
  check(descriptor && Object.getPrototypeOf(descriptor) === Object.prototype,
    "oci_manifest_descriptor_shape");
  const allowedDescriptor = new Set(["annotations", "digest", "mediaType", "platform", "size"]);
  check(Object.keys(descriptor).every((key) => allowedDescriptor.has(key))
    && descriptor.mediaType === OCI_IMAGE_MANIFEST_MEDIA_TYPE
    && /^sha256:(?!0{64})[a-f0-9]{64}$/.test(descriptor.digest)
    && Number.isSafeInteger(descriptor.size) && descriptor.size > 0
    && descriptor.size <= 32 * 1024 * 1024, "oci_manifest_descriptor_shape");
  const platform = boundedPlatform(descriptor.platform, "oci_manifest_platform");
  const annotations = boundedAnnotations(descriptor.annotations, "oci_manifest_annotations");
  return Object.freeze({
    annotations,
    digest: descriptor.digest,
    manifestSha256: descriptor.digest.slice(7),
    mediaType: descriptor.mediaType,
    platform,
    size: descriptor.size,
  });
}

export function parseOciConfigDescriptor(text, index) {
  check(typeof text === "string" && Buffer.byteLength(text) > 0
    && Buffer.byteLength(text) <= 128 * 1024
    && Buffer.byteLength(text) === index?.size
    && sha256(Buffer.from(text)) === index?.manifestSha256, "oci_manifest_blob_binding");
  let value;
  try { value = JSON.parse(text); } catch { throw new Error("oci_manifest_blob_json"); }
  check(value && Object.getPrototypeOf(value) === Object.prototype
    && value.schemaVersion === 2 && value.mediaType === OCI_IMAGE_MANIFEST_MEDIA_TYPE,
  "oci_manifest_blob_shape");
  const config = value.config;
  check(config && Object.getPrototypeOf(config) === Object.prototype
    && config.mediaType === "application/vnd.oci.image.config.v1+json"
    && /^sha256:(?!0{64})[a-f0-9]{64}$/.test(config.digest)
    && Number.isSafeInteger(config.size) && config.size > 0 && config.size <= 128 * 1024,
  "oci_config_descriptor_shape");
  return Object.freeze({ digest: config.digest, size: config.size,
    sha256: config.digest.slice(7) });
}

// Diagnostic shape only, never an image-admission decision. Unrecognized key
// names and all values remain private; only a fixed known-name set is emitted.
export function parseOciRuntimeShapeEvidence(text, descriptor) {
  check(typeof text === "string" && Buffer.byteLength(text) > 0
    && Buffer.byteLength(text) <= 128 * 1024
    && Buffer.byteLength(text) === descriptor?.size
    && sha256(Buffer.from(text)) === descriptor?.sha256, "oci_config_blob_binding");
  let value;
  try { value = JSON.parse(text); } catch { throw new Error("oci_config_blob_json"); }
  check(value && Object.getPrototypeOf(value) === Object.prototype
    && value.config && Object.getPrototypeOf(value.config) === Object.prototype,
  "oci_config_blob_shape");
  const runtime = value.config;
  const keys = Object.keys(runtime).sort((left, right) =>
    Buffer.compare(Buffer.from(left), Buffer.from(right)));
  check(keys.length <= 64 && keys.every((key) => Buffer.byteLength(key) > 0
    && Buffer.byteLength(key) <= 128), "oci_runtime_key_bound");
  const known = new Set(["ArgsEscaped", "Cmd", "Entrypoint", "Env", "ExposedPorts",
    "Healthcheck", "Labels", "OnBuild", "Shell", "StopSignal", "User", "Volumes",
    "WorkingDir"]);
  const argsEscapedState = !Object.hasOwn(runtime, "ArgsEscaped") ? "absent"
    : runtime.ArgsEscaped === true ? "true" : runtime.ArgsEscaped === false ? "false" : "other";
  const environment = runtime.Env;
  const envState = !Object.hasOwn(runtime, "Env") ? "absent"
    : Array.isArray(environment) ? "array" : "other";
  let envEntryCount = null;
  let envOrderedKnownNames = Object.freeze([]);
  let envUnknownOrInvalid = false;
  let envSha256 = null;
  if (Array.isArray(environment)) {
    check(environment.length <= 64 && environment.every((entry) => typeof entry === "string"
      && Buffer.byteLength(entry) <= 4096), "oci_runtime_env_bound");
    envEntryCount = environment.length;
    const names = environment.map((entry) => {
      const separator = entry.indexOf("=");
      return separator > 0 ? entry.slice(0, separator) : "";
    });
    envOrderedKnownNames = Object.freeze(names.map((name) => OCI_ENVIRONMENT_NAMES.has(name)
      ? name : "unknown_or_invalid"));
    envUnknownOrInvalid = names.some((name) => !OCI_ENVIRONMENT_NAMES.has(name));
    envSha256 = sha256(canonicalJsonBytes(environment));
  }
  return Object.freeze({ version: 1, configSha256: descriptor.sha256,
    configBytes: descriptor.size, runtimeKeySetSha256: sha256(canonicalJsonBytes(keys)),
    runtimeKeyCount: keys.length,
    knownRuntimeKeys: Object.freeze(keys.filter((key) => known.has(key))),
    unknownRuntimeKeyCount: keys.filter((key) => !known.has(key)).length, argsEscapedState,
    envState, envEntryCount, envOrderedKnownNames, envUnknownOrInvalid, envSha256 });
}

function descriptorMatches(actual, expected) {
  if (!actual || actual.digest !== expected.digest
    || actual.mediaType !== expected.mediaType || actual.size !== expected.size) return false;
  if (actual.platform !== undefined && actual.platform !== null) {
    try { boundedPlatform(actual.platform, "loaded_manifest_platform"); }
    catch { return false; }
  }
  return true;
}

export function validateLoadedManifestIdentity(image, expected) {
  check(image.Id === expected.digest && descriptorMatches(image.Descriptor, expected),
    "loaded_manifest_identity");
  return image;
}

export function validateBuildkitManifestIdentity(image, container) {
  const expected = { digest: `sha256:${BUILDKIT_IMAGE.split("@sha256:")[1]}`,
    mediaType: OCI_IMAGE_MANIFEST_MEDIA_TYPE, size: BUILDKIT_MANIFEST_SIZE };
  check(image.Id === expected.digest && descriptorMatches(image.Descriptor, expected)
    && JSON.stringify(image.RepoDigests) === JSON.stringify([BUILDKIT_IMAGE])
    && container.Image === expected.digest
    && descriptorMatches(container.ImageManifestDescriptor, expected)
    && container.State.Running === true && container.Config.Image === BUILDKIT_IMAGE,
  "buildkit_image_identity");
  return expected;
}

export async function emitWorkloadProgress(progress, phase) {
  check(typeof progress === "function" && PROGRESS_PHASES.includes(phase),
    "workload_progress_phase");
  await progress(phase);
}

export function validatePrivateDirectoryStats(text, count) {
  check(typeof text === "string" && Number.isInteger(count) && count > 0,
    "private_directory_stats");
  const rows = text.trim().split("\n");
  check(rows.length === count, "private_directory_stats");
  const identities = rows.map((row) => /^([0-9]+):([0-9]+):700:directory$/.exec(row));
  check(identities.every(Boolean)
    && identities.every((row) => row[1] === identities[0][1] && row[2] === identities[0][2]),
  "private_directory_stats");
  return true;
}

function positiveId(text, code) {
  check(typeof text === "string" && /^[1-9][0-9]{0,9}\n?$/.test(text), code);
  const value = Number(text.trim());
  check(Number.isInteger(value) && value <= 4_294_967_295, code);
  return value;
}

function validateTmpfs(value, uid, gid, size) {
  const actual = new Set(String(value).split(","));
  const expected = ["rw", "noexec", "nosuid", "nodev", `size=${size}`, `uid=${uid}`,
    `gid=${gid}`, "mode=0700"];
  check(actual.size === expected.length && expected.every((item) => actual.has(item)),
    "smoke_tmpfs");
}

export function validateSmokeContainerInspection(value, expected) {
  exactKeys(expected, ["fixture", "gid", "imageManifestDescriptor", "imageManifestId", "out",
    "uid"],
    "smoke_expected_shape");
  check(value?.Image === expected.imageManifestId
    && descriptorMatches(value.ImageManifestDescriptor, expected.imageManifestDescriptor)
    && value.State?.Status === "exited" && value.State?.ExitCode === 0,
  "smoke_image_or_exit");
  const host = value.HostConfig;
  const emptyOrNull = (item) => item === null || (Array.isArray(item) && item.length === 0);
  check(host?.NetworkMode === "none" && host.ReadonlyRootfs === true
    && host.Privileged === false && emptyOrNull(host.CapAdd)
    && host.IpcMode === "private" && host.PidMode === ""
    && emptyOrNull(host.Devices) && emptyOrNull(host.DeviceRequests)
    && emptyOrNull(host.DeviceCgroupRules)
    && host.PublishAllPorts === false
    && (host.PortBindings === null || Object.keys(host.PortBindings).length === 0)
    && host.PidsLimit === 256 && host.Memory === 2 * GIB && host.NanoCpus === 2_000_000_000
    && JSON.stringify(host.CapDrop) === JSON.stringify(["ALL"])
    && Array.isArray(host.SecurityOpt) && host.SecurityOpt.length === 1
    && ["no-new-privileges", "no-new-privileges:true", "no-new-privileges=true"]
      .includes(host.SecurityOpt[0]),
  "smoke_isolation");
  const mounts = [...(value.Mounts ?? [])].sort((left, right) =>
    left.Destination.localeCompare(right.Destination));
  check(mounts.length === 2 && mounts[0].Type === "bind"
    && mounts[0].Source === expected.fixture && mounts[0].Destination === "/fixture"
    && mounts[0].RW === false && mounts[1].Type === "bind"
    && mounts[1].Source === expected.out && mounts[1].Destination === "/out"
    && mounts[1].RW === true, "smoke_mounts");
  exactKeys(host.Tmpfs, ["/dev/shm", "/tmp", "/var/lib/bluey-jobs-runner"],
    "smoke_tmpfs_shape");
  validateTmpfs(host.Tmpfs["/tmp"], expected.uid, expected.gid, 512 * 1024 * 1024);
  validateTmpfs(host.Tmpfs["/dev/shm"], expected.uid, expected.gid, 512 * 1024 * 1024);
  validateTmpfs(host.Tmpfs["/var/lib/bluey-jobs-runner"], expected.uid, expected.gid,
    128 * 1024 * 1024);
  return value;
}

export function validateImageInspectionReport(report, expected) {
  check(expectedSourceCommitValue(expected?.sourceCommit), "inspection_expected_source_commit");
  validateInspection(report, expected);
  check(JSON.stringify(report.expected) === JSON.stringify(expected),
    "inspection_expected_mismatch");
  return report;
}

export function validateQualification(value, expectedSourceCommit) {
  check(expectedSourceCommitValue(expectedSourceCommit),
    "qualification_expected_source_commit");
  exactKeys(value, ["buildEpoch", "builder", "cleanup", "container", "inspection",
    "qualification", "sourceArchiveSha256", "sourceCommit", "toolSourcesSha256", "version"],
  "qualification_shape");
  check(value.version === 1 && value.qualification === "isolated_measured_runner_image_only"
    && value.sourceCommit === expectedSourceCommit
    && Number.isInteger(value.buildEpoch) && /^[1-9][0-9]{8,11}$/.test(String(value.buildEpoch))
    && SHA.test(value.sourceArchiveSha256) && SHA.test(value.toolSourcesSha256),
  "qualification_identity");
  exactKeys(value.builder, ["image", "version"], "qualification_builder");
  check(value.builder.image === BUILDKIT_IMAGE && value.builder.version === "v0.34.1",
    "qualification_builder");
  validateImageInspectionReport(value.inspection, {
    ...value.inspection.expected,
    sourceCommit: expectedSourceCommit,
  });
  check(value.inspection.expected.sourceCommit === value.sourceCommit
    && value.inspection.expected.buildId === `nonprod-p639-${value.sourceCommit.slice(0, 12)}`,
  "qualification_inspection_source");
  exactKeys(value.container, ["exitCode", "identityProtocol", "imageConfigSha256",
    "imageManifestSha256", "networkMode", "readOnlyRootfs", "securityBoundaryVerified"],
  "qualification_container");
  check(value.container.imageConfigSha256 === value.inspection.oci.imageConfigSha256
    && value.container.imageManifestSha256 === value.inspection.oci.manifestSha256
    && value.container.identityProtocol === IMAGE_IDENTITY_PROTOCOL
    && value.container.networkMode === "none" && value.container.readOnlyRootfs === true
    && value.container.exitCode === 0 && value.container.securityBoundaryVerified === true,
  "qualification_container");
  check(value.inspection.oci.manifestSha256 !== value.container.imageConfigSha256,
    "qualification_oci_identities");
  exactKeys(value.cleanup, ["builderRemoved", "containerRemoved"], "qualification_cleanup");
  check(value.cleanup.builderRemoved === true && value.cleanup.containerRemoved === true,
    "qualification_cleanup");
  return value;
}

async function guestText(guest, args, options = {}) {
  const output = await guest(args, options);
  check(typeof output === "string", "guest_output_shape");
  return output;
}

export async function captureBuildFailureDiagnostic(guest, diagnostic) {
  check(typeof guest === "function" && typeof diagnostic === "function",
    "build_diagnostic_input");
  const tail = await guestText(guest,
    ["/usr/bin/tail", "-c", "8192", `${ROOT}/build.log`], { maximum: 65_536 });
  const bytes = Buffer.byteLength(tail, "utf8");
  check(bytes > 0 && bytes <= 32_768, "build_diagnostic_tail");
  await diagnostic(`Combined guest build log tail:\n${tail}`);
  return Object.freeze({ bytes, sha256: sha256(Buffer.from(tail, "utf8")) });
}

async function assertPrivateScratch(path) {
  const metadata = await lstat(path);
  check(metadata.isDirectory() && !metadata.isSymbolicLink()
    && metadata.uid === process.getuid() && (metadata.mode & 0o777) === 0o700
    && await realpath(path) === path, "scratch_root_identity");
}

async function assertToolsUnchanged(hashes) {
  for (const [name, digest] of Object.entries(hashes)) {
    check(sha256(await readFile(join(MODULE_DIRECTORY, name))) === digest,
      "inspection_tool_changed");
  }
}

export function validateExpectedSourceHead(head, expectedSourceCommit) {
  check(expectedSourceCommitValue(expectedSourceCommit)
    && typeof head === "string" && head.trim() === expectedSourceCommit, "source_head_changed");
  return expectedSourceCommit;
}

async function assertExpectedSourceHead(host, expectedSourceCommit) {
  validateExpectedSourceHead(
    await host("git", ["rev-parse", "HEAD"], { maximum: 1024 }), expectedSourceCommit,
  );
}

function toolSourceHash(toolBytes) {
  const names = Object.keys(toolBytes).sort();
  return sha256(Buffer.concat([Buffer.from("bluey-jobs-image-inspection-tools-v1\0"),
    ...names.flatMap((name) => [Buffer.from(`${name}\0`), toolBytes[name], Buffer.from("\0")])]));
}

async function assertResourcesAbsent(guest) {
  const containers = await guestText(guest, [...dockerEnvironment(), "ps", "-a", "--format",
    "{{.Names}}"], { maximum: 64 * 1024 });
  const names = containers.split("\n");
  check(!names.includes(CONTAINER) && !names.includes(BUILDKIT_CONTAINER),
    "fixed_resource_exists");
  const builders = await guestText(guest, [...dockerEnvironment(), "buildx", "ls", "--format",
    "{{.Name}}"], { maximum: 64 * 1024 });
  check(!builders.split("\n").some((name) => name.trim().replace(/\*$/, "") === BUILDER),
    "fixed_resource_exists");
}

export async function cleanupManagedRunnerImageResources(guest, state) {
  exactKeys(state, ["builderAttempted", "containerAttempted"], "cleanup_state_shape");
  const failures = [];
  if (state.containerAttempted) {
    try { await guestText(guest, [...dockerEnvironment(), "rm", "-f", CONTAINER],
      { timeout: 60_000, maximum: 8192 }); } catch { failures.push("container"); }
  }
  if (state.builderAttempted) {
    try { await guestText(guest, [...dockerEnvironment(), "buildx", "rm", BUILDER],
      { timeout: 120_000, maximum: 8192 }); } catch { failures.push("builder"); }
  }
  try { await assertResourcesAbsent(guest); } catch { failures.push("absence"); }
  check(failures.length === 0, "managed_runner_image_cleanup_failed");
  return Object.freeze({ containerRemoved: true, builderRemoved: true });
}

function sha256sum(text, code) {
  const match = /^([a-f0-9]{64})\s+\S+\n?$/.exec(text);
  check(match && SHA.test(match[1]), code);
  return match[1];
}

async function createExpected(source, fixture, sourceCommit) {
  check(expectedSourceCommitValue(sourceCommit), "expected_source_commit");
  const directory = join(fixture, "contracts");
  await mkdir(directory, { mode: 0o700 });
  await createManagedCloudContracts(source, directory, 3);
  const names = ["builder-policy.json", "config-contract.json", "migration-contract.json",
    "protocol-contract.json"];
  check(JSON.stringify((await readdir(directory)).sort()) === JSON.stringify(names),
    "contract_file_set");
  const hashes = {};
  for (const name of names.slice(1)) {
    const bytes = await readFile(join(directory, name));
    let parsed;
    try { parsed = JSON.parse(bytes.toString("utf8")); }
    catch { throw new Error("contract_json_invalid"); }
    check(bytes.equals(canonicalJsonBytes(parsed)), "contract_not_canonical");
    hashes[name] = sha256(bytes);
  }
  const expected = {
    version: 1,
    sourceCommit,
    buildId: `nonprod-p639-${sourceCommit.slice(0, 12)}`,
    configSchemaSha256: hashes["config-contract.json"],
    migrationSetSha256: hashes["migration-contract.json"],
    protocolSetSha256: hashes["protocol-contract.json"],
  };
  validateExpected(expected);
  return expected;
}

export function artifactAttachmentPlan(report, archiveSha256, reader, owner, source, expected) {
  check(expectedSourceCommitValue(expected?.sourceCommit),
    "artifact_attachment_expected_source_commit");
  validateExpected(expected);
  validateImageInspectionReport(report, expected);
  check(SHA.test(archiveSha256) && Number.isInteger(owner) && owner > 0
    && source.sourceCommit === expected.sourceCommit
    && SHA.test(source.sourceArchiveSha256) && SHA.test(source.toolSourcesSha256),
  "artifact_attachment_binding");
  return { reader, owner, inspection: report, source,
    sha256s: { "runner.oci.tar": archiveSha256, "inventory.json": report.oci.inventorySha256,
      "measurement.json": report.runtime.runtimeMeasurementSha256,
      "report.json": sha256(canonicalJsonBytes(report)) } };
}

export async function completeImageWorkload(
  provisional, cleanup, preservation, attachmentPlan, expectedSourceCommit,
) {
  const qualifiedImage = validateQualification({ ...provisional, cleanup }, expectedSourceCommit);
  if (preservation) {
    check(attachmentPlan, "artifact_attachment_plan_missing");
    await preservation.exportAttachments(attachmentPlan);
  }
  return qualifiedImage;
}

export function validateFailedArchiveDescriptor(value, reader, owner) {
  exactKeys(value, ["version", "artifact", "size", "sha256", "source", "reader"], "failed_archive_descriptor");
  exactKeys(value.source, ["dev", "ino", "uid", "mode", "nlink", "size", "mtimeNs", "ctimeNs"],
    "failed_archive_descriptor");
  check(value.version === 1 && value.artifact === "runner.oci.tar"
    && typeof value.sha256 === "string" && SHA.test(value.sha256)
    && Number.isSafeInteger(value.size) && value.size > 0 && value.size <= 4 * GIB
    && Number.isInteger(owner) && owner > 0 && owner <= 4_294_967_295 && value.source.uid === owner
    && value.source.mode === 0o400 && value.source.nlink === 1 && value.source.size === value.size
    && value.source.ino !== "0"
    && ["dev", "ino", "mtimeNs", "ctimeNs"].every((key) => typeof value.source[key] === "string"
      && /^(?:0|[1-9][0-9]{0,23})$/.test(value.source[key]))
    && canonicalJsonBytes(value.reader).equals(canonicalJsonBytes(reader)), "failed_archive_descriptor");
  return Object.freeze({ ...value, source: Object.freeze({ ...value.source }) });
}

export async function preserveFailedArchive(primaryFailure, cleanup, preservation, plan) {
  check(cleanup?.builderRemoved === true && cleanup?.containerRemoved === true, "failed_archive_cleanup");
  try {
    if (preservation && plan) await preservation.exportFailedArchive(plan);
  } finally {
    // The harness records retention's separate closed status. It can never turn
    // the failed image into a result or replace the original failure/exit1.
    throw primaryFailure;
  }
}

export async function runManagedRunnerImageWorkload({
  diagnostic, guest, host, progress, scratchRoot, sourceCommit, buildEpoch, preservation = null,
}) {
  check(typeof diagnostic === "function" && typeof guest === "function" && typeof host === "function"
    && typeof progress === "function"
    && expectedSourceCommitValue(sourceCommit)
    && Number.isInteger(buildEpoch) && /^[1-9][0-9]{8,11}$/.test(String(buildEpoch)),
  "workload_input");
  check(preservation === null || (typeof preservation.exportAttachments === "function"
    && typeof preservation.exportFailedArchive === "function"
    && typeof preservation.beforeBuild === "function" && typeof preservation.onPhase === "function"
    && preservation.approval), "artifact_preservation_input");
  const scratch = resolve(scratchRoot);
  check(scratch === scratchRoot, "scratch_root_scope");
  await assertExpectedSourceHead(host, sourceCommit);
  await assertPrivateScratch(scratch);
  const source = join(scratch, "source");
  const fixture = join(scratch, "fixture");
  await mkdir(source, { mode: 0o700 });
  await mkdir(fixture, { mode: 0o700 });
  await assertPrivateScratch(source);
  await assertPrivateScratch(fixture);

  const toolNames = ["managed-cloud-release-gate.mjs",
    "nonprod-managed-runner-image-inspect.mjs",
    ...(preservation ? ["managed-runner-artifacts.py"] : [])];
  const toolBytes = Object.fromEntries(await Promise.all(toolNames.map(async (name) =>
    [name, await readFile(join(MODULE_DIRECTORY, name))])));
  const toolHashes = Object.fromEntries(Object.entries(toolBytes)
    .map(([name, bytes]) => [name, sha256(bytes)]));
  if (preservation) validateReaderApproval(preservation.approval, toolHashes["managed-runner-artifacts.py"]);
  await assertToolsUnchanged(toolHashes);

  const tree = await host("git", ["ls-tree", "-r", "-z", "--full-tree", sourceCommit,
    "--", ...selectedPaths], { binaryStdout: true, maximum: MAX_SOURCE_ARCHIVE });
  check(Buffer.isBuffer(tree), "source_tree_output_shape");
  parseSelectedGitTree(tree);
  await assertExpectedSourceHead(host, sourceCommit);
  const archive = await host("git", ["archive", "--format=tar", sourceCommit, ...selectedPaths],
    { binaryStdout: true, maximum: MAX_SOURCE_ARCHIVE });
  check(Buffer.isBuffer(archive) && archive.length > 0 && archive.length <= MAX_SOURCE_ARCHIVE,
    "source_archive_bound");
  const extracted = await host("tar", ["-xf", "-", "-C", source],
    { inputBuffer: archive, maximum: 1024 });
  check(extracted === "", "source_extract_output");

  const expected = await createExpected(source, fixture, sourceCommit);
  await writeFile(join(fixture, "expected.json"), canonicalJsonBytes(expected),
    { flag: "wx", mode: 0o600 });
  for (const name of toolNames) {
    await writeFile(join(fixture, name), toolBytes[name], { flag: "wx", mode: 0o500 });
  }
  await writeFile(join(fixture, "run-build.sh"), Buffer.from(
    `#!/bin/sh\nset -eu\numask 077\nexec "$@" >${ROOT}/build.log 2>&1\n`),
  { flag: "wx", mode: 0o500 });
  const fixtureArchive = await host("tar", ["-cf", "-", "-C", fixture, "."],
    { binaryStdout: true, maximum: MAX_FIXTURE_ARCHIVE });
  check(Buffer.isBuffer(fixtureArchive) && fixtureArchive.length > 0
    && fixtureArchive.length <= MAX_FIXTURE_ARCHIVE, "fixture_archive_bound");
  await emitWorkloadProgress(progress, "inputs_prepared");

  const cleanupState = { builderAttempted: false, containerAttempted: false };
  const stagedEvidence = [];
  let evidenceFlushed = false;
  let buildAttempted = false;
  let primaryFailure;
  let provisional;
  let artifactReader;
  let attachmentPlan;
  let failedArchivePlan;
  try {
    await guestText(guest, ["mkdir", "-m", "700", ROOT], { maximum: 1024 });
    await guestText(guest, ["mkdir", "-m", "700", `${ROOT}/source`, `${ROOT}/fixture`,
      `${ROOT}/out`, `${ROOT}/docker-config`, `${ROOT}/home`], { maximum: 1024 });
    const privateDirectories = [ROOT, `${ROOT}/source`, `${ROOT}/fixture`, `${ROOT}/out`,
      `${ROOT}/docker-config`, `${ROOT}/home`];
    validatePrivateDirectoryStats(await guestText(guest,
      ["stat", "-c", "%u:%g:%a:%F", ...privateDirectories], { maximum: 8192 }),
    privateDirectories.length);
    const diskSpace = validateDiskSpace(await guestText(guest,
      ["df", "-Pk", "/", "/var/lib/docker", "/var/lib/containerd"], { maximum: 8192 }));
    stagedEvidence.push({ stage: "storage_capacity", value: diskSpace });
    const mountTexts = [];
    for (const target of ["/", "/var/lib/docker", "/var/lib/containerd"]) {
      mountTexts.push(await guestText(guest, ["findmnt", "--json", "--target", target,
        "--output", "SOURCE,TARGET,FSTYPE,FSROOT,MAJ:MIN"], { maximum: 8192 }));
    }
    const storageMounts = validateStorageMounts(mountTexts);
    stagedEvidence.push({ stage: "storage_mounts", value: storageMounts });
    await guestText(guest, ["tar", "-xf", "-", "-C", `${ROOT}/source`],
      { inputBuffer: archive, maximum: 1024 });
    await guestText(guest, ["tar", "-xf", "-", "-C", `${ROOT}/fixture`],
      { inputBuffer: fixtureArchive, maximum: 1024 });
    validatePrivateDirectoryStats(await guestText(guest,
      ["stat", "-c", "%u:%g:%a:%F", ...privateDirectories], { maximum: 8192 }),
    privateDirectories.length);
    for (const name of toolNames) {
      check(sha256sum(await guestText(guest,
        ["sha256sum", `${ROOT}/fixture/${name}`], { maximum: 1024 }),
      "guest_tool_hash") === toolHashes[name], "guest_tool_hash");
    }
    if (preservation) {
      // An independent guest-OS interpreter and source helper, never image-under-test code.
      // Missing/unapproved Python fails here before any builder or image build.
      const readerDirectory = "/var/tmp/bluey-jobs-artifact-reader-p639";
      await guestText(guest, ["/usr/bin/sudo", "-n", "mkdir", "-m", "700", readerDirectory],
        { maximum: 1024 });
      await guestText(guest, ["/usr/bin/sudo", "-n", "install", "-o", "0", "-g", "0", "-m", "500",
        `${ROOT}/fixture/managed-runner-artifacts.py`, `${readerDirectory}/reader.py`], { maximum: 1024 });
      const probeText = await guestText(guest,
        guestReaderArguments("guest-probe", preservation.approval, preservation.approval),
        { maximum: 8192, timeout: 30_000 });
      const probe = JSON.parse(probeText);
      check(canonicalJsonBytes(probe).toString("ascii") === probeText,
        "artifact_reader_probe_canonical");
      artifactReader = validateReaderProbe(probe, preservation.approval);
      await preservation.beforeBuild();
    }
    await assertResourcesAbsent(guest);

    const buildx = await guestText(guest, [...dockerEnvironment(), "buildx", "version"],
      { maximum: 4096 });
    check(/^github\.com\/docker\/buildx\s+v0\.34\.1(?:\s|$)/.test(buildx.trim()),
      "buildx_version_required");
    const engineMetadata = selectEngineMetadata(JSON.parse(await guestText(guest,
      [...dockerEnvironment(), "info", "--format",
        '{"ServerVersion":{{json .ServerVersion}},"Driver":{{json .Driver}},'
          + '"DriverStatus":{{json .DriverStatus}},"Os":{{json .OSType}},'
          + '"Architecture":{{json .Architecture}}}'],
      { maximum: 32 * 1024 })));
    stagedEvidence.push({ stage: "engine_metadata", value: engineMetadata });
    validateContainerdEngine(engineMetadata);
    cleanupState.builderAttempted = true;
    await guestText(guest, [...dockerEnvironment(), "buildx", "create", "--name", BUILDER,
      "--driver", "docker-container", "--driver-opt", `image=${BUILDKIT_IMAGE}`],
    { timeout: 120_000, maximum: 8192 });
    const bootstrap = await guestText(guest, [...dockerEnvironment(), "buildx", "inspect",
      BUILDER, "--bootstrap", "--timeout", "120s"],
    { timeout: 300_000, maximum: 32 * 1024 });
    await emitPrivateDiagnostic(diagnostic, "Buildx bootstrap stdout:", bootstrap);
    const buildkitImage = selectBuilderImageMetadata(JSON.parse(await guestText(guest,
      [...dockerEnvironment(), "image", "inspect", "--format", "{{json .}}", BUILDKIT_IMAGE],
      { maximum: 256 * 1024 })));
    stagedEvidence.push({ stage: "builder_image_metadata", value: buildkitImage });
    const buildkit = JSON.parse(await guestText(guest,
      [...dockerEnvironment(), "inspect", BUILDKIT_CONTAINER, "--format", "{{json .}}"],
      { maximum: 64 * 1024 }));
    const buildkitContainer = selectBuilderContainerMetadata(buildkit);
    stagedEvidence.push({ stage: "builder_container_metadata", value: buildkitContainer });
    stagedEvidence.push({ stage: "builder_known_config_metadata",
      value: { configSha256: BUILDKIT_CONFIG_SHA256, observedAsImageId: false } });
    validateBuildkitManifestIdentity(buildkitImage, buildkitContainer);
    const daemonVersionText = await guestText(guest,
      [...dockerEnvironment(), "exec", BUILDKIT_CONTAINER, "buildkitd", "--version"],
      { maximum: 4096, timeout: 30_000 });
    await emitPrivateDiagnostic(diagnostic, "BuildKit daemon version:", daemonVersionText);
    const daemonVersion = parseBuildkitDaemonVersion(daemonVersionText);
    await emitWorkloadProgress(progress, "builder_started");

    await emitWorkloadProgress(progress, "image_build_started");
    buildAttempted = true;
    await guestText(guest, ["/bin/sh", `${ROOT}/fixture/run-build.sh`, ...dockerEnvironment(),
      ...buildArguments(expected, buildEpoch)], { timeout: 1_800_000, maximum: 1024 });
    await emitWorkloadProgress(progress, "image_build_finished");
    const logBytes = Number((await guestText(guest, ["stat", "-c", "%s", `${ROOT}/build.log`],
      { maximum: 1024 })).trim());
    check(Number.isInteger(logBytes) && logBytes > 0 && logBytes <= 16 * 1024 * 1024,
      "build_log_bound");
    sha256sum(await guestText(guest, ["sha256sum", `${ROOT}/build.log`], { maximum: 1024 }),
      "build_log_hash");

    const archiveIndex = parseOciArchiveIndex(await guestText(guest,
      ["tar", "-xOf", `${ROOT}/fixture/runner.oci.tar`, "index.json"],
      { maximum: 32 * 1024 }));
    stagedEvidence.push({ stage: "oci_archive_index", value: archiveIndex });
    const archiveSizeText = await guestText(guest,
      ["stat", "-c", "%s", `${ROOT}/fixture/runner.oci.tar`], { maximum: 1024 });
    check(/^[1-9][0-9]*\n$/.test(archiveSizeText), "oci_archive_size_shape");
    const archiveSize = Number(archiveSizeText.trim());
    check(Number.isSafeInteger(archiveSize), "oci_archive_size_shape");
    stagedEvidence.push({ stage: "oci_archive_bytes", value: archiveSize });
    check(archiveSize <= 4 * GIB, "oci_archive_size_bound");
    const manifestText = await guestText(guest,
      ["tar", "-xOf", `${ROOT}/fixture/runner.oci.tar`,
        `blobs/sha256/${archiveIndex.manifestSha256}`], { maximum: 128 * 1024 });
    const configDescriptor = parseOciConfigDescriptor(manifestText, archiveIndex);
    const configText = await guestText(guest,
      ["tar", "-xOf", `${ROOT}/fixture/runner.oci.tar`,
        `blobs/sha256/${configDescriptor.sha256}`], { maximum: 128 * 1024 });
    stagedEvidence.push({ stage: "oci_runtime_config_shape",
      value: parseOciRuntimeShapeEvidence(configText, configDescriptor) });
    const loadOutput = await guestText(guest,
      [...dockerEnvironment(), "image", "load", "--input", `${ROOT}/fixture/runner.oci.tar`,
        "--platform", "linux/amd64"],
      { maximum: 32 * 1024, timeout: 300_000 });
    await emitPrivateDiagnostic(diagnostic, "Docker image load stdout:", loadOutput);

    const image = JSON.parse(await guestText(guest,
      [...dockerEnvironment(), "image", "inspect", "--format", "{{json .}}",
        archiveIndex.digest],
      { maximum: 128 * 1024 }));
    const imageMetadata = selectBuiltImageMetadata(image);
    stagedEvidence.push({ stage: "built_image_metadata", value: imageMetadata });
    validateLoadedManifestIdentity(imageMetadata, archiveIndex);
    check(imageMetadata.Os === "linux" && imageMetadata.Architecture === "amd64",
      "image_platform_identity");
    const idCommand = [...dockerEnvironment(), "run", "--rm", "--network", "none",
      "--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges=true",
      "--entrypoint", "/usr/bin/id", imageMetadata.Id];
    const uid = positiveId(await guestText(guest, [...idCommand, "-u"],
      { timeout: 60_000, maximum: 1024 }), "image_uid");
    const gid = positiveId(await guestText(guest, [...idCommand, "-g"],
      { timeout: 60_000, maximum: 1024 }), "image_gid");
    await guestText(guest, ["/usr/bin/sudo", "-n", "chown", `${uid}:${gid}`,
      `${ROOT}/fixture`, `${ROOT}/out`,
      `${ROOT}/fixture/expected.json`, `${ROOT}/fixture/managed-cloud-release-gate.mjs`,
      `${ROOT}/fixture/nonprod-managed-runner-image-inspect.mjs`,
      `${ROOT}/fixture/runner.oci.tar`], { maximum: 1024 });
    await guestText(guest, ["/usr/bin/sudo", "-n", "chmod", "700", `${ROOT}/fixture`,
      `${ROOT}/out`],
      { maximum: 1024 });
    await guestText(guest, ["/usr/bin/sudo", "-n", "chmod", "400",
      `${ROOT}/fixture/expected.json`,
      `${ROOT}/fixture/managed-cloud-release-gate.mjs`,
      `${ROOT}/fixture/nonprod-managed-runner-image-inspect.mjs`,
      `${ROOT}/fixture/runner.oci.tar`], { maximum: 1024 });
    check((await guestText(guest, ["stat", "-c", "%u:%g:%a", `${ROOT}/out`],
      { maximum: 1024 })).trim() === `${uid}:${gid}:700`, "smoke_output_identity");

    if (preservation) {
      await assertToolsUnchanged(toolHashes);
      const text = await guestText(guest, guestReaderArguments("guest-read", preservation.approval, {
        artifact: "runner.oci.tar", approval: preservation.approval, reader: artifactReader,
        owner: uid, describe: true,
      }), { maximum: 8192, timeout: 60_000 });
      const observed = JSON.parse(text);
      check(canonicalJsonBytes(observed).toString("ascii") === text, "failed_archive_descriptor_canonical");
      const descriptor = validateFailedArchiveDescriptor(observed, artifactReader, uid);
      check(descriptor.size === archiveSize, "failed_archive_size_changed");
      failedArchivePlan = Object.freeze({ descriptor, reader: artifactReader, owner: uid,
        source: Object.freeze({ sourceCommit, sourceArchiveSha256: sha256(archive),
          toolSourcesSha256: toolSourceHash(toolBytes) }) });
    }

    await emitWorkloadProgress(progress, "image_inspection_started");
    const tmpfs = (path, size) => ["--tmpfs",
      `${path}:rw,noexec,nosuid,nodev,size=${size},uid=${uid},gid=${gid},mode=0700`];
    cleanupState.containerAttempted = true;
    await guestText(guest, [...dockerEnvironment(), "create", "--name", CONTAINER,
      "--network", "none", "--ipc", "private", "--read-only", "--cap-drop", "ALL",
      "--security-opt",
      "no-new-privileges=true", "--pids-limit", "256", "--memory", "2g", "--cpus", "2",
      "--mount", `type=bind,src=${ROOT}/fixture,dst=/fixture,readonly`,
      "--mount", `type=bind,src=${ROOT}/out,dst=/out`,
      ...tmpfs("/tmp", 512 * 1024 * 1024), ...tmpfs("/dev/shm", 512 * 1024 * 1024),
      ...tmpfs("/var/lib/bluey-jobs-runner", 128 * 1024 * 1024),
      "--entrypoint", "/usr/local/bin/node", imageMetadata.Id,
      "/fixture/nonprod-managed-runner-image-inspect.mjs", "--inspect-image"],
    { timeout: 60_000, maximum: 8192 });
    const eventText = await guestText(guest, [...dockerEnvironment(), "start", "-a", CONTAINER],
      { timeout: 300_000, maximum: 64 * 1024 });
    let event;
    try { event = JSON.parse(eventText); } catch { throw new Error("smoke_event_json"); }
    exactKeys(event, ["event", "imageConfigSha256", "inspectorSha256", "manifestSha256",
      "reportSha256", "version"], "smoke_event_shape");
    check(event.event === "managed_runner_image_inspected" && event.version === 1,
      "smoke_event_shape");
    const container = JSON.parse(await guestText(guest,
      [...dockerEnvironment(), "inspect", CONTAINER, "--format", "{{json .}}"],
      { maximum: 256 * 1024 }));
    stagedEvidence.push({ stage: "smoke_container_metadata",
      value: selectSmokeContainerMetadata(container) });
    validateSmokeContainerInspection(container, {
      fixture: `${ROOT}/fixture`, gid, imageManifestDescriptor: archiveIndex,
      imageManifestId: archiveIndex.digest, out: `${ROOT}/out`, uid,
    });
    const reportText = await guestText(guest,
      ["/usr/bin/sudo", "-n", "cat", `${ROOT}/out/report.json`],
      { maximum: MAX_REPORT });
    check(Buffer.byteLength(reportText) > 0 && Buffer.byteLength(reportText) <= MAX_REPORT,
      "inspection_report_bound");
    check(Buffer.byteLength(reportText) <= 32 * 1024, "inspection_report_diagnostic_bound");
    stagedEvidence.push({ stage: "inspection_report_text", value: reportText });
    let report;
    try { report = JSON.parse(reportText); } catch { throw new Error("inspection_report_json"); }
    validateImageInspectionReport(report, expected);
    const inspectorSha256 = toolHashes["nonprod-managed-runner-image-inspect.mjs"];
    check(imageMetadata.Id === `sha256:${report.oci.manifestSha256}`
      && archiveIndex.manifestSha256 === report.oci.manifestSha256
      && report.oci.imageConfigSha256 !== report.oci.manifestSha256
      && event.imageConfigSha256 === report.oci.imageConfigSha256
      && event.manifestSha256 === report.oci.manifestSha256
      && event.reportSha256 === sha256(canonicalJsonBytes(report))
      && event.inspectorSha256 === inspectorSha256
      && report.inspectorSha256 === inspectorSha256,
    "inspection_image_binding");
    const archiveSha256 = sha256sum(await guestText(guest,
      ["/usr/bin/sudo", "-n", "sha256sum", `${ROOT}/fixture/runner.oci.tar`],
      { maximum: 1024 }), "oci_archive_hash");
    await emitWorkloadProgress(progress, "image_inspection_finished");
    if (preservation) {
      await assertExpectedSourceHead(host, sourceCommit);
      attachmentPlan = artifactAttachmentPlan(report, archiveSha256, artifactReader,
        uid, { sourceCommit, sourceArchiveSha256: sha256(archive), toolSourcesSha256: toolSourceHash(toolBytes) },
        expected);
    }
    provisional = {
      version: 1,
      qualification: "isolated_measured_runner_image_only",
      sourceCommit,
      sourceArchiveSha256: sha256(archive),
      buildEpoch,
      builder: { image: BUILDKIT_IMAGE, version: daemonVersion.version },
      inspection: report,
      container: {
        identityProtocol: IMAGE_IDENTITY_PROTOCOL,
        imageManifestSha256: report.oci.manifestSha256,
        imageConfigSha256: report.oci.imageConfigSha256,
        networkMode: "none",
        readOnlyRootfs: true,
        exitCode: 0,
        securityBoundaryVerified: true,
      },
      toolSourcesSha256: toolSourceHash(toolBytes),
    };
    evidenceFlushed = true;
    try { await flushStagedImageEvidence(diagnostic, stagedEvidence); }
    catch (error) { throw new Error("staged_evidence_diagnostic_failed", { cause: error }); }
  } catch (error) {
    primaryFailure = error;
  }

  let diagnosticFailure;
  if (primaryFailure && stagedEvidence.length > 0 && !evidenceFlushed) {
    evidenceFlushed = true;
    try { await flushStagedImageEvidence(diagnostic, stagedEvidence); }
    catch (error) { diagnosticFailure = error; }
  }
  if (primaryFailure && buildAttempted) {
    if (preservation) preservation.onPhase("failure_diagnostic");
    try { await captureBuildFailureDiagnostic(guest, diagnostic); }
    catch (error) { diagnosticFailure ??= error; }
  }

  let cleanup;
  let cleanupFailure;
  if (preservation) preservation.onPhase("cleanup", { failed: Boolean(primaryFailure || diagnosticFailure) });
  try {
    cleanup = cleanupState.builderAttempted || cleanupState.containerAttempted
      ? await cleanupManagedRunnerImageResources(guest, cleanupState)
      : Object.freeze({ containerRemoved: true, builderRemoved: true });
  }
  catch (error) { cleanupFailure = error; }
  if (preservation && !cleanupFailure) preservation.onPhase("cleaned");
  let freezeFailure;
  try { await assertToolsUnchanged(toolHashes); }
  catch (error) { freezeFailure = error; }
  let sourceFailure;
  try { await assertExpectedSourceHead(host, sourceCommit); }
  catch (error) { sourceFailure = error; }
  if (cleanupFailure) {
    const code = diagnosticFailure
      ? "managed_runner_image_primary_diagnostic_and_cleanup_failed"
      : primaryFailure
        ? "managed_runner_image_primary_and_cleanup_failed"
        : cleanupFailure.message;
    throw new Error(code, { cause: primaryFailure ?? diagnosticFailure ?? cleanupFailure });
  }
  if (sourceFailure) throw sourceFailure;
  if (diagnosticFailure) {
    const failure = new Error("managed_runner_image_primary_and_diagnostic_failed", { cause: primaryFailure });
    if (freezeFailure) throw failure;
    return preserveFailedArchive(failure, cleanup, preservation, failedArchivePlan);
  }
  if (freezeFailure) throw freezeFailure;
  if (primaryFailure) {
    return preserveFailedArchive(primaryFailure, cleanup, preservation, failedArchivePlan);
  }
  check(provisional, "workload_result_missing");
  // Both named resources are absent before transfer. Their bind-source archive
  // and /out attachments remain in ROOT until the caller's outer VM deletion.
  return completeImageWorkload(provisional, cleanup, preservation, attachmentPlan, sourceCommit);
}
