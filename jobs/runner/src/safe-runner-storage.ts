import { createHash, randomBytes } from "node:crypto";
import { constants, type Stats } from "node:fs";
import {
  chmod,
  link,
  lstat,
  mkdir,
  open,
  readdir,
  realpath,
  rename,
  rmdir,
  unlink,
} from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

const PRIVATE_DIRECTORY_MODE = 0o700;
const PRIVATE_FILE_MODE = 0o600;
const DEFAULT_MAX_INVENTORY_ENTRIES = 100_000;
const DEFAULT_MAX_INVENTORY_BYTES = 4 * 1024 * 1024 * 1024;
const DEFAULT_MAX_FILE_BYTES = 512 * 1024 * 1024;
const DEFAULT_MAX_INVENTORY_DEPTH = 128;
const DIRECTORY_DIGEST = createHash("sha256")
  .update("bluey-jobs-runner\0inventory-directory-v1", "utf8")
  .digest("hex");

export type RunnerStorageErrorCode =
  | "configuration"
  | "inventory_limit"
  | "path_escape"
  | "root_changed"
  | "unsafe_entry"
  | "unsafe_permissions";

export class RunnerStorageError extends Error {
  constructor(readonly code: RunnerStorageErrorCode) {
    super({
      configuration: "Runner storage configuration is invalid.",
      inventory_limit: "Runner storage inventory exceeded its bounded limits.",
      path_escape: "Runner storage path escaped the configured data root.",
      root_changed: "Runner storage root changed while it was in use.",
      unsafe_entry: "Runner storage contains an unsafe filesystem entry.",
      unsafe_permissions: "Runner storage is not owner-private.",
    }[code]);
    this.name = "RunnerStorageError";
  }
}

export interface RunnerDataRoot {
  readonly path: string;
  readonly device: number;
  readonly inode: number;
  readonly retainedRoot?: RunnerDataRootRetainedCapability;
}

export interface RunnerDataRootRetainedCapability {
  readonly configuredPath: string;
  assertUnchanged(): void;
}

export interface RunnerInventoryEntry {
  readonly relativePath: string;
  readonly kind: "directory" | "file";
  readonly sizeBytes: number;
  readonly sha256: string;
}

export interface RunnerInventory {
  readonly entries: readonly RunnerInventoryEntry[];
  readonly count: number;
  readonly bytes: number;
  readonly sha256: string;
}

export interface RunnerInventoryLimits {
  readonly maxEntries?: number;
  readonly maxBytes?: number;
  readonly maxFileBytes?: number;
  readonly maxDepth?: number;
}

/**
 * Open one explicit runner data root and reduce it to its real, non-symlink
 * directory. Callers must retain and pass this object rather than re-resolving
 * the operator-supplied path.
 */
export async function openRunnerDataRoot(configuredRoot: string): Promise<RunnerDataRoot> {
  if (typeof configuredRoot !== "string" || !configuredRoot.trim() || !isAbsolute(configuredRoot)) {
    throw new RunnerStorageError("configuration");
  }
  const normalized = resolve(configuredRoot);
  if (dirname(normalized) === normalized) throw new RunnerStorageError("configuration");
  const prior = await lstatIfPresent(normalized);
  if (prior?.isSymbolicLink() || (prior && !prior.isDirectory())) {
    throw new RunnerStorageError("unsafe_entry");
  }
  await mkdir(normalized, { recursive: true, mode: PRIVATE_DIRECTORY_MODE });
  const created = await lstat(normalized);
  if (created.isSymbolicLink() || !created.isDirectory()) {
    throw new RunnerStorageError("unsafe_entry");
  }
  assertOwnedByCurrentUser(created.uid);
  await chmod(normalized, PRIVATE_DIRECTORY_MODE);

  const canonical = await realpath(normalized);
  const metadata = await lstat(canonical);
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    throw new RunnerStorageError("unsafe_entry");
  }
  assertOwnedByCurrentUser(metadata.uid);
  await chmod(canonical, PRIVATE_DIRECTORY_MODE);
  const secured = await lstat(canonical);
  assertOwnerPrivate(secured.mode, secured.uid, PRIVATE_DIRECTORY_MODE);
  return { path: canonical, device: secured.dev, inode: secured.ino };
}

/**
 * Derives the compatibility pathname view only after the native root has been
 * opened and exclusively locked. Production callers retain this capability on
 * every subsequent legacy control-file operation.
 */
