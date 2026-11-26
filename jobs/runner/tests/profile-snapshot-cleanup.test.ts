import { randomBytes } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { persistBrowserProfileSnapshot } from "../src/server.js";
import {
  profilePaths,
  readEncryptedProfileSnapshot,
  restoreProfile,
} from "../src/profile-store.js";

describe("browser profile snapshot cleanup", () => {
  it("retains the sealed local snapshot until the remote commit is confirmed", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-profile-cleanup-"));
    const paths = profilePaths(root, "account-123", "identity-123");
    const key = randomBytes(32);
    await restoreProfile(paths, key);
    await writeFile(join(paths.directory, "Cookies"), "durable-profile-state", { mode: 0o600 });
    const store = vi.fn(async () => {
      throw new Error("remote commit response remained ambiguous");
    });

    try {
      await expect(persistBrowserProfileSnapshot(paths, key, {
        accountId: "account-123",
        applicationId: "application-123",
        runId: "run-123",
        browserProfileId: "account-123:identity-123",
        leaseToken: "lease-token",
        fence: 7,
      }, { store })).rejects.toThrow("remote commit response remained ambiguous");

      const retained = await readEncryptedProfileSnapshot(paths);
      expect(retained).toBeDefined();
      expect(retained?.bytes.subarray(0, 8).toString("ascii")).toBe("BLUEYJP2");
      expect(retained?.generation).toBe(0);
      expect(store).toHaveBeenCalledWith(expect.any(Object), expect.objectContaining({
        generation: 0,
        envelopeVersion: 2,
      }));
      await expect(readFile(paths.encryptedSnapshot)).resolves.toEqual(retained?.bytes);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
