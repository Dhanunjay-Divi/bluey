import {
  chmod,
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  readdir,
  rename,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  assertRunnerDataRoot,
  inventoryRunnerPaths,
  openRunnerDataRoot,
  removeEmptyRunnerDirectorySafely,
  removeRunnerPathsSafely,
  replaceDurableFile,
  runnerPath,
  writeDurableFileExclusive,
} from "../src/safe-runner-storage.js";

const EMPTY_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("safe runner storage", () => {
  it("requires an explicit absolute non-root path and canonicalizes it to mode 0700", async () => {
    await expect(openRunnerDataRoot("relative/runner-data"))
      .rejects.toMatchObject({ code: "configuration" });
    await expect(openRunnerDataRoot("/"))
      .rejects.toMatchObject({ code: "configuration" });

    const parent = await temporaryDirectory();
    const configured = join(parent, "runner-data");
    const root = await openRunnerDataRoot(configured);

    expect(root.path).toBe(await realpath(configured));
    if (process.platform !== "win32") {
      expect((await lstat(root.path)).mode & 0o777).toBe(0o700);
      await chmod(root.path, 0o750);
      await expect(assertRunnerDataRoot(root))
        .rejects.toMatchObject({ code: "unsafe_permissions" });
      await chmod(root.path, 0o700);
    }
    await expect(assertRunnerDataRoot(root)).resolves.toBeUndefined();
  });

  it("rejects a symlink data root and an inode-swapped root", async () => {
    const parent = await temporaryDirectory();
    const real = join(parent, "real");
    const alias = join(parent, "alias");
    await mkdir(real, { mode: 0o700 });
    await symlink(real, alias);
    await expect(openRunnerDataRoot(alias)).rejects.toMatchObject({ code: "unsafe_entry" });

    const root = await openRunnerDataRoot(join(parent, "runner"));
    await rename(root.path, join(parent, "old-runner"));
    await mkdir(root.path, { mode: 0o700 });
    await expect(assertRunnerDataRoot(root)).rejects.toMatchObject({ code: "root_changed" });
  });

  it("produces a bounded canonical inventory with the standard empty digest", async () => {
    const root = await freshRoot();
    expect(await inventoryRunnerPaths(root, [runnerPath(root, "missing")])).toEqual({
      entries: [],
      count: 0,
      bytes: 0,
      sha256: EMPTY_SHA256,
    });

    const directory = runnerPath(root, "profile");
    await mkdir(join(directory, "nested"), { recursive: true, mode: 0o700 });
    await writeFile(join(directory, "one"), "one", { mode: 0o600 });
    await writeFile(join(directory, "nested", "two"), "two", { mode: 0o600 });
    const inventory = await inventoryRunnerPaths(root, [directory]);
    expect(inventory.count).toBe(4);
    expect(inventory.bytes).toBe(6);
    expect(inventory.entries.map((entry) => entry.relativePath)).toEqual([
      "profile",
      "profile/nested",
      "profile/nested/two",
      "profile/one",
    ]);
    await expect(inventoryRunnerPaths(root, [directory], { maxEntries: 3 }))
      .rejects.toMatchObject({ code: "inventory_limit" });
    await expect(inventoryRunnerPaths(root, [directory], { maxBytes: 5 }))
      .rejects.toMatchObject({ code: "inventory_limit" });
    await expect(inventoryRunnerPaths(root, [directory], { maxFileBytes: 2 }))
      .rejects.toMatchObject({ code: "inventory_limit" });
    await expect(inventoryRunnerPaths(root, [directory], { maxDepth: 1 }))
      .rejects.toMatchObject({ code: "inventory_limit" });
  });

  it("rejects path escape and symlinks without touching an outside sentinel", async () => {
    const parent = await temporaryDirectory();
    const root = await openRunnerDataRoot(join(parent, "runner"));
    const outside = join(parent, "outside.txt");
    await writeFile(outside, "sentinel", { mode: 0o600 });
    expect(() => runnerPath(root, "..", "outside.txt"))
      .toThrow(expect.objectContaining({ code: "path_escape" }));

    const unsafe = runnerPath(root, "unsafe");
    await symlink(parent, unsafe);
    const throughSymlink = join(unsafe, "outside.txt");
    await expect(inventoryRunnerPaths(root, [throughSymlink]))
      .rejects.toMatchObject({ code: "unsafe_entry" });
    await expect(removeRunnerPathsSafely(root, [throughSymlink]))
      .rejects.toMatchObject({ code: "unsafe_entry" });
    await expect(readFile(outside, "utf8")).resolves.toBe("sentinel");
  });

  it.runIf(process.platform !== "win32")(
    "rejects hardlinked files and preserves the outside link",
    async () => {
      const parent = await temporaryDirectory();
      const root = await openRunnerDataRoot(join(parent, "runner"));
      const outside = join(parent, "outside.txt");
      const inside = runnerPath(root, "inside.txt");
      await writeFile(outside, "sentinel", { mode: 0o600 });
      await link(outside, inside);

      await expect(inventoryRunnerPaths(root, [inside]))
        .rejects.toMatchObject({ code: "unsafe_entry" });
      await expect(removeRunnerPathsSafely(root, [inside]))
        .rejects.toMatchObject({ code: "unsafe_entry" });
      await expect(readFile(outside, "utf8")).resolves.toBe("sentinel");
      expect((await lstat(outside)).nlink).toBe(2);
    },
  );

  it("deletes only the requested subtree and retains exact siblings", async () => {
    const root = await freshRoot();
    const subjectA = runnerPath(root, "subject-a");
    const subjectB = runnerPath(root, "subject-b");
    await mkdir(subjectA, { mode: 0o700 });
    await mkdir(subjectB, { mode: 0o700 });
    await writeFile(join(subjectA, "data"), "a", { mode: 0o600 });
    await writeFile(join(subjectB, "data"), "b", { mode: 0o600 });

    await removeRunnerPathsSafely(root, [subjectA]);

    await expect(lstat(subjectA)).rejects.toMatchObject({ code: "ENOENT" });
    await expect(readFile(join(subjectB, "data"), "utf8")).resolves.toBe("b");
  });

  it("removes only a safe empty same-device directory", async () => {
    const root = await freshRoot();
    const empty = runnerPath(root, "legacy-empty");
    await mkdir(empty, { mode: 0o700 });

    await expect(removeEmptyRunnerDirectorySafely(root, empty)).resolves.toBeUndefined();
    await expect(lstat(empty)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("rejects a nonempty directory without removing its contents", async () => {
    const root = await freshRoot();
    const directory = runnerPath(root, "legacy-nonempty");
    const sentinel = join(directory, "sentinel");
    await mkdir(directory, { mode: 0o700 });
    await writeFile(sentinel, "retain", { mode: 0o600 });

    await expect(removeEmptyRunnerDirectorySafely(root, directory)).rejects.toMatchObject({
      code: expect.stringMatching(/^(?:EEXIST|ENOTEMPTY)$/),
    });
    await expect(readFile(sentinel, "utf8")).resolves.toBe("retain");
  });

  it("rejects paths outside the retained root and unsafe entry kinds", async () => {
    const parent = await temporaryDirectory();
    const root = await openRunnerDataRoot(join(parent, "runner"));
    const outside = join(parent, "outside");
    const regularFile = runnerPath(root, "legacy-file");
    const alias = runnerPath(root, "legacy-alias");
    await mkdir(outside, { mode: 0o700 });
    await writeFile(regularFile, "retain", { mode: 0o600 });
    await symlink(outside, alias);

    await expect(removeEmptyRunnerDirectorySafely(root, root.path))
      .rejects.toMatchObject({ code: "path_escape" });
    await expect(removeEmptyRunnerDirectorySafely(root, outside))
      .rejects.toMatchObject({ code: "path_escape" });
    await expect(removeEmptyRunnerDirectorySafely(root, regularFile))
      .rejects.toMatchObject({ code: "unsafe_entry" });
    await expect(removeEmptyRunnerDirectorySafely(root, alias))
      .rejects.toMatchObject({ code: "unsafe_entry" });
    await expect(readFile(regularFile, "utf8")).resolves.toBe("retain");
    await expect(lstat(outside)).resolves.toMatchObject({});
  });

  it("rejects retained-device mismatch and an inode-swapped root", async () => {
    const parent = await temporaryDirectory();
    const root = await openRunnerDataRoot(join(parent, "runner"));
    const retained = runnerPath(root, "legacy-empty");
    await mkdir(retained, { mode: 0o700 });

    await expect(removeEmptyRunnerDirectorySafely(
      { ...root, device: root.device + 1 },
      retained,
    )).rejects.toMatchObject({ code: "root_changed" });
    await expect(lstat(retained)).resolves.toMatchObject({});

    await rename(root.path, join(parent, "old-runner"));
    await mkdir(root.path, { mode: 0o700 });
    const replacementTarget = join(root.path, "legacy-empty");
    await mkdir(replacementTarget, { mode: 0o700 });

    await expect(removeEmptyRunnerDirectorySafely(root, replacementTarget))
      .rejects.toMatchObject({ code: "root_changed" });
    await expect(lstat(replacementTarget)).resolves.toMatchObject({});
  });

  it("creates immutable files and durably replaces only regular destinations", async () => {
    const root = await freshRoot();
    const destination = runnerPath(root, "control.json");
    await expect(writeDurableFileExclusive(root, destination, "one"))
      .resolves.toBe(true);
    await expect(writeDurableFileExclusive(root, destination, "two"))
      .resolves.toBe(false);
    await expect(readFile(destination, "utf8")).resolves.toBe("one");
    await replaceDurableFile(root, destination, "three");
    await expect(readFile(destination, "utf8")).resolves.toBe("three");
    expect(await readdir(root.path)).toEqual(["control.json"]);
    if (process.platform !== "win32") {
      expect((await lstat(destination)).mode & 0o777).toBe(0o600);
      expect((await lstat(destination)).nlink).toBe(1);
    }
  });
});

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-safe-storage-"));
  temporaryDirectories.push(path);
  return path;
}

async function freshRoot() {
  const parent = await temporaryDirectory();
  return openRunnerDataRoot(join(parent, "runner"));
}
