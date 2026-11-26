import {
  createHash,
  createPrivateKey,
  createPublicKey,
  type KeyObject,
} from "node:crypto";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  canonicalAccountResidencyLocatorBytes,
  type AccountResidencyLocator,
} from "../src/account-residency.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerStorageDirectory,
  NativeRunnerStorageRoot,
} from "../src/native-runner-storage.js";
import {
  SubjectStorageManager,
  type LockedSubjectStorage,
  type SubjectStorageResidencyAuthority,
} from "../src/subject-storage-manager.js";
import { subjectStoragePaths } from "../src/subject-storage-layout.js";
import {
  ed25519PublicKeyFingerprint,
  signEd25519,
  type RunnerVolumeIdentity,
} from "../src/volume-identity.js";
import { runnerVolumeLocatorSetEvidence } from "../src/storage-attestation.js";

const ROOT = "/srv/bluey-runner";
const DEVICE_ID = "device-602";
const SUBJECT_A = "1".repeat(64);
const SUBJECT_B = "2".repeat(64);
const SUBJECT_C = "3".repeat(64);
const PROFILE_A = "a".repeat(40);
const PROFILE_B = "b".repeat(40);
const RESULT_A = "c".repeat(64);

describe("subject storage manager", () => {
  it("publishes v1 intent into exact v2 scaffolds with the immutable owner last", async () => {
    const fixture = createFixture([
      locator(SUBJECT_A, "profile", PROFILE_A),
      locator(SUBJECT_A, "result", RESULT_A),
    ]);

    const profile = await fixture.manager.ensureProfile(fixture.locators[0]!);
    const result = await fixture.manager.ensureResult(fixture.locators[1]!);

    expect(profile.active.relativePath).toBe(
      `account-data-v2/subjects/${SUBJECT_A}/profiles/${PROFILE_A}/active`,
    );
    expect(profile.temporary.relativePath).toBe(
      `account-data-v2/subjects/${SUBJECT_A}/profiles/${PROFILE_A}/temporary`,
    );
    expect(result.temporary.relativePath).toBe(
      `account-data-v2/subjects/${SUBJECT_A}/results/${RESULT_A}/temporary`,
    );
    const profileOwner = `account-data-v2/scope-owners/profiles/${PROFILE_A}.json`;
    const resultOwner = `account-data-v2/scope-owners/results/${RESULT_A}.json`;
    expect(fixture.storage.mutations.filter((event) => event.startsWith("create:")))
      .toContain(`create:${profileOwner}`);
    expect(fixture.storage.mutations.indexOf(`create:${profileOwner}`)).toBeGreaterThan(
      fixture.storage.mutations.indexOf(
        `create:account-data-v2/subjects/${SUBJECT_A}/subject.json`,
      ),
    );
    expect(fixture.storage.mutations.indexOf(`create:${resultOwner}`)).toBeGreaterThan(
      fixture.storage.mutations.indexOf(
        `mkdir:account-data-v2/subjects/${SUBJECT_A}/results/${RESULT_A}/temporary`,
      ),
    );

    await expect(fixture.manager.resolveProfile(PROFILE_A)).resolves.toMatchObject({
      subjectSha256: SUBJECT_A,
      scope: PROFILE_A,
    });
    await expect(fixture.manager.resolveResult(RESULT_A)).resolves.toMatchObject({
      subjectSha256: SUBJECT_A,
      scope: RESULT_A,
    });
    await expect(fixture.manager.inventorySubject(SUBJECT_A)).resolves.toMatchObject({
      residency: "resident",
      subject: {
        profileScopes: [PROFILE_A],
        resultScopes: [RESULT_A],
      },
    });
  });

  it("reconciles a crash-incomplete empty scaffold and exact owner replay", async () => {
    const profileLocator = locator(SUBJECT_A, "profile", PROFILE_A);
    const fixture = createFixture([profileLocator]);
    fixture.storage.seedDirectory([
      "account-data-v2",
      "subjects",
      SUBJECT_A,
      "profiles",
      PROFILE_A,
      "active",
    ]);

    const repaired = await fixture.manager.reconcileLocators([profileLocator]);
    expect(repaired).toEqual({
      createdOwners: 1,
      reconciledLocators: 1,
      skippedPurgedSubjects: [],
    });
    const ownerPath = `account-data-v2/scope-owners/profiles/${PROFILE_A}.json`;
    expect(fixture.storage.has(ownerPath)).toBe(true);
    expect(fixture.storage.mutations.at(-1)).toBe(`create:${ownerPath}`);
    await expect(fixture.manager.resolveProfile(PROFILE_A)).resolves.toMatchObject({
      subjectSha256: SUBJECT_A,
    });

    const ownerBytes = fixture.storage.bytes(ownerPath);
    const replayed = await fixture.manager.reconcileLocators([profileLocator]);
    expect(replayed.createdOwners).toBe(0);
    expect(fixture.storage.bytes(ownerPath)).toEqual(ownerBytes);
  });

  it("rejects a conflicting global owner before creating a second subject", async () => {
    const owner = locator(SUBJECT_B, "profile", PROFILE_A);
    const fixture = createFixture([owner]);
    await fixture.manager.ensureProfile(owner);

    const conflicting = locator(SUBJECT_A, "profile", PROFILE_A);
    fixture.residency.addLocator(conflicting);
    await expect(fixture.manager.ensureProfile(conflicting)).rejects.toMatchObject({
      code: "owner_conflict",
    });
    expect(fixture.storage.has(`account-data-v2/subjects/${SUBJECT_A}`)).toBe(false);
    await expect(fixture.manager.resolveProfile(PROFILE_A)).resolves.toMatchObject({
      subjectSha256: SUBJECT_B,
    });
  });

  it("fails closed on signed-control corruption and data in an incomplete scope", async () => {
    const profileLocator = locator(SUBJECT_A, "profile", PROFILE_A);
    const fixture = createFixture([profileLocator]);
    await fixture.manager.ensureProfile(profileLocator);
    fixture.storage.seedFile(
      ["account-data-v2", "subjects", SUBJECT_A, "subject.json"],
      Buffer.from("{\"corrupt\":true}\n"),
    );
    await expect(fixture.manager.ensureProfile(profileLocator)).rejects.toMatchObject({
      code: "audit_rejected",
    });

    const dirtyLocator = locator(SUBJECT_C, "profile", PROFILE_B);
    const dirty = createFixture([dirtyLocator]);
    dirty.storage.seedFile(
      [
        "account-data-v2",
        "subjects",
        SUBJECT_C,
        "profiles",
        PROFILE_B,
        "active",
        "Cookies",
      ],
      Buffer.from("account-data"),
    );
    await expect(dirty.manager.reconcileLocators([dirtyLocator])).rejects.toMatchObject({
      code: "corrupt_storage",
    });
    expect(dirty.storage.has(
      `account-data-v2/scope-owners/profiles/${PROFILE_B}.json`,
    )).toBe(false);
  });

  it("never recreates a fenced subject and detects restored target data", async () => {
    const profileLocator = locator(SUBJECT_A, "profile", PROFILE_A);
    const clean = createFixture([profileLocator]);
    clean.residency.barriers.add(SUBJECT_A);
    await expect(clean.manager.ensureProfile(profileLocator)).rejects.toMatchObject({
      code: "purged_subject",
    });
    const skipped = await clean.manager.reconcileLocators([profileLocator]);
    expect(skipped.skippedPurgedSubjects).toEqual([SUBJECT_A]);
    expect(clean.storage.has(`account-data-v2/subjects/${SUBJECT_A}`)).toBe(false);

    const restored = createFixture([profileLocator]);
    await restored.manager.ensureProfile(profileLocator);
    restored.residency.barriers.add(SUBJECT_A);
    await expect(restored.manager.reconcileLocators([profileLocator])).rejects.toMatchObject({
      code: "restored_subject",
    });
  });

  it("rejects a barrier installed during publication even after durable owner creation", async () => {
    const profileLocator = locator(SUBJECT_A, "profile", PROFILE_A);
    const fixture = createFixture([profileLocator]);
    const ownerPath = `account-data-v2/scope-owners/profiles/${PROFILE_A}.json`;
    fixture.storage.onCreate = (path) => {
      if (path === ownerPath) fixture.residency.barriers.add(SUBJECT_A);
    };

    await expect(fixture.manager.ensureProfile(profileLocator)).rejects.toMatchObject({
      code: "purged_subject",
    });
    expect(fixture.storage.has(ownerPath)).toBe(true);
  });

  it("resumes subject-first deletion and preserves every other subject", async () => {
    const locatorA = locator(SUBJECT_A, "profile", PROFILE_A);
    const locatorB = locator(SUBJECT_B, "profile", PROFILE_B);
    const fixture = createFixture([locatorA, locatorB]);
    const profileA = await fixture.manager.ensureProfile(locatorA);
    const profileB = await fixture.manager.ensureProfile(locatorB);
    await profileA.active.writeFileExclusive("Cookies", Buffer.from("subject-a"));
    await profileB.active.writeFileExclusive("Cookies", Buffer.from("subject-b"));

    await expect(fixture.manager.removeSubject(SUBJECT_A)).rejects.toMatchObject({
      code: "purge_barrier_required",
    });
    fixture.residency.barriers.add(SUBJECT_A);
    fixture.storage.removeAt(["account-data-v2", "subjects", SUBJECT_A]);
    const removed = await fixture.manager.removeSubject(SUBJECT_A);

    expect(removed.after.residency).toBe("never_resident");
    expect(fixture.storage.has(
      `account-data-v2/scope-owners/profiles/${PROFILE_A}.json`,
    )).toBe(false);
    expect(fixture.storage.bytes(
      `account-data-v2/subjects/${SUBJECT_B}/profiles/${PROFILE_B}/active/Cookies`,
    )).toEqual(Buffer.from("subject-b"));
    await expect(fixture.manager.resolveProfile(PROFILE_B)).resolves.toMatchObject({
      subjectSha256: SUBJECT_B,
    });
    await expect(fixture.manager.inventorySubject(SUBJECT_B)).resolves.toMatchObject({
      residency: "resident",
    });
  });

  it("requires one stable root inventory around signed-control reads", async () => {
    const profileLocator = locator(SUBJECT_A, "profile", PROFILE_A);
    const fixture = createFixture([profileLocator]);
    await fixture.manager.ensureProfile(profileLocator);
    fixture.storage.mutateAfterNextInventory = () => {
      fixture.storage.seedFile(
        ["account-data-v2", "subjects", SUBJECT_A, "profiles", PROFILE_A, "active", "late"],
        Buffer.from("changed"),
      );
    };

    await expect(fixture.manager.auditSubject(SUBJECT_A)).rejects.toMatchObject({
      code: "audit_changed",
    });
  });

  it("attests one stable closed-world subject set while retaining purged locators", async () => {
    const locatorA = locator(SUBJECT_A, "profile", PROFILE_A);
    const locatorB = locator(SUBJECT_B, "profile", PROFILE_B);
    const fixture = createFixture([locatorA, locatorB]);
    await fixture.manager.ensureProfile(locatorA);
    await fixture.manager.ensureProfile(locatorB);
    fixture.residency.barriers.add(SUBJECT_B);
    await fixture.manager.removeSubject(SUBJECT_B);

    const current = await fixture.manager.inventoryCurrentStorage(
      fixture.locators,
    );
    const locatorSet = runnerVolumeLocatorSetEvidence(
      fixture.locators,
      fixture.identity,
      new Set([SUBJECT_B]),
    );

    expect(current.subjectStorage).toMatchObject({
      layoutVersion: 2,
      subjectCount: 1,
      scopeCount: 1,
      subjects: [
        {
          subjectSha256: SUBJECT_A,
          profileScopeCount: 1,
          resultScopeCount: 0,
        },
      ],
    });
    expect(current.root.deviceId).toBe(DEVICE_ID);
    expect(current.legacy).toMatchObject({
      version: 1,
      legacyArtifactCount: 0,
      unclassifiedRootPaths: [],
    });
    expect(locatorSet).toMatchObject({ count: 2, residentCount: 1 });
  });

  it("refuses a current-storage attestation when its single snapshot changes", async () => {
    const profileLocator = locator(SUBJECT_A, "profile", PROFILE_A);
    const fixture = createFixture([profileLocator]);
    await fixture.manager.ensureProfile(profileLocator);
    fixture.storage.mutateAfterNextInventory = () => {
      fixture.storage.seedFile(
        [
          "account-data-v2",
          "subjects",
          SUBJECT_A,
          "profiles",
          PROFILE_A,
          "active",
          "late",
        ],
        Buffer.from("changed"),
      );
    };

    await expect(
      fixture.manager.inventoryCurrentStorage(fixture.locators),
    ).rejects.toMatchObject({ code: "audit_changed" });
  });

  it("holds one subject lock for purge evidence and invalidates the scoped capability", async () => {
    const profileLocator = locator(SUBJECT_A, "profile", PROFILE_A);
    const fixture = createFixture([profileLocator]);
    const profile = await fixture.manager.ensureProfile(profileLocator);
    await profile.active.writeFileExclusive("Cookies", Buffer.from("subject-a"));
    fixture.residency.barriers.add(SUBJECT_A);
    let escaped: LockedSubjectStorage | undefined;

    const removal = await fixture.manager.withLockedSubject(SUBJECT_A, async (storage) => {
      escaped = storage;
      const before = await storage.inventory();
      expect(before.residency).toBe("resident");
      return storage.remove();
    });

    expect(removal.after.residency).toBe("never_resident");
    await expect(escaped!.inventory()).rejects.toMatchObject({ code: "corrupt_storage" });
  });

  it("waits for an unawaited scoped operation before releasing the subject lock", async () => {
    const fixture = createFixture([locator(SUBJECT_A, "profile", PROFILE_A)]);
    let releaseInventory = (): void => undefined;
    const inventoryGate = new Promise<void>((resolve) => { releaseInventory = resolve; });
    let inventoryEntered = (): void => undefined;
    const entered = new Promise<void>((resolve) => { inventoryEntered = resolve; });
    fixture.storage.beforeNextInventory = async () => {
      inventoryEntered();
      await inventoryGate;
    };

    const scoped = fixture.manager.withLockedSubject(SUBJECT_A, async (storage) => {
      void storage.inventory();
    });
    await entered;
    let competitorEntered = false;
    const competitor = fixture.residency.withSubjectLock(SUBJECT_A, async () => {
      competitorEntered = true;
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(competitorEntered).toBe(false);

    releaseInventory();
    await scoped;
    await competitor;
    expect(competitorEntered).toBe(true);
  });
});

interface Fixture {
  readonly identity: RunnerVolumeIdentity;
  readonly storage: MemoryNativeRoot;
  readonly residency: MemoryResidency;
  readonly locators: readonly AccountResidencyLocator[];
  readonly manager: SubjectStorageManager;
}

function createFixture(locators: readonly AccountResidencyLocator[]): Fixture {
  const identity = fixedIdentity();
  const storage = new MemoryNativeRoot(ROOT);
  const residency = new MemoryResidency(identity, locators);
  const manager = new SubjectStorageManager(storage, identity, residency);
  return { identity, storage, residency, locators, manager };
}

class MemoryResidency implements SubjectStorageResidencyAuthority {
  readonly barriers = new Set<string>();
  private readonly locators = new Map<string, AccountResidencyLocator[]>();
  private readonly tails = new Map<string, Promise<void>>();

  constructor(
    readonly identity: RunnerVolumeIdentity,
    initial: readonly AccountResidencyLocator[],
  ) {
    for (const locator of initial) this.addLocator(locator);
  }

  addLocator(value: AccountResidencyLocator): void {
    const values = this.locators.get(value.subjectSha256) ?? [];
    values.push(value);
    this.locators.set(value.subjectSha256, values);
  }

  async hasPurgeBarrier(subjectSha256: string): Promise<boolean> {
    return this.barriers.has(subjectSha256);
  }

  async locatorsForSubjectHash(
    subjectSha256: string,
  ): Promise<readonly AccountResidencyLocator[]> {
    return [...this.locators.get(subjectSha256) ?? []].sort((left, right) => (
      left.kind.localeCompare(right.kind) || left.scope.localeCompare(right.scope)
    ));
  }

  async withSubjectLock<T>(
    subjectSha256: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    const predecessor = this.tails.get(subjectSha256) ?? Promise.resolve();
    let release = (): void => undefined;
    const current = new Promise<void>((resolve) => { release = resolve; });
    this.tails.set(subjectSha256, current);
    await predecessor;
    try {
      return await operation();
    } finally {
      release();
      if (this.tails.get(subjectSha256) === current) this.tails.delete(subjectSha256);
    }
  }
}

type MemoryNode = MemoryDirectoryNode | MemoryFileNode;

interface MemoryDirectoryNode {
  readonly kind: "directory";
  readonly children: Map<string, MemoryNode>;
}

interface MemoryFileNode {
  readonly kind: "file";
  contents: Buffer;
}

class MemoryNativeRoot implements NativeRunnerStorageRoot {
  readonly deviceId = DEVICE_ID;
  readonly linkCount = 2;
  readonly mutations: string[] = [];
  readonly tree: MemoryDirectoryNode = directoryNode();
  onCreate: ((path: string) => void) | undefined;
  mutateAfterNextInventory: (() => void) | undefined;
  beforeNextInventory: (() => Promise<void>) | undefined;

  constructor(readonly configuredPath: string) {
    this.seedDirectory(["volume-identity"]);
    this.seedDirectory(["account-residency-v1"]);
    this.seedFile([".bluey-runner-storage.lock"], Buffer.alloc(0));
    this.mutations.length = 0;
  }

  assertUnchanged(): void {}

  async ensureDirectory(components: readonly string[]): Promise<NativeRunnerStorageDirectory> {
    let current = this.tree;
    const traversed: string[] = [];
    for (const component of components) {
      traversed.push(component);
      const existing = current.children.get(component);
      if (!existing) {
        const created = directoryNode();
        current.children.set(component, created);
        current = created;
        this.mutations.push(`mkdir:${traversed.join("/")}`);
      } else if (existing.kind === "directory") {
        current = existing;
      } else {
        throw new Error("bluey_runner_storage:unsafe_entry");
      }
    }
    return new MemoryNativeDirectory(this, [...components]);
  }

  async openDirectory(components: readonly string[]): Promise<NativeRunnerStorageDirectory> {
    this.directoryAt(components);
    return new MemoryNativeDirectory(this, [...components]);
  }

  async moveEntryNoReplace(
    sourceComponents: readonly string[],
    destinationComponents: readonly string[],
  ): Promise<"destination_exists" | "moved" | "source_missing"> {
    if (sourceComponents.length === 0 || destinationComponents.length === 0) {
      throw new Error("bluey_runner_storage:path_escape");
    }
    const sourceParent = this.directoryAt(sourceComponents.slice(0, -1));
    const sourceName = sourceComponents.at(-1)!;
    const source = sourceParent.children.get(sourceName);
    if (!source) return "source_missing";
    const destinationParent = this.directoryAt(destinationComponents.slice(0, -1));
    const destinationName = destinationComponents.at(-1)!;
    if (destinationParent.children.has(destinationName)) return "destination_exists";
    destinationParent.children.set(destinationName, source);
    sourceParent.children.delete(sourceName);
    this.mutations.push(
      `move:${sourceComponents.join("/")}->${destinationComponents.join("/")}`,
    );
    return "moved";
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
        throw new Error("file blocks directory seed");
      }
    }
  }

  seedFile(components: readonly string[], contents: Buffer): void {
    if (components.length === 0) throw new Error("cannot seed root file");
    this.seedDirectory(components.slice(0, -1));
    this.directoryAt(components.slice(0, -1)).children.set(
      components.at(-1)!,
      { kind: "file", contents: Buffer.from(contents) },
    );
  }

  removeAt(components: readonly string[]): void {
    if (components.length === 0) throw new Error("cannot remove root");
    this.directoryAt(components.slice(0, -1)).children.delete(components.at(-1)!);
  }

  has(path: string): boolean {
    return this.nodeAt(path.split("/")) !== undefined;
  }

  bytes(path: string): Buffer {
    const node = this.nodeAt(path.split("/"));
    if (!node || node.kind !== "file") throw new Error(`missing file ${path}`);
    return Buffer.from(node.contents);
  }

  directoryAt(components: readonly string[]): MemoryDirectoryNode {
    const node = this.nodeAt(components);
    if (!node || node.kind !== "directory") throw new Error("bluey_runner_storage:io_failure");
    return node;
  }

  nodeAt(components: readonly string[]): MemoryNode | undefined {
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

class MemoryNativeDirectory implements NativeRunnerStorageDirectory {
  readonly deviceId = DEVICE_ID;
  readonly linkCount = 2;

  constructor(
    private readonly storage: MemoryNativeRoot,
    private readonly components: readonly string[],
  ) {}

  get relativePath(): string {
    return this.components.join("/");
  }

  get canonicalPath(): string {
    return join(this.storage.configuredPath, ...this.components);
  }

  async ensureChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.storage.ensureDirectory([...this.components, name]);
  }

  async openChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.storage.openDirectory([...this.components, name]);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    const parent = this.storage.directoryAt(this.components);
    const existing = parent.children.get(name);
    if (existing) {
      if (existing.kind !== "file") throw new Error("bluey_runner_storage:unsafe_entry");
      this.storage.mutations.push(`replay:${[...this.components, name].join("/")}`);
      return false;
    }
    parent.children.set(name, { kind: "file", contents: Buffer.from(contents) });
    const path = [...this.components, name].join("/");
    this.storage.mutations.push(`create:${path}`);
    this.storage.onCreate?.(path);
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    const parent = this.storage.directoryAt(this.components);
    const existing = parent.children.get(name);
    if (existing?.kind === "directory") throw new Error("bluey_runner_storage:unsafe_entry");
    parent.children.set(name, { kind: "file", contents: Buffer.from(contents) });
    this.storage.mutations.push(`replace:${[...this.components, name].join("/")}`);
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    const node = this.storage.directoryAt(this.components).children.get(name);
    if (!node || node.kind !== "file") throw new Error("bluey_runner_storage:io_failure");
    if (node.contents.length > maximumBytes) throw new Error("bluey_runner_storage:inventory_limit");
    return Buffer.from(node.contents);
  }

  async inventory(): Promise<NativeRunnerInventory> {
    const beforeInventory = this.storage.beforeNextInventory;
    if (beforeInventory) {
      this.storage.beforeNextInventory = undefined;
      await beforeInventory();
    }
    const entries: NativeRunnerInventoryEntry[] = [];
    collectInventory(this.storage.directoryAt(this.components), "", entries);
    entries.sort((left, right) => left.relativePath.localeCompare(right.relativePath));
    const bytes = entries.reduce((sum, entry) => sum + entry.sizeBytes, 0);
    const sha256 = inventoryDigest(entries);
    const result = Object.freeze({
      entries: Object.freeze(entries),
      count: entries.length,
      bytes,
      sha256,
    });
    const mutation = this.storage.mutateAfterNextInventory;
    if (mutation) {
      this.storage.mutateAfterNextInventory = undefined;
      mutation();
    }
    return result;
  }

  async removeEntry(name: string): Promise<void> {
    this.storage.directoryAt(this.components).children.delete(name);
    this.storage.mutations.push(`remove:${[...this.components, name].join("/")}`);
  }
}

