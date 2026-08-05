import { createHash, randomBytes } from "node:crypto";
import {
  AccountResidencyIndex,
  accountPurgeSubjectHash,
} from "./account-residency.js";
import {
  ensurePrivateRunnerDirectory,
  listSafeRunnerDirectory,
  pathExistsNoFollow,
  readBoundedRegularFile,
  runnerPath,
  writeDurableFileExclusive,
} from "./safe-runner-storage.js";
import { assertRunnerPurgeCurrentTargetStorageEmpty } from "./purge-storage-evidence.js";
import {
  NativeRunnerStorageError,
  type NativeRunnerStorageDirectory,
} from "./native-runner-storage.js";
import { LegacyRunnerStorageError } from "./legacy-runner-storage.js";
import {
  decodeCanonicalBase64Url,
  signEd25519,
  type RunnerVolumeIdentity,
  verifyEd25519,
} from "./volume-identity.js";
import {
  canonicalRunnerVolumePurgeAck,
  parseRunnerVolumePurgeAck,
  RunnerVolumePurger,
  RunnerVolumePurgeError,
  runnerBuildSatisfies,
  type RunnerVolumePurgeAck,
  type RunnerVolumePurgeCommand,
  type StopAccountWorkResult,
} from "./volume-purge.js";
import {
  SubjectStorageManagerError,
  type ManagedProfileStorage,
  type ManagedResultStorage,
  type SubjectStorageManager,
} from "./subject-storage-manager.js";
import {
  createRunnerVolumeStorageAttestation,
  RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256,
  RunnerVolumeStorageAttestationError,
  runnerVolumeLocatorSetEvidence,
  runnerVolumeStorageAttestationSha256,
  type RunnerVolumeStorageAttestation,
} from "./storage-attestation.js";
import { createJobsWorkerAuthHeaders } from "@bluey/jobs-automation/worker-auth";

const API_PREFIX = "/api/jobs/internal/runner-volumes";
const AUTHORITY_AUDIENCE = "bluey-jobs-runner-volume-authority";
const ENROLLMENT_DIRECTORY = "runner-volume-control-v1";
const ENROLLMENT_FILE = "enrollment-proof.json";
const RESIDENCY_DIRECTORY = "account-residency-v1";
const TOMBSTONES_DIRECTORY = "purge-tombstones";
const TOMBSTONE_AUDIENCE = "bluey-jobs-runner-volume-purge-tombstone";
const TOMBSTONE_RETENTION_POLICY = "indefinite_managed_restore_safety";
const PROFILE_SCOPE_PATTERN = /^[0-9a-f]{40}$/;
const RESULT_SCOPE_PATTERN = /^[0-9a-f]{64}$/;
const MAXIMUM_ENROLLMENT_BYTES = 16 * 1024;
const MAXIMUM_TOMBSTONE_BYTES = 128 * 1024;
const MAXIMUM_LOCAL_TOMBSTONES = 100_000;
const DEFAULT_REQUEST_TIMEOUT_MS = 5_000;
const DEFAULT_MAX_RESPONSE_BYTES = 512 * 1024;
const DEFAULT_CONTROL_INTERVAL_MS = 5_000;
const DEFAULT_POLL_LIMIT = 32;
const MAXIMUM_DRAINED_COMMANDS = 4_096;
const ENROLLMENT_EPOCH = 1;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const SAFE_IDENTIFIER_PATTERN = /^[A-Za-z0-9._:+-]{1,128}$/;

export type RunnerVolumeOperation =
  | "instance_claim"
  | "instance_heartbeat"
  | "residency_bind"
  | "purge_poll"
  | "storage_attestation"
  | "execution_lease_claim";

export interface RunnerExecutionLeaseClaimProofInput {
  readonly accountId: string;
  readonly applicationId: string;
  readonly runId: string;
  readonly browserProfileId: string;
  readonly ownerId: string;
}

export type RunnerVolumeClientErrorCode =
  | "configuration"
  | "invalid_response"
  | "not_ready"
  | "redirect_blocked"
  | "request_failed"
  | "response_too_large"
  | "timed_out";

export class RunnerVolumeClientError extends Error {
  constructor(
    readonly operation:
      "enroll" | RunnerVolumeOperation | "purge_ack" | "configuration",
    readonly code: RunnerVolumeClientErrorCode,
    readonly status?: number,
  ) {
    super(`Runner volume ${operation} failed (${code}).`);
    this.name = "RunnerVolumeClientError";
  }
}

export interface UnsignedRunnerVolumeEnrollmentProof {
  readonly admissionGrantId: string;
  readonly volumeId: string;
  readonly workerId: string;
  readonly provider: string;
  readonly providerResourceId: string;
  readonly resourceFingerprint: string;
  readonly enrollmentEpoch: 1;
  readonly publicKeyBase64url: string;
  readonly keyFingerprint: string;
  readonly legacyArtifactCount: number;
  readonly requestedAtMs: number;
}

export interface RunnerVolumeEnrollmentProof extends UnsignedRunnerVolumeEnrollmentProof {
  readonly signature: string;
}

export interface UnsignedRunnerVolumeAuthorityProof {
  readonly version: 1;
  readonly audience: typeof AUTHORITY_AUDIENCE;
  readonly operation: RunnerVolumeOperation;
  readonly requestId: string;
  readonly volumeId: string;
  readonly enrollmentEpoch: number;
  readonly processInstanceId: string;
  readonly issuedAtMs: number;
  readonly payloadSha256: string;
}

export interface RunnerVolumeAuthorityProof extends UnsignedRunnerVolumeAuthorityProof {
  readonly signature: string;
}

export interface RunnerVolumeClientOptions {
  readonly origin: string;
  readonly workerSigningKey: string;
  readonly workerId: string;
  readonly admissionGrantId: string;
  readonly admissionGrantToken: string;
  readonly provider: string;
  readonly providerResourceId: string;
  readonly resourceFingerprint: string;
  readonly legacyArtifactCount: number;
  readonly runnerBuildId: string;
  readonly processInstanceId: string;
  readonly identity: RunnerVolumeIdentity;
  readonly residency: AccountResidencyIndex;
  readonly subjectStorage: RunnerSubjectStorage;
  readonly serverCommandKeys: ReadonlyMap<string, string>;
  readonly stopAccountWork: (
    purgeSubjectSha256: string,
  ) => Promise<StopAccountWorkResult>;
  readonly requestTimeoutMs?: number;
  readonly maxResponseBytes?: number;
  readonly controlIntervalMs?: number;
  readonly pollLimit?: number;
  readonly fetch?: typeof globalThis.fetch;
  readonly nowMs?: () => number;
  readonly onFatalControlFailure?: (error: RunnerVolumeClientError) => void;
  readonly onReadinessChanged?: (ready: boolean) => void;
  readonly quiesceForStorageAttestation?: () => Promise<
    | { readonly status: "quiesced" }
    | { readonly status: "irreversible" }
    | { readonly status: "storage_not_ready" }
  >;
}

export type RunnerSubjectStorage = Pick<
  SubjectStorageManager,
  | "ensureProfile"
  | "ensureResult"
  | "identity"
  | "resolveProfile"
  | "resolveResult"
  | "root"
  | "inventoryCurrentStorage"
  | "withLockedSubject"
>;

export interface RunnerVolumeClientStartOptions {
  /**
   * Keep the process lease ready without scheduling background polling yet.
   * Startup recovery uses this narrow window to reconcile orphan plaintext
   * before any purge can race a recovery write.
   */
  readonly deferControlLoop?: boolean;
}

interface RunnerVolumeRecord {
  readonly volumeId: string;
  readonly workerId: string;
  readonly provider: string;
  readonly providerResourceId: string;
  readonly resourceFingerprint: string;
  readonly currentEpoch: number;
  readonly enrollmentGeneration: number;
  readonly requiredTombstoneGeneration: number;
  readonly reconciledTombstoneGeneration: number;
  readonly status: string;
  readonly activeInstanceId: string | null;
  readonly instanceLeaseExpiresAtMs: number | null;
  readonly legacyArtifactCount: number;
  readonly admissionGrantId: string;
  readonly enrolledAtMs: number;
  readonly lastSeenAtMs: number;
  readonly updatedAtMs: number;
  readonly publicKeyBase64url: string;
  readonly keyFingerprint: string;
  readonly disposition: "applied" | "replay";
}

interface RunnerVolumeInstanceLease {
  readonly volumeId: string;
  readonly enrollmentEpoch: number;
  readonly processInstanceId: string;
  readonly leaseExpiresAtMs: number;
  readonly disposition: "applied" | "replay";
}

interface RunnerVolumePollResponse {
  readonly commands: readonly RunnerVolumePurgeCommand[];
  readonly nextCommandCursor: string | null;
  readonly ready: boolean;
  readonly storageAttestationRequired: boolean;
  readonly enrollmentGeneration: number;
  readonly predecessorAttestationGeneration: number;
  readonly predecessorAttestationSha256: string;
  readonly requiredTombstoneGeneration: number;
  readonly reconciledTombstoneGeneration: number;
  readonly serverCommandKeys: Readonly<Record<string, string>>;
}

interface RunnerVolumeAckResponse {
  readonly disposition: "applied" | "replay";
  readonly status: Record<string, unknown>;
  readonly reconciledTombstoneGeneration: number;
}

interface RunnerVolumeStorageAttestationResponse {
  readonly disposition: "applied" | "replay";
  readonly attestationGeneration: number;
  readonly fleetAttestationGeneration: number;
  readonly attestationSha256: string;
  readonly volumeStatus: string;
}

interface PendingStorageAttestationRequest {
  readonly attestation: RunnerVolumeStorageAttestation;
  readonly sha256: string;
  readonly storageEvidenceRevision: number;
}

