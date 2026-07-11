import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { parseProfileKey, profilePaths, restoreProfile, sealProfile } from "../src/profile-store.js";

describe("encrypted cloud browser profiles", () => {
  it("round-trips a profile without leaving plaintext at rest", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-profile-"));
    const paths = profilePaths(root, "account-1", "identity-1");
    const key = parseProfileKey(Buffer.alloc(32, 7).toString("base64"));
    await restoreProfile(paths, key);
    await writeFile(join(paths.directory, "Cookies"), "session-cookie");
    await sealProfile(paths, key);
    await expect(readFile(join(paths.directory, "Cookies"))).rejects.toThrow();
    expect((await readFile(paths.encryptedSnapshot)).includes(Buffer.from("session-cookie"))).toBe(false);
    await restoreProfile(paths, key);
    expect(await readFile(join(paths.directory, "Cookies"), "utf8")).toBe("session-cookie");
    await rm(root, { recursive: true, force: true });
  });

  it("uses a different scope for every application email", () => {
    expect(profilePaths("/profiles", "account", "email-a").scope)
      .not.toBe(profilePaths("/profiles", "account", "email-b").scope);
  });
});
