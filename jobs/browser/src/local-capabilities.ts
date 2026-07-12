export type LocalRunCapabilityOperation = "result" | "resume";

export interface LocalRunCapabilities {
  readonly result: string;
  readonly resume: string;
  readonly expiresAtMs: number;
  readonly runId: string;
}

export interface ParsedLocalRunClaim<T extends object> {
  request: T;
  capabilities: LocalRunCapabilities;
}

interface LocalRunCapabilityClaims {
  version: number;
  audience: string;
  account_id: string;
  application_id: string;
  run_id: string;
  browser_profile_id: string;
  operation: LocalRunCapabilityOperation;
  expires_at_ms: number;
  nonce: string;
}

interface LocalRunCapabilityBindings {
  accountId: string;
  applicationId: string;
  browserProfileId: string;
}

const CAPABILITY_AUDIENCE = "bluey-jobs-local-run";
const CAPABILITY_VERSION = 1;
const MAX_CAPABILITY_LENGTH = 4_096;

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
  const expiresAtMs = requireExpiry(rawCapabilities.expiresAtMs, nowMs);
  const result = requireString(rawCapabilities.result, "result capability");
  const resume = requireString(rawCapabilities.resume, "resume capability");
  if (result === resume) throw new Error("Local run capabilities must be operation-scoped");

  const resultClaims = parseLocalRunCapability(result, "result", runId, nowMs);
  const resumeClaims = parseLocalRunCapability(resume, "resume", runId, nowMs);
  for (const capabilityClaims of [resultClaims, resumeClaims]) {
    if (capabilityClaims.expires_at_ms !== expiresAtMs
      || capabilityClaims.account_id !== bindings.accountId
      || capabilityClaims.application_id !== bindings.applicationId
      || capabilityClaims.browser_profile_id !== bindings.browserProfileId) {
      throw new Error("Local run capability bindings do not match the claim response");
    }
  }

  const request = { ...claim };
  delete request._blueyCapabilities;
  return {
    request: request as T,
    capabilities: Object.freeze({ result, resume, expiresAtMs, runId }),
  };
}

export function scopedLocalRunAuthorization(
  capabilities: LocalRunCapabilities,
  operation: LocalRunCapabilityOperation,
  nowMs = Date.now(),
): { capability: string } {
  if (capabilities.expiresAtMs <= nowMs) throw new Error("Local run capability has expired");
  const capability = capabilities[operation];
  const claims = parseLocalRunCapability(capability, operation, capabilities.runId, nowMs);
  if (claims.expires_at_ms !== capabilities.expiresAtMs) {
    throw new Error("Local run capability expiry does not match the claim response");
  }
  return { capability };
}

export function parseLocalRunCapability(
  capability: string,
  expectedOperation: LocalRunCapabilityOperation,
  expectedRunId: string,
  nowMs = Date.now(),
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
  const expiresAtMs = requireExpiry(claims.expires_at_ms, nowMs);
  const accountId = requireBinding(claims.account_id, "account");
  const applicationId = requireBinding(claims.application_id, "application");
  const runId = requireBinding(claims.run_id, "run");
  const browserProfileId = requireBinding(claims.browser_profile_id, "browser profile");
  const nonce = requireString(claims.nonce, "capability nonce");
  const operation = claims.operation;
  if (claims.version !== CAPABILITY_VERSION
    || claims.audience !== CAPABILITY_AUDIENCE
    || operation !== expectedOperation
    || runId !== expectedRunId
    || nonce.length < 24) {
    throw new Error("Invalid local run capability claims");
  }
  return {
    version: CAPABILITY_VERSION,
    audience: CAPABILITY_AUDIENCE,
    account_id: accountId,
    application_id: applicationId,
    run_id: runId,
    browser_profile_id: browserProfileId,
    operation: expectedOperation,
    expires_at_ms: expiresAtMs,
    nonce,
  };
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

function requireExpiry(value: unknown, nowMs: number): number {
  if (!Number.isSafeInteger(value) || (value as number) <= nowMs) {
    throw new Error("Local run capability has expired or has an invalid expiry");
  }
  return value as number;
}
