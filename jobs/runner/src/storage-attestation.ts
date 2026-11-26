import { createHash } from "node:crypto";
import {
  assertAccountResidencyLocator,
  type AccountResidencyLocator,
} from "./account-residency.js";
import { EMPTY_LEGACY_ARTIFACT_SET_SHA256 } from "./legacy-runner-storage.js";
import type { CurrentSubjectStorageInventory } from "./subject-storage-manager.js";
import {
  decodeCanonicalBase64Url,
  signEd25519,
  type RunnerVolumeIdentity,
  verifyEd25519,
} from "./volume-identity.js";

export const RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE =
  "bluey-jobs-runner-volume-storage-attestation-v1" as const;
export const RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256 =
  "af14da54b5862fedcda27f7b1ca9ccd2be4271870efa7707d6f2e308efd874c2" as const;

const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const SAFE_IDENTIFIER_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:+-]{0,127}$/;
const RUNNER_BUILD_PATTERN =
  /^runner-(0|[1-9][0-9]{0,8})(?:\.(0|[1-9][0-9]{0,8}))?$/;
const DECIMAL_U64_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const MAXIMUM_U64 = 18_446_744_073_709_551_615n;

export class RunnerVolumeStorageAttestationError extends Error {
  constructor(
    readonly code:
      | "invalid_attestation"
      | "invalid_inventory"
      | "invalid_locator_set",
  ) {
    super(
      {
        invalid_attestation: "The runner current-storage attestation is invalid.",
        invalid_inventory: "The runner current-storage inventory is invalid.",
        invalid_locator_set: "The runner current-storage locator set is invalid.",
      }[code],
    );
    this.name = "RunnerVolumeStorageAttestationError";
  }
}

export interface UnsignedRunnerVolumeStorageAttestation {
  readonly version: 1;
  readonly audience: typeof RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE;
  readonly attestationId: string;
  readonly volumeId: string;
  readonly volumeKeyFingerprint: string;
  readonly resourceFingerprint: string;
  readonly enrollmentEpoch: number;
  readonly enrollmentGeneration: number;
  readonly processInstanceId: string;
  readonly predecessorAttestationGeneration: number;
  readonly predecessorAttestationSha256: string;
  readonly requiredTombstoneGeneration: number;
  readonly reconciledTombstoneGeneration: number;
  readonly storageEvidenceVersion: 2;
  readonly subjectStorageLayoutVersion: 2;
  readonly rootDeviceId: string;
  readonly rootLinkCount: number;
  readonly rootEntryCount: number;
  readonly rootFileBytes: string;
  readonly rootSha256: string;
  readonly subjectStorageSubjectCount: number;
  readonly subjectStorageSubjectSetSha256: string;
  readonly subjectStorageScopeCount: number;
  readonly subjectStorageCompleteRootEntryCount: number;
  readonly subjectStorageCompleteRootFileBytes: string;
  readonly subjectStorageCompleteRootSha256: string;
  readonly locatorCount: number;
  readonly residentLocatorCount: number;
  readonly locatorSetSha256: string;
  readonly legacyInventoryVersion: 1;
  readonly legacyArtifactCount: number;
  readonly legacyArtifactBytes: string;
  readonly legacyArtifactSetSha256: string;
  readonly unclassifiedRootCount: number;
  readonly runnerBuildId: string;
  readonly observedAtMs: number;
}

export interface RunnerVolumeStorageAttestation
  extends UnsignedRunnerVolumeStorageAttestation {
  readonly signature: string;
}

export interface RunnerVolumeLocatorSetEvidence {
  readonly count: number;
  readonly residentCount: number;
  readonly sha256: string;
}

export interface CreateRunnerVolumeStorageAttestationInput {
  readonly identity: RunnerVolumeIdentity;
  readonly attestationId: string;
  readonly resourceFingerprint: string;
  readonly enrollmentEpoch: number;
  readonly enrollmentGeneration: number;
  readonly processInstanceId: string;
  readonly predecessorAttestationGeneration: number;
  readonly predecessorAttestationSha256: string;
  readonly requiredTombstoneGeneration: number;
  readonly reconciledTombstoneGeneration: number;
  readonly inventory: CurrentSubjectStorageInventory;
  readonly locatorSet: RunnerVolumeLocatorSetEvidence;
  readonly runnerBuildId: string;
  readonly observedAtMs: number;
}

