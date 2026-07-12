import { describe, expect, it } from "vitest";
import {
  parseLocalRunCapability,
  parseLocalRunClaim,
  scopedLocalRunAuthorization,
} from "../src/local-capabilities.js";

const NOW_MS = 1_000;
const EXPIRES_AT_MS = 2_000;

describe("local run capabilities", () => {
  it("parses scoped claim capabilities and removes them from the run request", () => {
    const rawClaim = claimResponse();
    const parsed = parseLocalRunClaim<Record<string, unknown>>(rawClaim, "run-123", NOW_MS);

    expect(parsed.request).toEqual({
      runId: "run-123",
      accountId: "account-123",
      applicationId: "application-123",
      browserProfileId: "profile-123",
      applicationIdentityId: "identity-123",
      url: "https://jobs.example.test/apply",
      packet: { applicationId: "application-123" },
    });
    expect(parsed.request).not.toHaveProperty("_blueyCapabilities");
    expect(parsed.capabilities).toEqual({
      result: capability("result"),
      resume: capability("resume"),
      expiresAtMs: EXPIRES_AT_MS,
      runId: "run-123",
    });
  });

  it("produces operation-specific authorization without a root ticket", () => {
    const { capabilities } = parseLocalRunClaim<Record<string, unknown>>(
      claimResponse(),
      "run-123",
      NOW_MS,
    );

    expect(scopedLocalRunAuthorization(capabilities, "result", NOW_MS)).toEqual({
      capability: capability("result"),
    });
    expect(scopedLocalRunAuthorization(capabilities, "resume", NOW_MS)).toEqual({
      capability: capability("resume"),
    });
    expect(scopedLocalRunAuthorization(capabilities, "result", NOW_MS)).not.toHaveProperty("ticket");
  });

  it("rejects missing, malformed, swapped, and mismatched claim capabilities", () => {
    const missing = claimResponse();
    delete missing._blueyCapabilities;
    expect(() => parseLocalRunClaim(missing, "run-123", NOW_MS)).toThrow();

    const malformed = claimResponse();
    malformed._blueyCapabilities.result = "malformed";
    expect(() => parseLocalRunClaim(malformed, "run-123", NOW_MS)).toThrow();

    const swapped = claimResponse();
    swapped._blueyCapabilities.result = capability("resume");
    swapped._blueyCapabilities.resume = capability("result");
    expect(() => parseLocalRunClaim(swapped, "run-123", NOW_MS)).toThrow();

    const wrongRun = claimResponse();
    wrongRun._blueyCapabilities.result = capability("result", { run_id: "run-other" });
    expect(() => parseLocalRunClaim(wrongRun, "run-123", NOW_MS)).toThrow();

    const wrongBinding = claimResponse();
    wrongBinding._blueyCapabilities.resume = capability("resume", { account_id: "account-other" });
    expect(() => parseLocalRunClaim(wrongBinding, "run-123", NOW_MS)).toThrow();

    const wrongExpiry = claimResponse();
    wrongExpiry._blueyCapabilities.expiresAtMs = EXPIRES_AT_MS + 1;
    expect(() => parseLocalRunClaim(wrongExpiry, "run-123", NOW_MS)).toThrow();
  });

  it("rejects expired capabilities both at claim and at use time", () => {
    expect(() => parseLocalRunClaim(claimResponse(), "run-123", EXPIRES_AT_MS)).toThrow();

    const { capabilities } = parseLocalRunClaim<Record<string, unknown>>(
      claimResponse(),
      "run-123",
      NOW_MS,
    );
    expect(() => scopedLocalRunAuthorization(capabilities, "result", EXPIRES_AT_MS)).toThrow();
    expect(() => scopedLocalRunAuthorization(capabilities, "resume", EXPIRES_AT_MS)).toThrow();
  });

  it("rejects malformed token shape and non-resume authority in resume links", () => {
    expect(() => parseLocalRunCapability("abc.def", "resume", "run-123", NOW_MS)).toThrow();
    expect(() => parseLocalRunCapability(capability("result"), "resume", "run-123", NOW_MS)).toThrow();
    expect(() => parseLocalRunCapability(
      capability("resume", { expires_at_ms: NOW_MS }),
      "resume",
      "run-123",
      NOW_MS,
    )).toThrow();
  });
});

function claimResponse() {
  return {
    runId: "run-123",
    accountId: "account-123",
    applicationId: "application-123",
    browserProfileId: "profile-123",
    applicationIdentityId: "identity-123",
    url: "https://jobs.example.test/apply",
    packet: { applicationId: "application-123" },
    _blueyCapabilities: {
      result: capability("result"),
      resume: capability("resume"),
      expiresAtMs: EXPIRES_AT_MS,
    },
  };
}

function capability(
  operation: "result" | "resume",
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
    expires_at_ms: EXPIRES_AT_MS,
    nonce: "n".repeat(32),
    ...overrides,
  };
  return `${Buffer.from(JSON.stringify(claims)).toString("base64url")}.${"a".repeat(64)}`;
}
