import { copyFile, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  installEncryptedProfileSnapshot,
  parseProfileKey,
  profilePaths,
  readEncryptedProfileSnapshot,
  restoreProfile,
  sealProfile,
  writeProfileSnapshotGeneration,
} from "../src/profile-store.js";

describe("encrypted cloud browser profiles", () => {
  it("round-trips a profile without leaving plaintext at rest", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-profile-"));
    const paths = profilePaths(root, "account-1", "identity-1");
    const key = parseProfileKey(Buffer.alloc(32, 7).toString("base64"));
    await restoreProfile(paths, key);
    await writeFile(join(paths.directory, "Cookies"), "session-cookie");
    await sealProfile(paths, key);
    await expect(readFile(join(paths.directory, "Cookies"))).rejects.toThrow();
    const snapshot = await readFile(paths.encryptedSnapshot);
    expect(snapshot.subarray(0, 8).toString("ascii")).toBe("BLUEYJP2");
    expect(snapshot.includes(Buffer.from("session-cookie"))).toBe(false);
    if (process.platform !== "win32") {
      expect((await stat(paths.encryptedSnapshot)).mode & 0o777).toBe(0o600);
    }
    await restoreProfile(paths, key);
    expect(await readFile(join(paths.directory, "Cookies"), "utf8")).toBe("session-cookie");
    await rm(root, { recursive: true, force: true });
  });

  it("uses a different scope for every application email", () => {
    expect(profilePaths("/profiles", "account", "email-a").scope)
      .not.toBe(profilePaths("/profiles", "account", "email-b").scope);
  });

  it("rejects a snapshot moved to another tenant/profile scope", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-profile-swap-"));
    const key = parseProfileKey(Buffer.alloc(32, 9).toString("base64"));
    const source = profilePaths(root, "account-a", "identity-a");
    const target = profilePaths(root, "account-b", "identity-b");
    await restoreProfile(source, key);
    await writeFile(join(source.directory, "Cookies"), "scoped-session");
    await sealProfile(source, key);

    await copyFile(source.encryptedSnapshot, target.encryptedSnapshot);

    await expect(restoreProfile(target, key)).rejects.toMatchObject({ code: "authentication_failed" });
    await expect(readFile(join(target.directory, "Cookies"))).rejects.toThrow();
    await rm(root, { recursive: true, force: true });
  });

  it("restores a profile on a replacement runner from an encrypted snapshot", async () => {
    const sourceRoot = await mkdtemp(join(tmpdir(), "bluey-jobs-profile-source-"));
    const replacementRoot = await mkdtemp(join(tmpdir(), "bluey-jobs-profile-replacement-"));
    const key = parseProfileKey(Buffer.alloc(32, 11).toString("base64"));
    const source = profilePaths(sourceRoot, "account-1", "identity-1");
    const replacement = profilePaths(replacementRoot, "account-1", "identity-1");

    await restoreProfile(source, key);
    await writeFile(join(source.directory, "Cookies"), "replacement-runner-session");
    await sealProfile(source, key);
    await writeProfileSnapshotGeneration(source, 4);
    const snapshot = await readEncryptedProfileSnapshot(source);
    expect(snapshot?.generation).toBe(4);

    await installEncryptedProfileSnapshot(replacement, snapshot!);
    await restoreProfile(replacement, key);
    expect(await readFile(join(replacement.directory, "Cookies"), "utf8"))
      .toBe("replacement-runner-session");

    await rm(sourceRoot, { recursive: true, force: true });
    await rm(replacementRoot, { recursive: true, force: true });
  });

  it("refuses an older remote snapshot and conflicting bytes at one generation", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-profile-generation-"));
    const key = parseProfileKey(Buffer.alloc(32, 13).toString("base64"));
    const paths = profilePaths(root, "account-1", "identity-1");
    await restoreProfile(paths, key);
    await writeFile(join(paths.directory, "Cookies"), "newest-session");
    await sealProfile(paths, key);
    await writeProfileSnapshotGeneration(paths, 7);
    const newest = await readEncryptedProfileSnapshot(paths);
    expect(newest).toBeDefined();

    await expect(installEncryptedProfileSnapshot(paths, {
      ...newest!,
      generation: 6,
    })).rejects.toThrow("newer browser profile snapshot");

    await expect(installEncryptedProfileSnapshot(paths, {
      bytes: Buffer.from(newest!.bytes.map((byte, index) => index === 12 ? byte ^ 1 : byte)),
      generation: 7,
      envelopeVersion: 2,
    })).rejects.toThrow("snapshot generation conflict");

    await rm(root, { recursive: true, force: true });
  });
});
