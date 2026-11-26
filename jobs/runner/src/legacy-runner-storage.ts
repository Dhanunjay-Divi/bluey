import { createHash } from "node:crypto";
import {
  ACCOUNT_DATA_V2_DIRECTORY,
  LEGACY_ARTIFACT_ROOTS,
  RUNNER_CONTROL_DIRECTORIES,
  RUNNER_CONTROL_FILES,
} from "./subject-storage-layout.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerStorageRoot,
} from "./native-runner-storage.js";

const MAXIMUM_LEGACY_ARTIFACTS = 100_000;
const LEGACY_ROOTS = new Set<string>(LEGACY_ARTIFACT_ROOTS);
const CONTROL_DIRECTORY_ROOTS = new Set<string>(RUNNER_CONTROL_DIRECTORIES);
const CONTROL_FILE_ROOTS = new Set<string>(RUNNER_CONTROL_FILES);
export const EMPTY_LEGACY_ARTIFACT_SET_SHA256 = createHash("sha256")
  .update("bluey-jobs-runner-legacy-artifact-set-v1\n", "utf8")
  .digest("hex");

export type LegacyRunnerStorageErrorCode =
  | "inventory_changed"
  | "invalid_control_root"
  | "invalid_inventory";

export class LegacyRunnerStorageError extends Error {
  constructor(readonly code: LegacyRunnerStorageErrorCode) {
    super({
      inventory_changed: "The runner root changed while legacy storage was inventoried.",
      invalid_control_root: "A runner storage control root has an unsafe entry kind.",
      invalid_inventory: "The native runner storage inventory is invalid.",
    }[code]);
    this.name = "LegacyRunnerStorageError";
  }
}

export interface LegacyRunnerArtifactEvidence {
  readonly relativePath: string;
  readonly kind: "directory" | "file";
  readonly deviceId: string;
  readonly linkCount: number;
  readonly sizeBytes: number;
  readonly sha256: string;
  readonly classification: "known_legacy" | "unclassified_root";
}

export interface LegacyRunnerStorageInventory {
  readonly version: 1;
  readonly rootDeviceId: string;
  readonly rootLinkCount: number;
  readonly nativeInventorySha256: string;
  readonly nativeInventoryCount: number;
  readonly nativeInventoryBytes: number;
  readonly legacyArtifactCount: number;
  readonly legacyArtifactBytes: number;
  readonly legacyInventorySha256: string;
  readonly legacyArtifactSetSha256: string;
  readonly artifacts: readonly LegacyRunnerArtifactEvidence[];
  readonly unclassifiedRootPaths: readonly string[];
}

/**
 * Takes two identical full native-root inventories and derives the enrollment
 * legacy count. Empty known legacy scaffold directories do not count; every
 * descendant and every unclassified root entry does, so absence cannot be
 * inferred from an environment variable or a partial locator scan.
 */
export async function scanLegacyRunnerStorage(
  root: NativeRunnerStorageRoot,
): Promise<LegacyRunnerStorageInventory> {
  root.assertUnchanged();
  const initialDeviceId = root.deviceId;
  const initialLinkCount = root.linkCount;
  const rootDirectory = await root.openDirectory([]);
  if (rootDirectory.relativePath !== ""
    || rootDirectory.deviceId !== initialDeviceId
    || rootDirectory.linkCount !== initialLinkCount) {
    throw new LegacyRunnerStorageError("invalid_inventory");
  }
  const first = await rootDirectory.inventory();
  const second = await rootDirectory.inventory();
  root.assertUnchanged();
  if (root.deviceId !== initialDeviceId
    || root.linkCount !== initialLinkCount
    || !inventoriesEqual(first, second)) {
    throw new LegacyRunnerStorageError("inventory_changed");
  }
  return classifyLegacyRunnerInventory(
    first,
    initialDeviceId,
    initialLinkCount,
  );
}

