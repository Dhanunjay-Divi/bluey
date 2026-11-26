import { createHash } from "node:crypto";
import type { AccountResidencyLocator } from "./account-residency.js";
import {
  EMPTY_LEGACY_ARTIFACT_SET_SHA256,
  type LegacyRunnerArtifactEvidence,
  type LegacyRunnerStorageInventory,
} from "./legacy-runner-storage.js";
import {
  EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
  SUBJECT_STORAGE_LAYOUT_VERSION,
  type SubjectStorageInventoryEvidence,
} from "./subject-storage-layout.js";
import type { SubjectStorageInventory } from "./subject-storage-manager.js";

export const RUNNER_PURGE_STORAGE_EVIDENCE_VERSION = 2 as const;
export const RUNNER_LEGACY_INVENTORY_VERSION = 1 as const;
export const EMPTY_RUNNER_INVENTORY_SHA256 =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

const STORAGE_EVIDENCE_DOMAIN = "bluey-jobs-runner-purge-storage-evidence-v2";
const TARGET_INVENTORY_DOMAIN = "bluey-jobs-runner-purge-target-inventory-v2";
const LEGACY_TARGET_INVENTORY_DOMAIN =
  "bluey-jobs-runner-legacy-target-inventory-v2";
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const DEVICE_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:+-]{0,127}$/;
const PROFILE_SCOPE_PATTERN = /^[0-9a-f]{40}$/;
const RESULT_SCOPE_PATTERN = /^[0-9a-f]{64}$/;
const DECIMAL_U64_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const MAXIMUM_U64 = 18_446_744_073_709_551_615n;
const MAXIMUM_SAFE_INTEGER = Number.MAX_SAFE_INTEGER;

export class RunnerPurgeStorageEvidenceError extends Error {
  constructor() {
    super("Runner purge storage evidence is invalid.");
    this.name = "RunnerPurgeStorageEvidenceError";
  }
}

export interface RunnerPurgeInventoryEvidence {
  readonly entryCount: number;
  readonly fileBytes: string;
  readonly sha256: string;
}

export interface RunnerPurgeRootSnapshotEvidence extends RunnerPurgeInventoryEvidence {
  readonly linkCount: number;
}

export interface RunnerPurgeSubjectStorageSnapshotEvidence {
  readonly residency: "never_resident" | "resident";
  readonly subjectTree: RunnerPurgeInventoryEvidence;
  readonly ownership: RunnerPurgeInventoryEvidence;
  readonly target: RunnerPurgeInventoryEvidence;
  readonly completeRoot: RunnerPurgeInventoryEvidence;
}

export interface RunnerPurgeLegacyRootSnapshotEvidence {
  readonly artifactCount: number;
  readonly artifactBytes: string;
  readonly artifactSetSha256: string;
  readonly unclassifiedRootCount: number;
}

/** Immutable journal payload persisted before any target byte is removed. */
export interface RunnerPurgeStorageBeforeEvidence {
  readonly version: typeof RUNNER_PURGE_STORAGE_EVIDENCE_VERSION;
  readonly root: {
    readonly deviceId: string;
    readonly before: RunnerPurgeRootSnapshotEvidence;
  };
  readonly locators: RunnerPurgeLocatorEvidence;
  readonly subjectStorage: {
    readonly layoutVersion: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
    readonly before: RunnerPurgeSubjectStorageSnapshotEvidence;
  };
  readonly legacy: {
    readonly inventoryVersion: typeof RUNNER_LEGACY_INVENTORY_VERSION;
    readonly targetBefore: RunnerPurgeInventoryEvidence;
    readonly rootBefore: RunnerPurgeLegacyRootSnapshotEvidence;
  };
}

export interface RunnerPurgeLocatorEvidence {
  readonly count: number;
  readonly sha256: string;
}

export interface RunnerPurgeStorageEvidence {
  readonly version: typeof RUNNER_PURGE_STORAGE_EVIDENCE_VERSION;
  readonly root: {
    readonly deviceId: string;
    readonly before: RunnerPurgeRootSnapshotEvidence;
    readonly after: RunnerPurgeRootSnapshotEvidence;
  };
  readonly locators: RunnerPurgeLocatorEvidence;
  readonly subjectStorage: {
    readonly layoutVersion: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
    readonly before: RunnerPurgeSubjectStorageSnapshotEvidence;
    readonly after: RunnerPurgeSubjectStorageSnapshotEvidence;
  };
  readonly legacy: {
    readonly inventoryVersion: typeof RUNNER_LEGACY_INVENTORY_VERSION;
    readonly targetBefore: RunnerPurgeInventoryEvidence;
    readonly targetAfter: RunnerPurgeInventoryEvidence;
    readonly rootBefore: RunnerPurgeLegacyRootSnapshotEvidence;
    readonly rootAfter: RunnerPurgeLegacyRootSnapshotEvidence;
  };
}

