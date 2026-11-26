import { createHash, randomBytes } from "node:crypto";
import { createWriteStream } from "node:fs";
import { chmod, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { pipeline } from "node:stream/promises";
import { gzip, gunzip } from "node:zlib";
import * as tar from "tar";
import {
  decryptBytes,
  decryptFile,
  encryptBytes,
  encryptFile,
  replaceFileDurably,
} from "./crypto-envelope.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerStorageDirectory,
} from "./native-runner-storage.js";
import type { ManagedProfileStorage } from "./subject-storage-manager.js";

export { decryptFile, encryptFile } from "./crypto-envelope.js";

export interface ProfilePaths {
  scope: string;
  directory: string;
  encryptedSnapshot: string;
  snapshotGeneration: string;
}

/**
 * Managed-v2 paths keep the only Chromium-facing pathname on the retained
 * active-directory capability. The remaining strings are canonical evidence;
 * managed operations never reopen them through Node's path APIs.
 */
export interface ManagedProfilePaths extends ProfilePaths {
  readonly storageVersion: "managed-v2";
  readonly subjectSha256: string;
  readonly storage: ManagedProfileStorage;
}

export type ProfileStorePaths = ProfilePaths | ManagedProfilePaths;

export interface EncryptedProfileSnapshot {
  bytes: Buffer;
  generation: number;
  envelopeVersion: 2;
}

export type ManagedProfileStoreErrorCode =
  | "archive_invalid"
  | "binding_changed"
  | "configuration"
  | "profile_too_large"
  | "snapshot_conflict"
  | "snapshot_corrupt"
  | "unsafe_entry";

export class ManagedProfileStoreError extends Error {
  constructor(
    readonly code: ManagedProfileStoreErrorCode,
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = "ManagedProfileStoreError";
  }
}

const PROFILE_SCOPE_PATTERN = /^[a-f0-9]{40}$/;
const SUBJECT_SHA256_PATTERN = /^[a-f0-9]{64}$/;
const ENVELOPE_MAGIC = Buffer.from("BLUEYJP2", "ascii");
const MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES = 25 * 1024 * 1024;
const MAXIMUM_PROFILE_BYTES = 256 * 1024 * 1024;
const MAXIMUM_TAR_BYTES = 288 * 1024 * 1024;
const MAXIMUM_PROFILE_ENTRIES = 20_000;
const MAXIMUM_PROFILE_PATH_BYTES = 4_096;
const MAXIMUM_PROFILE_DEPTH = 64;
const MAXIMUM_GENERATION_RECORD_BYTES = 2 * 1024;
const MANAGED_SNAPSHOT_AUDIENCE = "bluey-jobs-managed-profile-snapshot-v1";
const RESERVED_LOCK_NAME = ".bluey-runner-storage.lock";
const RESERVED_STAGING_NAME = /^\.bluey-stage-[0-9]+-[0-9a-fA-F]{16}$/;

interface DirectoryBinding {
  readonly relativePath: string;
  readonly canonicalPath: string;
  readonly deviceId: string;
}

interface ManagedProfileBinding {
  readonly subjectSha256: string;
  readonly scope: string;
  readonly snapshotName: string;
  readonly generationName: string;
  readonly root: DirectoryBinding;
  readonly active: DirectoryBinding;
  readonly snapshots: DirectoryBinding;
  readonly checkpoints: DirectoryBinding;
  readonly receipts: DirectoryBinding;
  readonly temporary: DirectoryBinding;
}

interface ManagedSnapshotRecord {
  readonly version: 1;
  readonly audience: typeof MANAGED_SNAPSHOT_AUDIENCE;
  readonly subjectSha256: string;
  readonly scope: string;
  readonly generation: number;
  readonly envelopeVersion: 2;
  readonly snapshotSha256: string;
  readonly snapshotBytes: number;
}

interface ArchiveEntry {
  readonly path: string;
  readonly kind: "directory" | "file";
  readonly contents?: Buffer;
}

const managedBindings = new WeakMap<
  ManagedProfilePaths,
  ManagedProfileBinding
>();
const managedOperationTails = new Map<string, Promise<void>>();

export function profilePaths(
  root: string,
  accountId: string,
  applicationIdentityId: string,
): ProfilePaths {
  const scope = createHash("sha256")
    .update(`${accountId}\0${applicationIdentityId}`)
    .digest("hex")
    .slice(0, 40);
  return profilePathsFromScope(root, scope);
}

export function profilePathsFromScope(
  root: string,
  scope: string,
): ProfilePaths {
  if (!/^[a-f0-9]{40}$/.test(scope))
    throw new Error("Invalid browser profile scope");
  return {
    scope,
    directory: join(root, "active", scope),
    encryptedSnapshot: join(root, "snapshots", `${scope}.tar.gz.enc`),
    snapshotGeneration: join(root, "snapshots", `${scope}.generation`),
  };
}

export function managedProfilePaths(
  storage: ManagedProfileStorage,
): ManagedProfilePaths {
  const binding = captureManagedBinding(storage);
  const paths: ManagedProfilePaths = Object.freeze({
    storageVersion: "managed-v2",
    subjectSha256: binding.subjectSha256,
    scope: binding.scope,
    directory: binding.active.canonicalPath,
    encryptedSnapshot: storage.paths.encryptedSnapshot.relativePath,
    snapshotGeneration: storage.paths.snapshotGeneration.relativePath,
    storage,
  });
  managedBindings.set(paths, binding);
  return paths;
}

export function parseProfileKey(encoded: string): Buffer {
  const key = Buffer.from(encoded, "base64");
  if (key.length !== 32)
    throw new Error(
      "BLUEY_JOBS_PROFILE_ENCRYPTION_KEY must be a base64 32-byte key",
    );
  return key;
}

export async function restoreProfile(
  paths: ProfileStorePaths,
  key: Buffer,
): Promise<void> {
  if (isManagedProfilePaths(paths)) return restoreManagedProfile(paths, key);
  return restoreLegacyProfile(paths, key);
}

async function restoreLegacyProfile(
  paths: ProfilePaths,
  key: Buffer,
): Promise<void> {
  await rm(paths.directory, { recursive: true, force: true });
  await mkdir(paths.directory, { recursive: true, mode: 0o700 });
  await chmod(paths.directory, 0o700);
  try {
    await stat(paths.encryptedSnapshot);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
    await rm(paths.directory, { recursive: true, force: true });
    throw error;
  }
  const archive = `${paths.directory}.restore.tar.gz`;
  try {
    await decryptFile(
      paths.encryptedSnapshot,
      archive,
      key,
      profileEncryptionContext(paths),
    );
    await tar.x({
      cwd: paths.directory,
      file: archive,
      gzip: true,
      preservePaths: false,
    });
  } catch (error) {
    await rm(paths.directory, { recursive: true, force: true });
    throw error;
  } finally {
    await rm(archive, { force: true });
  }
}