export function runnerVolumeLocatorSetEvidence(
  locators: readonly AccountResidencyLocator[],
  identity: Pick<
    RunnerVolumeIdentity,
    "publicKeyFingerprint" | "publicKeyRaw" | "volumeId"
  >,
  purgedSubjects: ReadonlySet<string>,
): RunnerVolumeLocatorSetEvidence {
  try {
    if (!Array.isArray(locators) || !(purgedSubjects instanceof Set)) {
      throw new Error("invalid locator set");
    }
    const ordered = [...locators].sort(compareLocators);
    const seen = new Set<string>();
    let residentCount = 0;
    const digest = createHash("sha256");
    digest.update("bluey-jobs-runner-volume-locator-set-v1\n", "utf8");
    for (const locator of ordered) {
      assertAccountResidencyLocator(locator, identity);
      const key = `${locator.subjectSha256}\0${locator.kind}\0${locator.scope}`;
      if (seen.has(key)) throw new Error("duplicate locator");
      seen.add(key);
      if (!purgedSubjects.has(locator.subjectSha256)) residentCount += 1;
      digest.update(`version=${locator.version}\n`, "utf8");
      digest.update(`audience=${locator.audience}\n`, "utf8");
      digest.update(`volume_id=${locator.volumeId}\n`, "utf8");
      digest.update(
        `volume_key_fingerprint=${locator.volumeKeyFingerprint}\n`,
        "utf8",
      );
      digest.update(`subject_sha256=${locator.subjectSha256}\n`, "utf8");
      digest.update(`kind=${locator.kind}\n`, "utf8");
      digest.update(`scope=${locator.scope}\n`, "utf8");
      digest.update(
        `artifact_families=${locator.artifactFamilies.join(",")}\n`,
        "utf8",
      );
      digest.update(`signature=${locator.signature}\n`, "utf8");
    }
    return Object.freeze({
      count: ordered.length,
      residentCount,
      sha256: digest.digest("hex"),
    });
  } catch (error) {
    if (error instanceof RunnerVolumeStorageAttestationError) throw error;
    throw new RunnerVolumeStorageAttestationError("invalid_locator_set");
  }
}

export function createRunnerVolumeStorageAttestation(
  input: CreateRunnerVolumeStorageAttestationInput,
): RunnerVolumeStorageAttestation {
  const { inventory, locatorSet } = input;
  if (
    inventory.subjectStorage.layoutVersion !== 2 ||
    inventory.root.linkCount < 1 ||
    inventory.legacy.version !== 1 ||
    inventory.legacy.legacyArtifactCount !== 0 ||
    inventory.legacy.legacyArtifactBytes !== 0 ||
    inventory.legacy.legacyArtifactSetSha256 !==
      EMPTY_LEGACY_ARTIFACT_SET_SHA256 ||
    inventory.legacy.unclassifiedRootPaths.length !== 0 ||
    inventory.subjectStorage.scopeCount !== locatorSet.residentCount ||
    locatorSet.residentCount > locatorSet.count
  ) {
    throw new RunnerVolumeStorageAttestationError("invalid_inventory");
  }
  const unsigned: UnsignedRunnerVolumeStorageAttestation = {
    version: 1,
    audience: RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE,
    attestationId: input.attestationId,
    volumeId: input.identity.volumeId,
    volumeKeyFingerprint: input.identity.publicKeyFingerprint,
    resourceFingerprint: input.resourceFingerprint,
    enrollmentEpoch: input.enrollmentEpoch,
    enrollmentGeneration: input.enrollmentGeneration,
    processInstanceId: input.processInstanceId,
    predecessorAttestationGeneration:
      input.predecessorAttestationGeneration,
    predecessorAttestationSha256: input.predecessorAttestationSha256,
    requiredTombstoneGeneration: input.requiredTombstoneGeneration,
    reconciledTombstoneGeneration: input.reconciledTombstoneGeneration,
    storageEvidenceVersion: 2,
    subjectStorageLayoutVersion: 2,
    rootDeviceId: inventory.root.deviceId,
    rootLinkCount: inventory.root.linkCount,
    rootEntryCount: inventory.root.entryCount,
    rootFileBytes: inventory.root.fileBytes,
    rootSha256: inventory.root.sha256,
    subjectStorageSubjectCount: inventory.subjectStorage.subjectCount,
    subjectStorageSubjectSetSha256:
      inventory.subjectStorage.subjectSetSha256,
    subjectStorageScopeCount: inventory.subjectStorage.scopeCount,
    subjectStorageCompleteRootEntryCount:
      inventory.subjectStorage.completeRoot.entryCount,
    subjectStorageCompleteRootFileBytes:
      inventory.subjectStorage.completeRoot.fileBytes,
    subjectStorageCompleteRootSha256:
      inventory.subjectStorage.completeRoot.sha256,
    locatorCount: locatorSet.count,
    residentLocatorCount: locatorSet.residentCount,
    locatorSetSha256: locatorSet.sha256,
    legacyInventoryVersion: 1,
    legacyArtifactCount: inventory.legacy.legacyArtifactCount,
    legacyArtifactBytes: String(inventory.legacy.legacyArtifactBytes),
    legacyArtifactSetSha256: inventory.legacy.legacyArtifactSetSha256,
    unclassifiedRootCount: inventory.legacy.unclassifiedRootPaths.length,
    runnerBuildId: input.runnerBuildId,
    observedAtMs: input.observedAtMs,
  };
  const attestation = Object.freeze({
    ...unsigned,
    signature: signEd25519(
      input.identity.privateKey,
      canonicalRunnerVolumeStorageAttestation(unsigned),
    ),
  });
  return parseRunnerVolumeStorageAttestation(
    attestation,
    input.identity.publicKeyRaw,
  );
}