export interface RunnerPurgeTargetInventoryState {
  readonly count: number;
  readonly sha256: string;
}

export function createRunnerPurgeStorageBeforeEvidence(
  inventory: SubjectStorageInventory,
  locators: readonly AccountResidencyLocator[],
): RunnerPurgeStorageBeforeEvidence {
  validateSnapshotBinding(inventory);
  const locatorEvidence = runnerPurgeLocatorEvidence(locators);
  const before = subjectStorageSnapshot(inventory);
  if (
    (before.residency === "resident" &&
      before.ownership.entryCount !== locatorEvidence.count) ||
    (before.residency === "never_resident" && before.ownership.entryCount !== 0)
  ) {
    throw invalidEvidence();
  }
  const targetBefore = legacyTargetEvidence(
    inventory.legacyInventory,
    locators,
  );
  const rootBefore = legacyRootSnapshot(inventory.legacyInventory);
  // A purge never owns an unclassified root. It must fail before deletion rather
  // than make an unknown namespace disappear to satisfy the final zero proof.
  if (rootBefore.unclassifiedRootCount !== 0) throw invalidEvidence();
  const evidence: RunnerPurgeStorageBeforeEvidence = Object.freeze({
    version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
    root: Object.freeze({
      deviceId: inventory.runnerRootInventory.deviceId,
      before: rootSnapshot(inventory),
    }),
    locators: locatorEvidence,
    subjectStorage: Object.freeze({
      layoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
      before,
    }),
    legacy: Object.freeze({
      inventoryVersion: RUNNER_LEGACY_INVENTORY_VERSION,
      targetBefore,
      rootBefore,
    }),
  });
  return parseRunnerPurgeStorageBeforeEvidence(evidence);
}

export function completeRunnerPurgeStorageEvidence(
  beforeInput: RunnerPurgeStorageBeforeEvidence,
  inventory: SubjectStorageInventory,
  locators: readonly AccountResidencyLocator[],
): RunnerPurgeStorageEvidence {
  const before = parseRunnerPurgeStorageBeforeEvidence(beforeInput);
  validateSnapshotBinding(inventory);
  const currentLocators = runnerPurgeLocatorEvidence(locators);
  if (
    currentLocators.count !== before.locators.count ||
    currentLocators.sha256 !== before.locators.sha256 ||
    inventory.runnerRootInventory.deviceId !== before.root.deviceId
  ) {
    throw invalidEvidence();
  }
  const evidence: RunnerPurgeStorageEvidence = Object.freeze({
    version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
    root: Object.freeze({
      deviceId: before.root.deviceId,
      before: before.root.before,
      after: rootSnapshot(inventory),
    }),
    locators: before.locators,
    subjectStorage: Object.freeze({
      layoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
      before: before.subjectStorage.before,
      after: subjectStorageSnapshot(inventory),
    }),
    legacy: Object.freeze({
      inventoryVersion: RUNNER_LEGACY_INVENTORY_VERSION,
      targetBefore: before.legacy.targetBefore,
      targetAfter: legacyTargetEvidence(inventory.legacyInventory, locators),
      rootBefore: before.legacy.rootBefore,
      rootAfter: legacyRootSnapshot(inventory.legacyInventory),
    }),
  });
  return parseRunnerPurgeStorageEvidence(evidence);
}

/**
 * Verifies local enforcement for a previously acknowledged tombstone without
 * replaying its historical global-root proof. This is intentionally narrower
 * than ACK evidence: the exact subject and locator-owned legacy targets must
 * be absent and no unclassified root may exist, while a different account's
 * classified legacy target can keep the control plane online and non-serving.
 */
export function assertRunnerPurgeCurrentTargetStorageEmpty(
  inventory: SubjectStorageInventory,
  locators: readonly AccountResidencyLocator[],
): void {
  try {
    validateSnapshotBinding(inventory);
    runnerPurgeLocatorEvidence(locators);
    const subject = subjectStorageSnapshot(inventory);
    const legacyTarget = legacyTargetEvidence(
      inventory.legacyInventory,
      locators,
    );
    if (
      subject.residency !== "never_resident" ||
      !isEmptyInventory(
        subject.subjectTree,
        EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      ) ||
      !isEmptyInventory(
        subject.ownership,
        EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      ) ||
      !isEmptyInventory(
        subject.target,
        EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      ) ||
      !isEmptyInventory(legacyTarget, EMPTY_RUNNER_INVENTORY_SHA256) ||
      inventory.legacyInventory.unclassifiedRootPaths.length !== 0
    ) {
      throw invalidEvidence();
    }
  } catch (error) {
    if (error instanceof RunnerPurgeStorageEvidenceError) throw error;
    throw invalidEvidence();
  }
}

