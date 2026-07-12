import { createCipheriv, createDecipheriv, hkdfSync, randomBytes } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { appendFile, mkdir, open, rename, rm, stat, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { pipeline } from "node:stream/promises";

const LEGACY_MAGIC = Buffer.from("BLUEYJP1");
const MAGIC = Buffer.from("BLUEYJP2");
const MAGIC_PREFIX = Buffer.from("BLUEYJP");
const IV_BYTES = 12;
const TAG_BYTES = 16;
const KEY_BYTES = 32;
const FILE_MODE = 0o600;
const HKDF_SALT = Buffer.from("bluey-jobs-runner:BLUEYJP2:hkdf-sha256", "utf8");
const VALID_SCOPE = /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/;

export type RunnerEncryptionPurpose = "profile-snapshot" | "durable-result";

export type RunnerEncryptionContext =
  | { purpose: "profile-snapshot"; scope: string }
  | { purpose: "durable-result"; scope: string; requestScope: string };

export type RunnerEncryptionErrorCode =
  | "authentication_failed"
  | "configuration"
  | "invalid_envelope"
  | "legacy_envelope"
  | "unsupported_envelope_version";

export class RunnerEncryptionError extends Error {
  constructor(readonly code: RunnerEncryptionErrorCode) {
    super({
      authentication_failed: "Runner encrypted data failed authentication.",
      configuration: "Runner encryption configuration is invalid.",
      invalid_envelope: "Runner encrypted data has an invalid envelope.",
      legacy_envelope: "Legacy BLUEYJP1 runner encrypted data is not supported.",
      unsupported_envelope_version: "Runner encrypted data uses an unsupported envelope version.",
    }[code]);
    this.name = "RunnerEncryptionError";
  }
}

export async function encryptFile(
  source: string,
  destination: string,
  masterKey: Buffer,
  context: RunnerEncryptionContext,
): Promise<void> {
  const aad = authenticatedContext(context);
  const encryptionKey = deriveEncryptionKey(masterKey, aad);
  const iv = randomBytes(IV_BYTES);
  const cipher = createCipheriv("aes-256-gcm", encryptionKey, iv);
  cipher.setAAD(aad);

  await mkdir(dirname(destination), { recursive: true });
  const staging = stagingPath(destination, "seal");
  try {
    await writeFile(staging, Buffer.concat([MAGIC, iv]), { flag: "wx", mode: FILE_MODE });
    await pipeline(
      createReadStream(source),
      cipher,
      createWriteStream(staging, { flags: "a", mode: FILE_MODE }),
    );
    await appendFile(staging, cipher.getAuthTag());
    await replaceFileDurably(staging, destination);
  } catch (error) {
    await rm(staging, { force: true });
    throw error;
  } finally {
    encryptionKey.fill(0);
  }
}

export async function replaceFileDurably(staging: string, destination: string): Promise<void> {
  await syncFile(staging);
  await rename(staging, destination);
  await syncDirectory(dirname(destination));
}

export async function decryptFile(
  source: string,
  destination: string,
  masterKey: Buffer,
  context: RunnerEncryptionContext,
): Promise<void> {
  const aad = authenticatedContext(context);
  const metadata = await stat(source);
  if (!Number.isSafeInteger(metadata.size) || metadata.size < MAGIC.length) {
    throw new RunnerEncryptionError("invalid_envelope");
  }

  const magic = await readExactly(source, 0, MAGIC.length);
  if (magic.equals(LEGACY_MAGIC)) throw new RunnerEncryptionError("legacy_envelope");
  if (!magic.equals(MAGIC)) {
    if (magic.subarray(0, MAGIC_PREFIX.length).equals(MAGIC_PREFIX)) {
      throw new RunnerEncryptionError("unsupported_envelope_version");
    }
    throw new RunnerEncryptionError("invalid_envelope");
  }
  if (metadata.size < MAGIC.length + IV_BYTES + TAG_BYTES) {
    throw new RunnerEncryptionError("invalid_envelope");
  }

  const iv = await readExactly(source, MAGIC.length, IV_BYTES);
  const tag = await readExactly(source, metadata.size - TAG_BYTES, TAG_BYTES);
  const ciphertextStart = MAGIC.length + IV_BYTES;
  const ciphertextBytes = metadata.size - ciphertextStart - TAG_BYTES;
  const encryptionKey = deriveEncryptionKey(masterKey, aad);
  const decipher = createDecipheriv("aes-256-gcm", encryptionKey, iv);
  decipher.setAAD(aad);
  decipher.setAuthTag(tag);

  await mkdir(dirname(destination), { recursive: true });
  const staging = stagingPath(destination, "open");
  try {
    if (ciphertextBytes === 0) {
      await writeFile(staging, decipher.final(), { flag: "wx", mode: FILE_MODE });
    } else {
      await pipeline(
        createReadStream(source, {
          start: ciphertextStart,
          end: metadata.size - TAG_BYTES - 1,
        }),
        decipher,
        createWriteStream(staging, { flags: "wx", mode: FILE_MODE }),
      );
    }
  } catch {
    await rm(staging, { force: true });
    throw new RunnerEncryptionError("authentication_failed");
  } finally {
    encryptionKey.fill(0);
  }

  try {
    await rename(staging, destination);
  } catch (error) {
    await rm(staging, { force: true });
    throw error;
  }
}

function authenticatedContext(context: RunnerEncryptionContext): Buffer {
  if (!context || typeof context !== "object" || !VALID_SCOPE.test(context.scope)) {
    throw new RunnerEncryptionError("configuration");
  }
  if (context.purpose === "profile-snapshot") {
    return Buffer.from(
      `bluey-jobs-runner\0BLUEYJP2\0aes-256-gcm\0profile-snapshot\0profile-scope\0${context.scope}`,
      "utf8",
    );
  }
  if (context.purpose === "durable-result" && /^[a-f0-9]{64}$/.test(context.requestScope)) {
    return Buffer.from(
      `bluey-jobs-runner\0BLUEYJP2\0aes-256-gcm\0durable-result\0profile-scope\0${context.scope}`
        + `\0request-scope\0${context.requestScope}`,
      "utf8",
    );
  }
  throw new RunnerEncryptionError("configuration");
}

function deriveEncryptionKey(masterKey: Buffer, aad: Buffer): Buffer {
  if (!Buffer.isBuffer(masterKey) || masterKey.length !== KEY_BYTES) {
    throw new RunnerEncryptionError("configuration");
  }
  return Buffer.from(hkdfSync("sha256", masterKey, HKDF_SALT, aad, KEY_BYTES));
}

function stagingPath(destination: string, operation: "open" | "seal"): string {
  return `${destination}.${process.pid}.${randomBytes(8).toString("hex")}.${operation}`;
}

async function readExactly(path: string, position: number, length: number): Promise<Buffer> {
  const handle = await open(path, "r");
  try {
    const contents = Buffer.alloc(length);
    let offset = 0;
    while (offset < length) {
      const { bytesRead } = await handle.read(contents, offset, length - offset, position + offset);
      if (bytesRead === 0) throw new RunnerEncryptionError("invalid_envelope");
      offset += bytesRead;
    }
    return contents;
  } finally {
    await handle.close();
  }
}

async function syncFile(path: string): Promise<void> {
  const handle = await open(path, "r+");
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function syncDirectory(path: string): Promise<void> {
  let handle;
  try {
    handle = await open(path, "r");
    await handle.sync();
  } catch (error) {
    if (!directorySyncUnsupported(error)) throw error;
  } finally {
    await handle?.close();
  }
}

function directorySyncUnsupported(error: unknown): boolean {
  const code = (error as NodeJS.ErrnoException).code;
  return code === "EBADF"
    || code === "EINVAL"
    || code === "EISDIR"
    || code === "ENOSYS"
    || code === "ENOTSUP"
    || code === "EPERM";
}
