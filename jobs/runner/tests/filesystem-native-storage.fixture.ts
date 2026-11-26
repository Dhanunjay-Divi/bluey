// Shared test-only filesystem facade. It exercises managed layout on real temporary
// files, not native retained-handle semantics, production custody or deletion proof.
import { createHash } from "node:crypto";
import { lstatSync, realpathSync, type Stats } from "node:fs";
import { lstat, mkdir, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerMoveOutcome,
  NativeRunnerStorageDirectory,
  NativeRunnerStorageRoot,
} from "../src/native-runner-storage.js";

/**
 * Filesystem-backed native capability injected into the production manager.
 * Native retained-handle semantics are covered by native-storage tests; this
 * fixture keeps client integration evidence on the real managed layout.
 */
export class FilesystemNativeRootForTest implements NativeRunnerStorageRoot {
  readonly #device: number;
  readonly #inode: number;
  readonly deviceId: string;

  constructor(readonly configuredPath: string) {
    const metadata = testDirectoryStat(configuredPath);
    this.#device = metadata.dev;
    this.#inode = metadata.ino;
    this.deviceId = `unix:${metadata.dev.toString(16)}:runner-client-test`;
  }

  get linkCount(): number {
    return this.assertRootBinding().nlink;
  }

  assertUnchanged(): void {
    this.assertRootBinding();
  }

  async ensureDirectory(
    components: readonly string[],
  ): Promise<NativeRunnerStorageDirectory> {
    return this.openDirectoryInternal(components, true);
  }

  async openDirectory(
    components: readonly string[],
  ): Promise<NativeRunnerStorageDirectory> {
    return this.openDirectoryInternal(components, false);
  }