export function runnerPurgeLocatorEvidence(
  locators: readonly AccountResidencyLocator[],
): RunnerPurgeLocatorEvidence {
  if (!Array.isArray(locators)) throw invalidEvidence();
  const digest = createHash("sha256");
  let previous: Buffer | undefined;
  const seen = new Set<string>();
  for (const locator of locators) {
    validateLocator(locator);
    const canonical = Buffer.from(`${locator.kind}\0${locator.scope}`, "utf8");
    if (previous && Buffer.compare(previous, canonical) >= 0)
      throw invalidEvidence();
    previous = canonical;
    const key = canonical.toString("hex");
    if (seen.has(key)) throw invalidEvidence();
    seen.add(key);
    digest.update(locator.kind, "utf8");
    digest.update("\n", "utf8");
    digest.update(locator.scope, "utf8");
    digest.update("\n", "utf8");
    digest.update(locator.signature, "utf8");
    digest.update("\n", "utf8");
  }
  return Object.freeze({
    count: locators.length,
    sha256: digest.digest("hex"),
  });
}

export function legacyTargetArtifacts(
  inventory: LegacyRunnerStorageInventory,
  locators: readonly AccountResidencyLocator[],
): readonly LegacyRunnerArtifactEvidence[] {
  runnerPurgeLocatorEvidence(locators);
  const targets = inventory.artifacts.filter((artifact) => {
    validateLegacyArtifact(artifact, inventory.rootDeviceId);
    return (
      artifact.classification === "known_legacy" &&
      locators.some((locator) =>
        legacyArtifactMatchesLocator(artifact.relativePath, locator),
      )
    );
  });
  return Object.freeze(
    [...targets].sort((left, right) =>
      compareUtf8(left.relativePath, right.relativePath),
    ),
  );
}

export function canonicalRunnerPurgeStorageEvidence(
  input: RunnerPurgeStorageEvidence,
): Buffer {
  const evidence = parseRunnerPurgeStorageEvidence(input);
  const before = evidence.subjectStorage.before;
  const after = evidence.subjectStorage.after;
  return Buffer.from(
    [
      STORAGE_EVIDENCE_DOMAIN,
      `version=${evidence.version}`,
      `root_device_id=${evidence.root.deviceId}`,
      ...canonicalRootSnapshot("root_before", evidence.root.before),
      ...canonicalRootSnapshot("root_after", evidence.root.after),
      `locators_count=${evidence.locators.count}`,
      `locators_sha256=${evidence.locators.sha256}`,
      `subject_storage_layout_version=${evidence.subjectStorage.layoutVersion}`,
      `subject_storage_before_residency=${before.residency}`,
      ...canonicalInventory(
        "subject_storage_before_subject_tree",
        before.subjectTree,
      ),
      ...canonicalInventory(
        "subject_storage_before_ownership",
        before.ownership,
      ),
      ...canonicalInventory("subject_storage_before_target", before.target),
      ...canonicalInventory(
        "subject_storage_before_complete_root",
        before.completeRoot,
      ),
      `subject_storage_after_residency=${after.residency}`,
      ...canonicalInventory(
        "subject_storage_after_subject_tree",
        after.subjectTree,
      ),
      ...canonicalInventory("subject_storage_after_ownership", after.ownership),
      ...canonicalInventory("subject_storage_after_target", after.target),
      ...canonicalInventory(
        "subject_storage_after_complete_root",
        after.completeRoot,
      ),
      `legacy_inventory_version=${evidence.legacy.inventoryVersion}`,
      ...canonicalInventory(
        "legacy_target_before",
        evidence.legacy.targetBefore,
      ),
      ...canonicalInventory("legacy_target_after", evidence.legacy.targetAfter),
      ...canonicalLegacyRoot("legacy_root_before", evidence.legacy.rootBefore),
      ...canonicalLegacyRoot("legacy_root_after", evidence.legacy.rootAfter),
      "",
    ].join("\n"),
    "utf8",
  );
}

export function runnerPurgeStorageEvidenceSha256(
  evidence: RunnerPurgeStorageEvidence,
): string {
  return createHash("sha256")
    .update(canonicalRunnerPurgeStorageEvidence(evidence))
    .digest("hex");
}

export function runnerPurgeStorageBeforeEvidence(
  input: RunnerPurgeStorageEvidence,
): RunnerPurgeStorageBeforeEvidence {
  const evidence = parseRunnerPurgeStorageEvidence(input);
  return parseRunnerPurgeStorageBeforeEvidence({
    version: evidence.version,
    root: { deviceId: evidence.root.deviceId, before: evidence.root.before },
    locators: evidence.locators,
    subjectStorage: {
      layoutVersion: evidence.subjectStorage.layoutVersion,
      before: evidence.subjectStorage.before,
    },
    legacy: {
      inventoryVersion: evidence.legacy.inventoryVersion,
      targetBefore: evidence.legacy.targetBefore,
      rootBefore: evidence.legacy.rootBefore,
    },
  });
}