export function canonicalRunnerVolumeStorageAttestation(
  attestation: UnsignedRunnerVolumeStorageAttestation,
): Buffer {
  return Buffer.from(
    [
      RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE,
      `version=${attestation.version}`,
      `audience=${attestation.audience}`,
      `attestation_id=${attestation.attestationId}`,
      `volume_id=${attestation.volumeId}`,
      `volume_key_fingerprint=${attestation.volumeKeyFingerprint}`,
      `resource_fingerprint=${attestation.resourceFingerprint}`,
      `enrollment_epoch=${attestation.enrollmentEpoch}`,
      `enrollment_generation=${attestation.enrollmentGeneration}`,
      `process_instance_id=${attestation.processInstanceId}`,
      `predecessor_attestation_generation=${attestation.predecessorAttestationGeneration}`,
      `predecessor_attestation_sha256=${attestation.predecessorAttestationSha256}`,
      `required_tombstone_generation=${attestation.requiredTombstoneGeneration}`,
      `reconciled_tombstone_generation=${attestation.reconciledTombstoneGeneration}`,
      `storage_evidence_version=${attestation.storageEvidenceVersion}`,
      `subject_storage_layout_version=${attestation.subjectStorageLayoutVersion}`,
      `root_device_id=${attestation.rootDeviceId}`,
      `root_link_count=${attestation.rootLinkCount}`,
      `root_entry_count=${attestation.rootEntryCount}`,
      `root_file_bytes=${attestation.rootFileBytes}`,
      `root_sha256=${attestation.rootSha256}`,
      `subject_storage_subject_count=${attestation.subjectStorageSubjectCount}`,
      `subject_storage_subject_set_sha256=${attestation.subjectStorageSubjectSetSha256}`,
      `subject_storage_scope_count=${attestation.subjectStorageScopeCount}`,
      `subject_storage_complete_root_entry_count=${attestation.subjectStorageCompleteRootEntryCount}`,
      `subject_storage_complete_root_file_bytes=${attestation.subjectStorageCompleteRootFileBytes}`,
      `subject_storage_complete_root_sha256=${attestation.subjectStorageCompleteRootSha256}`,
      `locator_count=${attestation.locatorCount}`,
      `resident_locator_count=${attestation.residentLocatorCount}`,
      `locator_set_sha256=${attestation.locatorSetSha256}`,
      `legacy_inventory_version=${attestation.legacyInventoryVersion}`,
      `legacy_artifact_count=${attestation.legacyArtifactCount}`,
      `legacy_artifact_bytes=${attestation.legacyArtifactBytes}`,
      `legacy_artifact_set_sha256=${attestation.legacyArtifactSetSha256}`,
      `unclassified_root_count=${attestation.unclassifiedRootCount}`,
      `runner_build_id=${attestation.runnerBuildId}`,
      `observed_at_ms=${attestation.observedAtMs}`,
      "",
    ].join("\n"),
    "utf8",
  );
}

