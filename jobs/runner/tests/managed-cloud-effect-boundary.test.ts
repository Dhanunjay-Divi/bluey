import { describe, expect, it } from "vitest";
import type {
  ManagedCloudReleaseMemoAuthority,
} from "@bluey/jobs-automation/managed-cloud-execution";
import { parseCloudRunRequestForRuntime } from "../src/server.js";

const LEGACY_RUN = {
  accountId: "account-123",
  applicationId: "application-123",
  applicationIdentityId: "identity-123",
  browserProfileId: "profile-123",
  browserSessionId: "cloud-application-123",
  job: {},
  packet: {},
  requestId: "wfreq-v2-12345678-1234-5678-9234-123456789abc",
  runId: "run-123",
  url: "https://boards.example/jobs/123",
};

describe("managed-cloud runner effect boundary", () => {
  it("keeps the local run body exact and rejects managed authority locally", () => {
    expect(parseCloudRunRequestForRuntime(LEGACY_RUN, false)).toEqual(LEGACY_RUN);
    expect(() => parseCloudRunRequestForRuntime({
      ...LEGACY_RUN,
      managedCloudRelease: managedCloudReleaseMemo(),
    }, false)).toThrow("Invalid run request");
  });

  it("requires one exact closed release memo A for managed run bodies", () => {
    const managedCloudRelease = managedCloudReleaseMemo();
    expect(parseCloudRunRequestForRuntime({
      ...LEGACY_RUN,
      managedCloudRelease,
    }, true)).toEqual({
      ...LEGACY_RUN,
      managedCloudRelease,
    });
    expect(() => parseCloudRunRequestForRuntime(LEGACY_RUN, true))
      .toThrow("Invalid run request");
    expect(() => parseCloudRunRequestForRuntime({
      ...LEGACY_RUN,
      managedCloudRelease: { ...managedCloudRelease, extra: true },
    }, true)).toThrow("Invalid managed-cloud run authority");
    expect(() => parseCloudRunRequestForRuntime({
      ...LEGACY_RUN,
      managedCloudRelease,
      unexpected: true,
    }, true)).toThrow("Invalid run request");
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
