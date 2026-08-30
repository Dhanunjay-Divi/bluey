import { createHash, createHmac } from "node:crypto";
import { afterEach, describe, expect, it, vi } from "vitest";
import { finalSubmitSurfaceSha256 } from "@bluey/jobs-automation";
import {
  managedCloudReleaseMemoBytes,
  parseManagedCloudGatewayAuthority,
  recoveryAuthorizationSha256,
  type ManagedCloudReleaseMemoAuthority,
} from "@bluey/jobs-automation/managed-cloud-execution";
import type {
  ManagedCloudRuntimeInstance,
} from "@bluey/jobs-automation/managed-cloud-runtime-client";
import {
  createExecutionLeaseClientFromEnv,
  ExecutionLeaseClient,
  ExecutionLeaseError,
  runnerOwnerId,
} from "../src/execution-lease.js";
import {
  authorizeCloudFinalSubmitBeforeCheckpoint,
  hasCloudIrreversibleCheckpointAuthority,
} from "../src/certified-final-submit.js";

const WORKER_SIGNING_KEY = "worker-signing-key-secret-0123456789abcdef";
const VOLUME_ID = Buffer.alloc(32, 1).toString("base64url");
const PROCESS_INSTANCE_ID = Buffer.alloc(32, 2).toString("base64url");
const PURGE_SUBJECT = Buffer.alloc(32, 3).toString("base64url");
const VOLUME_KEY_FINGERPRINT = "4".repeat(64);
const RUNTIME_GRANT_ID = "runner-process-runtime-grant-test";
const RUNTIME_SHA256 = "7".repeat(64);
const MANAGED_WORKFLOW_REQUEST_ID =
  "wfreq-v2-12345678-1234-5678-9234-123456789abc";
const MANAGED_RESUME_REQUEST_ID =
  "wfreq-v2-22345678-1234-5678-9234-123456789abc";
const MANAGED_SECOND_RESUME_REQUEST_ID =
  "wfreq-v2-32345678-1234-5678-9234-123456789abc";
const VOLUME_PROOF = {
  version: 1 as const,
  audience: "bluey-jobs-runner-volume-authority" as const,
  operation: "execution_lease_claim" as const,
  requestId: "proof-request-123",
  volumeId: VOLUME_ID,
  enrollmentEpoch: 1,
  processInstanceId: PROCESS_INSTANCE_ID,
  issuedAtMs: 1_000,
  payloadSha256: "5".repeat(64),
  signature: Buffer.alloc(64, 6).toString("base64url"),
};

