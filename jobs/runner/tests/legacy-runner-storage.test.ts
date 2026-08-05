import { describe, expect, it } from "vitest";
import {
  classifyLegacyRunnerInventory,
  EMPTY_LEGACY_ARTIFACT_SET_SHA256,
  LegacyRunnerStorageError,
  scanLegacyRunnerStorage,
} from "../src/legacy-runner-storage.js";
import type {
  NativeRunnerInventory,
  NativeRunnerStorageDirectory,
  NativeRunnerStorageRoot,
} from "../src/native-runner-storage.js";

const DEVICE_ID = "unix:602:mount:7";
const FILE_SHA = "a".repeat(64);
const DIRECTORY_SHA = "b".repeat(64);

describe("legacy runner storage inventory", () => {
  it("derives the enrollment count from every legacy descendant and unknown root", () => {
    const inventory = nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 0, FILE_SHA),
      entry("account-data-v2", "directory", 0, DIRECTORY_SHA),
      entry("active", "directory", 0, DIRECTORY_SHA),
      entry("active/profile-a", "directory", 0, DIRECTORY_SHA),
      entry("active/profile-a/Local State", "file", 5, FILE_SHA),
      entry("snapshots", "directory", 0, DIRECTORY_SHA),
      entry("unknown-cache", "directory", 0, DIRECTORY_SHA),
      entry("unknown-cache/value", "file", 3, FILE_SHA),
      entry("volume-identity", "directory", 0, DIRECTORY_SHA),
      entry("volume-identity/identity.json", "file", 7, FILE_SHA),
    ]);

    const result = classifyLegacyRunnerInventory(inventory, DEVICE_ID, 6);
    expect(result.legacyArtifactCount).toBe(4);
    expect(result.legacyArtifactBytes).toBe(8);
    expect(result.artifacts.map((value) => value.relativePath)).toEqual([
      "active/profile-a",
      "active/profile-a/Local State",
      "unknown-cache",
      "unknown-cache/value",
    ]);
    expect(result.unclassifiedRootPaths).toEqual(["unknown-cache"]);
    expect(result.legacyInventorySha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it("treats empty known legacy scaffolds as zero but rejects a wrong root kind", () => {
    const clean = classifyLegacyRunnerInventory(nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 0, FILE_SHA),
      entry("active", "directory", 0, DIRECTORY_SHA),
      entry("receipts", "directory", 0, DIRECTORY_SHA),
      entry("runner-volume-control-v1", "directory", 0, DIRECTORY_SHA),
    ]), DEVICE_ID, 4);
    expect(clean.legacyArtifactCount).toBe(0);

    expect(() => classifyLegacyRunnerInventory(nativeInventory([
      entry("account-residency-v1", "file", 1, FILE_SHA),
    ]), DEVICE_ID, 2)).toThrowError(LegacyRunnerStorageError);
  });

  it("keeps the empty artifact-set hash independent of root controls", () => {
    const initial = classifyLegacyRunnerInventory(nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 0, FILE_SHA),
      entry("active", "directory", 0, DIRECTORY_SHA),
    ]), DEVICE_ID, 2);
    const changedControls = classifyLegacyRunnerInventory(nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 4, "d".repeat(64)),
      entry("account-data-v2", "directory", 0, DIRECTORY_SHA),
      entry("account-data-v2/layout-control", "file", 8, "e".repeat(64)),
      entry("active", "directory", 0, DIRECTORY_SHA),
      entry("runner-volume-control-v1", "directory", 0, DIRECTORY_SHA),
      entry("runner-volume-control-v1/journal.json", "file", 7, "f".repeat(64)),
      entry("volume-identity", "directory", 0, DIRECTORY_SHA),
      entry("volume-identity/identity.json", "file", 9, FILE_SHA),
    ]), DEVICE_ID, 9);

    expect(initial.legacyArtifactSetSha256).toBe(EMPTY_LEGACY_ARTIFACT_SET_SHA256);
    expect(changedControls.legacyArtifactSetSha256)
      .toBe(EMPTY_LEGACY_ARTIFACT_SET_SHA256);
    expect(changedControls.legacyArtifactSetSha256).toBe(initial.legacyArtifactSetSha256);
    expect(changedControls.legacyInventorySha256).not.toBe(initial.legacyInventorySha256);
  });

  it("changes the artifact-set hash only when legacy artifact evidence changes", () => {
    const initial = classifyLegacyRunnerInventory(nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 0, FILE_SHA),
      entry("active", "directory", 0, DIRECTORY_SHA),
      entry("active/profile-a", "file", 5, FILE_SHA),
    ]), DEVICE_ID, 3);
    const controlOnlyChange = classifyLegacyRunnerInventory(nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 6, "d".repeat(64)),
      entry("active", "directory", 0, DIRECTORY_SHA),
      entry("active/profile-a", "file", 5, FILE_SHA),
      entry("volume-identity", "directory", 0, DIRECTORY_SHA),
      entry("volume-identity/identity.json", "file", 9, "e".repeat(64)),
    ]), DEVICE_ID, 7);
    const artifactChange = classifyLegacyRunnerInventory(nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 6, "d".repeat(64)),
      entry("active", "directory", 0, DIRECTORY_SHA),
      entry("active/profile-a", "file", 5, "f".repeat(64)),
      entry("volume-identity", "directory", 0, DIRECTORY_SHA),
      entry("volume-identity/identity.json", "file", 9, "e".repeat(64)),
    ]), DEVICE_ID, 7);

    expect(initial.legacyArtifactSetSha256).not.toBe(EMPTY_LEGACY_ARTIFACT_SET_SHA256);
    expect(controlOnlyChange.legacyArtifactSetSha256).toBe(initial.legacyArtifactSetSha256);
    expect(artifactChange.legacyArtifactSetSha256).not.toBe(initial.legacyArtifactSetSha256);
  });

  it("requires identical full-root inventories and stable retained-root evidence", async () => {
    const first = nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 0, FILE_SHA),
    ]);
    const changed = nativeInventory([
      entry(".bluey-runner-storage.lock", "file", 0, FILE_SHA),
      entry("active", "directory", 0, DIRECTORY_SHA),
    ]);
    const root = fakeRoot([first, changed]);
    await expect(scanLegacyRunnerStorage(root))
      .rejects.toMatchObject({ code: "inventory_changed" });
  });
});

