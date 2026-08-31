import { Buffer } from "node:buffer";
import { createHash, createHmac } from "node:crypto";
import {
  closeSync,
  constants as fsConstants,
  fstatSync,
  lstatSync,
  openSync,
  readdirSync,
  readFileSync,
  type Stats,
} from "node:fs";
import { relative, resolve, sep } from "node:path";
import { normalizeManagedCloudWorkerOrigin } from "./worker-auth.js";

const SAFE_ID = /^[A-Za-z0-9_-]{20,128}$/;
const RELEASE_ID = /^[a-z0-9][a-z0-9._:-]{2,127}$/;
const SHA256 = /^[a-f0-9]{64}$/;
const SOURCE_COMMIT = /^[a-f0-9]{40}$/;
const TOKEN = /^[A-Za-z0-9_-]{43}$/;
const MAX_MEASUREMENT_BYTES = 128 * 1024;
const MAX_MEASURED_FILES = 512;
const RUNTIME_MEASUREMENT_AUDIENCE =
  "bluey-jobs-managed-cloud-runtime-measurement-v1";
const RUNTIME_MEASUREMENT_PATH =
  "app/.bluey/managed-cloud-runtime-measurement.json";
const RUNTIME_IDENTITY_DOMAIN =
  "bluey-jobs-managed-cloud-runtime-identity-v1\0";
const MANAGED_CLOUD_ENV_PREFIX = "BLUEY_" + "JOBS_MANAGED_CLOUD_";
const CALLER_RUNTIME_IDENTITY_SUFFIX = "_RUNTIME_IDENTITY_SHA256";

export type ManagedCloudRuntimeRole =
  | "discovery_worker"
  | "global_discovery_worker"
  | "jobs_api"
  | "managed_runner"
  | "original_source_verifier"
  | "workflow_command_dispatcher"
  | "workflow_cleanup_dispatcher"
  | "workflow_gateway"
  | "workflow_worker";

export interface ManagedCloudScope {
  environment: "staging" | "production";
  region: string;
  channel: "shadow" | "canary" | "general";
}

export interface ManagedCloudRuntimeConfig {
  apiOrigin: string;
  signingKey: string;
  workerId: string;
  grantId: string;
  grantToken: string;
  sessionToken: string;
  runtimeInstanceId: string;
  runtimeIdentitySha256: string;
  runtimeMeasurementSha256: string;
  componentId: ManagedCloudRuntimeComponentId;
  configSchemaSha256: string;
  migrationSetSha256: string;
  protocolSetSha256: string;
  measurementOptions: Readonly<ManagedCloudRuntimeMeasurementOptions>;
  scope: ManagedCloudScope;
  heartbeatIntervalMs: number;
  expectedRole: ManagedCloudRuntimeRole;
}

export type ManagedCloudRuntimeComponentId =
  | "jobs-api"
  | "jobs-runner"
  | "jobs-workflows";

export interface ManagedCloudRuntimeMeasurementFile {
  path: string;
  sha256: string;
}

export interface ManagedCloudRuntimeMeasurement {
  audience: typeof RUNTIME_MEASUREMENT_AUDIENCE;
  buildId: string;
  componentId: ManagedCloudRuntimeComponentId;
  configSchemaSha256: string;
  measuredFiles: ManagedCloudRuntimeMeasurementFile[];
  migrationSetSha256: string;
  protocolSetSha256: string;
  roles: ManagedCloudRuntimeRole[];
  sourceCommit: string;
  version: 1;
}

export interface ManagedCloudRuntimeMeasurementIdentity {
  componentId: ManagedCloudRuntimeComponentId;
  configSchemaSha256: string;
  migrationSetSha256: string;
  protocolSetSha256: string;
  runtimeIdentitySha256: string;
  runtimeMeasurementSha256: string;
}

export interface ManagedCloudRuntimeMeasurementOptions {
  measurementPath?: string;
  rootPath?: string;
}

export interface ManagedCloudRuntimeClaimRequest {
  grantId: string;
  grantToken: string;
  workerId: string;
  sessionToken: string;
  runtimeInstanceId: string;
  runtimeIdentitySha256: string;
}

