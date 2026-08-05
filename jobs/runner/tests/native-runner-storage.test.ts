import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  createInjectedNativeRunnerStorageForTest,
  NativeRunnerStorageError,
} from "../src/native-runner-storage.js";

const ROOT = "/srv/bluey-runner";
const FILE_DIGEST = "a".repeat(64);
const DIRECTORY_DIGEST = "b".repeat(64);
const INVENTORY_DIGEST = "c".repeat(64);
const DEVICE_ID = "unix:602";

describe("native runner storage facade", () => {
  it("validates the exact module and result contract while preserving promises", async () => {
    const factory = createInjectedNativeRunnerStorageForTest(validModule());
    const root = factory.openRoot(ROOT);
    expect(root.configuredPath).toBe(ROOT);
    expect(root.deviceId).toBe(DEVICE_ID);
    expect(root.linkCount).toBe(2);
    expect(root.assertUnchanged()).toBeUndefined();

    const directory = await root.ensureDirectory(["account-data-v2", "subjects"]);
    expect(directory.relativePath).toBe("account-data-v2/subjects");
    expect(directory.canonicalPath).toBe(join(ROOT, "account-data-v2", "subjects"));
    expect(directory.deviceId).toBe(DEVICE_ID);
    expect(directory.linkCount).toBe(2);
    const subject = await directory.ensureChildDirectory("subject-1");
    await expect(subject.writeFileExclusive("Local State", Buffer.from("chrome")))
      .resolves.toBe(true);
    await expect(subject.writeFileExclusive(".com.google.Chrome", Buffer.from("dot")))
      .resolves.toBe(true);
    await expect(subject.writeFileExclusive(".bluey-runner-storage.lock", Buffer.from("no")))
      .rejects.toMatchObject({ code: "path_escape" });

    await expect(
      root.moveEntryNoReplace(
        ["account-data-v1", "profile-1"],
        ["account-data-v2", "subjects", "subject-1", "profiles", "profile-1"],
      ),
    ).resolves.toBe("moved");

    const write = subject.writeFileExclusive("record.json", Buffer.from("bluey"));
    expect(write).toBeInstanceOf(Promise);
    await expect(write).resolves.toBe(true);
    await expect(subject.replaceFile("record.json", Buffer.from("cue"))).resolves.toBeUndefined();
    await expect(subject.readFileBounded("record.json", 16)).resolves.toEqual(Buffer.from("bluey"));
    await expect(subject.inventory()).resolves.toEqual({
      entries: [
        {
          relativePath: ".bluey-runner-storage.lock",
          kind: "file",
          deviceId: DEVICE_ID,
          linkCount: 1,
          sizeBytes: 0,
          sha256: FILE_DIGEST,
        },
        {
          relativePath: "Local State",
          kind: "file",
          deviceId: DEVICE_ID,
          linkCount: 1,
          sizeBytes: 0,
          sha256: FILE_DIGEST,
        },
        {
          relativePath: "profiles",
          kind: "directory",
          deviceId: DEVICE_ID,
          linkCount: 2,
          sizeBytes: 0,
          sha256: DIRECTORY_DIGEST,
        },
        {
          relativePath: "profiles/record.json",
          kind: "file",
          deviceId: DEVICE_ID,
          linkCount: 1,
          sizeBytes: 5,
          sha256: FILE_DIGEST,
        },
      ],
      count: 4,
      bytes: 5,
      sha256: INVENTORY_DIGEST,
    });
    await expect(subject.removeEntry("record.json")).resolves.toBeUndefined();
  });

  it("rejects missing, extra, or non-constructor exports", () => {
    expectContractFailure(() => createInjectedNativeRunnerStorageForTest({}));
    expectContractFailure(() => createInjectedNativeRunnerStorageForTest({
      ...validModule(),
      unsafeFallback: true,
    }));
    expectContractFailure(() => createInjectedNativeRunnerStorageForTest({
      RunnerStorageDirectory: FakeDirectory,
      RunnerStorageRoot: {},
    }));
  });

  it("rejects synchronous heavy methods and malformed inventory results", async () => {
    class SynchronousDirectory extends FakeDirectory {
      override inventory(): unknown {
        return validInventory();
      }
    }
    const syncFactory = createInjectedNativeRunnerStorageForTest(moduleWith(SynchronousDirectory));
    const syncDirectory = await syncFactory.openRoot(ROOT).ensureDirectory(["subject"]);
    await expect(syncDirectory.inventory())
      .rejects.toMatchObject({ code: "native_contract_invalid", operation: "inventory" });

    class MalformedInventoryDirectory extends FakeDirectory {
      override inventory(): Promise<unknown> {
        return Promise.resolve({ ...validInventory(), extra: true });
      }
    }
    const malformedFactory = createInjectedNativeRunnerStorageForTest(
      moduleWith(MalformedInventoryDirectory),
    );
    const malformedDirectory = await malformedFactory.openRoot(ROOT).ensureDirectory(["subject"]);
    await expect(malformedDirectory.inventory())
      .rejects.toMatchObject({ code: "native_contract_invalid", operation: "inventory" });
  });

  it("accepts only enumerated native error codes", async () => {
    class KnownErrorDirectory extends FakeDirectory {
      override removeEntry(): Promise<void> {
        return Promise.reject(new Error("bluey_runner_storage:unsafe_entry"));
      }
    }
    const knownFactory = createInjectedNativeRunnerStorageForTest(moduleWith(KnownErrorDirectory));
    const knownDirectory = await knownFactory.openRoot(ROOT).ensureDirectory(["subject"]);
    await expect(knownDirectory.removeEntry("record"))
      .rejects.toMatchObject({ code: "unsafe_entry", operation: "remove_entry" });

    class UnknownErrorDirectory extends FakeDirectory {
      override removeEntry(): Promise<void> {
        return Promise.reject(new Error("bluey_runner_storage:not_a_real_code"));
      }
    }
    const unknownFactory = createInjectedNativeRunnerStorageForTest(
      moduleWith(UnknownErrorDirectory),
    );
    const unknownDirectory = await unknownFactory.openRoot(ROOT).ensureDirectory(["subject"]);
    await expect(unknownDirectory.removeEntry("record"))
      .rejects.toMatchObject({ code: "native_contract_invalid", operation: "remove_entry" });
  });

  it("rejects lexical escapes and invalid bounded-read inputs before native dispatch", async () => {
    const factory = createInjectedNativeRunnerStorageForTest(validModule());
    expect(() => factory.openRoot("relative/path")).toThrowError(NativeRunnerStorageError);
    const root = factory.openRoot(ROOT);
    await expect(root.moveEntryNoReplace([], ["subject"]))
      .rejects.toMatchObject({ code: "configuration" });
    await expect(root.ensureDirectory([".."]))
      .rejects.toMatchObject({ code: "path_escape" });
    const directory = await root.ensureDirectory(["subject"]);
    await expect(directory.readFileBounded("record", 0))
      .rejects.toMatchObject({ code: "configuration" });
  });
});