  async moveEntryNoReplace(
    sourceComponents: readonly string[],
    destinationComponents: readonly string[],
  ): Promise<NativeRunnerMoveOutcome> {
    const source = this.pathFor(sourceComponents);
    const destination = this.pathFor(destinationComponents);
    const sourceMetadata = await testLstat(source);
    if (!sourceMetadata) return "source_missing";
    assertTestEntry(sourceMetadata, this.#device);
    const destinationMetadata = await testLstat(destination);
    if (destinationMetadata) {
      assertTestEntry(destinationMetadata, this.#device);
      return "destination_exists";
    }
    await rename(source, destination);
    this.assertRootBinding();
    return "moved";
  }

  async openDirectoryInternal(
    components: readonly string[],
    create: boolean,
  ): Promise<FilesystemNativeDirectoryForTest> {
    let current = this.configuredPath;
    for (const component of components) {
      assertTestComponent(component);
      current = join(current, component);
      let metadata = await testLstat(current);
      if (!metadata && create) {
        await mkdir(current, { mode: 0o700 });
        metadata = await lstat(current);
      }
      if (!metadata) throw new Error("test native directory is absent");
      assertTestDirectory(metadata, this.#device);
    }
    const metadata = testDirectoryStat(current, this.#device);
    this.assertRootBinding();
    return new FilesystemNativeDirectoryForTest(
      this,
      [...components],
      metadata.ino,
    );
  }

  assertDirectoryBinding(components: readonly string[], inode: number): Stats {
    const metadata = testDirectoryStat(this.pathFor(components), this.#device);
    if (metadata.ino !== inode)
      throw new Error("test native directory changed");
    this.assertRootBinding();
    return metadata;
  }

  pathFor(components: readonly string[]): string {
    for (const component of components) assertTestComponent(component);
    return join(this.configuredPath, ...components);
  }

  get device(): number {
    return this.#device;
  }

  private assertRootBinding(): Stats {
    const metadata = testDirectoryStat(this.configuredPath);
    if (
      metadata.dev !== this.#device ||
      metadata.ino !== this.#inode ||
      realpathSync(this.configuredPath) !== this.configuredPath
    ) {
      throw new Error("test native root changed");
    }
    return metadata;
  }
}

export class FilesystemNativeDirectoryForTest implements NativeRunnerStorageDirectory {
  readonly relativePath: string;
  readonly canonicalPath: string;

  constructor(
    private readonly root: FilesystemNativeRootForTest,
    private readonly components: readonly string[],
    private readonly inode: number,
  ) {
    this.relativePath = components.join("/");
    this.canonicalPath = root.pathFor(components);
  }

  get deviceId(): string {
    return this.root.deviceId;
  }

  get linkCount(): number {
    return this.assertBinding().nlink;
  }

  ensureChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.root.ensureDirectory([...this.components, name]);
  }

  openChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.root.openDirectory([...this.components, name]);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const existing = await testLstat(path);
    if (existing) {
      assertTestFile(existing, this.root.device);
      return false;
    }
    try {
      await writeFile(path, contents, { flag: "wx", mode: 0o600 });
    } catch (error) {
      if (!hasFsCode(error, "EEXIST")) throw error;
      assertTestFile(await lstat(path), this.root.device);
      return false;
    }
    assertTestFile(await lstat(path), this.root.device);
    this.assertBinding();
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const existing = await testLstat(path);
    if (existing) assertTestFile(existing, this.root.device);
    await writeFile(path, contents, { mode: 0o600 });
    assertTestFile(await lstat(path), this.root.device);
    this.assertBinding();
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const before = await lstat(path);
    assertTestFile(before, this.root.device);
    if (before.size > maximumBytes)
      throw new Error("test native read exceeds limit");
    const contents = await readFile(path);
    const after = await lstat(path);
    assertTestFile(after, this.root.device);
    if (
      before.ino !== after.ino ||
      before.size !== after.size ||
      contents.byteLength !== after.size
    ) {
      throw new Error("test native file changed during read");
    }
    this.assertBinding();
    return contents;
  }

  async inventory(): Promise<NativeRunnerInventory> {
    this.assertBinding();
    const entries = await collectTestInventory(
      this.canonicalPath,
      this.root.device,
      this.root.deviceId,
    );
    this.assertBinding();
    const bytes = entries.reduce((total, entry) => total + entry.sizeBytes, 0);
    const digest = createHash("sha256");
    digest.update("bluey-jobs-runner-native-inventory-v1\0", "utf8");
    for (const entry of entries) {
      digest.update(
        `${Buffer.byteLength(entry.relativePath, "utf8")}:`,
        "utf8",
      );
      digest.update(entry.relativePath, "utf8");
      digest.update("\n", "utf8");
      digest.update(`${entry.kind}\n`, "utf8");
      digest.update(`${entry.sizeBytes}\n`, "utf8");
      digest.update(`${entry.sha256}\n`, "utf8");
    }
    return Object.freeze({
      entries: Object.freeze(entries),
      count: entries.length,
      bytes,
      sha256: digest.digest("hex"),
    });
  }

  async removeEntry(name: string): Promise<void> {
    this.assertBinding();
    assertTestComponent(name);
    const path = join(this.canonicalPath, name);
    const metadata = await testLstat(path);
    if (!metadata) return;
    assertTestEntry(metadata, this.root.device);
    await rm(path, { recursive: metadata.isDirectory() });
    this.assertBinding();
  }

  private assertBinding(): Stats {
    return this.root.assertDirectoryBinding(this.components, this.inode);
  }
}

async function collectTestInventory(
  root: string,
  device: number,
  deviceId: string,
): Promise<NativeRunnerInventoryEntry[]> {
  const entries: NativeRunnerInventoryEntry[] = [];
  const collect = async (directory: string, prefix: string): Promise<void> => {
    const names = await readdir(directory);
    names.sort(compareTestUtf8);
    for (const name of names) {
      assertTestComponent(name);
      const relativePath = prefix ? `${prefix}/${name}` : name;
      const path = join(directory, name);
      const before = await lstat(path);
      if (before.isDirectory()) {
        assertTestDirectory(before, device);
        entries.push(
          Object.freeze({
            relativePath,
            kind: "directory",
            deviceId,
            linkCount: before.nlink,
            sizeBytes: 0,
            sha256: createHash("sha256")
              .update("bluey-jobs-runner-native-inventory-directory-v1", "utf8")
              .digest("hex"),
          }),
        );
        await collect(path, relativePath);
        continue;
      }
      assertTestFile(before, device);
      const contents = await readFile(path);
      const after = await lstat(path);
      assertTestFile(after, device);
      if (
        before.ino !== after.ino ||
        before.size !== after.size ||
        contents.byteLength !== after.size
      ) {
        throw new Error("test native file changed during inventory");
      }
      entries.push(
        Object.freeze({
          relativePath,
          kind: "file",
          deviceId,
          linkCount: before.nlink,
          sizeBytes: before.size,
          sha256: createHash("sha256").update(contents).digest("hex"),
        }),
      );
    }
  };
  await collect(root, "");
  entries.sort((left, right) =>
    compareTestUtf8(left.relativePath, right.relativePath),
  );
  return entries;
}

function compareTestUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function assertTestComponent(value: string): void {
  if (!/^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,159}$/.test(value)) {
    throw new Error("invalid test native component");
  }
}

function testDirectoryStat(path: string, expectedDevice?: number): Stats {
  const metadata = lstatSync(path);
  assertTestDirectory(metadata, expectedDevice ?? metadata.dev);
  return metadata;
}

async function testLstat(path: string): Promise<Stats | undefined> {
  try {
    return await lstat(path);
  } catch (error) {
    if (hasFsCode(error, "ENOENT")) return undefined;
    throw error;
  }
}

function assertTestEntry(metadata: Stats, device: number): void {
  if (metadata.isDirectory()) assertTestDirectory(metadata, device);
  else assertTestFile(metadata, device);
}

function assertTestDirectory(metadata: Stats, device: number): void {
  if (!metadata.isDirectory() || metadata.dev !== device) {
    throw new Error("unsafe test native directory");
  }
}

function assertTestFile(metadata: Stats, device: number): void {
  if (!metadata.isFile() || metadata.dev !== device || metadata.nlink !== 1) {
    throw new Error("unsafe test native file");
  }
}

function hasFsCode(error: unknown, code: string): boolean {
  return Boolean(
    error &&
    typeof error === "object" &&
    "code" in error &&
    (error as { code?: unknown }).code === code,
  );
}
