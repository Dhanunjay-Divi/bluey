import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import type { AccountResidencyLocator } from "../src/account-residency.js";
import {
  EMPTY_LEGACY_ARTIFACT_SET_SHA256,
  type LegacyRunnerArtifactEvidence,
  type LegacyRunnerStorageInventory,
} from "../src/legacy-runner-storage.js";
import {
  assertRunnerPurgeCurrentTargetStorageEmpty,
  canonicalRunnerPurgeStorageEvidence,
  completeRunnerPurgeStorageEvidence,
  createRunnerPurgeStorageBeforeEvidence,
  legacyTargetArtifacts,
  parseRunnerPurgeStorageBeforeEvidence,
  parseRunnerPurgeStorageEvidence,
  runnerPurgeLocatorEvidence,
  runnerPurgeStorageEvidenceSha256,
  runnerPurgeTargetInventoryState,
  RunnerPurgeStorageEvidenceError,
  type RunnerPurgeStorageEvidence,
} from "../src/purge-storage-evidence.js";
import {
  EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
  type SubjectStorageInventoryEvidence,
} from "../src/subject-storage-layout.js";
import type { SubjectStorageInventory } from "../src/subject-storage-manager.js";

const DEVICE_ID = "unix:602:volume-7";
const OTHER_DEVICE_ID = "unix:602:volume-8";
const SUBJECT = "1".repeat(64);
const PROFILE = "a".repeat(40);
const OTHER_PROFILE = "b".repeat(40);
const RESULT = "c".repeat(64);
const OTHER_RESULT = "d".repeat(64);
const EMPTY_RUNNER_SHA256 =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

const LOCATORS = Object.freeze([
  locator("profile", PROFILE),
  locator("result", RESULT),
]);

