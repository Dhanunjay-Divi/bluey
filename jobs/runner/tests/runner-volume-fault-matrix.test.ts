import {
  createHash,
  createHmac,
  generateKeyPairSync,
  timingSafeEqual,
  type KeyObject,
} from "node:crypto";
import {
  closeSync,
  lstatSync,
  openSync,
  realpathSync,
  type Stats,
} from "node:fs";
import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { createJobsWorkerAuthHeaders } from "@bluey/jobs-automation/worker-auth";
import {
  AccountResidencyIndex,
  accountPurgeSubjectHash,
} from "../src/account-residency.js";
import { createInjectedNativeRunnerStorageForTest } from "../src/native-runner-storage.js";
import {
  canonicalRunnerVolumeAuthorityProof,
  canonicalRunnerVolumeEnrollmentProof,
  runnerVolumeHttpPayloadSha256,
  runnerProcessRuntimeSha256,
  RunnerVolumeClient,
  type RunnerVolumeAuthorityProof,
  type RunnerVolumeEnrollmentProof,
  type UnsignedRunnerVolumeAuthorityProof,
} from "../src/runner-volume-client.js";
import {
  openRunnerDataRoot,
  type RunnerDataRoot,
} from "../src/safe-runner-storage.js";
import {
  createRunnerProcessInstanceId,
  decodeCanonicalBase64Url,
  loadOrCreateRunnerVolumeIdentity,
  signEd25519,
  verifyEd25519,
  type RunnerVolumeIdentity,
} from "../src/volume-identity.js";
import {
  canonicalRunnerVolumePurgeAck,
  canonicalRunnerVolumePurgeCommand,
  ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256,
  EMPTY_RUNNER_INVENTORY_SHA256,
  parseRunnerVolumePurgeAck,
  RunnerVolumePurger,
  RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
  type RunnerVolumePurgeAck,
  type RunnerVolumePurgeCommand,
  type RunnerVolumePurgeFaultPhase,
  type UnsignedRunnerVolumePurgeCommand,
} from "../src/volume-purge.js";
import { SubjectStorageManager } from "../src/subject-storage-manager.js";
import {
  parseRunnerVolumeStorageAttestation,
  RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256,
  runnerVolumeStorageAttestationSha256,
} from "../src/storage-attestation.js";

const WORKER_ID = "runner-volume-fault-matrix";
const WORKER_SIGNING_KEY =
  "runner-volume-fault-matrix-worker-key-0123456789abcdef";
const PROVIDER = "local-fault-matrix";
const RUNNER_BUILD_ID = "runner-602.0";
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
const SERVER_KEY_ID = "server-key-fault-matrix";
const ACCOUNT_A_SUBJECT = Buffer.alloc(32, 0xa1).toString("base64url");
const ACCOUNT_B_SUBJECT = Buffer.alloc(32, 0xb2).toString("base64url");
const AUTHORITY_AUDIENCE = "bluey-jobs-runner-volume-authority";
const API_PREFIX = "/api/jobs/internal/runner-volumes";
const MAXIMUM_REQUEST_BYTES = 1024 * 1024;

const temporaryDirectories: string[] = [];
const controlPlanes: LocalRunnerVolumeAuthority[] = [];

afterEach(async () => {
  await Promise.all(
    controlPlanes.splice(0).map((authority) => authority.close()),
  );
  await Promise.all(
    temporaryDirectories
      .splice(0)
      .map((path) => rm(path, { recursive: true, force: true })),
  );
});

