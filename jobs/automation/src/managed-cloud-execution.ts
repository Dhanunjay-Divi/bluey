import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import type { ManagedCloudScope } from "./managed-cloud-runtime.js";

const SHA256 = /^[a-f0-9]{64}$/;
const TOKEN = /^[A-Za-z0-9_.:/-]{1,128}$/;
const RECOVERY_AUDIENCE = "bluey-jobs-managed-cloud-recovery-authorization-v1";

export const MANAGED_CLOUD_RELEASE_MEMO_KEY = "bluey_jobs_managed_cloud_release_v1";

export interface ManagedCloudExecutionAuthority {
  bindingSha256: string;
  scope: ManagedCloudScope;
  headRevision: number;
  transitionSha256: string;
  activationSha256: string;
  manifestSha256: string;
  cohortSha256: string;
  trustGeneration: number;
  channelSequence: number;
  releaseId: string;
  releaseSequence: number;
  taskQueueSha256: string;
  failureConverterSha256: string;
  readinessSha256: string;
  activationExpiresAtMs: number;
  resolvedAtMs: number;
}

export interface ManagedCloudCurrentAuthorization {
  currentHeadRevision: number;
  currentTransitionSha256: string;
  currentActivationSha256: string;
  currentManifestSha256: string;
  currentActivationExpiresAtMs: number;
  currentTaskQueueSha256: string;
  currentFailureConverterSha256: string;
  currentReadinessSha256: string;
  recoveryAccepted: boolean;
  recoveryAuthorizationSha256: string;
  authorizedAtMs: number;
}

export interface ManagedCloudGatewayAuthority {
  version: 1;
  execution: ManagedCloudExecutionAuthority;
  authorization: ManagedCloudCurrentAuthorization;
}

export type ManagedCloudReleaseMemoAuthority = { version: 1 }
  & ManagedCloudExecutionAuthority;

export interface ManagedCloudRuntimeReleaseIdentity {
  scope: ManagedCloudScope;
  role: string;
  headRevision: number;
  transitionSha256: string;
  activationSha256: string;
  manifestSha256: string;
  taskQueueSha256: string;
  failureConverterSha256: string;
  activationExpiresAtMs: number;
}

export function parseManagedCloudGatewayAuthority(
  value: unknown,
): ManagedCloudGatewayAuthority {
  const record = exactRecord(value, ["authorization", "execution", "version"]);
  if (record.version !== 1) throw invalidAuthority();
  const authority = Object.freeze({
    version: 1,
    execution: parseExecution(record.execution),
    authorization: parseAuthorization(record.authorization),
  } satisfies ManagedCloudGatewayAuthority);
  const { execution, authorization } = authority;
  const exactCurrent = execution.headRevision === authorization.currentHeadRevision
    && execution.transitionSha256 === authorization.currentTransitionSha256
    && execution.activationSha256 === authorization.currentActivationSha256
    && execution.manifestSha256 === authorization.currentManifestSha256
    && execution.activationExpiresAtMs === authorization.currentActivationExpiresAtMs
    && execution.taskQueueSha256 === authorization.currentTaskQueueSha256
    && execution.failureConverterSha256 === authorization.currentFailureConverterSha256;
  if ((!authorization.recoveryAccepted && !exactCurrent)
    || (authorization.recoveryAccepted && exactCurrent)
    || authorization.authorizedAtMs < execution.resolvedAtMs
    || authorization.authorizedAtMs >= authorization.currentActivationExpiresAtMs
    || recoveryAuthorizationSha256(authority)
      !== authorization.recoveryAuthorizationSha256) {
    throw invalidAuthority();
  }
  return authority;
}

export function managedCloudReleaseMemo(
  authority: ManagedCloudGatewayAuthority,
): ManagedCloudReleaseMemoAuthority {
  const execution = authority.execution;
  return Object.freeze({
    activationExpiresAtMs: execution.activationExpiresAtMs,
    activationSha256: execution.activationSha256,
    bindingSha256: execution.bindingSha256,
    channelSequence: execution.channelSequence,
    cohortSha256: execution.cohortSha256,
    failureConverterSha256: execution.failureConverterSha256,
    headRevision: execution.headRevision,
    manifestSha256: execution.manifestSha256,
    readinessSha256: execution.readinessSha256,
    releaseId: execution.releaseId,
    releaseSequence: execution.releaseSequence,
    resolvedAtMs: execution.resolvedAtMs,
    scope: Object.freeze({
      channel: execution.scope.channel,
      environment: execution.scope.environment,
      region: execution.scope.region,
    }),
    taskQueueSha256: execution.taskQueueSha256,
    transitionSha256: execution.transitionSha256,
    trustGeneration: execution.trustGeneration,
    version: 1,
  });
}

