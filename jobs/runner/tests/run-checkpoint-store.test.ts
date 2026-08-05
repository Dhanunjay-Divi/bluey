import { createHash, randomBytes } from "node:crypto";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  approvedExecutionChecksum,
  restartDisposition,
  type ApplicationPacket,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import { profilePaths, restoreProfile } from "../src/profile-store.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerStorageDirectory,
} from "../src/native-runner-storage.js";
import {
  CURRENT_CHECKPOINT_VERSION,
  cloudCheckpointScope,
  listManagedRunCheckpoints,
  listRunCheckpoints,
  readManagedRunCheckpoint,
  readRunCheckpoint,
  reconcileOrphanActiveProfiles,
  removeManagedRunCheckpoint,
  removeRunCheckpoint,
  scanManagedRunCheckpoints,
  scanRunCheckpoints,
  writeManagedRunCheckpoint,
  writeRunCheckpoint,
  type CloudRunCheckpoint,
} from "../src/run-checkpoint-store.js";
import type { ManagedProfileStorage } from "../src/subject-storage-manager.js";
import { subjectStoragePaths } from "../src/subject-storage-layout.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("encrypted cloud run checkpoints", () => {
  it("rehydrates a needs-input checkpoint after a hard store restart", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const checkpoint = fixture();
    const scope = cloudCheckpointScope(checkpoint.profileScope, checkpoint.browserSessionId);
    await writeRunCheckpoint(root, checkpoint, key);

    const path = checkpointPath(root, checkpoint.profileScope, scope);
    const encrypted = await readFile(path);
    expect(encrypted.subarray(0, 8).toString("ascii")).toBe("BLUEYJP2");
    expect(encrypted.includes(Buffer.from("person@example.test"))).toBe(false);
    expect(encrypted.includes(Buffer.from("private answer"))).toBe(false);
    expect(encrypted.includes(Buffer.from("lease-secret-value"))).toBe(false);
    if (process.platform !== "win32") {
      expect((await stat(path)).mode & 0o777).toBe(0o600);
      expect((await stat(dirname(path))).mode & 0o777).toBe(0o700);
    }
    expect((await readdir(dirname(path))).every((name) => name.endsWith(".json.enc"))).toBe(true);

    await expect(readRunCheckpoint(root, checkpoint.profileScope, scope, key))
      .resolves.toEqual(checkpoint);
    await expect(listRunCheckpoints(root, key)).resolves.toEqual([{ checkpointScope: scope, checkpoint }]);
  });

  it("authenticates both profile and browser-session scopes", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const source = fixture();
    const target = fixture({ browserSessionId: "cloud-application-other" });
    const sourceScope = cloudCheckpointScope(source.profileScope, source.browserSessionId);
    const targetScope = cloudCheckpointScope(target.profileScope, target.browserSessionId);
    await writeRunCheckpoint(root, source, key);
    const targetPath = checkpointPath(root, target.profileScope, targetScope);
    await mkdir(dirname(targetPath), { recursive: true });
    await copyFile(checkpointPath(root, source.profileScope, sourceScope), targetPath);

    await expect(readRunCheckpoint(root, target.profileScope, targetScope, key))
      .rejects.toMatchObject({ code: "authentication_failed" });
  });

  it("removes a checkpoint without exposing its raw session ID in the path", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const checkpoint = fixture();
    const scope = cloudCheckpointScope(checkpoint.profileScope, checkpoint.browserSessionId);
    const path = checkpointPath(root, checkpoint.profileScope, scope);
    expect(path).not.toContain(checkpoint.browserSessionId);
    await writeRunCheckpoint(root, checkpoint, key);
    await removeRunCheckpoint(root, checkpoint.profileScope, checkpoint.browserSessionId);
    await expect(readFile(path)).rejects.toThrow();
  });

  it("rehydrates before the final-submit checkpoint and never after it", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const safe = {
      ...fixture(),
      phase: "prepared" as const,
      workflow: { ...fixture().workflow, status: "prepared" as const },
    };
    const scope = cloudCheckpointScope(safe.profileScope, safe.browserSessionId);
    await writeRunCheckpoint(root, safe, key);
    const before = await readRunCheckpoint(root, safe.profileScope, scope, key);
    expect(restartDisposition(
      before!.phase,
      before!.expiresAtMs,
      Date.parse("2026-07-16T12:02:00.000Z"),
    )).toBe("restore");

    await writeRunCheckpoint(root, {
      ...safe,
      phase: "final_submit_started",
      updatedAtMs: Date.parse("2026-07-16T12:03:00.000Z"),
      workflow: { ...safe.workflow, status: "side_effect_unknown" },
    }, key);
    const after = await readRunCheckpoint(root, safe.profileScope, scope, key);
    expect(restartDisposition(
      after!.phase,
      after!.expiresAtMs,
      Date.parse("2026-07-16T12:04:00.000Z"),
    )).toBe("side_effect_unknown");
  });

  it("rejects a checkpoint whose approved answers changed", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const checkpoint = fixture();
    const packet = checkpoint.request.packet as ApplicationPacket;
    packet.answers.private_question = "changed after approval";

    await expect(writeRunCheckpoint(root, checkpoint, key))
      .rejects.toThrow("changed after review");
  });

  it("keeps legacy v1 checkpoints readable without a persisted lease token", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const checkpoint = fixture({ version: 1 });
    const scope = cloudCheckpointScope(checkpoint.profileScope, checkpoint.browserSessionId);

    await writeRunCheckpoint(root, checkpoint, key);

    await expect(readRunCheckpoint(root, checkpoint.profileScope, scope, key))
      .resolves.toEqual(checkpoint);
    expect(checkpoint.lease.leaseToken).toBeUndefined();
  });

  it("rejects a v2 checkpoint that cannot prove its original lease capability", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const checkpoint = fixture();
    delete checkpoint.lease.leaseToken;

    await expect(writeRunCheckpoint(root, checkpoint, key))
      .rejects.toThrow("Invalid cloud run checkpoint lease metadata");
  });

  it("isolates a corrupt checkpoint while retaining it and reading another profile", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const corrupt = fixture({
      profileScope: "a".repeat(40),
      browserSessionId: "cloud-application-corrupt",
    });
    const healthy = fixture({
      profileScope: "b".repeat(40),
      browserSessionId: "cloud-application-healthy",
    });
    await writeRunCheckpoint(root, corrupt, key);
    await writeRunCheckpoint(root, healthy, key);
    const corruptScope = cloudCheckpointScope(corrupt.profileScope, corrupt.browserSessionId);
    const corruptPath = checkpointPath(root, corrupt.profileScope, corruptScope);
    const corruptedBytes = await readFile(corruptPath);
    corruptedBytes[20] ^= 0x01;
    await writeFile(corruptPath, corruptedBytes, { mode: 0o600 });

    const scan = await scanRunCheckpoints<FixtureRequest, unknown>(root, key);

    expect(scan.checkpoints).toHaveLength(1);
    expect(scan.checkpoints[0]?.checkpoint.profileScope).toBe(healthy.profileScope);
    expect(scan.failures).toEqual([{
      profileScope: corrupt.profileScope,
      checkpointScope: corruptScope,
      code: "checkpoint_unreadable",
    }]);
    await expect(readFile(corruptPath)).resolves.toEqual(corruptedBytes);
  });

  it("treats a non-directory profile checkpoint scope as isolated corruption", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const corruptProfileScope = "a".repeat(40);
    const healthy = fixture({
      profileScope: "b".repeat(40),
      browserSessionId: "cloud-application-healthy",
    });
    await writeRunCheckpoint(root, healthy, key);
    const invalidProfilePath = join(root, "run-checkpoints", corruptProfileScope);
    await writeFile(invalidProfilePath, "retained-corrupt-profile", { mode: 0o600 });

    const scan = await scanRunCheckpoints<FixtureRequest, unknown>(root, key);

    expect(scan.checkpoints).toHaveLength(1);
    expect(scan.checkpoints[0]?.checkpoint.profileScope).toBe(healthy.profileScope);
    expect(scan.failures).toEqual([{
      profileScope: corruptProfileScope,
      code: "profile_unreadable",
    }]);
    await expect(readFile(invalidProfilePath, "utf8")).resolves.toBe("retained-corrupt-profile");
  });
});

