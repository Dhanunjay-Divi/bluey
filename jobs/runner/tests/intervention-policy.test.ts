import { describe, expect, it } from "vitest";
import type {
  ManagedCloudReleaseMemoAuthority,
} from "@bluey/jobs-automation/managed-cloud-execution";
import {
  decideRunnerInterventionResolution,
  parseRunnerInterventionResolution,
} from "../src/intervention-policy.js";

const base = {
  accountId: "account-1",
  applicationId: "application-1",
  applicationIdentityId: "identity-1",
  runId: "run-1",
  requestId: "run-1:resume:1",
  profileScope: "a".repeat(40),
};

describe("runner intervention reapproval policy", () => {
  it.each([
    { ...base, action: "answer", field: "salary", answer: "$150,000" },
    { ...base, action: "approve_submission", field: "legal_name" },
    { ...base, action: "approve_email_otp", answer: "123456" },
    { ...base, action: "answer" },
  ])("refuses browser resume for content-changing or unknown input %#", (resolution) => {
    expect(decideRunnerInterventionResolution(resolution)).toMatchObject({
      kind: "requires_reapproval",
    });
  });

  it.each([
    "approve_submission",
    "approve_email_otp",
    "resume_browser_takeover",
    "browser_takeover_complete",
  ])("allows the explicit content-neutral action %s", (action) => {
    expect(decideRunnerInterventionResolution({ ...base, action })).toEqual({ kind: "resume" });
  });

  it("rejects malformed or unexpected direct-runner payloads", () => {
    expect(() => parseRunnerInterventionResolution({ ...base, action: 42 })).toThrow("action is invalid");
    expect(() => parseRunnerInterventionResolution({
      ...base,
      action: "approve_submission",
      answers: { salary: "$150,000" },
    })).toThrow("unsupported data");
  });

  it("requires exact release memo A only for managed-runtime resume payloads", () => {
    const managedCloudRelease = managedCloudReleaseMemo();
    expect(parseRunnerInterventionResolution({
      ...base,
      action: "approve_submission",
      managedCloudRelease,
    }, true)).toEqual({
      ...base,
      action: "approve_submission",
      managedCloudRelease,
    });
    expect(() => parseRunnerInterventionResolution({
      ...base,
      action: "approve_submission",
    }, true)).toThrow("managed-cloud release authority");
    expect(() => parseRunnerInterventionResolution({
      ...base,
      action: "approve_submission",
      managedCloudRelease,
    })).toThrow("unsupported data");
    expect(() => parseRunnerInterventionResolution({
      ...base,
      action: "approve_submission",
      managedCloudRelease: { ...managedCloudRelease, extra: true },
    }, true)).toThrow("managed-cloud release authority");
  });
});

function managedCloudReleaseMemo(): ManagedCloudReleaseMemoAuthority {
  return {
    version: 1,
    bindingSha256: "1".repeat(64),
    scope: { environment: "staging", region: "us-east-1", channel: "canary" },
    headRevision: 7,
    transitionSha256: "2".repeat(64),
    activationSha256: "3".repeat(64),
    manifestSha256: "4".repeat(64),
    cohortSha256: "5".repeat(64),
    trustGeneration: 2,
    channelSequence: 9,
    releaseId: "managed-cloud-release-1234",
    releaseSequence: 4,
    taskQueueSha256: "6".repeat(64),
    failureConverterSha256: "7".repeat(64),
    readinessSha256: "8".repeat(64),
    activationExpiresAtMs: 1_900_000_000_000,
    resolvedAtMs: 1_800_000_000_000,
  };
}