export function managedCloudRuntimeConfig(
  expectedRole: ManagedCloudRuntimeRole,
  env: NodeJS.ProcessEnv = process.env,
  measurementOptions: ManagedCloudRuntimeMeasurementOptions = {},
): ManagedCloudRuntimeConfig | undefined {
  if (env.BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED !== "true") return undefined;
  if (Object.keys(env).some((name) =>
    name.startsWith(MANAGED_CLOUD_ENV_PREFIX) &&
    name.endsWith(CALLER_RUNTIME_IDENTITY_SUFFIX)
  )) {
    throw new Error("Managed-cloud runtime identity cannot be configured");
  }
  const apiOrigin = normalizeManagedCloudWorkerOrigin(
    env.BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN,
  );
  if (env.NODE_ENV === "production" && !apiOrigin.startsWith("https://")) {
    throw new Error("Managed-cloud API origin is invalid");
  }
  const signingKey = required(env.BLUEY_JOBS_WORKER_SIGNING_KEY, "worker signing key");
  const workerId = required(env.BLUEY_JOBS_MANAGED_CLOUD_WORKER_ID, "worker ID");
  const grantId = required(env.BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_ID, "grant ID");
  const grantToken = required(env.BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_TOKEN, "grant token");
  const runtimeInstanceId = required(
    env.BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_INSTANCE_ID,
    "runtime instance ID",
  );
  const measurement = measureManagedCloudRuntimeIdentity(
    expectedRole,
    measurementOptions,
  );
  const scope = managedCloudScope(
    env.BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT,
    env.BLUEY_JOBS_MANAGED_CLOUD_REGION,
    env.BLUEY_JOBS_MANAGED_CLOUD_CHANNEL,
  );
  const heartbeatSeconds = optionalBoundedInteger(
    env.BLUEY_JOBS_MANAGED_CLOUD_HEARTBEAT_SECONDS,
    5,
    1,
    60,
    "heartbeat seconds",
  );
  if (!SAFE_ID.test(workerId)) throw new Error("Managed-cloud worker ID is invalid");
  if (!SAFE_ID.test(grantId) || !SAFE_ID.test(runtimeInstanceId)) {
    throw new Error("Managed-cloud runtime identity is invalid");
  }
  if (!TOKEN.test(grantToken)) throw new Error("Managed-cloud grant token is invalid");
  const signingKeyBytes = Buffer.byteLength(signingKey, "utf8");
  if (signingKeyBytes < 32 || signingKeyBytes > 4_096) {
    throw new Error("Managed-cloud worker signing key is invalid");
  }
  const rawGrantToken = Buffer.from(grantToken, "base64url");
  if (rawGrantToken.length !== 32 || rawGrantToken.toString("base64url") !== grantToken) {
    throw new Error("Managed-cloud grant token is invalid");
  }
  const sessionToken = createHmac("sha256", rawGrantToken)
    .update("bluey-jobs-managed-cloud-runtime-session-v1\0")
    .update(grantId)
    .update("\0")
    .update(runtimeInstanceId)
    .digest("base64url");
  return {
    apiOrigin,
    signingKey,
    workerId,
    grantId,
    grantToken,
    sessionToken,
    runtimeInstanceId,
    runtimeIdentitySha256: measurement.runtimeIdentitySha256,
    runtimeMeasurementSha256: measurement.runtimeMeasurementSha256,
    componentId: measurement.componentId,
    configSchemaSha256: measurement.configSchemaSha256,
    migrationSetSha256: measurement.migrationSetSha256,
    protocolSetSha256: measurement.protocolSetSha256,
    measurementOptions: Object.freeze({ ...measurementOptions }),
    scope,
    heartbeatIntervalMs: heartbeatSeconds * 1_000,
    expectedRole,
  };
}

export function assertManagedCloudRuntimeMeasurement(
  config: ManagedCloudRuntimeConfig,
): void {
  const measured = measureManagedCloudRuntimeIdentity(
    config.expectedRole,
    config.measurementOptions,
  );
  if (measured.runtimeIdentitySha256 !== config.runtimeIdentitySha256
    || measured.runtimeMeasurementSha256 !== config.runtimeMeasurementSha256
    || measured.componentId !== config.componentId
    || measured.configSchemaSha256 !== config.configSchemaSha256
    || measured.migrationSetSha256 !== config.migrationSetSha256
    || measured.protocolSetSha256 !== config.protocolSetSha256) {
    throw new Error("Managed-cloud runtime measurement changed after startup");
  }
}

