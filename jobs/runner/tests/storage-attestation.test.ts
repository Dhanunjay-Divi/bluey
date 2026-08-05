import {
  createHash,
  createPrivateKey,
  createPublicKey,
  type KeyObject,
} from "node:crypto";
import { describe, expect, it } from "vitest";
import { EMPTY_LEGACY_ARTIFACT_SET_SHA256 } from "../src/legacy-runner-storage.js";
import {
  canonicalRunnerVolumeStorageAttestation,
  parseRunnerVolumeStorageAttestation,
  RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE,
  runnerVolumeStorageAttestationSha256,
  type RunnerVolumeStorageAttestation,
  type UnsignedRunnerVolumeStorageAttestation,
} from "../src/storage-attestation.js";
import { signEd25519, verifyEd25519 } from "../src/volume-identity.js";

describe("runner current-storage attestation", () => {
  it("matches the shared Rust and Node canonical signature vector", () => {
    const identity = identityFromSeed(Buffer.alloc(32, 0x0b));
    const unsigned: UnsignedRunnerVolumeStorageAttestation = {
      version: 1,
      audience: RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE,
      attestationId: "attestation-vector-1",
      volumeId: identity.volumeId,
      volumeKeyFingerprint: identity.publicKeyFingerprint,
      resourceFingerprint: sha256("resource-vector"),
      enrollmentEpoch: 1,
      enrollmentGeneration: 7,
      processInstanceId: Buffer.alloc(32, 0x0c).toString("base64url"),
      predecessorAttestationGeneration: 3,
      predecessorAttestationSha256: sha256("previous-attestation"),
      requiredTombstoneGeneration: 5,
      reconciledTombstoneGeneration: 5,
      storageEvidenceVersion: 2,
      subjectStorageLayoutVersion: 2,
      rootDeviceId: "device-vector-1",
      rootLinkCount: 7,
      rootEntryCount: 29,
      rootFileBytes: "4096",
      rootSha256: sha256("root-vector"),
      subjectStorageSubjectCount: 2,
      subjectStorageSubjectSetSha256: sha256("subjects-vector"),
      subjectStorageScopeCount: 3,
      subjectStorageCompleteRootEntryCount: 18,
      subjectStorageCompleteRootFileBytes: "3072",
      subjectStorageCompleteRootSha256: sha256("account-data-vector"),
      locatorCount: 5,
      residentLocatorCount: 3,
      locatorSetSha256: sha256("locators-vector"),
      legacyInventoryVersion: 1,
      legacyArtifactCount: 0,
      legacyArtifactBytes: "0",
      legacyArtifactSetSha256: EMPTY_LEGACY_ARTIFACT_SET_SHA256,
      unclassifiedRootCount: 0,
      runnerBuildId: "runner-602.1",
      observedAtMs: 1_750_000_000_000,
    };
    const signature = signEd25519(
      identity.privateKey,
      canonicalRunnerVolumeStorageAttestation(unsigned),
    );
    const attestation: RunnerVolumeStorageAttestation = {
      ...unsigned,
      signature,
    };
    const vector = {
      volumeId: identity.volumeId,
      publicKeyRaw: identity.publicKeyRaw,
      keyFingerprint: identity.publicKeyFingerprint,
      canonicalSha256: createHash("sha256")
        .update(canonicalRunnerVolumeStorageAttestation(unsigned))
        .digest("hex"),
      signature,
      attestationSha256: runnerVolumeStorageAttestationSha256(attestation),
    };
    expect(vector).toEqual({
      volumeId: "rKICyDCH2xY-a181MC4t5toQkAmP68eLvHkYxXlXN9s",
      publicKeyRaw: "Zr5-Myx6RTMyvZ0Kf32wVfXF7xoGraZtmLOftoEMRzo",
      keyFingerprint:
        "fdf72a088f18f7399e8c52bce448441501f759a595a86980e5d9a422a01e5d55",
      canonicalSha256:
        "6dfe24fee1e8364a314dd3c82e21326f5ff8f7d52f982678bcfa1525291ab2cf",
      signature:
        "1oGMUm4-B6Og2DXENeDTT33CofW2QhsRHcDhesVSxLB-43PJ5f7ymIcGnb0U-4-wxde6PJRNndWqwWmprnqzBw",
      attestationSha256:
        "cbc3eabe0fe038531082be1341e693080f1e9d7ca192c7e8157ff73be099064d",
    });
    expect(
      verifyEd25519(
        identity.publicKeyRaw,
        canonicalRunnerVolumeStorageAttestation(unsigned),
        signature,
      ),
    ).toBe(true);
    expect(
      parseRunnerVolumeStorageAttestation(attestation, identity.publicKeyRaw),
    ).toEqual(attestation);
  });

  it("rejects unknown fields and every signed-field mutation", () => {
    const identity = identityFromSeed(Buffer.alloc(32, 0x0b));
    const attestation = vectorAttestation(identity);
    expect(() =>
      parseRunnerVolumeStorageAttestation(
        { ...attestation, unknown: true },
        identity.publicKeyRaw,
      ),
    ).toThrow("current-storage attestation is invalid");
    expect(() =>
      parseRunnerVolumeStorageAttestation(
        { ...attestation, rootEntryCount: attestation.rootEntryCount + 1 },
        identity.publicKeyRaw,
      ),
    ).toThrow("current-storage attestation is invalid");
    expect(() =>
      parseRunnerVolumeStorageAttestation(
        { ...attestation, legacyArtifactCount: 1 },
        identity.publicKeyRaw,
      ),
    ).toThrow("current-storage attestation is invalid");
  });

  it("matches the server's strict predecessor, root-subset, and build invariants", () => {
    const identity = identityFromSeed(Buffer.alloc(32, 0x0b));
    const attestation = vectorAttestation(identity);
    for (const invalid of [
      {
        ...attestation,
        predecessorAttestationSha256:
          "af14da54b5862fedcda27f7b1ca9ccd2be4271870efa7707d6f2e308efd874c2",
      },
      {
        ...attestation,
        subjectStorageCompleteRootEntryCount: attestation.rootEntryCount + 1,
      },
      {
        ...attestation,
        subjectStorageCompleteRootFileBytes:
          (BigInt(attestation.rootFileBytes) + 1n).toString(),
      },
      { ...attestation, runnerBuildId: "runner-602.01" },
    ]) {
      expect(() => parseRunnerVolumeStorageAttestation(invalid)).toThrow(
        "current-storage attestation is invalid",
      );
    }
  });
});