export function classifyLegacyRunnerInventory(
  inventory: NativeRunnerInventory,
  rootDeviceId: string,
  rootLinkCount: number,
): LegacyRunnerStorageInventory {
  if (!Number.isSafeInteger(rootLinkCount) || rootLinkCount < 1) {
    throw new LegacyRunnerStorageError("invalid_inventory");
  }
  validateNativeInventory(inventory);
  const rootEntries = new Map<string, NativeRunnerInventoryEntry>();
  for (const entry of inventory.entries) {
    if (entry.deviceId !== rootDeviceId) {
      throw new LegacyRunnerStorageError("invalid_inventory");
    }
    if (!entry.relativePath.includes("/")) rootEntries.set(entry.relativePath, entry);
  }
  validateRootKinds(rootEntries);

  const artifacts: LegacyRunnerArtifactEvidence[] = [];
  const unclassified = new Set<string>();
  for (const entry of inventory.entries) {
    const [rootName] = entry.relativePath.split("/", 1);
    if (!rootName) throw new LegacyRunnerStorageError("invalid_inventory");
    if (LEGACY_ROOTS.has(rootName)) {
      if (entry.relativePath === rootName && entry.kind === "directory") continue;
      artifacts.push(artifactEvidence(entry, "known_legacy"));
      continue;
    }
    if (isManagedRoot(rootName)) continue;
    artifacts.push(artifactEvidence(entry, "unclassified_root"));
    unclassified.add(rootName);
  }
  if (artifacts.length > MAXIMUM_LEGACY_ARTIFACTS) {
    throw new LegacyRunnerStorageError("invalid_inventory");
  }
  const legacyArtifactBytes = artifacts.reduce((total, entry) => {
    const next = total + entry.sizeBytes;
    if (!Number.isSafeInteger(next)) {
      throw new LegacyRunnerStorageError("invalid_inventory");
    }
    return next;
  }, 0);
  const legacyInventorySha256 = hashLegacyInventory(
    rootDeviceId,
    rootLinkCount,
    inventory,
    artifacts,
  );
  const legacyArtifactSetSha256 = hashLegacyArtifactSet(artifacts);
  return Object.freeze({
    version: 1 as const,
    rootDeviceId,
    rootLinkCount,
    nativeInventorySha256: inventory.sha256,
    nativeInventoryCount: inventory.count,
    nativeInventoryBytes: inventory.bytes,
    legacyArtifactCount: artifacts.length,
    legacyArtifactBytes,
    legacyInventorySha256,
    legacyArtifactSetSha256,
    artifacts: Object.freeze(artifacts),
    unclassifiedRootPaths: Object.freeze([...unclassified].sort(compareCanonicalUtf8)),
  });
}

function validateNativeInventory(inventory: NativeRunnerInventory): void {
  if (!inventory || !Array.isArray(inventory.entries)
    || inventory.count !== inventory.entries.length
    || !Number.isSafeInteger(inventory.bytes)
    || inventory.bytes < 0
    || !/^[0-9a-f]{64}$/.test(inventory.sha256)) {
    throw new LegacyRunnerStorageError("invalid_inventory");
  }
  let bytes = 0;
  let previous: Buffer | undefined;
  for (const entry of inventory.entries) {
    const path = Buffer.from(entry.relativePath, "utf8");
    if (previous && Buffer.compare(previous, path) >= 0) {
      throw new LegacyRunnerStorageError("invalid_inventory");
    }
    previous = path;
    bytes += entry.sizeBytes;
    if (!Number.isSafeInteger(bytes)) {
      throw new LegacyRunnerStorageError("invalid_inventory");
    }
  }
  if (bytes !== inventory.bytes) throw new LegacyRunnerStorageError("invalid_inventory");
}

function validateRootKinds(
  rootEntries: ReadonlyMap<string, NativeRunnerInventoryEntry>,
): void {
  for (const name of [...LEGACY_ROOTS, ...RUNNER_CONTROL_DIRECTORIES, ACCOUNT_DATA_V2_DIRECTORY]) {
    const entry = rootEntries.get(name);
    if (entry && entry.kind !== "directory") {
      throw new LegacyRunnerStorageError("invalid_control_root");
    }
  }
  for (const name of RUNNER_CONTROL_FILES) {
    const entry = rootEntries.get(name);
    if (entry && entry.kind !== "file") {
      throw new LegacyRunnerStorageError("invalid_control_root");
    }
  }
}