describe("Phase 602 three-volume runner purge fault matrix", () => {
  it("keeps offline deletion pending and enforces every out-of-order tombstone", async () => {
    const authority = await LocalRunnerVolumeAuthority.start();
    controlPlanes.push(authority);
    const volumes = await Promise.all([
      createVolumeFixture(authority, 1),
      createVolumeFixture(authority, 2),
      createVolumeFixture(authority, 3),
    ]);

    for (const volume of volumes) {
      const client = createClient(authority, volume);
      await client.start();
      await client.registerProfile(
        ACCOUNT_A_SUBJECT,
        volume.accountAProfileScope,
      );
      await writeProfileArtifact(
        volume,
        volume.accountAProfileScope,
        "account-a",
      );
      await client.registerProfile(
        ACCOUNT_B_SUBJECT,
        volume.accountBProfileScope,
      );
      await writeProfileArtifact(
        volume,
        volume.accountBProfileScope,
        "account-b",
      );
      await captureManagedProfileBackup(volume, volume.accountAProfileScope);
      await captureManagedProfileBackup(volume, volume.accountBProfileScope);
      volume.client = client;
    }

    expect(authority.enrolledVolumeIds()).toEqual(
      volumes
        .map(({ identity }) => identity.volumeId)
        .sort((left, right) => left.localeCompare(right)),
    );
    for (const volume of volumes) {
      expect(authority.residenciesFor(volume.identity.volumeId)).toEqual(
        [ACCOUNT_A_SUBJECT, ACCOUNT_B_SUBJECT].sort((left, right) =>
          left.localeCompare(right),
        ),
      );
    }

    const clone = createClient(authority, volumes[0]!, {
      processInstanceId: createRunnerProcessInstanceId(),
    });
    await expect(clone.start()).rejects.toMatchObject({
      operation: "instance_claim",
      code: "request_failed",
      status: 409,
    });
    expect(clone.ready).toBe(false);
    await clone.stop();

    const staleEpochResponse = await postStaleEpochHeartbeat(
      authority,
      volumes[0]!,
    );
    expect(staleEpochResponse.status).toBe(401);

    for (const volume of volumes) await volume.client?.stop();

    const lower = authority.preparePurge(
      "request-account-a-lower",
      ACCOUNT_A_SUBJECT,
    );
    expect(lower.purgeGeneration).toBe(1);
    await drainPendingCommands(authority, volumes[0]!);
    await drainPendingCommands(authority, volumes[2]!);

    expect(authority.requestStatus(lower.requestId)).toEqual({
      acknowledgementCount: 2,
      completionGeneration: null,
      outstandingTargetCount: 1,
      state: "pending",
    });
    expect(authority.completionOrder()).toEqual([]);
    await expect(
      readFile(
        profileArtifactPath(volumes[1]!, volumes[1]!.accountAProfileScope),
      ),
    ).resolves.toBeDefined();
    for (const volume of volumes) {
      await expect(
        readFile(profileArtifactPath(volume, volume.accountBProfileScope)),
      ).resolves.toBeDefined();
    }

    const higher = authority.preparePurge(
      "request-account-b-higher",
      ACCOUNT_B_SUBJECT,
    );
    expect(higher.purgeGeneration).toBe(2);

    const staleCommand = resignCommand(authority, {
      ...commandFor(higher, volumes[0]!),
      enrollmentEpoch: 2,
    });
    await expect(
      createPurger(authority, volumes[0]!).execute(staleCommand),
    ).rejects.toMatchObject({ code: "wrong_epoch" });

    await drainPendingCommands(authority, volumes[0]!);
    await drainPendingCommands(authority, volumes[2]!);

    const offlineVolume = volumes[1]!;
    const higherAck = await createPurger(authority, offlineVolume).execute(
      commandFor(higher, offlineVolume),
    );
    const higherApplied = await postAck(authority, higherAck);
    expect(higherApplied.status).toBe(200);
    expect(await higherApplied.json()).toMatchObject({
      disposition: "applied",
    });

    expect(authority.requestStatus(higher.requestId)).toEqual({
      acknowledgementCount: 3,
      completionGeneration: 1,
      outstandingTargetCount: 0,
      state: "complete",
    });
    expect(authority.requestStatus(lower.requestId).state).toBe("pending");
    expect(authority.completionOrder()).toEqual([higher.requestId]);
    await expect(
      readFile(
        profileArtifactPath(offlineVolume, offlineVolume.accountAProfileScope),
      ),
    ).resolves.toBeDefined();

    const firstHigherAck = authority.acknowledgementFor(
      higher.requestId,
      volumes[0]!.identity.volumeId,
    );
    const replayedHigherAck = await createPurger(
      authority,
      volumes[0]!,
    ).execute(commandFor(higher, volumes[0]!));
    expect(replayedHigherAck).toEqual(firstHigherAck);
    const replayResponse = await postAck(authority, replayedHigherAck);
    expect(replayResponse.status).toBe(200);
    expect(await replayResponse.json()).toMatchObject({
      disposition: "replay",
    });
    expect(authority.requestStatus(higher.requestId).completionGeneration).toBe(
      1,
    );

    let injected = true;
    let stopCalls = 0;
    const crashingPurger = createPurger(authority, offlineVolume, {
      stopAccountWork: async () => {
        stopCalls += 1;
        return { status: "stopped" };
      },
      faultInjector: async (phase) => {
        if (injected && phase === "after_delete_target") {
          injected = false;
          throw new Error("simulated-runner-crash-after-delete");
        }
      },
    });
    const offlineLowerCommand = commandFor(lower, offlineVolume);
    await expect(crashingPurger.execute(offlineLowerCommand)).rejects.toThrow(
      "simulated-runner-crash-after-delete",
    );
    expect(authority.requestStatus(lower.requestId).state).toBe("pending");

    const resumedAck = await createPurger(authority, offlineVolume, {
      stopAccountWork: async () => {
        stopCalls += 1;
        return { status: "stopped" };
      },
    }).execute(offlineLowerCommand);
    expect(stopCalls).toBe(1);
    expect(resumedAck.version).toBe(2);
    expect(resumedAck.beforeInventoryCount).toBeGreaterThan(0);
    expect(resumedAck.afterInventoryCount).toBe(0);
    expect(resumedAck.afterInventorySha256).toBe(EMPTY_RUNNER_INVENTORY_SHA256);
    expect(resumedAck.storageEvidence.subjectStorage.after).toMatchObject({
      residency: "never_resident",
      subjectTree: { entryCount: 0, fileBytes: "0" },
      ownership: { entryCount: 0, fileBytes: "0" },
      target: { entryCount: 0, fileBytes: "0" },
    });
    expect(resumedAck.storageEvidence.legacy.rootAfter).toMatchObject({
      artifactCount: 0,
      artifactBytes: "0",
      unclassifiedRootCount: 0,
    });
    expect(parseRunnerVolumePurgeAck(resumedAck)).toEqual(resumedAck);
    const resumedResponse = await postAck(authority, resumedAck);
    expect(resumedResponse.status).toBe(200);
    expect(await resumedResponse.json()).toMatchObject({
      disposition: "applied",
    });

    expect(authority.requestStatus(lower.requestId)).toEqual({
      acknowledgementCount: 3,
      completionGeneration: 2,
      outstandingTargetCount: 0,
      state: "complete",
    });
    expect(authority.completionOrder()).toEqual([
      higher.requestId,
      lower.requestId,
    ]);

    for (const volume of volumes) {
      expect(authority.volumeCursors(volume.identity.volumeId)).toEqual({
        reconciledTombstoneGeneration: 2,
        requiredTombstoneGeneration: 2,
      });
      await expect(
        readFile(
          volume.residency.tombstonePath(
            accountPurgeSubjectHash(ACCOUNT_A_SUBJECT),
          ),
        ),
      ).resolves.toBeDefined();
      await expect(
        readFile(
          volume.residency.tombstonePath(
            accountPurgeSubjectHash(ACCOUNT_B_SUBJECT),
          ),
        ),
      ).resolves.toBeDefined();
    }

    const restoredVolume = volumes[2]!;
    await restoreManagedProfileBackup(
      restoredVolume,
      restoredVolume.accountAProfileScope,
    );
    await restoreManagedProfileBackup(
      restoredVolume,
      restoredVolume.accountBProfileScope,
    );
    await writeLegacyProfileArtifact(
      restoredVolume,
      restoredVolume.accountAProfileScope,
      "restored-legacy-a",
    );
    await writeLegacyProfileArtifact(
      restoredVolume,
      restoredVolume.accountBProfileScope,
      "restored-legacy-b",
    );
    await expect(
      readFile(
        profileArtifactPath(
          restoredVolume,
          restoredVolume.accountAProfileScope,
        ),
      ),
    ).resolves.toBeDefined();
    await expect(
      readFile(
        legacyProfileArtifactPath(
          restoredVolume,
          restoredVolume.accountBProfileScope,
        ),
      ),
    ).resolves.toBeDefined();
    const acknowledgementPostsBeforeRestart =
      authority.acknowledgementPostCount();

    for (const volume of volumes) {
      const restart = createClient(authority, volume);
      await restart.start({ deferControlLoop: true });
      await restart.activateControlLoop();
      expect(restart.ready).toBe(true);
      await expect(
        readFile(profileArtifactPath(volume, volume.accountAProfileScope)),
      ).rejects.toMatchObject({ code: "ENOENT" });
      await expect(
        readFile(profileArtifactPath(volume, volume.accountBProfileScope)),
      ).rejects.toMatchObject({ code: "ENOENT" });
      await expect(
        readFile(
          legacyProfileArtifactPath(volume, volume.accountAProfileScope),
        ),
      ).rejects.toMatchObject({ code: "ENOENT" });
      await expect(
        readFile(
          legacyProfileArtifactPath(volume, volume.accountBProfileScope),
        ),
      ).rejects.toMatchObject({ code: "ENOENT" });
      await restart.stop();
    }
    expect(authority.acknowledgementPostCount()).toBe(
      acknowledgementPostsBeforeRestart,
    );

    const blocked = createClient(authority, restoredVolume);
    await blocked.start();
    await expect(
      blocked.registerProfile(ACCOUNT_A_SUBJECT, "f".repeat(40)),
    ).rejects.toMatchObject({ code: "purged_subject" });
    await blocked.stop();
  }, 30_000);
});

interface VolumeFixture {
  readonly root: RunnerDataRoot;
  readonly identity: RunnerVolumeIdentity;
  readonly residency: AccountResidencyIndex;
  readonly subjectStorage: SubjectStorageManager;
  readonly backupRoot: string;
  readonly processInstanceId: string;
  readonly grantId: string;
  readonly grantToken: string;
  readonly runtimeGrantId: string;
  readonly runtimeGrantToken: string;
  readonly providerResourceId: string;
  readonly resourceFingerprint: string;
  readonly accountAProfileScope: string;
  readonly accountBProfileScope: string;
  client?: RunnerVolumeClient;
}

interface PreparedPurge {
  readonly requestId: string;
  readonly purgeGeneration: number;
  readonly commands: ReadonlyMap<string, RunnerVolumePurgeCommand>;
}

interface RequestStatus {
  readonly acknowledgementCount: number;
  readonly completionGeneration: number | null;
  readonly outstandingTargetCount: number;
  readonly state: "complete" | "pending";
}

interface StoredGrant {
  readonly grantId: string;
  readonly token: string;
  readonly providerResourceId: string;
  readonly resourceFingerprint: string;
}

interface StoredVolume {
  readonly proof: RunnerVolumeEnrollmentProof;
  readonly enrolledAtMs: number;
  readonly enrollmentGeneration: number;
  readonly residencies: Set<string>;
  activeInstanceId: string | null;
  instanceLeaseExpiresAtMs: number | null;
  requiredTombstoneGeneration: number;
  reconciledTombstoneGeneration: number;
  storageAttestationRequired: boolean;
  attestationGeneration: number;
  attestationSha256: string;
  runtimeGrantId: string | null;
  runtimeSha256: string | null;
}

interface StoredPurge {
  readonly requestId: string;
  readonly purgeSubject: string;
  readonly purgeGeneration: number;
  readonly commands: ReadonlyMap<string, RunnerVolumePurgeCommand>;
  readonly acknowledgements: Map<string, RunnerVolumePurgeAck>;
  completionGeneration: number | null;
}

interface PurgerOverrides {
  readonly stopAccountWork?: () => Promise<{ readonly status: "stopped" }>;
  readonly faultInjector?: (
    phase: RunnerVolumePurgeFaultPhase,
  ) => Promise<void>;
}

class HttpFailure extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Runner-side authority harness for the exact production HTTP and signature contracts.
 * Rust DB fan-out/cursor correctness remains the server suite's responsibility; this
 * fixture supplies only the immutable commands and verified status needed to exercise
 * three real runner roots without reaching into RunnerVolumeClient private state.
 */