export async function bindRunnerDataRoot(
  retainedRoot: RunnerDataRootRetainedCapability,
): Promise<RunnerDataRoot> {
  retainedRoot.assertUnchanged();
  const root = await openRunnerDataRoot(retainedRoot.configuredPath);
  retainedRoot.assertUnchanged();
  if (root.path !== retainedRoot.configuredPath) {
    throw new RunnerStorageError("unsafe_entry");
  }
  return Object.freeze({ ...root, retainedRoot });
}

export async function assertRunnerDataRoot(root: RunnerDataRoot): Promise<void> {
  root.retainedRoot?.assertUnchanged();
  const metadata = await lstat(root.path).catch(() => {
    throw new RunnerStorageError("root_changed");
  });
  if (metadata.isSymbolicLink()
    || !metadata.isDirectory()
    || metadata.dev !== root.device
    || metadata.ino !== root.inode) {
    throw new RunnerStorageError("root_changed");
  }
  assertOwnerPrivate(metadata.mode, metadata.uid, PRIVATE_DIRECTORY_MODE);
  root.retainedRoot?.assertUnchanged();
}

export function runnerPath(root: RunnerDataRoot, ...components: string[]): string {
  root.retainedRoot?.assertUnchanged();
  if (components.length === 0) return root.path;
  for (const component of components) {
    if (!component
      || component === "."
      || component === ".."
      || component.includes("/")
      || component.includes("\\")
      || component.includes("\0")) {
      throw new RunnerStorageError("path_escape");
    }
  }
  const candidate = resolve(root.path, ...components);
  assertPathInsideRoot(root, candidate, false);
  return candidate;
}

export function assertPathInsideRoot(
  root: RunnerDataRoot,
  candidate: string,
  allowRoot = false,
): string {
  const normalized = resolve(candidate);
  const relation = relative(root.path, normalized);
  if ((!allowRoot && !relation)
    || relation === ".."
    || relation.startsWith(`..${sep}`)
    || isAbsolute(relation)) {
    throw new RunnerStorageError("path_escape");
  }
  return normalized;
}

export async function ensurePrivateRunnerDirectory(
  root: RunnerDataRoot,
  ...components: string[]
): Promise<string> {
  await assertRunnerDataRoot(root);
  let current = root.path;
  for (const component of components) {
    const target = runnerPath({ ...root, path: current }, component);
    assertPathInsideRoot(root, target);
    const existing = await lstatIfPresent(target);
    if (!existing) {
      try {
        await mkdir(target, { mode: PRIVATE_DIRECTORY_MODE });
      } catch (error) {
        if (nodeErrorCode(error) !== "EEXIST") throw error;
      }
    }
    await assertNoSymlinkComponents(root, target, false);
    const metadata = await lstat(target);
    if (metadata.isSymbolicLink() || !metadata.isDirectory() || metadata.dev !== root.device) {
      throw new RunnerStorageError("unsafe_entry");
    }
    assertOwnedByCurrentUser(metadata.uid);
    await chmod(target, PRIVATE_DIRECTORY_MODE);
    const secured = await lstat(target);
    assertOwnerPrivate(secured.mode, secured.uid, PRIVATE_DIRECTORY_MODE);
    current = target;
  }
  return current;
}

export async function readBoundedRegularFile(
  root: RunnerDataRoot,
  path: string,
  maximumBytes: number,
): Promise<Buffer | undefined> {
  assertPathInsideRoot(root, path);
  assertPositiveBound(maximumBytes);
  await assertNoSymlinkComponents(root, path, true);
  const metadata = await lstatIfPresent(path);
  if (!metadata) return undefined;
  if (metadata.isSymbolicLink()
    || !metadata.isFile()
    || metadata.nlink !== 1
    || metadata.dev !== root.device
    || metadata.size > maximumBytes) {
    throw new RunnerStorageError(metadata.size > maximumBytes ? "inventory_limit" : "unsafe_entry");
  }
  assertOwnerPrivate(metadata.mode, metadata.uid, PRIVATE_FILE_MODE);
  await assertRealPathInsideRoot(root, path);
  const handle = await openNoFollow(path, constants.O_RDONLY);
  try {
    const opened = await handle.stat();
    if (!opened.isFile() || opened.nlink !== 1 || opened.dev !== root.device
      || opened.size > maximumBytes
      || opened.dev !== metadata.dev || opened.ino !== metadata.ino) {
      throw new RunnerStorageError(opened.size > maximumBytes ? "inventory_limit" : "unsafe_entry");
    }
    const bytes = await handle.readFile();
    if (bytes.length !== opened.size) throw new RunnerStorageError("unsafe_entry");
    return bytes;
  } finally {
    await handle.close();
  }
}

