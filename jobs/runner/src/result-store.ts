import { createHash, randomBytes } from "node:crypto";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  decryptBytes,
  decryptFile,
  encryptBytes,
  encryptFile,
  replaceFileDurably,
} from "./crypto-envelope.js";
import type { ManagedResultStorage } from "./subject-storage-manager.js";

const RESULT_ENVELOPE_VERSION = 1;
const VALID_PROFILE_SCOPE = /^[a-f0-9]{40}$/;
const MANAGED_RESULT_FILE = "step-result.json.enc";
const MAXIMUM_MANAGED_RESULT_BYTES = 128 * 1024 * 1024;

export interface DurableResultContext {
  requestId: string;
  profileScope: string;
}

interface StoredResultEnvelope {
  __blueyResultEnvelope: typeof RESULT_ENVELOPE_VERSION;
  committed: boolean;
  result: unknown;
}

export interface DurableResultState<T> {
  state: "staged" | "committed";
  result: T;
  resultSha256: string;
}

export type ResultStoreErrorCode =
  | "invalid_result_envelope"
  | "invalid_result_scope"
  | "result_promotion_conflict"
  | "unsupported_result_envelope_version";

export class ResultStoreError extends Error {
  constructor(readonly code: ResultStoreErrorCode) {
    super({
      invalid_result_envelope: "Stored runner result has an invalid envelope.",
      invalid_result_scope: "Stored runner result scope is invalid.",
      result_promotion_conflict: "Stored runner result does not match the recoverable submission.",
      unsupported_result_envelope_version: "Stored runner result uses an unsupported envelope version.",
    }[code]);
    this.name = "ResultStoreError";
  }
}

export function resultPath(root: string, context: DurableResultContext): string {
  return join(root, "step-results", `${durableResultScope(context)}.json.enc`);
}

export async function readResult<T>(
  root: string,
  context: DurableResultContext,
  key: Buffer,
): Promise<T | undefined> {
  const stored = await readResultState<T>(root, context, key);
  return stored?.state === "committed" ? stored.result : undefined;
}

