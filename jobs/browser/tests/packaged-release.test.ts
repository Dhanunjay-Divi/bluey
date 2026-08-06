import { createPrivateKey, createPublicKey } from "node:crypto";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  claimBrowserBuildProof,
  loadPackagedBrowserBuildProof,
  type BrowserPackagedRuntime,
} from "../src/packaged-release.js";
import {
  BLUEY_BROWSER_BUILD_AUDIENCE,
  canonicalBrowserBuildDescriptorBytes,
  createBrowserBuildDescriptor,
} from "../src/release-authority.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((path) =>
      rm(path, { recursive: true, force: true }),
    ),
  );
});

describe("packaged Browser release identity", () => {
  it("loads exact signed resources and matches the running target", async () => {
    const resourcesPath = await releaseResources();
    const verified = await loadPackagedBrowserBuildProof(runtime(resourcesPath));

    expect(verified?.descriptor.buildId).toBe("browser-603.1");
    expect(claimBrowserBuildProof(verified)).toEqual(verified?.proof);
  });

  it("has no development claim authority without an explicit resource path", async () => {
    const resourcesPath = await temporaryDirectory();
    await expect(
      loadPackagedBrowserBuildProof({
        ...runtime(resourcesPath),
        isPackaged: false,
      }),
    ).resolves.toBeUndefined();
  });

  it("rejects a signed descriptor for another runtime", async () => {
    const resourcesPath = await releaseResources();
    await expect(
      loadPackagedBrowserBuildProof({
        ...runtime(resourcesPath),
        appVersion: "0.1.1",
      }),
    ).rejects.toThrow(/does not match runtime/i);
  });

  it("rejects symlinked and tampered release resources", async () => {
    const resourcesPath = await releaseResources();
    const releaseDirectory = join(resourcesPath, "release");
    const signaturePath = join(releaseDirectory, "build-descriptor.sig");
    await rm(signaturePath);
    await symlink("build-descriptor.txt", signaturePath);
    await expect(
      loadPackagedBrowserBuildProof(runtime(resourcesPath)),
    ).rejects.toThrow(/release resource/i);

    await rm(signaturePath);
    await writeFile(signaturePath, "a".repeat(86));
    await expect(
      loadPackagedBrowserBuildProof(runtime(resourcesPath)),
    ).rejects.toThrow(/descriptor|signature/i);
  });
});

async function releaseResources(): Promise<string> {
  const resourcesPath = await temporaryDirectory();
  const releaseDirectory = join(resourcesPath, "release");
  await mkdir(releaseDirectory);
  const descriptor = createBrowserBuildDescriptor(
    {
      version: 1,
      audience: BLUEY_BROWSER_BUILD_AUDIENCE,
      releaseId: "browser-release-603-1",
      buildId: "browser-603.1",
      appVersion: "0.1.0",
      appId: "sh.bluey.jobs.browser",
      protocolVersion: 1,
      sourceCommit: "1".repeat(40),
      platform: "darwin",
      architecture: "arm64",
      electronVersion: "43.1.0",
      playwrightVersion: "1.61.1",
      chromiumRevision: "1228",
      issuedAtMs: 1_785_970_000_000,
      signingKeyId: "browser-build-key-2026-01",
    },
    signingKey(),
  );
  const publicKey = createPublicKey(signingKey()).export({ format: "jwk" });
  if (!publicKey.x) throw new Error("test key has no public value");
  await Promise.all([
    writeFile(
      join(releaseDirectory, "build-descriptor.txt"),
      canonicalBrowserBuildDescriptorBytes(descriptor),
    ),
    writeFile(
      join(releaseDirectory, "build-descriptor.sig"),
      `${descriptor.signature}\n`,
    ),
    writeFile(
      join(releaseDirectory, "build-public-keys.json"),
      JSON.stringify({
        version: 1,
        audience: "bluey-jobs-browser-build-keyring-v1",
        keys: { "browser-build-key-2026-01": publicKey.x },
      }),
    ),
  ]);
  return resourcesPath;
}

function runtime(resourcesPath: string): BrowserPackagedRuntime {
  return {
    isPackaged: true,
    resourcesPath,
    appVersion: "0.1.0",
    electronVersion: "43.1.0",
    platform: "darwin",
    architecture: "arm64",
  };
}

function signingKey() {
  const prefix = Buffer.from("302e020100300506032b657004220420", "hex");
  const seed = Buffer.from(Array.from({ length: 32 }, (_, index) => index));
  return createPrivateKey({
    key: Buffer.concat([prefix, seed]),
    format: "der",
    type: "pkcs8",
  });
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-browser-release-"));
  temporaryDirectories.push(path);
  return path;
}
