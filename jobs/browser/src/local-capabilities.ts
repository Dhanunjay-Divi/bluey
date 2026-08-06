export type LocalRunCapabilityOperation = "result" | "resume" | "submit";

export interface LocalRunCapabilities {
  readonly result: string;
  readonly resume: string;
  /**
   * Optional only so checkpoints written by an older Browser release remain
   * readable. New claims always require this value, and a restored legacy run
   * fails closed if it reaches the final-submit fence.
   */
  readonly submit?: string;
  readonly expiresAtMs: number;
  readonly runId: string;
  /** Missing only on a checkpoint created by a pre-release-authority Browser. */
  readonly release?: LocalRunReleaseBinding;
}

export interface LocalRunReleaseBinding {
  readonly descriptor_sha256: string;
  readonly manifest_sha256: string;
  readonly activation_sha256: string;
  readonly artifact_id: string;
  readonly artifact_sha256: string;
  readonly release_id: string;
  readonly build_id: string;
  readonly app_version: string;
  readonly protocol_version: number;
  readonly platform: "darwin" | "windows";
  readonly architecture: "arm64" | "x64";
  readonly channel: "internal" | "beta" | "stable";
  readonly trust_generation: number;
  readonly activation_generation: number;
  readonly channel_sequence: number;
}

export interface ParsedLocalRunClaim<T extends object> {
  request: T;
  capabilities: LocalRunCapabilities;
}

interface LocalRunCapabilityClaimsBase {
  audience: string;
  account_id: string;
  application_id: string;
  run_id: string;
  browser_profile_id: string;
  operation: LocalRunCapabilityOperation;
  expires_at_ms: number;
  nonce: string;
}

interface LegacyLocalRunCapabilityClaims extends LocalRunCapabilityClaimsBase {
  version: 1;
  release?: never;
}

interface CurrentLocalRunCapabilityClaims extends LocalRunCapabilityClaimsBase {
  version: 2;
  release: LocalRunReleaseBinding;
}

type LocalRunCapabilityClaims =
  | LegacyLocalRunCapabilityClaims
  | CurrentLocalRunCapabilityClaims;

interface LocalRunCapabilityBindings {
  accountId: string;
  applicationId: string;
  browserProfileId: string;
}

const CAPABILITY_AUDIENCE = "bluey-jobs-local-run";
const LEGACY_CAPABILITY_VERSION = 1;
const CAPABILITY_VERSION = 2;
const MAX_CAPABILITY_LENGTH = 4_096;
export const LOCAL_RUN_RECONCILIATION_GRACE_MS = 24 * 60 * 60 * 1_000;

export function parseLocalRunClaim<T extends object>(
  rawClaim: unknown,
  expectedRunId: string,
  nowMs = Date.now(),
): ParsedLocalRunClaim<T> {
  const claim = requireRecord(rawClaim, "claim response");
  const runId = requireBinding(claim.runId, "run");
  if (runId !== expectedRunId) throw new Error("Local run claim does not match the requested run");

  const bindings: LocalRunCapabilityBindings = {
    accountId: requireBinding(claim.accountId, "account"),
    applicationId: requireBinding(claim.applicationId, "application"),
    browserProfileId: requireBinding(claim.browserProfileId, "browser profile"),
  };
  const rawCapabilities = requireRecord(claim._blueyCapabilities, "local run capabilities");
  const responseRelease = parseLocalRunReleaseBinding(claim._blueyRelease);
  const expiresAtMs = requireExpiry(rawCapabilities.expiresAtMs, nowMs);
  const result = requireString(rawCapabilities.result, "result capability");
  const resume = requireString(rawCapabilities.resume, "resume capability");
  const submit = requireString(rawCapabilities.submit, "submit capability");
  if (new Set([result, resume, submit]).size !== 3) {
    throw new Error("Local run capabilities must be operation-scoped");
  }

  const resultClaims = parseLocalRunCapability(result, "result", runId, nowMs);
  const resumeClaims = parseLocalRunCapability(resume, "resume", runId, nowMs);
  const submitClaims = parseLocalRunCapability(submit, "submit", runId, nowMs);
  for (const capabilityClaims of [resultClaims, resumeClaims, submitClaims]) {
    if (capabilityClaims.expires_at_ms !== expiresAtMs
      || capabilityClaims.account_id !== bindings.accountId
      || capabilityClaims.application_id !== bindings.applicationId
      || capabilityClaims.browser_profile_id !== bindings.browserProfileId) {
      throw new Error("Local run capability bindings do not match the claim response");
    }
  }
  if (![resultClaims, resumeClaims, submitClaims].every(
    (capabilityClaims) => releaseBindingsMatch(
      capabilityClaims.release,
      responseRelease,
    ),
  )) {
    throw new Error("Local run release bindings do not match the claim response");
  }

  const request = { ...claim };
  delete request._blueyCapabilities;
  delete request._blueyRelease;
  return {
    request: request as T,
    capabilities: Object.freeze({
      result,
      resume,
      submit,
      expiresAtMs,
      runId,
      release: responseRelease,
    }),
  };
}