export async function readResultState<T>(
  root: string,
  context: DurableResultContext,
  key: Buffer,
): Promise<DurableResultState<T> | undefined> {
  const encrypted = resultPath(root, context);
  const temporary = `${encrypted}.${temporarySuffix()}.read.json`;
  try {
    await decryptFile(encrypted, temporary, key, resultEncryptionContext(context));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
  try {
    const stored = parseResultEnvelope(await readFile(temporary, "utf8"));
    return {
      state: stored.committed ? "committed" : "staged",
      result: stored.result as T,
      resultSha256: resultSha256(stored.result),
    };
  } finally {
    await rm(temporary, { force: true });
  }
}

export async function readManagedResult<T>(
  storage: ManagedResultStorage,
  context: DurableResultContext,
  key: Buffer,
): Promise<T | undefined> {
  const stored = await readManagedResultState<T>(storage, context, key);
  return stored?.state === "committed" ? stored.result : undefined;
}

export async function readManagedResultState<T>(
  storage: ManagedResultStorage,
  context: DurableResultContext,
  key: Buffer,
): Promise<DurableResultState<T> | undefined> {
  assertManagedResultBinding(storage, context);
  const inventory = await storage.root.inventory();
  const entry = inventory.entries.find((candidate) => (
    candidate.relativePath === MANAGED_RESULT_FILE
  ));
  if (!entry) return undefined;
  if (entry.kind !== "file"
    || entry.linkCount !== 1
    || entry.deviceId !== storage.root.deviceId
    || entry.sizeBytes < 1
    || entry.sizeBytes > MAXIMUM_MANAGED_RESULT_BYTES) {
    throw new ResultStoreError("invalid_result_envelope");
  }
  const encrypted = await storage.root.readFileBounded(
    MANAGED_RESULT_FILE,
    MAXIMUM_MANAGED_RESULT_BYTES,
  );
  if (encrypted.length !== entry.sizeBytes
    || createHash("sha256").update(encrypted).digest("hex") !== entry.sha256) {
    encrypted.fill(0);
    throw new ResultStoreError("invalid_result_envelope");
  }
  let plaintext: Buffer | undefined;
  try {
    plaintext = decryptBytes(encrypted, key, resultEncryptionContext(context));
    const stored = parseResultEnvelope(plaintext.toString("utf8"));
    return {
      state: stored.committed ? "committed" : "staged",
      result: stored.result as T,
      resultSha256: resultSha256(stored.result),
    };
  } finally {
    plaintext?.fill(0);
    encrypted.fill(0);
  }
}

export async function promoteStagedResult<T>(
  root: string,
  context: DurableResultContext,
  expectedResultSha256: string,
  key: Buffer,
): Promise<T> {
  if (!/^[a-f0-9]{64}$/.test(expectedResultSha256)) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const stored = await readResultState<T>(root, context, key);
  if (!stored || stored.resultSha256 !== expectedResultSha256) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  if (stored.state === "staged") {
    await writeStoredResult(root, context, stored.result, key, true);
  }
  return stored.result;
}

export async function promoteManagedStagedResult<T>(
  storage: ManagedResultStorage,
  context: DurableResultContext,
  expectedResultSha256: string,
  key: Buffer,
): Promise<T> {
  if (!/^[a-f0-9]{64}$/.test(expectedResultSha256)) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const stored = await readManagedResultState<T>(storage, context, key);
  if (!stored || stored.resultSha256 !== expectedResultSha256) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  if (stored.state === "staged") {
    await writeManagedStoredResult(storage, context, stored.result, key, true);
  }
  return stored.result;
}

export async function writeResult(
  root: string,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
): Promise<void> {
  await writeStoredResult(root, context, result, key, true);
}

export async function writeManagedResult(
  storage: ManagedResultStorage,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
): Promise<void> {
  await writeManagedStoredResult(storage, context, result, key, true);
}

export async function stageResult(
  root: string,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
): Promise<void> {
  await writeStoredResult(root, context, result, key, false);
}

export async function stageManagedResult(
  storage: ManagedResultStorage,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
): Promise<void> {
  await writeManagedStoredResult(storage, context, result, key, false);
}

async function writeManagedStoredResult(
  storage: ManagedResultStorage,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
  committed: boolean,
): Promise<void> {
  assertManagedResultBinding(storage, context);
  const plaintext = serializeResultEnvelope(result, committed);
  if (plaintext.length > MAXIMUM_MANAGED_RESULT_BYTES - 64) {
    plaintext.fill(0);
    throw new ResultStoreError("invalid_result_envelope");
  }
  let encrypted: Buffer | undefined;
  try {
    encrypted = encryptBytes(plaintext, key, resultEncryptionContext(context));
    await storage.root.replaceFile(MANAGED_RESULT_FILE, encrypted);
    const persisted = await storage.root.readFileBounded(
      MANAGED_RESULT_FILE,
      MAXIMUM_MANAGED_RESULT_BYTES,
    );
    try {
      if (!persisted.equals(encrypted)) {
        throw new ResultStoreError("invalid_result_envelope");
      }
      const verified = decryptBytes(persisted, key, resultEncryptionContext(context));
      try {
        const envelope = parseResultEnvelope(verified.toString("utf8"));
        if (envelope.committed !== committed
          || resultSha256(envelope.result) !== resultSha256(result)) {
          throw new ResultStoreError("invalid_result_envelope");
        }
      } finally {
        verified.fill(0);
      }
    } finally {
      persisted.fill(0);
    }
  } finally {
    plaintext.fill(0);
    encrypted?.fill(0);
  }
}

async function writeStoredResult(
  root: string,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
  committed: boolean,
): Promise<void> {
  const path = resultPath(root, context);
  const suffix = temporarySuffix();
  const plaintext = `${path}.${suffix}.write.json`;
  const encrypted = `${path}.${suffix}.tmp`;
  const serialized = serializeResultEnvelope(result, committed);
  await mkdir(join(root, "step-results"), { recursive: true });
  await writeFile(plaintext, serialized, { mode: 0o600 });
  try {
    await encryptFile(plaintext, encrypted, key, resultEncryptionContext(context));
    await replaceFileDurably(encrypted, path);
  } finally {
    serialized.fill(0);
    await rm(plaintext, { force: true });
    await rm(encrypted, { force: true });
  }
}

function serializeResultEnvelope(result: unknown, committed: boolean): Buffer {
  const encoded = JSON.stringify({
    __blueyResultEnvelope: RESULT_ENVELOPE_VERSION,
    committed,
    result,
  } satisfies StoredResultEnvelope);
  if (encoded === undefined) throw new ResultStoreError("invalid_result_envelope");
  return Buffer.from(`${encoded}\n`, "utf8");
}

function parseResultEnvelope(contents: string): StoredResultEnvelope {
  let value: unknown;
  try {
    value = JSON.parse(contents) as unknown;
  } catch {
    throw new ResultStoreError("invalid_result_envelope");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new ResultStoreError("invalid_result_envelope");
  }
  const record = value as Record<string, unknown>;
  if (record.__blueyResultEnvelope !== RESULT_ENVELOPE_VERSION) {
    throw new ResultStoreError(Object.prototype.hasOwnProperty.call(record, "__blueyResultEnvelope")
      ? "unsupported_result_envelope_version"
      : "invalid_result_envelope");
  }
  if (typeof record.committed !== "boolean"
    || !Object.prototype.hasOwnProperty.call(record, "result")
    || Object.keys(record).length !== 3) {
    throw new ResultStoreError("invalid_result_envelope");
  }
  return record as unknown as StoredResultEnvelope;
}

export function durableResultScope(context: DurableResultContext): string {
  const requestScope = requestScopeFor(context);
  return createHash("sha256")
    .update("bluey-jobs-runner\0durable-result-path\0")
    .update(context.profileScope)
    .update("\0")
    .update(requestScope)
    .digest("hex");
}

function assertManagedResultBinding(
  storage: ManagedResultStorage,
  context: DurableResultContext,
): void {
  const expectedScope = durableResultScope(context);
  if (storage.kind !== "result"
    || storage.scope !== expectedScope
    || storage.paths.root.relativePath !== storage.root.relativePath
    || storage.paths.encryptedResult.relativePath
      !== `${storage.root.relativePath}/${MANAGED_RESULT_FILE}`
    || storage.root.deviceId !== storage.temporary.deviceId) {
    throw new ResultStoreError("invalid_result_scope");
  }
}

function resultEncryptionContext(context: DurableResultContext) {
  return {
    purpose: "durable-result",
    scope: context.profileScope,
    requestScope: requestScopeFor(context),
  } as const;
}

function requestScopeFor(context: DurableResultContext): string {
  if (!context || typeof context.requestId !== "string" || !VALID_PROFILE_SCOPE.test(context.profileScope)) {
    throw new ResultStoreError("invalid_result_scope");
  }
  return createHash("sha256").update(context.requestId).digest("hex");
}

function resultSha256(result: unknown): string {
  const serialized = JSON.stringify(result);
  if (serialized === undefined) throw new ResultStoreError("invalid_result_envelope");
  return createHash("sha256")
    .update("bluey-jobs-runner\0durable-result-promotion\0")
    .update(serialized)
    .digest("hex");
}

function temporarySuffix(): string {
  return `${process.pid}.${randomBytes(8).toString("hex")}`;
}