export async function sealProfile(
  paths: ProfileStorePaths,
  key: Buffer,
): Promise<void> {
  if (isManagedProfilePaths(paths)) return sealManagedProfile(paths, key);
  return sealLegacyProfile(paths, key);
}

async function sealLegacyProfile(
  paths: ProfilePaths,
  key: Buffer,
): Promise<void> {
  const archive = `${paths.directory}.seal.tar.gz`;
  await mkdir(dirname(paths.encryptedSnapshot), {
    recursive: true,
    mode: 0o700,
  });
  await chmod(dirname(paths.encryptedSnapshot), 0o700);
  await rm(archive, { force: true });
  try {
    await pipeline(
      tar.c({ cwd: paths.directory, gzip: true, portable: true }, ["."]),
      createWriteStream(archive, { flags: "wx", mode: 0o600 }),
    );
    await encryptFile(
      archive,
      paths.encryptedSnapshot,
      key,
      profileEncryptionContext(paths),
    );
  } finally {
    await rm(archive, { force: true });
  }
  await rm(paths.directory, { recursive: true, force: true });
}

export async function readEncryptedProfileSnapshot(
  paths: ProfileStorePaths,
): Promise<EncryptedProfileSnapshot | undefined> {
  if (isManagedProfilePaths(paths))
    return readManagedEncryptedProfileSnapshot(paths);
  return readLegacyEncryptedProfileSnapshot(paths);
}

