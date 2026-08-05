import { describe, expect, it, vi } from "vitest";
import { acquireRunnerStorageRoot } from "../src/server.js";
import {
  NativeRunnerStorageError,
  type NativeRunnerStorageRoot,
} from "../src/native-runner-storage.js";
import type { RunnerDataRoot } from "../src/safe-runner-storage.js";

describe("production runner storage acquisition", () => {
  it("acquires and retains the native lock before compatibility-root I/O", async () => {
    const events: string[] = [];
    const nativeRoot = retainedRoot("/srv/bluey-runner", events);
    const dataRoot: RunnerDataRoot = {
      path: "/srv/bluey-runner",
      device: 7,
      inode: 11,
      retainedRoot: nativeRoot,
    };

    await expect(
      acquireRunnerStorageRoot(
        dataRoot.path,
        (configuredPath) => {
          events.push(`native:${configuredPath}`);
          return nativeRoot;
        },
        async (retained) => {
          events.push("compatibility-root");
          expect(retained).toBe(nativeRoot);
          return dataRoot;
        },
      ),
    ).resolves.toEqual({ nativeRoot, dataRoot });
    expect(events).toEqual([
      "native:/srv/bluey-runner",
      "compatibility-root",
      "assert-native-root",
    ]);
  });

  it("performs no compatibility or identity-path I/O after unsafe-root or lock rejection", async () => {
    for (const code of ["unsafe_entry", "root_locked"] as const) {
      const bindDataRoot = vi.fn(async (): Promise<RunnerDataRoot> => {
        throw new Error("compatibility root must not be touched");
      });
      await expect(
        acquireRunnerStorageRoot(
          "/srv/bluey-runner",
          () => {
            throw new NativeRunnerStorageError("open_root", code);
          },
          bindDataRoot,
        ),
      ).rejects.toMatchObject({ code });
      expect(bindDataRoot).not.toHaveBeenCalled();
    }
  });
});

function retainedRoot(
  configuredPath: string,
  events: string[],
): NativeRunnerStorageRoot {
  return {
    configuredPath,
    deviceId: "unix:7:mount:11",
    linkCount: 2,
    assertUnchanged: () => events.push("assert-native-root"),
    ensureDirectory: async () => {
      throw new Error("unused");
    },
    openDirectory: async () => {
      throw new Error("unused");
    },
    moveEntryNoReplace: async () => {
      throw new Error("unused");
    },
  };
}