export function parseManagedCloudReleaseMemo(
  value: unknown,
): ManagedCloudReleaseMemoAuthority {
  const record = exactRecord(value, [
    "activationExpiresAtMs", "activationSha256", "bindingSha256", "channelSequence",
    "cohortSha256", "failureConverterSha256", "headRevision", "manifestSha256",
    "readinessSha256", "releaseId", "releaseSequence", "resolvedAtMs", "scope",
    "taskQueueSha256", "transitionSha256", "trustGeneration", "version",
  ]);
  if (record.version !== 1) throw invalidAuthority();
  const { version: _, ...execution } = record;
  return Object.freeze({ version: 1, ...parseExecution(execution) });
}

export function managedCloudReleaseMemoBytes(value: unknown): Uint8Array {
  const canonical = managedCloudCanonicalJsonBytes(parseManagedCloudReleaseMemo(value));
  return canonical.slice(0, -1);
}

export function managedCloudGatewayMatchesRuntime(
  authority: ManagedCloudGatewayAuthority,
  runtime: ManagedCloudRuntimeReleaseIdentity,
  expectedRole: string,
): boolean {
  const { execution, authorization } = authority;
  if (runtime.role !== expectedRole
    || !sameScope(runtime.scope, execution.scope)
    || runtime.headRevision !== authorization.currentHeadRevision
    || runtime.transitionSha256 !== authorization.currentTransitionSha256
    || runtime.activationSha256 !== authorization.currentActivationSha256
    || runtime.manifestSha256 !== authorization.currentManifestSha256
    || runtime.activationExpiresAtMs !== authorization.currentActivationExpiresAtMs
    || runtime.taskQueueSha256 !== authorization.currentTaskQueueSha256
    || runtime.failureConverterSha256 !== authorization.currentFailureConverterSha256) {
    return false;
  }
  return recoveryAuthorizationSha256(authority)
    === authorization.recoveryAuthorizationSha256;
}

export function recoveryAuthorizationSha256(
  authority: ManagedCloudGatewayAuthority,
): string {
  const { execution, authorization } = authority;
  return createHash("sha256").update(managedCloudCanonicalJsonBytes({
    version: 1,
    audience: RECOVERY_AUDIENCE,
    bindingSha256: execution.bindingSha256,
    currentHeadRevision: authorization.currentHeadRevision,
    currentTransitionSha256: authorization.currentTransitionSha256,
    currentActivationSha256: authorization.currentActivationSha256,
    currentManifestSha256: authorization.currentManifestSha256,
    currentActivationExpiresAtMs: authorization.currentActivationExpiresAtMs,
    currentTaskQueueSha256: authorization.currentTaskQueueSha256,
    currentFailureConverterSha256: authorization.currentFailureConverterSha256,
    currentReadinessSha256: authorization.currentReadinessSha256,
    frozenActivationSha256: execution.activationSha256,
    frozenManifestSha256: execution.manifestSha256,
    recoveryAccepted: authorization.recoveryAccepted,
    authorizedAtMs: authorization.authorizedAtMs,
  })).digest("hex");
}

export function managedCloudCanonicalJsonBytes(value: unknown): Uint8Array {
  return new TextEncoder().encode(`${canonicalJson(value)}\n`);
}

function parseExecution(value: unknown): ManagedCloudExecutionAuthority {
  const record = exactRecord(value, [
    "activationExpiresAtMs", "activationSha256", "bindingSha256", "channelSequence",
    "cohortSha256", "failureConverterSha256", "headRevision", "manifestSha256",
    "readinessSha256", "releaseId", "releaseSequence", "resolvedAtMs", "scope",
    "taskQueueSha256", "transitionSha256", "trustGeneration",
  ]);
  const execution = {
    bindingSha256: digest(record.bindingSha256),
    scope: parseScope(record.scope),
    headRevision: positiveInteger(record.headRevision),
    transitionSha256: digest(record.transitionSha256),
    activationSha256: digest(record.activationSha256),
    manifestSha256: digest(record.manifestSha256),
    cohortSha256: digest(record.cohortSha256),
    trustGeneration: positiveInteger(record.trustGeneration),
    channelSequence: positiveInteger(record.channelSequence),
    releaseId: token(record.releaseId),
    releaseSequence: positiveInteger(record.releaseSequence),
    taskQueueSha256: digest(record.taskQueueSha256),
    failureConverterSha256: digest(record.failureConverterSha256),
    readinessSha256: digest(record.readinessSha256),
    activationExpiresAtMs: positiveInteger(record.activationExpiresAtMs),
    resolvedAtMs: positiveInteger(record.resolvedAtMs),
  } satisfies ManagedCloudExecutionAuthority;
  if (execution.scope.channel === "shadow"
    || execution.resolvedAtMs >= execution.activationExpiresAtMs) {
    throw invalidAuthority();
  }
  return Object.freeze(execution);
}