const CLAIM = {
  accountId: "account-123",
  applicationId: "application-123",
  runId: "run-123",
  browserProfileId: "profile-123",
};

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("execution lease client", () => {
  it("signs every lease operation and keeps lease data in internal requests", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        calls.push({ url: String(input), init });
        if (String(input).endsWith("/claim")) return grantResponse();
        if (String(input).endsWith("/irreversible"))
          return recordResponse("click_started");
        return new Response(null, { status: 204 });
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fetch);

    const lease = await client.claim(CLAIM);
    expect(lease.fence).toBe(7);
    expect(lease.expiresAtMs).toBeGreaterThan(Date.now());
    expect(lease.ownerId).toBe("runner-test-1");
    expect(lease.purgeSubject).toBe(PURGE_SUBJECT);
    expect(lease.checkpointMetadata()).toEqual({
      leaseToken: "lease-secret-value",
      fence: 7,
      expiresAtMs: lease.expiresAtMs,
      ownerId: "runner-test-1",
    });
    await lease.beforeFinalSubmit(finalSubmitProof());
    await lease.afterFinalSubmit("activated");
    await lease.finish("submitted");

    expect(calls).toHaveLength(3);
    expect(calls[0]?.url).toBe(
      "https://jobs-api.example/api/jobs/internal/execution-leases/claim",
    );
    expect(calls[0]?.init).toMatchObject({ method: "POST", redirect: "error" });
    expect(
      new Headers(calls[0]?.init?.headers).get("authorization"),
    ).toBeNull();
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      account_id: "account-123",
      application_id: "application-123",
      run_id: "run-123",
      browser_profile_id: "profile-123",
      owner_id: "runner-test-1",
      volume_id: VOLUME_ID,
      enrollment_epoch: 1,
      process_instance_id: PROCESS_INSTANCE_ID,
      runtime_grant_id: RUNTIME_GRANT_ID,
      runtime_sha256: RUNTIME_SHA256,
      volume_proof: VOLUME_PROOF,
    });
    expect(JSON.parse(String(calls[1]?.init?.body))).toMatchObject({
      lease_token: "lease-secret-value",
      fence: 7,
      action: "submit",
      final_submit_proof: finalSubmitProof(),
    });
    expect(JSON.parse(String(calls[2]?.init?.body))).toMatchObject({
      lease_token: "lease-secret-value",
      fence: 7,
      outcome: "submitted",
    });
    for (const call of calls) expectSignedWorkerRequest(call, "runner-test-1");
    expect(
      new Set(
        calls.map((call) =>
          new Headers(call.init?.headers).get("x-bluey-jobs-worker-nonce"),
        ),
      ).size,
    ).toBe(calls.length);
    expect(JSON.stringify(lease)).not.toContain("lease-secret-value");
  });

  it("binds managed claim, pre-effect authorization, and irreversible response to A and runtime B", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const release = managedCloudRelease();
    const managedCloud = managedCloudAuthority(release);
    const proofInput = vi.fn(() => VOLUME_PROOF);
    const fetch = vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith("/claim")) {
        return grantResponse({
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID),
        });
      }
      if (url.endsWith("/authorize-managed-effect")) {
        return recordResponse("prepared", {
          managedCloud,
          ...managedResponseCorrelation(MANAGED_RESUME_REQUEST_ID),
        });
      }
      if (url.endsWith("/irreversible")) {
        return recordResponse("click_started", {
          managedCloud,
          ...managedResponseCorrelation(MANAGED_RESUME_REQUEST_ID),
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const client = createClient(fetch, {
      managedCloudRuntime: managedCloudRuntime(),
      runnerVolume: {
        ...runnerVolume(),
        createExecutionLeaseClaimProof: proofInput,
      },
    });

    const lease = await client.claim({
      ...CLAIM,
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
    });
    const releaseSha256 = createHash("sha256")
      .update(managedCloudReleaseMemoBytes(release))
      .digest("hex");
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      account_id: CLAIM.accountId,
      application_id: CLAIM.applicationId,
      run_id: CLAIM.runId,
      browser_profile_id: CLAIM.browserProfileId,
      owner_id: "runner-test-1",
      workflow_request_id: MANAGED_WORKFLOW_REQUEST_ID,
      managed_cloud_release: release,
      managed_cloud_release_sha256: releaseSha256,
      managed_cloud_runtime_instance_id: managedCloudRuntime().runtimeInstanceId,
      managed_cloud_runtime_instance_epoch: managedCloudRuntime().instanceEpoch,
      volume_id: VOLUME_ID,
      enrollment_epoch: 1,
      process_instance_id: PROCESS_INSTANCE_ID,
      runtime_grant_id: RUNTIME_GRANT_ID,
      runtime_sha256: RUNTIME_SHA256,
      volume_proof: VOLUME_PROOF,
    });
    expect(proofInput).toHaveBeenCalledWith({
      accountId: CLAIM.accountId,
      applicationId: CLAIM.applicationId,
      runId: CLAIM.runId,
      browserProfileId: CLAIM.browserProfileId,
      ownerId: "runner-test-1",
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
      managedCloudReleaseSha256: releaseSha256,
      managedCloudRuntimeInstanceId: managedCloudRuntime().runtimeInstanceId,
      managedCloudRuntimeInstanceEpoch: managedCloudRuntime().instanceEpoch,
    });

    await lease.authorizeManagedEffect(MANAGED_RESUME_REQUEST_ID, release);
    expect(calls[1]?.url).toBe(
      "https://jobs-api.example/api/jobs/internal/execution-leases/run-123/authorize-managed-effect",
    );
    expect(JSON.parse(String(calls[1]?.init?.body))).toEqual({
      account_id: CLAIM.accountId,
      application_id: CLAIM.applicationId,
      lease_token: "lease-secret-value",
      fence: 7,
      workflow_request_id: MANAGED_RESUME_REQUEST_ID,
      managed_cloud_release: release,
      managed_cloud_release_sha256: releaseSha256,
      managed_cloud_runtime_instance_id: managedCloudRuntime().runtimeInstanceId,
      managed_cloud_runtime_instance_epoch: managedCloudRuntime().instanceEpoch,
    });
    await lease.beforeFinalSubmit(finalSubmitProof());
    await lease.finish("failed");
    expect(JSON.parse(String(calls[2]?.init?.body))).toEqual({
      account_id: CLAIM.accountId,
      application_id: CLAIM.applicationId,
      lease_token: "lease-secret-value",
      fence: 7,
      action: "submit",
      final_submit_proof: finalSubmitProof(),
      workflow_request_id: MANAGED_RESUME_REQUEST_ID,
      managed_cloud_release: release,
      managed_cloud_release_sha256: releaseSha256,
      managed_cloud_runtime_instance_id: managedCloudRuntime().runtimeInstanceId,
      managed_cloud_runtime_instance_epoch: managedCloudRuntime().instanceEpoch,
    });
    expect(JSON.parse(String(calls[3]?.init?.body))).toEqual({
      account_id: CLAIM.accountId,
      application_id: CLAIM.applicationId,
      lease_token: "lease-secret-value",
      fence: 7,
      outcome: "failed",
    });
  });

  it("requires all managed claim authority before network I/O", async () => {
    const fetch = vi.fn() as typeof globalThis.fetch;
    const managedClient = createClient(fetch, {
      managedCloudRuntime: managedCloudRuntime(),
    });
    await expect(managedClient.claim(CLAIM)).rejects.toMatchObject({
      operation: "claim",
      code: "invalid_state",
    });

    const localClient = createClient(fetch);
    await expect(localClient.claim({
      ...CLAIM,
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: managedCloudRelease(),
    })).rejects.toMatchObject({ operation: "claim", code: "invalid_state" });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("poisons irreversible I/O after a changed-A authorization attempt", async () => {
    const release = managedCloudRelease();
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) {
        return grantResponse({
          managedCloud: managedCloudAuthority(release),
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID),
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch, {
      managedCloudRuntime: managedCloudRuntime(),
    }).claim({
      ...CLAIM,
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
    });

    await expect(lease.authorizeManagedEffect(MANAGED_RESUME_REQUEST_ID, {
      ...release,
      bindingSha256: "9".repeat(64),
    })).rejects.toMatchObject({
      operation: "authorize_managed_effect",
      code: "invalid_state",
    });
    expect(fetch).toHaveBeenCalledTimes(1);
    await expect(lease.beforeFinalSubmit(finalSubmitProof())).rejects.toMatchObject({
      operation: "irreversible",
      code: "invalid_state",
    });
    await lease.finish("side_effect_unknown");
  });

  it("poisons irreversible I/O when managed authorization omits fresh B", async () => {
    const release = managedCloudRelease();
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) {
        return grantResponse({
          managedCloud: managedCloudAuthority(release),
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID),
        });
      }
      if (url.endsWith("/authorize-managed-effect")) {
        return recordResponse("prepared");
      }
      if (url.endsWith("/irreversible")) {
        return recordResponse("click_started");
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const client = createClient(fetch, {
      managedCloudRuntime: managedCloudRuntime(),
    });
    const lease = await client.claim({
      ...CLAIM,
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
    });

    await expect(
      lease.authorizeManagedEffect(MANAGED_RESUME_REQUEST_ID, release),
    ).rejects.toMatchObject({
      operation: "authorize_managed_effect",
      code: "invalid_response",
    });
    await expect(lease.beforeFinalSubmit(finalSubmitProof())).rejects.toMatchObject({
      operation: "irreversible",
      code: "invalid_state",
    });
    expect(fetch.mock.calls.some(([input]) => String(input).endsWith("/irreversible"))).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it.each([
    ["lost", "request_failed"],
    ["invalid", "invalid_response"],
  ])("blocks irreversible I/O when a resume authorization response is %s", async (
    failureKind,
    expectedCode,
  ) => {
    const release = managedCloudRelease();
    const managedCloud = managedCloudAuthority(release);
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetch = vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith("/claim")) {
        return grantResponse({
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID),
        });
      }
      if (url.endsWith("/authorize-managed-effect")) {
        if (failureKind === "lost") throw new Error("authorization response lost");
        return recordResponse("prepared", {
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID),
        });
      }
      if (url.endsWith("/irreversible")) {
        return recordResponse("click_started", {
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID),
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch, {
      managedCloudRuntime: managedCloudRuntime(),
    }).claim({
      ...CLAIM,
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
    });

    await expect(
      lease.authorizeManagedEffect(MANAGED_RESUME_REQUEST_ID, release),
    ).rejects.toMatchObject({
      operation: "authorize_managed_effect",
      code: expectedCode,
    });
    await expect(lease.beforeFinalSubmit(finalSubmitProof())).rejects.toMatchObject({
      operation: "irreversible",
      code: "invalid_state",
    });
    expect(calls.some(({ url }) => url.endsWith("/irreversible"))).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it.each([
    ["original claim", MANAGED_WORKFLOW_REQUEST_ID],
    ["prior resume", MANAGED_RESUME_REQUEST_ID],
  ])("rejects a stale %s echo after a later managed resume", async (_label, staleId) => {
    const release = managedCloudRelease();
    const managedCloud = managedCloudAuthority(release);
    let authorizationCount = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) {
        return grantResponse({
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID),
        });
      }
      if (url.endsWith("/authorize-managed-effect")) {
        authorizationCount += 1;
        const requestId = authorizationCount === 1
          ? MANAGED_RESUME_REQUEST_ID
          : MANAGED_SECOND_RESUME_REQUEST_ID;
        return recordResponse("prepared", {
          managedCloud,
          ...managedResponseCorrelation(requestId),
        });
      }
      if (url.endsWith("/irreversible")) {
        return recordResponse("click_started", {
          managedCloud,
          ...managedResponseCorrelation(staleId),
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch, {
      managedCloudRuntime: managedCloudRuntime(),
    }).claim({
      ...CLAIM,
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
    });
    await lease.authorizeManagedEffect(MANAGED_RESUME_REQUEST_ID, release);
    await lease.authorizeManagedEffect(MANAGED_SECOND_RESUME_REQUEST_ID, release);

    await expect(lease.beforeFinalSubmit(finalSubmitProof())).rejects.toMatchObject({
      operation: "irreversible",
      code: "invalid_response",
    });
    await lease.finish("side_effect_unknown");
  });

  it("rejects swapped managed runtime instance, epoch, and worker response identities", async () => {
    const release = managedCloudRelease();
    const managedCloud = managedCloudAuthority(release);
    const runtime = managedCloudRuntime();
    const claimInput = {
      ...CLAIM,
      workflowRequestId: MANAGED_WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
    };
    const wrongWorkflowClaimClient = createClient(vi.fn(async () => grantResponse({
      managedCloud,
      ...managedResponseCorrelation(MANAGED_RESUME_REQUEST_ID, runtime),
    })) as typeof globalThis.fetch, { managedCloudRuntime: runtime });
    await expect(wrongWorkflowClaimClient.claim(claimInput)).rejects.toMatchObject({
      operation: "claim",
      code: "invalid_response",
    });
    const wrongClaimClient = createClient(vi.fn(async () => grantResponse({
      managedCloud,
      ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID, {
        ...runtime,
        runtimeInstanceId: "managed-runner-instance-swapped",
      }),
    })) as typeof globalThis.fetch, { managedCloudRuntime: runtime });
    await expect(wrongClaimClient.claim(claimInput)).rejects.toMatchObject({
      operation: "claim",
      code: "invalid_response",
    });

    const epochFetch = vi.fn(async (input: string | URL | Request) => {
      if (String(input).endsWith("/claim")) {
        return grantResponse({
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID, runtime),
        });
      }
      if (String(input).endsWith("/authorize-managed-effect")) {
        return recordResponse("prepared", {
          managedCloud,
          ...managedResponseCorrelation(MANAGED_RESUME_REQUEST_ID, {
            ...runtime,
            instanceEpoch: runtime.instanceEpoch + 1,
          }),
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const epochLease = await createClient(epochFetch, {
      managedCloudRuntime: runtime,
    }).claim(claimInput);
    await expect(
      epochLease.authorizeManagedEffect(MANAGED_RESUME_REQUEST_ID, release),
    ).rejects.toMatchObject({
      operation: "authorize_managed_effect",
      code: "invalid_response",
    });
    await epochLease.finish("side_effect_unknown");

    const workerFetch = vi.fn(async (input: string | URL | Request) => {
      if (String(input).endsWith("/claim")) {
        return grantResponse({
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID, runtime),
        });
      }
      if (String(input).endsWith("/irreversible")) {
        return recordResponse("click_started", {
          managedCloud,
          ...managedResponseCorrelation(MANAGED_WORKFLOW_REQUEST_ID, {
            ...runtime,
            workerId: "managed-runner-worker-swapped",
          }),
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const workerLease = await createClient(workerFetch, {
      managedCloudRuntime: runtime,
    }).claim(claimInput);
    await expect(workerLease.beforeFinalSubmit(finalSubmitProof())).rejects.toMatchObject({
      operation: "irreversible",
      code: "invalid_response",
    });
    await workerLease.finish("side_effect_unknown");
  });

  it.each([
    ["missing proof", undefined],
    [
      "wrong provider control",
      {
        ...finalSubmitProof(),
        control: "lever_application_submit",
      },
    ],
    [
      "unsorted documents",
      {
        ...finalSubmitProof(),
        documents: [
          finalSubmitProof().documents[0],
          { kind: "cover_letter", sha256: "b".repeat(64) },
        ],
      },
    ],
  ])(
    "rejects %s before consuming the irreversible lease fence",
    async (_label, proof) => {
      let irreversibleCalls = 0;
      const fetch = vi.fn(async (input: string | URL | Request) => {
        if (String(input).endsWith("/claim")) return grantResponse();
        irreversibleCalls += 1;
        return recordResponse("click_started");
      }) as typeof globalThis.fetch;
      const lease = await createClient(fetch).claim(CLAIM);

      await expect(
        lease.beforeFinalSubmit(proof as never),
      ).rejects.toMatchObject({
        code: "invalid_state",
      });

      expect(irreversibleCalls).toBe(0);
      expect(lease.finalSubmitAttempted).toBe(false);
      await lease.finish("failed");
    },
  );

  it("reconciles an encrypted restart checkpoint with a signed bounded request", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        calls.push({ url: String(input), init });
        return recordResponse("side_effect_unknown");
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fetch);

    await client.reconcileCheckpoint({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      ownerId: "runner-test-1",
      fence: 7,
      leaseToken: "lease-secret-value",
      checkpointVersion: 2,
      checkpointPhase: "final_submit_started",
    });

    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe(
      "https://jobs-api.example/api/jobs/internal/execution-leases/run-123/reconcile-checkpoint",
    );
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      account_id: "account-123",
      application_id: "application-123",
      owner_id: "runner-test-1",
      fence: 7,
      lease_token: "lease-secret-value",
      checkpoint_version: 2,
      checkpoint_phase: "final_submit_started",
    });
    expectSignedWorkerRequest(calls[0]!, "runner-test-1");
  });

  it("replays the exact submitted finish from durable checkpoint authority", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        calls.push({ url: String(input), init });
        return new Response(null, { status: 204 });
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fetch);

    await client.replaySubmittedFinish({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      leaseToken: "lease-secret-value",
      fence: 7,
    });

    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe(
      "https://jobs-api.example/api/jobs/internal/execution-leases/run-123/finish",
    );
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      account_id: "account-123",
      application_id: "application-123",
      lease_token: "lease-secret-value",
      fence: 7,
      outcome: "submitted",
    });
    expectSignedWorkerRequest(calls[0]!, "runner-test-1");
  });

  it("omits the lease token only for a legacy v1 restart checkpoint", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        calls.push({ url: String(input), init });
        return recordResponse("released");
      },
    ) as typeof globalThis.fetch;

    await createClient(fetch).reconcileCheckpoint({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      ownerId: "runner-test-1",
      fence: 7,
      checkpointVersion: 1,
      checkpointPhase: "prepared",
    });

    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      account_id: "account-123",
      application_id: "application-123",
      owner_id: "runner-test-1",
      fence: 7,
      checkpoint_version: 1,
      checkpoint_phase: "prepared",
    });
  });

  it("makes the irreversible fence single-shot when its success response is lost", async () => {
    let irreversibleCalls = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/irreversible")) {
        irreversibleCalls += 1;
        throw new Error(
          `network failure containing ${WORKER_SIGNING_KEY} and lease-secret-value`,
        );
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch).claim(CLAIM);

    const first = await lease
      .beforeFinalSubmit(finalSubmitProof())
      .catch((error: unknown) => error);
    const second = await lease
      .beforeFinalSubmit(finalSubmitProof())
      .catch((error: unknown) => error);

    expect(first).toBeInstanceOf(ExecutionLeaseError);
    expect(String(first)).not.toContain(WORKER_SIGNING_KEY);
    expect(String(first)).not.toContain("lease-secret-value");
    expect(second).toMatchObject({ code: "invalid_state" });
    expect(irreversibleCalls).toBe(1);
    expect(lease.finalSubmitAttempted).toBe(true);
    expect(lease.finalSubmitAuthorized).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it("does not authorize a click from an incomplete successful fence response", async () => {
    const fetch = vi.fn(async (input: string | URL | Request) => {
      if (String(input).endsWith("/claim")) return grantResponse();
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch).claim(CLAIM);

    await expect(
      lease.beforeFinalSubmit(finalSubmitProof()),
    ).rejects.toMatchObject({
      code: "invalid_response",
    });
    expect(lease.finalSubmitAttempted).toBe(true);
    expect(lease.finalSubmitAuthorized).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it.each([
    ["Phase B denial", "denied", "request_failed", 403],
    ["network error", "network", "request_failed", undefined],
    ["malformed response", "malformed", "invalid_response", undefined],
    ["expiry-like denial", "expired", "request_failed", 409],
    ["timeout", "timeout", "timed_out", undefined],
  ] as const)(
    "retains the irreversible-attempt checkpoint without submit activation after %s",
    async (_label, mode, expectedCode, expectedStatus) => {
      if (mode === "timeout") vi.useFakeTimers();
      const order: string[] = [];
      const fetch = vi.fn(
        async (input: string | URL | Request, init?: RequestInit) => {
          const url = String(input);
          if (url.endsWith("/claim")) return grantResponse();
          if (url.endsWith("/irreversible")) {
            order.push("phase_b");
            if (mode === "network") throw new Error("network unavailable");
            if (mode === "malformed") {
              return new Response("{", {
                status: 200,
                headers: { "Content-Type": "application/json" },
              });
            }
            if (mode === "timeout") {
              return new Promise<Response>((_resolve, reject) => {
                init?.signal?.addEventListener(
                  "abort",
                  () => reject(new Error("authorization timed out")),
                  { once: true },
                );
              });
            }
            return new Response(JSON.stringify({ error: mode }), {
              status: mode === "expired" ? 409 : 403,
              headers: { "Content-Type": "application/json" },
            });
          }
          return new Response(null, { status: 204 });
        },
      ) as typeof globalThis.fetch;
      const lease = await createClient(fetch, { requestTimeoutMs: 100 }).claim(
        CLAIM,
      );
      const writeCheckpoint = vi.fn(async () => {
        expect(lease.finalSubmitAttempted).toBe(false);
        order.push("checkpoint");
      });
      const activate = vi.fn(() => {
        order.push("activation");
      });
      const failure = authorizeCloudFinalSubmitBeforeCheckpoint(
        lease,
        finalSubmitProof(),
        writeCheckpoint,
      )
        .then(() => activate())
        .catch((error: unknown) => error);

      if (mode === "timeout") await vi.advanceTimersByTimeAsync(100);
      const error = await failure;

      expect(error).toBeInstanceOf(ExecutionLeaseError);
      expect(error).toMatchObject({
        operation: "irreversible",
        code: expectedCode,
        ...(expectedStatus === undefined ? {} : { status: expectedStatus }),
      });
      expect(order).toEqual(["checkpoint", "phase_b"]);
      expect(writeCheckpoint).toHaveBeenCalledOnce();
      expect(activate).not.toHaveBeenCalled();
      expect(lease.finalSubmitAttempted).toBe(true);
      expect(lease.finalSubmitAuthorized).toBe(false);
      expect(hasCloudIrreversibleCheckpointAuthority(lease)).toBe(false);
      await lease.stopHeartbeat();
    },
  );

  it("writes the irreversible-attempt checkpoint before successful Phase B authorization", async () => {
    const order: string[] = [];
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/irreversible")) {
        order.push("phase_b");
        return recordResponse("click_started");
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch).claim(CLAIM);
    const writeCheckpoint = vi.fn(async () => {
      expect(lease.finalSubmitAttempted).toBe(false);
      expect(lease.finalSubmitAuthorized).toBe(false);
      order.push("checkpoint");
    });
    const activate = vi.fn(() => {
      order.push("activation");
    });

    await authorizeCloudFinalSubmitBeforeCheckpoint(
      lease,
      finalSubmitProof(),
      writeCheckpoint,
    ).then(() => activate());

    expect(order).toEqual(["checkpoint", "phase_b", "activation"]);
    expect(writeCheckpoint).toHaveBeenCalledOnce();
    expect(activate).toHaveBeenCalledOnce();
    expect(hasCloudIrreversibleCheckpointAuthority(lease)).toBe(true);
    await lease.stopHeartbeat();
  });

  it("does not issue irreversible Phase B I/O when the attempt checkpoint cannot be persisted", async () => {
    let irreversibleCalls = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/irreversible")) {
        irreversibleCalls += 1;
        return recordResponse("click_started");
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch).claim(CLAIM);

    await expect(
      authorizeCloudFinalSubmitBeforeCheckpoint(
        lease,
        finalSubmitProof(),
        async () => {
          throw new Error("checkpoint write failed");
        },
      ),
    ).rejects.toThrow("checkpoint write failed");

    expect(irreversibleCalls).toBe(0);
    expect(lease.finalSubmitAttempted).toBe(false);
    expect(lease.finalSubmitAuthorized).toBe(false);
    await lease.stopHeartbeat();
  });

  it("retains exact certified Phase B authority for the immutable receipt", async () => {
    const authority = certifiedReceiptAuthority();
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/irreversible")) {
        return recordResponse("click_started", {
          atsCertifiedReceiptAuthority: authority,
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch).claim(CLAIM);

    await lease.beforeFinalSubmit(certifiedFinalSubmitProof());

    expect(lease.finalSubmitAuthorized).toBe(true);
    expect(lease.atsCertifiedReceiptAuthority).toEqual(authority);
    const copy = lease.atsCertifiedReceiptAuthority;
    if (copy) copy.bindingFence = 99;
    expect(lease.atsCertifiedReceiptAuthority?.bindingFence).toBe(1);
    await lease.stopHeartbeat();
  });

  it.each([
    ["missing", undefined],
    ["wrong run", { ...certifiedReceiptAuthority(), runId: "run-other" }],
    [
      "wrong observed surface",
      {
        ...certifiedReceiptAuthority(),
        observedSurfaceSha256: "b".repeat(64),
      },
    ],
    [
      "invalid signed layout observation",
      {
        ...certifiedReceiptAuthority(),
        layoutObservationSha256: "B".repeat(64),
      },
    ],
    ["unknown field", { ...certifiedReceiptAuthority(), extra: true }],
  ])(
    "rejects %s certified Phase B authority before checkpoint",
    async (_label, authority) => {
      const fetch = vi.fn(async (input: string | URL | Request) => {
        const url = String(input);
        if (url.endsWith("/claim")) return grantResponse();
        if (url.endsWith("/irreversible")) {
          return recordResponse(
            "click_started",
            authority === undefined
              ? {}
              : { atsCertifiedReceiptAuthority: authority },
          );
        }
        return new Response(null, { status: 204 });
      }) as typeof globalThis.fetch;
      const lease = await createClient(fetch).claim(CLAIM);
      const checkpoint = vi.fn(async () => undefined);

      await expect(
        authorizeCloudFinalSubmitBeforeCheckpoint(
          lease,
          certifiedFinalSubmitProof(),
          checkpoint,
        ),
      ).rejects.toMatchObject({ code: "invalid_response" });

      expect(checkpoint).toHaveBeenCalledOnce();
      expect(lease.finalSubmitAuthorized).toBe(false);
      expect(lease.atsCertifiedReceiptAuthority).toBeUndefined();
      await lease.stopHeartbeat();
    },
  );

  it("times out stalled responses and rejects oversized or redirected responses", async () => {
    vi.useFakeTimers();
    const stalledFetch = vi.fn(
      (_input: string | URL | Request, init?: RequestInit) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener(
            "abort",
            () => reject(new Error("aborted")),
            { once: true },
          );
        }),
    ) as typeof globalThis.fetch;
    const stalledClaim = createClient(stalledFetch, {
      requestTimeoutMs: 100,
    }).claim(CLAIM);
    const stalledExpectation = expect(stalledClaim).rejects.toMatchObject({
      code: "timed_out",
    });
    await vi.advanceTimersByTimeAsync(100);
    await stalledExpectation;
    vi.useRealTimers();

    const oversized = createClient(
      vi.fn(
        async () =>
          new Response("x".repeat(300), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }),
      ) as typeof globalThis.fetch,
      { maxResponseBytes: 256 },
    );
    await expect(oversized.claim(CLAIM)).rejects.toMatchObject({
      code: "response_too_large",
    });

    const redirected = createClient(
      vi.fn(
        async () =>
          new Response("", {
            status: 302,
            headers: { Location: "https://elsewhere.example/lease" },
          }),
      ) as typeof globalThis.fetch,
    );
    await expect(redirected.claim(CLAIM)).rejects.toMatchObject({
      code: "redirect_blocked",
    });
  });

  it("reports duplicate claims without exposing the server response", async () => {
    const fetch = vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            error: `owner ${WORKER_SIGNING_KEY} lease-secret-value https://private.example`,
          }),
          {
            status: 409,
            headers: { "Content-Type": "application/json" },
          },
        ),
    ) as typeof globalThis.fetch;

    const error = await createClient(fetch)
      .claim(CLAIM)
      .catch((caught: unknown) => caught);

    expect(error).toMatchObject({ code: "lease_unavailable", status: 409 });
    expect(String(error)).not.toContain(WORKER_SIGNING_KEY);
    expect(String(error)).not.toContain("lease-secret-value");
    expect(String(error)).not.toContain("private.example");
  });

  it.each([
    ["volume id", { volume_id: Buffer.alloc(32, 9).toString("base64url") }],
    ["enrollment epoch", { enrollment_epoch: 2 }],
    [
      "process instance",
      { process_instance_id: Buffer.alloc(32, 8).toString("base64url") },
    ],
    ["key fingerprint", { volume_key_fingerprint: "9".repeat(64) }],
    ["runtime grant", { runtime_grant_id: "other-runtime-grant" }],
    ["runtime digest", { runtime_sha256: "9".repeat(64) }],
    ["purge subject", { purge_subject: "not-canonical" }],
  ])(
    "rejects a claim grant with a mismatched %s binding",
    async (_label, override) => {
      const fetch = vi.fn(async () =>
        grantResponse(override),
      ) as typeof globalThis.fetch;

      await expect(createClient(fetch).claim(CLAIM)).rejects.toMatchObject({
        code: "invalid_response",
      });
    },
  );

  it("heartbeats while active, records failures, and cleans up on finish", async () => {
    vi.useFakeTimers();
    let heartbeatCalls = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/heartbeat")) {
        heartbeatCalls += 1;
        if (heartbeatCalls === 1)
          throw new Error("transient heartbeat failure");
        return recordResponse("prepared");
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch, { heartbeatIntervalMs: 100 }).claim(
      CLAIM,
    );

    await vi.advanceTimersByTimeAsync(100);
    expect(heartbeatCalls).toBe(1);
    expect(lease.heartbeatFailureCode).toBe("request_failed");
    await vi.advanceTimersByTimeAsync(100);
    expect(heartbeatCalls).toBe(2);
    expect(lease.heartbeatFailureCode).toBeUndefined();

    await lease.finish("failed");
    expect(lease.heartbeatActive).toBe(false);
    await vi.advanceTimersByTimeAsync(500);
    expect(heartbeatCalls).toBe(2);
  });

  it("revalidates the live fence before submit after a heartbeat failure", async () => {
    vi.useFakeTimers();
    let irreversibleCalls = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/heartbeat")) throw new Error("heartbeat unavailable");
      if (url.endsWith("/irreversible")) {
        irreversibleCalls += 1;
        return new Response(JSON.stringify({ error: "expired" }), {
          status: 409,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch, { heartbeatIntervalMs: 100 }).claim(
      CLAIM,
    );
    await vi.advanceTimersByTimeAsync(100);
    expect(lease.heartbeatFailureCode).toBe("request_failed");

    await expect(
      lease.beforeFinalSubmit(finalSubmitProof()),
    ).rejects.toMatchObject({ status: 409 });

    expect(irreversibleCalls).toBe(1);
    expect(lease.finalSubmitAttempted).toBe(true);
    expect(lease.finalSubmitAuthorized).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it("derives a stable bounded process owner without preserving unsafe input", () => {
    const fallback = runnerOwnerId();
    expect(runnerOwnerId()).toBe(fallback);
    expect(Buffer.byteLength(fallback)).toBeLessThanOrEqual(128);

    const unsafe = "runner with spaces/".repeat(20);
    const bounded = runnerOwnerId(unsafe);
    expect(runnerOwnerId(unsafe)).toBe(bounded);
    expect(bounded).toMatch(/^runner-[a-f0-9]{48}$/);
    expect(Buffer.byteLength(bounded)).toBeLessThanOrEqual(128);
  });

  it("allows plaintext worker credentials only for explicit loopback development origins", () => {
    const fetch = vi.fn() as typeof globalThis.fetch;
    expect(() =>
      createClient(fetch, { origin: "http://jobs.internal:8080" }),
    ).toThrow("configuration");
    expect(() => createClient(fetch, { origin: "http://example.com" })).toThrow(
      "configuration",
    );

    for (const origin of [
      "http://localhost:8080",
      "http://127.0.0.1:8080",
      "http://[::1]:8080",
    ]) {
      expect(() => createClient(fetch, { origin })).not.toThrow();
    }
    expect(() =>
      createClient(fetch, { origin: "https://jobs.internal" }),
    ).not.toThrow();
  });

  it("loads the signing key from worker auth env without accepting the legacy token", () => {
    expect(() =>
      createExecutionLeaseClientFromEnv(runnerVolume(), {
        BLUEY_JOBS_API_ORIGIN: "https://jobs.internal",
        BLUEY_JOBS_WORKER_TOKEN: WORKER_SIGNING_KEY,
        BLUEY_JOBS_RUNNER_ID: "runner-env-test",
      }),
    ).toThrow("configuration");
    expect(() =>
      createExecutionLeaseClientFromEnv(runnerVolume(), {
        BLUEY_JOBS_API_ORIGIN: "https://jobs.internal",
        BLUEY_JOBS_WORKER_SIGNING_KEY: WORKER_SIGNING_KEY,
        BLUEY_JOBS_RUNNER_ID: "runner-env-test",
      }),
    ).not.toThrow();
  });
});

function managedCloudRelease(): ManagedCloudReleaseMemoAuthority {
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

function managedCloudRuntime(): ManagedCloudRuntimeInstance {
  const release = managedCloudRelease();
  return {
    grantId: "managed-cloud-grant-1234567890",
    runtimeInstanceId: "managed-runner-instance-1234",
    runtimeIdentitySha256: "9".repeat(64),
    workerId: "managed-runner-worker-1234",
    scope: release.scope,
    activationSha256: release.activationSha256,
    activationExpiresAtMs: release.activationExpiresAtMs,
    manifestSha256: release.manifestSha256,
    componentId: "jobs-runner",
    role: "managed_runner",
    headRevision: release.headRevision,
    transitionSha256: release.transitionSha256,
    artifactSha256: "a".repeat(64),
    configSchemaSha256: "b".repeat(64),
    migrationSetSha256: "c".repeat(64),
    protocolSetSha256: "d".repeat(64),
    taskQueueSha256: release.taskQueueSha256,
    failureConverterSha256: release.failureConverterSha256,
    dependencyEvidenceSha256: "e".repeat(64),
    instanceEpoch: 3,
    nextHeartbeatSequence: 1,
    claimedAtMs: 1_800_000_000_100,
    replayed: false,
  };
}

function managedResponseCorrelation(
  workflowRequestId: string,
  runtime = managedCloudRuntime(),
) {
  return {
    managedCloudWorkflowRequestId: workflowRequestId,
    managedCloudRuntimeInstanceId: runtime.runtimeInstanceId,
    managedCloudRuntimeInstanceEpoch: runtime.instanceEpoch,
    managedCloudWorkerId: runtime.workerId,
  };
}

function managedCloudAuthority(release: ManagedCloudReleaseMemoAuthority) {
  const { version: _, ...execution } = release;
  const value = {
    version: 1,
    execution,
    authorization: {
      currentHeadRevision: release.headRevision,
      currentTransitionSha256: release.transitionSha256,
      currentActivationSha256: release.activationSha256,
      currentManifestSha256: release.manifestSha256,
      currentActivationExpiresAtMs: release.activationExpiresAtMs,
      currentTaskQueueSha256: release.taskQueueSha256,
      currentFailureConverterSha256: release.failureConverterSha256,
      currentReadinessSha256: release.readinessSha256,
      recoveryAccepted: false,
      recoveryAuthorizationSha256: "f".repeat(64),
      authorizedAtMs: 1_800_000_000_200,
    },
  };
  value.authorization.recoveryAuthorizationSha256 = recoveryAuthorizationSha256(
    value as never,
  );
  return parseManagedCloudGatewayAuthority(value);
}

function createClient(
  fetch: typeof globalThis.fetch,
  overrides: Partial<
    ConstructorParameters<typeof ExecutionLeaseClient>[0]
  > = {},
): ExecutionLeaseClient {
  return new ExecutionLeaseClient({
    origin: "https://jobs-api.example",
    workerSigningKey: WORKER_SIGNING_KEY,
    ownerId: "runner-test-1",
    runnerVolume: runnerVolume(),
    heartbeatIntervalMs: 60_000,
    fetch,
    ...overrides,
  });
}

function finalSubmitProof() {
  return {
    schemaVersion: 3 as const,
    adapter: "greenhouse" as const,
    adapterVersion: "2026.07.1-beta.1",
    control: "greenhouse_submit_application" as const,
    target: {
      actionUrl: "https://boards.greenhouse.io/acme/jobs/123",
      method: "post",
      enctype: "multipart/form-data",
      formTarget: "_self",
      providerJobKey: "greenhouse:acme:123",
      formIdentity: "greenhouse-form",
    },
    files: [
      {
        fieldName: "resume",
        name: `resume-${"a".repeat(64)}.pdf`,
        byteLength: 1,
        sha256: "a".repeat(64),
      },
    ],
    fields: [
      {
        fieldName: "job_id",
        valueByteLength: 3,
        valueSha256: "d".repeat(64),
      },
    ],
    partOrder: [
      { kind: "field" as const, index: 0 },
      { kind: "file" as const, index: 0 },
    ],
    job: {
      approvedCanonicalUrl: "https://boards.greenhouse.io/acme/jobs/123",
      pageUrl: "https://boards.greenhouse.io/acme/jobs/123#app",
    },
    documents: [
      {
        kind: "resume" as const,
        versionId: "resume-version-123",
        sha256: "a".repeat(64),
      },
    ],
  };
}

function certifiedFinalSubmitProof() {
  return {
    ...finalSubmitProof(),
    schemaVersion: 4 as const,
    certification: {
      schemaVersion: 1 as const,
      provider: "greenhouse" as const,
      adapterVersion: "2026.07.1-beta.1",
      manifestSha256: "1".repeat(64),
      activationSha256: "2".repeat(64),
      activationGeneration: 2,
      targetKeySha256: "3".repeat(64),
      layoutSetSha256: "4".repeat(64),
      adapterBundleSha256: "5".repeat(64),
      runnerTargetSha256s: ["6".repeat(64)],
      expiresAtMs: Date.now() + 60_000,
    },
    observedSurface: {
      schemaVersion: 1 as const,
      variantKey: "public",
      layoutContractVersion: 1,
      surfaceSha256: certifiedObservedSurfaceSha256(),
    },
  };
}

function certifiedReceiptAuthority() {
  return {
    schemaVersion: 1 as const,
    accountId: CLAIM.accountId,
    applicationId: CLAIM.applicationId,
    runId: CLAIM.runId,
    provider: "greenhouse" as const,
    adapter: "greenhouse" as const,
    adapterVersion: "2026.07.1-beta.1",
    manifestSha256: "1".repeat(64),
    activationSha256: "2".repeat(64),
    activationGeneration: 2,
    targetKeySha256: "3".repeat(64),
    layoutSetSha256: "4".repeat(64),
    layoutObservationSha256: "7".repeat(64),
    observedSurfaceSha256: certifiedObservedSurfaceSha256(),
    adapterBundleSha256: "5".repeat(64),
    runnerKind: "cloud" as const,
    runnerTargetSha256: "6".repeat(64),
    bindingSha256: "8".repeat(64),
    bindingFence: 1,
    bindingConsumedAtMs: Date.now(),
    applicationAttemptId: "attempt-123",
    phaseBRequestId: "phase-b-request-123",
    rolloutChannel: "canary" as const,
    canaryReservationSha256: "9".repeat(64),
    meteringReservationSha256: "a".repeat(64),
  };
}

function certifiedObservedSurfaceSha256(): string {
  return finalSubmitSurfaceSha256(finalSubmitProof());
}

function expectSignedWorkerRequest(
  call: { url: string; init?: RequestInit },
  workerId: string,
): void {
  const url = new URL(call.url);
  const headers = new Headers(call.init?.headers);
  const body = String(call.init?.body ?? "");
  const timestamp = headers.get("x-bluey-jobs-worker-timestamp");
  const nonce = headers.get("x-bluey-jobs-worker-nonce");
  const contentSha256 = createHash("sha256").update(body).digest("hex");
  expect(headers.get("x-bluey-jobs-worker-id")).toBe(workerId);
  expect(headers.get("x-bluey-jobs-worker-audience")).toBe("bluey-jobs-api");
  expect(headers.get("x-bluey-jobs-worker-scope")).toBe("execution");
  expect(headers.get("x-bluey-jobs-worker-content-sha256")).toBe(contentSha256);
  expect(timestamp).toMatch(/^\d+$/);
  expect(nonce).toMatch(/^[A-Za-z0-9._:-]{24,128}$/);
  const canonical = [
    "bluey-jobs-worker-v1",
    timestamp,
    nonce,
    workerId,
    "bluey-jobs-api",
    "execution",
    "POST",
    url.pathname,
    contentSha256,
  ].join("\n");
  expect(headers.get("x-bluey-jobs-worker-signature")).toBe(
    createHmac("sha256", WORKER_SIGNING_KEY).update(canonical).digest("hex"),
  );
}

function grantResponse(overrides: Record<string, unknown> = {}): Response {
  return new Response(
    JSON.stringify({
      run_id: "run-123",
      lease_token: "lease-secret-value",
      fence: 7,
      lease_expires_at_ms: Date.now() + 60_000,
      phase: "prepared",
      purge_subject: PURGE_SUBJECT,
      volume_id: VOLUME_ID,
      enrollment_epoch: 1,
      process_instance_id: PROCESS_INSTANCE_ID,
      volume_key_fingerprint: VOLUME_KEY_FINGERPRINT,
      runtime_grant_id: RUNTIME_GRANT_ID,
      runtime_sha256: RUNTIME_SHA256,
      ...overrides,
    }),
    {
      status: 200,
      headers: { "Content-Type": "application/json" },
    },
  );
}

function runnerVolume() {
  return {
    volumeId: VOLUME_ID,
    enrollmentEpoch: 1,
    processInstanceId: PROCESS_INSTANCE_ID,
    keyFingerprint: VOLUME_KEY_FINGERPRINT,
    runtimeGrantId: RUNTIME_GRANT_ID,
    runtimeSha256: RUNTIME_SHA256,
    createExecutionLeaseClaimProof: () => VOLUME_PROOF,
  };
}

function recordResponse(
  phase:
    | "prepared"
    | "click_started"
    | "released"
    | "side_effect_unknown"
    | "submitted",
  overrides: Record<string, unknown> = {},
): Response {
  return new Response(
    JSON.stringify({
      run_id: "run-123",
      fence: 7,
      lease_expires_at_ms: Date.now() + 60_000,
      phase,
      ...overrides,
    }),
    {
      status: 200,
      headers: { "Content-Type": "application/json" },
    },
  );
}