export function measureManagedCloudRuntimeIdentity(
  expectedRole: ManagedCloudRuntimeRole,
  options: ManagedCloudRuntimeMeasurementOptions = {},
): ManagedCloudRuntimeMeasurementIdentity {
  const rootPath = resolve(options.rootPath ?? "/");
  const measurementPath = resolve(
    options.measurementPath ?? resolve(rootPath, RUNTIME_MEASUREMENT_PATH),
  );
  const relativeMeasurementPath = relative(rootPath, measurementPath)
    .split(sep)
    .join("/");
  if (relativeMeasurementPath !== RUNTIME_MEASUREMENT_PATH) {
    throw new Error("Managed-cloud runtime measurement path is invalid");
  }
  const bytes = readExactRegularFile(
    measurementPath,
    2,
    MAX_MEASUREMENT_BYTES,
    "runtime measurement",
  );
  let value: unknown;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("Managed-cloud runtime measurement JSON is invalid");
  }
  const measurement = parseRuntimeMeasurement(value, expectedRole);
  if (!bytes.equals(canonicalJsonBytes(measurement))) {
    throw new Error("Managed-cloud runtime measurement is not canonical");
  }
  const currentPaths = currentMeasuredRuntimePaths(
    rootPath,
    measurement.componentId,
  );
  if (currentPaths.length !== measurement.measuredFiles.length
    || currentPaths.some(
      (path, index) => path !== measurement.measuredFiles[index]?.path,
    )) {
    throw new Error("Managed-cloud measured file set changed after release verification");
  }
  for (const measuredFile of measurement.measuredFiles) {
    const path = resolve(rootPath, measuredFile.path);
    const relativePath = relative(rootPath, path).split(sep).join("/");
    if (relativePath !== measuredFile.path) {
      throw new Error("Managed-cloud measured file escapes the runtime root");
    }
    const fileBytes = readExactRegularFile(
      path,
      1,
      Number.MAX_SAFE_INTEGER,
      "measured file",
    );
    const actualSha256 = createHash("sha256").update(fileBytes).digest("hex");
    if (actualSha256 !== measuredFile.sha256) {
      throw new Error("Managed-cloud measured file digest is invalid");
    }
  }
  const runtimeMeasurementSha256 = createHash("sha256").update(bytes).digest("hex");
  const runtimeIdentitySha256 = createHash("sha256")
    .update(RUNTIME_IDENTITY_DOMAIN)
    .update(runtimeMeasurementSha256, "ascii")
    .update("\0")
    .update(measurement.componentId, "utf8")
    .update("\0")
    .update(expectedRole, "utf8")
    .digest("hex");
  return {
    componentId: measurement.componentId,
    configSchemaSha256: measurement.configSchemaSha256,
    migrationSetSha256: measurement.migrationSetSha256,
    protocolSetSha256: measurement.protocolSetSha256,
    runtimeIdentitySha256,
    runtimeMeasurementSha256,
  };
}

function currentMeasuredRuntimePaths(
  rootPath: string,
  componentId: ManagedCloudRuntimeComponentId,
): string[] {
  const paths: string[] = [];
  const roots = componentId === "jobs-api"
    ? ["usr/local/bin/bluey-jobs-api"]
    : componentId === "jobs-runner"
      ? ["app/automation", "app/runner", "ms-playwright", "usr/local/bin/node"]
      : ["app/automation", "app/workflows", "usr/local/bin/node"];

  function visit(path: string): void {
    let metadata: Stats;
    try {
      metadata = lstatSync(path);
    } catch {
      throw new Error("Managed-cloud measured runtime root is missing");
    }
    if (metadata.isSymbolicLink()) {
      throw new Error("Managed-cloud measured runtime roots cannot contain symbolic links");
    }
    if (metadata.isDirectory()) {
      const children = readdirSync(path, { withFileTypes: true })
        .sort((left, right) => compareUtf8(left.name, right.name));
      for (const child of children) visit(resolve(path, child.name));
      return;
    }
    if (!metadata.isFile()) {
      throw new Error("Managed-cloud measured runtime root contains a special file");
    }
    if (metadata.size === 0) return;
    const relativePath = relative(rootPath, path).split(sep).join("/");
    if (relativePath === RUNTIME_MEASUREMENT_PATH
      || relativePath.startsWith("../")
      || relativePath === "..") {
      throw new Error("Managed-cloud measured file escapes the runtime root");
    }
    paths.push(relativePath);
    if (paths.length > MAX_MEASURED_FILES) {
      throw new Error("Managed-cloud measured file set is invalid");
    }
  }

  for (const selectedRoot of roots) visit(resolve(rootPath, selectedRoot));
  return paths.sort(compareUtf8);
}