/** Write a file exactly once. The existing destination is never replaced. */
export async function writeDurableFileExclusive(
  root: RunnerDataRoot,
  destination: string,
  contents: string | Uint8Array,
): Promise<boolean> {
  await assertSafeParent(root, destination);
  const staging = stagingPath(destination);
  let created = false;
  let staged: Stats | undefined;
  try {
    await writeAndSync(staging, contents);
    staged = await lstat(staging);
    assertPrivateRegularFile(root, staged);
    try {
      await link(staging, destination);
      created = true;
    } catch (error) {
      if (nodeErrorCode(error) !== "EEXIST") throw error;
    }
    if (created) await syncRunnerDirectoryDurably(dirname(destination));
  } finally {
    await removeStagingFileDurably(staging);
  }
  if (!created) {
    assertPrivateRegularFile(root, await lstat(destination));
    return false;
  }
  if (!staged) throw new RunnerStorageError("unsafe_entry");
  const persisted = await lstat(destination);
  assertPrivateRegularFile(root, persisted);
  if (persisted.dev !== staged.dev
    || persisted.ino !== staged.ino
    || persisted.size !== staged.size) {
    throw new RunnerStorageError("unsafe_entry");
  }
  return true;
}

/** Atomically replace a mutable journal or tombstone after syncing its bytes. */
export async function replaceDurableFile(
  root: RunnerDataRoot,
  destination: string,
  contents: string | Uint8Array,
): Promise<void> {
  await assertSafeParent(root, destination);
  const existing = await lstatIfPresent(destination);
  if (existing && (existing.isSymbolicLink() || !existing.isFile() || existing.nlink !== 1)) {
    throw new RunnerStorageError("unsafe_entry");
  }
  const staging = stagingPath(destination);
  let staged: Stats | undefined;
  try {
    await writeAndSync(staging, contents);
    staged = await lstat(staging);
    assertPrivateRegularFile(root, staged);
    await rename(staging, destination);
    const persisted = await lstat(destination);
    assertPrivateRegularFile(root, persisted);
    if (persisted.dev !== staged.dev
      || persisted.ino !== staged.ino
      || persisted.size !== staged.size) {
      throw new RunnerStorageError("unsafe_entry");
    }
    await syncRunnerDirectoryDurably(dirname(destination));
  } finally {
    await removeStagingFileDurably(staging);
  }
}

export async function listSafeRunnerDirectory(
  root: RunnerDataRoot,
  directory: string,
): Promise<readonly string[]> {
  assertPathInsideRoot(root, directory, directory === root.path);
  await assertNoSymlinkComponents(root, directory, true);
  const metadata = await lstatIfPresent(directory);
  if (!metadata) return [];
  if (metadata.isSymbolicLink() || !metadata.isDirectory() || metadata.dev !== root.device) {
    throw new RunnerStorageError("unsafe_entry");
  }
  await assertRealPathInsideRoot(root, directory);
  return (await readdir(directory)).sort((left, right) => left.localeCompare(right));
}

export async function pathExistsNoFollow(
  root: RunnerDataRoot,
  path: string,
): Promise<boolean> {
  assertPathInsideRoot(root, path);
  await assertNoSymlinkComponents(root, path, true);
  const metadata = await lstatIfPresent(path);
  if (!metadata) return false;
  if (metadata.isSymbolicLink() || metadata.dev !== root.device) {
    throw new RunnerStorageError("unsafe_entry");
  }
  await assertRealPathInsideRoot(root, path);
  return true;
}

