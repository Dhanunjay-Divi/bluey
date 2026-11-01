// Private protocol fixture only. This adapter does not construct or measure a
// native RunnerVolumeClient, browser, container, or cloud runtime.
import assert from "node:assert/strict";
import { createHash, createPrivateKey, createPublicKey, sign } from "node:crypto";
import { ExecutionLeaseClient } from "../src/execution-lease.ts";
import { createJobsWorkerAuthHeaders } from "../../automation/src/worker-auth.ts";
import {
  canonicalRunnerVolumeAuthorityProof,
  createRunnerVolumeAuthorityRequestId,
  runnerVolumeHttpPayloadSha256,
} from "../src/runner-volume-client.ts";
import {
  managedCloudReleaseMemoBytes,
  parseManagedCloudReleaseMemo,
} from "../../automation/src/managed-cloud-execution.ts";

const PRIVATE_PREFIX = Buffer.from("302e020100300506032b657004220420", "hex");
const PUBLIC_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const CLAIM_FAULTS = Object.freeze([
  "proof_signature", "proof_payload", "runtime_grant", "runtime_hash", "runtime_instance", "wrong_worker",
]);

// Fresh transport authentication for a terminal replay. Calling finish() twice
// on ActiveExecutionLease only reuses its cached Promise and proves no replay.
export function fixtureWorkerAuthHeaders({ workerSigningKey, workerId, path, body }) {
  return createJobsWorkerAuthHeaders({ signingKey: workerSigningKey, workerId,
    method: "POST", path, body });
}

