import { createHash, randomBytes } from "node:crypto";
import { createWriteStream } from "node:fs";
import { chmod, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { pipeline } from "node:stream/promises";
import * as tar from "tar";
import { decryptFile, encryptFile, replaceFileDurably } from "./crypto-envelope.js";

export { decryptFile, encryptFile } from "./crypto-envelope.js";

export interface ProfilePaths {
  scope: string;
  directory: string;
  encryptedSnapshot: string;
  snapshotGeneration: string;
}

export interface EncryptedProfileSnapshot {
  bytes: Buffer;
  generation: number;
  envelopeVersion: 2;
}

export function profilePaths(root: string, accountId: string, applicationIdentityId: string): ProfilePaths {
  const scope = createHash("sha256")
    .update(`${accountId}\0${applicationIdentityId}`)
    .digest("hex")
    .slice(0, 40);
  return profilePathsFromScope(root, scope);
}

export function profilePathsFromScope(root: string, scope: string): ProfilePaths {
  if (!/^[a-f0-9]{40}$/.test(scope)) throw new Error("Invalid browser profile scope");
  return {
    scope,
    directory: join(root, "active", scope),
    encryptedSnapshot: join(root, "snapshots", `${scope}.tar.gz.enc`),
    snapshotGeneration: join(root, "snapshots", `${scope}.generation`),
  };
}

export function parseProfileKey(encoded: string): Buffer {
  const key = Buffer.from(encoded, "base64");
  if (key.length !== 32) throw new Error("BLUEY_JOBS_PROFILE_ENCRYPTION_KEY must be a base64 32-byte key");
  return key;
}

export async function restoreProfile(paths: ProfilePaths, key: Buffer): Promise<void> {
  await rm(paths.directory, { recursive: true, force: true });
  await mkdir(paths.directory, { recursive: true, mode: 0o700 });
  await chmod(paths.directory, 0o700);
  try {
    await stat(paths.encryptedSnapshot);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
    await rm(paths.directory, { recursive: true, force: true });
    throw error;
  }
  const archive = `${paths.directory}.restore.tar.gz`;
  try {
    await decryptFile(paths.encryptedSnapshot, archive, key, profileEncryptionContext(paths));
    await tar.x({ cwd: paths.directory, file: archive, gzip: true, preservePaths: false });
  } catch (error) {
    await rm(paths.directory, { recursive: true, force: true });
    throw error;
  } finally {
    await rm(archive, { force: true });
  }
}

export async function sealProfile(paths: ProfilePaths, key: Buffer): Promise<void> {
  const archive = `${paths.directory}.seal.tar.gz`;
  await mkdir(dirname(paths.encryptedSnapshot), { recursive: true, mode: 0o700 });
  await chmod(dirname(paths.encryptedSnapshot), 0o700);
  await rm(archive, { force: true });
  try {
    await pipeline(
      tar.c({ cwd: paths.directory, gzip: true, portable: true }, ["."]),
      createWriteStream(archive, { flags: "wx", mode: 0o600 }),
    );
    await encryptFile(archive, paths.encryptedSnapshot, key, profileEncryptionContext(paths));
  } finally {
    await rm(archive, { force: true });
  }
  await rm(paths.directory, { recursive: true, force: true });
}

export async function readEncryptedProfileSnapshot(
  paths: ProfilePaths,
): Promise<EncryptedProfileSnapshot | undefined> {
  let bytes: Buffer;
  try {
    bytes = await readFile(paths.encryptedSnapshot);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
  if (bytes.length < 8 || bytes.subarray(0, 8).toString("ascii") !== "BLUEYJP2") {
    throw new Error("Invalid encrypted browser profile snapshot");
  }
  return {
    bytes,
    generation: await readProfileSnapshotGeneration(paths),
    envelopeVersion: 2,
  };
}

export async function installEncryptedProfileSnapshot(
  paths: ProfilePaths,
  snapshot: EncryptedProfileSnapshot,
): Promise<void> {
  if (!Number.isSafeInteger(snapshot.generation) || snapshot.generation <= 0) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  if (snapshot.envelopeVersion !== 2
    || snapshot.bytes.length < 8
    || snapshot.bytes.subarray(0, 8).toString("ascii") !== "BLUEYJP2") {
    throw new Error("Invalid encrypted browser profile snapshot");
  }
  const existing = await readEncryptedProfileSnapshot(paths);
  if (existing && existing.generation > snapshot.generation) {
    throw new Error("Refusing to replace a newer browser profile snapshot");
  }
  if (existing && existing.generation === snapshot.generation) {
    if (!existing.bytes.equals(snapshot.bytes)) {
      throw new Error("Browser profile snapshot generation conflict");
    }
    return;
  }

  const snapshotsDirectory = dirname(paths.encryptedSnapshot);
  await mkdir(snapshotsDirectory, { recursive: true, mode: 0o700 });
  await chmod(snapshotsDirectory, 0o700);
  const suffix = `${process.pid}-${randomBytes(8).toString("hex")}`;
  const snapshotStaging = `${paths.encryptedSnapshot}.${suffix}.incoming`;
  const generationStaging = `${paths.snapshotGeneration}.${suffix}.incoming`;
  try {
    await writeFile(snapshotStaging, snapshot.bytes, { flag: "wx", mode: 0o600 });
    await writeFile(generationStaging, `${snapshot.generation}\n`, { flag: "wx", mode: 0o600 });
    await replaceFileDurably(snapshotStaging, paths.encryptedSnapshot);
    await replaceFileDurably(generationStaging, paths.snapshotGeneration);
  } finally {
    await rm(snapshotStaging, { force: true });
    await rm(generationStaging, { force: true });
  }
}

export async function writeProfileSnapshotGeneration(
  paths: ProfilePaths,
  generation: number,
): Promise<void> {
  if (!Number.isSafeInteger(generation) || generation <= 0) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  const snapshotsDirectory = dirname(paths.snapshotGeneration);
  await mkdir(snapshotsDirectory, { recursive: true, mode: 0o700 });
  await chmod(snapshotsDirectory, 0o700);
  const staging = `${paths.snapshotGeneration}.${process.pid}-${randomBytes(8).toString("hex")}.incoming`;
  try {
    await writeFile(staging, `${generation}\n`, { flag: "wx", mode: 0o600 });
    await replaceFileDurably(staging, paths.snapshotGeneration);
  } finally {
    await rm(staging, { force: true });
  }
}

async function readProfileSnapshotGeneration(paths: ProfilePaths): Promise<number> {
  let value: string;
  try {
    value = (await readFile(paths.snapshotGeneration, "utf8")).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return 0;
    throw error;
  }
  if (!/^[0-9]{1,16}$/.test(value)) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  const generation = Number(value);
  if (!Number.isSafeInteger(generation) || generation <= 0) {
    throw new Error("Invalid browser profile snapshot generation");
  }
  return generation;
}

function profileEncryptionContext(paths: ProfilePaths) {
  return { purpose: "profile-snapshot", scope: paths.scope } as const;
}