export function runnerVolumeStorageAttestationSha256(
  attestation: RunnerVolumeStorageAttestation,
): string {
  return createHash("sha256")
    .update(canonicalRunnerVolumeStorageAttestation(attestation))
    .update(`signature=${attestation.signature}\n`, "utf8")
    .digest("hex");
}

export function parseRunnerVolumeStorageAttestation(
  value: unknown,
  publicKeyRaw?: string,
): RunnerVolumeStorageAttestation {
  try {
    if (!isRecord(value) || !hasExactKeys(value, ATTESTATION_KEYS)) {
      throw new Error("invalid fields");
    }
    if (
      value.version !== 1 ||
      value.audience !== RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE ||
      value.storageEvidenceVersion !== 2 ||
      value.subjectStorageLayoutVersion !== 2 ||
      value.legacyInventoryVersion !== 1 ||
      typeof value.attestationId !== "string" ||
      typeof value.volumeId !== "string" ||
      typeof value.volumeKeyFingerprint !== "string" ||
      typeof value.resourceFingerprint !== "string" ||
      typeof value.processInstanceId !== "string" ||
      typeof value.predecessorAttestationSha256 !== "string" ||
      typeof value.rootDeviceId !== "string" ||
      typeof value.rootFileBytes !== "string" ||
      typeof value.rootSha256 !== "string" ||
      typeof value.subjectStorageSubjectSetSha256 !== "string" ||
      typeof value.subjectStorageCompleteRootFileBytes !== "string" ||
      typeof value.subjectStorageCompleteRootSha256 !== "string" ||
      typeof value.locatorSetSha256 !== "string" ||
      typeof value.legacyArtifactBytes !== "string" ||
      typeof value.legacyArtifactSetSha256 !== "string" ||
      typeof value.runnerBuildId !== "string" ||
      typeof value.signature !== "string"
    ) {
      throw new Error("invalid types");
    }
    for (const identifier of [
      value.attestationId,
      value.rootDeviceId,
    ]) {
      requireIdentifier(identifier);
    }
    requireRunnerBuildId(value.runnerBuildId);
    decodeCanonicalBase64Url(value.volumeId, 32);
    decodeCanonicalBase64Url(value.processInstanceId, 32);
    decodeCanonicalBase64Url(value.signature, 64);
    for (const sha256 of [
      value.volumeKeyFingerprint,
      value.resourceFingerprint,
      value.predecessorAttestationSha256,
      value.rootSha256,
      value.subjectStorageSubjectSetSha256,
      value.subjectStorageCompleteRootSha256,
      value.locatorSetSha256,
      value.legacyArtifactSetSha256,
    ]) {
      requireSha256(sha256);
    }
    for (const integer of [
      value.enrollmentEpoch,
      value.enrollmentGeneration,
      value.predecessorAttestationGeneration,
      value.requiredTombstoneGeneration,
      value.reconciledTombstoneGeneration,
      value.rootEntryCount,
      value.subjectStorageSubjectCount,
      value.subjectStorageScopeCount,
      value.subjectStorageCompleteRootEntryCount,
      value.locatorCount,
      value.residentLocatorCount,
      value.legacyArtifactCount,
      value.unclassifiedRootCount,
      value.observedAtMs,
    ]) {
      requireNonNegativeSafeInteger(integer);
    }
    for (const decimal of [
      value.rootFileBytes,
      value.subjectStorageCompleteRootFileBytes,
      value.legacyArtifactBytes,
    ]) {
      requireDecimalU64(decimal);
    }
    if (
      !Number.isSafeInteger(value.rootLinkCount) ||
      Number(value.rootLinkCount) < 1 ||
      Number(value.enrollmentEpoch) < 1 ||
      Number(value.enrollmentGeneration) < 1 ||
      value.requiredTombstoneGeneration !==
        value.reconciledTombstoneGeneration ||
      Number(value.residentLocatorCount) > Number(value.locatorCount) ||
      value.subjectStorageScopeCount !== value.residentLocatorCount ||
      Number(value.subjectStorageCompleteRootEntryCount) >
        Number(value.rootEntryCount) ||
      BigInt(value.subjectStorageCompleteRootFileBytes) >
        BigInt(value.rootFileBytes) ||
      value.legacyArtifactCount !== 0 ||
      value.legacyArtifactBytes !== "0" ||
      value.legacyArtifactSetSha256 !== EMPTY_LEGACY_ARTIFACT_SET_SHA256 ||
      value.unclassifiedRootCount !== 0 ||
      (value.predecessorAttestationGeneration === 0) !==
        (value.predecessorAttestationSha256 ===
          RUNNER_VOLUME_STORAGE_ATTESTATION_GENESIS_SHA256)
    ) {
      throw new Error("invalid invariants");
    }
    const attestation = value as unknown as RunnerVolumeStorageAttestation;
    if (publicKeyRaw) {
      const { signature, ...unsigned } = attestation;
      if (
        !verifyEd25519(
          publicKeyRaw,
          canonicalRunnerVolumeStorageAttestation(unsigned),
          signature,
        )
      ) {
        throw new Error("invalid signature");
      }
    }
    return Object.freeze(attestation);
  } catch (error) {
    if (error instanceof RunnerVolumeStorageAttestationError) throw error;
    throw new RunnerVolumeStorageAttestationError("invalid_attestation");
  }
}