async function readLegacyEncryptedProfileSnapshot(
  paths: ProfilePaths,
): Promise<EncryptedProfileSnapshot | undefined> {
  let bytes: Buffer;
  try {
    bytes = await readFile(paths.encryptedSnapshot);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
  if (
    bytes.length < 8 ||
    bytes.subarray(0, 8).toString("ascii") !== "BLUEYJP2"
  ) {
    throw new Error("Invalid encrypted browser profile snapshot");
  }
  return {
    bytes,
    generation: await readProfileSnapshotGeneration(paths),
    envelopeVersion: 2,
  };
}

export async function installEncryptedProfileSnapshot(
  paths: ProfileStorePaths,
  snapshot: EncryptedProfileSnapshot,
): Promise<void> {
  if (isManagedProfilePaths(paths)) {
    return installManagedEncryptedProfileSnapshot(paths, snapshot);
  }
  return installLegacyEncryptedProfileSnapshot(paths, snapshot);
}

async function installLegacyEncryptedProfileSnapshot(
  paths: ProfilePaths,
  snapshot: EncryptedProfileSnapshot,
): Promise<void> {
  if (!Number.isSafeInteger(snapshot.generation) || snapshot.generation <= 0) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  if (
    snapshot.envelopeVersion !== 2 ||
    snapshot.bytes.length < 8 ||
    snapshot.bytes.subarray(0, 8).toString("ascii") !== "BLUEYJP2"
  ) {
    throw new Error("Invalid encrypted browser profile snapshot");
  }
  const existing = await readEncryptedProfileSnapshot(paths);
  if (existing && existing.generation > snapshot.generation) {
    throw new Error("Refusing to replace a newer browser profile snapshot");
  }
  if (existing && existing.generation === snapshot.generation) {
    if (!existing.bytes.equals(snapshot.bytes)) {
      throw new Error("Browser profile snapshot generation conflict");
    }
    return;
  }

  const snapshotsDirectory = dirname(paths.encryptedSnapshot);
  await mkdir(snapshotsDirectory, { recursive: true, mode: 0o700 });
  await chmod(snapshotsDirectory, 0o700);
  const suffix = `${process.pid}-${randomBytes(8).toString("hex")}`;
  const snapshotStaging = `${paths.encryptedSnapshot}.${suffix}.incoming`;
  const generationStaging = `${paths.snapshotGeneration}.${suffix}.incoming`;
  try {
    await writeFile(snapshotStaging, snapshot.bytes, {
      flag: "wx",
      mode: 0o600,
    });
    await writeFile(generationStaging, `${snapshot.generation}\n`, {
      flag: "wx",
      mode: 0o600,
    });
    await replaceFileDurably(snapshotStaging, paths.encryptedSnapshot);
    await replaceFileDurably(generationStaging, paths.snapshotGeneration);
  } finally {
    await rm(snapshotStaging, { force: true });
    await rm(generationStaging, { force: true });
  }
}

export async function writeProfileSnapshotGeneration(
  paths: ProfileStorePaths,
  generation: number,
): Promise<void> {
  if (isManagedProfilePaths(paths)) {
    return writeManagedProfileSnapshotGeneration(paths, generation);
  }
  return writeLegacyProfileSnapshotGeneration(paths, generation);
}

async function writeLegacyProfileSnapshotGeneration(
  paths: ProfilePaths,
  generation: number,
): Promise<void> {
  if (!Number.isSafeInteger(generation) || generation <= 0) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  const snapshotsDirectory = dirname(paths.snapshotGeneration);
  await mkdir(snapshotsDirectory, { recursive: true, mode: 0o700 });
  await chmod(snapshotsDirectory, 0o700);
  const staging = `${paths.snapshotGeneration}.${process.pid}-${randomBytes(8).toString("hex")}.incoming`;
  try {
    await writeFile(staging, `${generation}\n`, { flag: "wx", mode: 0o600 });
    await replaceFileDurably(staging, paths.snapshotGeneration);
  } finally {
    await rm(staging, { force: true });
  }
}

export async function restoreManagedProfile(
  paths: ManagedProfilePaths,
  key: Buffer,
): Promise<void> {
  return withManagedProfileOperation(paths, async (binding) => {
    await assertManagedBinding(paths, binding);
    const snapshot = await readManagedSnapshotLocked(paths, binding);
    let archive: Buffer | undefined;
    let entries: readonly ArchiveEntry[] = [];
    try {
      if (snapshot) {
        assertEncryptionKey(key);
        archive = decryptBytes(
          snapshot.bytes,
          key,
          profileEncryptionContext(paths),
        );
        if (archive.length > MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES) {
          throw managedError(
            "profile_too_large",
            "Browser profile archive exceeds its bound.",
          );
        }
        entries = await parseProfileArchive(archive);
      }
      await clearActiveProfile(paths.storage.active, binding);
      try {
        await restoreArchiveEntries(paths.storage.active, binding, entries);
        await assertRestoredInventory(paths.storage.active, binding, entries);
      } catch (error) {
        try {
          await clearActiveProfile(paths.storage.active, binding);
        } catch (cleanupError) {
          throw managedError(
            "binding_changed",
            "Browser profile restore failed and retained cleanup could not be proven.",
            new AggregateError([error, cleanupError]),
          );
        }
        throw error;
      }
      await assertManagedBinding(paths, binding);
    } finally {
      archive?.fill(0);
      eraseArchiveEntries(entries);
      snapshot?.bytes.fill(0);
    }
  });
}

export async function sealManagedProfile(
  paths: ManagedProfilePaths,
  key: Buffer,
): Promise<void> {
  return withManagedProfileOperation(paths, async (binding) => {
    assertEncryptionKey(key);
    await assertManagedBinding(paths, binding);
    const existing = await readManagedSnapshotLocked(paths, binding);
    const before = await validatedProfileInventory(
      paths.storage.active,
      binding,
    );
    let tarBytes: Buffer | undefined;
    let compressed: Buffer | undefined;
    let encrypted: Buffer | undefined;
    try {
      tarBytes = await createProfileTar(paths.storage.active, binding, before);
      const afterRead = await validatedProfileInventory(
        paths.storage.active,
        binding,
      );
      if (!inventoriesEqual(before, afterRead)) {
        throw managedError(
          "binding_changed",
          "Browser profile changed while its retained snapshot was being created.",
        );
      }
      compressed = await gzipBounded(tarBytes);
      if (compressed.length > MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES) {
        throw managedError(
          "profile_too_large",
          "Browser profile archive exceeds its bound.",
        );
      }
      encrypted = encryptBytes(
        compressed,
        key,
        profileEncryptionContext(paths),
      );
      assertEncryptedSnapshotBytes(encrypted);
      await publishManagedSnapshotPair(
        paths,
        binding,
        encrypted,
        existing?.generation ?? 0,
      );
      const beforeClear = await validatedProfileInventory(
        paths.storage.active,
        binding,
      );
      if (!inventoriesEqual(before, beforeClear)) {
        throw managedError(
          "binding_changed",
          "Browser profile changed before its sealed active copy could be removed.",
        );
      }
      await clearActiveProfile(paths.storage.active, binding, beforeClear);
      await assertManagedBinding(paths, binding);
    } finally {
      tarBytes?.fill(0);
      compressed?.fill(0);
      encrypted?.fill(0);
      existing?.bytes.fill(0);
    }
  });
}

export async function readManagedEncryptedProfileSnapshot(
  paths: ManagedProfilePaths,
): Promise<EncryptedProfileSnapshot | undefined> {
  return withManagedProfileOperation(paths, async (binding) => {
    await assertManagedBinding(paths, binding);
    const snapshot = await readManagedSnapshotLocked(paths, binding);
    await assertManagedBinding(paths, binding);
    return snapshot;
  });
}

export async function installManagedEncryptedProfileSnapshot(
  paths: ManagedProfilePaths,
  snapshot: EncryptedProfileSnapshot,
): Promise<void> {
  return withManagedProfileOperation(paths, async (binding) => {
    assertRemoteSnapshot(snapshot);
    await assertManagedBinding(paths, binding);
    const existing = await readManagedSnapshotLocked(paths, binding);
    try {
      if (existing && existing.generation > snapshot.generation) {
        throw managedError(
          "snapshot_conflict",
          "Refusing to replace a newer browser profile snapshot.",
        );
      }
      if (existing && existing.generation === snapshot.generation) {
        if (!existing.bytes.equals(snapshot.bytes)) {
          throw managedError(
            "snapshot_conflict",
            "Browser profile snapshot generation conflict.",
          );
        }
        return;
      }
      await publishManagedSnapshotPair(
        paths,
        binding,
        snapshot.bytes,
        snapshot.generation,
      );
      await assertManagedBinding(paths, binding);
    } finally {
      existing?.bytes.fill(0);
    }
  });
}

export async function writeManagedProfileSnapshotGeneration(
  paths: ManagedProfilePaths,
  generation: number,
): Promise<void> {
  return withManagedProfileOperation(paths, async (binding) => {
    if (!Number.isSafeInteger(generation) || generation <= 0) {
      throw managedError(
        "snapshot_corrupt",
        "Invalid browser profile snapshot generation.",
      );
    }
    await assertManagedBinding(paths, binding);
    const existing = await readManagedSnapshotLocked(paths, binding);
    if (!existing) {
      throw managedError(
        "snapshot_corrupt",
        "Encrypted browser profile snapshot is missing.",
      );
    }
    try {
      if (
        generation < existing.generation ||
        generation > existing.generation + 1
      ) {
        throw managedError(
          "snapshot_conflict",
          "Browser profile snapshot generation conflict.",
        );
      }
      if (generation === existing.generation) return;
      const record = managedSnapshotRecord(paths, generation, existing.bytes);
      await paths.storage.snapshots.replaceFile(
        binding.generationName,
        encodeManagedSnapshotRecord(record),
      );
      const persisted = await readManagedSnapshotLocked(paths, binding);
      try {
        if (
          !persisted ||
          persisted.generation !== generation ||
          !persisted.bytes.equals(existing.bytes)
        ) {
          throw managedError(
            "snapshot_corrupt",
            "Browser profile snapshot publication failed.",
          );
        }
      } finally {
        persisted?.bytes.fill(0);
      }
      await assertManagedBinding(paths, binding);
    } finally {
      existing.bytes.fill(0);
    }
  });
}

async function publishManagedSnapshotPair(
  paths: ManagedProfilePaths,
  binding: ManagedProfileBinding,
  encrypted: Buffer,
  generation: number,
): Promise<void> {
  assertLocalSnapshotGeneration(generation);
  assertEncryptedSnapshotBytes(encrypted);
  const record = managedSnapshotRecord(paths, generation, encrypted);
  await paths.storage.snapshots.replaceFile(binding.snapshotName, encrypted);
  await paths.storage.snapshots.replaceFile(
    binding.generationName,
    encodeManagedSnapshotRecord(record),
  );
  const persisted = await readManagedSnapshotLocked(paths, binding);
  try {
    if (
      !persisted ||
      persisted.generation !== generation ||
      !persisted.bytes.equals(encrypted)
    ) {
      throw managedError(
        "snapshot_corrupt",
        "Browser profile snapshot publication failed.",
      );
    }
  } finally {
    persisted?.bytes.fill(0);
  }
}

async function readManagedSnapshotLocked(
  paths: ManagedProfilePaths,
  binding: ManagedProfileBinding,
): Promise<EncryptedProfileSnapshot | undefined> {
  const before = await validatedSnapshotInventory(
    paths.storage.snapshots,
    binding,
  );
  const snapshotEntry = before.entries.find(
    (entry) => entry.relativePath === binding.snapshotName,
  );
  const generationEntry = before.entries.find(
    (entry) => entry.relativePath === binding.generationName,
  );
  if (!snapshotEntry && !generationEntry) return undefined;
  if (
    !snapshotEntry ||
    !generationEntry ||
    snapshotEntry.kind !== "file" ||
    generationEntry.kind !== "file"
  ) {
    throw managedError(
      "snapshot_corrupt",
      "Browser profile snapshot publication is incomplete.",
    );
  }
  const [bytes, encodedRecord] = await Promise.all([
    paths.storage.snapshots.readFileBounded(
      binding.snapshotName,
      MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES,
    ),
    paths.storage.snapshots.readFileBounded(
      binding.generationName,
      MAXIMUM_GENERATION_RECORD_BYTES,
    ),
  ]);
  try {
    assertEncryptedSnapshotBytes(bytes);
    const record = parseManagedSnapshotRecord(
      encodedRecord,
      paths.subjectSha256,
      paths.scope,
    );
    if (
      record.snapshotBytes !== bytes.length ||
      record.snapshotSha256 !== sha256(bytes) ||
      snapshotEntry.sizeBytes !== bytes.length ||
      snapshotEntry.sha256 !== record.snapshotSha256 ||
      generationEntry.sizeBytes !== encodedRecord.length ||
      generationEntry.sha256 !== sha256(encodedRecord)
    ) {
      throw managedError(
        "snapshot_corrupt",
        "Browser profile snapshot binding is invalid.",
      );
    }
    const after = await validatedSnapshotInventory(
      paths.storage.snapshots,
      binding,
    );
    if (!inventoriesEqual(before, after)) {
      throw managedError(
        "binding_changed",
        "Browser profile snapshot changed while being read.",
      );
    }
    return {
      bytes: Buffer.from(bytes),
      generation: record.generation,
      envelopeVersion: 2,
    };
  } finally {
    bytes.fill(0);
    encodedRecord.fill(0);
  }
}

function captureManagedBinding(
  storage: ManagedProfileStorage,
): ManagedProfileBinding {
  if (
    !storage ||
    storage.kind !== "profile" ||
    !SUBJECT_SHA256_PATTERN.test(storage.subjectSha256) ||
    !PROFILE_SCOPE_PATTERN.test(storage.scope)
  ) {
    throw managedError(
      "binding_changed",
      "Invalid managed browser profile capability.",
    );
  }
  const snapshotName = fileNameForPath(
    storage.paths.encryptedSnapshot.components,
    storage.paths.snapshots.components,
  );
  const generationName = fileNameForPath(
    storage.paths.snapshotGeneration.components,
    storage.paths.snapshots.components,
  );
  if (
    snapshotName !== "profile.tar.gz.enc" ||
    generationName !== "generation"
  ) {
    throw managedError(
      "binding_changed",
      "Invalid managed browser profile layout.",
    );
  }
  const binding: ManagedProfileBinding = Object.freeze({
    subjectSha256: storage.subjectSha256,
    scope: storage.scope,
    snapshotName,
    generationName,
    root: captureDirectoryBinding(
      storage.root,
      storage.paths.root.relativePath,
    ),
    active: captureDirectoryBinding(
      storage.active,
      storage.paths.active.relativePath,
    ),
    snapshots: captureDirectoryBinding(
      storage.snapshots,
      storage.paths.snapshots.relativePath,
    ),
    checkpoints: captureDirectoryBinding(
      storage.checkpoints,
      storage.paths.checkpoints.relativePath,
    ),
    receipts: captureDirectoryBinding(
      storage.receipts,
      storage.paths.receipts.relativePath,
    ),
    temporary: captureDirectoryBinding(
      storage.temporary,
      storage.paths.temporary.relativePath,
    ),
  });
  const devices = new Set([
    binding.root.deviceId,
    binding.active.deviceId,
    binding.snapshots.deviceId,
    binding.checkpoints.deviceId,
    binding.receipts.deviceId,
    binding.temporary.deviceId,
  ]);
  if (devices.size !== 1) {
    throw managedError(
      "binding_changed",
      "Managed browser profile crossed a storage boundary.",
    );
  }
  return binding;
}

function captureDirectoryBinding(
  directory: NativeRunnerStorageDirectory,
  expectedRelativePath: string,
): DirectoryBinding {
  if (
    !directory ||
    directory.relativePath !== expectedRelativePath ||
    !directory.canonicalPath ||
    !directory.deviceId ||
    !Number.isSafeInteger(directory.linkCount) ||
    directory.linkCount < 1
  ) {
    throw managedError(
      "binding_changed",
      "Invalid retained browser profile directory.",
    );
  }
  return Object.freeze({
    relativePath: directory.relativePath,
    canonicalPath: directory.canonicalPath,
    deviceId: directory.deviceId,
  });
}

async function assertManagedBinding(
  paths: ManagedProfilePaths,
  binding: ManagedProfileBinding,
): Promise<void> {
  if (
    managedBindings.get(paths) !== binding ||
    paths.storageVersion !== "managed-v2" ||
    paths.subjectSha256 !== binding.subjectSha256 ||
    paths.scope !== binding.scope ||
    paths.directory !== binding.active.canonicalPath ||
    paths.encryptedSnapshot !==
      paths.storage.paths.encryptedSnapshot.relativePath ||
    paths.snapshotGeneration !==
      paths.storage.paths.snapshotGeneration.relativePath ||
    paths.storage.subjectSha256 !== binding.subjectSha256 ||
    paths.storage.scope !== binding.scope
  ) {
    throw managedError(
      "binding_changed",
      "Managed browser profile binding changed.",
    );
  }
  assertDirectoryBinding(paths.storage.root, binding.root);
  assertDirectoryBinding(paths.storage.active, binding.active);
  assertDirectoryBinding(paths.storage.snapshots, binding.snapshots);
  assertDirectoryBinding(paths.storage.checkpoints, binding.checkpoints);
  assertDirectoryBinding(paths.storage.receipts, binding.receipts);
  assertDirectoryBinding(paths.storage.temporary, binding.temporary);
  await Promise.all([
    paths.storage.active
      .inventory()
      .then((value) => validateInventoryDevice(value, binding)),
    paths.storage.snapshots
      .inventory()
      .then((value) => validateInventoryDevice(value, binding)),
  ]);
}

function assertDirectoryBinding(
  directory: NativeRunnerStorageDirectory,
  expected: DirectoryBinding,
): void {
  if (
    directory.relativePath !== expected.relativePath ||
    directory.canonicalPath !== expected.canonicalPath ||
    directory.deviceId !== expected.deviceId ||
    !Number.isSafeInteger(directory.linkCount) ||
    directory.linkCount < 1
  ) {
    throw managedError(
      "binding_changed",
      "Retained browser profile directory changed.",
    );
  }
}

function isManagedProfilePaths(
  paths: ProfileStorePaths,
): paths is ManagedProfilePaths {
  if (managedBindings.has(paths as ManagedProfilePaths)) return true;
  if ("storageVersion" in paths) {
    throw managedError(
      "binding_changed",
      "Unrecognized managed browser profile paths.",
    );
  }
  return false;
}

async function withManagedProfileOperation<T>(
  paths: ManagedProfilePaths,
  operation: (binding: ManagedProfileBinding) => Promise<T>,
): Promise<T> {
  const binding = managedBindings.get(paths);
  if (!binding) {
    throw managedError(
      "binding_changed",
      "Unrecognized managed browser profile paths.",
    );
  }
  const key = `${binding.active.deviceId}\0${binding.active.relativePath}`;
  const predecessor = managedOperationTails.get(key) ?? Promise.resolve();
  let release = (): void => undefined;
  const turn = new Promise<void>((resolve) => {
    release = resolve;
  });
  const tail = predecessor.catch(() => undefined).then(() => turn);
  managedOperationTails.set(key, tail);
  await predecessor.catch(() => undefined);
  try {
    return await operation(binding);
  } finally {
    release();
    if (managedOperationTails.get(key) === tail)
      managedOperationTails.delete(key);
  }
}

async function validatedProfileInventory(
  directory: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
): Promise<NativeRunnerInventory> {
  assertDirectoryBinding(directory, binding.active);
  const inventory = await directory.inventory();
  validateInventoryDevice(inventory, binding);
  validateInventoryBounds(inventory);
  for (const entry of inventory.entries) validateProfileInventoryEntry(entry);
  return inventory;
}

async function validatedSnapshotInventory(
  directory: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
): Promise<NativeRunnerInventory> {
  assertDirectoryBinding(directory, binding.snapshots);
  const inventory = await directory.inventory();
  validateInventoryDevice(inventory, binding);
  if (
    inventory.count > 2 ||
    inventory.bytes >
      MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES + MAXIMUM_GENERATION_RECORD_BYTES
  ) {
    throw managedError(
      "snapshot_corrupt",
      "Browser profile snapshot namespace is invalid.",
    );
  }
  const allowed = new Set([binding.snapshotName, binding.generationName]);
  for (const entry of inventory.entries) {
    if (
      entry.kind !== "file" ||
      entry.linkCount !== 1 ||
      !allowed.has(entry.relativePath)
    ) {
      throw managedError(
        "snapshot_corrupt",
        "Browser profile snapshot namespace is invalid.",
      );
    }
  }
  return inventory;
}

function validateInventoryDevice(
  inventory: NativeRunnerInventory,
  binding: ManagedProfileBinding,
): void {
  if (
    !inventory ||
    !Number.isSafeInteger(inventory.count) ||
    !Number.isSafeInteger(inventory.bytes) ||
    inventory.count !== inventory.entries.length
  ) {
    throw managedError(
      "binding_changed",
      "Invalid retained browser profile inventory.",
    );
  }
  for (const entry of inventory.entries) {
    if (entry.deviceId !== binding.active.deviceId) {
      throw managedError(
        "binding_changed",
        "Browser profile inventory crossed a storage boundary.",
      );
    }
  }
}

function validateInventoryBounds(inventory: NativeRunnerInventory): void {
  if (
    inventory.count > MAXIMUM_PROFILE_ENTRIES ||
    inventory.bytes > MAXIMUM_PROFILE_BYTES
  ) {
    throw managedError(
      "profile_too_large",
      "Browser profile exceeds its retained storage bound.",
    );
  }
}

function validateProfileInventoryEntry(
  entry: NativeRunnerInventoryEntry,
): void {
  const components = safeRelativeComponents(entry.relativePath);
  if (entry.kind === "file") {
    if (
      entry.linkCount !== 1 ||
      !Number.isSafeInteger(entry.sizeBytes) ||
      entry.sizeBytes < 0
    ) {
      throw managedError(
        "unsafe_entry",
        "Browser profile contains an unsafe file entry.",
      );
    }
  } else if (entry.sizeBytes !== 0 || entry.linkCount < 1) {
    throw managedError(
      "unsafe_entry",
      "Browser profile contains an unsafe directory entry.",
    );
  }
  for (const component of components) assertSafeProfileComponent(component);
}

function inventoriesEqual(
  left: NativeRunnerInventory,
  right: NativeRunnerInventory,
): boolean {
  if (
    left.count !== right.count ||
    left.bytes !== right.bytes ||
    left.sha256 !== right.sha256 ||
    left.entries.length !== right.entries.length
  )
    return false;
  return left.entries.every((entry, index) => {
    const other = right.entries[index];
    return (
      other !== undefined &&
      entry.relativePath === other.relativePath &&
      entry.kind === other.kind &&
      entry.deviceId === other.deviceId &&
      entry.linkCount === other.linkCount &&
      entry.sizeBytes === other.sizeBytes &&
      entry.sha256 === other.sha256
    );
  });
}

async function createProfileTar(
  active: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
  inventory: NativeRunnerInventory,
): Promise<Buffer> {
  const chunks: Buffer[] = [];
  const sensitiveBuffers: Buffer[] = [];
  let totalBytes = 0;
  try {
    for (const entry of inventory.entries) {
      validateProfileInventoryEntry(entry);
      let contents: Buffer | undefined;
      if (entry.kind === "file") {
        const components = safeRelativeComponents(entry.relativePath);
        const parent = await openRelativeDirectory(
          active,
          binding,
          components.slice(0, -1),
        );
        contents = await parent.readFileBounded(
          components.at(-1)!,
          Math.max(1, entry.sizeBytes),
        );
        sensitiveBuffers.push(contents);
        if (
          contents.length !== entry.sizeBytes ||
          sha256(contents) !== entry.sha256
        ) {
          contents.fill(0);
          throw managedError(
            "binding_changed",
            "Browser profile file changed while its retained snapshot was being created.",
          );
        }
      }
      const headerData = {
        path: entry.relativePath,
        mode: entry.kind === "directory" ? 0o700 : 0o600,
        uid: 0,
        gid: 0,
        size: contents?.length ?? 0,
        type:
          entry.kind === "directory"
            ? ("Directory" as const)
            : ("File" as const),
        uname: "",
        gname: "",
      };
      const header = new tar.Header(headerData);
      const headerBlock = Buffer.alloc(512);
      const needsPax = header.encode(headerBlock);
      if (needsPax) {
        const pax = new tar.Pax(headerData).encode();
        totalBytes = checkedArchiveSize(totalBytes, pax.length);
        chunks.push(pax);
      }
      totalBytes = checkedArchiveSize(totalBytes, headerBlock.length);
      chunks.push(headerBlock);
      if (contents) {
        totalBytes = checkedArchiveSize(totalBytes, contents.length);
        chunks.push(contents);
        const paddingBytes = (512 - (contents.length % 512)) % 512;
        if (paddingBytes > 0) {
          totalBytes = checkedArchiveSize(totalBytes, paddingBytes);
          chunks.push(Buffer.alloc(paddingBytes));
        }
      }
    }
    totalBytes = checkedArchiveSize(totalBytes, 1024);
    chunks.push(Buffer.alloc(1024));
    return Buffer.concat(chunks, totalBytes);
  } finally {
    for (const chunk of chunks) chunk.fill(0);
    for (const contents of sensitiveBuffers) contents.fill(0);
  }
}

function checkedArchiveSize(current: number, added: number): number {
  const next = current + added;
  if (!Number.isSafeInteger(next) || next > MAXIMUM_TAR_BYTES) {
    throw managedError(
      "profile_too_large",
      "Browser profile archive exceeds its bound.",
    );
  }
  return next;
}

async function parseProfileArchive(
  compressed: Buffer,
): Promise<readonly ArchiveEntry[]> {
  let unpacked: Buffer | undefined;
  try {
    unpacked = await gunzipBounded(compressed);
    return await parseTarEntries(unpacked);
  } catch (error) {
    if (error instanceof ManagedProfileStoreError) throw error;
    throw managedError(
      "archive_invalid",
      "Browser profile archive is invalid.",
      error,
    );
  } finally {
    unpacked?.fill(0);
  }
}

async function parseTarEntries(
  unpacked: Buffer,
): Promise<readonly ArchiveEntry[]> {
  const entries: ArchiveEntry[] = [];
  const seen = new Set<string>();
  let totalBytes = 0;
  return new Promise<readonly ArchiveEntry[]>((resolve, reject) => {
    let settled = false;
    const entryPromises: Promise<void>[] = [];
    const parser = new tar.Parser({
      strict: true,
      noResume: true,
      maxMetaEntrySize: MAXIMUM_PROFILE_PATH_BYTES * 4,
      maxDepth: MAXIMUM_PROFILE_DEPTH,
      onReadEntry(entry) {
        try {
          const normalized = normalizedArchivePath(entry.path, entry.type);
          if (normalized === null) {
            entry.resume();
            return;
          }
          if (seen.size >= MAXIMUM_PROFILE_ENTRIES || seen.has(normalized)) {
            throw managedError(
              "archive_invalid",
              "Browser profile archive has duplicate entries.",
            );
          }
          seen.add(normalized);
          if (entry.type === "Directory") {
            if (entry.size !== 0) {
              throw managedError(
                "archive_invalid",
                "Browser profile archive directory is invalid.",
              );
            }
            entries.push(
              Object.freeze({ path: normalized, kind: "directory" }),
            );
            entry.resume();
            return;
          }
          if (
            entry.type !== "File" ||
            !Number.isSafeInteger(entry.size) ||
            entry.size < 0 ||
            entry.size > MAXIMUM_PROFILE_BYTES - totalBytes
          ) {
            throw managedError(
              "archive_invalid",
              "Browser profile archive entry is invalid.",
            );
          }
          totalBytes += entry.size;
          const promise = new Promise<void>((entryResolve, entryReject) => {
            const chunks: Buffer[] = [];
            let received = 0;
            let entryFailed = false;
            entry.on("data", (chunk: Buffer) => {
              received += chunk.length;
              if (received > entry.size || received > MAXIMUM_PROFILE_BYTES) {
                if (!entryFailed) {
                  entryFailed = true;
                  for (const buffered of chunks) buffered.fill(0);
                  chunks.length = 0;
                  entryReject(
                    managedError(
                      "profile_too_large",
                      "Browser profile archive exceeds its bound.",
                    ),
                  );
                }
                return;
              }
              chunks.push(Buffer.from(chunk));
            });
            entry.once("error", (error) => {
              entryFailed = true;
              for (const buffered of chunks) buffered.fill(0);
              chunks.length = 0;
              entryReject(error);
            });
            entry.once("end", () => {
              if (entryFailed || received !== entry.size) {
                for (const chunk of chunks) chunk.fill(0);
                if (!entryFailed) {
                  entryReject(
                    managedError(
                      "archive_invalid",
                      "Browser profile archive entry is truncated.",
                    ),
                  );
                }
                return;
              }
              const contents = Buffer.concat(chunks, received);
              for (const chunk of chunks) chunk.fill(0);
              if (settled) {
                contents.fill(0);
                entryResolve();
                return;
              }
              entries.push(
                Object.freeze({ path: normalized, kind: "file", contents }),
              );
              entryResolve();
            });
          });
          entryPromises.push(promise);
          entry.resume();
        } catch (error) {
          entry.resume();
          parser.abort(
            error instanceof Error
              ? error
              : new Error("Invalid profile archive"),
          );
        }
      },
    });
    const fail = (error: unknown): void => {
      if (settled) return;
      settled = true;
      void Promise.allSettled(entryPromises);
      eraseArchiveEntries(entries);
      reject(
        error instanceof ManagedProfileStoreError
          ? error
          : managedError(
              "archive_invalid",
              "Browser profile archive is invalid.",
              error,
            ),
      );
    };
    parser.once("error", fail);
    parser.once("end", () => {
      void Promise.all(entryPromises).then(() => {
        if (settled) return;
        try {
          validateArchiveStructure(entries);
          settled = true;
          resolve(Object.freeze([...entries]));
        } catch (error) {
          fail(error);
        }
      }, fail);
    });
    try {
      parser.end(unpacked);
    } catch (error) {
      fail(error);
    }
  });
}

function validateArchiveStructure(entries: readonly ArchiveEntry[]): void {
  const kinds = new Map(entries.map((entry) => [entry.path, entry.kind]));
  for (const entry of entries) {
    const components = safeRelativeComponents(entry.path);
    for (let index = 1; index < components.length; index += 1) {
      const parent = components.slice(0, index).join("/");
      if (kinds.get(parent) === "file") {
        throw managedError(
          "archive_invalid",
          "Browser profile archive has a file parent.",
        );
      }
    }
  }
}

function normalizedArchivePath(
  value: string,
  type: tar.ReadEntry["type"],
): string | null {
  if (type !== "Directory" && type !== "File") {
    throw managedError(
      "unsafe_entry",
      "Browser profile archive has an unsupported entry type.",
    );
  }
  let path = value;
  while (path.startsWith("./")) path = path.slice(2);
  if (type === "Directory") path = path.replace(/\/+$/, "");
  if (path === "" || path === ".")
    return type === "Directory" ? null : invalidArchivePath();
  safeRelativeComponents(path);
  return path;
}

function invalidArchivePath(): never {
  throw managedError(
    "unsafe_entry",
    "Browser profile archive has an unsafe path.",
  );
}

async function restoreArchiveEntries(
  active: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
  entries: readonly ArchiveEntry[],
): Promise<void> {
  const directoryPaths = new Set<string>([""]);
  for (const entry of entries) {
    const components = safeRelativeComponents(entry.path);
    const limit =
      entry.kind === "directory" ? components.length : components.length - 1;
    for (let index = 1; index <= limit; index += 1) {
      directoryPaths.add(components.slice(0, index).join("/"));
    }
  }
  const directories = new Map<string, NativeRunnerStorageDirectory>([
    ["", active],
  ]);
  const orderedDirectories = [...directoryPaths]
    .filter(Boolean)
    .sort(comparePathDepthThenUtf8);
  for (const path of orderedDirectories) {
    const components = safeRelativeComponents(path);
    const parentPath = components.slice(0, -1).join("/");
    const parent = directories.get(parentPath);
    if (!parent) {
      throw managedError(
        "archive_invalid",
        "Browser profile archive directory graph is invalid.",
      );
    }
    const directory = await parent.ensureChildDirectory(components.at(-1)!);
    assertOpenedRelativeDirectory(active, directory, binding, components);
    directories.set(path, directory);
  }
  const files = entries
    .filter(
      (entry): entry is ArchiveEntry & { readonly contents: Buffer } =>
        entry.kind === "file" && entry.contents !== undefined,
    )
    .sort((left, right) => compareUtf8(left.path, right.path));
  for (const entry of files) {
    const components = safeRelativeComponents(entry.path);
    const parent = directories.get(components.slice(0, -1).join("/"));
    if (!parent) {
      throw managedError(
        "archive_invalid",
        "Browser profile archive file parent is invalid.",
      );
    }
    if (
      !(await parent.writeFileExclusive(components.at(-1)!, entry.contents))
    ) {
      throw managedError(
        "binding_changed",
        "Browser profile restore encountered an existing file.",
      );
    }
    const persisted = await parent.readFileBounded(
      components.at(-1)!,
      Math.max(1, entry.contents.length),
    );
    try {
      if (!persisted.equals(entry.contents)) {
        throw managedError(
          "binding_changed",
          "Browser profile restore read-back failed.",
        );
      }
    } finally {
      persisted.fill(0);
    }
  }
}

async function assertRestoredInventory(
  active: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
  entries: readonly ArchiveEntry[],
): Promise<void> {
  const inventory = await validatedProfileInventory(active, binding);
  const expected = new Map<
    string,
    { kind: "directory" | "file"; size: number; sha256?: string }
  >();
  for (const entry of entries) {
    const components = safeRelativeComponents(entry.path);
    const limit =
      entry.kind === "directory" ? components.length : components.length - 1;
    for (let index = 1; index <= limit; index += 1) {
      const path = components.slice(0, index).join("/");
      const prior = expected.get(path);
      if (prior?.kind === "file") {
        throw managedError(
          "archive_invalid",
          "Browser profile archive has a file parent.",
        );
      }
      expected.set(path, { kind: "directory", size: 0 });
    }
    if (entry.kind === "file") {
      expected.set(entry.path, {
        kind: "file",
        size: entry.contents!.length,
        sha256: sha256(entry.contents!),
      });
    }
  }
  if (inventory.count !== expected.size) {
    throw managedError(
      "binding_changed",
      "Browser profile restore inventory is incomplete.",
    );
  }
  for (const entry of inventory.entries) {
    const target = expected.get(entry.relativePath);
    if (
      !target ||
      target.kind !== entry.kind ||
      target.size !== entry.sizeBytes ||
      (entry.kind === "file" && target.sha256 !== entry.sha256)
    ) {
      throw managedError(
        "binding_changed",
        "Browser profile restore inventory does not match.",
      );
    }
  }
}

async function clearActiveProfile(
  active: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
  knownInventory?: NativeRunnerInventory,
): Promise<void> {
  const inventory =
    knownInventory ?? (await validatedProfileInventory(active, binding));
  const topLevel = new Set<string>();
  for (const entry of inventory.entries) {
    topLevel.add(safeRelativeComponents(entry.relativePath)[0]!);
  }
  for (const name of [...topLevel].sort(compareUtf8))
    await active.removeEntry(name);
  const after = await validatedProfileInventory(active, binding);
  if (after.count !== 0 || after.bytes !== 0 || after.entries.length !== 0) {
    throw managedError(
      "binding_changed",
      "Browser profile active directory did not become empty.",
    );
  }
}

async function openRelativeDirectory(
  root: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
  components: readonly string[],
): Promise<NativeRunnerStorageDirectory> {
  let current = root;
  const traversed: string[] = [];
  for (const component of components) {
    assertSafeProfileComponent(component);
    traversed.push(component);
    current = await current.openChildDirectory(component);
    assertOpenedRelativeDirectory(root, current, binding, traversed);
  }
  return current;
}

function assertOpenedRelativeDirectory(
  root: NativeRunnerStorageDirectory,
  directory: NativeRunnerStorageDirectory,
  binding: ManagedProfileBinding,
  components: readonly string[],
): void {
  const suffix = components.join("/");
  const expectedRelative = suffix
    ? `${root.relativePath}/${suffix}`
    : root.relativePath;
  const expectedCanonical = suffix
    ? `${root.canonicalPath}/${suffix}`
    : root.canonicalPath;
  if (
    directory.relativePath !== expectedRelative ||
    directory.canonicalPath !== expectedCanonical ||
    directory.deviceId !== binding.active.deviceId ||
    !Number.isSafeInteger(directory.linkCount) ||
    directory.linkCount < 1
  ) {
    throw managedError(
      "binding_changed",
      "Retained browser profile child directory changed.",
    );
  }
}

function safeRelativeComponents(value: string): readonly string[] {
  if (
    !value ||
    value.startsWith("/") ||
    value.endsWith("/") ||
    value.includes("\\") ||
    value.includes("\0") ||
    Buffer.byteLength(value, "utf8") > MAXIMUM_PROFILE_PATH_BYTES
  ) {
    return invalidArchivePath();
  }
  const components = value.split("/");
  if (components.length > MAXIMUM_PROFILE_DEPTH) return invalidArchivePath();
  for (const component of components) assertSafeProfileComponent(component);
  return components;
}

function assertSafeProfileComponent(component: string): void {
  if (
    !component ||
    component === "." ||
    component === ".." ||
    component === RESERVED_LOCK_NAME ||
    RESERVED_STAGING_NAME.test(component) ||
    Buffer.byteLength(component, "utf8") > 255 ||
    /[\\/\u0000-\u001f\u007f]/.test(component)
  ) {
    invalidArchivePath();
  }
}

function fileNameForPath(
  components: readonly string[],
  expectedParent: readonly string[],
): string {
  if (
    components.length !== expectedParent.length + 1 ||
    expectedParent.some((component, index) => components[index] !== component)
  ) {
    throw managedError(
      "binding_changed",
      "Invalid managed browser profile file path.",
    );
  }
  const name = components.at(-1)!;
  assertSafeProfileComponent(name);
  return name;
}

function comparePathDepthThenUtf8(left: string, right: string): number {
  const depth = left.split("/").length - right.split("/").length;
  return depth || compareUtf8(left, right);
}

function compareUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function eraseArchiveEntries(entries: readonly ArchiveEntry[]): void {
  for (const entry of entries) entry.contents?.fill(0);
}

function managedSnapshotRecord(
  paths: ManagedProfilePaths,
  generation: number,
  bytes: Buffer,
): ManagedSnapshotRecord {
  assertLocalSnapshotGeneration(generation);
  return Object.freeze({
    version: 1,
    audience: MANAGED_SNAPSHOT_AUDIENCE,
    subjectSha256: paths.subjectSha256,
    scope: paths.scope,
    generation,
    envelopeVersion: 2,
    snapshotSha256: sha256(bytes),
    snapshotBytes: bytes.length,
  });
}

function encodeManagedSnapshotRecord(record: ManagedSnapshotRecord): Buffer {
  return Buffer.from(`${JSON.stringify(record)}\n`, "utf8");
}

function parseManagedSnapshotRecord(
  encoded: Buffer,
  expectedSubjectSha256: string,
  expectedScope: string,
): ManagedSnapshotRecord {
  let value: unknown;
  try {
    value = JSON.parse(encoded.toString("utf8"));
  } catch (error) {
    throw managedError(
      "snapshot_corrupt",
      "Browser profile snapshot generation is invalid.",
      error,
    );
  }
  if (
    !isObject(value) ||
    !hasExactKeys(value, [
      "audience",
      "envelopeVersion",
      "generation",
      "scope",
      "snapshotBytes",
      "snapshotSha256",
      "subjectSha256",
      "version",
    ]) ||
    value.version !== 1 ||
    value.audience !== MANAGED_SNAPSHOT_AUDIENCE ||
    value.subjectSha256 !== expectedSubjectSha256 ||
    value.scope !== expectedScope ||
    value.envelopeVersion !== 2 ||
    typeof value.generation !== "number" ||
    typeof value.snapshotBytes !== "number" ||
    typeof value.snapshotSha256 !== "string" ||
    !/^[a-f0-9]{64}$/.test(value.snapshotSha256)
  ) {
    throw managedError(
      "snapshot_corrupt",
      "Browser profile snapshot generation is invalid.",
    );
  }
  assertLocalSnapshotGeneration(value.generation);
  if (
    !Number.isSafeInteger(value.snapshotBytes) ||
    value.snapshotBytes < ENVELOPE_MAGIC.length ||
    value.snapshotBytes > MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES
  ) {
    throw managedError(
      "snapshot_corrupt",
      "Browser profile snapshot generation is invalid.",
    );
  }
  const record = value as unknown as ManagedSnapshotRecord;
  if (!encoded.equals(encodeManagedSnapshotRecord(record))) {
    throw managedError(
      "snapshot_corrupt",
      "Browser profile snapshot generation is not canonical.",
    );
  }
  return record;
}

function assertRemoteSnapshot(snapshot: EncryptedProfileSnapshot): void {
  if (
    !snapshot ||
    !Number.isSafeInteger(snapshot.generation) ||
    snapshot.generation <= 0 ||
    snapshot.envelopeVersion !== 2
  ) {
    throw managedError(
      "snapshot_corrupt",
      "Invalid browser profile snapshot generation.",
    );
  }
  assertEncryptedSnapshotBytes(snapshot.bytes);
}

function assertLocalSnapshotGeneration(generation: number): void {
  if (!Number.isSafeInteger(generation) || generation < 0) {
    throw managedError(
      "snapshot_corrupt",
      "Invalid browser profile snapshot generation.",
    );
  }
}

function assertEncryptedSnapshotBytes(bytes: Buffer): void {
  if (
    !Buffer.isBuffer(bytes) ||
    bytes.length < ENVELOPE_MAGIC.length + 12 + 16 ||
    bytes.length > MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES ||
    !bytes.subarray(0, ENVELOPE_MAGIC.length).equals(ENVELOPE_MAGIC)
  ) {
    throw managedError(
      "snapshot_corrupt",
      "Invalid encrypted browser profile snapshot.",
    );
  }
}

function assertEncryptionKey(key: Buffer): void {
  if (!Buffer.isBuffer(key) || key.length !== 32) {
    throw managedError(
      "configuration",
      "Browser profile encryption key is invalid.",
    );
  }
}

function sha256(value: Buffer): string {
  return createHash("sha256").update(value).digest("hex");
}

function gzipBounded(contents: Buffer): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    gzip(contents, { level: 6 }, (error, compressed) => {
      if (error) {
        reject(
          managedError(
            "archive_invalid",
            "Browser profile archive compression failed.",
            error,
          ),
        );
        return;
      }
      if (compressed.length > MAXIMUM_ENCRYPTED_SNAPSHOT_BYTES) {
        compressed.fill(0);
        reject(
          managedError(
            "profile_too_large",
            "Browser profile archive exceeds its bound.",
          ),
        );
        return;
      }
      resolve(compressed);
    });
  });
}