export async function inventoryRunnerPaths(
  root: RunnerDataRoot,
  paths: readonly string[],
  limits: RunnerInventoryLimits = {},
): Promise<RunnerInventory> {
  await assertRunnerDataRoot(root);
  const state: InventoryState = {
    entries: [],
    bytes: 0,
    maxEntries: boundedLimit(limits.maxEntries, DEFAULT_MAX_INVENTORY_ENTRIES),
    maxBytes: boundedLimit(limits.maxBytes, DEFAULT_MAX_INVENTORY_BYTES),
    maxFileBytes: boundedLimit(limits.maxFileBytes, DEFAULT_MAX_FILE_BYTES),
    maxDepth: boundedLimit(limits.maxDepth, DEFAULT_MAX_INVENTORY_DEPTH),
  };
  const targets = minimalUniquePaths(root, paths);
  for (const target of targets) await inventoryEntry(root, target, state, 0);
  state.entries.sort((left, right) => left.relativePath.localeCompare(right.relativePath));
  return {
    entries: state.entries,
    count: state.entries.length,
    bytes: state.bytes,
    sha256: inventoryDigest(state.entries),
  };
}

export async function removeRunnerPathsSafely(
  root: RunnerDataRoot,
  paths: readonly string[],
): Promise<void> {
  await assertRunnerDataRoot(root);
  for (const target of minimalUniquePaths(root, paths)) {
    await removeSafeEntry(root, target);
  }
}

/**
 * Remove one already-empty legacy scaffold without recursively deleting a
 * concurrently created entry. This transitional helper retains all existing
 * root/same-device/no-follow checks and lets `rmdir` fail closed on ENOTEMPTY.
 */
export async function removeEmptyRunnerDirectorySafely(
  root: RunnerDataRoot,
  path: string,
): Promise<void> {
  assertPathInsideRoot(root, path);
  await assertRunnerDataRoot(root);
  await assertNoSymlinkComponents(root, path, true);
  const metadata = await lstatIfPresent(path);
  if (!metadata) return;
  if (metadata.isSymbolicLink() || !metadata.isDirectory() || metadata.dev !== root.device) {
    throw new RunnerStorageError("unsafe_entry");
  }
  await assertRealPathInsideRoot(root, path);
  try {
    await rmdir(path);
  } catch (error) {
    if (nodeErrorCode(error) === "ENOENT") return;
    throw error;
  }
  await syncRunnerDirectoryDurably(dirname(path));
}

async function syncRunnerDirectoryDurably(path: string): Promise<void> {
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY);
    const metadata = await handle.stat();
    if (!metadata.isDirectory()) throw new RunnerStorageError("unsafe_entry");
    await handle.sync();
  } finally {
    if (handle) await handle.close();
  }
}

interface InventoryState {
  entries: RunnerInventoryEntry[];
  bytes: number;
  maxEntries: number;
  maxBytes: number;
  maxFileBytes: number;
  maxDepth: number;
}

async function inventoryEntry(
  root: RunnerDataRoot,
  path: string,
  state: InventoryState,
  depth: number,
): Promise<void> {
  if (depth > state.maxDepth) throw new RunnerStorageError("inventory_limit");
  assertPathInsideRoot(root, path);
  await assertNoSymlinkComponents(root, path, true);
  const metadata = await lstatIfPresent(path);
  if (!metadata) return;
  if (metadata.isSymbolicLink()) throw new RunnerStorageError("unsafe_entry");
  if (metadata.dev !== root.device) throw new RunnerStorageError("unsafe_entry");
  await assertRealPathInsideRoot(root, path);
  if (metadata.isDirectory()) {
    addInventoryEntry(state, {
      relativePath: relativeRunnerPath(root, path),
      kind: "directory",
      sizeBytes: 0,
      sha256: DIRECTORY_DIGEST,
    });
    const entries = (await readdir(path)).sort((left, right) => left.localeCompare(right));
    for (const entry of entries) await inventoryEntry(root, join(path, entry), state, depth + 1);
    return;
  }
  if (!metadata.isFile() || metadata.nlink !== 1) {
    throw new RunnerStorageError("unsafe_entry");
  }
  if (metadata.size > state.maxFileBytes
    || state.bytes > state.maxBytes - metadata.size) {
    throw new RunnerStorageError("inventory_limit");
  }
  const digest = await hashRegularFile(root, path, metadata.dev, metadata.ino, metadata.size);
  state.bytes += metadata.size;
  addInventoryEntry(state, {
    relativePath: relativeRunnerPath(root, path),
    kind: "file",
    sizeBytes: metadata.size,
    sha256: digest,
  });
}

function addInventoryEntry(state: InventoryState, entry: RunnerInventoryEntry): void {
  if (state.entries.length >= state.maxEntries) throw new RunnerStorageError("inventory_limit");
  state.entries.push(entry);
}