function parseAuthorization(value: unknown): ManagedCloudCurrentAuthorization {
  const record = exactRecord(value, [
    "authorizedAtMs", "currentActivationExpiresAtMs", "currentActivationSha256",
    "currentFailureConverterSha256", "currentHeadRevision", "currentManifestSha256",
    "currentReadinessSha256", "currentTaskQueueSha256", "currentTransitionSha256",
    "recoveryAccepted", "recoveryAuthorizationSha256",
  ]);
  if (typeof record.recoveryAccepted !== "boolean") throw invalidAuthority();
  return Object.freeze({
    currentHeadRevision: positiveInteger(record.currentHeadRevision),
    currentTransitionSha256: digest(record.currentTransitionSha256),
    currentActivationSha256: digest(record.currentActivationSha256),
    currentManifestSha256: digest(record.currentManifestSha256),
    currentActivationExpiresAtMs: positiveInteger(record.currentActivationExpiresAtMs),
    currentTaskQueueSha256: digest(record.currentTaskQueueSha256),
    currentFailureConverterSha256: digest(record.currentFailureConverterSha256),
    currentReadinessSha256: digest(record.currentReadinessSha256),
    recoveryAccepted: record.recoveryAccepted,
    recoveryAuthorizationSha256: digest(record.recoveryAuthorizationSha256),
    authorizedAtMs: positiveInteger(record.authorizedAtMs),
  });
}

function parseScope(value: unknown): ManagedCloudScope {
  const record = exactRecord(value, ["channel", "environment", "region"]);
  if ((record.environment !== "staging" && record.environment !== "production")
    || (record.channel !== "shadow" && record.channel !== "canary" && record.channel !== "general")
    || typeof record.region !== "string"
    || !/^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/.test(record.region)) {
    throw invalidAuthority();
  }
  return Object.freeze({
    environment: record.environment,
    region: record.region,
    channel: record.channel,
  });
}

function sameScope(left: ManagedCloudScope, right: ManagedCloudScope): boolean {
  return left.environment === right.environment
    && left.region === right.region
    && left.channel === right.channel;
}

function exactRecord(value: unknown, expectedKeys: readonly string[]): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw invalidAuthority();
  const record = value as Record<string, unknown>;
  const actual = Object.keys(record).sort();
  const expected = [...expectedKeys].sort();
  if (actual.length !== expected.length
    || actual.some((key, index) => key !== expected[index])) {
    throw invalidAuthority();
  }
  return record;
}

function digest(value: unknown): string {
  if (typeof value !== "string" || !SHA256.test(value)) throw invalidAuthority();
  return value;
}

function token(value: unknown): string {
  if (typeof value !== "string" || !TOKEN.test(value)) throw invalidAuthority();
  return value;
}

function positiveInteger(value: unknown): number {
  if (!Number.isSafeInteger(value) || (value as number) < 1) throw invalidAuthority();
  return value as number;
}

function canonicalJson(value: unknown): string {
  if (value === null || typeof value === "boolean" || typeof value === "string") {
    if (typeof value === "string"
      && (Buffer.from(value, "utf8").toString("utf8") !== value
        || value.includes("\ufffd"))) {
      throw invalidAuthority();
    }
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || Object.is(value, -0)) throw invalidAuthority();
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (!value || typeof value !== "object") throw invalidAuthority();
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record);
  if (keys.some((key) => Buffer.from(key, "utf8").toString("utf8") !== key
    || key.includes("\ufffd"))) {
    throw invalidAuthority();
  }
  return `{${keys.sort((left, right) => (
    Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"))
  )).map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(record[key])}`
  )).join(",")}}`;
}

function invalidAuthority(): Error {
  return new Error("Managed-cloud execution authority is invalid");
}