function gunzipBounded(contents: Buffer): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    gunzip(
      contents,
      { maxOutputLength: MAXIMUM_TAR_BYTES },
      (error, unpacked) => {
        if (error) {
          reject(
            managedError(
              "archive_invalid",
              "Browser profile archive is invalid.",
              error,
            ),
          );
          return;
        }
        if (unpacked.length > MAXIMUM_TAR_BYTES) {
          unpacked.fill(0);
          reject(
            managedError(
              "profile_too_large",
              "Browser profile archive exceeds its bound.",
            ),
          );
          return;
        }
        resolve(unpacked);
      },
    );
  });
}

function managedError(
  code: ManagedProfileStoreErrorCode,
  message: string,
  cause?: unknown,
): ManagedProfileStoreError {
  return new ManagedProfileStoreError(
    code,
    message,
    cause === undefined ? undefined : { cause },
  );
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return (
    actual.length === wanted.length &&
    actual.every((key, index) => key === wanted[index])
  );
}

async function readProfileSnapshotGeneration(
  paths: ProfilePaths,
): Promise<number> {
  let value: string;
  try {
    value = (await readFile(paths.snapshotGeneration, "utf8")).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return 0;
    throw error;
  }
  if (!/^[0-9]{1,16}$/.test(value)) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  const generation = Number(value);
  if (!Number.isSafeInteger(generation) || generation <= 0) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  return generation;
}

function profileEncryptionContext(paths: ProfilePaths) {
  return { purpose: "profile-snapshot", scope: paths.scope } as const;
}
