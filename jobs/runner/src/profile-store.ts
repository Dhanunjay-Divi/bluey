import { createCipheriv, createDecipheriv, createHash, randomBytes } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { appendFile, mkdir, open, rm, stat, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { pipeline } from "node:stream/promises";
import * as tar from "tar";

const MAGIC = Buffer.from("BLUEYJP1");
const IV_BYTES = 12;
const TAG_BYTES = 16;

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
  await mkdir(paths.directory, { recursive: true });
  try {
    await stat(paths.encryptedSnapshot);
  } catch {
    return;
  }
  const archive = `${paths.directory}.restore.tar.gz`;
  await decryptFile(paths.encryptedSnapshot, archive, key);
  await tar.x({ cwd: paths.directory, file: archive, gzip: true, preservePaths: false });
  await rm(archive, { force: true });
}

export async function sealProfile(paths: ProfilePaths, key: Buffer): Promise<void> {
  const archive = `${paths.directory}.seal.tar.gz`;
  await mkdir(dirname(paths.encryptedSnapshot), { recursive: true });
  await tar.c({ cwd: paths.directory, file: archive, gzip: true, portable: true }, ["."]);
  await encryptFile(archive, paths.encryptedSnapshot, key);
  await rm(archive, { force: true });
  await rm(paths.directory, { recursive: true, force: true });
}

export async function encryptFile(source: string, destination: string, key: Buffer): Promise<void> {
  const iv = randomBytes(IV_BYTES);
  const cipher = createCipheriv("aes-256-gcm", key, iv);
  await mkdir(destination.slice(0, destination.lastIndexOf("/")), { recursive: true });
  await writeFile(destination, Buffer.concat([MAGIC, iv]), { mode: 0o600 });
  await pipeline(createReadStream(source), cipher, createWriteStream(destination, { flags: "a", mode: 0o600 }));
  await appendFile(destination, cipher.getAuthTag());
}

export async function decryptFile(source: string, destination: string, key: Buffer): Promise<void> {
  const metadata = await stat(source);
  const header = await readRange(source, 0, MAGIC.length + IV_BYTES - 1);
  if (!header.subarray(0, MAGIC.length).equals(MAGIC)) throw new Error("Invalid Bluey browser profile snapshot");
  const iv = header.subarray(MAGIC.length);
  const tag = await readRange(source, metadata.size - TAG_BYTES, metadata.size - 1);
  const decipher = createDecipheriv("aes-256-gcm", key, iv);
  decipher.setAuthTag(tag);
  await pipeline(
    createReadStream(source, { start: MAGIC.length + IV_BYTES, end: metadata.size - TAG_BYTES - 1 }),
    decipher,
    createWriteStream(destination, { mode: 0o600 }),
  );
}

async function readRange(path: string, start: number, end: number): Promise<Buffer> {
  const handle = await open(path, "r");
  try {
    const contents = Buffer.alloc(end - start + 1);
    await handle.read(contents, 0, contents.length, start);
    return contents;
  } finally {
    await handle.close();
  }
}