export class RunnerVolumeClient {
  readonly #origin: string;
  readonly #workerSigningKey: string;
  readonly #workerId: string;
  readonly #admissionGrantId: string;
  readonly #admissionGrantToken: string;
  readonly #provider: string;
  readonly #providerResourceId: string;
  readonly #resourceFingerprint: string;
  readonly #legacyArtifactCount: number;
  readonly #runnerBuildId: string;
  readonly #processInstanceId: string;
  readonly #identity: RunnerVolumeIdentity;
  readonly #residency: AccountResidencyIndex;
  readonly #subjectStorage: RunnerSubjectStorage;
  readonly #requestTimeoutMs: number;
  readonly #maxResponseBytes: number;
  readonly #controlIntervalMs: number;
  readonly #pollLimit: number;
  readonly #fetch: typeof globalThis.fetch;
  readonly #nowMs: () => number;
  readonly #onFatalControlFailure?: (error: RunnerVolumeClientError) => void;
  readonly #onReadinessChanged?: (ready: boolean) => void;
  readonly #quiesceForStorageAttestation: NonNullable<
    RunnerVolumeClientOptions["quiesceForStorageAttestation"]
  >;
  readonly #serverCommandKeys: ReadonlyMap<string, string>;
  readonly #purger: RunnerVolumePurger;
  readonly #reportedSubjects = new Set<string>();
  readonly #profileScopeSubjects = new Map<string, string>();
  readonly #profileScopeLocks = new Map<string, Promise<void>>();
  readonly #resultScopeSubjects = new Map<string, string>();
  readonly #resultScopeLocks = new Map<string, Promise<void>>();
  #controlTimer?: ReturnType<typeof setTimeout>;
  #controlInFlight?: Promise<void>;
  #stopped = false;
  #started = false;
  #controlEnabled = false;
  #recoveryComplete = false;
  #ready = false;
  #enrollmentGeneration = 0;
  #storageEvidenceStale = true;
  #storageEvidenceRevision = 0;
  #acceptedAttestationGeneration = 0;
  #acceptedAttestationSha256 = "";
  #pendingStorageAttestation?: PendingStorageAttestationRequest;
  #preparationInProgress = false;
  #preparationCursor: string | null = null;
  #preparationLegacyArtifactCount?: number;
  #legacyPreparationComplete = false;

