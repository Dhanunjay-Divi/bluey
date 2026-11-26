import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gzipSync } from "node:zlib";
import * as tar from "tar";
import { describe, expect, it } from "vitest";
import { encryptBytes } from "../src/crypto-envelope.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerStorageDirectory,
} from "../src/native-runner-storage.js";
import {
  installManagedEncryptedProfileSnapshot,
  installEncryptedProfileSnapshot,
  managedProfilePaths,
  profilePathsFromScope,
  readEncryptedProfileSnapshot,
  readManagedEncryptedProfileSnapshot,
  restoreProfile,
  sealProfile,
  writeManagedProfileSnapshotGeneration,
  type EncryptedProfileSnapshot,
} from "../src/profile-store.js";
import { subjectStoragePaths } from "../src/subject-storage-layout.js";
import type { ManagedProfileStorage } from "../src/subject-storage-manager.js";

const ROOT = "/srv/bluey-runner";
const DEVICE_ID = "unix:602:mount:test";
const SUBJECT = "a".repeat(64);
const PROFILE_SCOPE = "b".repeat(40);
const KEY = Buffer.alloc(32, 0x42);

describe("managed-v2 browser profile store", () => {
  it("round-trips realistic Chromium names through retained handles", async () => {
    const fixture = createFixture();
    const paths = managedProfilePaths(fixture.profile);
    const crashReports =
      await fixture.profile.active.ensureChildDirectory("Crash Reports");
    const longExtensionName = `extension-${"x".repeat(160)}`;
    const extension =
      await fixture.profile.active.ensureChildDirectory(longExtensionName);
    await fixture.profile.active.replaceFile(
      "Local State",
      Buffer.from("browser-state"),
    );
    await crashReports.replaceFile(".metadata", Buffer.from("crash-state"));
    await extension.replaceFile("Café ☕", Buffer.from("unicode-state"));

    expect(paths.directory).toBe(fixture.profile.active.canonicalPath);
    expect(paths.encryptedSnapshot).toBe(
      fixture.profile.paths.encryptedSnapshot.relativePath,
    );
    await sealProfile(paths, KEY);

    expect((await fixture.profile.active.inventory()).count).toBe(0);
    const sealed = await readManagedEncryptedProfileSnapshot(paths);
    expect(sealed?.generation).toBe(0);
    expect(sealed?.bytes.subarray(0, 8).toString("ascii")).toBe("BLUEYJP2");
    expect(sealed?.bytes.includes(Buffer.from("browser-state"))).toBe(false);
    await writeManagedProfileSnapshotGeneration(paths, 1);
    expect((await readManagedEncryptedProfileSnapshot(paths))?.generation).toBe(
      1,
    );

    await restoreProfile(paths, KEY);
    expect(
      fixture.bytes(`${fixture.profile.paths.active.relativePath}/Local State`),
    ).toEqual(Buffer.from("browser-state"));
    expect(
      fixture.bytes(
        `${fixture.profile.paths.active.relativePath}/Crash Reports/.metadata`,
      ),
    ).toEqual(Buffer.from("crash-state"));
    expect(
      fixture.bytes(
        `${fixture.profile.paths.active.relativePath}/${longExtensionName}/Café ☕`,
      ),
    ).toEqual(Buffer.from("unicode-state"));
    expect(
      fixture.mutations.some((value) =>
        value.endsWith("snapshots/profile.tar.gz.enc"),
      ),
    ).toBe(true);
    sealed?.bytes.fill(0);
  });

  it("installs a replacement-runner snapshot and rejects generation conflicts", async () => {
    const source = createFixture();
    const sourcePaths = managedProfilePaths(source.profile);
    await source.profile.active.replaceFile(
      "Cookies",
      Buffer.from("replacement-session"),
    );
    await sealProfile(sourcePaths, KEY);
    const local = await readManagedEncryptedProfileSnapshot(sourcePaths);
    expect(local).toBeDefined();
    const remote: EncryptedProfileSnapshot = { ...local!, generation: 4 };

    const replacement = createFixture();
    const replacementPaths = managedProfilePaths(replacement.profile);
    await installManagedEncryptedProfileSnapshot(replacementPaths, remote);
    await restoreProfile(replacementPaths, KEY);
    expect(
      replacement.bytes(
        `${replacement.profile.paths.active.relativePath}/Cookies`,
      ),
    ).toEqual(Buffer.from("replacement-session"));

    const conflicting = Buffer.from(remote.bytes);
    conflicting[12] ^= 1;
    await expect(
      installManagedEncryptedProfileSnapshot(replacementPaths, {
        ...remote,
        bytes: conflicting,
      }),
    ).rejects.toMatchObject({ code: "snapshot_conflict" });
    await expect(
      installManagedEncryptedProfileSnapshot(replacementPaths, {
        ...remote,
        generation: 3,
      }),
    ).rejects.toMatchObject({ code: "snapshot_conflict" });
    conflicting.fill(0);
    local?.bytes.fill(0);
  });

  it("restores a legacy BLUEYJP2 tar snapshot through the managed handle boundary", async () => {
    const legacyRoot = await mkdtemp(
      join(tmpdir(), "bluey-managed-profile-legacy-"),
    );
    const legacyPaths = profilePathsFromScope(legacyRoot, PROFILE_SCOPE);
    try {
      await restoreProfile(legacyPaths, KEY);
      await writeFile(
        join(legacyPaths.directory, "Local State"),
        "legacy-state",
      );
      await sealProfile(legacyPaths, KEY);
      const legacy = await readEncryptedProfileSnapshot(legacyPaths);
      expect(legacy).toBeDefined();

      const fixture = createFixture();
      const managedPaths = managedProfilePaths(fixture.profile);
      await installManagedEncryptedProfileSnapshot(managedPaths, {
        ...legacy!,
        generation: 1,
      });
      await restoreProfile(managedPaths, KEY);
      expect(
        fixture.bytes(
          `${fixture.profile.paths.active.relativePath}/Local State`,
        ),
      ).toEqual(Buffer.from("legacy-state"));
      legacy?.bytes.fill(0);
    } finally {
      await rm(legacyRoot, { recursive: true, force: true });
    }
  });

  it("keeps managed snapshot bytes readable by the legacy migration path", async () => {
    const fixture = createFixture();
    const managedPaths = managedProfilePaths(fixture.profile);
    await fixture.profile.active.replaceFile(
      "Local State",
      Buffer.from("managed-state"),
    );
    await sealProfile(managedPaths, KEY);
    const managed = await readManagedEncryptedProfileSnapshot(managedPaths);
    expect(managed).toBeDefined();

    const legacyRoot = await mkdtemp(
      join(tmpdir(), "bluey-managed-profile-export-"),
    );
    const legacyPaths = profilePathsFromScope(legacyRoot, PROFILE_SCOPE);
    try {
      await installEncryptedProfileSnapshot(legacyPaths, {
        ...managed!,
        generation: 1,
      });
      await restoreProfile(legacyPaths, KEY);
      expect(
        await readFile(join(legacyPaths.directory, "Local State"), "utf8"),
      ).toBe("managed-state");
    } finally {
      managed?.bytes.fill(0);
      await rm(legacyRoot, { recursive: true, force: true });
    }
  });

  it("fails closed on an incomplete snapshot-generation publication", async () => {
    const fixture = createFixture();
    const paths = managedProfilePaths(fixture.profile);
    const encrypted = encryptedTar("Cookies", Buffer.from("state"));
    await fixture.profile.snapshots.replaceFile(
      "profile.tar.gz.enc",
      encrypted,
    );

    await expect(
      readManagedEncryptedProfileSnapshot(paths),
    ).rejects.toMatchObject({
      code: "snapshot_corrupt",
    });
    encrypted.fill(0);
  });

  it("validates every archive path before writing the active profile", async () => {
    const fixture = createFixture();
    const paths = managedProfilePaths(fixture.profile);
    const encrypted = encryptedTar("../escaped-cookie", Buffer.from("secret"));
    await installManagedEncryptedProfileSnapshot(paths, {
      bytes: encrypted,
      generation: 1,
      envelopeVersion: 2,
    });

    await expect(restoreProfile(paths, KEY)).rejects.toMatchObject({
      code: "unsafe_entry",
    });
    expect((await fixture.profile.active.inventory()).count).toBe(0);
    encrypted.fill(0);
  });

  it("does not publish when the retained active inventory changes during sealing", async () => {
    const fixture = createFixture();
    const paths = managedProfilePaths(fixture.profile);
    await fixture.profile.active.replaceFile("Cookies", Buffer.from("first"));
    fixture.mutateAfterInventory(fixture.profile.active.relativePath, 2, () => {
      fixture.seedFile(
        [...fixture.profile.paths.active.components, "late-write"],
        Buffer.from("changed"),
      );
    });

    await expect(sealProfile(paths, KEY)).rejects.toMatchObject({
      code: "binding_changed",
    });
    expect((await fixture.profile.snapshots.inventory()).count).toBe(0);
  });

  it("rejects a changed retained storage-device binding before mutation", async () => {
    const fixture = createFixture();
    const paths = managedProfilePaths(fixture.profile);
    fixture.deviceId = "unix:changed:mount:test";

    await expect(
      readManagedEncryptedProfileSnapshot(paths),
    ).rejects.toMatchObject({
      code: "binding_changed",
    });
    expect(fixture.mutations).toEqual([]);
  });
});