export function scopedLocalRunAuthorization(
  capabilities: LocalRunCapabilities,
  operation: LocalRunCapabilityOperation,
  nowMs = Date.now(),
): { capability: string } {
  const capability = capabilities[operation];
  if (!capability) throw new Error("Local run capability is unavailable");
  if (!Number.isSafeInteger(nowMs) || nowMs < 0) {
    throw new Error("Invalid local run capability time");
  }
  if (capabilities.expiresAtMs <= nowMs) throw new Error("Local run capability has expired");
  const claims = parseStoredLocalRunCapability(capabilities, capability, operation);
  assertStoredCapabilityBindings(capabilities, claims);
  return { capability };
}

/**
 * Returns only result authority for an irreversible run being reconciled after
 * a crash. The server remains authoritative for the exact 24-hour late window.
 * This path never revives resume or submit authority.
 */
export function scopedLocalRunReconciliationAuthorization(
  capabilities: LocalRunCapabilities,
  nowMs = Date.now(),
): { capability: string } {
  if (!Number.isSafeInteger(nowMs)
    || nowMs < 0
    || !Number.isSafeInteger(capabilities.expiresAtMs)
    || capabilities.expiresAtMs <= 0
    || (nowMs >= capabilities.expiresAtMs
      && nowMs - capabilities.expiresAtMs >= LOCAL_RUN_RECONCILIATION_GRACE_MS)) {
    throw new Error("Local run reconciliation capability has expired");
  }
  const capability = capabilities.result;
  const claims = parseStoredLocalRunCapability(capabilities, capability, "result");
  assertStoredCapabilityBindings(capabilities, claims);
  return { capability };
}

export function parseLocalRunCapability(
  capability: string,
  expectedOperation: LocalRunCapabilityOperation,
  expectedRunId: string,
  nowMs = Date.now(),
): CurrentLocalRunCapabilityClaims {
  if (!Number.isSafeInteger(nowMs) || nowMs < 0) {
    throw new Error("Invalid local run capability time");
  }
  const claims = parseLocalRunCapabilityClaims(
    capability,
    expectedOperation,
    expectedRunId,
  );
  if (claims.version !== CAPABILITY_VERSION || claims.expires_at_ms <= nowMs) {
    throw new Error("Invalid local run capability claims");
  }
  return claims;
}

