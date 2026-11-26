import {
  createCipheriv,
  createDecipheriv,
  createHash,
  hkdfSync,
  randomBytes,
} from "node:crypto";
import { constants } from "node:fs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  chmod,
  lstat,
  mkdir,
  open,
  readFile,
  readdir,
  rename,
  rm,
  stat,
} from "node:fs/promises";
import { dirname, join } from "node:path";
import type { DurableRunPhase } from "@bluey/jobs-automation";
import type { LocalRunCapabilities } from "./local-capabilities.js";
import type { LocalSideEffectReason } from "./local-failure.js";

const CHECKPOINT_VERSION = 1;
const MAGIC = Buffer.from("BLUEYLJ1");
const KEY_BYTES = 32;
const IV_BYTES = 12;
const TAG_BYTES = 16;
const MAX_CHECKPOINT_BYTES = 5 * 1024 * 1024;
const MAX_CHECKPOINTS = 256;
const SCOPE_PATTERN = /^[a-f0-9]{64}$/;
const WINDOWS_KEY_MAGIC = Buffer.from("BLUEYLK1");
const execFileAsync = promisify(execFile);

interface WindowsSafeStorage {
  isEncryptionAvailable(): boolean;
  encryptString(plaintext: string): Buffer;
  decryptString(ciphertext: Buffer): string;
}

type WindowsSafeStorageLoader = () => Promise<WindowsSafeStorage>;

export interface LocalCheckpointStoreOpenOptions {
  loadWindowsSafeStorage?: WindowsSafeStorageLoader;
}

export interface LocalCheckpointDelivery {
  apiOrigin: string;
  capabilities: LocalRunCapabilities;
}

export interface LocalRunCheckpoint<Request extends object = Record<string, unknown>, ProviderReview = unknown> {
  version: typeof CHECKPOINT_VERSION;
  phase: DurableRunPhase;
  createdAtMs: number;
  updatedAtMs: number;
  expiresAtMs: number;
  request: Request;
  delivery: LocalCheckpointDelivery;
  browser: { url: string };
  workflow: {
    status: "prepared" | "needs_input" | "provider_review" | "side_effect_unknown";
    adapter?: string;
    approvedSubmitActionConsumed?: boolean;
    sideEffectReason?: LocalSideEffectReason;
  };
  providerFinalReview?: ProviderReview;
  events: Array<{ event: string; details: Record<string, unknown>; at: string }>;
}

/**
 * An encrypted checkpoint store rooted in Electron's per-user data directory.
 * The random installation key is deliberately a private file instead of a
 * Keychain item, so normal Bluey Browser startup never triggers a Keychain UI.
 */
export class LocalCheckpointStore {
  private constructor(
    readonly directory: string,
    private readonly key: Buffer,
  ) {}

  static async open(
    baseDirectory: string,
    options: LocalCheckpointStoreOpenOptions = {},
  ): Promise<LocalCheckpointStore> {
    const recoveryDirectory = join(baseDirectory, "recovery");
    const directory = join(recoveryDirectory, "checkpoints");
    await ensurePrivateDirectory(recoveryDirectory);
    await ensurePrivateDirectory(directory);
    const key = await loadOrCreateKey(
      join(recoveryDirectory, "checkpoint-key-v1"),
      options.loadWindowsSafeStorage,
    );
    return new LocalCheckpointStore(directory, key);
  }

  async write<Request extends object, ProviderReview>(
    checkpoint: LocalRunCheckpoint<Request, ProviderReview>,
  ): Promise<void> {
    validateCheckpoint(checkpoint);
    const scope = localCheckpointScope(checkpoint.request);
    const serialized = Buffer.from(JSON.stringify(checkpoint), "utf8");
    if (serialized.length === 0 || serialized.length > MAX_CHECKPOINT_BYTES) {
      serialized.fill(0);
      throw new Error("Local run checkpoint is too large");
    }
    const encrypted = encryptCheckpoint(serialized, this.key, scope);
    const destination = this.path(scope);
    const temporary = `${destination}.${process.pid}.${randomBytes(8).toString("hex")}.tmp`;
    let handle;
    try {
      handle = await open(
        temporary,
        constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | noFollowFlag(),
        0o600,
      );
      await handle.chmod(0o600);
      await handle.writeFile(encrypted);
      await handle.sync();
      await handle.close();
      handle = undefined;
      await rename(temporary, destination);
      await protectWindowsPath(destination, false);
      await syncDirectory(this.directory);
    } finally {
      await handle?.close().catch(() => undefined);
      await rm(temporary, { force: true });
      serialized.fill(0);
    }
  }