function readExactRegularFile(
  path: string,
  minimumBytes: number,
  maximumBytes: number,
  label: string,
): Buffer {
  let descriptor: number;
  try {
    descriptor = openSync(
      path,
      fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW,
    );
  } catch {
    throw new Error(`Managed-cloud ${label} cannot be opened safely`);
  }
  try {
    const metadata = fstatSync(descriptor);
    if (!metadata.isFile()
      || metadata.size < minimumBytes
      || metadata.size > maximumBytes) {
      throw new Error(`Managed-cloud ${label} is not a bounded regular file`);
    }
    const bytes = readFileSync(descriptor);
    if (bytes.length !== metadata.size) {
      throw new Error(`Managed-cloud ${label} changed while being read`);
    }
    return bytes;
  } finally {
    closeSync(descriptor);
  }
}

function managedCloudScope(
  environment: string | undefined,
  region: string | undefined,
  channel: string | undefined,
): ManagedCloudScope {
  if (environment !== "staging" && environment !== "production") {
    throw new Error("Managed-cloud environment is invalid");
  }
  if (channel !== "shadow" && channel !== "canary" && channel !== "general") {
    throw new Error("Managed-cloud channel is invalid");
  }
  if (typeof region !== "string"
    || region.length < 1
    || region.length > 64
    || !/^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/.test(region)) {
    throw new Error("Managed-cloud region is invalid");
  }
  return { environment, region, channel };
}

export function managedCloudRuntimeClaimRequest(
  config: ManagedCloudRuntimeConfig,
): ManagedCloudRuntimeClaimRequest {
  return {
    grantId: config.grantId,
    grantToken: config.grantToken,
    workerId: config.workerId,
    sessionToken: config.sessionToken,
    runtimeInstanceId: config.runtimeInstanceId,
    runtimeIdentitySha256: config.runtimeIdentitySha256,
  };
}

function parseRuntimeMeasurement(
  value: unknown,
  expectedRole: ManagedCloudRuntimeRole,
): ManagedCloudRuntimeMeasurement {
  const record = exactRecord(value, [
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
  ]);
  const componentId = runtimeComponentId(record.componentId);
  if (componentId !== componentForRuntimeRole(expectedRole)) {
    throw new Error("Managed-cloud runtime measurement component is invalid");
  }
  if (record.version !== 1
    || record.audience !== RUNTIME_MEASUREMENT_AUDIENCE
    || typeof record.buildId !== "string"
    || !RELEASE_ID.test(record.buildId)
    || typeof record.sourceCommit !== "string"
    || !SOURCE_COMMIT.test(record.sourceCommit)) {
    throw new Error("Managed-cloud runtime measurement identity is invalid");
  }
  const roles = runtimeRoles(record.roles, componentId);
  if (!roles.includes(expectedRole)) {
    throw new Error("Managed-cloud runtime measurement omits the process role");
  }
  const measuredFiles = measuredRuntimeFiles(record.measuredFiles);
  return {
    audience: RUNTIME_MEASUREMENT_AUDIENCE,
    buildId: record.buildId,
    componentId,
    configSchemaSha256: measurementSha256(
      record.configSchemaSha256,
      "config schema",
    ),
    measuredFiles,
    migrationSetSha256: measurementSha256(
      record.migrationSetSha256,
      "migration set",
    ),
    protocolSetSha256: measurementSha256(
      record.protocolSetSha256,
      "protocol set",
    ),
    roles,
    sourceCommit: record.sourceCommit,
    version: 1,
  };
}

function measuredRuntimeFiles(value: unknown): ManagedCloudRuntimeMeasurementFile[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > MAX_MEASURED_FILES) {
    throw new Error("Managed-cloud measured file set is invalid");
  }
  const paths = new Set<string>();
  const files = value.map((item) => {
    const record = exactRecord(item, ["path", "sha256"]);
    if (typeof record.path !== "string"
      || record.path !== record.path.normalize("NFC")
      || record.path.startsWith("/")
      || record.path.includes("\\")
      || record.path.split("/").some((part) => !part || part === "." || part === "..")
      || /[\u0000-\u001f\u007f\ud800-\udfff\ufffd]/.test(record.path)
      || record.path === RUNTIME_MEASUREMENT_PATH
      || paths.has(record.path)) {
      throw new Error("Managed-cloud measured file path is invalid");
    }
    paths.add(record.path);
    return {
      path: record.path,
      sha256: measurementSha256(record.sha256, "measured file"),
    };
  });
  if (files.some((file, index) => index > 0
    && compareUtf8(files[index - 1].path, file.path) >= 0)) {
    throw new Error("Managed-cloud measured files are not sorted");
  }
  return files;
}