export function createFixtureLeaseClient({ origin, workerSigningKey, key, runtime, fetch, claimFault }) {
  // Reject unknown selectors before inspecting private keys or descriptors. No mutation callback,
  // arbitrary target, or production-client guard bypass is supported by this private adapter.
  assert.ok(claimFault === undefined || claimFault === null || CLAIM_FAULTS.includes(claimFault),
    "fixture_claim_fault");
  assert.equal(key.version, 1, "fixture_key_version");
  assert.equal(key.worker_id, runtime.workerId, "fixture_key_worker");
  assert.equal(runtime.role, "managed_runner", "fixture_runtime_role");
  assert.deepEqual(runtime.scope, { environment: "staging", region: "us-east-1", channel: "general" },
    "fixture_runtime_scope");
  const seed = Buffer.from(key.signing_seed_base64url, "base64url");
  assert.equal(seed.length, 32, "fixture_key_seed_length");
  assert.equal(seed.toString("base64url"), key.signing_seed_base64url, "fixture_key_seed_encoding");
  const privateDer = Buffer.concat([PRIVATE_PREFIX, seed]);
  const privateKey = createPrivateKey({ key: privateDer, format: "der", type: "pkcs8" });
  privateDer.fill(0); seed.fill(0);
  const publicDer = createPublicKey(privateKey).export({ format: "der", type: "spki" });
  assert.ok(publicDer.subarray(0, PUBLIC_PREFIX.length).equals(PUBLIC_PREFIX), "fixture_public_key_prefix");
  const publicBytes = publicDer.subarray(PUBLIC_PREFIX.length);
  assert.equal(publicBytes.toString("base64url"), key.public_key_base64url, "fixture_key_pair");
  assert.equal(createHash("sha256").update(publicBytes).digest("hex"), key.volume.key_fingerprint,
    "fixture_key_fingerprint");
  // Original key/descriptor identity checks above always run first. Fault views are private
  // clones; neither the original handoff nor any production runtime authority is changed.
  const effectiveRuntime = claimFault === "runtime_instance" || claimFault === "wrong_worker"
    ? structuredClone(runtime) : runtime;
  const workerId = claimFault === "wrong_worker"
    ? `fixture-wrong-worker-${createRunnerVolumeAuthorityRequestId()}` : key.worker_id;
  if (claimFault === "wrong_worker") {
    assert.notEqual(workerId, key.worker_id, "fixture_alternate_worker");
    effectiveRuntime.workerId = workerId;
  }
  if (claimFault === "runtime_instance") {
    effectiveRuntime.runtimeInstanceId = `${runtime.runtimeInstanceId[0] === "a" ? "b" : "a"}${runtime.runtimeInstanceId.slice(1)}`;
  }
  const volume = {
    volumeId: key.volume.volume_id,
    enrollmentEpoch: key.volume.enrollment_epoch,
    processInstanceId: key.volume.process_instance_id,
    keyFingerprint: key.volume.key_fingerprint,
    // Mutate the volume view before production claim serialization, so its fresh proof covers
    // exactly the same intentionally wrong grant/hash sent in the actual HTTP body.
    runtimeGrantId: claimFault === "runtime_grant"
      ? `${key.volume.runtime_grant_id[0] === "a" ? "b" : "a"}${key.volume.runtime_grant_id.slice(1)}`
      : key.volume.runtime_grant_id,
    runtimeSha256: claimFault === "runtime_hash"
      ? `${key.volume.runtime_sha256[0] === "0" ? "1" : "0"}${key.volume.runtime_sha256.slice(1)}`
      : key.volume.runtime_sha256,
    createExecutionLeaseClaimProof(input) {
      const memo = parseManagedCloudReleaseMemo(input.managedCloudRelease);
      const memoHash = createHash("sha256").update(managedCloudReleaseMemoBytes(memo)).digest("hex");
      assert.equal(memoHash, input.managedCloudReleaseSha256, "fixture_memo_hash");
      const path = "/api/jobs/internal/execution-leases/claim";
      const unsigned = {
        version: 1,
        audience: "bluey-jobs-runner-volume-authority",
        operation: "execution_lease_claim",
        requestId: createRunnerVolumeAuthorityRequestId(),
        volumeId: volume.volumeId,
        enrollmentEpoch: volume.enrollmentEpoch,
        processInstanceId: volume.processInstanceId,
        issuedAtMs: Date.now(),
        payloadSha256: runnerVolumeHttpPayloadSha256(path, workerId, [
          `account_id=${input.accountId}`,
          `application_id=${input.applicationId}`,
          `run_id=${input.runId}`,
          `browser_profile_id=${input.browserProfileId}`,
          `owner_id=${input.ownerId}`,
          `volume_id=${volume.volumeId}`,
          `enrollment_epoch=${volume.enrollmentEpoch}`,
          `process_instance_id=${volume.processInstanceId}`,
          `runtime_grant_id=${volume.runtimeGrantId}`,
          `runtime_sha256=${volume.runtimeSha256}`,
          `workflow_request_id=${input.workflowRequestId}`,
          `managed_cloud_release_sha256=${memoHash}`,
          `managed_cloud_runtime_instance_id=${input.managedCloudRuntimeInstanceId}`,
          `managed_cloud_runtime_instance_epoch=${input.managedCloudRuntimeInstanceEpoch}`,
        ]),
      };
      const proof = { ...unsigned,
        signature: sign(null, canonicalRunnerVolumeAuthorityProof(unsigned), privateKey).toString("base64url") };
      if (claimFault === "proof_signature") {
        // Change a nonfinal base64url character; the 64-byte signature encoding stays canonical.
        proof.signature = `${proof.signature[0] === "A" ? "B" : "A"}${proof.signature.slice(1)}`;
      } else if (claimFault === "proof_payload") {
        // Change only the digest AFTER signing the correct fresh proof. Signature/request ID
        // stay untouched; this is a payload-integrity negative, not a valid alternate authority.
        proof.payloadSha256 = `${proof.payloadSha256[0] === "0" ? "1" : "0"}${proof.payloadSha256.slice(1)}`;
      }
      return proof;
    },
  };
  return new ExecutionLeaseClient({ origin, workerSigningKey, ownerId: workerId,
    runnerVolume: volume, managedCloudRuntime: effectiveRuntime, heartbeatIntervalMs: 1_000, fetch });
}