describe("managed v2 cloud run checkpoints", () => {
  it("atomically replaces and authenticates a checkpoint through retained handles", async () => {
    const key = randomBytes(32);
    const checkpoint = fixture();
    const managed = managedProfileStorage(checkpoint.profileScope);
    const checkpointScope = cloudCheckpointScope(
      checkpoint.profileScope,
      checkpoint.browserSessionId,
    );

    await writeManagedRunCheckpoint(managed.capability, checkpoint, key);

    const fileName = `${checkpointScope}.json.enc`;
    const encrypted = managed.checkpoints.bytes(fileName);
    expect(managed.checkpoints.names()).toEqual([fileName]);
    expect(managed.checkpoints.operations[0]).toBe(`replace:${fileName}`);
    expect(encrypted.subarray(0, 8).toString("ascii")).toBe("BLUEYJP2");
    expect(encrypted.includes(Buffer.from("person@example.test"))).toBe(false);
    expect(encrypted.includes(Buffer.from("private answer"))).toBe(false);
    expect(encrypted.includes(Buffer.from("lease-secret-value"))).toBe(false);
    await expect(readManagedRunCheckpoint<FixtureRequest, unknown>(
      managed.capability,
      checkpointScope,
      key,
    )).resolves.toEqual(checkpoint);
    await expect(listManagedRunCheckpoints<FixtureRequest, unknown>(
      managed.capability,
      key,
    )).resolves.toEqual([{ checkpointScope, checkpoint }]);
  });

  it("binds the capability, ciphertext, and file name to the exact profile and session", async () => {
    const key = randomBytes(32);
    const source = fixture();
    const sourceStorage = managedProfileStorage(source.profileScope);
    const wrongProfile = managedProfileStorage("b".repeat(40));
    await expect(writeManagedRunCheckpoint(wrongProfile.capability, source, key))
      .rejects.toThrow("profile binding");

    await writeManagedRunCheckpoint(sourceStorage.capability, source, key);
    const sourceScope = cloudCheckpointScope(source.profileScope, source.browserSessionId);
    const target = fixture({ browserSessionId: "cloud-application-other" });
    const targetScope = cloudCheckpointScope(target.profileScope, target.browserSessionId);
    sourceStorage.checkpoints.seedFile(
      `${targetScope}.json.enc`,
      sourceStorage.checkpoints.bytes(`${sourceScope}.json.enc`),
    );

    await expect(readManagedRunCheckpoint(
      sourceStorage.capability,
      targetScope,
      key,
    )).rejects.toMatchObject({ code: "authentication_failed" });
  });

  it("rejects a string checkpoint version instead of bypassing the v2 lease-token rule", async () => {
    const key = randomBytes(32);
    const checkpoint = fixture();
    Reflect.set(checkpoint, "version", "2");
    delete checkpoint.lease.leaseToken;

    await expect(writeManagedRunCheckpoint(
      managedProfileStorage(checkpoint.profileScope).capability,
      checkpoint,
      key,
    )).rejects.toThrow("Invalid cloud run checkpoint envelope");
  });

  it("fails closed on unknown, nested, hardlinked, and oversized scan entries", async () => {
    const key = randomBytes(32);
    const scenarios: Array<(directory: MemoryNativeDirectory) => void> = [
      (directory) => directory.seedFile(".DS_Store", Buffer.from("metadata")),
      (directory) => directory.seedDirectory("nested"),
      (directory) => directory.seedFile(`${"1".repeat(64)}.json.enc`, Buffer.from("linked"), 2),
      (directory) => directory.seedFile(
        `${"2".repeat(64)}.json.enc`,
        Buffer.alloc(5 * 1024 * 1024 + 65),
      ),
    ];

    for (const seed of scenarios) {
      const managed = managedProfileStorage("a".repeat(40));
      seed(managed.checkpoints);
      const scan = await scanManagedRunCheckpoints<FixtureRequest, unknown>(
        managed.capability,
        key,
      );
      expect(scan).toEqual({
        checkpoints: [],
        failures: [{ profileScope: managed.capability.scope, code: "profile_unreadable" }],
      });
      expect(managed.checkpoints.readCount).toBe(0);
    }
  });

  it("enforces the per-profile bound before reading any checkpoint", async () => {
    const key = randomBytes(32);
    const managed = managedProfileStorage("a".repeat(40));
    for (let index = 0; index < 9; index += 1) {
      managed.checkpoints.seedFile(
        `${index.toString(16).padStart(64, "0")}.json.enc`,
        Buffer.from("not-read"),
      );
    }

    await expect(scanManagedRunCheckpoints(managed.capability, key)).resolves.toEqual({
      checkpoints: [],
      failures: [{
        profileScope: managed.capability.scope,
        code: "checkpoint_limit_exceeded",
      }],
    });
    expect(managed.checkpoints.readCount).toBe(0);
  });

  it("discards scan results when the closed-world inventory changes mid-read", async () => {
    const key = randomBytes(32);
    const checkpoint = fixture();
    const managed = managedProfileStorage(checkpoint.profileScope);
    await writeManagedRunCheckpoint(managed.capability, checkpoint, key);
    managed.checkpoints.mutateAfterNextInventory = () => {
      managed.checkpoints.seedFile("late-entry", Buffer.from("changed"));
    };

    await expect(scanManagedRunCheckpoints(managed.capability, key)).resolves.toEqual({
      checkpoints: [],
      failures: [{ profileScope: checkpoint.profileScope, code: "profile_unreadable" }],
    });
  });

  it("requires exact native readback and verifies removal", async () => {
    const key = randomBytes(32);
    const checkpoint = fixture();
    const failed = managedProfileStorage(checkpoint.profileScope);
    failed.checkpoints.mutateAfterReplace = (fileName) => {
      failed.checkpoints.seedFile(fileName, Buffer.from("replaced-after-publication"));
    };
    await expect(writeManagedRunCheckpoint(failed.capability, checkpoint, key))
      .rejects.toThrow("publication failed");

    const managed = managedProfileStorage(checkpoint.profileScope);
    await writeManagedRunCheckpoint(managed.capability, checkpoint, key);
    await removeManagedRunCheckpoint(
      managed.capability,
      checkpoint.browserSessionId,
    );
    expect(managed.checkpoints.names()).toEqual([]);
  });
});

