import type {
  LocalRunCapabilities,
  LocalRunCapabilityOperation,
  LocalRunReleaseBinding,
} from "../../src/local-capabilities.js";

export function localRunReleaseFixture(
  overrides: Partial<LocalRunReleaseBinding> = {},
): LocalRunReleaseBinding {
  return {
    descriptor_sha256: "a".repeat(64),
    automation_bundle_sha256: "e".repeat(64),
    chromium_executable_sha256: "f".repeat(64),
    manifest_sha256: "b".repeat(64),
    activation_sha256: "c".repeat(64),
    artifact_id: "browser-artifact-603-1-darwin-arm64",
    artifact_sha256: "d".repeat(64),
    release_id: "browser-release-603-1",
    build_id: "browser-603.1",
    app_version: "0.1.0",
    protocol_version: 1,
    platform: "darwin",
    architecture: "arm64",
    channel: "beta",
    trust_generation: 1,
    activation_generation: 1,
    channel_sequence: 1,
    ...overrides,
  };
}

export function localRunCapabilityFixture(
  operation: LocalRunCapabilityOperation,
  expiresAtMs: number,
  overrides: Record<string, unknown> = {},
): string {
  const claims = {
    version: 2,
    audience: "bluey-jobs-local-run",
    account_id: "account-123",
    application_id: "application-123",
    run_id: "run-123",
    browser_profile_id: "profile-123",
    operation,
    expires_at_ms: expiresAtMs,
    nonce: `${operation}-`.padEnd(32, "n"),
    release: localRunReleaseFixture(),
    ...overrides,
  };
  return `${Buffer.from(JSON.stringify(claims)).toString("base64url")}.${"a".repeat(64)}`;
}

export function legacyLocalRunCapabilityFixture(
  operation: LocalRunCapabilityOperation,
  expiresAtMs: number,
  overrides: Record<string, unknown> = {},
): string {
  const claims = {
    version: 1,
    audience: "bluey-jobs-local-run",
    account_id: "account-123",
    application_id: "application-123",
    run_id: "run-123",
    browser_profile_id: "profile-123",
    operation,
    expires_at_ms: expiresAtMs,
    nonce: `legacy-${operation}-`.padEnd(32, "n"),
    ...overrides,
  };
  return `${Buffer.from(JSON.stringify(claims)).toString("base64url")}.${"a".repeat(64)}`;
}

export function localRunCapabilitiesFixture(
  expiresAtMs: number,
  release: LocalRunReleaseBinding = localRunReleaseFixture(),
): LocalRunCapabilities {
  return {
    runId: "run-123",
    expiresAtMs,
    result: localRunCapabilityFixture("result", expiresAtMs, { release }),
    resume: localRunCapabilityFixture("resume", expiresAtMs, { release }),
    submit: localRunCapabilityFixture("submit", expiresAtMs, { release }),
    release,
  };
}

export function legacyLocalRunCapabilitiesFixture(
  expiresAtMs: number,
): LocalRunCapabilities {
  return {
    runId: "run-123",
    expiresAtMs,
    result: legacyLocalRunCapabilityFixture("result", expiresAtMs),
    resume: legacyLocalRunCapabilityFixture("resume", expiresAtMs),
    submit: legacyLocalRunCapabilityFixture("submit", expiresAtMs),
  };
}