export function runnerPurgeTargetInventoryState(
  input: RunnerPurgeStorageEvidence,
  phase: "before" | "after",
): RunnerPurgeTargetInventoryState {
  const evidence = parseRunnerPurgeStorageEvidence(input);
  const legacy =
    phase === "before"
      ? evidence.legacy.targetBefore
      : evidence.legacy.targetAfter;
  const subject =
    phase === "before"
      ? evidence.subjectStorage.before.target
      : evidence.subjectStorage.after.target;
  const count = checkedSafeSum(legacy.entryCount, subject.entryCount);
  if (count === 0) {
    return Object.freeze({ count: 0, sha256: EMPTY_RUNNER_INVENTORY_SHA256 });
  }
  const canonical = Buffer.from(
    [
      TARGET_INVENTORY_DOMAIN,
      `legacy_entry_count=${legacy.entryCount}`,
      `legacy_file_bytes=${legacy.fileBytes}`,
      `legacy_sha256=${legacy.sha256}`,
      `subject_entry_count=${subject.entryCount}`,
      `subject_file_bytes=${subject.fileBytes}`,
      `subject_sha256=${subject.sha256}`,
      "",
    ].join("\n"),
    "utf8",
  );
  return Object.freeze({
    count,
    sha256: createHash("sha256").update(canonical).digest("hex"),
  });
}

export function parseRunnerPurgeStorageBeforeEvidence(
  input: unknown,
): RunnerPurgeStorageBeforeEvidence {
  try {
    const value = exactRecord(input, [
      "legacy",
      "locators",
      "root",
      "subjectStorage",
      "version",
    ]);
    if (value.version !== RUNNER_PURGE_STORAGE_EVIDENCE_VERSION)
      throw invalidEvidence();
    const root = exactRecord(value.root, ["before", "deviceId"]);
    const subjectStorage = exactRecord(value.subjectStorage, [
      "before",
      "layoutVersion",
    ]);
    const legacy = exactRecord(value.legacy, [
      "inventoryVersion",
      "rootBefore",
      "targetBefore",
    ]);
    if (
      subjectStorage.layoutVersion !== SUBJECT_STORAGE_LAYOUT_VERSION ||
      legacy.inventoryVersion !== RUNNER_LEGACY_INVENTORY_VERSION
    ) {
      throw invalidEvidence();
    }
    const parsed: RunnerPurgeStorageBeforeEvidence = Object.freeze({
      version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
      root: Object.freeze({
        deviceId: parseDeviceId(root.deviceId),
        before: parseRootSnapshot(root.before),
      }),
      locators: parseLocatorEvidence(value.locators),
      subjectStorage: Object.freeze({
        layoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
        before: parseSubjectSnapshot(subjectStorage.before),
      }),
      legacy: Object.freeze({
        inventoryVersion: RUNNER_LEGACY_INVENTORY_VERSION,
        targetBefore: parseInventory(
          legacy.targetBefore,
          EMPTY_RUNNER_INVENTORY_SHA256,
        ),
        rootBefore: parseLegacyRootSnapshot(legacy.rootBefore),
      }),
    });
    validateBeforeEvidence(parsed);
    return parsed;
  } catch (error) {
    if (error instanceof RunnerPurgeStorageEvidenceError) throw error;
    throw invalidEvidence();
  }
}

export function parseRunnerPurgeStorageEvidence(
  input: unknown,
): RunnerPurgeStorageEvidence {
  try {
    const value = exactRecord(input, [
      "legacy",
      "locators",
      "root",
      "subjectStorage",
      "version",
    ]);
    if (value.version !== RUNNER_PURGE_STORAGE_EVIDENCE_VERSION)
      throw invalidEvidence();
    const root = exactRecord(value.root, ["after", "before", "deviceId"]);
    const subjectStorage = exactRecord(value.subjectStorage, [
      "after",
      "before",
      "layoutVersion",
    ]);
    const legacy = exactRecord(value.legacy, [
      "inventoryVersion",
      "rootAfter",
      "rootBefore",
      "targetAfter",
      "targetBefore",
    ]);
    if (
      subjectStorage.layoutVersion !== SUBJECT_STORAGE_LAYOUT_VERSION ||
      legacy.inventoryVersion !== RUNNER_LEGACY_INVENTORY_VERSION
    ) {
      throw invalidEvidence();
    }
    const parsed: RunnerPurgeStorageEvidence = Object.freeze({
      version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
      root: Object.freeze({
        deviceId: parseDeviceId(root.deviceId),
        before: parseRootSnapshot(root.before),
        after: parseRootSnapshot(root.after),
      }),
      locators: parseLocatorEvidence(value.locators),
      subjectStorage: Object.freeze({
        layoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
        before: parseSubjectSnapshot(subjectStorage.before),
        after: parseSubjectSnapshot(subjectStorage.after),
      }),
      legacy: Object.freeze({
        inventoryVersion: RUNNER_LEGACY_INVENTORY_VERSION,
        targetBefore: parseInventory(
          legacy.targetBefore,
          EMPTY_RUNNER_INVENTORY_SHA256,
        ),
        targetAfter: parseInventory(
          legacy.targetAfter,
          EMPTY_RUNNER_INVENTORY_SHA256,
        ),
        rootBefore: parseLegacyRootSnapshot(legacy.rootBefore),
        rootAfter: parseLegacyRootSnapshot(legacy.rootAfter),
      }),
    });
    validateCompleteEvidence(parsed);
    return parsed;
  } catch (error) {
    if (error instanceof RunnerPurgeStorageEvidenceError) throw error;
    throw invalidEvidence();
  }
}