function vectorAttestation(identity: ReturnType<typeof identityFromSeed>) {
  const unsigned: UnsignedRunnerVolumeStorageAttestation = {
    version: 1,
    audience: RUNNER_VOLUME_STORAGE_ATTESTATION_AUDIENCE,
    attestationId: "attestation-vector-1",
    volumeId: identity.volumeId,
    volumeKeyFingerprint: identity.publicKeyFingerprint,
    resourceFingerprint: sha256("resource-vector"),
    enrollmentEpoch: 1,
    enrollmentGeneration: 7,
    processInstanceId: Buffer.alloc(32, 0x0c).toString("base64url"),
    predecessorAttestationGeneration: 3,
    predecessorAttestationSha256: sha256("previous-attestation"),
    requiredTombstoneGeneration: 5,
    reconciledTombstoneGeneration: 5,
    storageEvidenceVersion: 2,
    subjectStorageLayoutVersion: 2,
    rootDeviceId: "device-vector-1",
    rootLinkCount: 7,
    rootEntryCount: 29,
    rootFileBytes: "4096",
    rootSha256: sha256("root-vector"),
    subjectStorageSubjectCount: 2,
    subjectStorageSubjectSetSha256: sha256("subjects-vector"),
    subjectStorageScopeCount: 3,
    subjectStorageCompleteRootEntryCount: 18,
    subjectStorageCompleteRootFileBytes: "3072",
    subjectStorageCompleteRootSha256: sha256("account-data-vector"),
    locatorCount: 5,
    residentLocatorCount: 3,
    locatorSetSha256: sha256("locators-vector"),
    legacyInventoryVersion: 1,
    legacyArtifactCount: 0,
    legacyArtifactBytes: "0",
    legacyArtifactSetSha256: EMPTY_LEGACY_ARTIFACT_SET_SHA256,
    unclassifiedRootCount: 0,
    runnerBuildId: "runner-602.1",
    observedAtMs: 1_750_000_000_000,
  };
  return {
    ...unsigned,
    signature: signEd25519(
      identity.privateKey,
      canonicalRunnerVolumeStorageAttestation(unsigned),
    ),
  };
}

function identityFromSeed(seed: Buffer): {
  readonly volumeId: string;
  readonly publicKeyRaw: string;
  readonly publicKeyFingerprint: string;
  readonly privateKey: KeyObject;
} {
  const privateKey = createPrivateKey({
    key: Buffer.concat([
      Buffer.from("302e020100300506032b657004220420", "hex"),
      seed,
    ]),
    format: "der",
    type: "pkcs8",
  });
  const publicKey = createPublicKey(privateKey).export({ format: "jwk" });
  if (!publicKey.x) throw new Error("missing public key");
  const publicBytes = Buffer.from(publicKey.x, "base64url");
  return {
    volumeId: createHash("sha256")
      .update("bluey-jobs-runner\0volume-id-v1\0", "utf8")
      .update(publicBytes)
      .digest("base64url"),
    publicKeyRaw: publicKey.x,
    publicKeyFingerprint: createHash("sha256")
      .update(publicBytes)
      .digest("hex"),
    privateKey,
  };
}

function sha256(value: string): string {
  return createHash("sha256").update(value, "utf8").digest("hex");
}