describe("runner purge storage evidence", () => {
  it("hashes only canonical locator order and binds every locator signature", () => {
    const expected = createHash("sha256");
    for (const value of LOCATORS) {
      expected.update(value.kind, "utf8");
      expected.update("\n", "utf8");
      expected.update(value.scope, "utf8");
      expected.update("\n", "utf8");
      expected.update(value.signature, "utf8");
      expected.update("\n", "utf8");
    }

    expect(runnerPurgeLocatorEvidence(LOCATORS)).toEqual({
      count: 2,
      sha256: expected.digest("hex"),
    });
    expect(() => runnerPurgeLocatorEvidence([LOCATORS[1]!, LOCATORS[0]!]))
      .toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => runnerPurgeLocatorEvidence([LOCATORS[0]!, LOCATORS[0]!]))
      .toThrowError(RunnerPurgeStorageEvidenceError);

    const changedSignature = Object.freeze({
      ...LOCATORS[0]!,
      signature: `${LOCATORS[0]!.signature}-changed`,
    });
    expect(runnerPurgeLocatorEvidence([changedSignature, LOCATORS[1]!]).sha256)
      .not.toBe(runnerPurgeLocatorEvidence(LOCATORS).sha256);
  });

  it("selects every matching legacy family without selecting another subject", () => {
    const artifacts = mixedLegacyArtifacts();
    const inventory = legacyInventory(artifacts, rootSnapshot("mixed-before"));
    const selected = legacyTargetArtifacts(inventory, LOCATORS);

    expect(selected.map((value) => value.relativePath)).toEqual([
      `active/${PROFILE}`,
      `active/${PROFILE}.restore.tar.gz.pending`,
      `active/${PROFILE}/Cookies`,
      `receipts/${PROFILE}/receipt.json`,
      `run-checkpoints/${PROFILE}/checkpoint.json`,
      `snapshots/${PROFILE}.generation.previous`,
      `snapshots/${PROFILE}.tar.gz.enc.staged`,
      `step-results/${RESULT}.json.enc.pending`,
    ]);
    expect(selected.every((value) => value.classification === "known_legacy"))
      .toBe(true);
    expect(selected.some((value) => value.relativePath.includes(OTHER_PROFILE)))
      .toBe(false);
    expect(selected.some((value) => value.relativePath.includes(OTHER_RESULT)))
      .toBe(false);
  });

  it("builds immutable before evidence and completes only with a zero target rescan", () => {
    const beforeInventory = residentInventory(mixedLegacyArtifacts());
    const before = createRunnerPurgeStorageBeforeEvidence(beforeInventory, LOCATORS);
    expect(() => completeRunnerPurgeStorageEvidence(
      before,
      clearedInventory(survivingLegacyArtifacts()),
      LOCATORS,
    )).toThrowError(RunnerPurgeStorageEvidenceError);
    const afterInventory = clearedInventory([]);
    const completed = completeRunnerPurgeStorageEvidence(before, afterInventory, LOCATORS);

    expect(before.legacy.targetBefore.entryCount).toBe(8);
    expect(before.legacy.rootBefore.artifactCount).toBe(11);
    expect(completed.legacy.targetAfter).toEqual({
      entryCount: 0,
      fileBytes: "0",
      sha256: EMPTY_RUNNER_SHA256,
    });
    expect(completed.legacy.rootAfter).toEqual({
      artifactCount: 0,
      artifactBytes: "0",
      artifactSetSha256: EMPTY_LEGACY_ARTIFACT_SET_SHA256,
      unclassifiedRootCount: 0,
    });
    expect(completed.subjectStorage.after).toMatchObject({
      residency: "never_resident",
      subjectTree: {
        entryCount: 0,
        fileBytes: "0",
        sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      },
      ownership: {
        entryCount: 0,
        fileBytes: "0",
        sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      },
      target: {
        entryCount: 0,
        fileBytes: "0",
        sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      },
    });
    expect(runnerPurgeTargetInventoryState(completed, "before").count).toBe(14);
    expect(runnerPurgeTargetInventoryState(completed, "after")).toEqual({
      count: 0,
      sha256: EMPTY_RUNNER_SHA256,
    });
    expect(parseRunnerPurgeStorageBeforeEvidence(structuredClone(before))).toEqual(before);
    expect(parseRunnerPurgeStorageEvidence(structuredClone(completed))).toEqual(completed);
    expect(canonicalRunnerPurgeStorageEvidence(completed).toString("utf8"))
      .toContain("legacy_target_after_file_bytes=0\n");
    expect(runnerPurgeStorageEvidenceSha256(completed)).toMatch(/^[0-9a-f]{64}$/);
  });

  it("separates local tombstone enforcement from global ACK evidence", () => {
    const before = createRunnerPurgeStorageBeforeEvidence(
      residentInventory(mixedLegacyArtifacts()),
      LOCATORS,
    );
    const locallyCleared = clearedInventory(survivingLegacyArtifacts());

    expect(() =>
      assertRunnerPurgeCurrentTargetStorageEmpty(locallyCleared, LOCATORS),
    ).not.toThrow();
    expect(() =>
      completeRunnerPurgeStorageEvidence(before, locallyCleared, LOCATORS),
    ).toThrowError(RunnerPurgeStorageEvidenceError);
  });

  it("rejects unknown fields at every strict codec boundary", () => {
    const before = createRunnerPurgeStorageBeforeEvidence(
      residentInventory(mixedLegacyArtifacts()),
      LOCATORS,
    );
    const completed = completeRunnerPurgeStorageEvidence(
      before,
      clearedInventory([]),
      LOCATORS,
    );

    expect(() => parseRunnerPurgeStorageBeforeEvidence({
      ...before,
      unknown: true,
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageBeforeEvidence({
      ...before,
      root: { ...before.root, unknown: true },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      legacy: { ...completed.legacy, unknown: true },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      subjectStorage: {
        ...completed.subjectStorage,
        after: { ...completed.subjectStorage.after, unknown: true },
      },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
  });

  it("rejects wrong retained-device and same-snapshot bindings", () => {
    const inventory = residentInventory(mixedLegacyArtifacts());
    const wrongLegacyDevice: SubjectStorageInventory = Object.freeze({
      ...inventory,
      legacyInventory: Object.freeze({
        ...inventory.legacyInventory,
        rootDeviceId: OTHER_DEVICE_ID,
      }),
    });
    const wrongNativeSnapshot: SubjectStorageInventory = Object.freeze({
      ...inventory,
      runnerRootInventory: Object.freeze({
        ...inventory.runnerRootInventory,
        sha256: differentSha(inventory.runnerRootInventory.sha256),
      }),
    });

    expect(() => createRunnerPurgeStorageBeforeEvidence(wrongLegacyDevice, LOCATORS))
      .toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => createRunnerPurgeStorageBeforeEvidence(wrongNativeSnapshot, LOCATORS))
      .toThrowError(RunnerPurgeStorageEvidenceError);

    const before = createRunnerPurgeStorageBeforeEvidence(inventory, LOCATORS);
    expect(() => completeRunnerPurgeStorageEvidence(
      before,
      clearedInventory([], OTHER_DEVICE_ID),
      LOCATORS,
    )).toThrowError(RunnerPurgeStorageEvidenceError);
  });

  it("rejects an unclassified root before any purge evidence can be journaled", () => {
    const unknown = legacyArtifact(
      "unknown-cache",
      "directory",
      0,
      sha256("unknown-directory"),
      "unclassified_root",
    );
    expect(() => createRunnerPurgeStorageBeforeEvidence(
      residentInventory([unknown]),
      LOCATORS,
    )).toThrowError(RunnerPurgeStorageEvidenceError);
    const before = createRunnerPurgeStorageBeforeEvidence(
      residentInventory(mixedLegacyArtifacts()),
      LOCATORS,
    );
    expect(() =>
      parseRunnerPurgeStorageBeforeEvidence({
        ...before,
        legacy: {
          ...before.legacy,
          rootBefore: {
            ...before.legacy.rootBefore,
            unclassifiedRootCount: 1,
          },
        },
      }),
    ).toThrowError(RunnerPurgeStorageEvidenceError);
  });

  it("enforces canonical decimal u64s, lowercase hashes, and empty digests", () => {
    const before = createRunnerPurgeStorageBeforeEvidence(
      residentInventory(mixedLegacyArtifacts()),
      LOCATORS,
    );
    const completed = completeRunnerPurgeStorageEvidence(
      before,
      clearedInventory([]),
      LOCATORS,
    );

    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      root: {
        ...completed.root,
        before: { ...completed.root.before, fileBytes: "04096" },
      },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      root: {
        ...completed.root,
        before: { ...completed.root.before, fileBytes: 4096 },
      },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      root: {
        ...completed.root,
        before: { ...completed.root.before, fileBytes: "18446744073709551616" },
      },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      root: {
        ...completed.root,
        before: { ...completed.root.before, sha256: "A".repeat(64) },
      },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      subjectStorage: {
        ...completed.subjectStorage,
        after: {
          ...completed.subjectStorage.after,
          target: {
            ...completed.subjectStorage.after.target,
            sha256: "0".repeat(64),
          },
        },
      },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
    expect(() => parseRunnerPurgeStorageEvidence({
      ...completed,
      legacy: {
        ...completed.legacy,
        rootAfter: { ...completed.legacy.rootAfter, artifactBytes: -1 },
      },
    })).toThrowError(RunnerPurgeStorageEvidenceError);
  });

  it("binds the digest to every material evidence group", () => {
    const before = createRunnerPurgeStorageBeforeEvidence(
      residentInventory(mixedLegacyArtifacts()),
      LOCATORS,
    );
    const completed = completeRunnerPurgeStorageEvidence(
      before,
      clearedInventory([]),
      LOCATORS,
    );
    const variants: readonly RunnerPurgeStorageEvidence[] = [
      Object.freeze({
        ...completed,
        root: Object.freeze({ ...completed.root, deviceId: `${DEVICE_ID}-changed` }),
      }),
      Object.freeze({
        ...completed,
        locators: Object.freeze({
          ...completed.locators,
          sha256: differentSha(completed.locators.sha256),
        }),
      }),
      Object.freeze({
        ...completed,
        subjectStorage: Object.freeze({
          ...completed.subjectStorage,
          before: Object.freeze({
            ...completed.subjectStorage.before,
            completeRoot: Object.freeze({
              ...completed.subjectStorage.before.completeRoot,
              sha256: differentSha(completed.subjectStorage.before.completeRoot.sha256),
            }),
          }),
        }),
      }),
      Object.freeze({
        ...completed,
        legacy: Object.freeze({
          ...completed.legacy,
          targetBefore: Object.freeze({
            ...completed.legacy.targetBefore,
            sha256: differentSha(completed.legacy.targetBefore.sha256),
          }),
        }),
      }),
      Object.freeze({
        ...completed,
        legacy: Object.freeze({
          ...completed.legacy,
          rootBefore: Object.freeze({
            ...completed.legacy.rootBefore,
            artifactSetSha256: differentSha(
              completed.legacy.rootBefore.artifactSetSha256,
            ),
          }),
        }),
      }),
    ];
    const baseline = runnerPurgeStorageEvidenceSha256(completed);

    for (const variant of variants) {
      expect(parseRunnerPurgeStorageEvidence(variant)).toEqual(variant);
      expect(runnerPurgeStorageEvidenceSha256(variant)).not.toBe(baseline);
    }
  });
});

function residentInventory(
  artifacts: readonly LegacyRunnerArtifactEvidence[],
  deviceId = DEVICE_ID,
): SubjectStorageInventory {
  const root = rootSnapshot(`resident:${deviceId}:${artifactKey(artifacts)}`);
  const subjectTree = inventoryEvidence(4, 40, sha256("subject-tree"));
  const ownership = inventoryEvidence(2, 20, sha256("scope-ownership"));
  const target = inventoryEvidence(6, 60, sha256("subject-target"));
  const complete = inventoryEvidence(8, 80, sha256("complete-v2-before"));
  const subject = Object.freeze({
    subjectSha256: SUBJECT,
    profileScopes: Object.freeze([PROFILE]),
    resultScopes: Object.freeze([RESULT]),
    scopeOwnershipPaths: Object.freeze([
      `account-data-v2/scope-owners/profiles/${PROFILE}.json`,
      `account-data-v2/scope-owners/results/${RESULT}.json`,
    ]),
    subjectTreeInventory: subjectTree,
    scopeOwnershipInventory: ownership,
    inventory: target,
  }) as unknown as NonNullable<SubjectStorageInventory["subject"]>;
  return Object.freeze({
    residency: "resident" as const,
    subject,
    inventory: target,
    completeInventory: complete,
    runnerRootInventory: Object.freeze({
      deviceId,
      linkCount: root.linkCount,
      entryCount: root.entryCount,
      fileBytes: String(root.fileBytes),
      sha256: root.sha256,
    }),
    legacyInventory: legacyInventory(artifacts, root, deviceId),
  });
}

function clearedInventory(
  artifacts: readonly LegacyRunnerArtifactEvidence[],
  deviceId = DEVICE_ID,
): SubjectStorageInventory {
  const rebound = artifacts.map((value) => Object.freeze({ ...value, deviceId }));
  const root = rootSnapshot(`cleared:${deviceId}:${artifactKey(rebound)}`);
  const empty = inventoryEvidence(
    0,
    0,
    EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
  );
  return Object.freeze({
    residency: "never_resident" as const,
    subject: null,
    inventory: empty,
    completeInventory: inventoryEvidence(4, 40, sha256("complete-v2-after")),
    runnerRootInventory: Object.freeze({
      deviceId,
      linkCount: root.linkCount,
      entryCount: root.entryCount,
      fileBytes: String(root.fileBytes),
      sha256: root.sha256,
    }),
    legacyInventory: legacyInventory(rebound, root, deviceId),
  });
}

interface RootSnapshotFixture {
  readonly linkCount: number;
  readonly entryCount: number;
  readonly fileBytes: number;
  readonly sha256: string;
}

function rootSnapshot(seed: string): RootSnapshotFixture {
  return Object.freeze({
    linkCount: 7,
    entryCount: 64,
    fileBytes: 4096,
    sha256: sha256(`root:${seed}`),
  });
}

function legacyInventory(
  artifacts: readonly LegacyRunnerArtifactEvidence[],
  root: RootSnapshotFixture,
  deviceId = DEVICE_ID,
): LegacyRunnerStorageInventory {
  const artifactBytes = artifacts.reduce((total, value) => total + value.sizeBytes, 0);
  const unclassifiedRootPaths = [...new Set(
    artifacts
      .filter((value) => value.classification === "unclassified_root")
      .map((value) => value.relativePath.split("/", 1)[0]!),
  )].sort(compareUtf8);
  const artifactSetSha256 = artifacts.length === 0
    ? EMPTY_LEGACY_ARTIFACT_SET_SHA256
    : sha256(`artifact-set:${artifactKey(artifacts)}`);
  return Object.freeze({
    version: 1 as const,
    rootDeviceId: deviceId,
    rootLinkCount: root.linkCount,
    nativeInventorySha256: root.sha256,
    nativeInventoryCount: root.entryCount,
    nativeInventoryBytes: root.fileBytes,
    legacyArtifactCount: artifacts.length,
    legacyArtifactBytes: artifactBytes,
    legacyInventorySha256: sha256(`legacy:${artifactKey(artifacts)}`),
    legacyArtifactSetSha256: artifactSetSha256,
    artifacts: Object.freeze([...artifacts]),
    unclassifiedRootPaths: Object.freeze(unclassifiedRootPaths),
  });
}

function mixedLegacyArtifacts(): readonly LegacyRunnerArtifactEvidence[] {
  return Object.freeze([
    legacyArtifact(`active/${PROFILE}`, "directory", 0, sha256("profile-directory")),
    legacyArtifact(`active/${PROFILE}/Cookies`, "file", 5, sha256("profile-cookie")),
    legacyArtifact(
      `active/${PROFILE}.restore.tar.gz.pending`,
      "file",
      6,
      sha256("restore-stage"),
    ),
    legacyArtifact(
      `snapshots/${PROFILE}.tar.gz.enc.staged`,
      "file",
      7,
      sha256("snapshot-stage"),
    ),
    legacyArtifact(
      `snapshots/${PROFILE}.generation.previous`,
      "file",
      8,
      sha256("generation-stage"),
    ),
    legacyArtifact(
      `run-checkpoints/${PROFILE}/checkpoint.json`,
      "file",
      9,
      sha256("checkpoint"),
    ),
    legacyArtifact(
      `receipts/${PROFILE}/receipt.json`,
      "file",
      10,
      sha256("receipt"),
    ),
    legacyArtifact(
      `step-results/${RESULT}.json.enc.pending`,
      "file",
      11,
      sha256("result-stage"),
    ),
    ...survivingLegacyArtifacts(),
  ]);
}

function survivingLegacyArtifacts(): readonly LegacyRunnerArtifactEvidence[] {
  return Object.freeze([
    legacyArtifact(
      `active/${OTHER_PROFILE}`,
      "directory",
      0,
      sha256("other-profile-directory"),
    ),
    legacyArtifact(
      `active/${OTHER_PROFILE}/Cookies`,
      "file",
      12,
      sha256("other-profile-cookie"),
    ),
    legacyArtifact(
      `step-results/${OTHER_RESULT}.json.enc`,
      "file",
      13,
      sha256("other-result"),
    ),
  ]);
}

function legacyArtifact(
  relativePath: string,
  kind: "directory" | "file",
  sizeBytes: number,
  digest: string,
  classification: LegacyRunnerArtifactEvidence["classification"] = "known_legacy",
): LegacyRunnerArtifactEvidence {
  return Object.freeze({
    relativePath,
    kind,
    deviceId: DEVICE_ID,
    linkCount: kind === "file" ? 1 : 2,
    sizeBytes,
    sha256: digest,
    classification,
  });
}

function locator(
  kind: "profile" | "result",
  scope: string,
): AccountResidencyLocator {
  return Object.freeze({
    version: 1 as const,
    audience: "bluey-jobs-runner-account-residency" as const,
    volumeId: "volume-602",
    volumeKeyFingerprint: sha256("volume-key"),
    subjectSha256: SUBJECT,
    kind,
    scope,
    artifactFamilies: kind === "profile"
      ? Object.freeze(["active", "snapshots", "run-checkpoints", "receipts", "temporary"])
      : Object.freeze(["step-results", "temporary"]),
    signature: createHash("sha256")
      .update(`${kind}\0${scope}`, "utf8")
      .digest("base64url"),
  });
}

function inventoryEvidence(
  entryCount: number,
  fileBytes: number,
  digest: string,
): SubjectStorageInventoryEvidence {
  return Object.freeze({ entryCount, fileBytes: String(fileBytes), sha256: digest });
}

function artifactKey(artifacts: readonly LegacyRunnerArtifactEvidence[]): string {
  return artifacts.map((value) => [
    value.relativePath,
    value.kind,
    value.deviceId,
    value.linkCount,
    value.sizeBytes,
    value.sha256,
    value.classification,
  ].join("\0")).join("\n");
}

function sha256(value: string): string {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function differentSha(value: string): string {
  return value === "a".repeat(64) ? "b".repeat(64) : "a".repeat(64);
}

function compareUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}
