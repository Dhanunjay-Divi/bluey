import { createHash, randomBytes } from "node:crypto";
import { constants, type Dirent } from "node:fs";
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
import {
  assertApprovedExecutionChecksum,
  type ApplicationPacket,
  type DurableRunPhase,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import {
  decryptBytes,
  encryptBytes,
  replaceFileDurably,
} from "./crypto-envelope.js";
import { profilePathsFromScope, sealProfile } from "./profile-store.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
} from "./native-runner-storage.js";
import type { ManagedProfileStorage } from "./subject-storage-manager.js";
import { subjectStoragePaths } from "./subject-storage-layout.js";

export const CURRENT_CHECKPOINT_VERSION = 2 as const;
export type RunCheckpointVersion = 1 | typeof CURRENT_CHECKPOINT_VERSION;
const MAX_CHECKPOINT_BYTES = 5 * 1024 * 1024;
const MAX_CHECKPOINTS_PER_PROFILE = 8;
const MAXIMUM_MANAGED_CHECKPOINT_BYTES = MAX_CHECKPOINT_BYTES + 64;
const PROFILE_SCOPE = /^[a-f0-9]{40}$/;
const CHECKPOINT_SCOPE = /^[a-f0-9]{64}$/;
const MANAGED_CHECKPOINT_FILE = /^([a-f0-9]{64})\.json\.enc$/;
const WORKFLOW_COMMAND_REQUEST_ID =
  /^wfreq-v2-[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

export interface CloudRunCheckpoint<
  Request extends object = Record<string, unknown>,
  Event = unknown,
> {
  version: RunCheckpointVersion;
  phase: DurableRunPhase;
  createdAtMs: number;
  updatedAtMs: number;
  expiresAtMs: number;
  profileScope: string;
  browserSessionId: string;
  request: Request;
  browser: { url: string };
  workflow: {
    status:
      "prepared" | "needs_input" | "provider_review" | "side_effect_unknown";
    requestId: string;
    providerReview?: { adapter: string; adapterVersion?: string };
  };
  events: Event[];
  lease: {
    fence: number;
    expiresAtMs: number;
    ownerId: string;
    leaseToken?: string;
    purgeSubject?: string;
  };
}

export async function writeRunCheckpoint<Request extends object, Event>(
  root: string,
  checkpoint: CloudRunCheckpoint<Request, Event>,
  key: Buffer,
): Promise<void> {
  validateCheckpoint(checkpoint);
  const checkpointScope = cloudCheckpointScope(
    checkpoint.profileScope,
    checkpoint.browserSessionId,
  );
  const destination = runCheckpointPath(
    root,
    checkpoint.profileScope,
    checkpointScope,
  );
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

/**
 * Persist a v2-layout checkpoint through the retained native profile
 * capability. Plaintext exists only in bounded memory and the atomic native
 * replacement is read back and authenticated before this call succeeds.
 */
export async function writeManagedRunCheckpoint<Request extends object, Event>(
  storage: ManagedProfileStorage,
  checkpoint: CloudRunCheckpoint<Request, Event>,
  key: Buffer,
): Promise<void> {
  validateCheckpoint(checkpoint);
  assertManagedCheckpointBinding(storage, checkpoint.profileScope);
  const checkpointScope = cloudCheckpointScope(
    checkpoint.profileScope,
    checkpoint.browserSessionId,
  );
  const fileName = managedCheckpointFileName(checkpointScope);
  const serialized = serializeCheckpoint(checkpoint);
  let encrypted: Buffer | undefined;
  try {
    encrypted = encryptBytes(
      serialized,
      key,
      encryptionContext(checkpoint.profileScope, checkpointScope),
    );
    if (encrypted.length > MAXIMUM_MANAGED_CHECKPOINT_BYTES) {
      throw new Error("Cloud run checkpoint is too large");
    }
    await storage.checkpoints.replaceFile(fileName, encrypted);
    const inventory = await storage.checkpoints.inventory();
    const entry = inventory.entries.find(
      (candidate) => candidate.relativePath === fileName,
    );
    if (!entry)
      throw new Error("Managed cloud run checkpoint publication failed");
    assertManagedCheckpointEntry(storage, entry, fileName);
    const persisted = await storage.checkpoints.readFileBounded(
      fileName,
      MAXIMUM_MANAGED_CHECKPOINT_BYTES,
    );
    try {
      if (
        !persisted.equals(encrypted) ||
        persisted.length !== entry.sizeBytes ||
        sha256(persisted) !== entry.sha256
      ) {
        throw new Error("Managed cloud run checkpoint publication failed");
      }
      const verified = decryptBytes(
        persisted,
        key,
        encryptionContext(checkpoint.profileScope, checkpointScope),
      );
      try {
        parseCheckpoint<Request, Event>(
          verified,
          checkpoint.profileScope,
          checkpointScope,
        );
      } finally {
        verified.fill(0);
      }
      const after = await storage.checkpoints.inventory();
      const afterEntry = after.entries.find(
        (candidate) => candidate.relativePath === fileName,
      );
      if (!afterEntry || !sameInventoryEntry(entry, afterEntry)) {
        throw new Error("Managed cloud run checkpoint publication failed");
      }
    } finally {
      persisted.fill(0);
    }
  } finally {
    serialized.fill(0);
    encrypted?.fill(0);
  }
}

export async function readRunCheckpoint<Request extends object, Event>(
  root: string,
  profileScope: string,
  checkpointScope: string,
  key: Buffer,
): Promise<CloudRunCheckpoint<Request, Event> | undefined> {
  if (
    !PROFILE_SCOPE.test(profileScope) ||
    !CHECKPOINT_SCOPE.test(checkpointScope)
  )
    return undefined;
  const source = runCheckpointPath(root, profileScope, checkpointScope);
  let metadata;
  try {
    metadata = await lstat(source);
  } catch (error) {
    if (nodeErrorCode(error) === "ENOENT") return undefined;
    throw error;
  }
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size > MAX_CHECKPOINT_BYTES + 256
  ) {
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
    return parseCheckpoint<Request, Event>(
      plaintext,
      profileScope,
      checkpointScope,
    );
  } finally {
    plaintext.fill(0);
    encrypted.fill(0);
  }
}

export async function readManagedRunCheckpoint<Request extends object, Event>(
  storage: ManagedProfileStorage,
  checkpointScope: string,
  key: Buffer,
): Promise<CloudRunCheckpoint<Request, Event> | undefined> {
  assertManagedCheckpointBinding(storage, storage.scope);
  if (!CHECKPOINT_SCOPE.test(checkpointScope)) return undefined;
  const fileName = managedCheckpointFileName(checkpointScope);
  const before = await storage.checkpoints.inventory();
  const entry = before.entries.find(
    (candidate) => candidate.relativePath === fileName,
  );
  if (!entry) {
    const after = await storage.checkpoints.inventory();
    if (
      after.entries.some((candidate) => candidate.relativePath === fileName)
    ) {
      throw new Error("Managed cloud run checkpoint changed while it was read");
    }
    return undefined;
  }
  assertManagedCheckpointEntry(storage, entry, fileName);
  const checkpoint = await readManagedCheckpointEntry<Request, Event>(
    storage,
    entry,
    checkpointScope,
    key,
  );
  const after = await storage.checkpoints.inventory();
  const afterEntry = after.entries.find(
    (candidate) => candidate.relativePath === fileName,
  );
  if (!afterEntry || !sameInventoryEntry(entry, afterEntry)) {
    throw new Error("Managed cloud run checkpoint changed while it was read");
  }
  return checkpoint;
}

export async function listRunCheckpoints<Request extends object, Event>(
  root: string,
  key: Buffer,
): Promise<
  Array<{
    checkpointScope: string;
    checkpoint: CloudRunCheckpoint<Request, Event>;
  }>
> {
  const scan = await scanRunCheckpoints<Request, Event>(root, key);
  if (scan.failures.length > 0)
    throw new Error("Cloud run checkpoint scan failed");
  return scan.checkpoints;
}

export async function listManagedRunCheckpoints<Request extends object, Event>(
  storage: ManagedProfileStorage,
  key: Buffer,
): Promise<
  Array<{
    checkpointScope: string;
    checkpoint: CloudRunCheckpoint<Request, Event>;
  }>
> {
  const scan = await scanManagedRunCheckpoints<Request, Event>(storage, key);
  if (scan.failures.length > 0)
    throw new Error("Managed cloud run checkpoint scan failed");
  return scan.checkpoints;
}

export interface RunCheckpointScanFailure {
  profileScope: string;
  checkpointScope?: string;
  code:
    | "checkpoint_limit_exceeded"
    | "checkpoint_unreadable"
    | "profile_unreadable";
}

export interface RunCheckpointScan<Request extends object, Event> {
  checkpoints: Array<{
    checkpointScope: string;
    checkpoint: CloudRunCheckpoint<Request, Event>;
  }>;
  failures: RunCheckpointScanFailure[];
}

export async function scanRunCheckpoints<Request extends object, Event>(
  root: string,
  key: Buffer,
  onlyProfileScope?: string,
): Promise<RunCheckpointScan<Request, Event>> {
  if (onlyProfileScope !== undefined && !PROFILE_SCOPE.test(onlyProfileScope)) {
    throw new Error("Invalid cloud run checkpoint profile scope");
  }
  const base = join(root, "run-checkpoints");
  await ensurePrivateDirectory(base);
  const profileDirectories = await readdir(base, { withFileTypes: true }).catch(
    (error: unknown) => {
      if (nodeErrorCode(error) === "ENOENT") return [];
      throw error;
    },
  );
  const results: Array<{
    checkpointScope: string;
    checkpoint: CloudRunCheckpoint<Request, Event>;
  }> = [];
  const failures: RunCheckpointScanFailure[] = [];
  for (const profileEntry of profileDirectories.sort((left, right) =>
    left.name.localeCompare(right.name),
  )) {
    if (
      !PROFILE_SCOPE.test(profileEntry.name) ||
      (onlyProfileScope !== undefined && profileEntry.name !== onlyProfileScope)
    )
      continue;
    if (!profileEntry.isDirectory() || profileEntry.isSymbolicLink()) {
      failures.push({
        profileScope: profileEntry.name,
        code: "profile_unreadable",
      });
      continue;
    }
    const profileDirectory = join(base, profileEntry.name);
    let entries: Dirent[];
    try {
      await ensurePrivateDirectory(profileDirectory);
      entries = await readdir(profileDirectory, { withFileTypes: true });
    } catch {
      failures.push({
        profileScope: profileEntry.name,
        code: "profile_unreadable",
      });
      continue;
    }
    const checkpointEntries = entries
      .map((entry) => ({
        entry,
        matched: entry.name.match(/^([a-f0-9]{64})\.json\.enc$/),
      }))
      .filter((candidate) => candidate.matched !== null)
      .sort((left, right) => left.entry.name.localeCompare(right.entry.name));
    if (checkpointEntries.length > MAX_CHECKPOINTS_PER_PROFILE) {
      failures.push({
        profileScope: profileEntry.name,
        code: "checkpoint_limit_exceeded",
      });
      continue;
    }
    for (const { matched } of checkpointEntries) {
      const checkpointScope = matched![1]!;
      try {
        const checkpoint = await readRunCheckpoint<Request, Event>(
          root,
          profileEntry.name,
          checkpointScope,
          key,
        );
        if (checkpoint) results.push({ checkpointScope, checkpoint });
      } catch {
        failures.push({
          profileScope: profileEntry.name,
          checkpointScope,
          code: "checkpoint_unreadable",
        });
      }
    }
  }
  return { checkpoints: results, failures };
}

/**
 * Scan one audited v2 profile capability. Unlike the legacy pathname scanner,
 * this is closed-world: an unknown name, nested directory, hardlink, device
 * mismatch, or oversized file invalidates the profile scan instead of being
 * silently skipped.
 */
export async function scanManagedRunCheckpoints<Request extends object, Event>(
  storage: ManagedProfileStorage,
  key: Buffer,
): Promise<RunCheckpointScan<Request, Event>> {
  assertManagedCheckpointBinding(storage, storage.scope);
  let before: NativeRunnerInventory;
  try {
    before = await storage.checkpoints.inventory();
  } catch {
    return managedProfileScanFailure(storage.scope);
  }
  const candidates: Array<{
    checkpointScope: string;
    entry: NativeRunnerInventoryEntry;
  }> = [];
  try {
    for (const entry of before.entries) {
      const matched = MANAGED_CHECKPOINT_FILE.exec(entry.relativePath);
      if (!matched) throw new Error("Unknown managed checkpoint entry");
      assertManagedCheckpointEntry(storage, entry, entry.relativePath);
      candidates.push({ checkpointScope: matched[1]!, entry });
    }
  } catch {
    return managedProfileScanFailure(storage.scope);
  }
  if (candidates.length > MAX_CHECKPOINTS_PER_PROFILE) {
    return {
      checkpoints: [],
      failures: [
        { profileScope: storage.scope, code: "checkpoint_limit_exceeded" },
      ],
    };
  }

  const checkpoints: Array<{
    checkpointScope: string;
    checkpoint: CloudRunCheckpoint<Request, Event>;
  }> = [];
  const failures: RunCheckpointScanFailure[] = [];
  for (const { checkpointScope, entry } of candidates) {
    try {
      checkpoints.push({
        checkpointScope,
        checkpoint: await readManagedCheckpointEntry<Request, Event>(
          storage,
          entry,
          checkpointScope,
          key,
        ),
      });
    } catch {
      failures.push({
        profileScope: storage.scope,
        checkpointScope,
        code: "checkpoint_unreadable",
      });
    }
  }

  let after: NativeRunnerInventory;
  try {
    after = await storage.checkpoints.inventory();
  } catch {
    return managedProfileScanFailure(storage.scope);
  }
  if (!sameInventory(before, after))
    return managedProfileScanFailure(storage.scope);
  return { checkpoints, failures };
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

export async function removeManagedRunCheckpoint(
  storage: ManagedProfileStorage,
  browserSessionId: string,
): Promise<void> {
  assertManagedCheckpointBinding(storage, storage.scope);
  const checkpointScope = cloudCheckpointScope(storage.scope, browserSessionId);
  const fileName = managedCheckpointFileName(checkpointScope);
  await storage.checkpoints.removeEntry(fileName);
  const inventory = await storage.checkpoints.inventory();
  if (inventory.entries.some((entry) => entry.relativePath === fileName)) {
    throw new Error("Managed cloud run checkpoint removal failed");
  }
}

export function cloudCheckpointScope(
  profileScope: string,
  browserSessionId: string,
): string {
  if (
    !PROFILE_SCOPE.test(profileScope) ||
    !/^[A-Za-z0-9_-]{3,160}$/.test(browserSessionId)
  ) {
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
      if (!metadata.isDirectory() || metadata.isSymbolicLink())
        throw new Error("unsafe active profile");
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
  if (remaining.length !== 0)
    throw new Error("Orphan plaintext browser profiles remain");
  return { sealed, removed };
}

function runCheckpointPath(
  root: string,
  profileScope: string,
  checkpointScope: string,
): string {
  if (
    !PROFILE_SCOPE.test(profileScope) ||
    !CHECKPOINT_SCOPE.test(checkpointScope)
  ) {
    throw new Error("Invalid cloud run checkpoint path");
  }
  return join(
    root,
    "run-checkpoints",
    profileScope,
    `${checkpointScope}.json.enc`,
  );
}

function managedCheckpointFileName(checkpointScope: string): string {
  if (!CHECKPOINT_SCOPE.test(checkpointScope)) {
    throw new Error("Invalid managed cloud run checkpoint scope");
  }
  return `${checkpointScope}.json.enc`;
}

function assertManagedCheckpointBinding(
  storage: ManagedProfileStorage,
  profileScope: string,
): void {
  if (
    !PROFILE_SCOPE.test(profileScope) ||
    storage.kind !== "profile" ||
    storage.scope !== profileScope ||
    !/^[a-f0-9]{64}$/.test(storage.subjectSha256)
  ) {
    throw new Error("Invalid managed cloud run checkpoint profile binding");
  }
  const expected = subjectStoragePaths(storage.subjectSha256).profile(
    profileScope,
  );
  if (
    storage.paths.root.relativePath !== expected.root.relativePath ||
    storage.paths.checkpoints.relativePath !==
      expected.checkpoints.relativePath ||
    storage.root.relativePath !== expected.root.relativePath ||
    storage.checkpoints.relativePath !== expected.checkpoints.relativePath ||
    storage.root.deviceId !== storage.checkpoints.deviceId
  ) {
    throw new Error("Invalid managed cloud run checkpoint profile binding");
  }
}

function assertManagedCheckpointEntry(
  storage: ManagedProfileStorage,
  entry: NativeRunnerInventoryEntry,
  expectedFileName: string,
): void {
  if (
    entry.relativePath !== expectedFileName ||
    !MANAGED_CHECKPOINT_FILE.test(entry.relativePath) ||
    entry.kind !== "file" ||
    entry.deviceId !== storage.checkpoints.deviceId ||
    entry.linkCount !== 1 ||
    entry.sizeBytes < 1 ||
    entry.sizeBytes > MAXIMUM_MANAGED_CHECKPOINT_BYTES
  ) {
    throw new Error("Invalid managed cloud run checkpoint file");
  }
}

async function readManagedCheckpointEntry<Request extends object, Event>(
  storage: ManagedProfileStorage,
  entry: NativeRunnerInventoryEntry,
  checkpointScope: string,
  key: Buffer,
): Promise<CloudRunCheckpoint<Request, Event>> {
  const fileName = managedCheckpointFileName(checkpointScope);
  assertManagedCheckpointEntry(storage, entry, fileName);
  const encrypted = await storage.checkpoints.readFileBounded(
    fileName,
    MAXIMUM_MANAGED_CHECKPOINT_BYTES,
  );
  let plaintext: Buffer | undefined;
  try {
    if (
      encrypted.length !== entry.sizeBytes ||
      sha256(encrypted) !== entry.sha256
    ) {
      throw new Error("Managed cloud run checkpoint changed while it was read");
    }
    plaintext = decryptBytes(
      encrypted,
      key,
      encryptionContext(storage.scope, checkpointScope),
    );
    return parseCheckpoint<Request, Event>(
      plaintext,
      storage.scope,
      checkpointScope,
    );
  } finally {
    plaintext?.fill(0);
    encrypted.fill(0);
  }
}

function serializeCheckpoint(checkpoint: CloudRunCheckpoint): Buffer {
  const encoded = JSON.stringify(checkpoint);
  if (encoded === undefined)
    throw new Error("Invalid cloud run checkpoint envelope");
  const serialized = Buffer.from(`${encoded}\n`, "utf8");
  if (serialized.length > MAX_CHECKPOINT_BYTES) {
    serialized.fill(0);
    throw new Error("Cloud run checkpoint is too large");
  }
  return serialized;
}

function parseCheckpoint<Request extends object, Event>(
  plaintext: Buffer,
  profileScope: string,
  checkpointScope: string,
): CloudRunCheckpoint<Request, Event> {
  if (plaintext.length > MAX_CHECKPOINT_BYTES) {
    throw new Error("Cloud run checkpoint is too large");
  }
  const checkpoint = normalizeLegacyCheckpointRequestId(
    JSON.parse(plaintext.toString("utf8")) as unknown,
  );
  validateCheckpoint(checkpoint);
  if (
    checkpoint.profileScope !== profileScope ||
    cloudCheckpointScope(profileScope, checkpoint.browserSessionId) !==
      checkpointScope
  ) {
    throw new Error("Cloud run checkpoint scope mismatch");
  }
  return checkpoint as CloudRunCheckpoint<Request, Event>;
}

function sameInventory(
  left: NativeRunnerInventory,
  right: NativeRunnerInventory,
): boolean {
  return (
    left.count === right.count &&
    left.bytes === right.bytes &&
    left.sha256 === right.sha256 &&
    left.entries.length === right.entries.length &&
    left.entries.every((entry, index) =>
      sameInventoryEntry(entry, right.entries[index]),
    )
  );
}

function sameInventoryEntry(
  left: NativeRunnerInventoryEntry,
  right: NativeRunnerInventoryEntry | undefined,
): boolean {
  return (
    right !== undefined &&
    left.relativePath === right.relativePath &&
    left.kind === right.kind &&
    left.deviceId === right.deviceId &&
    left.linkCount === right.linkCount &&
    left.sizeBytes === right.sizeBytes &&
    left.sha256 === right.sha256
  );
}

function managedProfileScanFailure<Request extends object, Event>(
  profileScope: string,
): RunCheckpointScan<Request, Event> {
  return {
    checkpoints: [],
    failures: [{ profileScope, code: "profile_unreadable" }],
  };
}

function sha256(contents: Buffer): string {
  return createHash("sha256").update(contents).digest("hex");
}

function encryptionContext(profileScope: string, checkpointScope: string) {
  return {
    purpose: "run-checkpoint",
    scope: profileScope,
    checkpointScope,
  } as const;
}

function validateCheckpoint(
  value: unknown,
): asserts value is CloudRunCheckpoint {
  const checkpoint = requireRecord(value, "cloud run checkpoint");
  if (
    (checkpoint.version !== 1 &&
      checkpoint.version !== CURRENT_CHECKPOINT_VERSION) ||
    ![
      "prepared",
      "needs_input",
      "provider_review",
      "final_submit_started",
      "final_submit_activated",
      "side_effect_unknown",
    ].includes(String(checkpoint.phase)) ||
    !isTimestamp(checkpoint.createdAtMs) ||
    !isTimestamp(checkpoint.updatedAtMs) ||
    !isTimestamp(checkpoint.expiresAtMs) ||
    checkpoint.updatedAtMs < checkpoint.createdAtMs ||
    !PROFILE_SCOPE.test(String(checkpoint.profileScope)) ||
    !/^[A-Za-z0-9_-]{3,160}$/.test(String(checkpoint.browserSessionId))
  ) {
    throw new Error("Invalid cloud run checkpoint envelope");
  }
  const request = requireRecord(checkpoint.request, "cloud checkpoint request");
  for (const field of [
    "accountId",
    "applicationIdentityId",
    "browserSessionId",
    "runId",
    "applicationId",
  ] as const) {
    if (!/^[A-Za-z0-9_-]{3,160}$/.test(String(request[field]))) {
      throw new Error("Invalid cloud run checkpoint request");
    }
  }
  if (request.browserSessionId !== checkpoint.browserSessionId)
    throw new Error("Cloud checkpoint session mismatch");
  if (!/^[A-Za-z0-9:_-]{3,160}$/.test(String(request.browserProfileId))) {
    throw new Error("Invalid cloud run checkpoint browser profile");
  }
  const packet = requireRecord(request.packet, "cloud checkpoint packet");
  const job = requireRecord(request.job, "cloud checkpoint job");
  if (
    packet.applicationId !== request.applicationId ||
    (packet.applicationIdentityId !== undefined &&
      packet.applicationIdentityId !== request.applicationIdentityId) ||
    (packet.browserProfileId !== undefined &&
      packet.browserProfileId !== request.browserProfileId)
  ) {
    throw new Error("Cloud run checkpoint request binding mismatch");
  }
  assertApprovedExecutionChecksum(
    packet as unknown as ApplicationPacket,
    job as unknown as NormalizedJob,
  );
  const browser = requireRecord(checkpoint.browser, "cloud checkpoint browser");
  if (
    typeof browser.url !== "string" ||
    browser.url.length === 0 ||
    browser.url.length > 8_192
  ) {
    throw new Error("Invalid cloud run checkpoint browser URL");
  }
  const workflow = requireRecord(
    checkpoint.workflow,
    "cloud checkpoint workflow",
  );
  if (
    ![
      "prepared",
      "needs_input",
      "provider_review",
      "side_effect_unknown",
    ].includes(String(workflow.status)) ||
    typeof workflow.requestId !== "string" ||
    !/^[A-Za-z0-9:_-]{3,240}$/.test(workflow.requestId) ||
    request.requestId !== workflow.requestId ||
    !requestIdMatchesRun(String(request.runId), workflow.requestId)
  ) {
    throw new Error("Invalid cloud run checkpoint workflow state");
  }
  if (
    !phaseMatchesWorkflow(String(checkpoint.phase), String(workflow.status))
  ) {
    throw new Error("Invalid cloud run checkpoint phase transition");
  }
  if (!Array.isArray(checkpoint.events) || checkpoint.events.length > 10_000) {
    throw new Error("Invalid cloud run checkpoint events");
  }
  for (const event of checkpoint.events) {
    const record = requireRecord(event, "cloud run checkpoint event");
    if (
      typeof record.id !== "string" ||
      record.id.length === 0 ||
      record.id.length > 240 ||
      typeof record.type !== "string" ||
      record.type.length === 0 ||
      record.type.length > 120 ||
      (record.occurredAt !== undefined &&
        (typeof record.occurredAt !== "string" ||
          record.occurredAt.length > 64)) ||
      (record.detail !== undefined &&
        (!record.detail ||
          typeof record.detail !== "object" ||
          Array.isArray(record.detail)))
    ) {
      throw new Error("Invalid cloud run checkpoint event");
    }
  }
  const lease = requireRecord(
    checkpoint.lease,
    "cloud checkpoint lease metadata",
  );
  if (
    !Number.isSafeInteger(lease.fence) ||
    Number(lease.fence) <= 0 ||
    !isTimestamp(lease.expiresAtMs) ||
    typeof lease.ownerId !== "string" ||
    !/^[A-Za-z0-9._:-]{1,128}$/.test(lease.ownerId) ||
    (checkpoint.version === CURRENT_CHECKPOINT_VERSION &&
      (typeof lease.leaseToken !== "string" ||
        lease.leaseToken.length === 0 ||
        Buffer.byteLength(lease.leaseToken, "utf8") > 256)) ||
    (lease.leaseToken !== undefined &&
      (typeof lease.leaseToken !== "string" ||
        lease.leaseToken.length === 0 ||
        Buffer.byteLength(lease.leaseToken, "utf8") > 256)) ||
    (lease.purgeSubject !== undefined &&
      !isCanonicalPurgeSubject(lease.purgeSubject))
  ) {
    throw new Error("Invalid cloud run checkpoint lease metadata");
  }
}

async function ensurePrivateDirectory(path: string): Promise<void> {
  await mkdir(path, { recursive: true, mode: 0o700 });
  const metadata = await lstat(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink())
    throw new Error("Runner directory is unsafe");
  await chmod(path, 0o700);
  const normalized = await stat(path);
  if (
    process.platform !== "win32" &&
    ((normalized.mode & 0o077) !== 0 ||
      (typeof process.getuid === "function" &&
        normalized.uid !== process.getuid()))
  ) {
    throw new Error("Runner directory is not owner-private");
  }
}

function assertOwnerPrivate(metadata: { mode: number; uid: number }): void {
  if (
    process.platform !== "win32" &&
    ((metadata.mode & 0o077) !== 0 ||
      (typeof process.getuid === "function" &&
        metadata.uid !== process.getuid()))
  ) {
    throw new Error("Cloud run checkpoint is not owner-private");
  }
}

async function syncDirectory(path: string): Promise<void> {
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY);
    await handle.sync();
  } catch (error) {
    if (
      ![
        "EBADF",
        "EINVAL",
        "EISDIR",
        "ENOENT",
        "ENOSYS",
        "ENOTSUP",
        "EPERM",
      ].includes(nodeErrorCode(error) || "")
    )
      throw error;
  } finally {
    await handle?.close().catch(() => undefined);
  }
}

function requireRecord(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error(`Invalid ${label}`);
  return value as Record<string, unknown>;
}

function isTimestamp(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function isCanonicalPurgeSubject(value: unknown): value is string {
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(value)) {
    return false;
  }
  const decoded = Buffer.from(value, "base64url");
  return decoded.length === 32 && decoded.toString("base64url") === value;
}

function phaseMatchesWorkflow(phase: string, status: string): boolean {
  if (phase === "prepared") return status === "prepared";
  if (phase === "needs_input") return status === "needs_input";
  if (phase === "provider_review") return status === "provider_review";
  return status === "side_effect_unknown";
}

export function isWorkflowCommandRequestId(requestId: string): boolean {
  return WORKFLOW_COMMAND_REQUEST_ID.test(requestId);
}

export function requestIdMatchesRun(
  runId: string,
  requestId: string,
  resumeOnly = false,
): boolean {
  return (
    isWorkflowCommandRequestId(requestId) ||
    (!resumeOnly && requestId === `${runId}:initial`) ||
    (requestId.startsWith(`${runId}:resume:`) &&
      /^:resume:[1-6]$/.test(requestId.slice(runId.length)))
  );
}

function normalizeLegacyCheckpointRequestId(value: unknown): unknown {
  if (!value || typeof value !== "object" || Array.isArray(value)) return value;
  const checkpoint = value as Record<string, unknown>;
  if (
    checkpoint.version !== 1 &&
    checkpoint.version !== CURRENT_CHECKPOINT_VERSION
  ) {
    return value;
  }
  if (
    !checkpoint.request ||
    typeof checkpoint.request !== "object" ||
    Array.isArray(checkpoint.request) ||
    !checkpoint.workflow ||
    typeof checkpoint.workflow !== "object" ||
    Array.isArray(checkpoint.workflow)
  ) {
    return value;
  }
  const request = checkpoint.request as Record<string, unknown>;
  const workflow = checkpoint.workflow as Record<string, unknown>;
  if (
    request.requestId !== undefined ||
    typeof request.runId !== "string" ||
    typeof workflow.requestId !== "string" ||
    !requestIdMatchesRun(request.runId, workflow.requestId)
  ) {
    return value;
  }
  return {
    ...checkpoint,
    request: { ...request, requestId: workflow.requestId },
  };
}

function nodeErrorCode(error: unknown): string | undefined {
  return error && typeof error === "object" && "code" in error
    ? String((error as { code?: unknown }).code)
    : undefined;
}