  async read<Request extends object, ProviderReview>(
    scope: string,
  ): Promise<LocalRunCheckpoint<Request, ProviderReview> | undefined> {
    if (!SCOPE_PATTERN.test(scope)) return undefined;
    const path = this.path(scope);
    let metadata;
    try {
      metadata = await lstat(path);
    } catch (error) {
      if (nodeErrorCode(error) === "ENOENT") return undefined;
      throw error;
    }
    if (!metadata.isFile() || metadata.isSymbolicLink()
      || metadata.size < MAGIC.length + IV_BYTES + TAG_BYTES
      || metadata.size > MAX_CHECKPOINT_BYTES + 128) {
      throw new Error("Invalid local run checkpoint file");
    }
    assertOwnerPrivate(metadata, path);
    const plaintext = decryptCheckpoint(await readFile(path), this.key, scope);
    try {
      const checkpoint = JSON.parse(plaintext.toString("utf8")) as unknown;
      validateCheckpoint(checkpoint);
      if (localCheckpointScope((checkpoint as LocalRunCheckpoint).request) !== scope) {
        throw new Error("Local run checkpoint scope mismatch");
      }
      return checkpoint as LocalRunCheckpoint<Request, ProviderReview>;
    } finally {
      plaintext.fill(0);
    }
  }

  async list<Request extends object, ProviderReview>(): Promise<Array<{
    scope: string;
    checkpoint: LocalRunCheckpoint<Request, ProviderReview>;
  }>> {
    const entries = (await readdir(this.directory, { withFileTypes: true }))
      .filter((entry) => entry.isFile() && /^([a-f0-9]{64})\.json\.enc$/.test(entry.name));
    if (entries.length > MAX_CHECKPOINTS) throw new Error("Too many local run checkpoints");
    const checkpoints = [];
    for (const entry of entries) {
      const scope = entry.name.slice(0, 64);
      const checkpoint = await this.read<Request, ProviderReview>(scope);
      if (checkpoint) checkpoints.push({ scope, checkpoint });
    }
    return checkpoints;
  }

  async remove(scope: string): Promise<void> {
    if (!SCOPE_PATTERN.test(scope)) return;
    await rm(this.path(scope), { force: true });
    await syncDirectory(this.directory);
  }

  scopeFor(request: object): string {
    return localCheckpointScope(request);
  }

  private path(scope: string): string {
    if (!SCOPE_PATTERN.test(scope)) throw new Error("Invalid local run checkpoint scope");
    return join(this.directory, `${scope}.json.enc`);
  }
}

export function localCheckpointScope(request: object): string {
  const record = requireRecord(request, "checkpoint request");
  const accountId = requireIdentifier(record.accountId, "account");
  const identityId = requireIdentifier(record.applicationIdentityId, "application identity");
  const runId = requireIdentifier(record.runId, "run");
  return createHash("sha256")
    .update("bluey-jobs-local-checkpoint\0")
    .update(accountId)
    .update("\0")
    .update(identityId)
    .update("\0")
    .update(runId)
    .digest("hex");
}