function isManagedRoot(rootName: string): boolean {
  return rootName === ACCOUNT_DATA_V2_DIRECTORY
    || CONTROL_DIRECTORY_ROOTS.has(rootName)
    || CONTROL_FILE_ROOTS.has(rootName);
}

function artifactEvidence(
  entry: NativeRunnerInventoryEntry,
  classification: LegacyRunnerArtifactEvidence["classification"],
): LegacyRunnerArtifactEvidence {
  return Object.freeze({
    relativePath: entry.relativePath,
    kind: entry.kind,
    deviceId: entry.deviceId,
    linkCount: entry.linkCount,
    sizeBytes: entry.sizeBytes,
    sha256: entry.sha256,
    classification,
  });
}

function hashLegacyInventory(
  rootDeviceId: string,
  rootLinkCount: number,
  inventory: NativeRunnerInventory,
  artifacts: readonly LegacyRunnerArtifactEvidence[],
): string {
  const digest = createHash("sha256");
  digest.update("bluey-jobs-runner-legacy-inventory-v1\0", "utf8");
  digest.update(`root_device_id=${rootDeviceId}\n`, "utf8");
  digest.update(`root_link_count=${rootLinkCount}\n`, "utf8");
  digest.update(`native_inventory_sha256=${inventory.sha256}\n`, "utf8");
  digest.update(`native_inventory_count=${inventory.count}\n`, "utf8");
  digest.update(`native_inventory_bytes=${inventory.bytes}\n`, "utf8");
  digest.update(`legacy_artifact_count=${artifacts.length}\n`, "utf8");
  for (const entry of artifacts) {
    const pathBytes = Buffer.from(entry.relativePath, "utf8");
    digest.update(`path_bytes=${pathBytes.length}:`, "utf8");
    digest.update(pathBytes);
    digest.update("\n", "utf8");
    digest.update(`classification=${entry.classification}\n`, "utf8");
    digest.update(`kind=${entry.kind}\n`, "utf8");
    digest.update(`device_id=${entry.deviceId}\n`, "utf8");
    digest.update(`link_count=${entry.linkCount}\n`, "utf8");
    digest.update(`size_bytes=${entry.sizeBytes}\n`, "utf8");
    digest.update(`sha256=${entry.sha256}\n`, "utf8");
  }
  return digest.digest("hex");
}

function hashLegacyArtifactSet(
  artifacts: readonly LegacyRunnerArtifactEvidence[],
): string {
  if (artifacts.length === 0) return EMPTY_LEGACY_ARTIFACT_SET_SHA256;
  const digest = createHash("sha256");
  digest.update("bluey-jobs-runner-legacy-artifact-set-v1\n", "utf8");
  for (const entry of artifacts) {
    const pathBytes = Buffer.from(entry.relativePath, "utf8");
    digest.update(`path_bytes=${pathBytes.length}:`, "utf8");
    digest.update(pathBytes);
    digest.update("\n", "utf8");
    digest.update(`classification=${entry.classification}\n`, "utf8");
    digest.update(`kind=${entry.kind}\n`, "utf8");
    digest.update(`device_id=${entry.deviceId}\n`, "utf8");
    digest.update(`link_count=${entry.linkCount}\n`, "utf8");
    digest.update(`size_bytes=${entry.sizeBytes}\n`, "utf8");
    digest.update(`sha256=${entry.sha256}\n`, "utf8");
  }
  return digest.digest("hex");
}

function inventoriesEqual(
  left: NativeRunnerInventory,
  right: NativeRunnerInventory,
): boolean {
  if (left.count !== right.count
    || left.bytes !== right.bytes
    || left.sha256 !== right.sha256
    || left.entries.length !== right.entries.length) {
    return false;
  }
  return left.entries.every((entry, index) => {
    const candidate = right.entries[index];
    return candidate !== undefined
      && entry.relativePath === candidate.relativePath
      && entry.kind === candidate.kind
      && entry.deviceId === candidate.deviceId
      && entry.linkCount === candidate.linkCount
      && entry.sizeBytes === candidate.sizeBytes
      && entry.sha256 === candidate.sha256;
  });
}

function compareCanonicalUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}