describe("cloud runner crash-start profile reconciliation", () => {
  it("seals valid orphan plaintext and removes every active entry before traffic", async () => {
    const root = await temporaryDirectory();
    const key = randomBytes(32);
    const paths = profilePaths(root, "account-123", "identity-123");
    await mkdir(paths.directory, { recursive: true });
    await writeFile(join(paths.directory, "Cookies"), "session-cookie", { mode: 0o600 });
    await writeFile(join(root, "active", "stale.restore.tar.gz"), "plaintext", { mode: 0o600 });

    await expect(reconcileOrphanActiveProfiles(root, key)).resolves.toEqual({ sealed: 1, removed: 1 });
    await expect(readdir(join(root, "active"))).resolves.toEqual([]);
    const snapshot = await readFile(paths.encryptedSnapshot);
    expect(snapshot.includes(Buffer.from("session-cookie"))).toBe(false);

    await restoreProfile(paths, key);
    expect(await readFile(join(paths.directory, "Cookies"), "utf8")).toBe("session-cookie");
  });
});

interface FixtureRequest {
  accountId: string;
  applicationIdentityId: string;
  browserProfileId: string;
  browserSessionId: string;
  runId: string;
  applicationId: string;
  url: string;
  packet: ApplicationPacket;
  job: NormalizedJob;
}

