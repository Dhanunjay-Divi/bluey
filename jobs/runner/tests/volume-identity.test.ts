import { randomBytes } from "node:crypto";
import {
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { openRunnerDataRoot, runnerPath } from "../src/safe-runner-storage.js";
import {
  createRunnerProcessInstanceId,
  ed25519PublicKeyFingerprint,
  loadOrCreateRunnerVolumeIdentity,
  signEd25519,
  verifyEd25519,
} from "../src/volume-identity.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("runner volume identity", () => {
  it("creates one stable owner-private Ed25519 identity and signs with it", async () => {
    const root = await freshRoot();
    const first = await loadOrCreateRunnerVolumeIdentity(root);
    const second = await loadOrCreateRunnerVolumeIdentity(root);
    const message = Buffer.from("phase-602-volume-proof", "utf8");
    const signature = signEd25519(first.privateKey, message);

    expect(second.volumeId).toBe(first.volumeId);
    expect(second.publicKeyRaw).toBe(first.publicKeyRaw);
    expect(second.publicKeyFingerprint).toBe(first.publicKeyFingerprint);
    expect(ed25519PublicKeyFingerprint(first.publicKeyRaw)).toBe(first.publicKeyFingerprint);
    expect(verifyEd25519(first.publicKeyRaw, message, signature)).toBe(true);
    expect(verifyEd25519(first.publicKeyRaw, Buffer.from("different"), signature)).toBe(false);
    expect(first.volumeId).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(first.publicKeyRaw).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(first.publicKeyFingerprint).toMatch(/^[0-9a-f]{64}$/);

    const identityPath = runnerPath(root, "volume-identity", "ed25519-private.pk8");
    expect((await readFile(identityPath)).length).toBe(48);
    if (process.platform !== "win32") {
      expect((await lstat(identityPath)).mode & 0o777).toBe(0o600);
      expect((await lstat(identityPath)).nlink).toBe(1);
    }
  });

  it("creates canonical independent process instance IDs", () => {
    const first = createRunnerProcessInstanceId();
    const second = createRunnerProcessInstanceId();
    expect(first).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(second).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(second).not.toBe(first);
  });

  it("never creates a replacement identity on a dirty root", async () => {
    const root = await freshRoot();
    await writeFile(runnerPath(root, "existing-data"), "account bytes", { mode: 0o600 });

    await expect(loadOrCreateRunnerVolumeIdentity(root))
      .rejects.toMatchObject({ code: "dirty_root_without_identity" });
  });

  it("fails closed when stable identity bytes are corrupt", async () => {
    const root = await freshRoot();
    await loadOrCreateRunnerVolumeIdentity(root);
    const identityPath = runnerPath(root, "volume-identity", "ed25519-private.pk8");
    await writeFile(identityPath, randomBytes(48), { mode: 0o600 });

    await expect(loadOrCreateRunnerVolumeIdentity(root))
      .rejects.toMatchObject({ code: "corrupt_identity" });
  });

  it("rejects identity material reached through a symlinked directory", async () => {
    const parent = await temporaryDirectory();
    const root = await openRunnerDataRoot(join(parent, "runner"));
    const outside = join(parent, "outside");
    await mkdir(outside, { mode: 0o700 });
    await writeFile(join(outside, "ed25519-private.pk8"), randomBytes(48), { mode: 0o600 });
    await symlink(outside, runnerPath(root, "volume-identity"));

    await expect(loadOrCreateRunnerVolumeIdentity(root))
      .rejects.toMatchObject({ code: "unsafe_entry" });
  });

  it.runIf(process.platform !== "win32")(
    "rejects hardlinked private identity material",
    async () => {
      const parent = await temporaryDirectory();
      const root = await openRunnerDataRoot(join(parent, "runner"));
      await loadOrCreateRunnerVolumeIdentity(root);
      const identityPath = runnerPath(root, "volume-identity", "ed25519-private.pk8");
      const outsideLink = join(parent, "identity-copy.pk8");
      await link(identityPath, outsideLink);

      await expect(loadOrCreateRunnerVolumeIdentity(root))
        .rejects.toMatchObject({ code: "unsafe_entry" });
      expect((await lstat(outsideLink)).nlink).toBe(2);
    },
  );
});

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-volume-identity-"));
  temporaryDirectories.push(path);
  return path;
}

async function freshRoot() {
  const parent = await temporaryDirectory();
  return openRunnerDataRoot(join(parent, "runner"));
}