function subjectStorageSnapshot(
  inventory: SubjectStorageInventory,
): RunnerPurgeSubjectStorageSnapshotEvidence {
  const subjectTree = cloneInventory(
    inventory.subject?.subjectTreeInventory ?? emptySubjectInventory(),
  );
  const ownership = cloneInventory(
    inventory.subject?.scopeOwnershipInventory ?? emptySubjectInventory(),
  );
  return Object.freeze({
    residency: inventory.residency,
    subjectTree,
    ownership,
    target: cloneInventory(inventory.inventory),
    completeRoot: cloneInventory(inventory.completeInventory),
  });
}

function rootSnapshot(
  inventory: SubjectStorageInventory,
): RunnerPurgeRootSnapshotEvidence {
  return Object.freeze({
    linkCount: inventory.runnerRootInventory.linkCount,
    entryCount: inventory.runnerRootInventory.entryCount,
    fileBytes: inventory.runnerRootInventory.fileBytes,
    sha256: inventory.runnerRootInventory.sha256,
  });
}

function legacyRootSnapshot(
  inventory: LegacyRunnerStorageInventory,
): RunnerPurgeLegacyRootSnapshotEvidence {
  return Object.freeze({
    artifactCount: inventory.legacyArtifactCount,
    artifactBytes: String(inventory.legacyArtifactBytes),
    artifactSetSha256: inventory.legacyArtifactSetSha256,
    unclassifiedRootCount: inventory.unclassifiedRootPaths.length,
  });
}

function legacyTargetEvidence(
  inventory: LegacyRunnerStorageInventory,
  locators: readonly AccountResidencyLocator[],
): RunnerPurgeInventoryEvidence {
  const artifacts = legacyTargetArtifacts(inventory, locators);
  return legacyArtifactEvidence(artifacts, LEGACY_TARGET_INVENTORY_DOMAIN);
}

function legacyArtifactEvidence(
  artifacts: readonly LegacyRunnerArtifactEvidence[],
  domain: string,
): RunnerPurgeInventoryEvidence {
  if (artifacts.length === 0) {
    return Object.freeze({
      entryCount: 0,
      fileBytes: "0",
      sha256: EMPTY_RUNNER_INVENTORY_SHA256,
    });
  }
  let bytes = 0n;
  const digest = createHash("sha256");
  digest.update(`${domain}\n`, "utf8");
  for (const artifact of artifacts) {
    const path = Buffer.from(artifact.relativePath, "utf8");
    bytes += BigInt(artifact.sizeBytes);
    if (bytes > MAXIMUM_U64) throw invalidEvidence();
    digest.update(`path_bytes=${path.length}:`, "utf8");
    digest.update(path);
    digest.update("\n", "utf8");
    digest.update(`classification=${artifact.classification}\n`, "utf8");
    digest.update(`kind=${artifact.kind}\n`, "utf8");
    digest.update(`device_id=${artifact.deviceId}\n`, "utf8");
    digest.update(`link_count=${artifact.linkCount}\n`, "utf8");
    digest.update(`size_bytes=${artifact.sizeBytes}\n`, "utf8");
    digest.update(`sha256=${artifact.sha256}\n`, "utf8");
  }
  return Object.freeze({
    entryCount: artifacts.length,
    fileBytes: bytes.toString(),
    sha256: digest.digest("hex"),
  });
}

function legacyArtifactMatchesLocator(
  relativePath: string,
  locator: AccountResidencyLocator,
): boolean {
  if (locator.kind === "profile") {
    return (
      isPathOrDescendant(relativePath, `active/${locator.scope}`) ||
      isExactOrDotRemnant(
        relativePath,
        "active",
        `${locator.scope}.restore.tar.gz`,
      ) ||
      isExactOrDotRemnant(
        relativePath,
        "active",
        `${locator.scope}.seal.tar.gz`,
      ) ||
      isExactOrDotRemnant(
        relativePath,
        "snapshots",
        `${locator.scope}.tar.gz.enc`,
      ) ||
      isExactOrDotRemnant(
        relativePath,
        "snapshots",
        `${locator.scope}.generation`,
      ) ||
      isPathOrDescendant(relativePath, `run-checkpoints/${locator.scope}`) ||
      isPathOrDescendant(relativePath, `receipts/${locator.scope}`)
    );
  }
  return isExactOrDotRemnant(
    relativePath,
    "step-results",
    `${locator.scope}.json.enc`,
  );
}

