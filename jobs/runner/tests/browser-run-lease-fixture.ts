import {
  assertApprovedExecutionChecksum,
  assertFinalSubmitProof,
  type AtsCertifiedReceiptAuthority,
  type FinalSubmitProof,
} from "@bluey/jobs-automation";
import { ExecutionLeaseClient } from "../src/execution-lease.js";
import type { CloudRunRequest } from "../src/server.js";

// This is a closed synthetic protocol peer, not DB, signature, quorum, or
// managed-runtime authority. The production lease client and parser remain real.
export type BrowserRunLeaseMode = "success" | "reject" | "response-loss";
export type BrowserRunLeaseCall =
  | { operation: "claim" | "heartbeat" | "finish" }
  | { operation: "irreversible"; proof: FinalSubmitProof };

export async function createBrowserRunLeaseFixture(
  input: CloudRunRequest,
  mode: BrowserRunLeaseMode,
  serverClockLeadMs = 0,
) {
  assertApprovedExecutionChecksum(input.packet, input.job);
  const admission = input.packet.approvedExecutionAdmission;
  if (input.packet.approvedExecutionSchemaVersion !== 3
    || admission?.kind !== "track_auto_submit" || !admission.ats_certification
    || input.packet.applicationId !== input.applicationId
    || input.packet.applicationIdentityId !== input.applicationIdentityId
    || input.packet.browserProfileId !== input.browserProfileId) {
    throw new Error("Synthetic lease fixture requires a bound certified packet");
  }
  const certification = admission.ats_certification;
  const volumeId = Buffer.alloc(32, 1).toString("base64url");
  const processInstanceId = Buffer.alloc(32, 2).toString("base64url");
  const purgeSubject = Buffer.alloc(32, 3).toString("base64url");
  const keyFingerprint = "4".repeat(64);
  const runtimeGrantId = "runner-process-runtime-grant-test";
  const runtimeSha256 = "7".repeat(64);
  const leaseToken = Buffer.alloc(32, 11).toString("base64url");
  const calls: BrowserRunLeaseCall[] = [];
  const response = (body: object) => new Response(JSON.stringify(body), {
    status: 200, headers: { "Content-Type": "application/json" },
  });
  const record = (phase: string) => ({
    run_id: input.runId, fence: 7, lease_expires_at_ms: Date.now() + 60_000, phase,
  });
  const fetch: typeof globalThis.fetch = async (url, init) => {
    const parsed = new URL(String(url));
    if (parsed.origin !== "https://jobs-api.example.test") {
      throw new Error("Unexpected synthetic lease origin");
    }
    const path = parsed.pathname;
    if (init?.method !== "POST" || typeof init.body !== "string") {
      throw new Error("Unexpected synthetic lease request");
    }
    const body = JSON.parse(init.body) as Record<string, unknown>;
    if (body.account_id !== input.accountId || body.application_id !== input.applicationId) {
      throw new Error("Synthetic lease owner mismatch");
    }
    if (path === "/api/jobs/internal/execution-leases/claim") {
      if (body.run_id !== input.runId || body.browser_profile_id !== input.browserProfileId) {
        throw new Error("Synthetic lease claim mismatch");
      }
      calls.push({ operation: "claim" });
      return response({
        ...record("prepared"), lease_token: leaseToken, purge_subject: purgeSubject,
        volume_id: volumeId, enrollment_epoch: 1, process_instance_id: processInstanceId,
        volume_key_fingerprint: keyFingerprint, runtime_grant_id: runtimeGrantId,
        runtime_sha256: runtimeSha256,
      });
    }
    const prefix = `/api/jobs/internal/execution-leases/${encodeURIComponent(input.runId)}/`;
    if (!path.startsWith(prefix) || body.fence !== 7
      || body.lease_token !== leaseToken) {
      throw new Error("Synthetic lease fence mismatch");
    }
    const operation = path.slice(prefix.length);
    if (operation === "heartbeat" || operation === "finish") {
      calls.push({ operation });
      return operation === "heartbeat" ? response(record("prepared")) : new Response(null, { status: 204 });
    }
    if (operation !== "irreversible" || body.action !== "submit") {
      throw new Error("Unexpected synthetic lease operation");
    }
    const proof = body.final_submit_proof;
    assertFinalSubmitProof(proof);
    if (proof.schemaVersion !== 4
      || proof.job.approvedCanonicalUrl !== input.job.canonicalUrl
      || proof.documents.find((document) => document.kind === "resume")?.versionId
        !== input.packet.resumeVersionId
      || proof.certification.provider !== certification.provider
      || proof.certification.adapterVersion !== certification.adapter_version
      || proof.certification.manifestSha256 !== certification.manifest_sha256
      || proof.certification.activationSha256 !== certification.activation_sha256
      || proof.certification.activationGeneration !== certification.activation_generation
      || proof.certification.targetKeySha256 !== certification.target_key_sha256
      || proof.certification.layoutSetSha256 !== certification.layout_set_sha256
      || proof.certification.adapterBundleSha256 !== certification.adapter_bundle_sha256
      || proof.certification.expiresAtMs !== certification.expires_at_ms
      || JSON.stringify(proof.certification.runnerTargetSha256s)
        !== JSON.stringify(certification.runner_target_sha256s)
      || proof.observedSurface.surfaceSha256 !== certification.surface_sha256
      || proof.observedSurface.variantKey !== certification.variant_key
      || proof.observedSurface.layoutContractVersion !== certification.layout_contract_version) {
      throw new Error("Synthetic certified proof mismatch");
    }
    calls.push({ operation: "irreversible", proof });
    if (mode === "reject") return new Response(null, { status: 409 });
    if (mode === "response-loss") throw new TypeError("Synthetic response loss");
    const runnerTarget = certification.runner_target_sha256s[0];
    if (!runnerTarget) throw new Error("Synthetic runner target missing");
    const authority: AtsCertifiedReceiptAuthority = {
      schemaVersion: 1, accountId: input.accountId, applicationId: input.applicationId,
      runId: input.runId, provider: proof.certification.provider, adapter: proof.adapter,
      adapterVersion: proof.adapterVersion, manifestSha256: certification.manifest_sha256,
      activationSha256: certification.activation_sha256,
      activationGeneration: certification.activation_generation,
      targetKeySha256: certification.target_key_sha256, layoutSetSha256: certification.layout_set_sha256,
      layoutObservationSha256: "8".repeat(64), observedSurfaceSha256: proof.observedSurface.surfaceSha256,
      adapterBundleSha256: certification.adapter_bundle_sha256, runnerKind: "cloud",
      runnerTargetSha256: runnerTarget, bindingSha256: "9".repeat(64), bindingFence: 1,
      bindingConsumedAtMs: Date.now() + serverClockLeadMs, applicationAttemptId: "synthetic-attempt",
      phaseBRequestId: "synthetic-phase-b", rolloutChannel: "canary",
      canaryReservationSha256: "a".repeat(64), meteringReservationSha256: "b".repeat(64),
    };
    return response({ ...record("click_started"), atsCertifiedReceiptAuthority: authority });
  };
  const client = new ExecutionLeaseClient({
    origin: "https://jobs-api.example.test",
    workerSigningKey: "synthetic-worker-signing-key-0123456789abcdef",
    ownerId: "synthetic-runner",
    heartbeatIntervalMs: 60_000,
    fetch,
    runnerVolume: {
      volumeId, enrollmentEpoch: 1, processInstanceId, keyFingerprint, runtimeGrantId,
      runtimeSha256,
      createExecutionLeaseClaimProof: () => ({
        version: 1, audience: "bluey-jobs-runner-volume-authority",
        operation: "execution_lease_claim", requestId: "synthetic-proof",
        volumeId, enrollmentEpoch: 1, processInstanceId, issuedAtMs: 1_000,
        payloadSha256: "5".repeat(64), signature: Buffer.alloc(64, 6).toString("base64url"),
      }),
    },
  });
  const lease = await client.claim({
    accountId: input.accountId, applicationId: input.applicationId, runId: input.runId,
    browserProfileId: input.browserProfileId,
  });
  return { lease, calls, close: () => lease.stopHeartbeat() };
}