function runtimeRoles(
  value: unknown,
  componentId: ManagedCloudRuntimeComponentId,
): ManagedCloudRuntimeRole[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > 4) {
    throw new Error("Managed-cloud runtime measurement roles are invalid");
  }
  const allowed = componentId === "jobs-api"
    ? [
      "jobs_api",
      "workflow_cleanup_dispatcher",
      "workflow_command_dispatcher",
    ] as const
    : componentId === "jobs-runner"
      ? ["managed_runner"] as const
      : [
        "discovery_worker",
        "global_discovery_worker",
        "original_source_verifier",
        "workflow_gateway",
        "workflow_worker",
      ] as const;
  const roles = value.map((item) => {
    if (
      typeof item !== "string" ||
      !(allowed as readonly string[]).includes(item)
    ) {
      throw new Error("Managed-cloud runtime measurement role is invalid");
    }
    return item as ManagedCloudRuntimeRole;
  });
  if (new Set(roles).size !== roles.length
    || roles.some((role, index) => index > 0
      && compareUtf8(roles[index - 1], role) >= 0)
    || (componentId === "jobs-api" && roles.length !== allowed.length)
    || (componentId === "jobs-runner" && roles.length !== 1)
    || (componentId === "jobs-workflows"
      && (!roles.includes("workflow_gateway") || !roles.includes("workflow_worker")))) {
    throw new Error("Managed-cloud runtime measurement role set is not exact");
  }
  return roles;
}

function componentForRuntimeRole(
  role: ManagedCloudRuntimeRole,
): ManagedCloudRuntimeComponentId {
  if (role === "jobs_api"
    || role === "workflow_cleanup_dispatcher"
    || role === "workflow_command_dispatcher") return "jobs-api";
  if (role === "managed_runner") return "jobs-runner";
  return "jobs-workflows";
}

function runtimeComponentId(value: unknown): ManagedCloudRuntimeComponentId {
  if (value !== "jobs-api" && value !== "jobs-runner" && value !== "jobs-workflows") {
    throw new Error("Managed-cloud runtime component is invalid");
  }
  return value;
}

function measurementSha256(value: unknown, label: string): string {
  if (typeof value !== "string" || !SHA256.test(value)) {
    throw new Error(`Managed-cloud ${label} SHA-256 is invalid`);
  }
  return value;
}

function exactRecord(
  value: unknown,
  keys: readonly string[],
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Managed-cloud runtime measurement object is invalid");
  }
  const record = value as Record<string, unknown>;
  const actual = Object.keys(record).sort(compareUtf8);
  const expected = [...keys].sort(compareUtf8);
  if (actual.length !== expected.length
    || actual.some((key, index) => key !== expected[index])) {
    throw new Error("Managed-cloud runtime measurement keys are invalid");
  }
  return record;
}

function compareUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function canonicalJsonBytes(value: unknown): Buffer {
  return Buffer.from(JSON.stringify(canonicalize(value)) + "\n", "utf8");
}

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value === null || typeof value === "boolean" || typeof value === "string") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || Object.is(value, -0)) {
      throw new Error("Managed-cloud runtime measurement number is invalid");
    }
    return value;
  }
  if (value && typeof value === "object") {
    const result: Record<string, unknown> = {};
    const record = value as Record<string, unknown>;
    for (const key of Object.keys(record).sort(compareUtf8)) {
      result[key] = canonicalize(record[key]);
    }
    return result;
  }
  throw new Error("Managed-cloud runtime measurement value is invalid");
}

function required(value: string | undefined, label: string): string {
  if (typeof value !== "string" || value.length === 0 || value !== value.trim()) {
    throw new Error(`Managed-cloud ${label} is required`);
  }
  return value;
}

function optionalBoundedInteger(
  value: string | undefined,
  fallback: number,
  minimum: number,
  maximum: number,
  label: string,
): number {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new Error(`Managed-cloud ${label} is invalid`);
  }
  return parsed;
}