function isPathOrDescendant(candidate: string, root: string): boolean {
  return candidate === root || candidate.startsWith(`${root}/`);
}

function isExactOrDotRemnant(
  candidate: string,
  directory: string,
  name: string,
): boolean {
  const exact = `${directory}/${name}`;
  return candidate === exact || candidate.startsWith(`${exact}.`);
}

function validateSnapshotBinding(inventory: SubjectStorageInventory): void {
  if (
    !inventory ||
    !inventory.runnerRootInventory ||
    !inventory.legacyInventory ||
    inventory.runnerRootInventory.deviceId !==
      inventory.legacyInventory.rootDeviceId ||
    inventory.runnerRootInventory.linkCount !==
      inventory.legacyInventory.rootLinkCount ||
    inventory.runnerRootInventory.entryCount !==
      inventory.legacyInventory.nativeInventoryCount ||
    inventory.runnerRootInventory.fileBytes !==
      String(inventory.legacyInventory.nativeInventoryBytes) ||
    inventory.runnerRootInventory.sha256 !==
      inventory.legacyInventory.nativeInventorySha256
  ) {
    throw invalidEvidence();
  }
  parseRootSnapshot(rootSnapshot(inventory));
  parseSubjectSnapshot(subjectStorageSnapshot(inventory));
  parseLegacyRootSnapshot(legacyRootSnapshot(inventory.legacyInventory));
}

function validateBeforeEvidence(
  evidence: RunnerPurgeStorageBeforeEvidence,
): void {
  if (
    (evidence.subjectStorage.before.residency === "resident" &&
      evidence.subjectStorage.before.ownership.entryCount !==
        evidence.locators.count) ||
    (evidence.subjectStorage.before.residency === "never_resident" &&
      evidence.subjectStorage.before.ownership.entryCount !== 0) ||
    evidence.legacy.rootBefore.unclassifiedRootCount !== 0
  ) {
    throw invalidEvidence();
  }
  validateSnapshotSubsets(
    evidence.root.before,
    evidence.subjectStorage.before.completeRoot,
    evidence.legacy.rootBefore,
  );
  validateInventorySubset(
    evidence.legacy.targetBefore,
    evidence.legacy.rootBefore,
  );
}

function validateCompleteEvidence(evidence: RunnerPurgeStorageEvidence): void {
  validateBeforeEvidence(
    Object.freeze({
      version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
      root: Object.freeze({
        deviceId: evidence.root.deviceId,
        before: evidence.root.before,
      }),
      locators: evidence.locators,
      subjectStorage: Object.freeze({
        layoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
        before: evidence.subjectStorage.before,
      }),
      legacy: Object.freeze({
        inventoryVersion: RUNNER_LEGACY_INVENTORY_VERSION,
        targetBefore: evidence.legacy.targetBefore,
        rootBefore: evidence.legacy.rootBefore,
      }),
    }),
  );
  validateSnapshotSubsets(
    evidence.root.after,
    evidence.subjectStorage.after.completeRoot,
    evidence.legacy.rootAfter,
  );
  validateInventorySubset(
    evidence.legacy.targetAfter,
    evidence.legacy.rootAfter,
  );
  if (
    evidence.subjectStorage.after.residency !== "never_resident" ||
    !isEmptyInventory(
      evidence.subjectStorage.after.subjectTree,
      EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    ) ||
    !isEmptyInventory(
      evidence.subjectStorage.after.ownership,
      EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    ) ||
    !isEmptyInventory(
      evidence.subjectStorage.after.target,
      EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    ) ||
    !isEmptyInventory(
      evidence.legacy.targetAfter,
      EMPTY_RUNNER_INVENTORY_SHA256,
    ) ||
    evidence.legacy.rootAfter.artifactCount !== 0 ||
    evidence.legacy.rootAfter.artifactBytes !== "0" ||
    evidence.legacy.rootAfter.artifactSetSha256 !==
      EMPTY_LEGACY_ARTIFACT_SET_SHA256 ||
    evidence.legacy.rootAfter.unclassifiedRootCount !== 0
  ) {
    throw invalidEvidence();
  }
}

function validateSnapshotSubsets(
  root: RunnerPurgeRootSnapshotEvidence,
  subjectRoot: RunnerPurgeInventoryEvidence,
  legacyRoot: RunnerPurgeLegacyRootSnapshotEvidence,
): void {
  if (
    checkedSafeSum(subjectRoot.entryCount, legacyRoot.artifactCount) >
      root.entryCount ||
    parseU64(subjectRoot.fileBytes) + parseU64(legacyRoot.artifactBytes) >
      parseU64(root.fileBytes)
  ) {
    throw invalidEvidence();
  }
}