function validateCheckpoint(value: unknown): asserts value is LocalRunCheckpoint {
  const checkpoint = requireRecord(value, "local run checkpoint");
  if (checkpoint.version !== CHECKPOINT_VERSION
    || !["prepared", "needs_input", "provider_review", "final_submit_started",
      "final_submit_activated", "side_effect_unknown"].includes(String(checkpoint.phase))
    || !isTimestamp(checkpoint.createdAtMs)
    || !isTimestamp(checkpoint.updatedAtMs)
    || !isTimestamp(checkpoint.expiresAtMs)
    || checkpoint.updatedAtMs < checkpoint.createdAtMs) {
    throw new Error("Invalid local run checkpoint envelope");
  }
  localCheckpointScope(requireRecord(checkpoint.request, "checkpoint request"));
  const delivery = requireRecord(checkpoint.delivery, "checkpoint delivery");
  const apiOrigin = new URL(requireString(delivery.apiOrigin, "API origin"));
  if (apiOrigin.username || apiOrigin.password
    || (apiOrigin.protocol !== "https:"
      && !(apiOrigin.protocol === "http:" && ["127.0.0.1", "localhost"].includes(apiOrigin.hostname)))) {
    throw new Error("Invalid local run checkpoint API origin");
  }
  const capabilities = requireRecord(delivery.capabilities, "checkpoint capabilities");
  for (const field of ["result", "resume", "runId"] as const) requireString(capabilities[field], field);
  if (capabilities.submit !== undefined) requireString(capabilities.submit, "submit");
  const request = requireRecord(checkpoint.request, "checkpoint request");
  if (capabilities.runId !== request.runId
    || capabilities.result === capabilities.resume
    || (capabilities.submit !== undefined
      && [capabilities.result, capabilities.resume].includes(capabilities.submit))) {
    throw new Error("Invalid local run checkpoint capability bindings");
  }
  const packet = requireRecord(request.packet, "checkpoint packet");
  if (packet.applicationId !== request.applicationId
    || (packet.applicationIdentityId !== undefined
      && packet.applicationIdentityId !== request.applicationIdentityId)) {
    throw new Error("Invalid local run checkpoint request binding");
  }
  if (!isTimestamp(capabilities.expiresAtMs)
    || capabilities.expiresAtMs !== checkpoint.expiresAtMs) {
    throw new Error("Invalid local run checkpoint capability expiry");
  }
  const browser = requireRecord(checkpoint.browser, "checkpoint browser");
  const url = requireString(browser.url, "checkpoint browser URL");
  if (url.length > 8_192) throw new Error("Invalid local run checkpoint browser URL");
  const workflow = requireRecord(checkpoint.workflow, "checkpoint workflow");
  if (!["prepared", "needs_input", "provider_review", "side_effect_unknown"]
    .includes(String(workflow.status))) {
    throw new Error("Invalid local run checkpoint workflow state");
  }
  if (!phaseMatchesWorkflow(String(checkpoint.phase), String(workflow.status))) {
    throw new Error("Invalid local run checkpoint phase transition");
  }
  if (workflow.approvedSubmitActionConsumed !== undefined
    && typeof workflow.approvedSubmitActionConsumed !== "boolean") {
    throw new Error("Invalid local run checkpoint approval state");
  }
  if (workflow.sideEffectReason !== undefined
    && (workflow.status !== "side_effect_unknown"
      || !["manual_submission_observed", "submit_marker_state_unavailable", "submit_outcome_unknown"]
        .includes(String(workflow.sideEffectReason)))) {
    throw new Error("Invalid local run checkpoint side-effect reason");
  }
  if (!Array.isArray(checkpoint.events) || checkpoint.events.length > 10_000) {
    throw new Error("Invalid local run checkpoint events");
  }
  for (const event of checkpoint.events) {
    const record = requireRecord(event, "local checkpoint event");
    if (typeof record.event !== "string" || record.event.length > 120
      || typeof record.at !== "string" || record.at.length > 64
      || !record.details || typeof record.details !== "object" || Array.isArray(record.details)) {
      throw new Error("Invalid local run checkpoint event");
    }
  }
  if (checkpoint.providerFinalReview !== undefined) {
    const review = requireRecord(checkpoint.providerFinalReview, "provider review checkpoint");
    if (!["greenhouse", "lever"].includes(String(review.adapter))
      || typeof review.adapterVersion !== "string"
      || review.adapterVersion.length > 120) {
      throw new Error("Invalid local provider review checkpoint");
    }
  }
}

function encryptCheckpoint(plaintext: Buffer, masterKey: Buffer, scope: string): Buffer {
  const aad = checkpointAad(scope);
  const key = checkpointKey(masterKey, aad);
  const iv = randomBytes(IV_BYTES);
  try {
    const cipher = createCipheriv("aes-256-gcm", key, iv);
    cipher.setAAD(aad);
    const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
    return Buffer.concat([MAGIC, iv, ciphertext, cipher.getAuthTag()]);
  } finally {
    key.fill(0);
  }
}

