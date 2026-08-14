import { createHash, generateKeyPairSync } from "node:crypto";
import { lstatSync, realpathSync, type Stats } from "node:fs";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  managedCloudReleaseMemoBytes,
  type ManagedCloudReleaseMemoAuthority,
} from "@bluey/jobs-automation/managed-cloud-execution";
import {
  AccountResidencyIndex,
  accountPurgeSubjectHash,
} from "../src/account-residency.js";
import {
  canonicalRunnerVolumeAuthorityProof,
  canonicalRunnerVolumeEnrollmentProof,
  runnerVolumeHttpPayloadSha256,
  runnerProcessRuntimeSha256,
  RunnerVolumeClient,
  type RunnerVolumeClientError,
} from "../src/runner-volume-client.js";
import {
  openRunnerDataRoot,
  type RunnerDataRoot,
} from "../src/safe-runner-storage.js";
import {
  createRunnerProcessInstanceId,
  loadOrCreateRunnerVolumeIdentity,
  signEd25519,
  verifyEd25519,
  type RunnerVolumeIdentity,
} from "../src/volume-identity.js";
import {
  canonicalRunnerVolumePurgeCommand,
  ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256,
  parseRunnerVolumePurgeAck,
  RunnerVolumePurger,
  RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
  type RunnerVolumePurgeCommand,
  type UnsignedRunnerVolumePurgeCommand,
} from "../src/volume-purge.js";
import {
  runnerDataRootFromEnv,
  runnerProcessRuntimeGrantFromEnv,
} from "../src/server.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerMoveOutcome,
  NativeRunnerStorageDirectory,
  NativeRunnerStorageRoot,
} from "../src/native-runner-storage.js";
import {
  SubjectStorageManager,
  type ManagedProfileStorage,
} from "../src/subject-storage-manager.js";
import {
  RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256,
  runnerVolumeStorageAttestationSha256,
  type RunnerVolumeStorageAttestation,
} from "../src/storage-attestation.js";

const WORKER_ID = "runner-volume-test";
const WORKER_SIGNING_KEY = "worker-signing-key-secret-0123456789abcdef";
const GRANT_ID = "grant-test-1";
const GRANT_TOKEN = Buffer.alloc(32, 4).toString("base64url");
const RESOURCE_FINGERPRINT = "8".repeat(64);
const RUNNER_BUILD_ID = "runner-602.1";
const RUNTIME_GRANT_ID = "runtime-grant-test-1";
const RUNTIME_GRANT_TOKEN = Buffer.alloc(32, 9).toString("base64url");
const PROCESS_RUNTIME = {
  runnerImageSha256: "1".repeat(64),
  runnerBuildId: RUNNER_BUILD_ID,
  platform: "linux" as const,
  architecture: "x86_64" as const,
  automationBundleSha256: "2".repeat(64),
  playwrightVersion: "1.61.1",
  chromiumRevision: "chromium-123456",
  chromiumExecutableSha256: "3".repeat(64),
};
const PROCESS_RUNTIME_SHA256 = runnerProcessRuntimeSha256(PROCESS_RUNTIME);
const PROFILE_SCOPE = "a".repeat(40);
const SUBJECT = Buffer.alloc(32, 5).toString("base64url");
const WORKFLOW_REQUEST_ID =
  "wfreq-v2-12345678-1234-5678-9234-123456789abc";

const cleanups: string[] = [];

afterEach(async () => {
  vi.restoreAllMocks();
  await Promise.all(
    cleanups
      .splice(0)
      .map((path) => rm(path, { recursive: true, force: true })),
  );
});

