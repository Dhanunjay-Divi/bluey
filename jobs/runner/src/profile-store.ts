import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { chmod, mkdir, rm, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { pipeline } from "node:stream/promises";
import * as tar from "tar";
import { decryptFile, encryptFile } from "./crypto-envelope.js";

export { decryptFile, encryptFile } from "./crypto-envelope.js";

export interface ProfilePaths {
  scope: string;
  directory: string;
  encryptedSnapshot: string;
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

function profileEncryptionContext(paths: ProfilePaths) {
  return { purpose: "profile-snapshot", scope: paths.scope } as const;
}