function encryptedTar(path: string, contents: Buffer): Buffer {
  const header = new tar.Header({
    path,
    mode: 0o600,
    uid: 0,
    gid: 0,
    size: contents.length,
    type: "File",
  });
  const headerBytes = Buffer.alloc(512);
  header.encode(headerBytes);
  const padding = Buffer.alloc((512 - (contents.length % 512)) % 512);
  const tarBytes = Buffer.concat([
    headerBytes,
    contents,
    padding,
    Buffer.alloc(1024),
  ]);
  try {
    const compressed = gzipSync(tarBytes);
    try {
      return encryptBytes(compressed, KEY, {
        purpose: "profile-snapshot",
        scope: PROFILE_SCOPE,
      });
    } finally {
      compressed.fill(0);
    }
  } finally {
    tarBytes.fill(0);
  }
}

interface MemoryDirectoryNode {
  readonly kind: "directory";
  readonly children: Map<string, MemoryNode>;
}

interface MemoryFileNode {
  readonly kind: "file";
  readonly contents: Buffer;
}

type MemoryNode = MemoryDirectoryNode | MemoryFileNode;

interface InventoryMutation {
  readonly call: number;
  readonly operation: () => void;
}

class MemoryStorage {
  deviceId = DEVICE_ID;
  readonly tree: MemoryDirectoryNode = directoryNode();
  readonly mutations: string[] = [];
  private readonly inventoryCalls = new Map<string, number>();
  private readonly inventoryMutations = new Map<string, InventoryMutation>();