function collectInventory(
  directory: MemoryDirectoryNode,
  prefix: string,
  entries: NativeRunnerInventoryEntry[],
): void {
  for (const [name, node] of [...directory.children].sort(([left], [right]) => (
    left.localeCompare(right)
  ))) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (node.kind === "directory") {
      entries.push(Object.freeze({
        relativePath: path,
        kind: "directory",
        deviceId: DEVICE_ID,
        linkCount: 2,
        sizeBytes: 0,
        sha256: createHash("sha256").update("directory").digest("hex"),
      }));
      collectInventory(node, path, entries);
    } else {
      entries.push(Object.freeze({
        relativePath: path,
        kind: "file",
        deviceId: DEVICE_ID,
        linkCount: 1,
        sizeBytes: node.contents.length,
        sha256: createHash("sha256").update(node.contents).digest("hex"),
      }));
    }
  }
}

function inventoryDigest(entries: readonly NativeRunnerInventoryEntry[]): string {
  const digest = createHash("sha256");
  for (const entry of entries) {
    digest.update([
      entry.relativePath,
      entry.kind,
      entry.deviceId,
      String(entry.linkCount),
      String(entry.sizeBytes),
      entry.sha256,
      "",
    ].join("\n"));
  }
  return digest.digest("hex");
}

function directoryNode(): MemoryDirectoryNode {
  return { kind: "directory", children: new Map() };
}

