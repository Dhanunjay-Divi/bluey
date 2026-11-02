import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import {
  acquireRunnerStorageRoot,
  assertManagedRunnerProcessArtifactBinding,
  assertManagedRunnerProcessBundleBinding,
} from "../src/server.js";
import {
  NativeRunnerStorageError,
  type NativeRunnerStorageRoot,
} from "../src/native-runner-storage.js";
import type { RunnerDataRoot } from "../src/safe-runner-storage.js";

describe("managed runner process startup bindings", () => {
  const measured = { componentId: "jobs-runner", expectedRole: "managed_runner",
    automationBundleSha256: "a".repeat(64) } as const;
  const runtime = { automationBundleSha256: "a".repeat(64), runnerImageSha256: "b".repeat(64) };

  it("accepts exact bundle and claimed artifact bindings", () => {
    expect(() => assertManagedRunnerProcessBundleBinding(measured, runtime)).not.toThrow();
    expect(() => assertManagedRunnerProcessArtifactBinding({ artifactSha256: "b".repeat(64) }, runtime))
      .not.toThrow();
  });

  it("denies bundle drift, missing measurement and wrong roles before claiming", () => {
    const claim = vi.fn();
    for (const altered of [
      { ...measured, automationBundleSha256: "c".repeat(64) },
      { ...measured, automationBundleSha256: undefined },
      { ...measured, componentId: "jobs-workflows" as const },
      { ...measured, expectedRole: "workflow_worker" as const },
    ]) {
      expect(() => {
        assertManagedRunnerProcessBundleBinding(altered, runtime);
        claim();
      }).toThrow("automation bundle binding is invalid");
    }
    expect(claim).not.toHaveBeenCalled();
  });

  it("denies a different claimed artifact before starting volume control", () => {
    const startVolume = vi.fn();
    expect(() => {
      assertManagedRunnerProcessArtifactBinding({ artifactSha256: "c".repeat(64) }, runtime);
      startVolume();
    }).toThrow("process image binding is invalid");
    expect(startVolume).not.toHaveBeenCalled();
  });

  it("does not impose managed bindings on the existing nonmanaged path", () => {
    expect(() => assertManagedRunnerProcessBundleBinding(undefined, runtime)).not.toThrow();
    expect(() => assertManagedRunnerProcessArtifactBinding(undefined, runtime)).not.toThrow();
  });

  it("keeps the production startup checks before claim and volume construction", () => {
    const source = readFileSync(new URL("../src/server.ts", import.meta.url), "utf8");
    const startup = source.split("async function startRunner(): Promise<void> {")[1]
      .split("await runnerVolumeClient.start(")[0];
    expect(startup.match(/runnerProcessRuntimeGrantFromEnv\(runnerBuildId\)/g)).toHaveLength(1);
    const bundle = startup.indexOf("assertManagedRunnerProcessBundleBinding(");
    const claim = startup.indexOf("await claimManagedCloudRuntimeInstance(");
    const artifact = startup.indexOf("assertManagedRunnerProcessArtifactBinding(");
    const volume = startup.indexOf("new RunnerVolumeClient(");
    expect(bundle).toBeGreaterThanOrEqual(0);
    expect(bundle).toBeLessThan(claim);
    expect(claim).toBeLessThan(artifact);
    expect(artifact).toBeLessThan(volume);
    expect(startup).toContain("processRuntimeGrant,");
  });
});

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