describe("runner volume client", () => {
  it("freezes the process runtime digest across Rust and TypeScript", () => {
    expect(PROCESS_RUNTIME_SHA256).toBe(
      "0a6faf7674e8166a8d54d0aea177f73842012dcbd68c15a80fc5ed2571c75dfd",
    );
    expect(
      runnerProcessRuntimeSha256({
        ...PROCESS_RUNTIME,
        chromiumRevision: "chromium-mutated",
      }),
    ).not.toBe(PROCESS_RUNTIME_SHA256);
  });

  it("requires an explicit runner data root instead of falling back to temporary storage", () => {
    expect(() => runnerDataRootFromEnv({})).toThrow(
      "BLUEY_JOBS_RUNNER_DATA is required",
    );
    expect(
      runnerDataRootFromEnv({ BLUEY_JOBS_RUNNER_DATA: "/srv/bluey-runner" }),
    ).toBe("/srv/bluey-runner");
  });

  it("loads deployment runtime authority separately from the persistent volume", () => {
    expect(
      runnerProcessRuntimeGrantFromEnv(
        RUNNER_BUILD_ID,
        {
          BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_ID: RUNTIME_GRANT_ID,
          BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_TOKEN: RUNTIME_GRANT_TOKEN,
          BLUEY_JOBS_RUNNER_IMAGE_SHA256: PROCESS_RUNTIME.runnerImageSha256,
          BLUEY_JOBS_AUTOMATION_BUNDLE_SHA256:
            PROCESS_RUNTIME.automationBundleSha256,
          BLUEY_JOBS_PLAYWRIGHT_VERSION: PROCESS_RUNTIME.playwrightVersion,
          BLUEY_JOBS_CHROMIUM_REVISION: PROCESS_RUNTIME.chromiumRevision,
          BLUEY_JOBS_CHROMIUM_EXECUTABLE_SHA256:
            PROCESS_RUNTIME.chromiumExecutableSha256,
        },
        "linux",
        "x64",
      ),
    ).toEqual({
      grantId: RUNTIME_GRANT_ID,
      grantToken: RUNTIME_GRANT_TOKEN,
      runtime: PROCESS_RUNTIME,
    });
    expect(() =>
      runnerProcessRuntimeGrantFromEnv(
        RUNNER_BUILD_ID,
        {},
        "linux",
        "x64",
      ),
    ).toThrow("BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_ID is required");
  });

  it("rejects a noncanonical runner build before enrollment", async () => {
    const fixture = await createFixture();
    expect(() =>
      createClient(fixture, successfulFetch(fixture, []), {
        runnerBuildId: "runner-602.01",
      }),
    ).toThrow("Runner volume configuration failed");
  });

  it("binds all-or-none managed release and runtime identity into the lease proof", async () => {
    const fixture = await createFixture();
    const client = createClient(fixture, successfulFetch(fixture, []));
    const release = managedCloudRelease();
    const releaseSha256 = createHash("sha256")
      .update(managedCloudReleaseMemoBytes(release))
      .digest("hex");
    const proof = client.createExecutionLeaseClaimProof({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      browserProfileId: "profile-123",
      ownerId: WORKER_ID,
      workflowRequestId: WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
      managedCloudReleaseSha256: releaseSha256,
      managedCloudRuntimeInstanceId: "managed-runner-instance-1234",
      managedCloudRuntimeInstanceEpoch: 3,
    });

    expect(proof.payloadSha256).toBe(
      runnerVolumeHttpPayloadSha256(
        "/api/jobs/internal/execution-leases/claim",
        WORKER_ID,
        [
          "account_id=account-123",
          "application_id=application-123",
          "run_id=run-123",
          "browser_profile_id=profile-123",
          `owner_id=${WORKER_ID}`,
          `volume_id=${fixture.identity.volumeId}`,
          "enrollment_epoch=1",
          `process_instance_id=${fixture.processInstanceId}`,
          `runtime_grant_id=${RUNTIME_GRANT_ID}`,
          `runtime_sha256=${PROCESS_RUNTIME_SHA256}`,
          `workflow_request_id=${WORKFLOW_REQUEST_ID}`,
          `managed_cloud_release_sha256=${releaseSha256}`,
          "managed_cloud_runtime_instance_id=managed-runner-instance-1234",
          "managed_cloud_runtime_instance_epoch=3",
        ],
      ),
    );
    expect(() => client.createExecutionLeaseClaimProof({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      browserProfileId: "profile-123",
      ownerId: WORKER_ID,
      workflowRequestId: WORKFLOW_REQUEST_ID,
    })).toThrow("configuration");
    expect(() => client.createExecutionLeaseClaimProof({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      browserProfileId: "profile-123",
      ownerId: WORKER_ID,
      workflowRequestId: WORKFLOW_REQUEST_ID,
      managedCloudRelease: release,
      managedCloudReleaseSha256: "0".repeat(64),
      managedCloudRuntimeInstanceId: "managed-runner-instance-1234",
      managedCloudRuntimeInstanceEpoch: 3,
    })).toThrow("configuration");
  });

  it("uses the exact signed wire fields and binds residency before account writes", async () => {
    const fixture = await createFixture();
    const calls: CapturedCall[] = [];
    const fetch = successfulFetch(fixture, calls);
    const client = createClient(fixture, fetch);

    const leaseProof = client.createExecutionLeaseClaimProof({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      browserProfileId: "profile-123",
      ownerId: WORKER_ID,
    });
    expect(leaseProof.operation).toBe("execution_lease_claim");
    expect(leaseProof.payloadSha256).toBe(
      runnerVolumeHttpPayloadSha256(
        "/api/jobs/internal/execution-leases/claim",
        WORKER_ID,
        [
          "account_id=account-123",
          "application_id=application-123",
          "run_id=run-123",
          "browser_profile_id=profile-123",
          `owner_id=${WORKER_ID}`,
          `volume_id=${fixture.identity.volumeId}`,
          "enrollment_epoch=1",
          `process_instance_id=${fixture.processInstanceId}`,
          `runtime_grant_id=${RUNTIME_GRANT_ID}`,
          `runtime_sha256=${PROCESS_RUNTIME_SHA256}`,
        ],
      ),
    );

    await client.start();
    expect(client.ready).toBe(false);
    await client.registerProfile(SUBJECT, PROFILE_SCOPE);
    const accountPath = join(
      fixture.root.path,
      "active",
      PROFILE_SCOPE,
      "Cookies",
    );
    await mkdir(join(fixture.root.path, "active", PROFILE_SCOPE), {
      recursive: true,
      mode: 0o700,
    });
    await writeFile(accountPath, "cookie", { mode: 0o600 });
    await client.stop();

    expect(calls.map(({ path }) => path)).toEqual([
      "/api/jobs/internal/runner-volumes/enroll",
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/instances/claim`,
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/commands/poll`,
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/residencies`,
    ]);
    const enroll = calls[0]!.body as Record<string, unknown>;
    expect(Object.keys(enroll).sort()).toEqual(["grantToken", "proof"]);
    const enrollmentProof = enroll.proof as Record<string, unknown>;
    expect(Object.keys(enrollmentProof).sort()).toEqual([
      "admissionGrantId",
      "enrollmentEpoch",
      "keyFingerprint",
      "legacyArtifactCount",
      "provider",
      "providerResourceId",
      "publicKeyBase64url",
      "requestedAtMs",
      "resourceFingerprint",
      "signature",
      "volumeId",
      "workerId",
    ]);
    const { signature: enrollmentSignature, ...unsignedEnrollment } =
      enrollmentProof;
    expect(
      verifyEd25519(
        fixture.identity.publicKeyRaw,
        canonicalRunnerVolumeEnrollmentProof(unsignedEnrollment as never),
        String(enrollmentSignature),
      ),
    ).toBe(true);

    const claim = calls[1]!.body as Record<string, unknown>;
    expect(Object.keys(claim).sort()).toEqual(["proof", "runtimeGrant"]);
    expect(claim.runtimeGrant).toEqual({
      grantId: RUNTIME_GRANT_ID,
      grantToken: RUNTIME_GRANT_TOKEN,
      runtime: PROCESS_RUNTIME,
    });
    assertAuthorityProof(fixture, calls[1]!, "instance_claim", [
      `runtime_grant_id=${RUNTIME_GRANT_ID}`,
      `runtime_grant_token_sha256=${createHash("sha256")
        .update(RUNTIME_GRANT_TOKEN, "utf8")
        .digest("hex")}`,
      `runtime_sha256=${PROCESS_RUNTIME_SHA256}`,
    ]);
    const poll = calls[2]!.body as Record<string, unknown>;
    expect(Object.keys(poll).sort()).toEqual([
      "afterCommandId",
      "limit",
      "proof",
    ]);
    expect(poll.afterCommandId).toBeNull();
    assertAuthorityProof(fixture, calls[2]!, "purge_poll", [
      "after_command_id=",
      `limit=${poll.limit}`,
    ]);
    const residency = calls[3]!.body as Record<string, unknown>;
    expect(Object.keys(residency).sort()).toEqual(["proof", "purgeSubject"]);
    expect(residency.purgeSubject).toBe(SUBJECT);
    assertAuthorityProof(fixture, calls[3]!, "residency_bind", [
      `purge_subject=${SUBJECT}`,
    ]);
    expect(JSON.stringify(calls)).not.toContain("accountId");
    expect(JSON.stringify(calls)).not.toContain("account_id");
    await expect(readFile(accountPath, "utf8")).resolves.toBe("cookie");
  });

  it("reclaims and drains control after deferred orphan-profile reconciliation", async () => {
    const fixture = await createFixture();
    const calls: CapturedCall[] = [];
    const client = createClient(fixture, successfulFetch(fixture, calls));

    await client.start({ deferControlLoop: true });
    expect(client.ready).toBe(false);
    expect(calls.map(({ path }) => path)).toEqual([
      "/api/jobs/internal/runner-volumes/enroll",
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/instances/claim`,
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/commands/poll`,
    ]);

    await client.activateControlLoop();

    expect(client.ready).toBe(true);
    expect(calls.map(({ path }) => path).slice(3)).toEqual([
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/instances/claim`,
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/commands/poll`,
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/commands/poll`,
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/storage-attestations`,
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}/commands/poll`,
    ]);
    const attestationCall = calls.at(-2)!;
    const attestation = (
      attestationCall.body as {
        attestation: RunnerVolumeStorageAttestation;
      }
    ).attestation;
    expect(attestation.attestationId).toMatch(/^att-[A-Za-z0-9_-]{32}$/);
    assertAuthorityProof(fixture, attestationCall, "storage_attestation", [
      `attestation_sha256=${runnerVolumeStorageAttestationSha256(attestation)}`,
    ]);
    await client.stop();
  });

  it("keeps classified legacy storage online but non-serving until purge and attestation", async () => {
    const fixture = await createFixture();
    const locator = await fixture.residency.registerProfile(
      SUBJECT,
      PROFILE_SCOPE,
    );
    await fixture.subjectStorage.ensureProfile(locator);
    const legacyPath = join(
      fixture.root.path,
      "active",
      PROFILE_SCOPE,
      "Cookies",
    );
    await mkdir(join(fixture.root.path, "active", PROFILE_SCOPE), {
      recursive: true,
      mode: 0o700,
    });
    await writeFile(legacyPath, "classified-legacy", { mode: 0o600 });
    const command = purgeCommand(fixture, 1);
    let pollCount = 0;
    let acknowledged = false;
    let accepted:
      { readonly generation: number; readonly sha256: string } | undefined;
    let attestationCalls = 0;
    const fatalFailures: RunnerVolumeClientError[] = [];
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        }
        if (
          call.path.endsWith("/instances/claim") ||
          call.path.endsWith("/instances/heartbeat")
        ) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          pollCount += 1;
          const deliver = pollCount >= 4 && !acknowledged;
          return jsonResponse(
            pollResponse(
              deliver ? [command] : [],
              accepted !== undefined,
              pollCount >= 4 ? 1 : 0,
              acknowledged ? 1 : 0,
              fixture.serverPublicKeyRaw,
              accepted,
            ),
          );
        }
        if (call.path.endsWith("/ack")) {
          acknowledged = true;
          return jsonResponse({
            disposition: "applied",
            status: {},
            reconciledTombstoneGeneration: 1,
          });
        }
        if (call.path.endsWith("/storage-attestations")) {
          attestationCalls += 1;
          const attestation = (
            call.body as {
              attestation: RunnerVolumeStorageAttestation;
            }
          ).attestation;
          accepted = {
            generation: 1,
            sha256: runnerVolumeStorageAttestationSha256(attestation),
          };
          return jsonResponse({
            disposition: "applied",
            attestationGeneration: 1,
            fleetAttestationGeneration: 1,
            attestationSha256: accepted.sha256,
            volumeStatus: "active",
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, {
      controlIntervalMs: 100,
      onFatalControlFailure: (error) => fatalFailures.push(error),
    });

    await client.start({ deferControlLoop: true });
    await client.activateControlLoop();
    expect(client.ready).toBe(false);
    expect(attestationCalls).toBe(0);

    await waitFor(() => client.ready, 2_000);
    expect(acknowledged).toBe(true);
    expect(attestationCalls).toBe(1);
    expect(fatalFailures).toEqual([]);
    await expect(readFile(legacyPath)).rejects.toMatchObject({
      code: "ENOENT",
    });
    await client.stop();
  });

  it("prepares every command page before ACKing globally empty legacy storage", async () => {
    const fixture = await createFixture();
    const profiles = ["a", "b", "c", "d", "e"].map((value) =>
      value.repeat(40),
    );
    const subjects = profiles.map((_, index) =>
      Buffer.alloc(32, 21 + index).toString("base64url"),
    );
    for (const [index, subject] of subjects.entries()) {
      const locator = await fixture.residency.registerProfile(
        subject,
        profiles[index]!,
      );
      await fixture.subjectStorage.ensureProfile(locator);
    }
    const legacyPaths: string[] = [];
    for (const [index, profile] of profiles.entries()) {
      const directory = join(fixture.root.path, "active", profile);
      await mkdir(directory, { recursive: true, mode: 0o700 });
      const path = join(directory, "Cookies");
      legacyPaths.push(path);
      await writeFile(path, `legacy-${index}`, { mode: 0o600 });
    }
    const commands = subjects.map((subject, index) =>
      purgeCommand(fixture, index + 1, subject),
    );
    const acknowledged: string[] = [];
    const pollCursors: Array<string | null> = [];
    let pollCount = 0;
    let firstAckSawGlobalLegacyZero = false;
    let accepted:
      { readonly generation: number; readonly sha256: string } | undefined;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        }
        if (
          call.path.endsWith("/instances/claim") ||
          call.path.endsWith("/instances/heartbeat")
        ) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          pollCount += 1;
          const cursor = (call.body as { afterCommandId: string | null })
            .afterCommandId;
          pollCursors.push(cursor);
          if (pollCount < 2) {
            return jsonResponse(
              pollResponse([], false, 0, 0, fixture.serverPublicKeyRaw),
            );
          }
          const pending = commands.filter(
            (command) => !acknowledged.includes(command.commandId),
          );
          const cursorIndex =
            cursor === null
              ? -1
              : pending.findIndex(
                  (command) => command.commandId === cursor,
                );
          if (cursor !== null && cursorIndex < 0) {
            throw new Error("unknown test command cursor");
          }
          const page = pending.slice(cursorIndex + 1, cursorIndex + 3);
          const hasMore = cursorIndex + 1 + page.length < pending.length;
          const nextCursor = hasMore ? page.at(-1)!.commandId : null;
          return jsonResponse(
            pollResponse(
              page,
              pending.length === 0 && accepted !== undefined,
              0,
              0,
              fixture.serverPublicKeyRaw,
              accepted,
              nextCursor,
            ),
          );
        }
        if (call.path.endsWith("/ack")) {
          const ack = parseRunnerVolumePurgeAck(call.body);
          if (acknowledged.length === 0) {
            const remaining = await Promise.all(
              legacyPaths.map(async (path) => {
                try {
                  await lstat(path);
                  return true;
                } catch (error) {
                  if ((error as NodeJS.ErrnoException).code === "ENOENT") {
                    return false;
                  }
                  throw error;
                }
              }),
            );
            firstAckSawGlobalLegacyZero = remaining.every(
              (exists) => !exists,
            );
          }
          acknowledged.push(ack.commandId);
          expect(ack.storageEvidence.legacy.rootAfter).toMatchObject({
            artifactCount: 0,
            unclassifiedRootCount: 0,
          });
          return jsonResponse({
            disposition: "applied",
            status: {},
            reconciledTombstoneGeneration: 0,
          });
        }
        if (call.path.endsWith("/storage-attestations")) {
          const attestation = (
            call.body as { attestation: RunnerVolumeStorageAttestation }
          ).attestation;
          accepted = {
            generation: 1,
            sha256: runnerVolumeStorageAttestationSha256(attestation),
          };
          return jsonResponse({
            disposition: "applied",
            attestationGeneration: 1,
            fleetAttestationGeneration: 1,
            attestationSha256: accepted.sha256,
            volumeStatus: "active",
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, {
      controlIntervalMs: 100,
      pollLimit: 2,
    });

    await client.start({ deferControlLoop: true });
    await client.activateControlLoop();

    expect(acknowledged).toEqual(commands.map(({ commandId }) => commandId));
    expect(pollCursors).toContain(commands[1]!.commandId);
    expect(pollCursors).toContain(commands[3]!.commandId);
    expect(firstAckSawGlobalLegacyZero).toBe(true);
    expect(client.ready).toBe(true);
    for (const path of legacyPaths) {
      await expect(readFile(path)).rejects.toMatchObject({ code: "ENOENT" });
    }
    await client.stop();
  }, 20_000);

  it("promotes a response-lost attestation once polling proves exact commit", async () => {
    const fixture = await createFixture();
    const submittedBodies: Array<{
      readonly proof: Record<string, unknown>;
      readonly attestation: RunnerVolumeStorageAttestation;
    }> = [];
    const readiness: boolean[] = [];
    let accepted:
      { readonly generation: number; readonly sha256: string } | undefined;
    let loseFirstResponse = true;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        }
        if (
          call.path.endsWith("/instances/claim") ||
          call.path.endsWith("/instances/heartbeat")
        ) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(
            pollResponse(
              [],
              accepted !== undefined,
              0,
              0,
              fixture.serverPublicKeyRaw,
              accepted,
            ),
          );
        }
        if (call.path.endsWith("/storage-attestations")) {
          const body = call.body as {
            proof: Record<string, unknown>;
            attestation: RunnerVolumeStorageAttestation;
          };
          submittedBodies.push(body);
          const { attestation } = body;
          accepted = {
            generation: 1,
            sha256: runnerVolumeStorageAttestationSha256(attestation),
          };
          if (loseFirstResponse) {
            loseFirstResponse = false;
            throw new TypeError("response lost");
          }
          return jsonResponse({
            disposition: "replay",
            attestationGeneration: accepted.generation,
            fleetAttestationGeneration: 1,
            attestationSha256: accepted.sha256,
            volumeStatus: "active",
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, {
      controlIntervalMs: 100,
      onReadinessChanged: (ready) => readiness.push(ready),
    });

    await client.start({ deferControlLoop: true });
    await client.activateControlLoop();
    expect(client.ready).toBe(false);
    await waitFor(() => client.ready, 2_000);

    expect(submittedBodies).toHaveLength(1);
    expect(readiness).toEqual([true]);
    await client.stop();
  });

  it("rebinds an uncommitted ambiguous attestation after an intervening purge", async () => {
    const fixture = await createFixture();
    const locator = await fixture.residency.registerProfile(
      SUBJECT,
      PROFILE_SCOPE,
    );
    const managed = await fixture.subjectStorage.ensureProfile(locator);
    await managed.active.writeFileExclusive(
      "Cookies",
      Buffer.from("current-before-attestation"),
    );
    const command = purgeCommand(fixture, 1);
    const submitted: RunnerVolumeStorageAttestation[] = [];
    let firstAttestationFailed = false;
    let acknowledged = false;
    let accepted:
      { readonly generation: number; readonly sha256: string } | undefined;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        }
        if (
          call.path.endsWith("/instances/claim") ||
          call.path.endsWith("/instances/heartbeat")
        ) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(
            pollResponse(
              firstAttestationFailed && !acknowledged ? [command] : [],
              acknowledged && accepted !== undefined,
              0,
              0,
              fixture.serverPublicKeyRaw,
              accepted,
            ),
          );
        }
        if (call.path.endsWith("/ack")) {
          acknowledged = true;
          return jsonResponse({
            disposition: "applied",
            status: {},
            reconciledTombstoneGeneration: 0,
          });
        }
        if (call.path.endsWith("/storage-attestations")) {
          const attestation = (
            call.body as { attestation: RunnerVolumeStorageAttestation }
          ).attestation;
          submitted.push(attestation);
          if (!firstAttestationFailed) {
            firstAttestationFailed = true;
            throw new TypeError("request outcome unknown before commit");
          }
          const generation =
            attestation.predecessorAttestationGeneration + 1;
          accepted = {
            generation,
            sha256: runnerVolumeStorageAttestationSha256(attestation),
          };
          return jsonResponse({
            disposition: "applied",
            attestationGeneration: generation,
            fleetAttestationGeneration: generation,
            attestationSha256: accepted.sha256,
            volumeStatus: "active",
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, { controlIntervalMs: 100 });

    await client.start({ deferControlLoop: true });
    await client.activateControlLoop();
    await waitFor(() => client.ready, 2_000);

    expect(acknowledged).toBe(true);
    expect(submitted).toHaveLength(3);
    expect(submitted[1]).toEqual(submitted[0]);
    expect(submitted[2]!.attestationId).not.toBe(submitted[0]!.attestationId);
    expect(runnerVolumeStorageAttestationSha256(submitted[2]!)).not.toBe(
      runnerVolumeStorageAttestationSha256(submitted[0]!),
    );
    expect(submitted[2]!.predecessorAttestationGeneration).toBe(1);
    await expect(
      fixture.subjectStorage.resolveProfile(PROFILE_SCOPE),
    ).resolves.toBeNull();
    await client.stop();
  });

  it("retries an uncommitted post-purge successor when the base remains current", async () => {
    const fixture = await createFixture();
    const locator = await fixture.residency.registerProfile(
      SUBJECT,
      PROFILE_SCOPE,
    );
    const managed = await fixture.subjectStorage.ensureProfile(locator);
    await managed.active.writeFileExclusive(
      "Cookies",
      Buffer.from("current-before-post-purge-successor"),
    );
    const command = purgeCommand(fixture, 1);
    const submitted: RunnerVolumeStorageAttestation[] = [];
    const fatalFailures: unknown[] = [];
    let acknowledged = false;
    let accepted:
      | { readonly generation: number; readonly sha256: string }
      | undefined;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        }
        if (
          call.path.endsWith("/instances/claim") ||
          call.path.endsWith("/instances/heartbeat")
        ) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(
            pollResponse(
              accepted !== undefined && !acknowledged ? [command] : [],
              acknowledged && accepted?.generation === 2,
              0,
              0,
              fixture.serverPublicKeyRaw,
              accepted,
            ),
          );
        }
        if (call.path.endsWith("/ack")) {
          acknowledged = true;
          return jsonResponse({
            disposition: "applied",
            status: {},
            reconciledTombstoneGeneration: 0,
          });
        }
        if (call.path.endsWith("/storage-attestations")) {
          const attestation = (
            call.body as { attestation: RunnerVolumeStorageAttestation }
          ).attestation;
          submitted.push(attestation);
          if (submitted.length === 1) {
            accepted = {
              generation: 1,
              sha256: runnerVolumeStorageAttestationSha256(attestation),
            };
          } else if (submitted.length === 2) {
            throw new TypeError("post-purge successor outcome unknown");
          } else {
            expect(attestation).toEqual(submitted[1]);
            accepted = {
              generation: 2,
              sha256: runnerVolumeStorageAttestationSha256(attestation),
            };
          }
          return jsonResponse({
            disposition: "applied",
            attestationGeneration: accepted.generation,
            fleetAttestationGeneration: accepted.generation,
            attestationSha256: accepted.sha256,
            volumeStatus: "active",
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, {
      controlIntervalMs: 100,
      onFatalControlFailure: (failure) => fatalFailures.push(failure),
    });

    await client.start({ deferControlLoop: true });
    await client.activateControlLoop();
    await waitFor(() => client.ready, 2_000);

    expect(acknowledged).toBe(true);
    expect(fatalFailures).toEqual([]);
    expect(submitted).toHaveLength(3);
    expect(submitted[2]).toEqual(submitted[1]);
    expect(submitted[1]!.predecessorAttestationGeneration).toBe(1);
    await client.stop();
  });

  it("promotes a committed ambiguous attestation before its post-purge successor", async () => {
    const fixture = await createFixture();
    const locator = await fixture.residency.registerProfile(
      SUBJECT,
      PROFILE_SCOPE,
    );
    const managed = await fixture.subjectStorage.ensureProfile(locator);
    await managed.active.writeFileExclusive(
      "Cookies",
      Buffer.from("current-before-committed-attestation"),
    );
    const command = purgeCommand(fixture, 1);
    const submitted: RunnerVolumeStorageAttestation[] = [];
    let firstResponseLost = false;
    let acknowledged = false;
    let accepted:
      { readonly generation: number; readonly sha256: string } | undefined;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        }
        if (
          call.path.endsWith("/instances/claim") ||
          call.path.endsWith("/instances/heartbeat")
        ) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(
            pollResponse(
              firstResponseLost && !acknowledged ? [command] : [],
              acknowledged && accepted?.generation === 2,
              0,
              0,
              fixture.serverPublicKeyRaw,
              accepted,
            ),
          );
        }
        if (call.path.endsWith("/ack")) {
          acknowledged = true;
          return jsonResponse({
            disposition: "applied",
            status: {},
            reconciledTombstoneGeneration: 0,
          });
        }
        if (call.path.endsWith("/storage-attestations")) {
          const attestation = (
            call.body as { attestation: RunnerVolumeStorageAttestation }
          ).attestation;
          submitted.push(attestation);
          const generation =
            attestation.predecessorAttestationGeneration + 1;
          accepted = {
            generation,
            sha256: runnerVolumeStorageAttestationSha256(attestation),
          };
          if (!firstResponseLost) {
            firstResponseLost = true;
            throw new TypeError("response lost after commit");
          }
          return jsonResponse({
            disposition: "applied",
            attestationGeneration: generation,
            fleetAttestationGeneration: generation,
            attestationSha256: accepted.sha256,
            volumeStatus: "active",
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, { controlIntervalMs: 100 });

    await client.start({ deferControlLoop: true });
    await client.activateControlLoop();
    await waitFor(() => client.ready, 2_000);

    expect(acknowledged).toBe(true);
    expect(submitted).toHaveLength(2);
    expect(submitted[1]!.predecessorAttestationGeneration).toBe(1);
    expect(submitted[1]!.predecessorAttestationSha256).toBe(
      runnerVolumeStorageAttestationSha256(submitted[0]!),
    );
    await client.stop();
  });

  it("replaces an uncommitted attestation after tombstone bindings advance", async () => {
    const fixture = await createFixture();
    const submitted: RunnerVolumeStorageAttestation[] = [];
    let bindingsAdvanced = false;
    let accepted:
      { readonly generation: number; readonly sha256: string } | undefined;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        }
        if (
          call.path.endsWith("/instances/claim") ||
          call.path.endsWith("/instances/heartbeat")
        ) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          const generation = bindingsAdvanced ? 1 : 0;
          return jsonResponse(
            pollResponse(
              [],
              accepted !== undefined,
              generation,
              generation,
              fixture.serverPublicKeyRaw,
              accepted,
            ),
          );
        }
        if (call.path.endsWith("/storage-attestations")) {
          const attestation = (
            call.body as { attestation: RunnerVolumeStorageAttestation }
          ).attestation;
          submitted.push(attestation);
          if (!bindingsAdvanced) {
            bindingsAdvanced = true;
            throw new TypeError("request outcome unknown before commit");
          }
          accepted = {
            generation: 1,
            sha256: runnerVolumeStorageAttestationSha256(attestation),
          };
          return jsonResponse({
            disposition: "applied",
            attestationGeneration: 1,
            fleetAttestationGeneration: 1,
            attestationSha256: accepted.sha256,
            volumeStatus: "active",
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, { controlIntervalMs: 100 });

    await client.start({ deferControlLoop: true });
    await client.activateControlLoop();
    await waitFor(() => client.ready, 2_000);

    expect(submitted).toHaveLength(2);
    expect(submitted[1]!.attestationId).not.toBe(submitted[0]!.attestationId);
    expect(submitted[1]!.requiredTombstoneGeneration).toBe(1);
    expect(submitted[1]!.reconciledTombstoneGeneration).toBe(1);
    await client.stop();
  });

  it("includes all deferred recovery writes in the first attested snapshot", async () => {
    const fixture = await createFixture();
    const calls: CapturedCall[] = [];
    const client = createClient(fixture, successfulFetch(fixture, calls));

    await client.start({ deferControlLoop: true });
    await client.registerProfile(SUBJECT, PROFILE_SCOPE);
    await client.withExistingProfileResidency(
      PROFILE_SCOPE,
      async (storage) => {
        if (!storage) throw new Error("missing managed profile");
        await storage.active.writeFileExclusive(
          "recovered-cookie",
          Buffer.from("recovered-before-attestation"),
        );
      },
    );
    await client.activateControlLoop();

    const attestationCall = calls.find(({ path }) =>
      path.endsWith("/storage-attestations"),
    );
    const attestation = (
      attestationCall!.body as {
        attestation: RunnerVolumeStorageAttestation;
      }
    ).attestation;
    expect(attestation.subjectStorageSubjectCount).toBe(1);
    expect(attestation.subjectStorageScopeCount).toBe(1);
    expect(attestation.residentLocatorCount).toBe(1);
    expect(BigInt(attestation.rootFileBytes)).toBeGreaterThan(0n);
    expect(
      BigInt(attestation.subjectStorageCompleteRootFileBytes),
    ).toBeGreaterThan(0n);
    await client.stop();
  });

  it("drains retained tombstones before readiness and acknowledges exact commands", async () => {
    const fixture = await createFixture();
    const locator = await fixture.residency.registerProfile(
      SUBJECT,
      PROFILE_SCOPE,
    );
    const managed = await fixture.subjectStorage.ensureProfile(locator);
    await managed.active.writeFileExclusive(
      "Cookies",
      Buffer.from("managed-cookie"),
    );
    const accountPath = join(
      fixture.root.path,
      "active",
      PROFILE_SCOPE,
      "Cookies",
    );
    await mkdir(join(fixture.root.path, "active", PROFILE_SCOPE), {
      recursive: true,
      mode: 0o700,
    });
    await writeFile(accountPath, "cookie", { mode: 0o600 });
    const command = purgeCommand(fixture, 9);
    const calls: CapturedCall[] = [];
    let acknowledged = false;
    let stoppedSubject: string | undefined;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        calls.push(call);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 9, 0));
        }
        if (call.path.endsWith("/instances/claim")) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(
            !acknowledged
              ? pollResponse([command], false, 9, 0, fixture.serverPublicKeyRaw)
              : pollResponse([], true, 9, 9, fixture.serverPublicKeyRaw),
          );
        }
        if (call.path.endsWith(`/commands/${command.commandId}/ack`)) {
          acknowledged = true;
          return jsonResponse({
            disposition: "applied",
            status: {},
            reconciledTombstoneGeneration: 9,
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch, {
      stopAccountWork: async (subjectSha256) => {
        stoppedSubject = subjectSha256;
        return { status: "stopped" };
      },
    });

    await client.start();
    expect(client.ready).toBe(false);
    expect(stoppedSubject).toBe(accountPurgeSubjectHash(SUBJECT));
    await expect(readFile(accountPath)).rejects.toMatchObject({
      code: "ENOENT",
    });
    const ackCall = calls.find(({ path }) =>
      path.endsWith(`/${command.commandId}/ack`),
    );
    expect(ackCall?.path).toBe(
      `/api/jobs/internal/runner-volumes/${fixture.identity.volumeId}` +
        `/commands/${command.commandId}/ack`,
    );
    expect(
      Object.keys(ackCall!.body as Record<string, unknown>).sort(),
    ).toEqual([
      "afterInventoryCount",
      "afterInventorySha256",
      "audience",
      "beforeInventoryCount",
      "beforeInventorySha256",
      "commandId",
      "commandSha256",
      "completedAtMs",
      "enrollmentEpoch",
      "processInstanceId",
      "purgeGeneration",
      "purgeSubjectSha256",
      "removedCount",
      "requestId",
      "runnerBuildId",
      "signature",
      "storageEvidence",
      "storageEvidenceSha256",
      "targetKeyFingerprint",
      "targetVolumeId",
      "version",
    ]);
    const ack = parseRunnerVolumePurgeAck(ackCall!.body);
    expect(ack).toMatchObject({
      version: 2,
      commandId: command.commandId,
      purgeGeneration: command.purgeGeneration,
      storageEvidence: {
        version: 2,
        subjectStorage: {
          layoutVersion: 2,
          before: { residency: "resident" },
          after: { residency: "never_resident" },
        },
        legacy: {
          inventoryVersion: 1,
          targetAfter: { entryCount: 0, fileBytes: "0" },
          rootAfter: {
            artifactCount: 0,
            artifactBytes: "0",
            unclassifiedRootCount: 0,
          },
        },
      },
      afterInventoryCount: 0,
      removedCount: ack.beforeInventoryCount,
    });
    expect(JSON.stringify(ackCall!.body)).not.toContain(SUBJECT);
    await client.stop();
  });

  it("replays a crash ACK before re-signing it for the current process on 401", async () => {
    const fixture = await createFixture();
    const formerProcessFixture = {
      ...fixture,
      processInstanceId: createRunnerProcessInstanceId(),
    };
    await completeLocalProfilePurge(
      formerProcessFixture,
      SUBJECT,
      PROFILE_SCOPE,
      4,
    );
    const command = purgeCommand(fixture, 4);
    const acknowledgements: Array<Record<string, unknown>> = [];
    let acknowledged = false;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll")) {
          return jsonResponse(enrollmentResponse(fixture, 4, 3));
        }
        if (call.path.endsWith("/instances/claim")) {
          return jsonResponse(instanceResponse(fixture));
        }
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(
            !acknowledged
              ? pollResponse([command], false, 4, 3, fixture.serverPublicKeyRaw)
              : pollResponse([], true, 4, 4, fixture.serverPublicKeyRaw),
          );
        }
        if (call.path.endsWith(`/commands/${command.commandId}/ack`)) {
          acknowledgements.push(call.body as Record<string, unknown>);
          if (acknowledgements.length === 1) {
            return new Response('{"error":"old process"}', {
              status: 401,
              headers: { "Content-Type": "application/json" },
            });
          }
          acknowledged = true;
          return jsonResponse({
            disposition: "applied",
            status: {},
            reconciledTombstoneGeneration: 4,
          });
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch);

    await client.start();

    expect(client.ready).toBe(false);
    expect(acknowledgements).toHaveLength(2);
    const formerAck = parseRunnerVolumePurgeAck(acknowledgements[0]);
    const recoveredAck = parseRunnerVolumePurgeAck(acknowledgements[1]);
    expect(formerAck.processInstanceId).toBe(
      formerProcessFixture.processInstanceId,
    );
    expect(recoveredAck.processInstanceId).toBe(fixture.processInstanceId);
    expect(recoveredAck.beforeInventorySha256).toBe(
      formerAck.beforeInventorySha256,
    );
    expect(recoveredAck.beforeInventoryCount).toBe(
      formerAck.beforeInventoryCount,
    );
    expect(recoveredAck.storageEvidence).toEqual(formerAck.storageEvidence);
    expect(recoveredAck.storageEvidenceSha256).toBe(
      formerAck.storageEvidenceSha256,
    );
    await client.stop();
  });

  it("keeps startup closed when reconciliation is pending or work is irreversible", async () => {
    const pendingFixture = await createFixture();
    const pendingClient = createClient(
      pendingFixture,
      successfulFetch(pendingFixture, [], {
        ready: false,
        required: 2,
        reconciled: 1,
      }),
    );
    await expect(pendingClient.start()).resolves.toBeUndefined();
    expect(pendingClient.ready).toBe(false);

    const irreversibleFixture = await createFixture();
    await irreversibleFixture.residency.registerProfile(SUBJECT, PROFILE_SCOPE);
    const command = purgeCommand(irreversibleFixture, 3);
    let acknowledged = false;
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll"))
          return jsonResponse(enrollmentResponse(irreversibleFixture, 3, 0));
        if (call.path.endsWith("/instances/claim"))
          return jsonResponse(instanceResponse(irreversibleFixture));
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(
            pollResponse(
              [command],
              false,
              3,
              0,
              irreversibleFixture.serverPublicKeyRaw,
            ),
          );
        }
        acknowledged = true;
        return jsonResponse({});
      },
    ) as typeof globalThis.fetch;
    const irreversibleClient = createClient(irreversibleFixture, fetch, {
      stopAccountWork: async () => ({ status: "irreversible" }),
    });
    await expect(irreversibleClient.start()).rejects.toMatchObject({
      code: "irreversible_work",
    });
    expect(irreversibleClient.ready).toBe(false);
    expect(acknowledged).toBe(false);
  });

  it("rejects a poll response whose command key ring differs from the pinned keys", async () => {
    const fixture = await createFixture();
    const unpinnedKey = generateKeyPairSync("ed25519").publicKey.export({
      format: "jwk",
    }).x;
    if (!unpinnedKey) throw new Error("missing public key");
    const fetch = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const call = captureCall(input, init);
        if (call.path.endsWith("/enroll"))
          return jsonResponse(enrollmentResponse(fixture, 0, 0));
        if (call.path.endsWith("/instances/claim"))
          return jsonResponse(instanceResponse(fixture));
        if (call.path.endsWith("/commands/poll")) {
          return jsonResponse(pollResponse([], true, 0, 0, unpinnedKey));
        }
        throw new Error(`unexpected ${call.path}`);
      },
    ) as typeof globalThis.fetch;
    const client = createClient(fixture, fetch);

    await expect(client.start()).rejects.toMatchObject({
      code: "invalid_response",
    });
    expect(client.ready).toBe(false);
  });

  it("removes safely restored files beneath a completed tombstone before readiness", async () => {
    const fixture = await createFixture();
    await completeLocalProfilePurge(fixture, SUBJECT, PROFILE_SCOPE, 1);
    const restoredPath = join(
      fixture.root.path,
      "active",
      PROFILE_SCOPE,
      "Cookies",
    );
    await mkdir(join(fixture.root.path, "active", PROFILE_SCOPE), {
      recursive: true,
      mode: 0o700,
    });
    await writeFile(restoredPath, "restored-cookie", { mode: 0o600 });
    const client = createClient(
      fixture,
      successfulFetch(fixture, [], { ready: true, required: 1, reconciled: 1 }),
    );

    await client.start();

    expect(client.ready).toBe(false);
    await expect(readFile(restoredPath)).rejects.toMatchObject({
      code: "ENOENT",
    });
    await client.stop();
  });

  it("enforces tombstone targets while unrelated classified legacy remains control-only", async () => {
    const fixture = await createFixture();
    await completeLocalProfilePurge(fixture, SUBJECT, PROFILE_SCOPE, 1);
    const restoredPath = join(
      fixture.root.path,
      "active",
      PROFILE_SCOPE,
      "Cookies",
    );
    const unrelatedPath = join(
      fixture.root.path,
      "active",
      "b".repeat(40),
      "Cookies",
    );
    for (const [path, contents] of [
      [restoredPath, "restored-tombstoned"],
      [unrelatedPath, "unrelated-classified"],
    ] as const) {
      await mkdir(join(path, ".."), { recursive: true, mode: 0o700 });
      await writeFile(path, contents, { mode: 0o600 });
    }
    const client = createClient(
      fixture,
      successfulFetch(fixture, [], {
        ready: false,
        required: 1,
        reconciled: 1,
      }),
    );

    await expect(client.start()).resolves.toBeUndefined();
    expect(client.ready).toBe(false);
    await expect(readFile(restoredPath)).rejects.toMatchObject({
      code: "ENOENT",
    });
    await expect(readFile(unrelatedPath, "utf8")).resolves.toBe(
      "unrelated-classified",
    );
    await client.stop();
  });

  it("fails closed when a completed tombstone references a corrupt signed locator", async () => {
    const fixture = await createFixture();
    await completeLocalProfilePurge(fixture, SUBJECT, PROFILE_SCOPE, 1);
    const locator = profileLocatorPath(fixture, SUBJECT, PROFILE_SCOPE);
    await writeFile(locator, "{}\n", { mode: 0o600 });
    const fetch = vi.fn() as typeof globalThis.fetch;
    const client = createClient(fixture, fetch);

    await expect(client.start()).rejects.toMatchObject({
      code: "corrupt_locator",
    });
    expect(client.ready).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("enforces out-of-order local tombstones deterministically before readiness", async () => {
    const fixture = await createFixture();
    const subjects = [
      {
        subject: Buffer.alloc(32, 6).toString("base64url"),
        scope: "b".repeat(40),
      },
      {
        subject: Buffer.alloc(32, 7).toString("base64url"),
        scope: "c".repeat(40),
      },
    ].sort((left, right) =>
      accountPurgeSubjectHash(left.subject).localeCompare(
        accountPurgeSubjectHash(right.subject),
      ),
    );
    const higherGeneration = subjects[0]!;
    const lowerGeneration = subjects[1]!;
    await completeLocalProfilePurge(
      fixture,
      higherGeneration.subject,
      higherGeneration.scope,
      2,
    );
    await completeLocalProfilePurge(
      fixture,
      lowerGeneration.subject,
      lowerGeneration.scope,
      1,
    );
    const restored = [higherGeneration, lowerGeneration].map(({ scope }) =>
      join(fixture.root.path, "active", scope, "Cookies"),
    );
    for (const path of restored) {
      await mkdir(join(path, ".."), { recursive: true, mode: 0o700 });
      await writeFile(path, "restored-cookie", { mode: 0o600 });
    }
    const client = createClient(
      fixture,
      successfulFetch(fixture, [], { ready: true, required: 2, reconciled: 2 }),
    );

    await client.start();

    expect(client.ready).toBe(false);
    for (const path of restored) {
      await expect(readFile(path)).rejects.toMatchObject({ code: "ENOENT" });
    }
    await client.stop();
  });

  it("accepts a clean completed-tombstone replay without emitting another acknowledgement", async () => {
    const fixture = await createFixture();
    await completeLocalProfilePurge(fixture, SUBJECT, PROFILE_SCOPE, 1);
    const calls: CapturedCall[] = [];
    const client = createClient(
      fixture,
      successfulFetch(fixture, calls, {
        ready: true,
        required: 1,
        reconciled: 1,
      }),
    );

    await client.start();

    expect(client.ready).toBe(false);
    expect(calls.some(({ path }) => path.endsWith("/ack"))).toBe(false);
    await client.stop();
  });

  it("rejects runtime result recovery after a completed subject is restored", async () => {
    const fixture = await createFixture();
    const resultScope = "d".repeat(64);
    await completeLocalResultPurge(fixture, SUBJECT, resultScope, 1);
    const client = createClient(
      fixture,
      successfulFetch(fixture, [], { ready: true, required: 1, reconciled: 1 }),
    );
    await client.start();
    const restored = join(
      fixture.root.path,
      "step-results",
      `${resultScope}.json.enc`,
    );
    await mkdir(join(fixture.root.path, "step-results"), {
      recursive: true,
      mode: 0o700,
    });
    await writeFile(restored, "restored-result", { mode: 0o600 });
    const operation = vi.fn(async () => "unsafe");

    await expect(
      client.withExistingResultResidency(resultScope, operation),
    ).rejects.toMatchObject({ code: "restored_data" });
    expect(operation).not.toHaveBeenCalled();
    await client.stop();
  });

  it("serializes an absent-result read with registration and the complete write", async () => {
    const fixture = await createFixture();
    const resultScope = "e".repeat(64);
    const client = createClient(fixture, successfulFetch(fixture, []));
    await client.start();
    let releaseRead = (): void => undefined;
    const readBlocked = new Promise<void>((resolve) => {
      releaseRead = resolve;
    });
    let readStarted = (): void => undefined;
    const readEntered = new Promise<void>((resolve) => {
      readStarted = resolve;
    });
    const read = client.withExistingResultResidency(resultScope, async () => {
      readStarted();
      await readBlocked;
      return "absent";
    });
    await readEntered;
    const writeOperation = vi.fn(async () => "written");
    const write = client.withResultWriteResidency(
      SUBJECT,
      resultScope,
      writeOperation,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(writeOperation).not.toHaveBeenCalled();

    releaseRead();
    await expect(read).resolves.toBe("absent");
    await expect(write).resolves.toBe("written");
    expect(writeOperation).toHaveBeenCalledTimes(1);
    await client.stop();
  });

  it("fences profile write and existing-profile callbacks with managed subject storage", async () => {
    const fixture = await createFixture();
    const client = createClient(fixture, successfulFetch(fixture, []));
    await client.start();
    const subjectSha256 = accountPurgeSubjectHash(SUBJECT);

    let releaseWrite = (): void => undefined;
    const writeGate = new Promise<void>((resolve) => {
      releaseWrite = resolve;
    });
    let enterWrite = (): void => undefined;
    const writeEntered = new Promise<void>((resolve) => {
      enterWrite = resolve;
    });
    const write = client.withProfileWriteResidency(
      SUBJECT,
      PROFILE_SCOPE,
      async (storage) => {
        expect(storage).toMatchObject({
          kind: "profile",
          subjectSha256,
          scope: PROFILE_SCOPE,
        });
        expect(storage.active.relativePath).toContain(
          `/subjects/${subjectSha256}/profiles/${PROFILE_SCOPE}/active`,
        );
        await storage.active.writeFileExclusive(
          "Cookies",
          Buffer.from("cookie"),
        );
        enterWrite();
        await writeGate;
        return "written";
      },
    );
    await writeEntered;
    let writeCompetitorEntered = false;
    const writeCompetitor = fixture.residency.withSubjectLock(
      subjectSha256,
      async () => {
        writeCompetitorEntered = true;
      },
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(writeCompetitorEntered).toBe(false);
    releaseWrite();
    await expect(write).resolves.toBe("written");
    await writeCompetitor;
    expect(writeCompetitorEntered).toBe(true);

    let releaseRead = (): void => undefined;
    const readGate = new Promise<void>((resolve) => {
      releaseRead = resolve;
    });
    let enterRead = (): void => undefined;
    const readEntered = new Promise<void>((resolve) => {
      enterRead = resolve;
    });
    const read = client.withExistingProfileResidency(
      PROFILE_SCOPE,
      async (storage) => {
        expect(storage).not.toBeNull();
        expect(storage).toMatchObject({
          kind: "profile",
          subjectSha256,
          scope: PROFILE_SCOPE,
        });
        await expect(
          storage!.active.readFileBounded("Cookies", 64),
        ).resolves.toEqual(Buffer.from("cookie"));
        enterRead();
        await readGate;
        return "read";
      },
    );
    await readEntered;
    let readCompetitorEntered = false;
    const readCompetitor = fixture.residency.withSubjectLock(
      subjectSha256,
      async () => {
        readCompetitorEntered = true;
      },
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(readCompetitorEntered).toBe(false);
    releaseRead();
    await expect(read).resolves.toBe("read");
    await readCompetitor;
    expect(readCompetitorEntered).toBe(true);
    await client.stop();
  });

  it("revokes captured profile and result directory capabilities after callback release", async () => {
    const fixture = await createFixture();
    const client = createClient(fixture, successfulFetch(fixture, []));
    await client.start();
    const resultScope = "9".repeat(64);
    let profileDirectory: NativeRunnerStorageDirectory | undefined;
    let profileChild: NativeRunnerStorageDirectory | undefined;
    let resultDirectory: NativeRunnerStorageDirectory | undefined;
    let resultChild: NativeRunnerStorageDirectory | undefined;

    await client.withProfileWriteResidency(
      SUBJECT,
      PROFILE_SCOPE,
      async (storage) => {
        profileDirectory = storage.active;
        profileChild = await storage.active.ensureChildDirectory("Default");
      },
    );
    await client.withResultWriteResidency(
      SUBJECT,
      resultScope,
      async (storage) => {
        resultDirectory = storage.temporary;
        resultChild = await storage.temporary.ensureChildDirectory("staged");
      },
    );
    if (
      !profileDirectory ||
      !profileChild ||
      !resultDirectory ||
      !resultChild
    ) {
      throw new Error("scoped managed directory was not captured");
    }

    await expect(profileDirectory.inventory()).rejects.toMatchObject({
      code: "corrupt_state",
    });
    await expect(
      profileChild.writeFileExclusive("Cookies", Buffer.from("late")),
    ).rejects.toMatchObject({ code: "corrupt_state" });
    await expect(resultDirectory.inventory()).rejects.toMatchObject({
      code: "corrupt_state",
    });
    await expect(
      resultChild.writeFileExclusive("result", Buffer.from("late")),
    ).rejects.toMatchObject({ code: "corrupt_state" });
    await client.stop();
  });

  it("awaits and propagates an unawaited native failure before releasing the subject lock", async () => {
    const fixture = await createFixture();
    const client = createClient(fixture, successfulFetch(fixture, []));
    await client.start();
    const resultScope = "8".repeat(64);
    const subjectSha256 = accountPurgeSubjectHash(SUBJECT);
    const originalWrite =
      FilesystemNativeDirectoryForTest.prototype.writeFileExclusive;
    let rejectNative = (_error: Error): void => undefined;
    const nativeFailure = new Promise<boolean>((_resolve, reject) => {
      rejectNative = reject;
    });
    vi.spyOn(
      FilesystemNativeDirectoryForTest.prototype,
      "writeFileExclusive",
    ).mockImplementation(function (
      this: FilesystemNativeDirectoryForTest,
      name: string,
      contents: Buffer,
    ): Promise<boolean> {
      if (name === "unawaited-native-failure") return nativeFailure;
      return originalWrite.call(this, name, contents);
    });
    let enterCallback = (): void => undefined;
    const callbackEntered = new Promise<void>((resolve) => {
      enterCallback = resolve;
    });

    const operation = client.withResultWriteResidency(
      SUBJECT,
      resultScope,
      async (storage) => {
        void storage.temporary.writeFileExclusive(
          "unawaited-native-failure",
          Buffer.from("pending"),
        );
        enterCallback();
        return "callback-returned";
      },
    );
    await callbackEntered;
    let competitorEntered = false;
    const competitor = fixture.residency.withSubjectLock(
      subjectSha256,
      async () => {
        competitorEntered = true;
      },
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(competitorEntered).toBe(false);

    rejectNative(new Error("tracked-native-failure"));
    await expect(operation).rejects.toThrow("tracked-native-failure");
    await competitor;
    expect(competitorEntered).toBe(true);
    await client.stop();
  });

  it("returns a null profile only without an owner or locator and fails closed otherwise", async () => {
    const fixture = await createFixture();
    const client = createClient(fixture, successfulFetch(fixture, []));
    await client.start();

    const absentScope = "f".repeat(40);
    const absentOperation = vi.fn(
      async (storage: ManagedProfileStorage | null) => storage,
    );
    await expect(
      client.withExistingProfileResidency(absentScope, absentOperation),
    ).resolves.toBeNull();
    expect(absentOperation).toHaveBeenCalledWith(null);

    const locatorOnlyScope = "b".repeat(40);
    const locatorOnlySubject = Buffer.alloc(32, 8).toString("base64url");
    await fixture.residency.registerProfile(
      locatorOnlySubject,
      locatorOnlyScope,
    );
    const locatorOnlyOperation = vi.fn(async () => "unsafe");
    await expect(
      client.withExistingProfileResidency(
        locatorOnlyScope,
        locatorOnlyOperation,
      ),
    ).rejects.toMatchObject({ code: "corrupt_state" });
    expect(locatorOnlyOperation).not.toHaveBeenCalled();

    const ownerOnlyScope = "c".repeat(40);
    await client.registerProfile(SUBJECT, ownerOnlyScope);
    await rm(profileLocatorPath(fixture, SUBJECT, ownerOnlyScope), {
      force: true,
    });
    const ownerOnlyOperation = vi.fn(async () => "unsafe");
    await expect(
      client.withExistingProfileResidency(ownerOnlyScope, ownerOnlyOperation),
    ).rejects.toMatchObject({ code: "corrupt_state" });
    expect(ownerOnlyOperation).not.toHaveBeenCalled();
    await client.stop();
  });

  it("rejects an existing-profile callback after its subject was purged", async () => {
    const fixture = await createFixture();
    await completeLocalProfilePurge(fixture, SUBJECT, PROFILE_SCOPE, 1);
    const client = createClient(
      fixture,
      successfulFetch(fixture, [], { ready: true, required: 1, reconciled: 1 }),
    );
    await client.start();
    const restored = join(
      fixture.root.path,
      "active",
      PROFILE_SCOPE,
      "Cookies",
    );
    await mkdir(join(restored, ".."), { recursive: true, mode: 0o700 });
    await writeFile(restored, "restored-cookie", { mode: 0o600 });
    const operation = vi.fn(async () => "unsafe");

    await expect(
      client.withExistingProfileResidency(PROFILE_SCOPE, operation),
    ).rejects.toMatchObject({ code: "restored_data" });
    expect(operation).not.toHaveBeenCalled();
    await client.stop();
  });

  it("persists and replays one enrollment proof instead of rotating volume authority", async () => {
    const fixture = await createFixture();
    const calls: CapturedCall[] = [];
    const first = createClient(fixture, successfulFetch(fixture, calls), {
      nowMs: () => 1_000,
    });
    await first.start();
    await first.stop();
    const firstProof = (calls[0]!.body as { proof: unknown }).proof;

    const replayCalls: CapturedCall[] = [];
    const replay = createClient(
      fixture,
      successfulFetch(fixture, replayCalls),
      { nowMs: () => 2_000 },
    );
    await replay.start();
    await replay.stop();

    expect((replayCalls[0]!.body as { proof: unknown }).proof).toEqual(
      firstProof,
    );
    expect(replay.volumeId).toBe(first.volumeId);
    expect(replay.processInstanceId).toBe(first.processInstanceId);
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

interface Fixture {
  root: RunnerDataRoot;
  identity: RunnerVolumeIdentity;
  residency: AccountResidencyIndex;
  subjectStorage: SubjectStorageManager;
  processInstanceId: string;
  serverPrivateKey: ReturnType<typeof generateKeyPairSync>["privateKey"];
  serverPublicKeyRaw: string;
}

interface CapturedCall {
  path: string;
  body: unknown;
  headers: Headers;
}

async function createFixture(): Promise<Fixture> {
  const parent = await mkdtemp(join(tmpdir(), "bluey-runner-volume-client-"));
  cleanups.push(parent);
  const root = await openRunnerDataRoot(join(parent, "runner"));
  const identity = await loadOrCreateRunnerVolumeIdentity(root);
  const residency = new AccountResidencyIndex(root, identity);
  const subjectStorage = new SubjectStorageManager(
    new FilesystemNativeRootForTest(root.path),
    identity,
    residency,
  );
  const serverKeys = generateKeyPairSync("ed25519");
  const publicJwk = serverKeys.publicKey.export({ format: "jwk" });
  if (!publicJwk.x) throw new Error("missing public key");
  return {
    root,
    identity,
    residency,
    subjectStorage,
    processInstanceId: createRunnerProcessInstanceId(),
    serverPrivateKey: serverKeys.privateKey,
    serverPublicKeyRaw: publicJwk.x,
  };
}

function createClient(
  fixture: Fixture,
  fetch: typeof globalThis.fetch,
  overrides: Partial<ConstructorParameters<typeof RunnerVolumeClient>[0]> = {},
): RunnerVolumeClient {
  return new RunnerVolumeClient({
    origin: "https://jobs.example",
    workerSigningKey: WORKER_SIGNING_KEY,
    workerId: WORKER_ID,
    admissionGrantId: GRANT_ID,
    admissionGrantToken: GRANT_TOKEN,
    provider: "test-provider",
    providerResourceId: "provider-resource-test",
    resourceFingerprint: RESOURCE_FINGERPRINT,
    legacyArtifactCount: 0,
    runnerBuildId: RUNNER_BUILD_ID,
    processInstanceId: fixture.processInstanceId,
    processRuntimeGrant: {
      grantId: RUNTIME_GRANT_ID,
      grantToken: RUNTIME_GRANT_TOKEN,
      runtime: PROCESS_RUNTIME,
    },
    identity: fixture.identity,
    residency: fixture.residency,
    subjectStorage: fixture.subjectStorage,
    serverCommandKeys: new Map([
      ["server-key-test", fixture.serverPublicKeyRaw],
    ]),
    stopAccountWork: async () => ({ status: "stopped" }),
    controlIntervalMs: 60_000,
    fetch,
    ...overrides,
  });
}

function successfulFetch(
  fixture: Fixture,
  calls: CapturedCall[],
  readiness: { ready: boolean; required: number; reconciled: number } = {
    ready: true,
    required: 0,
    reconciled: 0,
  },
): typeof globalThis.fetch {
  let acceptedAttestation:
    { readonly generation: number; readonly sha256: string } | undefined;
  return vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
    const call = captureCall(input, init);
    calls.push(call);
    assertFleetHmac(call);
    if (call.path.endsWith("/enroll")) {
      return jsonResponse(
        enrollmentResponse(fixture, readiness.required, readiness.reconciled),
      );
    }
    if (call.path.endsWith("/instances/claim"))
      return jsonResponse(instanceResponse(fixture));
    if (call.path.endsWith("/commands/poll")) {
      return jsonResponse(
        pollResponse(
          [],
          readiness.ready && acceptedAttestation !== undefined,
          readiness.required,
          readiness.reconciled,
          fixture.serverPublicKeyRaw,
          acceptedAttestation,
        ),
      );
    }
    if (call.path.endsWith("/storage-attestations")) {
      const attestation = (
        call.body as {
          attestation: RunnerVolumeStorageAttestation;
        }
      ).attestation;
      const sha256 = runnerVolumeStorageAttestationSha256(attestation);
      acceptedAttestation = { generation: 1, sha256 };
      return jsonResponse({
        disposition: "applied",
        attestationGeneration: 1,
        fleetAttestationGeneration: 1,
        attestationSha256: sha256,
        volumeStatus: "active",
      });
    }
    if (call.path.endsWith("/residencies"))
      return new Response(null, { status: 204 });
    throw new Error(`unexpected ${call.path}`);
  }) as typeof globalThis.fetch;
}

function enrollmentResponse(
  fixture: Fixture,
  required: number,
  reconciled: number,
) {
  return {
    volumeId: fixture.identity.volumeId,
    workerId: WORKER_ID,
    provider: "test-provider",
    providerResourceId: "provider-resource-test",
    resourceFingerprint: RESOURCE_FINGERPRINT,
    currentEpoch: 1,
    enrollmentGeneration: 1,
    requiredTombstoneGeneration: required,
    reconciledTombstoneGeneration: reconciled,
    status: readinessStatus(required, reconciled),
    activeInstanceId: null,
    instanceLeaseExpiresAtMs: null,
    legacyArtifactCount: 0,
    admissionGrantId: GRANT_ID,
    enrolledAtMs: 1,
    lastSeenAtMs: 1,
    updatedAtMs: 1,
    publicKeyBase64url: fixture.identity.publicKeyRaw,
    keyFingerprint: fixture.identity.publicKeyFingerprint,
    disposition: "applied",
  };
}

function readinessStatus(required: number, reconciled: number): string {
  return required === reconciled ? "reconciling" : "suspended";
}

function instanceResponse(fixture: Fixture) {
  return {
    volumeId: fixture.identity.volumeId,
    enrollmentEpoch: 1,
    processInstanceId: fixture.processInstanceId,
    runtimeGrantId: RUNTIME_GRANT_ID,
    runtimeSha256: PROCESS_RUNTIME_SHA256,
    leaseExpiresAtMs: Date.now() + 60_000,
    disposition: "applied",
  };
}

function pollResponse(
  commands: readonly RunnerVolumePurgeCommand[],
  ready: boolean,
  requiredTombstoneGeneration: number,
  reconciledTombstoneGeneration: number,
  serverPublicKeyRaw: string,
  acceptedAttestation?: {
    readonly generation: number;
    readonly sha256: string;
  },
  nextCommandCursor: string | null = null,
) {
  return {
    commands,
    nextCommandCursor,
    ready,
    storageAttestationRequired: acceptedAttestation === undefined,
    enrollmentGeneration: 1,
    predecessorAttestationGeneration: acceptedAttestation?.generation ?? 0,
    predecessorAttestationSha256:
      acceptedAttestation?.sha256 ??
      RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256,
    requiredTombstoneGeneration,
    reconciledTombstoneGeneration,
    serverCommandKeys: { "server-key-test": serverPublicKeyRaw },
  };
}

function purgeCommand(
  fixture: Fixture,
  generation: number,
  purgeSubject = SUBJECT,
): RunnerVolumePurgeCommand {
  const unsigned: UnsignedRunnerVolumePurgeCommand = {
    version: 2,
    requestId: `request-${generation}`,
    audience: RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
    commandId: `command-${generation}`,
    targetVolumeId: fixture.identity.volumeId,
    targetKeyFingerprint: fixture.identity.publicKeyFingerprint,
    enrollmentEpoch: 1,
    purgeSubject,
    purgeGeneration: generation,
    storageEvidenceVersion: 2,
    subjectStorageLayoutVersion: 2,
    legacyInventoryAuthorityGeneration: 0,
    legacyInventoryAuthoritySha256:
      ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256,
    issuedAtMs: 1_000,
    minimumRunnerBuildId: RUNNER_BUILD_ID,
    serverKeyId: "server-key-test",
  };
  return {
    ...unsigned,
    signature: signEd25519(
      fixture.serverPrivateKey,
      canonicalRunnerVolumePurgeCommand(unsigned),
    ),
  };
}

async function completeLocalProfilePurge(
  fixture: Fixture,
  purgeSubject: string,
  profileScope: string,
  generation: number,
): Promise<void> {
  const locator = await fixture.residency.registerProfile(
    purgeSubject,
    profileScope,
  );
  const managed = await fixture.subjectStorage.ensureProfile(locator);
  await managed.active.writeFileExclusive(
    "Cookies",
    Buffer.from("managed-cookie"),
  );
  const artifact = join(fixture.root.path, "active", profileScope, "Cookies");
  await mkdir(join(fixture.root.path, "active", profileScope), {
    recursive: true,
    mode: 0o700,
  });
  await writeFile(artifact, "cookie", { mode: 0o600 });
  const purger = new RunnerVolumePurger(fixture.residency, {
    enrollmentEpoch: 1,
    processInstanceId: fixture.processInstanceId,
    runnerBuildId: RUNNER_BUILD_ID,
    serverCommandKeys: new Map([
      ["server-key-test", fixture.serverPublicKeyRaw],
    ]),
    subjectStorage: fixture.subjectStorage,
    stopAccountWork: async () => ({ status: "stopped" }),
  });
  await purger.execute(purgeCommand(fixture, generation, purgeSubject));
}

async function completeLocalResultPurge(
  fixture: Fixture,
  purgeSubject: string,
  resultScope: string,
  generation: number,
): Promise<void> {
  const locator = await fixture.residency.registerResult(
    purgeSubject,
    resultScope,
  );
  const managed = await fixture.subjectStorage.ensureResult(locator);
  await managed.root.writeFileExclusive(
    "step-result.json.enc",
    Buffer.from("managed-result"),
  );
  const artifact = join(
    fixture.root.path,
    "step-results",
    `${resultScope}.json.enc`,
  );
  await mkdir(join(fixture.root.path, "step-results"), {
    recursive: true,
    mode: 0o700,
  });
  await writeFile(artifact, "result", { mode: 0o600 });
  const purger = new RunnerVolumePurger(fixture.residency, {
    enrollmentEpoch: 1,
    processInstanceId: fixture.processInstanceId,
    runnerBuildId: RUNNER_BUILD_ID,
    serverCommandKeys: new Map([
      ["server-key-test", fixture.serverPublicKeyRaw],
    ]),
    subjectStorage: fixture.subjectStorage,
    stopAccountWork: async () => ({ status: "stopped" }),
  });
  await purger.execute(purgeCommand(fixture, generation, purgeSubject));
}

function profileLocatorPath(
  fixture: Fixture,
  purgeSubject: string,
  profileScope: string,
): string {
  return join(
    fixture.root.path,
    "account-residency-v1",
    "locators",
    accountPurgeSubjectHash(purgeSubject),
    `profile-${profileScope}.json`,
  );
}

function captureCall(
  input: string | URL | Request,
  init?: RequestInit,
): CapturedCall {
  return {
    path: new URL(String(input)).pathname,
    body: JSON.parse(String(init?.body)) as unknown,
    headers: new Headers(init?.headers),
  };
}

function assertAuthorityProof(
  fixture: Fixture,
  call: CapturedCall,
  operation: string,
  extensions: readonly string[],
): void {
  const proof = (call.body as { proof: Record<string, unknown> }).proof;
  expect(Object.keys(proof).sort()).toEqual([
    "audience",
    "enrollmentEpoch",
    "issuedAtMs",
    "operation",
    "payloadSha256",
    "processInstanceId",
    "requestId",
    "signature",
    "version",
    "volumeId",
  ]);
  expect(proof.operation).toBe(operation);
  expect(proof.payloadSha256).toBe(
    runnerVolumeHttpPayloadSha256(call.path, WORKER_ID, extensions),
  );
  const { signature, ...unsigned } = proof;
  expect(
    verifyEd25519(
      fixture.identity.publicKeyRaw,
      canonicalRunnerVolumeAuthorityProof(unsigned as never),
      String(signature),
    ),
  ).toBe(true);
}

function assertFleetHmac(call: CapturedCall): void {
  expect(call.headers.get("x-bluey-jobs-worker-id")).toBe(WORKER_ID);
  expect(call.headers.get("x-bluey-jobs-worker-scope")).toBe("runner-volume");
  expect(call.headers.get("x-bluey-jobs-worker-content-sha256")).toBe(
    createHash("sha256").update(JSON.stringify(call.body)).digest("hex"),
  );
}

function jsonResponse(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

async function waitFor(
  predicate: () => boolean,
  timeoutMs: number,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error("timed out waiting for state");
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

/**
 * Filesystem-backed native capability injected into the production manager.
 * Native retained-handle semantics are covered by native-storage tests; this
 * fixture keeps client integration evidence on the real managed layout.
 */
class FilesystemNativeRootForTest implements NativeRunnerStorageRoot {
  readonly #device: number;
  readonly #inode: number;
  readonly deviceId: string;

  constructor(readonly configuredPath: string) {
    const metadata = testDirectoryStat(configuredPath);
    this.#device = metadata.dev;
    this.#inode = metadata.ino;
    this.deviceId = `unix:${metadata.dev.toString(16)}:runner-client-test`;
  }

  get linkCount(): number {
    return this.assertRootBinding().nlink;
  }

  assertUnchanged(): void {
    this.assertRootBinding();
  }

  async ensureDirectory(
    components: readonly string[],
  ): Promise<NativeRunnerStorageDirectory> {
    return this.openDirectoryInternal(components, true);
  }

  async openDirectory(
    components: readonly string[],
  ): Promise<NativeRunnerStorageDirectory> {
    return this.openDirectoryInternal(components, false);
  }

  async moveEntryNoReplace(
    sourceComponents: readonly string[],
    destinationComponents: readonly string[],
  ): Promise<NativeRunnerMoveOutcome> {
    const source = this.pathFor(sourceComponents);
    const destination = this.pathFor(destinationComponents);
    const sourceMetadata = await testLstat(source);
    if (!sourceMetadata) return "source_missing";
    assertTestEntry(sourceMetadata, this.#device);
    const destinationMetadata = await testLstat(destination);
    if (destinationMetadata) {
      assertTestEntry(destinationMetadata, this.#device);
      return "destination_exists";
    }
    await rename(source, destination);
    this.assertRootBinding();
    return "moved";
  }

  async openDirectoryInternal(
    components: readonly string[],
    create: boolean,
  ): Promise<FilesystemNativeDirectoryForTest> {
    let current = this.configuredPath;
    for (const component of components) {
      assertTestComponent(component);
      current = join(current, component);
      let metadata = await testLstat(current);
      if (!metadata && create) {
        await mkdir(current, { mode: 0o700 });
        metadata = await lstat(current);
      }
      if (!metadata) throw new Error("test native directory is absent");
      assertTestDirectory(metadata, this.#device);
    }
    const metadata = testDirectoryStat(current, this.#device);
    this.assertRootBinding();
    return new FilesystemNativeDirectoryForTest(
      this,
      [...components],
      metadata.ino,
    );
  }

  assertDirectoryBinding(components: readonly string[], inode: number): Stats {
    const metadata = testDirectoryStat(this.pathFor(components), this.#device);
    if (metadata.ino !== inode)
      throw new Error("test native directory changed");
    this.assertRootBinding();
    return metadata;
  }

  pathFor(components: readonly string[]): string {
    for (const component of components) assertTestComponent(component);
    return join(this.configuredPath, ...components);
  }

  get device(): number {
    return this.#device;
  }

  private assertRootBinding(): Stats {
    const metadata = testDirectoryStat(this.configuredPath);
    if (
      metadata.dev !== this.#device ||
      metadata.ino !== this.#inode ||
      realpathSync(this.configuredPath) !== this.configuredPath
    ) {
      throw new Error("test native root changed");
    }
    return metadata;
  }
}

class FilesystemNativeDirectoryForTest implements NativeRunnerStorageDirectory {
  readonly relativePath: string;
  readonly canonicalPath: string;

  constructor(
    private readonly root: FilesystemNativeRootForTest,
    private readonly components: readonly string[],
    private readonly inode: number,
  ) {
    this.relativePath = components.join("/");
    this.canonicalPath = root.pathFor(components);
  }

  get deviceId(): string {
    return this.root.deviceId;
  }

  get linkCount(): number {
    return this.assertBinding().nlink;
  }

  ensureChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.root.ensureDirectory([...this.components, name]);
  }

  openChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.root.openDirectory([...this.components, name]);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const existing = await testLstat(path);
    if (existing) {
      assertTestFile(existing, this.root.device);
      return false;
    }
    try {
      await writeFile(path, contents, { flag: "wx", mode: 0o600 });
    } catch (error) {
      if (!hasFsCode(error, "EEXIST")) throw error;
      assertTestFile(await lstat(path), this.root.device);
      return false;
    }
    assertTestFile(await lstat(path), this.root.device);
    this.assertBinding();
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const existing = await testLstat(path);
    if (existing) assertTestFile(existing, this.root.device);
    await writeFile(path, contents, { mode: 0o600 });
    assertTestFile(await lstat(path), this.root.device);
    this.assertBinding();
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const before = await lstat(path);
    assertTestFile(before, this.root.device);
    if (before.size > maximumBytes)
      throw new Error("test native read exceeds limit");
    const contents = await readFile(path);
    const after = await lstat(path);
    assertTestFile(after, this.root.device);
    if (
      before.ino !== after.ino ||
      before.size !== after.size ||
      contents.byteLength !== after.size
    ) {
      throw new Error("test native file changed during read");
    }
    this.assertBinding();
    return contents;
  }

  async inventory(): Promise<NativeRunnerInventory> {
    this.assertBinding();
    const entries = await collectTestInventory(
      this.canonicalPath,
      this.root.device,
      this.root.deviceId,
    );
    this.assertBinding();
    const bytes = entries.reduce((total, entry) => total + entry.sizeBytes, 0);
    const digest = createHash("sha256");
    digest.update("bluey-jobs-runner-native-inventory-v1\0", "utf8");
    for (const entry of entries) {
      digest.update(
        `${Buffer.byteLength(entry.relativePath, "utf8")}:`,
        "utf8",
      );
      digest.update(entry.relativePath, "utf8");
      digest.update("\n", "utf8");
      digest.update(`${entry.kind}\n`, "utf8");
      digest.update(`${entry.sizeBytes}\n`, "utf8");
      digest.update(`${entry.sha256}\n`, "utf8");
    }
    return Object.freeze({
      entries: Object.freeze(entries),
      count: entries.length,
      bytes,
      sha256: digest.digest("hex"),
    });
  }

  async removeEntry(name: string): Promise<void> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const metadata = await testLstat(path);
    if (!metadata) return;
    assertTestEntry(metadata, this.root.device);
    await rm(path, { recursive: metadata.isDirectory() });
    this.assertBinding();
  }

  private assertBinding(): Stats {
    return this.root.assertDirectoryBinding(this.components, this.inode);
  }
}

async function collectTestInventory(
  root: string,
  device: number,
  deviceId: string,
): Promise<NativeRunnerInventoryEntry[]> {
  const entries: NativeRunnerInventoryEntry[] = [];
  const collect = async (directory: string, prefix: string): Promise<void> => {
    const names = await readdir(directory);
    names.sort(compareTestUtf8);
    for (const name of names) {
      assertTestComponent(name);
      const relativePath = prefix ? `${prefix}/${name}` : name;
      const path = join(directory, name);
      const before = await lstat(path);
      if (before.isDirectory()) {
        assertTestDirectory(before, device);
        entries.push(
          Object.freeze({
            relativePath,
            kind: "directory",
            deviceId,
            linkCount: before.nlink,
            sizeBytes: 0,
            sha256: createHash("sha256")
              .update("bluey-jobs-runner-native-inventory-directory-v1", "utf8")
              .digest("hex"),
          }),
        );
        await collect(path, relativePath);
        continue;
      }
      assertTestFile(before, device);
      const contents = await readFile(path);
      const after = await lstat(path);
      assertTestFile(after, device);
      if (
        before.ino !== after.ino ||
        before.size !== after.size ||
        contents.byteLength !== after.size
      ) {
        throw new Error("test native file changed during inventory");
      }
      entries.push(
        Object.freeze({
          relativePath,
          kind: "file",
          deviceId,
          linkCount: before.nlink,
          sizeBytes: before.size,
          sha256: createHash("sha256").update(contents).digest("hex"),
        }),
      );
    }
  };
  await collect(root, "");
  entries.sort((left, right) =>
    compareTestUtf8(left.relativePath, right.relativePath),
  );
  return entries;
}

function compareTestUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function assertTestComponent(value: string): void {
  if (!/^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,159}$/.test(value)) {
    throw new Error("invalid test native component");
  }
}

function testDirectoryStat(path: string, expectedDevice?: number): Stats {
  const metadata = lstatSync(path);
  assertTestDirectory(metadata, expectedDevice ?? metadata.dev);
  return metadata;
}

async function testLstat(path: string): Promise<Stats | undefined> {
  try {
    return await lstat(path);
  } catch (error) {
    if (hasFsCode(error, "ENOENT")) return undefined;
    throw error;
  }
}

function assertTestEntry(metadata: Stats, device: number): void {
  if (metadata.isDirectory()) assertTestDirectory(metadata, device);
  else assertTestFile(metadata, device);
}

function assertTestDirectory(metadata: Stats, device: number): void {
  if (!metadata.isDirectory() || metadata.dev !== device) {
    throw new Error("unsafe test native directory");
  }
}

function assertTestFile(metadata: Stats, device: number): void {
  if (!metadata.isFile() || metadata.dev !== device || metadata.nlink !== 1) {
    throw new Error("unsafe test native file");
  }
}

function hasFsCode(error: unknown, code: string): boolean {
  return Boolean(
    error &&
    typeof error === "object" &&
    "code" in error &&
    (error as { code?: unknown }).code === code,
  );
}