  constructor(readonly root: string) {}

  directory(components: readonly string[]): MemoryDirectory {
    this.seedDirectory(components);
    return new MemoryDirectory(this, [...components]);
  }

  seedDirectory(components: readonly string[]): void {
    let current = this.tree;
    for (const component of components) {
      const existing = current.children.get(component);
      if (!existing) {
        const created = directoryNode();
        current.children.set(component, created);
        current = created;
      } else if (existing.kind === "directory") {
        current = existing;
      } else {
        throw new Error("file blocks directory");
      }
    }
  }

  seedFile(components: readonly string[], contents: Buffer): void {
    this.seedDirectory(components.slice(0, -1));
    this.directoryNode(components.slice(0, -1)).children.set(
      components.at(-1)!,
      { kind: "file", contents: Buffer.from(contents) },
    );
  }

  bytes(path: string): Buffer {
    const node = this.node(path.split("/"));
    if (!node || node.kind !== "file") throw new Error(`missing ${path}`);
    return Buffer.from(node.contents);
  }

  mutateAfterInventory(
    path: string,
    call: number,
    operation: () => void,
  ): void {
    this.inventoryMutations.set(path, { call, operation });
  }

  afterInventory(path: string): void {
    const call = (this.inventoryCalls.get(path) ?? 0) + 1;
    this.inventoryCalls.set(path, call);
    const mutation = this.inventoryMutations.get(path);
    if (mutation?.call === call) {
      this.inventoryMutations.delete(path);
      mutation.operation();
    }
  }

  directoryNode(components: readonly string[]): MemoryDirectoryNode {
    const node = this.node(components);
    if (!node || node.kind !== "directory")
      throw new Error("missing directory");
    return node;
  }

  node(components: readonly string[]): MemoryNode | undefined {
    let current: MemoryNode = this.tree;
    for (const component of components) {
      if (current.kind !== "directory") return undefined;
      const child = current.children.get(component);
      if (!child) return undefined;
      current = child;
    }
    return current;
  }
}

class MemoryDirectory implements NativeRunnerStorageDirectory {
  readonly linkCount = 2;

  constructor(
    private readonly storage: MemoryStorage,
    private readonly components: readonly string[],
  ) {}

  get relativePath(): string {
    return this.components.join("/");
  }

  get canonicalPath(): string {
    return join(this.storage.root, ...this.components);
  }

  get deviceId(): string {
    return this.storage.deviceId;
  }

  async ensureChildDirectory(
    name: string,
  ): Promise<NativeRunnerStorageDirectory> {
    const components = [...this.components, name];
    const existing = this.storage.node(components);
    if (existing?.kind === "file")
      throw new Error("bluey_runner_storage:unsafe_entry");
    if (!existing) {
      this.storage.seedDirectory(components);
      this.storage.mutations.push(`mkdir:${components.join("/")}`);
    }
    return new MemoryDirectory(this.storage, components);
  }

