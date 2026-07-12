import { createHash, randomBytes } from "node:crypto";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { decryptFile, encryptFile, replaceFileDurably } from "./crypto-envelope.js";

const RESULT_ENVELOPE_VERSION = 1;
const VALID_PROFILE_SCOPE = /^[a-f0-9]{40}$/;

export interface DurableResultContext {
  requestId: string;
  profileScope: string;
}

interface StoredResultEnvelope {
  __blueyResultEnvelope: typeof RESULT_ENVELOPE_VERSION;
  committed: boolean;
  result: unknown;
}

export type ResultStoreErrorCode =
  | "invalid_result_envelope"
  | "invalid_result_scope"
  | "unsupported_result_envelope_version";

export class ResultStoreError extends Error {
  constructor(readonly code: ResultStoreErrorCode) {
    super({
      invalid_result_envelope: "Stored runner result has an invalid envelope.",
      invalid_result_scope: "Stored runner result scope is invalid.",
      unsupported_result_envelope_version: "Stored runner result uses an unsupported envelope version.",
    }[code]);
    this.name = "ResultStoreError";
  }
}

export function resultPath(root: string, context: DurableResultContext): string {
  return join(root, "step-results", `${resultFileScope(context)}.json.enc`);
}

export async function readResult<T>(
  root: string,
  context: DurableResultContext,
  key: Buffer,
): Promise<T | undefined> {
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
    return stored.committed ? stored.result as T : undefined;
  } finally {
    await rm(temporary, { force: true });
  }
}

export async function writeResult(
  root: string,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
): Promise<void> {
  await writeStoredResult(root, context, result, key, true);
}

export async function stageResult(
  root: string,
  context: DurableResultContext,
  result: unknown,
  key: Buffer,
): Promise<void> {
  await writeStoredResult(root, context, result, key, false);
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
  const envelope: StoredResultEnvelope = {
    __blueyResultEnvelope: RESULT_ENVELOPE_VERSION,
    committed,
    result,
  };
  await mkdir(join(root, "step-results"), { recursive: true });
  await writeFile(plaintext, `${JSON.stringify(envelope)}\n`, { mode: 0o600 });
  try {
    await encryptFile(plaintext, encrypted, key, resultEncryptionContext(context));
    await replaceFileDurably(encrypted, path);
  } finally {
    await rm(plaintext, { force: true });
    await rm(encrypted, { force: true });
  }
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

function resultFileScope(context: DurableResultContext): string {
  const requestScope = requestScopeFor(context);
  return createHash("sha256")
    .update("bluey-jobs-runner\0durable-result-path\0")
    .update(context.profileScope)
    .update("\0")
    .update(requestScope)
    .digest("hex");
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

function temporarySuffix(): string {
  return `${process.pid}.${randomBytes(8).toString("hex")}`;
}