class LocalRunnerVolumeAuthority {
  readonly #server: Server;
  readonly #signingKey: KeyObject;
  readonly #publicKeyRaw: string;
  readonly #grants = new Map<string, StoredGrant>();
  readonly #volumes = new Map<string, StoredVolume>();
  readonly #purges = new Map<string, StoredPurge>();
  readonly #seenWorkerNonces = new Set<string>();
  readonly #completedRequestIds: string[] = [];
  #origin = "";
  #purgeGeneration = 0;
  #completionGeneration = 0;
  #enrollmentGeneration = 0;
  #acknowledgementPosts = 0;

  private constructor(
    server: Server,
    signingKey: KeyObject,
    publicKeyRaw: string,
  ) {
    this.#server = server;
    this.#signingKey = signingKey;
    this.#publicKeyRaw = publicKeyRaw;
  }

  static async start(): Promise<LocalRunnerVolumeAuthority> {
    const keyPair = generateKeyPairSync("ed25519");
    const publicJwk = keyPair.publicKey.export({ format: "jwk" });
    if (!publicJwk.x)
      throw new Error("fault matrix server key is missing its public bytes");
    let authority: LocalRunnerVolumeAuthority;
    const server = createServer((request, response) => {
      void authority.handle(request, response);
    });
    authority = new LocalRunnerVolumeAuthority(
      server,
      keyPair.privateKey,
      publicJwk.x,
    );
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    const address = server.address();
    if (!address || typeof address === "string")
      throw new Error("fault matrix server did not bind");
    authority.#origin = `http://127.0.0.1:${address.port}`;
    return authority;
  }

  get origin(): string {
    return this.#origin;
  }

  get serverPublicKeyRaw(): string {
    return this.#publicKeyRaw;
  }

  createGrant(index: number): StoredGrant {
    const grant: StoredGrant = {
      grantId: `fault-matrix-grant-${index}`,
      token: Buffer.alloc(32, index).toString("base64url"),
      providerResourceId: `local-volume-${index}`,
      resourceFingerprint: createHash("sha256")
        .update(`local-volume-${index}`, "utf8")
        .digest("hex"),
    };
    this.#grants.set(grant.grantId, grant);
    return grant;
  }