function parseLocalRunCapabilityClaims(
  capability: string,
  expectedOperation: LocalRunCapabilityOperation,
  expectedRunId: string,
): LocalRunCapabilityClaims {
  if (typeof capability !== "string" || capability.length === 0 || capability.length > MAX_CAPABILITY_LENGTH) {
    throw new Error("Invalid local run capability");
  }
  const parts = capability.split(".");
  if (parts.length !== 2) throw new Error("Invalid local run capability");
  const [payload, signature] = parts;
  if (!payload || !/^[A-Za-z0-9_-]+$/.test(payload) || !/^[a-f0-9]{64}$/i.test(signature || "")) {
    throw new Error("Invalid local run capability");
  }

  let decoded: unknown;
  try {
    const bytes = Buffer.from(payload, "base64url");
    if (bytes.toString("base64url") !== payload) throw new Error("Non-canonical capability payload");
    decoded = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("Invalid local run capability");
  }
  const claims = requireRecord(decoded, "local run capability claims");
  const expiresAtMs = requireTimestamp(claims.expires_at_ms, "capability expiry");
  const accountId = requireBinding(claims.account_id, "account");
  const applicationId = requireBinding(claims.application_id, "application");
  const runId = requireBinding(claims.run_id, "run");
  const browserProfileId = requireBinding(claims.browser_profile_id, "browser profile");
  const nonce = requireString(claims.nonce, "capability nonce");
  const operation = claims.operation;
  const version = claims.version;
  if ((version !== LEGACY_CAPABILITY_VERSION && version !== CAPABILITY_VERSION)
    || claims.audience !== CAPABILITY_AUDIENCE
    || operation !== expectedOperation
    || runId !== expectedRunId
    || nonce.length < 24) {
    throw new Error("Invalid local run capability claims");
  }
  const common = {
    audience: CAPABILITY_AUDIENCE,
    account_id: accountId,
    application_id: applicationId,
    run_id: runId,
    browser_profile_id: browserProfileId,
    operation: expectedOperation,
    expires_at_ms: expiresAtMs,
    nonce,
  };
  if (version === LEGACY_CAPABILITY_VERSION) {
    assertExactKeys(claims, [
      "account_id",
      "application_id",
      "audience",
      "browser_profile_id",
      "expires_at_ms",
      "nonce",
      "operation",
      "run_id",
      "version",
    ]);
    return { version, ...common };
  }
  assertExactKeys(claims, [
    "account_id",
    "application_id",
    "audience",
    "browser_profile_id",
    "expires_at_ms",
    "nonce",
    "operation",
    "release",
    "run_id",
    "version",
  ]);
  return {
    version,
    ...common,
    release: parseLocalRunReleaseBinding(claims.release),
  };
}

function parseStoredLocalRunCapability(
  capabilities: LocalRunCapabilities,
  capability: string,
  operation: LocalRunCapabilityOperation,
): LocalRunCapabilityClaims {
  const claims = parseLocalRunCapabilityClaims(capability, operation, capabilities.runId);
  if (capabilities.release === undefined) {
    if (operation === "submit") throw new Error("Local run capability is unavailable");
    if (claims.version !== LEGACY_CAPABILITY_VERSION) {
      throw new Error("Invalid legacy local run capability");
    }
    return claims;
  }
  if (claims.version !== CAPABILITY_VERSION) {
    throw new Error("Invalid release-bound local run capability");
  }
  return claims;
}

function assertStoredCapabilityBindings(
  capabilities: LocalRunCapabilities,
  claims: LocalRunCapabilityClaims,
): void {
  if (claims.expires_at_ms !== capabilities.expiresAtMs) {
    throw new Error("Local run capability expiry does not match the claim response");
  }
  if (claims.version === CAPABILITY_VERSION) {
    const frozenRelease = parseLocalRunReleaseBinding(capabilities.release);
    if (!releaseBindingsMatch(claims.release, frozenRelease)) {
      throw new Error("Local run capability release does not match the claimed run");
    }
  }
}