  constructor(options: RunnerVolumeClientOptions) {
    this.#origin = normalizedOrigin(options.origin);
    this.#workerSigningKey = boundedSigningKey(options.workerSigningKey);
    this.#workerId = safeIdentifier(options.workerId);
    this.#admissionGrantId = safeIdentifier(options.admissionGrantId);
    decodeCanonicalBase64Url(options.admissionGrantToken, 32);
    this.#admissionGrantToken = options.admissionGrantToken;
    this.#provider = safeIdentifier(options.provider);
    this.#providerResourceId = boundedText(options.providerResourceId, 512);
    if (!SHA256_PATTERN.test(options.resourceFingerprint)) {
      throw configurationError();
    }
    this.#resourceFingerprint = options.resourceFingerprint;
    this.#legacyArtifactCount = boundedInteger(
      options.legacyArtifactCount,
      0,
      1_000_000,
    );
    this.#runnerBuildId = safeIdentifier(options.runnerBuildId);
    if (!runnerBuildSatisfies(this.#runnerBuildId, this.#runnerBuildId)) {
      throw configurationError();
    }
    decodeCanonicalBase64Url(options.processInstanceId, 32);
    this.#processInstanceId = options.processInstanceId;
    this.#identity = options.identity;
    this.#residency = options.residency;
    this.#subjectStorage = options.subjectStorage;
    if (this.#residency.identity.volumeId !== this.#identity.volumeId) {
      throw configurationError();
    }
    if (
      this.#subjectStorage.identity.volumeId !== this.#identity.volumeId ||
      this.#subjectStorage.root.configuredPath !== this.#residency.root.path
    ) {
      throw configurationError();
    }
    if (options.serverCommandKeys.size === 0) throw configurationError();
    for (const [keyId, publicKey] of options.serverCommandKeys) {
      safeIdentifier(keyId);
      decodeCanonicalBase64Url(publicKey, 32);
    }
    this.#serverCommandKeys = new Map(options.serverCommandKeys);
    this.#requestTimeoutMs = boundedInteger(
      options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS,
      100,
      60_000,
    );
    this.#maxResponseBytes = boundedInteger(
      options.maxResponseBytes ?? DEFAULT_MAX_RESPONSE_BYTES,
      256,
      4 * 1024 * 1024,
    );
    this.#controlIntervalMs = boundedInteger(
      options.controlIntervalMs ?? DEFAULT_CONTROL_INTERVAL_MS,
      100,
      60_000,
    );
    this.#pollLimit = boundedInteger(
      options.pollLimit ?? DEFAULT_POLL_LIMIT,
      1,
      64,
    );
    this.#fetch = options.fetch ?? globalThis.fetch;
    if (typeof this.#fetch !== "function") throw configurationError();
    this.#nowMs = options.nowMs ?? Date.now;
    this.#onFatalControlFailure = options.onFatalControlFailure;
    this.#onReadinessChanged = options.onReadinessChanged;
    this.#quiesceForStorageAttestation =
      options.quiesceForStorageAttestation ??
      (async () => ({ status: "quiesced" as const }));
    this.#purger = new RunnerVolumePurger(this.#residency, {
      enrollmentEpoch: ENROLLMENT_EPOCH,
      processInstanceId: this.#processInstanceId,
      runnerBuildId: this.#runnerBuildId,
      serverCommandKeys: this.#serverCommandKeys,
      subjectStorage: this.#subjectStorage,
      stopAccountWork: options.stopAccountWork,
      nowMs: this.#nowMs,
    });
  }

  get volumeId(): string {
    return this.#identity.volumeId;
  }

  get enrollmentEpoch(): number {
    return ENROLLMENT_EPOCH;
  }

  get processInstanceId(): string {
    return this.#processInstanceId;
  }

  get ready(): boolean {
    return this.#ready && !this.#stopped;
  }

  requireReady(): void {
    if (!this.ready)
      throw new RunnerVolumeClientError("purge_poll", "not_ready");
  }

  private requireStarted(): void {
    if (!this.#started || this.#stopped) {
      throw new RunnerVolumeClientError("purge_poll", "not_ready");
    }
  }

  async start(options: RunnerVolumeClientStartOptions = {}): Promise<void> {
    if (this.#stopped || this.#started || this.#controlInFlight) {
      throw new RunnerVolumeClientError("purge_poll", "not_ready");
    }
    await this.reconcileLocalTombstones();
    const enrollment = await this.enroll();
    assertEnrollmentBinding(enrollment, {
      identity: this.#identity,
      workerId: this.#workerId,
      provider: this.#provider,
      providerResourceId: this.#providerResourceId,
      resourceFingerprint: this.#resourceFingerprint,
      admissionGrantId: this.#admissionGrantId,
      legacyArtifactCount: this.#legacyArtifactCount,
    });
    this.#enrollmentGeneration = enrollment.enrollmentGeneration;
    await this.claimInstance();
    const poll = await this.drainPendingCommands();
    this.assertPollEnrollment(poll);
    this.#started = true;
    this.setReady(false);
    if (!options.deferControlLoop) {
      this.#controlEnabled = true;
      this.scheduleControlLoop();
    }
  }

  async activateControlLoop(): Promise<void> {
    if (
      !this.#started ||
      this.#stopped ||
      this.#controlTimer ||
      this.#controlInFlight ||
      this.#recoveryComplete
    ) {
      throw new RunnerVolumeClientError("purge_poll", "not_ready");
    }
    // Reclaim/refresh the same process instance after deferred startup work,
    // then drain anything that arrived before background control is enabled.
    this.#recoveryComplete = true;
    this.#controlEnabled = true;
    this.setReady(false);
    try {
      await this.claimInstance();
      const poll = await this.drainPendingCommands();
      this.assertPollEnrollment(poll);
      await this.reconcileStorageAttestationAndReadiness(poll);
      this.scheduleControlLoop();
    } catch (error) {
      this.setReady(false);
      if (isTransientControlFailure(error)) {
        this.scheduleControlLoop();
        return;
      }
      throw error;
    }
  }

  async stop(): Promise<void> {
    this.#stopped = true;
    this.setReady(false);
    if (this.#controlTimer) clearTimeout(this.#controlTimer);
    this.#controlTimer = undefined;
    await this.#controlInFlight;
  }

  async registerProfile(
    purgeSubject: string,
    profileScope: string,
  ): Promise<ManagedProfileStorage> {
    this.requireStarted();
    if (!PROFILE_SCOPE_PATTERN.test(profileScope)) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return this.withProfileScopeLock(profileScope, () =>
      this.registerProfileLocked(purgeSubject, profileScope),
    );
  }

  async withProfileWriteResidency<T>(
    purgeSubject: string,
    profileScope: string,
    operation: (storage: ManagedProfileStorage) => Promise<T>,
  ): Promise<T> {
    this.requireStarted();
    if (
      !PROFILE_SCOPE_PATTERN.test(profileScope) ||
      typeof operation !== "function"
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return this.withProfileScopeLock(profileScope, async () => {
      const storage = await this.registerProfileLocked(
        purgeSubject,
        profileScope,
      );
      const subjectSha256 = accountPurgeSubjectHash(purgeSubject);
      return this.#residency.withSubjectLock(subjectSha256, async () => {
        await this.requireProfileScopeSubject(profileScope, subjectSha256);
        if (storage.subjectSha256 !== subjectSha256) {
          throw new RunnerVolumePurgeError("corrupt_state");
        }
        return withScopedManagedProfileStorage(storage, operation);
      });
    });
  }

  async withExistingProfileResidency<T>(
    profileScope: string,
    operation: (storage: ManagedProfileStorage | null) => Promise<T>,
  ): Promise<T> {
    this.requireStarted();
    if (
      !PROFILE_SCOPE_PATTERN.test(profileScope) ||
      typeof operation !== "function"
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return this.withProfileScopeLock(profileScope, async () => {
      const indexedSubject = await this.profileScopeSubject(profileScope);
      if (
        indexedSubject &&
        (await this.#residency.hasPurgeBarrier(indexedSubject))
      ) {
        throw new RunnerVolumePurgeError("restored_data");
      }
      const storage = await this.#subjectStorage.resolveProfile(profileScope);
      const subjectSha256 = storage?.subjectSha256 ?? indexedSubject;
      if (!subjectSha256) return operation(null);
      return this.#residency.withSubjectLock(subjectSha256, async () => {
        await this.requireProfileScopeSubject(profileScope, subjectSha256);
        if (!storage || storage.subjectSha256 !== subjectSha256) {
          throw new RunnerVolumePurgeError("corrupt_state");
        }
        return withScopedManagedProfileStorage(storage, operation);
      });
    });
  }

  private async registerProfileLocked(
    purgeSubject: string,
    profileScope: string,
  ): Promise<ManagedProfileStorage> {
    const locator = await this.#residency.registerProfile(
      purgeSubject,
      profileScope,
    );
    const storage = await this.#subjectStorage.ensureProfile(locator);
    this.cacheProfileScopeSubject(profileScope, locator.subjectSha256);
    await this.bindResidency(purgeSubject);
    return storage;
  }

  private async requireProfileScopeSubject(
    profileScope: string,
    subjectSha256: string,
  ): Promise<void> {
    if (await this.#residency.hasPurgeBarrier(subjectSha256)) {
      throw new RunnerVolumePurgeError("restored_data");
    }
    const locators =
      await this.#residency.locatorsForSubjectHash(subjectSha256);
    if (
      !locators.some(
        (locator) =>
          locator.kind === "profile" && locator.scope === profileScope,
      )
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private async withProfileScopeLock<T>(
    profileScope: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    const predecessor =
      this.#profileScopeLocks.get(profileScope) ?? Promise.resolve();
    let release = (): void => undefined;
    const current = new Promise<void>((resolve) => {
      release = resolve;
    });
    this.#profileScopeLocks.set(profileScope, current);
    await predecessor;
    try {
      return await operation();
    } finally {
      release();
      if (this.#profileScopeLocks.get(profileScope) === current) {
        this.#profileScopeLocks.delete(profileScope);
      }
    }
  }

  async registerResult(
    purgeSubject: string,
    resultScope: string,
  ): Promise<ManagedResultStorage> {
    this.requireStarted();
    if (!RESULT_SCOPE_PATTERN.test(resultScope)) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return this.withResultScopeLock(resultScope, () =>
      this.registerResultLocked(purgeSubject, resultScope),
    );
  }

  async withResultWriteResidency<T>(
    purgeSubject: string,
    resultScope: string,
    operation: (storage: ManagedResultStorage) => Promise<T>,
  ): Promise<T> {
    this.requireStarted();
    if (
      !RESULT_SCOPE_PATTERN.test(resultScope) ||
      typeof operation !== "function"
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return this.withResultScopeLock(resultScope, async () => {
      const storage = await this.registerResultLocked(
        purgeSubject,
        resultScope,
      );
      const subjectSha256 = accountPurgeSubjectHash(purgeSubject);
      return this.#residency.withSubjectLock(subjectSha256, async () => {
        await this.requireResultScopeSubject(resultScope, subjectSha256);
        if (storage.subjectSha256 !== subjectSha256) {
          throw new RunnerVolumePurgeError("corrupt_state");
        }
        return withScopedManagedResultStorage(storage, operation);
      });
    });
  }

  async withExistingResultResidency<T>(
    resultScope: string,
    operation: (storage: ManagedResultStorage | null) => Promise<T>,
  ): Promise<T> {
    this.requireStarted();
    if (
      !RESULT_SCOPE_PATTERN.test(resultScope) ||
      typeof operation !== "function"
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return this.withResultScopeLock(resultScope, async () => {
      const indexedSubject = await this.resultScopeSubject(resultScope);
      if (
        indexedSubject &&
        (await this.#residency.hasPurgeBarrier(indexedSubject))
      ) {
        throw new RunnerVolumePurgeError("restored_data");
      }
      const storage = await this.#subjectStorage.resolveResult(resultScope);
      const subjectSha256 = storage?.subjectSha256 ?? indexedSubject;
      if (!subjectSha256) return operation(null);
      return this.#residency.withSubjectLock(subjectSha256, async () => {
        await this.requireResultScopeSubject(resultScope, subjectSha256);
        if (!storage || storage.subjectSha256 !== subjectSha256) {
          throw new RunnerVolumePurgeError("corrupt_state");
        }
        return withScopedManagedResultStorage(storage, operation);
      });
    });
  }

  private async registerResultLocked(
    purgeSubject: string,
    resultScope: string,
  ): Promise<ManagedResultStorage> {
    const locator = await this.#residency.registerResult(
      purgeSubject,
      resultScope,
    );
    const storage = await this.#subjectStorage.ensureResult(locator);
    this.cacheResultScopeSubject(resultScope, locator.subjectSha256);
    await this.bindResidency(purgeSubject);
    return storage;
  }

  private async requireResultScopeSubject(
    resultScope: string,
    subjectSha256: string,
  ): Promise<void> {
    if (await this.#residency.hasPurgeBarrier(subjectSha256)) {
      throw new RunnerVolumePurgeError("restored_data");
    }
    const locators =
      await this.#residency.locatorsForSubjectHash(subjectSha256);
    if (
      !locators.some(
        (locator) => locator.kind === "result" && locator.scope === resultScope,
      )
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private async withResultScopeLock<T>(
    resultScope: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    const predecessor =
      this.#resultScopeLocks.get(resultScope) ?? Promise.resolve();
    let release = (): void => undefined;
    const current = new Promise<void>((resolve) => {
      release = resolve;
    });
    this.#resultScopeLocks.set(resultScope, current);
    await predecessor;
    try {
      return await operation();
    } finally {
      release();
      if (this.#resultScopeLocks.get(resultScope) === current) {
        this.#resultScopeLocks.delete(resultScope);
      }
    }
  }

  createExecutionLeaseClaimProof(
    input: RunnerExecutionLeaseClaimProofInput,
  ): RunnerVolumeAuthorityProof {
    const path = "/api/jobs/internal/execution-leases/claim";
    return this.createAuthorityProof("execution_lease_claim", path, [
      `account_id=${input.accountId}`,
      `application_id=${input.applicationId}`,
      `run_id=${input.runId}`,
      `browser_profile_id=${input.browserProfileId}`,
      `owner_id=${input.ownerId}`,
      `volume_id=${this.volumeId}`,
      `enrollment_epoch=${this.enrollmentEpoch}`,
      `process_instance_id=${this.processInstanceId}`,
    ]);
  }

  private async enroll(): Promise<RunnerVolumeRecord> {
    const proof = await this.loadOrCreateEnrollmentProof();
    return parseEnrollmentResponse(
      await this.request("enroll", `${API_PREFIX}/enroll`, {
        grantToken: this.#admissionGrantToken,
        proof,
      }),
    );
  }

  private async claimInstance(): Promise<void> {
    const path = `${API_PREFIX}/${encodeURIComponent(this.volumeId)}/instances/claim`;
    const proof = this.createAuthorityProof("instance_claim", path, []);
    const lease = parseInstanceLease(
      await this.request("instance_claim", path, { proof }),
    );
    assertInstanceBinding(lease, this.volumeId, this.#processInstanceId);
  }

  private async heartbeat(): Promise<void> {
    const path = `${API_PREFIX}/${encodeURIComponent(this.volumeId)}/instances/heartbeat`;
    const proof = this.createAuthorityProof("instance_heartbeat", path, []);
    const lease = parseInstanceLease(
      await this.request("instance_heartbeat", path, { proof }),
    );
    assertInstanceBinding(lease, this.volumeId, this.#processInstanceId);
  }

  private async bindResidency(purgeSubject: string): Promise<void> {
    const subjectHash = accountPurgeSubjectHash(purgeSubject);
    if (this.#reportedSubjects.has(subjectHash)) return;
    const path = `${API_PREFIX}/${encodeURIComponent(this.volumeId)}/residencies`;
    const proof = this.createAuthorityProof("residency_bind", path, [
      `purge_subject=${purgeSubject}`,
    ]);
    await this.request("residency_bind", path, { proof, purgeSubject }, true);
    this.#reportedSubjects.add(subjectHash);
  }

  private async poll(
    afterCommandId: string | null = null,
  ): Promise<RunnerVolumePollResponse> {
    const path = `${API_PREFIX}/${encodeURIComponent(this.volumeId)}/commands/poll`;
    const proof = this.createAuthorityProof("purge_poll", path, [
      `after_command_id=${afterCommandId ?? ""}`,
      `limit=${this.#pollLimit}`,
    ]);
    return parsePollResponse(
      await this.request("purge_poll", path, {
        afterCommandId,
        proof,
        limit: this.#pollLimit,
      }),
      this.#serverCommandKeys,
      this.#pollLimit,
    );
  }

  private async acknowledge(
    ack: RunnerVolumePurgeAck,
  ): Promise<RunnerVolumeAckResponse> {
    const commandPath = encodeURIComponent(ack.commandId);
    const path = `${API_PREFIX}/${encodeURIComponent(this.volumeId)}/commands/${commandPath}/ack`;
    return parseAckResponse(await this.request("purge_ack", path, ack));
  }

  private async submitStorageAttestation(
    poll: RunnerVolumePollResponse,
  ): Promise<RunnerVolumeStorageAttestationResponse> {
    if (!this.#pendingStorageAttestation) {
      const locatorsBefore = await this.#residency.allLocators();
      const purgedBefore = await this.purgedSubjectsFor(locatorsBefore);
      const locatorSet = runnerVolumeLocatorSetEvidence(
        locatorsBefore,
        this.#identity,
        purgedBefore,
      );
      const inventory =
        await this.#subjectStorage.inventoryCurrentStorage(locatorsBefore);
      const locatorsAfter = await this.#residency.allLocators();
      const purgedAfter = await this.purgedSubjectsFor(locatorsAfter);
      const locatorSetAfter = runnerVolumeLocatorSetEvidence(
        locatorsAfter,
        this.#identity,
        purgedAfter,
      );
      if (
        locatorSet.count !== locatorSetAfter.count ||
        locatorSet.residentCount !== locatorSetAfter.residentCount ||
        locatorSet.sha256 !== locatorSetAfter.sha256
      ) {
        throw new RunnerVolumeClientError(
          "storage_attestation",
          "invalid_response",
        );
      }
      const observedAtMs = this.#nowMs();
      assertTimestamp(observedAtMs);
      const attestation = createRunnerVolumeStorageAttestation({
        identity: this.#identity,
        // Base64url entropy may begin with `-` or `_`, while the shared
        // runner-identifier grammar deliberately requires an alphanumeric
        // first byte. A fixed purpose prefix makes every generated ID valid.
        attestationId: `att-${randomBytes(24).toString("base64url")}`,
        resourceFingerprint: this.#resourceFingerprint,
        enrollmentEpoch: ENROLLMENT_EPOCH,
        enrollmentGeneration: poll.enrollmentGeneration,
        processInstanceId: this.#processInstanceId,
        predecessorAttestationGeneration: poll.predecessorAttestationGeneration,
        predecessorAttestationSha256: poll.predecessorAttestationSha256,
        requiredTombstoneGeneration: poll.requiredTombstoneGeneration,
        reconciledTombstoneGeneration: poll.reconciledTombstoneGeneration,
        inventory,
        locatorSet,
        runnerBuildId: this.#runnerBuildId,
        observedAtMs,
      });
      const sha256 = runnerVolumeStorageAttestationSha256(attestation);
      this.#pendingStorageAttestation = Object.freeze({
        attestation,
        sha256,
        storageEvidenceRevision: this.#storageEvidenceRevision,
      });
    }

    const pending = this.#pendingStorageAttestation;
    const path = `${API_PREFIX}/${encodeURIComponent(this.volumeId)}/storage-attestations`;
    const proof = this.createAuthorityProof("storage_attestation", path, [
      `attestation_sha256=${pending.sha256}`,
    ]);
    const response = parseStorageAttestationResponse(
      await this.request("storage_attestation", path, {
        proof,
        attestation: pending.attestation,
      }),
    );
    if (
      response.attestationSha256 !== pending.sha256 ||
      response.attestationGeneration !==
        pending.attestation.predecessorAttestationGeneration + 1
    ) {
      throw new RunnerVolumeClientError(
        "storage_attestation",
        "invalid_response",
      );
    }
    this.#pendingStorageAttestation = undefined;
    return response;
  }

  private async purgedSubjectsFor(
    locators: readonly { readonly subjectSha256: string }[],
  ): Promise<ReadonlySet<string>> {
    const purged = new Set<string>();
    for (const subjectSha256 of [
      ...new Set(locators.map((locator) => locator.subjectSha256)),
    ].sort()) {
      if (await this.#residency.hasPurgeBarrier(subjectSha256)) {
        purged.add(subjectSha256);
      }
    }
    return purged;
  }

  private async drainPendingCommands(): Promise<RunnerVolumePollResponse> {
    let work = 0;
    if (!this.#preparationInProgress && !this.#legacyPreparationComplete) {
      this.#preparationInProgress = true;
      this.#preparationCursor = null;
      this.#preparationLegacyArtifactCount = undefined;
    }

    while (this.#preparationInProgress) {
      const cursor = this.#preparationCursor;
      if (cursor !== null && this.#pendingStorageAttestation) {
        throw new RunnerVolumeClientError(
          "storage_attestation",
          "invalid_response",
        );
      }
      let response = await this.poll(cursor);
      if (cursor === null) {
        response = await this.resolvePendingStorageAttestation(response);
      }
      if (
        work > 0 &&
        work + response.commands.length > MAXIMUM_DRAINED_COMMANDS
      ) {
        return response;
      }
      for (const command of response.commands) {
        this.#storageEvidenceRevision += 1;
        this.#storageEvidenceStale = true;
        this.#preparationLegacyArtifactCount =
          await this.#purger.prepare(command);
        work += 1;
      }
      if (response.nextCommandCursor !== null) {
        this.#preparationCursor = response.nextCommandCursor;
        if (work >= MAXIMUM_DRAINED_COMMANDS) {
          return response;
        }
        continue;
      }

      this.#preparationInProgress = false;
      this.#preparationCursor = null;
      if (this.#preparationLegacyArtifactCount === undefined) {
        this.#legacyPreparationComplete = false;
        return response;
      }
      if (this.#preparationLegacyArtifactCount > 0) {
        return response;
      }
      this.#preparationLegacyArtifactCount = undefined;
      this.#legacyPreparationComplete = true;
      if (work >= MAXIMUM_DRAINED_COMMANDS) {
        return response;
      }
    }

    while (this.#legacyPreparationComplete) {
      let response = await this.poll();
      response = await this.resolvePendingStorageAttestation(response);
      if (response.commands.length === 0) {
        this.#legacyPreparationComplete = false;
        return response;
      }
      if (
        work > 0 &&
        work + response.commands.length * 2 > MAXIMUM_DRAINED_COMMANDS
      ) {
        return response;
      }

      let remainingLegacyArtifactCount = 0;
      for (const command of response.commands) {
        this.#storageEvidenceRevision += 1;
        this.#storageEvidenceStale = true;
        remainingLegacyArtifactCount = await this.#purger.prepare(command);
        work += 1;
      }
      if (remainingLegacyArtifactCount > 0) {
        this.#legacyPreparationComplete = false;
        this.#preparationInProgress = true;
        this.#preparationCursor = null;
        this.#preparationLegacyArtifactCount =
          remainingLegacyArtifactCount;
        return response;
      }

      for (const command of response.commands) {
        await this.executeAndAcknowledge(command);
        work += 1;
      }
      if (work >= MAXIMUM_DRAINED_COMMANDS) {
        return response;
      }
    }

    throw new RunnerVolumeClientError("purge_poll", "invalid_response");
  }

  private async executeAndAcknowledge(
    command: RunnerVolumePurgeCommand,
  ): Promise<void> {
    let ack = await this.#purger.execute(command);
    try {
      await this.acknowledge(ack);
    } catch (error) {
      // Always try the durable old-process ACK first: if the server committed
      // it before a response-loss crash, exact replay succeeds. Only a 401 for
      // a different process allows a zero-rescan-backed current-process ACK to
      // replace the mutable local journal.
      if (
        !(error instanceof RunnerVolumeClientError) ||
        error.operation !== "purge_ack" ||
        error.status !== 401 ||
        ack.processInstanceId === this.#processInstanceId
      ) {
        throw error;
      }
      ack = await this.#purger.recoverAcknowledgementForCurrentProcess(
        command,
        ack,
      );
      await this.acknowledge(ack);
    }
    this.#storageEvidenceStale = true;
  }

  private async reconcileLocalTombstones(): Promise<void> {
    const directory = runnerPath(
      this.#residency.root,
      RESIDENCY_DIRECTORY,
      TOMBSTONES_DIRECTORY,
    );
    const entries = await listSafeRunnerDirectory(
      this.#residency.root,
      directory,
    );
    if (entries.length > MAXIMUM_LOCAL_TOMBSTONES) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    const tombstones: Array<
      LocalPurgeTombstone & {
        readonly entry: string;
        readonly encodedSha256: string;
      }
    > = [];
    for (const entry of entries) {
      const match = /^([0-9a-f]{64})\.json$/.exec(entry);
      if (!match) throw new RunnerVolumePurgeError("corrupt_state");
      const encoded = await readBoundedRegularFile(
        this.#residency.root,
        runnerPath(
          this.#residency.root,
          RESIDENCY_DIRECTORY,
          TOMBSTONES_DIRECTORY,
          entry,
        ),
        MAXIMUM_TOMBSTONE_BYTES,
      );
      if (!encoded) throw new RunnerVolumePurgeError("corrupt_state");
      tombstones.push({
        ...parseLocalPurgeTombstone(encoded, match[1]!, this.#identity),
        entry,
        encodedSha256: createHash("sha256").update(encoded).digest("hex"),
      });
    }
    // Online commands remain authoritative in server poll order, which is
    // completion-generation order. Completed local tombstones do not encode
    // that separate cursor, so restart revalidation uses their signed request
    // generation only as a deterministic traversal order and proves all empty.
    tombstones.sort(
      (left, right) =>
        left.purgeGeneration - right.purgeGeneration ||
        left.purgeSubjectSha256.localeCompare(right.purgeSubjectSha256),
    );

    // A restored root can contain data for several independently tombstoned
    // subjects. Delete every exact target first; requiring a global legacy
    // zero after the first subject would make the remaining tombstones
    // impossible to reconcile deterministically.
    for (const tombstone of tombstones) {
      await this.#subjectStorage.withLockedSubject(
        tombstone.purgeSubjectSha256,
        async (storage) => {
          await this.requireCurrentLocalTombstone(tombstone);
          const locators = await this.#residency.locatorsForSubjectHash(
            tombstone.purgeSubjectSha256,
          );
          await storage.removeLegacyTargets(locators);
        },
      );
    }

    // The second pass reopens every signed tombstone and takes a fresh
    // same-snapshot proof only after the complete restored set is gone.
    for (const tombstone of tombstones) {
      await this.#subjectStorage.withLockedSubject(
        tombstone.purgeSubjectSha256,
        async (storage) => {
          await this.requireCurrentLocalTombstone(tombstone);
          const locators = await this.#residency.locatorsForSubjectHash(
            tombstone.purgeSubjectSha256,
          );
          const removal = await storage.remove();
          try {
            assertRunnerPurgeCurrentTargetStorageEmpty(removal.after, locators);
          } catch {
            throw new RunnerVolumePurgeError("incomplete_purge");
          }
        },
      );
    }
  }

  private async requireCurrentLocalTombstone(
    tombstone: LocalPurgeTombstone & {
      readonly entry: string;
      readonly encodedSha256: string;
    },
  ): Promise<void> {
    const encoded = await readBoundedRegularFile(
      this.#residency.root,
      runnerPath(
        this.#residency.root,
        RESIDENCY_DIRECTORY,
        TOMBSTONES_DIRECTORY,
        tombstone.entry,
      ),
      MAXIMUM_TOMBSTONE_BYTES,
    );
    if (
      !encoded ||
      createHash("sha256").update(encoded).digest("hex") !==
        tombstone.encodedSha256
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    const persisted = parseLocalPurgeTombstone(
      encoded,
      tombstone.purgeSubjectSha256,
      this.#identity,
    );
    if (JSON.stringify(persisted.ack) !== JSON.stringify(tombstone.ack)) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private async resultScopeSubject(
    resultScope: string,
  ): Promise<string | undefined> {
    const cached = this.#resultScopeSubjects.get(resultScope);
    if (cached) return cached;
    const locatorsDirectory = runnerPath(
      this.#residency.root,
      RESIDENCY_DIRECTORY,
      "locators",
    );
    const subjects = await listSafeRunnerDirectory(
      this.#residency.root,
      locatorsDirectory,
    );
    if (subjects.length > MAXIMUM_LOCAL_TOMBSTONES) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    let matched: string | undefined;
    for (const subjectSha256 of subjects) {
      if (!SHA256_PATTERN.test(subjectSha256)) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const locators =
        await this.#residency.locatorsForSubjectHash(subjectSha256);
      if (
        !locators.some(
          (locator) =>
            locator.kind === "result" && locator.scope === resultScope,
        )
      ) {
        continue;
      }
      if (matched && matched !== subjectSha256) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      matched = subjectSha256;
    }
    if (matched) this.cacheResultScopeSubject(resultScope, matched);
    return matched;
  }

  private async profileScopeSubject(
    profileScope: string,
  ): Promise<string | undefined> {
    const cached = this.#profileScopeSubjects.get(profileScope);
    if (cached) return cached;
    const locatorsDirectory = runnerPath(
      this.#residency.root,
      RESIDENCY_DIRECTORY,
      "locators",
    );
    const subjects = await listSafeRunnerDirectory(
      this.#residency.root,
      locatorsDirectory,
    );
    if (subjects.length > MAXIMUM_LOCAL_TOMBSTONES) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    let matched: string | undefined;
    for (const subjectSha256 of subjects) {
      if (!SHA256_PATTERN.test(subjectSha256)) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const locators =
        await this.#residency.locatorsForSubjectHash(subjectSha256);
      if (
        !locators.some(
          (locator) =>
            locator.kind === "profile" && locator.scope === profileScope,
        )
      ) {
        continue;
      }
      if (matched && matched !== subjectSha256) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      matched = subjectSha256;
    }
    if (matched) this.cacheProfileScopeSubject(profileScope, matched);
    return matched;
  }

  private cacheProfileScopeSubject(
    profileScope: string,
    subjectSha256: string,
  ): void {
    const existing = this.#profileScopeSubjects.get(profileScope);
    if (existing && existing !== subjectSha256) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    this.#profileScopeSubjects.set(profileScope, subjectSha256);
  }

  private cacheResultScopeSubject(
    resultScope: string,
    subjectSha256: string,
  ): void {
    const existing = this.#resultScopeSubjects.get(resultScope);
    if (existing && existing !== subjectSha256) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    this.#resultScopeSubjects.set(resultScope, subjectSha256);
  }

  private assertPollEnrollment(poll: RunnerVolumePollResponse): void {
    if (poll.enrollmentGeneration !== this.#enrollmentGeneration) {
      throw new RunnerVolumeClientError("purge_poll", "invalid_response");
    }
  }

  private async reconcileStorageAttestationAndReadiness(
    initialPoll: RunnerVolumePollResponse,
  ): Promise<void> {
    let poll = initialPoll;
    this.assertPollEnrollment(poll);
    poll = await this.resolvePendingStorageAttestation(poll);
    if (!this.#recoveryComplete) {
      this.setReady(false);
      return;
    }
    if (this.hasPendingPurgeWork(poll)) {
      this.setReady(false);
      return;
    }

    if (poll.storageAttestationRequired || this.#storageEvidenceStale) {
      this.setReady(false);
      const quiescence = await this.#quiesceForStorageAttestation();
      if (quiescence.status !== "quiesced") return;

      // Quiescence can overlap a newly delivered purge command. Drain again
      // after all reversible writes are closed so the attested root is the
      // final post-command snapshot rather than a pre-quiescence observation.
      poll = await this.drainPendingCommands();
      this.assertPollEnrollment(poll);
      if (this.hasPendingPurgeWork(poll)) return;
      if (
        poll.requiredTombstoneGeneration !== poll.reconciledTombstoneGeneration
      ) {
        return;
      }
      if (poll.storageAttestationRequired || this.#storageEvidenceStale) {
        let accepted: RunnerVolumeStorageAttestationResponse;
        try {
          accepted = await this.submitStorageAttestation(poll);
        } catch (error) {
          if (
            error instanceof RunnerVolumeStorageAttestationError &&
            error.code === "invalid_inventory"
          ) {
            return;
          }
          throw error;
        }
        this.#acceptedAttestationGeneration = accepted.attestationGeneration;
        this.#acceptedAttestationSha256 = accepted.attestationSha256;
        this.#storageEvidenceStale = false;
        poll = await this.poll();
        this.assertPollEnrollment(poll);
      }
    }

    const attestationMatches =
      this.#acceptedAttestationGeneration > 0 &&
      poll.predecessorAttestationGeneration ===
        this.#acceptedAttestationGeneration &&
      poll.predecessorAttestationSha256 === this.#acceptedAttestationSha256;
    this.setReady(
      poll.ready &&
        !poll.storageAttestationRequired &&
        poll.requiredTombstoneGeneration ===
          poll.reconciledTombstoneGeneration &&
        attestationMatches,
    );
  }

  private hasPendingPurgeWork(poll: RunnerVolumePollResponse): boolean {
    return (
      poll.commands.length > 0 ||
      this.#preparationInProgress ||
      this.#legacyPreparationComplete ||
      (this.#preparationLegacyArtifactCount ?? 0) > 0
    );
  }

  private async resolvePendingStorageAttestation(
    poll: RunnerVolumePollResponse,
  ): Promise<RunnerVolumePollResponse> {
    this.assertPollEnrollment(poll);
    const pending = this.#pendingStorageAttestation;
    if (!pending) return poll;
    if (
      pending.attestation.enrollmentGeneration !==
      poll.enrollmentGeneration
    ) {
      throw new RunnerVolumeClientError(
        "storage_attestation",
        "invalid_response",
      );
    }
    const acceptedGeneration =
      pending.attestation.predecessorAttestationGeneration + 1;
    if (
      poll.predecessorAttestationGeneration === acceptedGeneration &&
      poll.predecessorAttestationSha256 === pending.sha256
    ) {
      this.#acceptedAttestationGeneration = acceptedGeneration;
      this.#acceptedAttestationSha256 = pending.sha256;
      if (
        pending.storageEvidenceRevision === this.#storageEvidenceRevision &&
        !poll.storageAttestationRequired
      ) {
        this.#storageEvidenceStale = false;
      }
      this.#pendingStorageAttestation = undefined;
      return poll;
    }
    if (
      pending.attestation.predecessorAttestationGeneration !==
        poll.predecessorAttestationGeneration ||
      pending.attestation.predecessorAttestationSha256 !==
        poll.predecessorAttestationSha256
    ) {
      throw new RunnerVolumeClientError(
        "storage_attestation",
        "invalid_response",
      );
    }
    if (
      pending.attestation.requiredTombstoneGeneration !==
        poll.requiredTombstoneGeneration ||
      pending.attestation.reconciledTombstoneGeneration !==
        poll.reconciledTombstoneGeneration
    ) {
      this.#pendingStorageAttestation = undefined;
      return poll;
    }
    if (pending.storageEvidenceRevision !== this.#storageEvidenceRevision) {
      throw new RunnerVolumeClientError(
        "storage_attestation",
        "invalid_response",
      );
    }

    const accepted = await this.submitStorageAttestation(poll);
    this.#acceptedAttestationGeneration = accepted.attestationGeneration;
    this.#acceptedAttestationSha256 = accepted.attestationSha256;
    this.#storageEvidenceStale = false;
    const refreshed = await this.poll();
    this.assertPollEnrollment(refreshed);
    return refreshed;
  }

  private setReady(ready: boolean): void {
    const next = ready && !this.#stopped;
    if (this.#ready === next) return;
    this.#ready = next;
    this.#onReadinessChanged?.(next);
  }

  private scheduleControlLoop(): void {
    if (this.#stopped || !this.#started || !this.#controlEnabled) return;
    this.#controlTimer = setTimeout(() => {
      this.#controlTimer = undefined;
      const control = (async () => {
        await this.heartbeat();
        const poll = await this.drainPendingCommands();
        await this.reconcileStorageAttestationAndReadiness(poll);
      })()
        .catch((error: unknown) => {
          this.setReady(false);
          const failure =
            error instanceof RunnerVolumeClientError
              ? error
              : new RunnerVolumeClientError(
                  "storage_attestation",
                  isFatalLocalStorageFailure(error)
                    ? "invalid_response"
                    : "request_failed",
                );
          if (!isTransientControlFailure(failure)) {
            this.#onFatalControlFailure?.(failure);
          }
        })
        .finally(() => {
          if (this.#controlInFlight === control)
            this.#controlInFlight = undefined;
          this.scheduleControlLoop();
        });
      this.#controlInFlight = control;
    }, this.#controlIntervalMs);
  }

  private createAuthorityProof(
    operation: RunnerVolumeOperation,
    path: string,
    payloadExtensions: readonly string[],
  ): RunnerVolumeAuthorityProof {
    const issuedAtMs = this.#nowMs();
    assertTimestamp(issuedAtMs);
    const payloadSha256 = runnerVolumeHttpPayloadSha256(
      path,
      this.#workerId,
      payloadExtensions,
    );
    const unsigned: UnsignedRunnerVolumeAuthorityProof = {
      version: 1,
      audience: AUTHORITY_AUDIENCE,
      operation,
      requestId: randomBytes(24).toString("base64url"),
      volumeId: this.volumeId,
      enrollmentEpoch: ENROLLMENT_EPOCH,
      processInstanceId: this.#processInstanceId,
      issuedAtMs,
      payloadSha256,
    };
    return {
      ...unsigned,
      signature: signEd25519(
        this.#identity.privateKey,
        canonicalRunnerVolumeAuthorityProof(unsigned),
      ),
    };
  }

  private async loadOrCreateEnrollmentProof(): Promise<RunnerVolumeEnrollmentProof> {
    await ensurePrivateRunnerDirectory(
      this.#residency.root,
      ENROLLMENT_DIRECTORY,
    );
    const path = runnerPath(
      this.#residency.root,
      ENROLLMENT_DIRECTORY,
      ENROLLMENT_FILE,
    );
    const existing = await readBoundedRegularFile(
      this.#residency.root,
      path,
      MAXIMUM_ENROLLMENT_BYTES,
    );
    if (existing) {
      return parseStoredEnrollmentProof(
        existing,
        this.enrollmentBinding(),
        this.#identity,
      );
    }
    const requestedAtMs = this.#nowMs();
    assertTimestamp(requestedAtMs);
    const unsigned: UnsignedRunnerVolumeEnrollmentProof = {
      ...this.enrollmentBinding(),
      enrollmentEpoch: ENROLLMENT_EPOCH,
      publicKeyBase64url: this.#identity.publicKeyRaw,
      keyFingerprint: this.#identity.publicKeyFingerprint,
      legacyArtifactCount: this.#legacyArtifactCount,
      requestedAtMs,
    };
    const proof = {
      ...unsigned,
      signature: signEd25519(
        this.#identity.privateKey,
        canonicalRunnerVolumeEnrollmentProof(unsigned),
      ),
    };
    const encoded = Buffer.from(`${JSON.stringify(proof)}\n`, "utf8");
    const created = await writeDurableFileExclusive(
      this.#residency.root,
      path,
      encoded,
    );
    if (!created) {
      const raced = await readBoundedRegularFile(
        this.#residency.root,
        path,
        MAXIMUM_ENROLLMENT_BYTES,
      );
      if (!raced)
        throw new RunnerVolumeClientError("enroll", "invalid_response");
      return parseStoredEnrollmentProof(
        raced,
        this.enrollmentBinding(),
        this.#identity,
      );
    }
    return proof;
  }

  private enrollmentBinding(): Pick<
    UnsignedRunnerVolumeEnrollmentProof,
    | "admissionGrantId"
    | "volumeId"
    | "workerId"
    | "provider"
    | "providerResourceId"
    | "resourceFingerprint"
    | "legacyArtifactCount"
  > {
    return {
      admissionGrantId: this.#admissionGrantId,
      volumeId: this.volumeId,
      workerId: this.#workerId,
      provider: this.#provider,
      providerResourceId: this.#providerResourceId,
      resourceFingerprint: this.#resourceFingerprint,
      legacyArtifactCount: this.#legacyArtifactCount,
    };
  }

  private async request(
    operation: "enroll" | RunnerVolumeOperation | "purge_ack",
    path: string,
    body: object,
    allowEmpty = false,
  ): Promise<unknown> {
    const controller = new AbortController();
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, this.#requestTimeoutMs);
    timeout.unref?.();
    try {
      const serializedBody = JSON.stringify(body);
      const response = await this.#fetch(`${this.#origin}${path}`, {
        method: "POST",
        headers: {
          ...createJobsWorkerAuthHeaders({
            signingKey: this.#workerSigningKey,
            workerId: this.#workerId,
            method: "POST",
            path,
            body: serializedBody,
          }),
          "Content-Type": "application/json",
        },
        body: serializedBody,
        redirect: "error",
        signal: controller.signal,
      });
      if (
        response.redirected ||
        (response.status >= 300 && response.status < 400)
      ) {
        await readBounded(response, this.#maxResponseBytes, operation);
        throw new RunnerVolumeClientError(
          operation,
          "redirect_blocked",
          response.status,
        );
      }
      const responseText = await readBounded(
        response,
        this.#maxResponseBytes,
        operation,
      );
      if (!response.ok) {
        throw new RunnerVolumeClientError(
          operation,
          "request_failed",
          response.status,
        );
      }
      if (!responseText) {
        if (allowEmpty) return undefined;
        throw new RunnerVolumeClientError(
          operation,
          "invalid_response",
          response.status,
        );
      }
      try {
        return JSON.parse(responseText) as unknown;
      } catch {
        throw new RunnerVolumeClientError(
          operation,
          "invalid_response",
          response.status,
        );
      }
    } catch (error) {
      if (error instanceof RunnerVolumeClientError) {
        if (timedOut && error.code === "request_failed") {
          throw new RunnerVolumeClientError(operation, "timed_out");
        }
        throw error;
      }
      throw new RunnerVolumeClientError(
        operation,
        timedOut ? "timed_out" : "request_failed",
      );
    } finally {
      clearTimeout(timeout);
    }
  }
}

interface ScopedManagedStorageState {
  active: boolean;
  readonly calls: Promise<unknown>[];
}

async function withScopedManagedProfileStorage<T>(
  storage: ManagedProfileStorage,
  operation: (storage: ManagedProfileStorage) => Promise<T>,
): Promise<T> {
  const state: ScopedManagedStorageState = { active: true, calls: [] };
  const scoped = Object.freeze({
    ...storage,
    root: scopedNativeDirectory(storage.root, state),
    active: scopedNativeDirectory(storage.active, state),
    snapshots: scopedNativeDirectory(storage.snapshots, state),
    checkpoints: scopedNativeDirectory(storage.checkpoints, state),
    receipts: scopedNativeDirectory(storage.receipts, state),
    temporary: scopedNativeDirectory(storage.temporary, state),
  });
  return completeScopedManagedStorageOperation(state, () => operation(scoped));
}

async function withScopedManagedResultStorage<T>(
  storage: ManagedResultStorage,
  operation: (storage: ManagedResultStorage) => Promise<T>,
): Promise<T> {
  const state: ScopedManagedStorageState = { active: true, calls: [] };
  const scoped = Object.freeze({
    ...storage,
    root: scopedNativeDirectory(storage.root, state),
    temporary: scopedNativeDirectory(storage.temporary, state),
  });
  return completeScopedManagedStorageOperation(state, () => operation(scoped));
}

async function completeScopedManagedStorageOperation<T>(
  state: ScopedManagedStorageState,
  operation: () => Promise<T>,
): Promise<T> {
  let succeeded = false;
  let result: T | undefined;
  let callbackError: unknown;
  try {
    result = await operation();
    succeeded = true;
  } catch (error) {
    callbackError = error;
  } finally {
    state.active = false;
  }
  const settled = await Promise.allSettled(state.calls);
  if (!succeeded) throw callbackError;
  const failed = settled.find((outcome) => outcome.status === "rejected");
  if (failed?.status === "rejected") throw failed.reason;
  return result as T;
}

function scopedNativeDirectory(
  directory: NativeRunnerStorageDirectory,
  state: ScopedManagedStorageState,
): NativeRunnerStorageDirectory {
  const invoke = <T>(operation: () => Promise<T>): Promise<T> => {
    if (!state.active) {
      return Promise.reject(new RunnerVolumePurgeError("corrupt_state"));
    }
    const pending = Promise.resolve().then(operation);
    state.calls.push(pending);
    void pending.catch(() => undefined);
    return pending;
  };
  return Object.freeze({
    relativePath: directory.relativePath,
    canonicalPath: directory.canonicalPath,
    deviceId: directory.deviceId,
    linkCount: directory.linkCount,
    ensureChildDirectory: (name: string) =>
      invoke(async () =>
        scopedNativeDirectory(
          await directory.ensureChildDirectory(name),
          state,
        ),
      ),
    openChildDirectory: (name: string) =>
      invoke(async () =>
        scopedNativeDirectory(await directory.openChildDirectory(name), state),
      ),
    writeFileExclusive: (name: string, contents: Buffer) =>
      invoke(() => directory.writeFileExclusive(name, contents)),
    replaceFile: (name: string, contents: Buffer) =>
      invoke(() => directory.replaceFile(name, contents)),
    readFileBounded: (name: string, maximumBytes: number) =>
      invoke(() => directory.readFileBounded(name, maximumBytes)),
    inventory: () => invoke(() => directory.inventory()),
    removeEntry: (name: string) => invoke(() => directory.removeEntry(name)),
  });
}

export function canonicalRunnerVolumeEnrollmentProof(
  proof: UnsignedRunnerVolumeEnrollmentProof,
): Buffer {
  return Buffer.from(
    [
      "bluey-jobs-runner-volume-enrollment-v1",
      `admission_grant_id=${proof.admissionGrantId}`,
      `volume_id=${proof.volumeId}`,
      `worker_id=${proof.workerId}`,
      `provider=${proof.provider}`,
      `provider_resource_id=${proof.providerResourceId}`,
      `resource_fingerprint=${proof.resourceFingerprint}`,
      `enrollment_epoch=${proof.enrollmentEpoch}`,
      `public_key_base64url=${proof.publicKeyBase64url}`,
      `key_fingerprint=${proof.keyFingerprint}`,
      `legacy_artifact_count=${proof.legacyArtifactCount}`,
      `requested_at_ms=${proof.requestedAtMs}`,
      "",
    ].join("\n"),
    "utf8",
  );
}

export function canonicalRunnerVolumeAuthorityProof(
  proof: UnsignedRunnerVolumeAuthorityProof,
): Buffer {
  return Buffer.from(
    [
      "bluey-jobs-runner-volume-authority-v1",
      `version=${proof.version}`,
      `audience=${proof.audience}`,
      `operation=${proof.operation}`,
      `request_id=${proof.requestId}`,
      `volume_id=${proof.volumeId}`,
      `enrollment_epoch=${proof.enrollmentEpoch}`,
      `process_instance_id=${proof.processInstanceId}`,
      `issued_at_ms=${proof.issuedAtMs}`,
      `payload_sha256=${proof.payloadSha256}`,
      "",
    ].join("\n"),
    "utf8",
  );
}

export function runnerVolumeHttpPayloadSha256(
  path: string,
  workerId: string,
  extensions: readonly string[],
): string {
  const bytes = Buffer.from(
    [
      "bluey-jobs-runner-volume-http-payload-v1",
      "method=POST",
      `path=${path}`,
      `worker_id=${workerId}`,
      ...extensions,
      "",
    ].join("\n"),
    "utf8",
  );
  return createHash("sha256").update(bytes).digest("hex");
}

export function parseRunnerVolumeServerCommandKeys(
  encoded: string,
): ReadonlyMap<string, string> {
  let input: unknown;
  try {
    input = JSON.parse(encoded) as unknown;
  } catch {
    throw configurationError();
  }
  if (!isRecord(input) || Object.keys(input).length === 0)
    throw configurationError();
  const keys = new Map<string, string>();
  for (const [keyId, value] of Object.entries(input)) {
    safeIdentifier(keyId);
    if (typeof value !== "string") throw configurationError();
    decodeCanonicalBase64Url(value, 32);
    keys.set(keyId, value);
  }
  return keys;
}

interface LocalPurgeTombstone {
  readonly purgeSubjectSha256: string;
  readonly purgeGeneration: number;
  readonly ack: RunnerVolumePurgeAck;
}

function parseLocalPurgeTombstone(
  encoded: Buffer,
  expectedSubjectSha256: string,
  identity: RunnerVolumeIdentity,
): LocalPurgeTombstone {
  try {
    const value: unknown = JSON.parse(encoded.toString("utf8"));
    if (
      !isRecord(value) ||
      !hasExactKeys(value, [
        "ack",
        "audience",
        "commandId",
        "commandSha256",
        "localSignature",
        "purgeGeneration",
        "purgeSubjectSha256",
        "requestId",
        "retentionPolicy",
        "version",
        "volumeId",
        "volumeKeyFingerprint",
      ]) ||
      value.version !== 1 ||
      value.audience !== TOMBSTONE_AUDIENCE ||
      typeof value.volumeId !== "string" ||
      typeof value.volumeKeyFingerprint !== "string" ||
      typeof value.requestId !== "string" ||
      typeof value.commandId !== "string" ||
      typeof value.commandSha256 !== "string" ||
      typeof value.purgeSubjectSha256 !== "string" ||
      typeof value.purgeGeneration !== "number" ||
      value.retentionPolicy !== TOMBSTONE_RETENTION_POLICY ||
      typeof value.localSignature !== "string"
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    requireLocalSafeIdentifier(value.requestId);
    requireLocalSafeIdentifier(value.commandId);
    requireLocalSha256(value.volumeKeyFingerprint);
    requireLocalSha256(value.commandSha256);
    requireLocalSha256(value.purgeSubjectSha256);
    requireLocalPositiveInteger(value.purgeGeneration);
    decodeCanonicalBase64Url(value.volumeId, 32);
    decodeCanonicalBase64Url(value.localSignature, 64);
    const ack = parseLocalPurgeAck(value.ack, identity);
    const unsigned = {
      version: 1 as const,
      audience: TOMBSTONE_AUDIENCE,
      volumeId: value.volumeId,
      volumeKeyFingerprint: value.volumeKeyFingerprint,
      requestId: value.requestId,
      commandId: value.commandId,
      commandSha256: value.commandSha256,
      purgeSubjectSha256: value.purgeSubjectSha256,
      purgeGeneration: value.purgeGeneration,
      retentionPolicy: TOMBSTONE_RETENTION_POLICY,
      ack,
    };
    const tombstone = { ...unsigned, localSignature: value.localSignature };
    const localCanonical = Buffer.from(
      `${TOMBSTONE_AUDIENCE}\n${JSON.stringify(unsigned)}\n`,
      "utf8",
    );
    if (
      !verifyEd25519(
        identity.publicKeyRaw,
        localCanonical,
        value.localSignature,
      ) ||
      !encoded.equals(Buffer.from(`${JSON.stringify(tombstone)}\n`, "utf8")) ||
      value.volumeId !== identity.volumeId ||
      value.volumeKeyFingerprint !== identity.publicKeyFingerprint ||
      value.purgeSubjectSha256 !== expectedSubjectSha256 ||
      ack.requestId !== value.requestId ||
      ack.commandId !== value.commandId ||
      ack.commandSha256 !== value.commandSha256 ||
      ack.targetVolumeId !== value.volumeId ||
      ack.targetKeyFingerprint !== value.volumeKeyFingerprint ||
      ack.enrollmentEpoch !== ENROLLMENT_EPOCH ||
      ack.purgeSubjectSha256 !== value.purgeSubjectSha256 ||
      ack.purgeGeneration !== value.purgeGeneration
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return {
      purgeSubjectSha256: value.purgeSubjectSha256,
      purgeGeneration: value.purgeGeneration,
      ack,
    };
  } catch (error) {
    if (error instanceof RunnerVolumePurgeError) throw error;
    throw new RunnerVolumePurgeError("corrupt_state");
  }
}

function parseLocalPurgeAck(
  value: unknown,
  identity: RunnerVolumeIdentity,
): RunnerVolumePurgeAck {
  const ack = parseRunnerVolumePurgeAck(value);
  const { signature, ...unsigned } = ack;
  if (
    !verifyEd25519(
      identity.publicKeyRaw,
      canonicalRunnerVolumePurgeAck(unsigned),
      signature,
    )
  ) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
  return ack;
}

function requireLocalSafeIdentifier(value: string): void {
  if (!SAFE_IDENTIFIER_PATTERN.test(value)) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
}

function requireLocalSha256(value: string): void {
  if (!SHA256_PATTERN.test(value))
    throw new RunnerVolumePurgeError("corrupt_state");
}

function requireLocalPositiveInteger(value: number): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
}

function requireLocalNonNegativeInteger(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
}

function parseStoredEnrollmentProof(
  encoded: Buffer,
  expected: ReturnType<RunnerVolumeClient["enrollmentBinding"]>,
  identity: RunnerVolumeIdentity,
): RunnerVolumeEnrollmentProof {
  try {
    const value: unknown = JSON.parse(encoded.toString("utf8"));
    if (
      !isRecord(value) ||
      !hasExactKeys(value, [
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
      ])
    ) {
      throw new Error("invalid enrollment");
    }
    const proof = value as unknown as RunnerVolumeEnrollmentProof;
    if (
      proof.admissionGrantId !== expected.admissionGrantId ||
      proof.volumeId !== expected.volumeId ||
      proof.workerId !== expected.workerId ||
      proof.provider !== expected.provider ||
      proof.providerResourceId !== expected.providerResourceId ||
      proof.resourceFingerprint !== expected.resourceFingerprint ||
      proof.legacyArtifactCount !== expected.legacyArtifactCount ||
      proof.enrollmentEpoch !== ENROLLMENT_EPOCH ||
      !Number.isSafeInteger(proof.requestedAtMs) ||
      proof.requestedAtMs < 0 ||
      typeof proof.signature !== "string"
    ) {
      throw new Error("conflicting enrollment");
    }
    decodeCanonicalBase64Url(proof.publicKeyBase64url, 32);
    decodeCanonicalBase64Url(proof.signature, 64);
    const { signature, ...unsigned } = proof;
    if (
      proof.publicKeyBase64url !== identity.publicKeyRaw ||
      proof.keyFingerprint !== identity.publicKeyFingerprint ||
      !verifyEd25519(
        identity.publicKeyRaw,
        canonicalRunnerVolumeEnrollmentProof(unsigned),
        signature,
      ) ||
      !SHA256_PATTERN.test(proof.keyFingerprint) ||
      !encoded.equals(Buffer.from(`${JSON.stringify(proof)}\n`, "utf8"))
    ) {
      throw new Error("noncanonical enrollment");
    }
    return proof;
  } catch {
    throw new RunnerVolumeClientError("enroll", "invalid_response");
  }
}

function parseEnrollmentResponse(value: unknown): RunnerVolumeRecord {
  const record = exactRecord(
    value,
    [
      "activeInstanceId",
      "admissionGrantId",
      "currentEpoch",
      "disposition",
      "enrolledAtMs",
      "enrollmentGeneration",
      "instanceLeaseExpiresAtMs",
      "keyFingerprint",
      "lastSeenAtMs",
      "legacyArtifactCount",
      "provider",
      "providerResourceId",
      "publicKeyBase64url",
      "reconciledTombstoneGeneration",
      "requiredTombstoneGeneration",
      "resourceFingerprint",
      "status",
      "updatedAtMs",
      "volumeId",
      "workerId",
    ],
    "enroll",
  );
  for (const field of [
    "currentEpoch",
    "enrollmentGeneration",
    "requiredTombstoneGeneration",
    "reconciledTombstoneGeneration",
    "legacyArtifactCount",
    "enrolledAtMs",
    "lastSeenAtMs",
    "updatedAtMs",
  ] as const) {
    assertNonNegativeInteger(record[field], "enroll");
  }
  if (
    typeof record.volumeId !== "string" ||
    typeof record.workerId !== "string" ||
    typeof record.provider !== "string" ||
    typeof record.providerResourceId !== "string" ||
    typeof record.resourceFingerprint !== "string" ||
    typeof record.status !== "string" ||
    typeof record.admissionGrantId !== "string" ||
    typeof record.publicKeyBase64url !== "string" ||
    typeof record.keyFingerprint !== "string" ||
    (record.activeInstanceId !== null &&
      typeof record.activeInstanceId !== "string") ||
    (record.instanceLeaseExpiresAtMs !== null &&
      (!Number.isSafeInteger(record.instanceLeaseExpiresAtMs) ||
        Number(record.instanceLeaseExpiresAtMs) < 0)) ||
    (record.disposition !== "applied" && record.disposition !== "replay")
  ) {
    throw new RunnerVolumeClientError("enroll", "invalid_response");
  }
  return record as unknown as RunnerVolumeRecord;
}

function parseInstanceLease(value: unknown): RunnerVolumeInstanceLease {
  const record = exactRecord(
    value,
    [
      "disposition",
      "enrollmentEpoch",
      "leaseExpiresAtMs",
      "processInstanceId",
      "volumeId",
    ],
    "instance_claim",
  );
  if (
    typeof record.volumeId !== "string" ||
    record.enrollmentEpoch !== ENROLLMENT_EPOCH ||
    typeof record.processInstanceId !== "string" ||
    !Number.isSafeInteger(record.leaseExpiresAtMs) ||
    Number(record.leaseExpiresAtMs) <= Date.now() ||
    (record.disposition !== "applied" && record.disposition !== "replay")
  ) {
    throw new RunnerVolumeClientError("instance_claim", "invalid_response");
  }
  return record as unknown as RunnerVolumeInstanceLease;
}

function parsePollResponse(
  value: unknown,
  pinnedKeys: ReadonlyMap<string, string>,
  pollLimit: number,
): RunnerVolumePollResponse {
  const record = exactRecord(
    value,
    [
      "commands",
      "enrollmentGeneration",
      "nextCommandCursor",
      "predecessorAttestationGeneration",
      "predecessorAttestationSha256",
      "ready",
      "reconciledTombstoneGeneration",
      "requiredTombstoneGeneration",
      "serverCommandKeys",
      "storageAttestationRequired",
    ],
    "purge_poll",
  );
  if (
    !Array.isArray(record.commands) ||
    record.commands.length > pollLimit ||
    (record.nextCommandCursor !== null &&
      (typeof record.nextCommandCursor !== "string" ||
        !SAFE_IDENTIFIER_PATTERN.test(record.nextCommandCursor))) ||
    typeof record.ready !== "boolean" ||
    typeof record.storageAttestationRequired !== "boolean" ||
    typeof record.predecessorAttestationSha256 !== "string" ||
    !SHA256_PATTERN.test(record.predecessorAttestationSha256) ||
    !isRecord(record.serverCommandKeys) ||
    Object.keys(record.serverCommandKeys).length === 0
  ) {
    throw new RunnerVolumeClientError("purge_poll", "invalid_response");
  }
  for (const [keyId, publicKey] of Object.entries(record.serverCommandKeys)) {
    if (typeof publicKey !== "string" || pinnedKeys.get(keyId) !== publicKey) {
      throw new RunnerVolumeClientError("purge_poll", "invalid_response");
    }
  }
  for (const command of record.commands) {
    if (
      !isRecord(command) ||
      typeof command.serverKeyId !== "string" ||
      record.serverCommandKeys[command.serverKeyId] !==
        pinnedKeys.get(command.serverKeyId)
    ) {
      throw new RunnerVolumeClientError("purge_poll", "invalid_response");
    }
  }
  const lastCommand = record.commands.at(-1);
  if (
    record.nextCommandCursor !== null &&
    (record.commands.length !== pollLimit ||
      !isRecord(lastCommand) ||
      lastCommand.commandId !== record.nextCommandCursor)
  ) {
    throw new RunnerVolumeClientError("purge_poll", "invalid_response");
  }
  assertNonNegativeInteger(record.requiredTombstoneGeneration, "purge_poll");
  assertNonNegativeInteger(record.reconciledTombstoneGeneration, "purge_poll");
  assertNonNegativeInteger(record.enrollmentGeneration, "purge_poll");
  assertNonNegativeInteger(
    record.predecessorAttestationGeneration,
    "purge_poll",
  );
  if (
    Number(record.enrollmentGeneration) < 1 ||
    (record.predecessorAttestationGeneration === 0 &&
      record.predecessorAttestationSha256 !==
        RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256)
  ) {
    throw new RunnerVolumeClientError("purge_poll", "invalid_response");
  }
  return record as unknown as RunnerVolumePollResponse;
}

function parseAckResponse(value: unknown): RunnerVolumeAckResponse {
  const record = exactRecord(
    value,
    ["disposition", "reconciledTombstoneGeneration", "status"],
    "purge_ack",
  );
  if (
    (record.disposition !== "applied" && record.disposition !== "replay") ||
    !isRecord(record.status)
  ) {
    throw new RunnerVolumeClientError("purge_ack", "invalid_response");
  }
  assertNonNegativeInteger(record.reconciledTombstoneGeneration, "purge_ack");
  return record as unknown as RunnerVolumeAckResponse;
}

function parseStorageAttestationResponse(
  value: unknown,
): RunnerVolumeStorageAttestationResponse {
  const record = exactRecord(
    value,
    [
      "attestationGeneration",
      "attestationSha256",
      "disposition",
      "fleetAttestationGeneration",
      "volumeStatus",
    ],
    "storage_attestation",
  );
  if (
    (record.disposition !== "applied" && record.disposition !== "replay") ||
    typeof record.attestationSha256 !== "string" ||
    !SHA256_PATTERN.test(record.attestationSha256) ||
    typeof record.volumeStatus !== "string" ||
    !record.volumeStatus
  ) {
    throw new RunnerVolumeClientError(
      "storage_attestation",
      "invalid_response",
    );
  }
  assertNonNegativeInteger(record.attestationGeneration, "storage_attestation");
  assertNonNegativeInteger(
    record.fleetAttestationGeneration,
    "storage_attestation",
  );
  return record as unknown as RunnerVolumeStorageAttestationResponse;
}

function assertEnrollmentBinding(
  record: RunnerVolumeRecord,
  expected: {
    identity: RunnerVolumeIdentity;
    workerId: string;
    provider: string;
    providerResourceId: string;
    resourceFingerprint: string;
    admissionGrantId: string;
    legacyArtifactCount: number;
  },
): void {
  if (
    record.volumeId !== expected.identity.volumeId ||
    record.publicKeyBase64url !== expected.identity.publicKeyRaw ||
    record.keyFingerprint !== expected.identity.publicKeyFingerprint ||
    record.currentEpoch !== ENROLLMENT_EPOCH ||
    record.workerId !== expected.workerId ||
    record.provider !== expected.provider ||
    record.providerResourceId !== expected.providerResourceId ||
    record.resourceFingerprint !== expected.resourceFingerprint ||
    record.admissionGrantId !== expected.admissionGrantId ||
    record.legacyArtifactCount !== expected.legacyArtifactCount
  ) {
    throw new RunnerVolumeClientError("enroll", "invalid_response");
  }
}

function assertInstanceBinding(
  lease: RunnerVolumeInstanceLease,
  volumeId: string,
  processInstanceId: string,
): void {
  if (
    lease.volumeId !== volumeId ||
    lease.enrollmentEpoch !== ENROLLMENT_EPOCH ||
    lease.processInstanceId !== processInstanceId
  ) {
    throw new RunnerVolumeClientError("instance_claim", "invalid_response");
  }
}

async function readBounded(
  response: Response,
  maximumBytes: number,
  operation: "enroll" | RunnerVolumeOperation | "purge_ack",
): Promise<string> {
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maximumBytes) {
    await response.body?.cancel().catch(() => undefined);
    throw new RunnerVolumeClientError(
      operation,
      "response_too_large",
      response.status,
    );
  }
  if (!response.body) return "";
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read().catch(() => {
      throw new RunnerVolumeClientError(
        operation,
        "request_failed",
        response.status,
      );
    });
    if (done) break;
    total += value.byteLength;
    if (total > maximumBytes) {
      await reader.cancel().catch(() => undefined);
      throw new RunnerVolumeClientError(
        operation,
        "response_too_large",
        response.status,
      );
    }
    chunks.push(value);
  }
  return Buffer.concat(chunks.map((chunk) => Buffer.from(chunk))).toString(
    "utf8",
  );
}

function normalizedOrigin(rawOrigin: string): string {
  try {
    const url = new URL(rawOrigin);
    const loopback = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]).has(
      url.hostname,
    );
    if (
      !/^https?:$/.test(url.protocol) ||
      (url.protocol === "http:" && !loopback) ||
      url.username ||
      url.password ||
      url.pathname !== "/" ||
      url.search ||
      url.hash
    ) {
      throw new Error("invalid origin");
    }
    return url.origin;
  } catch {
    throw configurationError();
  }
}