function locator(
  subjectSha256: string,
  kind: "profile" | "result",
  scope: string,
): AccountResidencyLocator {
  const identity = fixedIdentity();
  const unsigned = {
    version: 1,
    audience: "bluey-jobs-runner-account-residency" as const,
    volumeId: identity.volumeId,
    volumeKeyFingerprint: identity.publicKeyFingerprint,
    subjectSha256,
    kind,
    scope,
    artifactFamilies: kind === "profile"
      ? Object.freeze(["active", "snapshots", "run-checkpoints", "receipts", "temporary"])
      : Object.freeze(["step-results", "temporary"]),
  };
  return Object.freeze({
    ...unsigned,
    signature: signEd25519(
      identity.privateKey,
      canonicalAccountResidencyLocatorBytes(unsigned),
    ),
  });
}

function fixedIdentity(): RunnerVolumeIdentity {
  const seed = Buffer.from(
    "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    "hex",
  );
  const privateKey: KeyObject = createPrivateKey({
    key: Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), seed]),
    format: "der",
    type: "pkcs8",
  });
  const publicKey = createPublicKey(privateKey).export({ format: "jwk" });
  if (!publicKey.x) throw new Error("missing fixed public key");
  const publicKeyRaw = publicKey.x;
  const publicKeyBytes = Buffer.from(publicKeyRaw, "base64url");
  return {
    volumeId: createHash("sha256")
      .update("bluey-jobs-runner\0volume-id-v1\0", "utf8")
      .update(publicKeyBytes)
      .digest("base64url"),
    publicKeyRaw,
    publicKeyFingerprint: ed25519PublicKeyFingerprint(publicKeyRaw),
    privateKey,
  };
}