async function hashRegularFile(
  root: RunnerDataRoot,
  path: string,
  expectedDevice: number,
  expectedInode: number,
  expectedSize: number,
): Promise<string> {
  const handle = await openNoFollow(path, constants.O_RDONLY);
  try {
    const metadata = await handle.stat();
    if (!metadata.isFile()
      || metadata.nlink !== 1
      || metadata.dev !== expectedDevice
      || metadata.ino !== expectedInode
      || metadata.size !== expectedSize) {
      throw new RunnerStorageError("unsafe_entry");
    }
    const digest = createHash("sha256");
    const buffer = Buffer.allocUnsafe(64 * 1024);
    let position = 0;
    while (position < expectedSize) {
      const { bytesRead } = await handle.read(
        buffer,
        0,
        Math.min(buffer.length, expectedSize - position),
        position,
      );
      if (bytesRead === 0) throw new RunnerStorageError("unsafe_entry");
      digest.update(buffer.subarray(0, bytesRead));
      position += bytesRead;
    }
    const after = await handle.stat();
    if (after.size !== expectedSize
      || after.nlink !== 1
      || after.dev !== expectedDevice
      || after.ino !== expectedInode) {
      throw new RunnerStorageError("unsafe_entry");
    }
    await assertRunnerDataRoot(root);
    return digest.digest("hex");
  } finally {
    await handle.close();
  }
}

async function removeSafeEntry(root: RunnerDataRoot, path: string): Promise<void> {
  assertPathInsideRoot(root, path);
  await assertNoSymlinkComponents(root, path, true);
  const metadata = await lstatIfPresent(path);
  if (!metadata) return;
  if (metadata.isSymbolicLink()) throw new RunnerStorageError("unsafe_entry");
  if (metadata.dev !== root.device) throw new RunnerStorageError("unsafe_entry");
  await assertRealPathInsideRoot(root, path);
  if (metadata.isFile()) {
    const current = await lstat(path);
    if (current.isSymbolicLink() || !current.isFile() || current.nlink !== 1
      || current.dev !== metadata.dev || current.ino !== metadata.ino) {
      throw new RunnerStorageError("unsafe_entry");
    }
    await unlink(path);
    await syncRunnerDirectoryDurably(dirname(path));
    return;
  }
  if (!metadata.isDirectory()) throw new RunnerStorageError("unsafe_entry");
  const entries = (await readdir(path)).sort((left, right) => left.localeCompare(right));
  for (const entry of entries) await removeSafeEntry(root, join(path, entry));
  const current = await lstat(path);
  if (current.isSymbolicLink() || !current.isDirectory()
    || current.dev !== metadata.dev || current.ino !== metadata.ino) {
    throw new RunnerStorageError("unsafe_entry");
  }
  await rmdir(path);
  await syncRunnerDirectoryDurably(dirname(path));
}

function minimalUniquePaths(root: RunnerDataRoot, paths: readonly string[]): string[] {
  const unique = [...new Set(paths.map((path) => assertPathInsideRoot(root, path)))];
  unique.sort((left, right) => left.length - right.length || left.localeCompare(right));
  return unique.filter((candidate, index) => !unique.slice(0, index).some((parent) => (
    candidate.startsWith(`${parent}${sep}`)
  )));
}

function inventoryDigest(entries: readonly RunnerInventoryEntry[]): string {
  const digest = createHash("sha256");
  for (const entry of entries) {
    digest.update(Buffer.from(entry.relativePath, "utf8").toString("base64url"), "utf8");
    digest.update("\n", "utf8");
    digest.update(entry.kind, "utf8");
    digest.update("\n", "utf8");
    digest.update(String(entry.sizeBytes), "utf8");
    digest.update("\n", "utf8");
    digest.update(entry.sha256, "utf8");
    digest.update("\n", "utf8");
  }
  return digest.digest("hex");
}

async function assertSafeParent(root: RunnerDataRoot, destination: string): Promise<void> {
  assertPathInsideRoot(root, destination);
  const parent = dirname(destination);
  assertPathInsideRoot(root, parent, parent === root.path);
  await assertNoSymlinkComponents(root, parent, false);
  const metadata = await lstat(parent);
  if (metadata.isSymbolicLink() || !metadata.isDirectory() || metadata.dev !== root.device) {
    throw new RunnerStorageError("unsafe_entry");
  }
  await assertRealPathInsideRoot(root, parent);
}