function decryptCheckpoint(encrypted: Buffer, masterKey: Buffer, scope: string): Buffer {
  if (encrypted.length < MAGIC.length + IV_BYTES + TAG_BYTES
    || !encrypted.subarray(0, MAGIC.length).equals(MAGIC)) {
    throw new Error("Invalid local run checkpoint envelope");
  }
  const aad = checkpointAad(scope);
  const key = checkpointKey(masterKey, aad);
  try {
    const iv = encrypted.subarray(MAGIC.length, MAGIC.length + IV_BYTES);
    const tag = encrypted.subarray(encrypted.length - TAG_BYTES);
    const ciphertext = encrypted.subarray(MAGIC.length + IV_BYTES, encrypted.length - TAG_BYTES);
    const decipher = createDecipheriv("aes-256-gcm", key, iv);
    decipher.setAAD(aad);
    decipher.setAuthTag(tag);
    return Buffer.concat([decipher.update(ciphertext), decipher.final()]);
  } catch {
    throw new Error("Local run checkpoint authentication failed");
  } finally {
    key.fill(0);
  }
}

function checkpointAad(scope: string): Buffer {
  if (!SCOPE_PATTERN.test(scope)) throw new Error("Invalid local run checkpoint scope");
  return Buffer.from(`bluey-jobs-local\0BLUEYLJ1\0aes-256-gcm\0${scope}`, "utf8");
}

function checkpointKey(masterKey: Buffer, aad: Buffer): Buffer {
  if (masterKey.length !== KEY_BYTES) throw new Error("Invalid local checkpoint key");
  return Buffer.from(hkdfSync(
    "sha256",
    masterKey,
    Buffer.from("bluey-jobs-local-checkpoint:hkdf-sha256"),
    aad,
    KEY_BYTES,
  ));
}

async function loadOrCreateKey(
  path: string,
  loadWindowsSafeStorage: WindowsSafeStorageLoader = defaultWindowsSafeStorageLoader,
): Promise<Buffer> {
  if (process.platform === "win32" && osSecureStoreEnabled()) {
    return loadOrCreateWindowsProtectedKey(path, loadWindowsSafeStorage);
  }
  return loadOrCreatePrivateFileKey(path);
}

async function loadOrCreatePrivateFileKey(path: string): Promise<Buffer> {
  let handle;
  try {
    handle = await open(
      path,
      constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | noFollowFlag(),
      0o600,
    );
    const key = randomBytes(KEY_BYTES);
    await handle.chmod(0o600);
    await handle.writeFile(key);
    await handle.sync();
    await handle.close();
    handle = undefined;
    await protectWindowsPath(path, false);
    await syncDirectory(dirname(path));
    return key;
  } catch (error) {
    await handle?.close().catch(() => undefined);
    if (nodeErrorCode(error) !== "EEXIST") throw error;
  }
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size !== KEY_BYTES) {
    throw new Error("Invalid local checkpoint key file");
  }
  await chmod(path, 0o600);
  await protectWindowsPath(path, false);
  assertOwnerPrivate(await stat(path), path);
  return readFile(path);
}