function fixture(overrides: Record<string, unknown> = {}): CloudRunCheckpoint<FixtureRequest> {
  const profileScope = String(overrides.profileScope || "a".repeat(40));
  const browserSessionId = String(overrides.browserSessionId || "cloud-application-123");
  const version = overrides.version === 1 ? 1 : CURRENT_CHECKPOINT_VERSION;
  const job: NormalizedJob = {
    externalId: "job-123",
    canonicalUrl: "https://jobs.example.test/apply",
    company: "Example",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Build useful things.",
    source: "greenhouse",
  };
  const packet: ApplicationPacket = {
    applicationId: "application-123",
    jobId: "job-123",
    resumeVersionId: "resume-123",
    approvedPacketChecksum: "",
    applicationEmail: "person@example.test",
    answers: { private_question: "private answer" },
    verifiedClaimIds: [],
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return {
    version,
    phase: "needs_input",
    createdAtMs: Date.parse("2026-07-16T12:00:00.000Z"),
    updatedAtMs: Date.parse("2026-07-16T12:01:00.000Z"),
    expiresAtMs: Date.parse("2026-07-17T12:00:00.000Z"),
    profileScope,
    browserSessionId,
    request: {
      accountId: "account-123",
      applicationIdentityId: "identity-123",
      browserProfileId: "profile:123",
      browserSessionId,
      runId: "run-123",
      applicationId: "application-123",
      url: "https://jobs.example.test/apply",
      packet,
      job,
    },
    browser: { url: "https://jobs.example.test/apply/review" },
    workflow: {
      status: "needs_input",
      requestId: "run-123:initial",
    },
    events: [{ id: "run-123:1", type: "needs_input" }],
    lease: {
      fence: 7,
      expiresAtMs: Date.parse("2026-07-16T12:05:00.000Z"),
      ownerId: "runner-test-1",
      ...(version === CURRENT_CHECKPOINT_VERSION
        ? { leaseToken: "lease-secret-value" }
        : {}),
    },
  };
}

function checkpointPath(root: string, profileScope: string, checkpointScope: string): string {
  return join(root, "run-checkpoints", profileScope, `${checkpointScope}.json.enc`);
}

interface ManagedProfileFixture {
  readonly capability: ManagedProfileStorage;
  readonly checkpoints: MemoryNativeDirectory;
}

function managedProfileStorage(profileScope: string): ManagedProfileFixture {
  const subjectSha256 = "1".repeat(64);
  const paths = subjectStoragePaths(subjectSha256).profile(profileScope);
  const root = new MemoryNativeDirectory(paths.root.relativePath);
  const active = new MemoryNativeDirectory(paths.active.relativePath);
  const snapshots = new MemoryNativeDirectory(paths.snapshots.relativePath);
  const checkpoints = new MemoryNativeDirectory(paths.checkpoints.relativePath);
  const receipts = new MemoryNativeDirectory(paths.receipts.relativePath);
  const temporary = new MemoryNativeDirectory(paths.temporary.relativePath);
  return {
    checkpoints,
    capability: {
      kind: "profile",
      subjectSha256,
      scope: profileScope,
      paths,
      root,
      active,
      snapshots,
      checkpoints,
      receipts,
      temporary,
    },
  };
}

interface MemoryNativeEntry {
  readonly kind: "directory" | "file";
  readonly contents: Buffer;
  readonly linkCount: number;
  readonly deviceId: string;
}

class MemoryNativeDirectory implements NativeRunnerStorageDirectory {
  readonly deviceId = "unix:602:mount:7";
  readonly linkCount = 2;
  readonly canonicalPath: string;
  readonly operations: string[] = [];
  readCount = 0;
  mutateAfterNextInventory: (() => void) | undefined;
  mutateAfterReplace: ((fileName: string) => void) | undefined;
  private readonly entries = new Map<string, MemoryNativeEntry>();

  constructor(readonly relativePath: string) {
    this.canonicalPath = `/srv/bluey-runner/${relativePath}`;
  }

  names(): string[] {
    return [...this.entries.keys()].sort((left, right) => (
      Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"))
    ));
  }

  bytes(name: string): Buffer {
    const entry = this.entries.get(name);
    if (!entry || entry.kind !== "file") throw new Error("missing memory checkpoint");
    return Buffer.from(entry.contents);
  }

  seedFile(name: string, contents: Buffer, linkCount = 1): void {
    this.entries.set(name, {
      kind: "file",
      contents: Buffer.from(contents),
      linkCount,
      deviceId: this.deviceId,
    });
  }

  seedDirectory(name: string): void {
    this.entries.set(name, {
      kind: "directory",
      contents: Buffer.alloc(0),
      linkCount: 2,
      deviceId: this.deviceId,
    });
  }

  async ensureChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return new MemoryNativeDirectory(`${this.relativePath}/${name}`);
  }

  async openChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return new MemoryNativeDirectory(`${this.relativePath}/${name}`);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    if (this.entries.has(name)) return false;
    this.seedFile(name, contents);
    this.operations.push(`create:${name}`);
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    this.seedFile(name, contents);
    this.operations.push(`replace:${name}`);
    this.mutateAfterReplace?.(name);
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    this.readCount += 1;
    const entry = this.entries.get(name);
    if (!entry || entry.kind !== "file") throw new Error("missing memory checkpoint");
    if (entry.contents.length > maximumBytes) throw new Error("bounded read exceeded");
    this.operations.push(`read:${name}:${maximumBytes}`);
    return Buffer.from(entry.contents);
  }

  async inventory(): Promise<NativeRunnerInventory> {
    const entries = this.names().map((name): NativeRunnerInventoryEntry => {
      const entry = this.entries.get(name)!;
      return Object.freeze({
        relativePath: name,
        kind: entry.kind,
        deviceId: entry.deviceId,
        linkCount: entry.linkCount,
        sizeBytes: entry.contents.length,
        sha256: createHash("sha256")
          .update(entry.kind === "file" ? entry.contents : Buffer.from("directory"))
          .digest("hex"),
      });
    });
    const bytes = entries.reduce((total, entry) => total + entry.sizeBytes, 0);
    const digest = createHash("sha256");
    for (const entry of entries) {
      digest.update(JSON.stringify(entry));
      digest.update("\n");
    }
    const inventory = Object.freeze({
      entries: Object.freeze(entries),
      count: entries.length,
      bytes,
      sha256: digest.digest("hex"),
    });
    const mutate = this.mutateAfterNextInventory;
    this.mutateAfterNextInventory = undefined;
    mutate?.();
    return inventory;
  }

  async removeEntry(name: string): Promise<void> {
    this.entries.delete(name);
    this.operations.push(`remove:${name}`);
  }
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-cloud-checkpoint-"));
  temporaryDirectories.push(path);
  return path;
}