  async openChildDirectory(
    name: string,
  ): Promise<NativeRunnerStorageDirectory> {
    const components = [...this.components, name];
    this.storage.directoryNode(components);
    return new MemoryDirectory(this.storage, components);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    const parent = this.storage.directoryNode(this.components);
    if (parent.children.has(name)) return false;
    parent.children.set(name, {
      kind: "file",
      contents: Buffer.from(contents),
    });
    this.storage.mutations.push(
      `create:${[...this.components, name].join("/")}`,
    );
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    const parent = this.storage.directoryNode(this.components);
    if (parent.children.get(name)?.kind === "directory") {
      throw new Error("bluey_runner_storage:unsafe_entry");
    }
    parent.children.set(name, {
      kind: "file",
      contents: Buffer.from(contents),
    });
    this.storage.mutations.push(
      `replace:${[...this.components, name].join("/")}`,
    );
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    const node = this.storage.directoryNode(this.components).children.get(name);
    if (!node || node.kind !== "file")
      throw new Error("bluey_runner_storage:io_failure");
    if (node.contents.length > maximumBytes) {
      throw new Error("bluey_runner_storage:inventory_limit");
    }
    return Buffer.from(node.contents);
  }

  async inventory(): Promise<NativeRunnerInventory> {
    const entries: NativeRunnerInventoryEntry[] = [];
    collectInventory(
      this.storage.directoryNode(this.components),
      "",
      entries,
      this.deviceId,
    );
    entries.sort((left, right) =>
      compareUtf8(left.relativePath, right.relativePath),
    );
    const bytes = entries.reduce((total, entry) => total + entry.sizeBytes, 0);
    const inventory = Object.freeze({
      entries: Object.freeze(entries),
      count: entries.length,
      bytes,
      sha256: inventoryDigest(entries),
    });
    this.storage.afterInventory(this.relativePath);
    return inventory;
  }

  async removeEntry(name: string): Promise<void> {
    this.storage.directoryNode(this.components).children.delete(name);
    this.storage.mutations.push(
      `remove:${[...this.components, name].join("/")}`,
    );
  }
}

function createFixture(): MemoryStorage & {
  readonly profile: ManagedProfileStorage;
} {
  const storage = new MemoryStorage(ROOT);
  const paths = subjectStoragePaths(SUBJECT).profile(PROFILE_SCOPE);
  const profile: ManagedProfileStorage = Object.freeze({
    kind: "profile",
    subjectSha256: SUBJECT,
    scope: PROFILE_SCOPE,
    paths,
    root: storage.directory(paths.root.components),
    active: storage.directory(paths.active.components),
    snapshots: storage.directory(paths.snapshots.components),
    checkpoints: storage.directory(paths.checkpoints.components),
    receipts: storage.directory(paths.receipts.components),
    temporary: storage.directory(paths.temporary.components),
  });
  storage.mutations.length = 0;
  return Object.assign(storage, { profile });
}

function collectInventory(
  directory: MemoryDirectoryNode,
  prefix: string,
  entries: NativeRunnerInventoryEntry[],
  deviceId: string,
): void {
  for (const [name, node] of [...directory.children].sort(([left], [right]) =>
    compareUtf8(left, right),
  )) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (node.kind === "directory") {
      entries.push(
        Object.freeze({
          relativePath: path,
          kind: "directory",
          deviceId,
          linkCount: 2,
          sizeBytes: 0,
          sha256: createHash("sha256").update("directory").digest("hex"),
        }),
      );
      collectInventory(node, path, entries, deviceId);
    } else {
      entries.push(
        Object.freeze({
          relativePath: path,
          kind: "file",
          deviceId,
          linkCount: 1,
          sizeBytes: node.contents.length,
          sha256: createHash("sha256").update(node.contents).digest("hex"),
        }),
      );
    }
  }
}

function inventoryDigest(
  entries: readonly NativeRunnerInventoryEntry[],
): string {
  const digest = createHash("sha256");
  for (const entry of entries) {
    digest.update(
      [
        entry.relativePath,
        entry.kind,
        entry.deviceId,
        String(entry.linkCount),
        String(entry.sizeBytes),
        entry.sha256,
        "",
      ].join("\n"),
    );
  }
  return digest.digest("hex");
}

function compareUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function directoryNode(): MemoryDirectoryNode {
  return { kind: "directory", children: new Map() };
}