function boundedSigningKey(value: string): string {
  const bytes = Buffer.byteLength(value, "utf8");
  if (bytes < 32 || bytes > 4_096) throw configurationError();
  return value;
}

function boundedText(value: string, maximumBytes: number): string {
  if (
    !value ||
    value.trim() !== value ||
    Buffer.byteLength(value, "utf8") > maximumBytes ||
    /[\u0000-\u001f\u007f]/.test(value)
  ) {
    throw configurationError();
  }
  return value;
}

function safeIdentifier(value: string): string {
  if (!SAFE_IDENTIFIER_PATTERN.test(value)) throw configurationError();
  return value;
}

function boundedInteger(
  value: number,
  minimum: number,
  maximum: number,
): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw configurationError();
  }
  return value;
}

function assertTimestamp(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) throw configurationError();
}

function assertNonNegativeInteger(
  value: unknown,
  operation: "enroll" | "purge_poll" | "purge_ack" | "storage_attestation",
): void {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    throw new RunnerVolumeClientError(operation, "invalid_response");
  }
}

function exactRecord(
  value: unknown,
  keys: readonly string[],
  operation:
    | "enroll"
    | "instance_claim"
    | "purge_poll"
    | "purge_ack"
    | "storage_attestation",
): Record<string, unknown> {
  if (!isRecord(value) || !hasExactKeys(value, keys)) {
    throw new RunnerVolumeClientError(operation, "invalid_response");
  }
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort((left, right) =>
    left.localeCompare(right),
  );
  const wanted = [...expected].sort((left, right) => left.localeCompare(right));
  return (
    actual.length === wanted.length &&
    actual.every((key, index) => key === wanted[index])
  );
}

function configurationError(): RunnerVolumeClientError {
  return new RunnerVolumeClientError("configuration", "configuration");
}

function isTransientControlFailure(error: unknown): boolean {
  return (
    error instanceof RunnerVolumeClientError &&
    (error.code === "not_ready" ||
      error.code === "timed_out" ||
      (error.code === "request_failed" &&
        (error.status === undefined || error.status >= 500)))
  );
}

function isFatalLocalStorageFailure(error: unknown): boolean {
  return (
    error instanceof SubjectStorageManagerError ||
    error instanceof RunnerVolumeStorageAttestationError ||
    error instanceof LegacyRunnerStorageError ||
    (error instanceof NativeRunnerStorageError &&
      new Set([
        "configuration",
        "native_addon_missing",
        "native_contract_invalid",
        "path_escape",
        "root_changed",
        "unsafe_entry",
        "unsafe_permissions",
        "unsupported_platform",
      ]).has(error.code))
  );
}