class FakeDirectory {
  readonly relativePath: string;
  readonly canonicalPath: string;
  readonly deviceId = DEVICE_ID;
  readonly linkCount = "2";

  constructor(relativePath: string, root = ROOT) {
    this.relativePath = relativePath;
    this.canonicalPath = join(root, ...relativePath.split("/").filter(Boolean));
  }

  ensureChildDirectory(name: string): Promise<FakeDirectory> {
    return Promise.resolve(
      new (this.constructor as typeof FakeDirectory)(`${this.relativePath}/${name}`),
    );
  }

  openChildDirectory(name: string): Promise<FakeDirectory> {
    return this.ensureChildDirectory(name);
  }

  writeFileExclusive(): Promise<boolean> {
    return Promise.resolve(true);
  }

  replaceFile(): Promise<void> {
    return Promise.resolve();
  }

  readFileBounded(): Promise<Buffer> {
    return Promise.resolve(Buffer.from("bluey"));
  }

  inventory(): Promise<unknown> | unknown {
    return Promise.resolve(validInventory());
  }

  removeEntry(): Promise<void> {
    return Promise.resolve();
  }
}

class FakeRoot {
  readonly configuredPath: string;
  readonly deviceId = DEVICE_ID;
  readonly linkCount = "2";

  constructor(
    configuredPath: string,
    private readonly Directory: typeof FakeDirectory = FakeDirectory,
  ) {
    this.configuredPath = configuredPath;
  }

  assertUnchanged(): void {}

  ensureDirectory(components: string[]): Promise<FakeDirectory> {
    return Promise.resolve(new this.Directory(components.join("/"), this.configuredPath));
  }

  openDirectory(components: string[]): Promise<FakeDirectory> {
    return this.ensureDirectory(components);
  }

  moveEntryNoReplace(): Promise<string> {
    return Promise.resolve("moved");
  }
}

function validModule(): object {
  return moduleWith(FakeDirectory);
}

function moduleWith(Directory: typeof FakeDirectory): object {
  return {
    RunnerStorageDirectory: Directory,
    RunnerStorageRoot: class extends FakeRoot {
      constructor(configuredPath: string) {
        super(configuredPath, Directory);
      }
    },
  };
}

function validInventory(): object {
  return {
    entries: [
      {
        relativePath: ".bluey-runner-storage.lock",
        kind: "file",
        deviceId: DEVICE_ID,
        linkCount: "1",
        sizeBytes: "0",
        sha256: FILE_DIGEST,
      },
      {
        relativePath: "Local State",
        kind: "file",
        deviceId: DEVICE_ID,
        linkCount: "1",
        sizeBytes: "0",
        sha256: FILE_DIGEST,
      },
      {
        relativePath: "profiles",
        kind: "directory",
        deviceId: DEVICE_ID,
        linkCount: "2",
        sizeBytes: "0",
        sha256: DIRECTORY_DIGEST,
      },
      {
        relativePath: "profiles/record.json",
        kind: "file",
        deviceId: DEVICE_ID,
        linkCount: "1",
        sizeBytes: "5",
        sha256: FILE_DIGEST,
      },
    ],
    count: 4,
    bytes: "5",
    sha256: INVENTORY_DIGEST,
  };
}

function expectContractFailure(action: () => unknown): void {
  try {
    action();
    throw new Error("expected native contract validation to fail");
  } catch (error) {
    expect(error).toBeInstanceOf(NativeRunnerStorageError);
    expect(error).toMatchObject({ code: "native_contract_invalid", operation: "load" });
  }
}