async function loadOrCreateWindowsProtectedKey(
  path: string,
  loadWindowsSafeStorage: WindowsSafeStorageLoader,
): Promise<Buffer> {
  const safeStorage = await loadWindowsSafeStorage();
  if (!safeStorage.isEncryptionAvailable()) {
    throw new Error("Windows user-scoped checkpoint encryption is unavailable");
  }
  let handle;
  try {
    handle = await open(
      path,
      constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | noFollowFlag(),
      0o600,
    );
    const key = randomBytes(KEY_BYTES);
    const protectedKey = safeStorage.encryptString(key.toString("base64"));
    await handle.writeFile(Buffer.concat([WINDOWS_KEY_MAGIC, protectedKey]));
    await handle.sync();
    await handle.close();
    handle = undefined;
    await protectWindowsPath(path, false);
    await syncDirectory(dirname(path));
    return key;
  } catch (error) {
    await handle?.close().catch(() => undefined);
    if (nodeErrorCode(error) !== "EEXIST") throw error;
  }
  await protectWindowsPath(path, false);
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink()
    || metadata.size <= WINDOWS_KEY_MAGIC.length
    || metadata.size > 64 * 1024) {
    throw new Error("Invalid Windows checkpoint key file");
  }
  const wrapped = await readFile(path);
  if (!wrapped.subarray(0, WINDOWS_KEY_MAGIC.length).equals(WINDOWS_KEY_MAGIC)) {
    throw new Error("Invalid Windows checkpoint key envelope");
  }
  let decoded: Buffer;
  try {
    decoded = Buffer.from(
      safeStorage.decryptString(wrapped.subarray(WINDOWS_KEY_MAGIC.length)),
      "base64",
    );
  } catch {
    throw new Error("Windows checkpoint key authentication failed");
  }
  if (decoded.length !== KEY_BYTES) throw new Error("Invalid Windows checkpoint key");
  return decoded;
}

async function defaultWindowsSafeStorageLoader(): Promise<WindowsSafeStorage> {
  const { safeStorage } = await import("electron");
  if (!safeStorage) throw new Error("Windows user-scoped checkpoint encryption is unavailable");
  return safeStorage;
}

function osSecureStoreEnabled(): boolean {
  return truthyEnvironmentVariable("BLUEY_USE_OS_KEYCHAIN")
    || truthyEnvironmentVariable("BLUEY_USE_SECURE_STORE");
}

function truthyEnvironmentVariable(name: string): boolean {
  return ["1", "true", "yes", "on"].includes(
    String(process.env[name] || "").trim().toLowerCase(),
  );
}

async function ensurePrivateDirectory(path: string): Promise<void> {
  await mkdir(path, { recursive: true, mode: 0o700 });
  const metadata = await lstat(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error("Local checkpoint directory is not private");
  }
  await chmod(path, 0o700);
  await protectWindowsPath(path, true);
  assertOwnerPrivate(await stat(path), path);
}

function assertOwnerPrivate(metadata: { mode: number; uid: number }, path: string): void {
  if (process.platform === "win32") return;
  if ((metadata.mode & 0o077) !== 0
    || (typeof process.getuid === "function" && metadata.uid !== process.getuid())) {
    throw new Error(`Local checkpoint file is not owner-private: ${path}`);
  }
}

async function protectWindowsPath(path: string, directory: boolean): Promise<void> {
  if (process.platform !== "win32") return;
  const systemRoot = process.env.SystemRoot || process.env.WINDIR || "C:\\Windows";
  const options = { windowsHide: true, timeout: 10_000, maxBuffer: 64 * 1024 } as const;
  const { stdout } = await execFileAsync(
    join(systemRoot, "System32", "whoami.exe"),
    ["/user", "/fo", "csv", "/nh"],
    options,
  );
  const sid = stdout.match(/S-1-(?:\d+-)+\d+/)?.[0];
  if (!sid) throw new Error("Could not resolve the current Windows account SID");
  const grant = `*${sid}:${directory ? "(OI)(CI)F" : "F"}`;
  await execFileAsync(
    join(systemRoot, "System32", "icacls.exe"),
    [path, "/inheritance:r", "/grant:r", grant],
    options,
  );
}

async function syncDirectory(path: string): Promise<void> {
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY);
    await handle.sync();
  } catch (error) {
    if (!["EBADF", "EINVAL", "EISDIR", "ENOSYS", "ENOTSUP", "EPERM"]
      .includes(nodeErrorCode(error) || "")) throw error;
  } finally {
    await handle?.close().catch(() => undefined);
  }
}

function requireRecord(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`Invalid ${label}`);
  return value as Record<string, unknown>;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) throw new Error(`Invalid ${label}`);
  return value;
}

function requireIdentifier(value: unknown, label: string): string {
  const identifier = requireString(value, label);
  if (!/^[A-Za-z0-9_-]{3,160}$/.test(identifier)) throw new Error(`Invalid ${label}`);
  return identifier;
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

function noFollowFlag(): number {
  return process.platform === "win32" ? 0 : constants.O_NOFOLLOW;
}