const ATTESTATION_KEYS = Object.freeze([
  "attestationId",
  "audience",
  "enrollmentEpoch",
  "enrollmentGeneration",
  "legacyArtifactBytes",
  "legacyArtifactCount",
  "legacyArtifactSetSha256",
  "legacyInventoryVersion",
  "locatorCount",
  "locatorSetSha256",
  "observedAtMs",
  "predecessorAttestationGeneration",
  "predecessorAttestationSha256",
  "processInstanceId",
  "reconciledTombstoneGeneration",
  "requiredTombstoneGeneration",
  "residentLocatorCount",
  "resourceFingerprint",
  "rootDeviceId",
  "rootEntryCount",
  "rootFileBytes",
  "rootLinkCount",
  "rootSha256",
  "runnerBuildId",
  "signature",
  "storageEvidenceVersion",
  "subjectStorageCompleteRootEntryCount",
  "subjectStorageCompleteRootFileBytes",
  "subjectStorageCompleteRootSha256",
  "subjectStorageLayoutVersion",
  "subjectStorageScopeCount",
  "subjectStorageSubjectCount",
  "subjectStorageSubjectSetSha256",
  "unclassifiedRootCount",
  "version",
  "volumeId",
  "volumeKeyFingerprint",
]);

function compareLocators(
  left: AccountResidencyLocator,
  right: AccountResidencyLocator,
): number {
  return (
    compareCanonicalUtf8(left.subjectSha256, right.subjectSha256) ||
    compareCanonicalUtf8(left.kind, right.kind) ||
    compareCanonicalUtf8(left.scope, right.scope)
  );
}

function compareCanonicalUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function requireIdentifier(value: string): void {
  if (!SAFE_IDENTIFIER_PATTERN.test(value)) throw new Error("invalid identifier");
}

function requireRunnerBuildId(value: string): void {
  if (!RUNNER_BUILD_PATTERN.test(value)) {
    throw new Error("invalid runner build id");
  }
}

function requireSha256(value: string): void {
  if (!SHA256_PATTERN.test(value)) throw new Error("invalid sha256");
}

function requireNonNegativeSafeInteger(value: unknown): void {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    throw new Error("invalid integer");
  }
}

function requireDecimalU64(value: string): void {
  if (!DECIMAL_U64_PATTERN.test(value) || BigInt(value) > MAXIMUM_U64) {
    throw new Error("invalid decimal");
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort(compareCanonicalUtf8);
  const wanted = [...expected].sort(compareCanonicalUtf8);
  return (
    actual.length === wanted.length &&
    actual.every((key, index) => key === wanted[index])
  );
}