function validateInventorySubset(
  target: RunnerPurgeInventoryEvidence,
  legacyRoot: RunnerPurgeLegacyRootSnapshotEvidence,
): void {
  if (
    target.entryCount > legacyRoot.artifactCount ||
    parseU64(target.fileBytes) > parseU64(legacyRoot.artifactBytes)
  ) {
    throw invalidEvidence();
  }
}

function parseRootSnapshot(input: unknown): RunnerPurgeRootSnapshotEvidence {
  const value = exactRecord(input, [
    "entryCount",
    "fileBytes",
    "linkCount",
    "sha256",
  ]);
  const linkCount = parseSafeInteger(value.linkCount, true);
  const inventory = parseInventory(value, EMPTY_RUNNER_INVENTORY_SHA256, [
    "linkCount",
  ]);
  return Object.freeze({ linkCount, ...inventory });
}

function parseSubjectSnapshot(
  input: unknown,
): RunnerPurgeSubjectStorageSnapshotEvidence {
  const value = exactRecord(input, [
    "completeRoot",
    "ownership",
    "residency",
    "subjectTree",
    "target",
  ]);
  if (value.residency !== "never_resident" && value.residency !== "resident") {
    throw invalidEvidence();
  }
  const parsed = Object.freeze({
    residency: value.residency,
    subjectTree: parseInventory(
      value.subjectTree,
      EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    ),
    ownership: parseInventory(
      value.ownership,
      EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    ),
    target: parseInventory(
      value.target,
      EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    ),
    completeRoot: parseInventory(
      value.completeRoot,
      EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    ),
  });
  if (
    parsed.target.entryCount !==
      checkedSafeSum(
        parsed.subjectTree.entryCount,
        parsed.ownership.entryCount,
      ) ||
    parseU64(parsed.target.fileBytes) !==
      parseU64(parsed.subjectTree.fileBytes) +
        parseU64(parsed.ownership.fileBytes) ||
    parsed.completeRoot.entryCount < parsed.target.entryCount ||
    parseU64(parsed.completeRoot.fileBytes) <
      parseU64(parsed.target.fileBytes) ||
    (parsed.residency === "never_resident" &&
      !isEmptyInventory(
        parsed.target,
        EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      )) ||
    (parsed.residency === "resident" && parsed.target.entryCount === 0)
  ) {
    throw invalidEvidence();
  }
  return parsed;
}

function parseLegacyRootSnapshot(
  input: unknown,
): RunnerPurgeLegacyRootSnapshotEvidence {
  const value = exactRecord(input, [
    "artifactBytes",
    "artifactCount",
    "artifactSetSha256",
    "unclassifiedRootCount",
  ]);
  const parsed = Object.freeze({
    artifactCount: parseSafeInteger(value.artifactCount),
    artifactBytes: parseDecimalU64(value.artifactBytes),
    artifactSetSha256: parseSha256(value.artifactSetSha256),
    unclassifiedRootCount: parseSafeInteger(value.unclassifiedRootCount),
  });
  if (
    parsed.unclassifiedRootCount > parsed.artifactCount ||
    (parsed.artifactCount === 0 &&
      (parsed.artifactBytes !== "0" ||
        parsed.unclassifiedRootCount !== 0 ||
        parsed.artifactSetSha256 !== EMPTY_LEGACY_ARTIFACT_SET_SHA256)) ||
    (parsed.artifactCount > 0 &&
      parsed.artifactSetSha256 === EMPTY_LEGACY_ARTIFACT_SET_SHA256)
  ) {
    throw invalidEvidence();
  }
  return parsed;
}

function parseInventory(
  input: unknown,
  emptySha256: string,
  extraKeys: readonly string[] = [],
): RunnerPurgeInventoryEvidence {
  const value = exactRecord(input, [
    "entryCount",
    "fileBytes",
    "sha256",
    ...extraKeys,
  ]);
  const parsed = Object.freeze({
    entryCount: parseSafeInteger(value.entryCount),
    fileBytes: parseDecimalU64(value.fileBytes),
    sha256: parseSha256(value.sha256),
  });
  if (
    (parsed.entryCount === 0 &&
      (parsed.fileBytes !== "0" || parsed.sha256 !== emptySha256)) ||
    (parsed.entryCount > 0 && parsed.sha256 === emptySha256)
  ) {
    throw invalidEvidence();
  }
  return parsed;
}

function parseLocatorEvidence(input: unknown): RunnerPurgeLocatorEvidence {
  const value = exactRecord(input, ["count", "sha256"]);
  return Object.freeze({
    count: parseSafeInteger(value.count),
    sha256: parseSha256(value.sha256),
  });
}

function cloneInventory(
  input: SubjectStorageInventoryEvidence,
): RunnerPurgeInventoryEvidence {
  return parseInventory(input, EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256);
}

function emptySubjectInventory(): SubjectStorageInventoryEvidence {
  return Object.freeze({
    entryCount: 0,
    fileBytes: "0",
    sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
  });
}