function parseLocalRunReleaseBinding(value: unknown): LocalRunReleaseBinding {
  const release = requireRecord(value, "local run release binding");
  const expectedKeys = [
    "activation_generation",
    "activation_sha256",
    "app_version",
    "architecture",
    "artifact_id",
    "artifact_sha256",
    "build_id",
    "channel",
    "channel_sequence",
    "descriptor_sha256",
    "manifest_sha256",
    "platform",
    "protocol_version",
    "release_id",
    "trust_generation",
  ];
  const actualKeys = Object.keys(release).sort();
  if (
    actualKeys.length !== expectedKeys.length ||
    actualKeys.some((key, index) => key !== expectedKeys[index])
  ) {
    throw new Error("Invalid local run release binding");
  }
  const sha256 = (input: unknown): string => {
    const digest = requireString(input, "release SHA-256");
    if (!/^[a-f0-9]{64}$/.test(digest)) {
      throw new Error("Invalid local run release binding");
    }
    return digest;
  };
  const binding = (input: unknown): string => {
    const result = requireBinding(input, "release");
    if (result.length > 200) throw new Error("Invalid local run release binding");
    return result;
  };
  const positiveInteger = (input: unknown): number => {
    if (!Number.isSafeInteger(input) || (input as number) < 1) {
      throw new Error("Invalid local run release binding");
    }
    return input as number;
  };
  const platform = release.platform;
  const architecture = release.architecture;
  const channel = release.channel;
  if (
    (platform !== "darwin" && platform !== "windows") ||
    (architecture !== "arm64" && architecture !== "x64") ||
    (platform === "windows" && architecture !== "x64") ||
    (channel !== "internal" && channel !== "beta" && channel !== "stable")
  ) {
    throw new Error("Invalid local run release binding");
  }
  return Object.freeze({
    descriptor_sha256: sha256(release.descriptor_sha256),
    manifest_sha256: sha256(release.manifest_sha256),
    activation_sha256: sha256(release.activation_sha256),
    artifact_id: binding(release.artifact_id),
    artifact_sha256: sha256(release.artifact_sha256),
    release_id: binding(release.release_id),
    build_id: binding(release.build_id),
    app_version: binding(release.app_version),
    protocol_version: positiveInteger(release.protocol_version),
    platform,
    architecture,
    channel,
    trust_generation: positiveInteger(release.trust_generation),
    activation_generation: positiveInteger(release.activation_generation),
    channel_sequence: positiveInteger(release.channel_sequence),
  });
}

function releaseBindingsMatch(
  left: LocalRunReleaseBinding,
  right: LocalRunReleaseBinding,
): boolean {
  return left.descriptor_sha256 === right.descriptor_sha256
    && left.manifest_sha256 === right.manifest_sha256
    && left.activation_sha256 === right.activation_sha256
    && left.artifact_id === right.artifact_id
    && left.artifact_sha256 === right.artifact_sha256
    && left.release_id === right.release_id
    && left.build_id === right.build_id
    && left.app_version === right.app_version
    && left.protocol_version === right.protocol_version
    && left.platform === right.platform
    && left.architecture === right.architecture
    && left.channel === right.channel
    && left.trust_generation === right.trust_generation
    && left.activation_generation === right.activation_generation
    && left.channel_sequence === right.channel_sequence;
}

function requireRecord(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`Invalid ${label}`);
  }
  return value as Record<string, unknown>;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) throw new Error(`Invalid ${label}`);
  return value;
}

function requireBinding(value: unknown, label: string): string {
  const binding = requireString(value, `${label} binding`);
  if (binding.trim() !== binding || binding.length > 200) throw new Error(`Invalid ${label} binding`);
  return binding;
}

function requireTimestamp(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) <= 0) {
    throw new Error(`Invalid ${label}`);
  }
  return value as number;
}

function assertExactKeys(value: Record<string, unknown>, expected: readonly string[]): void {
  const actual = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  if (actual.length !== sortedExpected.length
    || actual.some((key, index) => key !== sortedExpected[index])) {
    throw new Error("Invalid local run capability claims");
  }
}

function requireExpiry(value: unknown, nowMs: number): number {
  if (!Number.isSafeInteger(value) || (value as number) <= nowMs) {
    throw new Error("Local run capability has expired or has an invalid expiry");
  }
  return value as number;
}
