import { createHash, randomBytes } from "node:crypto";
import { constants } from "node:fs";
import {
  chmod,
  lstat,
  mkdir,
  open,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { dirname, join } from "node:path";
import type { DurableRunPhase } from "@bluey/jobs-automation";
import { decryptBytes, encryptBytes, replaceFileDurably } from "./crypto-envelope.js";
import { profilePathsFromScope, sealProfile } from "./profile-store.js";

const CHECKPOINT_VERSION = 1;
const MAX_CHECKPOINT_BYTES = 5 * 1024 * 1024;
const MAX_CHECKPOINTS = 512;
const PROFILE_SCOPE = /^[a-f0-9]{40}$/;
const CHECKPOINT_SCOPE = /^[a-f0-9]{64}$/;

export interface CloudRunCheckpoint<Request extends object = Record<string, unknown>, Event = unknown> {
  version: typeof CHECKPOINT_VERSION;
  phase: DurableRunPhase;
  createdAtMs: number;
  updatedAtMs: number;
  expiresAtMs: number;
  profileScope: string;
  browserSessionId: string;
  request: Request;
  browser: { url: string };
  workflow: {
    status: "prepared" | "needs_input" | "provider_review" | "side_effect_unknown";
    requestId: string;
    providerReview?: { adapter: string; adapterVersion?: string };
  };
  events: Event[];
  lease: {
    fence: number;
    expiresAtMs: number;
    ownerId: string;
  };
}

export async function writeRunCheckpoint<Request extends object, Event>(
  root: string,
  checkpoint: CloudRunCheckpoint<Request, Event>,
  key: Buffer,
): Promise<void> {
  validateCheckpoint(checkpoint);
  const checkpointScope = cloudCheckpointScope(checkpoint.profileScope, checkpoint.browserSessionId);
  const destination = runCheckpointPath(root, checkpoint.profileScope, checkpointScope);
  const directory = dirname(destination);
  await ensurePrivateDirectory(join(root, "run-checkpoints"));
  await ensurePrivateDirectory(directory);
  const suffix = `${process.pid}.${randomBytes(8).toString("hex")}`;
  const encrypted = `${destination}.${suffix}.tmp`;
  const serialized = Buffer.from(`${JSON.stringify(checkpoint)}\n`, "utf8");
  if (serialized.length > MAX_CHECKPOINT_BYTES) {
    serialized.fill(0);
    throw new Error("Cloud run checkpoint is too large");
  }
  try {
    const envelope = encryptBytes(
      serialized,
      key,
      encryptionContext(checkpoint.profileScope, checkpointScope),
    );
    try {
      await writeFile(encrypted, envelope, { flag: "wx", mode: 0o600 });
    } finally {
      envelope.fill(0);
    }
    await replaceFileDurably(encrypted, destination);
  } finally {
    await rm(encrypted, { force: true });
    serialized.fill(0);
  }
}

export async function readRunCheckpoint<Request extends object, Event>(
  root: string,
  profileScope: string,
  checkpointScope: string,
  key: Buffer,
): Promise<CloudRunCheckpoint<Request, Event> | undefined> {
  if (!PROFILE_SCOPE.test(profileScope) || !CHECKPOINT_SCOPE.test(checkpointScope)) return undefined;
  const source = runCheckpointPath(root, profileScope, checkpointScope);
  let metadata;
  try {
    metadata = await lstat(source);
  } catch (error) {
    if (nodeErrorCode(error) === "ENOENT") return undefined;
    throw error;
  }
  if (!metadata.isFile() || metadata.isSymbolicLink()
    || metadata.size > MAX_CHECKPOINT_BYTES + 256) {
    throw new Error("Invalid cloud run checkpoint file");
  }
  assertOwnerPrivate(metadata);
  const encrypted = await readFile(source);
  const plaintext = decryptBytes(
    encrypted,
    key,
    encryptionContext(profileScope, checkpointScope),
  );
  try {
    if (plaintext.length > MAX_CHECKPOINT_BYTES) throw new Error("Cloud run checkpoint is too large");
    const checkpoint = JSON.parse(plaintext.toString("utf8")) as unknown;
    validateCheckpoint(checkpoint);
    if ((checkpoint as CloudRunCheckpoint).profileScope !== profileScope
      || cloudCheckpointScope(
        profileScope,
        (checkpoint as CloudRunCheckpoint).browserSessionId,
      ) !== checkpointScope) {
      throw new Error("Cloud run checkpoint scope mismatch");
    }
    return checkpoint as CloudRunCheckpoint<Request, Event>;
  } finally {
    plaintext.fill(0);
    encrypted.fill(0);
  }
}

export async function listRunCheckpoints<Request extends object, Event>(
  root: string,
  key: Buffer,
): Promise<Array<{
  checkpointScope: string;
  checkpoint: CloudRunCheckpoint<Request, Event>;
}>> {
  const base = join(root, "run-checkpoints");
  await ensurePrivateDirectory(base);
  const profileDirectories = await readdir(base, { withFileTypes: true }).catch((error: unknown) => {
    if (nodeErrorCode(error) === "ENOENT") return [];
    throw error;
  });
  const results: Array<{
    checkpointScope: string;
    checkpoint: CloudRunCheckpoint<Request, Event>;
  }> = [];
  for (const profileEntry of profileDirectories) {
    if (!profileEntry.isDirectory() || !PROFILE_SCOPE.test(profileEntry.name)) continue;
    const profileDirectory = join(base, profileEntry.name);
    await ensurePrivateDirectory(profileDirectory);
    const entries = await readdir(profileDirectory, { withFileTypes: true });
    for (const entry of entries) {
      const matched = entry.isFile() && entry.name.match(/^([a-f0-9]{64})\.json\.enc$/);
      if (!matched) continue;
      if (results.length >= MAX_CHECKPOINTS) throw new Error("Too many cloud run checkpoints");
      const checkpoint = await readRunCheckpoint<Request, Event>(
        root,
        profileEntry.name,
        matched[1]!,
        key,
      );
      if (checkpoint) results.push({ checkpointScope: matched[1]!, checkpoint });
    }
  }
  return results;
}

export async function removeRunCheckpoint(
  root: string,
  profileScope: string,
  browserSessionId: string,
): Promise<void> {
  if (!PROFILE_SCOPE.test(profileScope)) return;
  const scope = cloudCheckpointScope(profileScope, browserSessionId);
  const path = runCheckpointPath(root, profileScope, scope);
  await rm(path, { force: true });
  await syncDirectory(dirname(path));
}

export function cloudCheckpointScope(profileScope: string, browserSessionId: string): string {
  if (!PROFILE_SCOPE.test(profileScope)
    || !/^[A-Za-z0-9_-]{3,160}$/.test(browserSessionId)) {
    throw new Error("Invalid cloud run checkpoint scope");
  }
  return createHash("sha256")
    .update("bluey-jobs-cloud-run-checkpoint\0")
    .update(profileScope)
    .update("\0")
    .update(browserSessionId)
    .digest("hex");
}

export interface OrphanProfileReconciliation {
  sealed: number;
  removed: number;
}

/** Seal or remove every crash-left plaintext profile before the HTTP listener starts. */
export async function reconcileOrphanActiveProfiles(
  root: string,
  key: Buffer,
): Promise<OrphanProfileReconciliation> {
  const activeDirectory = join(root, "active");
  await ensurePrivateDirectory(activeDirectory);
  const entries = await readdir(activeDirectory, { withFileTypes: true });
  let sealed = 0;
  let removed = 0;
  for (const entry of entries) {
    const path = join(activeDirectory, entry.name);
    if (!entry.isDirectory() || !PROFILE_SCOPE.test(entry.name)) {
      await rm(path, { recursive: true, force: true });
      removed += 1;
      continue;
    }
    try {
      const metadata = await lstat(path);
      if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error("unsafe active profile");
      await chmod(path, 0o700);
      await sealProfile(profilePathsFromScope(root, entry.name), key);
      sealed += 1;
    } catch {
      // A partially written browser profile is untrusted. If it cannot be
      // sealed into an authenticated envelope, remove it before serving work.
      await rm(path, { recursive: true, force: true });
      removed += 1;
    }
  }
  const remaining = await readdir(activeDirectory);
  if (remaining.length !== 0) throw new Error("Orphan plaintext browser profiles remain");
  return { sealed, removed };
}

function runCheckpointPath(root: string, profileScope: string, checkpointScope: string): string {
  if (!PROFILE_SCOPE.test(profileScope) || !CHECKPOINT_SCOPE.test(checkpointScope)) {
    throw new Error("Invalid cloud run checkpoint path");
  }
  return join(root, "run-checkpoints", profileScope, `${checkpointScope}.json.enc`);
}

function encryptionContext(profileScope: string, checkpointScope: string) {
  return { purpose: "run-checkpoint", scope: profileScope, checkpointScope } as const;
}

function validateCheckpoint(value: unknown): asserts value is CloudRunCheckpoint {
  const checkpoint = requireRecord(value, "cloud run checkpoint");
  if (checkpoint.version !== CHECKPOINT_VERSION
    || !["prepared", "needs_input", "provider_review", "final_submit_started",
      "final_submit_activated", "side_effect_unknown"].includes(String(checkpoint.phase))
    || !isTimestamp(checkpoint.createdAtMs)
    || !isTimestamp(checkpoint.updatedAtMs)
    || !isTimestamp(checkpoint.expiresAtMs)
    || checkpoint.updatedAtMs < checkpoint.createdAtMs
    || !PROFILE_SCOPE.test(String(checkpoint.profileScope))
    || !/^[A-Za-z0-9_-]{3,160}$/.test(String(checkpoint.browserSessionId))) {
    throw new Error("Invalid cloud run checkpoint envelope");
  }
  const request = requireRecord(checkpoint.request, "cloud checkpoint request");
  for (const field of ["accountId", "applicationIdentityId", "browserSessionId", "runId", "applicationId"] as const) {
    if (!/^[A-Za-z0-9_-]{3,160}$/.test(String(request[field]))) {
      throw new Error("Invalid cloud run checkpoint request");
    }
  }
  if (request.browserSessionId !== checkpoint.browserSessionId) throw new Error("Cloud checkpoint session mismatch");
  if (!/^[A-Za-z0-9:_-]{3,160}$/.test(String(request.browserProfileId))) {
    throw new Error("Invalid cloud run checkpoint browser profile");
  }
  const packet = requireRecord(request.packet, "cloud checkpoint packet");
  if (packet.applicationId !== request.applicationId
    || (packet.applicationIdentityId !== undefined
      && packet.applicationIdentityId !== request.applicationIdentityId)
    || (packet.browserProfileId !== undefined
      && packet.browserProfileId !== request.browserProfileId)) {
    throw new Error("Cloud run checkpoint request binding mismatch");
  }
  const browser = requireRecord(checkpoint.browser, "cloud checkpoint browser");
  if (typeof browser.url !== "string" || browser.url.length === 0 || browser.url.length > 8_192) {
    throw new Error("Invalid cloud run checkpoint browser URL");
  }
  const workflow = requireRecord(checkpoint.workflow, "cloud checkpoint workflow");
  if (!["prepared", "needs_input", "provider_review", "side_effect_unknown"]
    .includes(String(workflow.status))
    || typeof workflow.requestId !== "string"
    || !/^[A-Za-z0-9:_-]{3,240}$/.test(workflow.requestId)) {
    throw new Error("Invalid cloud run checkpoint workflow state");
  }
  if (!phaseMatchesWorkflow(String(checkpoint.phase), String(workflow.status))) {
    throw new Error("Invalid cloud run checkpoint phase transition");
  }
  if (!Array.isArray(checkpoint.events) || checkpoint.events.length > 10_000) {
    throw new Error("Invalid cloud run checkpoint events");
  }
  for (const event of checkpoint.events) {
    const record = requireRecord(event, "cloud run checkpoint event");
    if (typeof record.id !== "string" || record.id.length === 0 || record.id.length > 240
      || typeof record.type !== "string" || record.type.length === 0 || record.type.length > 120
      || (record.occurredAt !== undefined
        && (typeof record.occurredAt !== "string" || record.occurredAt.length > 64))
      || (record.detail !== undefined
        && (!record.detail || typeof record.detail !== "object" || Array.isArray(record.detail)))) {
      throw new Error("Invalid cloud run checkpoint event");
    }
  }
  const lease = requireRecord(checkpoint.lease, "cloud checkpoint lease metadata");
  if (!Number.isSafeInteger(lease.fence) || Number(lease.fence) <= 0
    || !isTimestamp(lease.expiresAtMs)
    || typeof lease.ownerId !== "string"
    || !/^[A-Za-z0-9._:-]{1,128}$/.test(lease.ownerId)) {
    throw new Error("Invalid cloud run checkpoint lease metadata");
  }
}

async function ensurePrivateDirectory(path: string): Promise<void> {
  await mkdir(path, { recursive: true, mode: 0o700 });
  const metadata = await lstat(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error("Runner directory is unsafe");
  await chmod(path, 0o700);
  const normalized = await stat(path);
  if (process.platform !== "win32" && ((normalized.mode & 0o077) !== 0
    || (typeof process.getuid === "function" && normalized.uid !== process.getuid()))) {
    throw new Error("Runner directory is not owner-private");
  }
}

function assertOwnerPrivate(metadata: { mode: number; uid: number }): void {
  if (process.platform !== "win32" && ((metadata.mode & 0o077) !== 0
    || (typeof process.getuid === "function" && metadata.uid !== process.getuid()))) {
    throw new Error("Cloud run checkpoint is not owner-private");
  }
}

async function syncDirectory(path: string): Promise<void> {
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY);
    await handle.sync();
  } catch (error) {
    if (!["EBADF", "EINVAL", "EISDIR", "ENOENT", "ENOSYS", "ENOTSUP", "EPERM"]
      .includes(nodeErrorCode(error) || "")) throw error;
  } finally {
    await handle?.close().catch(() => undefined);
  }
}

function requireRecord(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`Invalid ${label}`);
  return value as Record<string, unknown>;
}

function isTimestamp(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function phaseMatchesWorkflow(phase: string, status: string): boolean {
  if (phase === "prepared") return status === "prepared";
  if (phase === "needs_input") return status === "needs_input";
  if (phase === "provider_review") return status === "provider_review";
  return status === "side_effect_unknown";
}

function nodeErrorCode(error: unknown): string | undefined {
  return error && typeof error === "object" && "code" in error
    ? String((error as { code?: unknown }).code)
    : undefined;
}
