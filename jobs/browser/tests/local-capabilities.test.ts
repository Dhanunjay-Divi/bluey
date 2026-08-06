import { describe, expect, it } from "vitest";
import {
  LOCAL_RUN_RECONCILIATION_GRACE_MS,
  parseLocalRunCapability,
  parseLocalRunClaim,
  scopedLocalRunAuthorization,
  scopedLocalRunReconciliationAuthorization,
} from "../src/local-capabilities.js";
import {
  legacyLocalRunCapabilitiesFixture,
  legacyLocalRunCapabilityFixture,
  localRunCapabilityFixture,
  localRunReleaseFixture,
} from "./fixtures/local-run-capability.js";

const NOW_MS = 1_000;
const EXPIRES_AT_MS = 2_000;

describe("local run capabilities", () => {
  it("parses one exact release-bound claim and removes all authority from the run request", () => {
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
    expect(parsed.request).not.toHaveProperty("_blueyRelease");
    expect(parsed.capabilities).toEqual({
      result: capability("result"),
      resume: capability("resume"),
      submit: capability("submit"),
      expiresAtMs: EXPIRES_AT_MS,
      runId: "run-123",
      release: localRunReleaseFixture(),
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
    expect(scopedLocalRunAuthorization(capabilities, "submit", NOW_MS)).toEqual({
      capability: capability("submit"),
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

    const missingSubmit = claimResponse();
    delete missingSubmit._blueyCapabilities.submit;
    expect(() => parseLocalRunClaim(missingSubmit, "run-123", NOW_MS)).toThrow();

    const missingRelease = claimResponse();
    delete missingRelease._blueyRelease;
    expect(() => parseLocalRunClaim(missingRelease, "run-123", NOW_MS)).toThrow();

    const swapped = claimResponse();
    swapped._blueyCapabilities.submit = capability("resume");
    swapped._blueyCapabilities.resume = capability("submit");
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

    const responseReleaseMismatch = claimResponse();
    responseReleaseMismatch._blueyRelease = localRunReleaseFixture({
      activation_sha256: "e".repeat(64),
    });
    expect(() =>
      parseLocalRunClaim(responseReleaseMismatch, "run-123", NOW_MS),
    ).toThrow(/release bindings/i);

    for (const operation of ["result", "resume", "submit"] as const) {
      const capabilityReleaseMismatch = claimResponse();
      capabilityReleaseMismatch._blueyCapabilities[operation] = capability(operation, {
        release: localRunReleaseFixture({ artifact_sha256: "e".repeat(64) }),
      });
      expect(() =>
        parseLocalRunClaim(capabilityReleaseMismatch, "run-123", NOW_MS),
      ).toThrow(/release bindings/i);
    }

    const releaseWithUnknownField = claimResponse();
    releaseWithUnknownField._blueyRelease = {
      ...localRunReleaseFixture(),
      unexpected: true,
    };
    expect(() =>
      parseLocalRunClaim(releaseWithUnknownField, "run-123", NOW_MS),
    ).toThrow(/release binding/i);
  });

  it("keeps the frozen release usable for recovery but rejects rebinding every operation", () => {
    const { capabilities } = parseLocalRunClaim<Record<string, unknown>>(
      claimResponse(),
      "run-123",
      NOW_MS,
    );
    for (const operation of ["result", "resume", "submit"] as const) {
      expect(scopedLocalRunAuthorization(capabilities, operation, NOW_MS)).toEqual({
        capability: capability(operation),
      });
    }

    const laterRelease = localRunReleaseFixture({
      activation_sha256: "e".repeat(64),
      activation_generation: 2,
      channel_sequence: 2,
    });
    for (const operation of ["result", "resume", "submit"] as const) {
      expect(() =>
        scopedLocalRunAuthorization(
          { ...capabilities, release: laterRelease },
          operation,
          NOW_MS,
        ),
      ).toThrow(/release does not match/i);
    }
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
    expect(() => scopedLocalRunAuthorization(capabilities, "submit", EXPIRES_AT_MS)).toThrow();
    expect(scopedLocalRunReconciliationAuthorization(
      capabilities,
      EXPIRES_AT_MS,
    )).toEqual({ capability: capability("result") });
    expect(() => scopedLocalRunReconciliationAuthorization(
      capabilities,
      EXPIRES_AT_MS + LOCAL_RUN_RECONCILIATION_GRACE_MS,
    )).toThrow();
  });

  it("keeps genuine legacy result and resume authority but never legacy submit authority", () => {
    const capabilities = legacyLocalRunCapabilitiesFixture(EXPIRES_AT_MS);

    expect(scopedLocalRunAuthorization(capabilities, "result", NOW_MS)).toEqual({
      capability: legacyLocalRunCapabilityFixture("result", EXPIRES_AT_MS),
    });
    expect(scopedLocalRunAuthorization(capabilities, "resume", NOW_MS)).toEqual({
      capability: legacyLocalRunCapabilityFixture("resume", EXPIRES_AT_MS),
    });
    expect(() => scopedLocalRunAuthorization(capabilities, "submit", NOW_MS)).toThrow(
      /unavailable/i,
    );
    expect(scopedLocalRunReconciliationAuthorization(capabilities, EXPIRES_AT_MS)).toEqual({
      capability: legacyLocalRunCapabilityFixture("result", EXPIRES_AT_MS),
    });
    expect(() => parseLocalRunCapability(
      legacyLocalRunCapabilityFixture("result", EXPIRES_AT_MS),
      "result",
      "run-123",
      NOW_MS,
    )).toThrow();
  });

  it("does not let removing or adding a release binding change a capability version", () => {
    const current = parseLocalRunClaim<Record<string, unknown>>(
      claimResponse(),
      "run-123",
      NOW_MS,
    ).capabilities;
    expect(() => scopedLocalRunAuthorization(
      { ...current, release: undefined },
      "result",
      NOW_MS,
    )).toThrow(/legacy/i);

    const legacy = legacyLocalRunCapabilitiesFixture(EXPIRES_AT_MS);
    expect(() => scopedLocalRunAuthorization(
      { ...legacy, release: localRunReleaseFixture() },
      "result",
      NOW_MS,
    )).toThrow(/release-bound/i);
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
    _blueyRelease: localRunReleaseFixture(),
    _blueyCapabilities: {
      result: capability("result"),
      resume: capability("resume"),
      submit: capability("submit"),
      expiresAtMs: EXPIRES_AT_MS,
    },
  };
}

function capability(
  operation: "result" | "resume" | "submit",
  overrides: Record<string, unknown> = {},
): string {
  return localRunCapabilityFixture(operation, EXPIRES_AT_MS, overrides);
}