function entry(
  relativePath: string,
  kind: "directory" | "file",
  sizeBytes: number,
  sha256: string,
) {
  return {
    relativePath,
    kind,
    deviceId: DEVICE_ID,
    linkCount: kind === "file" ? 1 : 2,
    sizeBytes,
    sha256,
  } as const;
}

function nativeInventory(
  entries: NativeRunnerInventory["entries"],
): NativeRunnerInventory {
  const ordered = [...entries].sort((left, right) => (
    Buffer.compare(Buffer.from(left.relativePath), Buffer.from(right.relativePath))
  ));
  return Object.freeze({
    entries: Object.freeze(ordered),
    count: ordered.length,
    bytes: ordered.reduce((total, value) => total + value.sizeBytes, 0),
    sha256: String(ordered.length).padStart(64, "c").slice(-64),
  });
}

function fakeRoot(inventories: readonly NativeRunnerInventory[]): NativeRunnerStorageRoot {
  let call = 0;
  const directory = {
    relativePath: "",
    canonicalPath: "/srv/bluey-runner",
    deviceId: DEVICE_ID,
    linkCount: 2,
    inventory: () => Promise.resolve(inventories[Math.min(call++, inventories.length - 1)]!),
  } as unknown as NativeRunnerStorageDirectory;
  return {
    configuredPath: "/srv/bluey-runner",
    deviceId: DEVICE_ID,
    linkCount: 2,
    assertUnchanged: () => undefined,
    openDirectory: () => Promise.resolve(directory),
  } as unknown as NativeRunnerStorageRoot;
}
