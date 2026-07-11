import { createHash } from "node:crypto";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { decryptFile, encryptFile } from "./profile-store.js";

export function resultPath(root: string, requestId: string): string {
  const digest = createHash("sha256").update(requestId).digest("hex");
  return join(root, "step-results", `${digest}.json.enc`);
}

export async function readResult<T>(root: string, requestId: string, key: Buffer): Promise<T | undefined> {
  const encrypted = resultPath(root, requestId);
  const temporary = `${encrypted}.${process.pid}.${Date.now()}.read.json`;
  try {
    await decryptFile(encrypted, temporary, key);
    return JSON.parse(await readFile(temporary, "utf8")) as T;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  } finally {
    await rm(temporary, { force: true });
  }
}

export async function writeResult(root: string, requestId: string, result: unknown, key: Buffer): Promise<void> {
  const path = resultPath(root, requestId);
  const plaintext = `${path}.${process.pid}.${Date.now()}.write.json`;
  const encrypted = `${path}.${process.pid}.${Date.now()}.tmp`;
  await mkdir(join(root, "step-results"), { recursive: true });
  await writeFile(plaintext, `${JSON.stringify(result)}\n`, { mode: 0o600 });
  try {
    await encryptFile(plaintext, encrypted, key);
    await rename(encrypted, path);
  } finally {
    await rm(plaintext, { force: true });
    await rm(encrypted, { force: true });
  }
}