async function writeAndSync(path: string, contents: string | Uint8Array): Promise<void> {
  const handle = await open(
    path,
    constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL,
    PRIVATE_FILE_MODE,
  );
  try {
    const metadata = await handle.stat();
    if (!metadata.isFile() || metadata.nlink !== 1) {
      throw new RunnerStorageError("unsafe_entry");
    }
    await handle.writeFile(contents);
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function removeStagingFileDurably(path: string): Promise<void> {
  try {
    await unlink(path);
  } catch (error) {
    if (nodeErrorCode(error) === "ENOENT") return;
    throw error;
  }
  await syncRunnerDirectoryDurably(dirname(path));
}

async function openNoFollow(path: string, flags: number) {
  const noFollow = typeof constants.O_NOFOLLOW === "number" ? constants.O_NOFOLLOW : 0;
  try {
    return await open(path, flags | noFollow);
  } catch (error) {
    if (nodeErrorCode(error) === "ELOOP") throw new RunnerStorageError("unsafe_entry");
    throw error;
  }
}

async function assertRealPathInsideRoot(root: RunnerDataRoot, path: string): Promise<void> {
  const canonical = await realpath(path);
  assertPathInsideRoot(root, canonical, canonical === root.path);
}

async function assertNoSymlinkComponents(
  root: RunnerDataRoot,
  path: string,
  allowMissingSuffix: boolean,
): Promise<void> {
  const normalized = assertPathInsideRoot(root, path, path === root.path);
  await assertRunnerDataRoot(root);
  const relation = relative(root.path, normalized);
  if (!relation) return;
  const components = relation.split(sep);
  let current = root.path;
  for (let index = 0; index < components.length; index += 1) {
    current = join(current, components[index]!);
    const metadata = await lstatIfPresent(current);
    if (!metadata) {
      if (allowMissingSuffix) return;
      throw new RunnerStorageError("unsafe_entry");
    }
    if (metadata.isSymbolicLink()) throw new RunnerStorageError("unsafe_entry");
    if (index < components.length - 1 && !metadata.isDirectory()) {
      throw new RunnerStorageError("unsafe_entry");
    }
  }
}

function relativeRunnerPath(root: RunnerDataRoot, path: string): string {
  const value = relative(root.path, assertPathInsideRoot(root, path));
  if (!value || value === ".." || value.startsWith(`..${sep}`) || isAbsolute(value)) {
    throw new RunnerStorageError("path_escape");
  }
  return value.split(sep).join("/");
}

function stagingPath(destination: string): string {
  return `${destination}.${process.pid}.${randomBytes(12).toString("hex")}.new`;
}

async function lstatIfPresent(path: string) {
  try {
    return await lstat(path);
  } catch (error) {
    if (nodeErrorCode(error) === "ENOENT") return undefined;
    throw error;
  }
}

function assertOwnerPrivate(mode: number, uid: number, expectedMode: number): void {
  if (process.platform !== "win32" && (mode & 0o777) !== expectedMode) {
    throw new RunnerStorageError("unsafe_permissions");
  }
  assertOwnedByCurrentUser(uid);
}

function assertPrivateRegularFile(root: RunnerDataRoot, metadata: Stats): void {
  if (metadata.isSymbolicLink()
    || !metadata.isFile()
    || metadata.nlink !== 1
    || metadata.dev !== root.device) {
    throw new RunnerStorageError("unsafe_entry");
  }
  assertOwnerPrivate(metadata.mode, metadata.uid, PRIVATE_FILE_MODE);
}

function assertOwnedByCurrentUser(uid: number): void {
  if (process.platform !== "win32"
    && typeof process.getuid === "function"
    && uid !== process.getuid()) {
    throw new RunnerStorageError("unsafe_permissions");
  }
}

function assertPositiveBound(value: number): void {
  if (!Number.isSafeInteger(value) || value <= 0) throw new RunnerStorageError("configuration");
}

function boundedLimit(value: number | undefined, fallback: number): number {
  const candidate = value ?? fallback;
  assertPositiveBound(candidate);
  return candidate;
}

function nodeErrorCode(error: unknown): string | undefined {
  return error && typeof error === "object" && "code" in error
    ? String((error as { code?: unknown }).code)
    : undefined;
}