function canonicalRootSnapshot(
  prefix: string,
  value: RunnerPurgeRootSnapshotEvidence,
): readonly string[] {
  return [
    `${prefix}_link_count=${value.linkCount}`,
    `${prefix}_entry_count=${value.entryCount}`,
    `${prefix}_file_bytes=${value.fileBytes}`,
    `${prefix}_sha256=${value.sha256}`,
  ];
}

function canonicalInventory(
  prefix: string,
  value: RunnerPurgeInventoryEvidence,
): readonly string[] {
  return [
    `${prefix}_entry_count=${value.entryCount}`,
    `${prefix}_file_bytes=${value.fileBytes}`,
    `${prefix}_sha256=${value.sha256}`,
  ];
}

function canonicalLegacyRoot(
  prefix: string,
  value: RunnerPurgeLegacyRootSnapshotEvidence,
): readonly string[] {
  return [
    `${prefix}_artifact_count=${value.artifactCount}`,
    `${prefix}_artifact_bytes=${value.artifactBytes}`,
    `${prefix}_artifact_set_sha256=${value.artifactSetSha256}`,
    `${prefix}_unclassified_root_count=${value.unclassifiedRootCount}`,
  ];
}

function validateLocator(locator: AccountResidencyLocator): void {
  if (
    !locator ||
    (locator.kind !== "profile" && locator.kind !== "result") ||
    (locator.kind === "profile"
      ? !PROFILE_SCOPE_PATTERN.test(locator.scope)
      : !RESULT_SCOPE_PATTERN.test(locator.scope)) ||
    typeof locator.signature !== "string" ||
    locator.signature.length === 0
  ) {
    throw invalidEvidence();
  }
}

function validateLegacyArtifact(
  artifact: LegacyRunnerArtifactEvidence,
  rootDeviceId: string,
): void {
  if (
    !artifact ||
    typeof artifact.relativePath !== "string" ||
    artifact.relativePath.length === 0 ||
    artifact.relativePath.startsWith("/") ||
    artifact.relativePath
      .split("/")
      .some(
        (component) =>
          component.length === 0 || component === "." || component === "..",
      ) ||
    (artifact.kind !== "directory" && artifact.kind !== "file") ||
    artifact.deviceId !== rootDeviceId ||
    !Number.isSafeInteger(artifact.linkCount) ||
    artifact.linkCount < 1 ||
    !Number.isSafeInteger(artifact.sizeBytes) ||
    artifact.sizeBytes < 0 ||
    !SHA256_PATTERN.test(artifact.sha256)
  ) {
    throw invalidEvidence();
  }
}

function exactRecord(
  input: unknown,
  expectedKeys: readonly string[],
): Record<string, unknown> {
  if (typeof input !== "object" || input === null || Array.isArray(input)) {
    throw invalidEvidence();
  }
  const value = input as Record<string, unknown>;
  const keys = Object.keys(value).sort(compareUtf8);
  const expected = [...expectedKeys].sort(compareUtf8);
  if (
    keys.length !== expected.length ||
    keys.some((key, index) => key !== expected[index])
  ) {
    throw invalidEvidence();
  }
  return value;
}

function parseSafeInteger(input: unknown, positive = false): number {
  if (
    typeof input !== "number" ||
    !Number.isSafeInteger(input) ||
    input < (positive ? 1 : 0) ||
    input > MAXIMUM_SAFE_INTEGER
  ) {
    throw invalidEvidence();
  }
  return input;
}

function parseDecimalU64(input: unknown): string {
  if (typeof input !== "string" || !DECIMAL_U64_PATTERN.test(input)) {
    throw invalidEvidence();
  }
  const parsed = BigInt(input);
  if (parsed > MAXIMUM_U64) throw invalidEvidence();
  return input;
}

function parseU64(input: string): bigint {
  parseDecimalU64(input);
  return BigInt(input);
}

function parseSha256(input: unknown): string {
  if (typeof input !== "string" || !SHA256_PATTERN.test(input))
    throw invalidEvidence();
  return input;
}

function parseDeviceId(input: unknown): string {
  if (typeof input !== "string" || !DEVICE_ID_PATTERN.test(input))
    throw invalidEvidence();
  return input;
}

function isEmptyInventory(
  inventory: RunnerPurgeInventoryEvidence,
  emptySha256: string,
): boolean {
  return (
    inventory.entryCount === 0 &&
    inventory.fileBytes === "0" &&
    inventory.sha256 === emptySha256
  );
}

function checkedSafeSum(left: number, right: number): number {
  const sum = left + right;
  if (!Number.isSafeInteger(sum) || sum > MAXIMUM_SAFE_INTEGER)
    throw invalidEvidence();
  return sum;
}

function compareUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function invalidEvidence(): RunnerPurgeStorageEvidenceError {
  return new RunnerPurgeStorageEvidenceError();
}
