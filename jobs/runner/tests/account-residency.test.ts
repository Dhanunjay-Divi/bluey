import { randomBytes } from "node:crypto";
import { lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  AccountResidencyIndex,
  accountPurgeSubjectHash,
} from "../src/account-residency.js";
import { openRunnerDataRoot } from "../src/safe-runner-storage.js";
import { loadOrCreateRunnerVolumeIdentity } from "../src/volume-identity.js";

const PROFILE_SCOPE = "a".repeat(40);
const RESULT_SCOPE = "b".repeat(64);
const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("runner account residency index", () => {
  it("writes immutable signed profile and result locators without the raw subject", async () => {
    const { root, index } = await freshIndex();
    const subject = randomBytes(32).toString("base64url");
    const subjectSha256 = accountPurgeSubjectHash(subject);

    const [profile, result] = await Promise.all([
      index.registerProfile(subject, PROFILE_SCOPE),
      index.registerResult(subject, RESULT_SCOPE),
    ]);

    expect(profile).toMatchObject({
      subjectSha256,
      kind: "profile",
      scope: PROFILE_SCOPE,
      artifactFamilies: [
        "active",
        "snapshots",
        "run-checkpoints",
        "receipts",
        "temporary",
      ],
    });
    expect(result).toMatchObject({
      subjectSha256,
      kind: "result",
      scope: RESULT_SCOPE,
      artifactFamilies: ["step-results", "temporary"],
    });
    expect(await index.locatorsForSubjectHash(subjectSha256)).toEqual([profile, result]);
    expect(await index.allLocators()).toEqual([profile, result]);

    const diskBytes = await allRegularFileBytes(root.path);
    expect(diskBytes.includes(subject)).toBe(false);
    expect((await allRelativePaths(root.path)).join("\n")).not.toContain(subject);
    if (process.platform !== "win32") {
      for (const path of await allRegularFilePaths(root.path)) {
        expect((await lstat(path)).mode & 0o777).toBe(0o600);
        expect((await lstat(path)).nlink).toBe(1);
      }
    }
  });

  it("accepts only exact immutable replay and rejects a tampered locator", async () => {
    const { root, index } = await freshIndex();
    const subject = randomBytes(32).toString("base64url");
    const subjectSha256 = accountPurgeSubjectHash(subject);
    const first = await index.registerProfile(subject, PROFILE_SCOPE);
    await expect(index.registerProfile(subject, PROFILE_SCOPE)).resolves.toEqual(first);

    const locatorPath = join(
      root.path,
      "account-residency-v1",
      "locators",
      subjectSha256,
      `profile-${PROFILE_SCOPE}.json`,
    );
    const parsed = JSON.parse(await readFile(locatorPath, "utf8")) as Record<string, unknown>;
    parsed.artifactFamilies = ["active"];
    await writeFile(locatorPath, `${JSON.stringify(parsed)}\n`, { mode: 0o600 });

    await expect(index.locatorsForSubjectHash(subjectSha256))
      .rejects.toMatchObject({ code: "corrupt_locator" });
    await expect(index.registerProfile(subject, PROFILE_SCOPE))
      .rejects.toMatchObject({ code: "locator_conflict" });
  });

  it("blocks new locators once a subject fence exists", async () => {
    const { index } = await freshIndex();
    const subject = randomBytes(32).toString("base64url");
    const subjectSha256 = accountPurgeSubjectHash(subject);
    await index.ensurePurgeDirectories();
    await writeFile(index.fencePath(subjectSha256), "{}\n", { mode: 0o600 });

    await expect(index.registerProfile(subject, PROFILE_SCOPE))
      .rejects.toMatchObject({ code: "purged_subject" });
  });

  it("isolates subjects and validates fixed hashed scopes", async () => {
    const { index } = await freshIndex();
    const subjectA = randomBytes(32).toString("base64url");
    const subjectB = randomBytes(32).toString("base64url");
    await index.registerProfile(subjectA, PROFILE_SCOPE);
    await index.registerResult(subjectB, RESULT_SCOPE);

    expect(await index.locatorsForSubjectHash(accountPurgeSubjectHash(subjectA)))
      .toMatchObject([{ kind: "profile", scope: PROFILE_SCOPE }]);
    expect(await index.locatorsForSubjectHash(accountPurgeSubjectHash(subjectB)))
      .toMatchObject([{ kind: "result", scope: RESULT_SCOPE }]);
    await expect(index.registerProfile(subjectA, "raw-account-id"))
      .rejects.toMatchObject({ code: "invalid_scope" });
    await expect(index.registerResult("not-base64url", RESULT_SCOPE))
      .rejects.toMatchObject({ code: "invalid_subject" });
  });

  it("rejects an unclassified subject directory during complete enumeration", async () => {
    const { root, index } = await freshIndex();
    await mkdir(join(root.path, "account-residency-v1", "locators", "raw-account-id"), {
      recursive: true,
      mode: 0o700,
    });

    await expect(index.allLocators()).rejects.toMatchObject({ code: "corrupt_locator" });
  });
});

async function freshIndex() {
  const parent = await temporaryDirectory();
  const root = await openRunnerDataRoot(join(parent, "runner"));
  const identity = await loadOrCreateRunnerVolumeIdentity(root);
  return { root, index: new AccountResidencyIndex(root, identity) };
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-account-residency-"));
  temporaryDirectories.push(path);
  return path;
}

async function allRegularFilePaths(root: string): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) found.push(...await allRegularFilePaths(path));
    else if (entry.isFile()) found.push(path);
  }
  return found.sort((left, right) => left.localeCompare(right));
}

async function allRelativePaths(root: string): Promise<string[]> {
  return (await allRegularFilePaths(root)).map((path) => path.slice(root.length + 1));
}

async function allRegularFileBytes(root: string): Promise<string> {
  const buffers = await Promise.all((await allRegularFilePaths(root)).map((path) => readFile(path)));
  return Buffer.concat(buffers).toString("utf8");
}
