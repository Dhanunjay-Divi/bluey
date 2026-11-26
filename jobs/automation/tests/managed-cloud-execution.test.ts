import { describe, expect, it } from "vitest";
import {
  managedCloudGatewayMatchesRuntime,
  managedCloudCanonicalJsonBytes,
  managedCloudReleaseMemo,
  managedCloudReleaseMemoBytes,
  parseManagedCloudReleaseMemo,
  parseManagedCloudGatewayAuthority,
  recoveryAuthorizationSha256,
} from "../src/managed-cloud-execution.js";

const DIGESTS = Array.from({ length: 12 }, (_, index) =>
  (index + 1).toString(16).repeat(64));

function authority(recoveryAccepted = false) {
  const value = {
    version: 1,
    execution: {
      bindingSha256: DIGESTS[0],
      scope: { environment: "staging", region: "us-east-1", channel: "canary" },
      headRevision: 7,
      transitionSha256: DIGESTS[1],
      activationSha256: DIGESTS[2],
      manifestSha256: DIGESTS[3],
      cohortSha256: DIGESTS[4],
      trustGeneration: 2,
      channelSequence: 9,
      releaseId: "managed-cloud-release-1234",
      releaseSequence: 4,
      taskQueueSha256: DIGESTS[5],
      failureConverterSha256: DIGESTS[6],
      readinessSha256: DIGESTS[7],
      activationExpiresAtMs: 1_800_000_000_000,
      resolvedAtMs: 1_750_000_000_000,
    },
    authorization: {
      currentHeadRevision: recoveryAccepted ? 8 : 7,
      currentTransitionSha256: recoveryAccepted ? DIGESTS[8] : DIGESTS[1],
      currentActivationSha256: recoveryAccepted ? DIGESTS[9] : DIGESTS[2],
      currentManifestSha256: recoveryAccepted ? DIGESTS[10] : DIGESTS[3],
      currentActivationExpiresAtMs: recoveryAccepted
        ? 1_810_000_000_000
        : 1_800_000_000_000,
      currentTaskQueueSha256: recoveryAccepted ? DIGESTS[8] : DIGESTS[5],
      currentFailureConverterSha256: recoveryAccepted ? DIGESTS[9] : DIGESTS[6],
      currentReadinessSha256: recoveryAccepted ? DIGESTS[10] : DIGESTS[7],
      recoveryAccepted,
      recoveryAuthorizationSha256: DIGESTS[11],
      authorizedAtMs: 1_750_000_000_100,
    },
  };
  value.authorization.recoveryAuthorizationSha256 = recoveryAuthorizationSha256(value as never);
  return parseManagedCloudGatewayAuthority(value);
}

describe("managed-cloud execution authority", () => {
  it("binds current runtime identity without rewriting frozen execution", () => {
    const current = authority();
    expect(managedCloudGatewayMatchesRuntime(current, {
      scope: current.execution.scope,
      role: "workflow_gateway",
      headRevision: current.authorization.currentHeadRevision,
      transitionSha256: current.authorization.currentTransitionSha256,
      activationSha256: current.authorization.currentActivationSha256,
      manifestSha256: current.authorization.currentManifestSha256,
      taskQueueSha256: current.execution.taskQueueSha256,
      failureConverterSha256: current.execution.failureConverterSha256,
      activationExpiresAtMs: current.authorization.currentActivationExpiresAtMs,
    }, "workflow_gateway")).toBe(true);
    const memo = managedCloudReleaseMemo(current);
    expect(memo).toEqual({ ...current.execution, version: 1 });
    expect(new TextDecoder().decode(managedCloudReleaseMemoBytes(memo)))
      .toBe(JSON.stringify(memo));
    expect(parseManagedCloudReleaseMemo(memo)).toEqual(memo);
  });

  it("permits only a digest-bound current recovery authority", () => {
    const recovery = authority(true);
    const runtime = {
      scope: recovery.execution.scope,
      role: "workflow_gateway",
      headRevision: recovery.authorization.currentHeadRevision,
      transitionSha256: recovery.authorization.currentTransitionSha256,
      activationSha256: recovery.authorization.currentActivationSha256,
      manifestSha256: recovery.authorization.currentManifestSha256,
      taskQueueSha256: recovery.authorization.currentTaskQueueSha256,
      failureConverterSha256: recovery.authorization.currentFailureConverterSha256,
      activationExpiresAtMs: recovery.authorization.currentActivationExpiresAtMs,
    };
    expect(managedCloudGatewayMatchesRuntime(recovery, runtime, "workflow_gateway"))
      .toBe(true);
    expect(managedCloudGatewayMatchesRuntime({
      ...recovery,
      authorization: {
        ...recovery.authorization,
        recoveryAuthorizationSha256: DIGESTS[0],
      },
    }, runtime, "workflow_gateway")).toBe(false);
  });

  it("represents rollback recovery when the current head selects the same release bytes", () => {
    const exact = authority();
    const rollback = {
      ...exact,
      authorization: {
        ...exact.authorization,
        currentHeadRevision: exact.authorization.currentHeadRevision + 2,
        currentTransitionSha256: DIGESTS[8],
        recoveryAccepted: true,
        recoveryAuthorizationSha256: DIGESTS[11],
      },
    };
    rollback.authorization.recoveryAuthorizationSha256 = recoveryAuthorizationSha256(
      rollback,
    );

    const parsed = parseManagedCloudGatewayAuthority(rollback);
    expect(parsed.authorization.currentActivationSha256)
      .toBe(parsed.execution.activationSha256);
    expect(parsed.authorization.currentManifestSha256)
      .toBe(parsed.execution.manifestSha256);
    expect(parsed.authorization.currentTransitionSha256)
      .not.toBe(parsed.execution.transitionSha256);
  });

  it("rejects unknown fields, unsafe numbers, and incomplete scopes", () => {
    const exact = authority();
    expect(() => parseManagedCloudGatewayAuthority({ ...exact, extra: true })).toThrow();
    expect(() => parseManagedCloudGatewayAuthority({
      ...exact,
      execution: { ...exact.execution, headRevision: Number.MAX_SAFE_INTEGER + 1 },
    })).toThrow();
    expect(() => parseManagedCloudGatewayAuthority({
      ...exact,
      execution: { ...exact.execution, scope: { ...exact.execution.scope, region: "US" } },
    })).toThrow();
    expect(() => managedCloudCanonicalJsonBytes({ value: -0 })).toThrow();
    expect(new TextDecoder().decode(managedCloudCanonicalJsonBytes({
      "\u{10000}": 2,
      "\ue000": 1,
    }))).toBe("{\"\":1,\"𐀀\":2}\n");
  });
});