  preparePurge(requestId: string, purgeSubject: string): PreparedPurge {
    if (this.#purges.has(requestId))
      throw new Error("duplicate fixture purge request");
    decodeCanonicalBase64Url(purgeSubject, 32);
    this.#purgeGeneration += 1;
    const commands = new Map<string, RunnerVolumePurgeCommand>();
    for (const [volumeId, volume] of [...this.#volumes.entries()].sort(
      ([left], [right]) => left.localeCompare(right),
    )) {
      const unsigned: UnsignedRunnerVolumePurgeCommand = {
        version: 2,
        requestId,
        audience: RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
        commandId: `${requestId}-${volume.enrollmentGeneration}`,
        targetVolumeId: volumeId,
        targetKeyFingerprint: volume.proof.keyFingerprint,
        enrollmentEpoch: 1,
        purgeSubject,
        purgeGeneration: this.#purgeGeneration,
        storageEvidenceVersion: 2,
        subjectStorageLayoutVersion: 2,
        legacyInventoryAuthorityGeneration: 0,
        legacyInventoryAuthoritySha256:
          ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256,
        issuedAtMs: Date.now(),
        minimumRunnerBuildId: RUNNER_BUILD_ID,
        serverKeyId: SERVER_KEY_ID,
      };
      commands.set(volumeId, {
        ...unsigned,
        signature: signEd25519(
          this.#signingKey,
          canonicalRunnerVolumePurgeCommand(unsigned),
        ),
      });
    }
    const purge: StoredPurge = {
      requestId,
      purgeSubject,
      purgeGeneration: this.#purgeGeneration,
      commands,
      acknowledgements: new Map(),
      completionGeneration: null,
    };
    this.#purges.set(requestId, purge);
    return { requestId, purgeGeneration: purge.purgeGeneration, commands };
  }

  requestStatus(requestId: string): RequestStatus {
    const purge = this.requirePurge(requestId);
    return {
      acknowledgementCount: purge.acknowledgements.size,
      completionGeneration: purge.completionGeneration,
      outstandingTargetCount: purge.commands.size - purge.acknowledgements.size,
      state: purge.completionGeneration === null ? "pending" : "complete",
    };
  }

  acknowledgementFor(
    requestId: string,
    volumeId: string,
  ): RunnerVolumePurgeAck {
    const acknowledgement =
      this.requirePurge(requestId).acknowledgements.get(volumeId);
    if (!acknowledgement)
      throw new Error("fault matrix acknowledgement was not recorded");
    return acknowledgement;
  }

  completionOrder(): readonly string[] {
    return [...this.#completedRequestIds];
  }

  enrolledVolumeIds(): readonly string[] {
    return [...this.#volumes.keys()].sort((left, right) =>
      left.localeCompare(right),
    );
  }

  residenciesFor(volumeId: string): readonly string[] {
    return [...this.requireVolume(volumeId).residencies].sort((left, right) =>
      left.localeCompare(right),
    );
  }

  volumeCursors(volumeId: string): {
    readonly requiredTombstoneGeneration: number;
    readonly reconciledTombstoneGeneration: number;
  } {
    const volume = this.requireVolume(volumeId);
    return {
      requiredTombstoneGeneration: volume.requiredTombstoneGeneration,
      reconciledTombstoneGeneration: volume.reconciledTombstoneGeneration,
    };
  }

  acknowledgementPostCount(): number {
    return this.#acknowledgementPosts;
  }

  signCommand(
    command: UnsignedRunnerVolumePurgeCommand,
  ): RunnerVolumePurgeCommand {
    return {
      ...command,
      signature: signEd25519(
        this.#signingKey,
        canonicalRunnerVolumePurgeCommand(command),
      ),
    };
  }

  async close(): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      this.#server.close((error) => (error ? reject(error) : resolve()));
    });
  }

  private async handle(
    request: IncomingMessage,
    response: ServerResponse,
  ): Promise<void> {
    try {
      if (request.method !== "POST" || !request.url)
        throw new HttpFailure(404, "not found");
      const path = new URL(request.url, this.#origin).pathname;
      const bodyText = await readRequestBody(request);
      this.verifyWorkerAuth(request, path, bodyText);
      const body: unknown = JSON.parse(bodyText);
      const result = this.route(path, body);
      writeJson(response, 200, result);
    } catch (error) {
      const failure =
        error instanceof HttpFailure
          ? error
          : new HttpFailure(400, "invalid runner-volume request");
      writeJson(response, failure.status, { error: failure.message });
    }
  }

  private route(path: string, body: unknown): unknown {
    if (path === `${API_PREFIX}/enroll`) return this.enroll(body);
    const instance = new RegExp(
      `^${API_PREFIX}/([^/]+)/instances/(claim|heartbeat)$`,
    ).exec(path);
    if (instance)
      return this.instance(
        decodeURIComponent(instance[1]!),
        instance[2]!,
        path,
        body,
      );
    const residency = new RegExp(`^${API_PREFIX}/([^/]+)/residencies$`).exec(
      path,
    );
    if (residency)
      return this.recordResidency(
        decodeURIComponent(residency[1]!),
        path,
        body,
      );
    const poll = new RegExp(`^${API_PREFIX}/([^/]+)/commands/poll$`).exec(path);
    if (poll) return this.poll(decodeURIComponent(poll[1]!), path, body);
    const attestation = new RegExp(
      `^${API_PREFIX}/([^/]+)/storage-attestations$`,
    ).exec(path);
    if (attestation) {
      return this.storageAttestation(
        decodeURIComponent(attestation[1]!),
        path,
        body,
      );
    }
    const acknowledgement = new RegExp(
      `^${API_PREFIX}/([^/]+)/commands/([^/]+)/ack$`,
    ).exec(path);
    if (acknowledgement) {
      return this.acknowledge(
        decodeURIComponent(acknowledgement[1]!),
        decodeURIComponent(acknowledgement[2]!),
        body,
      );
    }
    throw new HttpFailure(404, "not found");
  }

  private enroll(input: unknown): unknown {
    const request = requireRecord(input);
    if (typeof request.grantToken !== "string")
      throw new HttpFailure(400, "invalid grant");
    const proof = parseEnrollmentProof(request.proof);
    const grant = this.#grants.get(proof.admissionGrantId);
    if (
      !grant ||
      request.grantToken !== grant.token ||
      proof.workerId !== WORKER_ID ||
      proof.provider !== PROVIDER ||
      proof.providerResourceId !== grant.providerResourceId ||
      proof.resourceFingerprint !== grant.resourceFingerprint ||
      proof.enrollmentEpoch !== 1 ||
      proof.legacyArtifactCount !== 0
    ) {
      throw new HttpFailure(401, "invalid admission authority");
    }
    const publicKeyBytes = decodeCanonicalBase64Url(
      proof.publicKeyBase64url,
      32,
    );
    const expectedVolumeId = createHash("sha256")
      .update("bluey-jobs-runner\0volume-id-v1\0", "utf8")
      .update(publicKeyBytes)
      .digest("base64url");
    const expectedFingerprint = createHash("sha256")
      .update(publicKeyBytes)
      .digest("hex");
    const { signature, ...unsigned } = proof;
    if (
      proof.volumeId !== expectedVolumeId ||
      proof.keyFingerprint !== expectedFingerprint ||
      !verifyEd25519(
        proof.publicKeyBase64url,
        canonicalRunnerVolumeEnrollmentProof(unsigned),
        signature,
      )
    ) {
      throw new HttpFailure(401, "invalid enrollment signature");
    }
    let stored = this.#volumes.get(proof.volumeId);
    let disposition: "applied" | "replay" = "replay";
    if (!stored) {
      this.#enrollmentGeneration += 1;
      stored = {
        proof,
        enrolledAtMs: Date.now(),
        enrollmentGeneration: this.#enrollmentGeneration,
        residencies: new Set(),
        activeInstanceId: null,
        instanceLeaseExpiresAtMs: null,
        requiredTombstoneGeneration: this.#completionGeneration,
        reconciledTombstoneGeneration: this.#completionGeneration,
        storageAttestationRequired: true,
        attestationGeneration: 0,
        attestationSha256:
          RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256,
        runtimeGrantId: null,
        runtimeSha256: null,
      };
      this.#volumes.set(proof.volumeId, stored);
      disposition = "applied";
    } else if (JSON.stringify(stored.proof) !== JSON.stringify(proof)) {
      throw new HttpFailure(409, "conflicting enrollment replay");
    }
    return this.enrollmentResponse(stored, disposition);
  }

  private instance(
    volumeId: string,
    action: string,
    path: string,
    input: unknown,
  ): unknown {
    const request = requireRecord(input);
    let runtimeGrantId: string | null = null;
    let runtimeSha256: string | null = null;
    const payloadExtensions: string[] = [];
    if (action === "claim") {
      const runtimeGrant = requireRecord(request.runtimeGrant);
      if (
        typeof runtimeGrant.grantId !== "string" ||
        typeof runtimeGrant.grantToken !== "string" ||
        JSON.stringify(runtimeGrant.runtime) !== JSON.stringify(PROCESS_RUNTIME)
      ) {
        throw new HttpFailure(401, "invalid process runtime grant");
      }
      runtimeGrantId = runtimeGrant.grantId;
      runtimeSha256 = PROCESS_RUNTIME_SHA256;
      payloadExtensions.push(
        `runtime_grant_id=${runtimeGrantId}`,
        `runtime_grant_token_sha256=${createHash("sha256")
          .update(runtimeGrant.grantToken, "utf8")
          .digest("hex")}`,
        `runtime_sha256=${runtimeSha256}`,
      );
    }
    const proof = this.verifyAuthorityProof(
      volumeId,
      request.proof,
      action === "claim" ? "instance_claim" : "instance_heartbeat",
      runnerVolumeHttpPayloadSha256(path, WORKER_ID, payloadExtensions),
    );
    const volume = this.requireVolume(volumeId);
    const nowMs = Date.now();
    if (
      action === "heartbeat" &&
      volume.activeInstanceId !== proof.processInstanceId
    ) {
      throw new HttpFailure(409, "stale process instance");
    }
    if (
      action === "claim" &&
      volume.activeInstanceId &&
      volume.activeInstanceId !== proof.processInstanceId &&
      (volume.instanceLeaseExpiresAtMs ?? 0) > nowMs
    ) {
      throw new HttpFailure(409, "concurrent volume clone");
    }
    const disposition =
      volume.activeInstanceId === proof.processInstanceId
        ? "replay"
        : "applied";
    volume.activeInstanceId = proof.processInstanceId;
    volume.instanceLeaseExpiresAtMs = nowMs + 300_000;
    if (action === "claim") {
      if (
        volume.runtimeGrantId !== null &&
        (volume.runtimeGrantId !== runtimeGrantId ||
          volume.runtimeSha256 !== runtimeSha256)
      ) {
        throw new HttpFailure(409, "runtime grant mutation");
      }
      volume.runtimeGrantId = runtimeGrantId;
      volume.runtimeSha256 = runtimeSha256;
    }
    return {
      volumeId,
      enrollmentEpoch: 1,
      processInstanceId: proof.processInstanceId,
      runtimeGrantId: volume.runtimeGrantId,
      runtimeSha256: volume.runtimeSha256,
      leaseExpiresAtMs: volume.instanceLeaseExpiresAtMs,
      disposition,
    };
  }

  private recordResidency(
    volumeId: string,
    path: string,
    input: unknown,
  ): unknown {
    const request = requireRecord(input);
    if (typeof request.purgeSubject !== "string") {
      throw new HttpFailure(400, "invalid purge subject");
    }
    decodeCanonicalBase64Url(request.purgeSubject, 32);
    const proof = this.verifyAuthorityProof(
      volumeId,
      request.proof,
      "residency_bind",
      runnerVolumeHttpPayloadSha256(path, WORKER_ID, [
        `purge_subject=${request.purgeSubject}`,
      ]),
    );
    this.requireCurrentProcess(volumeId, proof.processInstanceId);
    this.requireVolume(volumeId).residencies.add(request.purgeSubject);
    return null;
  }

  private poll(volumeId: string, path: string, input: unknown): unknown {
    const request = requireRecord(input);
    if (!Number.isSafeInteger(request.limit) || Number(request.limit) < 1) {
      throw new HttpFailure(400, "invalid poll limit");
    }
    if (
      request.afterCommandId !== null &&
      typeof request.afterCommandId !== "string"
    ) {
      throw new HttpFailure(400, "invalid poll cursor");
    }
    const proof = this.verifyAuthorityProof(
      volumeId,
      request.proof,
      "purge_poll",
      runnerVolumeHttpPayloadSha256(path, WORKER_ID, [
        `after_command_id=${request.afterCommandId ?? ""}`,
        `limit=${String(request.limit)}`,
      ]),
    );
    this.requireCurrentProcess(volumeId, proof.processInstanceId);
    const limit = Math.min(Number(request.limit), 64);
    const pending = [...this.#purges.values()]
      .filter(
        (purge) =>
          purge.commands.has(volumeId) && !purge.acknowledgements.has(volumeId),
      )
      .sort((left, right) => left.purgeGeneration - right.purgeGeneration)
      .map((purge) => purge.commands.get(volumeId)!);
    const cursorIndex =
      request.afterCommandId === null
        ? -1
        : pending.findIndex(
            (command) => command.commandId === request.afterCommandId,
          );
    if (request.afterCommandId !== null && cursorIndex < 0) {
      throw new HttpFailure(400, "unknown poll cursor");
    }
    const page = pending.slice(cursorIndex + 1, cursorIndex + 2 + limit);
    const hasMore = page.length > limit;
    const commands = page.slice(0, limit);
    const nextCommandCursor = hasMore
      ? commands.at(-1)!.commandId
      : null;
    const volume = this.requireVolume(volumeId);
    return {
      commands,
      nextCommandCursor,
      ready:
        request.afterCommandId === null &&
        commands.length === 0 &&
        !volume.storageAttestationRequired &&
        volume.requiredTombstoneGeneration ===
          volume.reconciledTombstoneGeneration,
      storageAttestationRequired: volume.storageAttestationRequired,
      enrollmentGeneration: volume.enrollmentGeneration,
      predecessorAttestationGeneration: volume.attestationGeneration,
      predecessorAttestationSha256: volume.attestationSha256,
      requiredTombstoneGeneration: volume.requiredTombstoneGeneration,
      reconciledTombstoneGeneration: volume.reconciledTombstoneGeneration,
      serverCommandKeys: { [SERVER_KEY_ID]: this.#publicKeyRaw },
    };
  }

  private storageAttestation(
    volumeId: string,
    path: string,
    input: unknown,
  ): unknown {
    const request = requireRecord(input);
    const volume = this.requireVolume(volumeId);
    const attestation = parseRunnerVolumeStorageAttestation(
      request.attestation,
      volume.proof.publicKeyBase64url,
    );
    const sha256 = runnerVolumeStorageAttestationSha256(attestation);
    const proof = this.verifyAuthorityProof(
      volumeId,
      request.proof,
      "storage_attestation",
      runnerVolumeHttpPayloadSha256(path, WORKER_ID, [
        `attestation_sha256=${sha256}`,
      ]),
    );
    this.requireCurrentProcess(volumeId, proof.processInstanceId);
    if (
      attestation.enrollmentGeneration !== volume.enrollmentGeneration ||
      attestation.processInstanceId !== proof.processInstanceId ||
      attestation.predecessorAttestationGeneration !==
        volume.attestationGeneration ||
      attestation.predecessorAttestationSha256 !== volume.attestationSha256 ||
      attestation.requiredTombstoneGeneration !==
        volume.requiredTombstoneGeneration ||
      attestation.reconciledTombstoneGeneration !==
        volume.reconciledTombstoneGeneration
    ) {
      throw new HttpFailure(409, "conflicting storage attestation");
    }
    volume.attestationGeneration += 1;
    volume.attestationSha256 = sha256;
    volume.storageAttestationRequired = false;
    return {
      disposition: "applied",
      attestationGeneration: volume.attestationGeneration,
      fleetAttestationGeneration: volume.attestationGeneration,
      attestationSha256: sha256,
      volumeStatus: "active",
    };
  }

  private acknowledge(
    volumeId: string,
    commandId: string,
    input: unknown,
  ): unknown {
    this.#acknowledgementPosts += 1;
    const acknowledgement = parsePurgeAck(input);
    const volume = this.requireVolume(volumeId);
    if (
      volume.activeInstanceId !== acknowledgement.processInstanceId ||
      acknowledgement.enrollmentEpoch !== 1
    ) {
      throw new HttpFailure(409, "stale process instance");
    }
    const purge = this.#purges.get(acknowledgement.requestId);
    const command = purge?.commands.get(volumeId);
    if (
      !purge ||
      !command ||
      command.commandId !== commandId ||
      acknowledgement.commandId !== commandId ||
      acknowledgement.targetVolumeId !== volumeId ||
      acknowledgement.targetKeyFingerprint !== volume.proof.keyFingerprint ||
      acknowledgement.purgeGeneration !== purge.purgeGeneration ||
      acknowledgement.purgeSubjectSha256 !==
        accountPurgeSubjectHash(purge.purgeSubject) ||
      acknowledgement.commandSha256 !==
        createHash("sha256")
          .update(canonicalRunnerVolumePurgeCommand(command))
          .digest("hex")
    ) {
      throw new HttpFailure(409, "acknowledgement binding conflict");
    }
    const { signature, ...unsigned } = acknowledgement;
    if (
      !verifyEd25519(
        volume.proof.publicKeyBase64url,
        canonicalRunnerVolumePurgeAck(unsigned),
        signature,
      )
    ) {
      throw new HttpFailure(401, "invalid acknowledgement signature");
    }
    const existing = purge.acknowledgements.get(volumeId);
    let disposition: "applied" | "replay" = "applied";
    if (existing) {
      if (JSON.stringify(existing) !== JSON.stringify(acknowledgement)) {
        throw new HttpFailure(409, "conflicting acknowledgement replay");
      }
      disposition = "replay";
    } else {
      purge.acknowledgements.set(volumeId, acknowledgement);
      volume.storageAttestationRequired = true;
      this.completeIfReady(purge);
    }
    const status = this.requestStatus(purge.requestId);
    return {
      disposition,
      status: {
        purgeGeneration: purge.purgeGeneration,
        state: status.state,
        requiredTargetCount: purge.commands.size,
        acknowledgementCount: status.acknowledgementCount,
        outstandingTargetCount: status.outstandingTargetCount,
      },
      reconciledTombstoneGeneration: volume.reconciledTombstoneGeneration,
    };
  }

  private completeIfReady(purge: StoredPurge): void {
    if (
      purge.completionGeneration !== null ||
      purge.acknowledgements.size !== purge.commands.size
    ) {
      return;
    }
    this.#completionGeneration += 1;
    purge.completionGeneration = this.#completionGeneration;
    this.#completedRequestIds.push(purge.requestId);
    for (const [volumeId, volume] of this.#volumes) {
      volume.requiredTombstoneGeneration = this.#completionGeneration;
      if (purge.acknowledgements.has(volumeId)) {
        volume.reconciledTombstoneGeneration = this.#completionGeneration;
      }
    }
  }

  private verifyAuthorityProof(
    volumeId: string,
    input: unknown,
    expectedOperation: string,
    expectedPayloadSha256: string,
  ): RunnerVolumeAuthorityProof {
    const proof = parseAuthorityProof(input);
    const volume = this.requireVolume(volumeId);
    const { signature, ...unsigned } = proof;
    if (
      proof.volumeId !== volumeId ||
      proof.enrollmentEpoch !== 1 ||
      proof.operation !== expectedOperation ||
      proof.payloadSha256 !== expectedPayloadSha256 ||
      Math.abs(Date.now() - proof.issuedAtMs) > 90_000 ||
      !verifyEd25519(
        volume.proof.publicKeyBase64url,
        canonicalRunnerVolumeAuthorityProof(unsigned),
        signature,
      )
    ) {
      throw new HttpFailure(401, "invalid runner-volume authority proof");
    }
    return proof;
  }

  private verifyWorkerAuth(
    request: IncomingMessage,
    path: string,
    body: string,
  ): void {
    const workerId = singleHeader(request, "x-bluey-jobs-worker-id");
    const timestamp = singleHeader(request, "x-bluey-jobs-worker-timestamp");
    const nonce = singleHeader(request, "x-bluey-jobs-worker-nonce");
    const audience = singleHeader(request, "x-bluey-jobs-worker-audience");
    const scope = singleHeader(request, "x-bluey-jobs-worker-scope");
    const contentSha256 = singleHeader(
      request,
      "x-bluey-jobs-worker-content-sha256",
    );
    const signature = singleHeader(request, "x-bluey-jobs-worker-signature");
    const timestampSeconds = Number(timestamp);
    if (
      workerId !== WORKER_ID ||
      audience !== "bluey-jobs-api" ||
      scope !== "runner-volume" ||
      !Number.isSafeInteger(timestampSeconds) ||
      Math.abs(Math.floor(Date.now() / 1_000) - timestampSeconds) > 300 ||
      this.#seenWorkerNonces.has(nonce) ||
      contentSha256 !== createHash("sha256").update(body).digest("hex")
    ) {
      throw new HttpFailure(401, "invalid worker authentication");
    }
    const canonical = [
      "bluey-jobs-worker-v1",
      timestamp,
      nonce,
      workerId,
      audience,
      scope,
      "POST",
      path,
      contentSha256,
    ].join("\n");
    const expected = createHmac("sha256", WORKER_SIGNING_KEY)
      .update(canonical)
      .digest("hex");
    const suppliedBytes = Buffer.from(signature, "hex");
    const expectedBytes = Buffer.from(expected, "hex");
    if (
      suppliedBytes.length !== expectedBytes.length ||
      !timingSafeEqual(suppliedBytes, expectedBytes)
    ) {
      throw new HttpFailure(401, "invalid worker signature");
    }
    this.#seenWorkerNonces.add(nonce);
  }

  private requireCurrentProcess(
    volumeId: string,
    processInstanceId: string,
  ): void {
    const volume = this.requireVolume(volumeId);
    if (
      volume.activeInstanceId !== processInstanceId ||
      (volume.instanceLeaseExpiresAtMs ?? 0) <= Date.now()
    ) {
      throw new HttpFailure(409, "stale process instance");
    }
  }

  private requireVolume(volumeId: string): StoredVolume {
    const volume = this.#volumes.get(volumeId);
    if (!volume) throw new HttpFailure(404, "runner volume not found");
    return volume;
  }

  private requirePurge(requestId: string): StoredPurge {
    const purge = this.#purges.get(requestId);
    if (!purge) throw new Error("fault matrix purge request was not found");
    return purge;
  }

  private enrollmentResponse(
    volume: StoredVolume,
    disposition: "applied" | "replay",
  ): unknown {
    return {
      volumeId: volume.proof.volumeId,
      workerId: volume.proof.workerId,
      provider: volume.proof.provider,
      providerResourceId: volume.proof.providerResourceId,
      resourceFingerprint: volume.proof.resourceFingerprint,
      currentEpoch: 1,
      enrollmentGeneration: volume.enrollmentGeneration,
      requiredTombstoneGeneration: volume.requiredTombstoneGeneration,
      reconciledTombstoneGeneration: volume.reconciledTombstoneGeneration,
      status: "active",
      activeInstanceId: volume.activeInstanceId,
      instanceLeaseExpiresAtMs: volume.instanceLeaseExpiresAtMs,
      legacyArtifactCount: volume.proof.legacyArtifactCount,
      admissionGrantId: volume.proof.admissionGrantId,
      enrolledAtMs: volume.enrolledAtMs,
      lastSeenAtMs: Date.now(),
      updatedAtMs: Date.now(),
      publicKeyBase64url: volume.proof.publicKeyBase64url,
      keyFingerprint: volume.proof.keyFingerprint,
      disposition,
    };
  }
}

async function createVolumeFixture(
  authority: LocalRunnerVolumeAuthority,
  index: number,
): Promise<VolumeFixture> {
  const parent = await mkdtemp(
    join(tmpdir(), `bluey-runner-volume-matrix-${index}-`),
  );
  temporaryDirectories.push(parent);
  const root = await openRunnerDataRoot(join(parent, "runner"));
  const identity = await loadOrCreateRunnerVolumeIdentity(root);
  const residency = new AccountResidencyIndex(root, identity);
  const nativeStorage = createInjectedNativeRunnerStorageForTest({
    RunnerStorageDirectory: FilesystemNativeDirectoryForTest,
    RunnerStorageRoot: FilesystemNativeRootForTest,
  }).openRoot(root.path);
  const subjectStorage = new SubjectStorageManager(
    nativeStorage,
    identity,
    residency,
  );
  const grant = authority.createGrant(index);
  return {
    root,
    identity,
    residency,
    subjectStorage,
    backupRoot: join(parent, "managed-restore-backup"),
    processInstanceId: createRunnerProcessInstanceId(),
    grantId: grant.grantId,
    grantToken: grant.token,
    runtimeGrantId: `fault-matrix-runtime-${index}`,
    runtimeGrantToken: Buffer.alloc(32, index + 64).toString("base64url"),
    providerResourceId: grant.providerResourceId,
    resourceFingerprint: grant.resourceFingerprint,
    accountAProfileScope: index.toString(16).repeat(40),
    accountBProfileScope: (index + 8).toString(16).repeat(40),
  };
}

function createClient(
  authority: LocalRunnerVolumeAuthority,
  volume: VolumeFixture,
  overrides: { readonly processInstanceId?: string } = {},
): RunnerVolumeClient {
  return new RunnerVolumeClient({
    origin: authority.origin,
    workerSigningKey: WORKER_SIGNING_KEY,
    workerId: WORKER_ID,
    admissionGrantId: volume.grantId,
    admissionGrantToken: volume.grantToken,
    provider: PROVIDER,
    providerResourceId: volume.providerResourceId,
    resourceFingerprint: volume.resourceFingerprint,
    legacyArtifactCount: 0,
    runnerBuildId: RUNNER_BUILD_ID,
    processInstanceId: overrides.processInstanceId ?? volume.processInstanceId,
    processRuntimeGrant: {
      grantId: volume.runtimeGrantId,
      grantToken: volume.runtimeGrantToken,
      runtime: PROCESS_RUNTIME,
    },
    identity: volume.identity,
    residency: volume.residency,
    subjectStorage: volume.subjectStorage,
    serverCommandKeys: new Map([[SERVER_KEY_ID, authority.serverPublicKeyRaw]]),
    stopAccountWork: async () => ({ status: "stopped" }),
    controlIntervalMs: 60_000,
  });
}

function createPurger(
  authority: LocalRunnerVolumeAuthority,
  volume: VolumeFixture,
  overrides: PurgerOverrides = {},
): RunnerVolumePurger {
  return new RunnerVolumePurger(volume.residency, {
    enrollmentEpoch: 1,
    processInstanceId: volume.processInstanceId,
    runnerBuildId: RUNNER_BUILD_ID,
    serverCommandKeys: new Map([[SERVER_KEY_ID, authority.serverPublicKeyRaw]]),
    subjectStorage: volume.subjectStorage,
    stopAccountWork:
      overrides.stopAccountWork ?? (async () => ({ status: "stopped" })),
    faultInjector: overrides.faultInjector,
  });
}

async function drainPendingCommands(
  authority: LocalRunnerVolumeAuthority,
  volume: VolumeFixture,
): Promise<void> {
  const client = createClient(authority, volume);
  await client.start({ deferControlLoop: true });
  await client.activateControlLoop();
  expect(client.ready).toBe(true);
  await client.stop();
}

function commandFor(
  purge: PreparedPurge,
  volume: VolumeFixture,
): RunnerVolumePurgeCommand {
  const command = purge.commands.get(volume.identity.volumeId);
  if (!command)
    throw new Error("fault matrix command was not frozen for the volume");
  return command;
}

function resignCommand(
  authority: LocalRunnerVolumeAuthority,
  command: RunnerVolumePurgeCommand,
): RunnerVolumePurgeCommand {
  const { signature: _signature, ...unsigned } = command;
  return authority.signCommand(unsigned);
}

async function postAck(
  authority: LocalRunnerVolumeAuthority,
  acknowledgement: RunnerVolumePurgeAck,
): Promise<Response> {
  const path =
    `${API_PREFIX}/${encodeURIComponent(acknowledgement.targetVolumeId)}` +
    `/commands/${encodeURIComponent(acknowledgement.commandId)}/ack`;
  const body = JSON.stringify(acknowledgement);
  return fetch(`${authority.origin}${path}`, {
    method: "POST",
    headers: {
      ...createJobsWorkerAuthHeaders({
        signingKey: WORKER_SIGNING_KEY,
        workerId: WORKER_ID,
        method: "POST",
        path,
        body,
      }),
      "Content-Type": "application/json",
    },
    body,
  });
}

async function postStaleEpochHeartbeat(
  authority: LocalRunnerVolumeAuthority,
  volume: VolumeFixture,
): Promise<Response> {
  const path = `${API_PREFIX}/${encodeURIComponent(volume.identity.volumeId)}/instances/heartbeat`;
  const unsigned: UnsignedRunnerVolumeAuthorityProof = {
    version: 1,
    audience: AUTHORITY_AUDIENCE,
    operation: "instance_heartbeat",
    requestId: "stale-epoch-heartbeat",
    volumeId: volume.identity.volumeId,
    enrollmentEpoch: 2,
    processInstanceId: volume.processInstanceId,
    issuedAtMs: Date.now(),
    payloadSha256: runnerVolumeHttpPayloadSha256(path, WORKER_ID, []),
  };
  const body = JSON.stringify({
    proof: {
      ...unsigned,
      signature: signEd25519(
        volume.identity.privateKey,
        canonicalRunnerVolumeAuthorityProof(unsigned),
      ),
    },
  });
  return fetch(`${authority.origin}${path}`, {
    method: "POST",
    headers: {
      ...createJobsWorkerAuthHeaders({
        signingKey: WORKER_SIGNING_KEY,
        workerId: WORKER_ID,
        method: "POST",
        path,
        body,
      }),
      "Content-Type": "application/json",
    },
    body,
  });
}

interface FilesystemNativeInventoryEntryForTest {
  readonly relativePath: string;
  readonly kind: "directory" | "file";
  readonly deviceId: string;
  readonly linkCount: string;
  readonly sizeBytes: string;
  readonly sha256: string;
}

interface FilesystemNativeInventoryForTest {
  readonly entries: readonly FilesystemNativeInventoryEntryForTest[];
  readonly count: number;
  readonly bytes: string;
  readonly sha256: string;
}

class FilesystemNativeRootForTest {
  readonly #device: number;
  readonly #inode: number;
  readonly #deviceId: string;

  constructor(readonly configuredPath: string) {
    const root = testNativeDirectoryStatSync(configuredPath);
    this.#device = root.dev;
    this.#inode = root.ino;
    this.#deviceId = `unix:${root.dev.toString(16)}:mount:test`;
    const lockPath = join(configuredPath, ".bluey-runner-storage.lock");
    try {
      const descriptor = openSync(lockPath, "ax", 0o600);
      closeSync(descriptor);
    } catch (error) {
      if (!hasFsCode(error, "EEXIST")) throw testNativeFailure("io_failure");
    }
    assertTestNativeFile(lstatSync(lockPath), this.#device);
  }

  get deviceId(): string {
    return this.#deviceId;
  }

  get linkCount(): string {
    return String(this.assertRootBinding().nlink);
  }

  assertUnchanged(): void {
    this.assertRootBinding();
  }

  async ensureDirectory(
    components: readonly string[],
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.openDirectoryInternal(components, true);
  }

  async openDirectory(
    components: readonly string[],
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.openDirectoryInternal(components, false);
  }

  async moveEntryNoReplace(
    sourceComponents: readonly string[],
    destinationComponents: readonly string[],
  ): Promise<"destination_exists" | "moved" | "source_missing"> {
    this.assertRootBinding();
    const source = join(this.configuredPath, ...sourceComponents);
    const destination = join(this.configuredPath, ...destinationComponents);
    const sourceStat = await testLstatOrUndefined(source);
    if (!sourceStat) return "source_missing";
    if (sourceStat.isDirectory()) {
      assertTestNativeDirectory(sourceStat, this.#device);
      await collectTestNativeInventory(source, this.#device, this.#deviceId);
    } else {
      assertTestNativeFile(sourceStat, this.#device);
    }
    const destinationStat = await testLstatOrUndefined(destination);
    if (destinationStat) {
      if (destinationStat.isDirectory()) {
        assertTestNativeDirectory(destinationStat, this.#device);
        await collectTestNativeInventory(
          destination,
          this.#device,
          this.#deviceId,
        );
      } else {
        assertTestNativeFile(destinationStat, this.#device);
      }
      return "destination_exists";
    }
    try {
      await rename(source, destination);
    } catch {
      throw testNativeFailure("io_failure");
    }
    this.assertRootBinding();
    return "moved";
  }

  async openDirectoryInternal(
    components: readonly string[],
    create: boolean,
  ): Promise<FilesystemNativeDirectoryForTest> {
    this.assertRootBinding();
    let current = this.configuredPath;
    try {
      for (const component of components) {
        current = join(current, component);
        let metadata = await testLstatOrUndefined(current);
        if (!metadata && create) {
          await mkdir(current, { mode: 0o700 });
          metadata = await lstat(current);
        }
        if (!metadata) throw testNativeFailure("io_failure");
        assertTestNativeDirectory(metadata, this.#device);
      }
    } catch (error) {
      throw translateTestNativeError(error);
    }
    const metadata = testNativeDirectoryStatSync(current, this.#device);
    this.assertRootBinding();
    return new FilesystemNativeDirectoryForTest(this, components, metadata.ino);
  }

  assertDirectoryBinding(components: readonly string[], inode: number): Stats {
    this.assertRootBinding();
    const metadata = testNativeDirectoryStatSync(
      join(this.configuredPath, ...components),
      this.#device,
    );
    if (metadata.ino !== inode) throw testNativeFailure("unsafe_entry");
    return metadata;
  }

  get rootDevice(): number {
    return this.#device;
  }

  private assertRootBinding(): Stats {
    let metadata: Stats;
    try {
      metadata = testNativeDirectoryStatSync(this.configuredPath);
      if (
        metadata.dev !== this.#device ||
        metadata.ino !== this.#inode ||
        realpathSync(this.configuredPath) !== this.configuredPath
      ) {
        throw testNativeFailure("root_changed");
      }
    } catch (error) {
      if (isTestNativeFailure(error)) throw error;
      throw testNativeFailure("root_changed");
    }
    return metadata;
  }
}

class FilesystemNativeDirectoryForTest {
  readonly relativePath: string;
  readonly canonicalPath: string;

  constructor(
    private readonly root: FilesystemNativeRootForTest,
    private readonly components: readonly string[],
    private readonly inode: number,
  ) {
    this.relativePath = components.join("/");
    this.canonicalPath = join(root.configuredPath, ...components);
  }

  get deviceId(): string {
    return this.root.deviceId;
  }

  get linkCount(): string {
    return String(this.assertBinding().nlink);
  }

  async ensureChildDirectory(
    name: string,
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.root.ensureDirectory([...this.components, name]);
  }

  async openChildDirectory(
    name: string,
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.root.openDirectory([...this.components, name]);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const existing = await testLstatOrUndefined(path);
    if (existing) {
      assertTestNativeFile(existing, this.root.rootDevice);
      return false;
    }
    try {
      await writeFile(path, contents, { flag: "wx", mode: 0o600 });
    } catch (error) {
      if (hasFsCode(error, "EEXIST")) {
        const raced = await lstat(path);
        assertTestNativeFile(raced, this.root.rootDevice);
        return false;
      }
      throw testNativeFailure("io_failure");
    }
    assertTestNativeFile(await lstat(path), this.root.rootDevice);
    this.assertBinding();
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const existing = await testLstatOrUndefined(path);
    if (existing) assertTestNativeFile(existing, this.root.rootDevice);
    try {
      await writeFile(path, contents, { mode: 0o600 });
    } catch {
      throw testNativeFailure("io_failure");
    }
    assertTestNativeFile(await lstat(path), this.root.rootDevice);
    this.assertBinding();
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const before = await testLstatOrUndefined(path);
    if (!before) throw testNativeFailure("io_failure");
    assertTestNativeFile(before, this.root.rootDevice);
    if (before.size > maximumBytes) throw testNativeFailure("inventory_limit");
    let contents: Buffer;
    try {
      contents = await readFile(path);
    } catch {
      throw testNativeFailure("io_failure");
    }
    const after = await lstat(path);
    assertTestNativeFile(after, this.root.rootDevice);
    if (
      before.ino !== after.ino ||
      before.size !== after.size ||
      contents.byteLength !== after.size
    ) {
      throw testNativeFailure("unsafe_entry");
    }
    this.assertBinding();
    return contents;
  }

  async inventory(): Promise<FilesystemNativeInventoryForTest> {
    this.assertBinding();
    const entries = await collectTestNativeInventory(
      this.canonicalPath,
      this.root.rootDevice,
      this.root.deviceId,
    );
    this.assertBinding();
    const bytes = entries.reduce(
      (total, entry) => total + Number(entry.sizeBytes),
      0,
    );
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
    return {
      entries,
      count: entries.length,
      bytes: String(bytes),
      sha256: digest.digest("hex"),
    };
  }

  async removeEntry(name: string): Promise<void> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const metadata = await testLstatOrUndefined(path);
    if (!metadata) return;
    if (metadata.isDirectory()) {
      assertTestNativeDirectory(metadata, this.root.rootDevice);
      await collectTestNativeInventory(
        path,
        this.root.rootDevice,
        this.root.deviceId,
      );
    } else {
      assertTestNativeFile(metadata, this.root.rootDevice);
    }
    try {
      await rm(path, { recursive: true });
    } catch {
      throw testNativeFailure("io_failure");
    }
    this.assertBinding();
  }

  private assertBinding(): Stats {
    return this.root.assertDirectoryBinding(this.components, this.inode);
  }
}

async function collectTestNativeInventory(
  directory: string,
  rootDevice: number,
  deviceId: string,
): Promise<FilesystemNativeInventoryEntryForTest[]> {
  const entries: FilesystemNativeInventoryEntryForTest[] = [];
  const collect = async (path: string, prefix: string): Promise<void> => {
    let names: string[];
    try {
      names = await readdir(path);
    } catch {
      throw testNativeFailure("io_failure");
    }
    names.sort(compareTestNativeUtf8);
    for (const name of names) {
      assertTestNativeInventoryName(name);
      const relativePath = prefix ? `${prefix}/${name}` : name;
      const child = join(path, name);
      const before = await lstat(child);
      if (before.isDirectory()) {
        assertTestNativeDirectory(before, rootDevice);
        entries.push({
          relativePath,
          kind: "directory",
          deviceId,
          linkCount: String(before.nlink),
          sizeBytes: "0",
          sha256: createHash("sha256")
            .update("bluey-jobs-runner-native-inventory-directory-v1", "utf8")
            .digest("hex"),
        });
        await collect(child, relativePath);
      } else if (before.isFile()) {
        assertTestNativeFile(before, rootDevice);
        const contents = await readFile(child);
        const after = await lstat(child);
        assertTestNativeFile(after, rootDevice);
        if (
          before.ino !== after.ino ||
          before.size !== after.size ||
          contents.byteLength !== after.size
        ) {
          throw testNativeFailure("unsafe_entry");
        }
        entries.push({
          relativePath,
          kind: "file",
          deviceId,
          linkCount: String(before.nlink),
          sizeBytes: String(before.size),
          sha256: createHash("sha256").update(contents).digest("hex"),
        });
      } else {
        throw testNativeFailure("unsafe_entry");
      }
    }
  };
  await collect(directory, "");
  entries.sort((left, right) =>
    compareTestNativeUtf8(left.relativePath, right.relativePath),
  );
  return entries;
}

function assertTestNativeInventoryName(name: string): void {
  if (
    name.length === 0 ||
    name === "." ||
    name === ".." ||
    Buffer.byteLength(name, "utf8") > 255 ||
    /[\\\u0000-\u001f\u007f]/.test(name)
  ) {
    throw testNativeFailure("unsafe_entry");
  }
}

function assertTestNativeDirectory(metadata: Stats, rootDevice: number): void {
  if (!metadata.isDirectory() || metadata.dev !== rootDevice) {
    throw testNativeFailure("unsafe_entry");
  }
}

function assertTestNativeFile(metadata: Stats, rootDevice: number): void {
  if (
    !metadata.isFile() ||
    metadata.dev !== rootDevice ||
    metadata.nlink !== 1
  ) {
    throw testNativeFailure("unsafe_entry");
  }
}

function testNativeDirectoryStatSync(path: string, rootDevice?: number): Stats {
  const metadata = lstatSync(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw testNativeFailure("unsafe_entry");
  }
  if (rootDevice !== undefined && metadata.dev !== rootDevice) {
    throw testNativeFailure("unsafe_entry");
  }
  return metadata;
}

async function testLstatOrUndefined(path: string): Promise<Stats | undefined> {
  try {
    return await lstat(path);
  } catch (error) {
    if (hasFsCode(error, "ENOENT")) return undefined;
    throw testNativeFailure("io_failure");
  }
}

function compareTestNativeUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function hasFsCode(error: unknown, code: string): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    error.code === code
  );
}

function isTestNativeFailure(error: unknown): boolean {
  return (
    error instanceof Error &&
    /^bluey_runner_storage:[a-z_]+$/.test(error.message)
  );
}

function translateTestNativeError(error: unknown): Error {
  return isTestNativeFailure(error)
    ? (error as Error)
    : testNativeFailure("io_failure");
}

function testNativeFailure(code: string): Error {
  return new Error(`bluey_runner_storage:${code}`);
}

async function writeProfileArtifact(
  volume: VolumeFixture,
  profileScope: string,
  contents: string,
): Promise<void> {
  const profile = await volume.subjectStorage.resolveProfile(profileScope);
  if (!profile) throw new Error("managed profile storage was not published");
  const directory = await profile.active.ensureChildDirectory("Default");
  const created = await directory.writeFileExclusive(
    "Cookies",
    Buffer.from(contents),
  );
  if (!created) await directory.replaceFile("Cookies", Buffer.from(contents));
}

function profileArtifactPath(
  volume: VolumeFixture,
  profileScope: string,
): string {
  return join(
    managedSubjectPath(volume, profileScope),
    "profiles",
    profileScope,
    "active",
    "Default",
    "Cookies",
  );
}

async function captureManagedProfileBackup(
  volume: VolumeFixture,
  profileScope: string,
): Promise<void> {
  const backup = join(volume.backupRoot, profileScope);
  await mkdir(backup, { recursive: true, mode: 0o700 });
  await cp(managedSubjectPath(volume, profileScope), join(backup, "subject"), {
    errorOnExist: true,
    force: false,
    recursive: true,
  });
  await cp(profileOwnerPath(volume, profileScope), join(backup, "owner.json"), {
    errorOnExist: true,
    force: false,
  });
}

async function restoreManagedProfileBackup(
  volume: VolumeFixture,
  profileScope: string,
): Promise<void> {
  const backup = join(volume.backupRoot, profileScope);
  const subject = managedSubjectPath(volume, profileScope);
  const owner = profileOwnerPath(volume, profileScope);
  await mkdir(dirname(subject), { recursive: true, mode: 0o700 });
  await mkdir(dirname(owner), { recursive: true, mode: 0o700 });
  await cp(join(backup, "subject"), subject, {
    errorOnExist: true,
    force: false,
    recursive: true,
  });
  await cp(join(backup, "owner.json"), owner, {
    errorOnExist: true,
    force: false,
  });
}

async function writeLegacyProfileArtifact(
  volume: VolumeFixture,
  profileScope: string,
  contents: string,
): Promise<void> {
  const path = legacyProfileArtifactPath(volume, profileScope);
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  await writeFile(path, contents, { mode: 0o600 });
}

function legacyProfileArtifactPath(
  volume: VolumeFixture,
  profileScope: string,
): string {
  return join(volume.root.path, "active", profileScope, "Default", "Cookies");
}

function managedSubjectPath(
  volume: VolumeFixture,
  profileScope: string,
): string {
  const purgeSubject =
    profileScope === volume.accountAProfileScope
      ? ACCOUNT_A_SUBJECT
      : ACCOUNT_B_SUBJECT;
  return join(
    volume.root.path,
    "account-data-v2",
    "subjects",
    accountPurgeSubjectHash(purgeSubject),
  );
}

function profileOwnerPath(volume: VolumeFixture, profileScope: string): string {
  return join(
    volume.root.path,
    "account-data-v2",
    "scope-owners",
    "profiles",
    `${profileScope}.json`,
  );
}

async function readRequestBody(request: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    size += bytes.length;
    if (size > MAXIMUM_REQUEST_BYTES)
      throw new HttpFailure(413, "request body too large");
    chunks.push(bytes);
  }
  return Buffer.concat(chunks).toString("utf8");
}

function writeJson(
  response: ServerResponse,
  status: number,
  value: unknown,
): void {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(body),
  });
  response.end(body);
}

function singleHeader(request: IncomingMessage, name: string): string {
  const value = request.headers[name];
  if (typeof value !== "string" || !value)
    throw new HttpFailure(401, "missing worker header");
  return value;
}

function requireRecord(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new HttpFailure(400, "expected JSON object");
  }
  return value as Record<string, unknown>;
}

function parseEnrollmentProof(value: unknown): RunnerVolumeEnrollmentProof {
  const proof = requireRecord(value);
  for (const field of [
    "admissionGrantId",
    "volumeId",
    "workerId",
    "provider",
    "providerResourceId",
    "resourceFingerprint",
    "publicKeyBase64url",
    "keyFingerprint",
    "signature",
  ]) {
    if (typeof proof[field] !== "string")
      throw new HttpFailure(400, "invalid enrollment proof");
  }
  if (
    proof.enrollmentEpoch !== 1 ||
    !Number.isSafeInteger(proof.legacyArtifactCount) ||
    !Number.isSafeInteger(proof.requestedAtMs)
  ) {
    throw new HttpFailure(400, "invalid enrollment proof");
  }
  return proof as unknown as RunnerVolumeEnrollmentProof;
}

function parseAuthorityProof(value: unknown): RunnerVolumeAuthorityProof {
  const proof = requireRecord(value);
  for (const field of [
    "audience",
    "operation",
    "requestId",
    "volumeId",
    "processInstanceId",
    "payloadSha256",
    "signature",
  ]) {
    if (typeof proof[field] !== "string")
      throw new HttpFailure(400, "invalid authority proof");
  }
  if (
    proof.version !== 1 ||
    proof.audience !== AUTHORITY_AUDIENCE ||
    !Number.isSafeInteger(proof.enrollmentEpoch) ||
    !Number.isSafeInteger(proof.issuedAtMs)
  ) {
    throw new HttpFailure(400, "invalid authority proof");
  }
  return proof as unknown as RunnerVolumeAuthorityProof;
}

function parsePurgeAck(value: unknown): RunnerVolumePurgeAck {
  try {
    return parseRunnerVolumePurgeAck(value);
  } catch {
    throw new HttpFailure(400, "invalid purge acknowledgement");
  }
}
