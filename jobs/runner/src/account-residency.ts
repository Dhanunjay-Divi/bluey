import { createHash } from "node:crypto";
import {
  ensurePrivateRunnerDirectory,
  listSafeRunnerDirectory,
  pathExistsNoFollow,
  readBoundedRegularFile,
  runnerPath,
  type RunnerDataRoot,
  writeDurableFileExclusive,
} from "./safe-runner-storage.js";
import {
  decodeCanonicalBase64Url,
  signEd25519,
  type RunnerVolumeIdentity,
  verifyEd25519,
} from "./volume-identity.js";

const RESIDENCY_DIRECTORY = "account-residency-v1";
const LOCATORS_DIRECTORY = "locators";
const FENCES_DIRECTORY = "purge-fences";
const JOURNALS_DIRECTORY = "purge-journals";
const TOMBSTONES_DIRECTORY = "purge-tombstones";
const LOCATOR_AUDIENCE = "bluey-jobs-runner-account-residency";
const MAXIMUM_LOCATOR_BYTES = 4 * 1024;
const MAXIMUM_LOCAL_SUBJECTS = 100_000;
const SUBJECT_BYTES = 32;
const SUBJECT_HASH_PATTERN = /^[0-9a-f]{64}$/;
const PROFILE_SCOPE_PATTERN = /^[0-9a-f]{40}$/;
const RESULT_SCOPE_PATTERN = /^[0-9a-f]{64}$/;

export type AccountResidencyErrorCode =
  | "corrupt_locator"
  | "invalid_scope"
  | "invalid_subject"
  | "locator_conflict"
  | "purged_subject";

export class AccountResidencyError extends Error {
  constructor(readonly code: AccountResidencyErrorCode) {
    super({
      corrupt_locator: "The runner account-residency index is corrupt.",
      invalid_scope: "The runner account-residency scope is invalid.",
      invalid_subject: "The runner account purge subject is invalid.",
      locator_conflict: "An immutable runner account locator conflicts with existing state.",
      purged_subject: "The runner account purge subject is fenced or tombstoned.",
    }[code]);
    this.name = "AccountResidencyError";
  }
}

export type AccountLocatorKind = "profile" | "result";

export interface AccountResidencyLocator {
  readonly version: 1;
  readonly audience: typeof LOCATOR_AUDIENCE;
  readonly volumeId: string;
  readonly volumeKeyFingerprint: string;
  readonly subjectSha256: string;
  readonly kind: AccountLocatorKind;
  readonly scope: string;
  readonly artifactFamilies: readonly string[];
  readonly signature: string;
}

const PROFILE_ARTIFACT_FAMILIES = Object.freeze([
  "active",
  "snapshots",
  "run-checkpoints",
  "receipts",
  "temporary",
]);
const RESULT_ARTIFACT_FAMILIES = Object.freeze(["step-results", "temporary"]);

export class AccountResidencyIndex {
  private readonly subjectLocks = new Map<string, Promise<void>>();
  private readonly subjectPurgeLocks = new Map<string, Promise<void>>();

  constructor(
    readonly root: RunnerDataRoot,
    readonly identity: RunnerVolumeIdentity,
  ) {}

  async registerProfile(purgeSubject: string, profileScope: string): Promise<AccountResidencyLocator> {
    assertProfileScope(profileScope);
    return this.register(purgeSubject, "profile", profileScope);
  }

  async registerResult(purgeSubject: string, resultScope: string): Promise<AccountResidencyLocator> {
    assertResultScope(resultScope);
    return this.register(purgeSubject, "result", resultScope);
  }

  async locatorsForSubjectHash(subjectSha256: string): Promise<readonly AccountResidencyLocator[]> {
    assertSubjectHash(subjectSha256);
    const directory = this.locatorDirectory(subjectSha256);
    const entries = await listSafeRunnerDirectory(this.root, directory);
    const locators: AccountResidencyLocator[] = [];
    for (const entry of entries) {
      const match = /^(profile|result)-([0-9a-f]+)\.json$/.exec(entry);
      if (!match) throw new AccountResidencyError("corrupt_locator");
      const kind = match[1] as AccountLocatorKind;
      const scope = match[2]!;
      assertLocatorScope(kind, scope, "corrupt_locator");
      const path = runnerPath(this.root, RESIDENCY_DIRECTORY, LOCATORS_DIRECTORY,
        subjectSha256, entry);
      const encoded = await readBoundedRegularFile(this.root, path, MAXIMUM_LOCATOR_BYTES);
      if (!encoded) throw new AccountResidencyError("corrupt_locator");
      const locator = this.parseLocator(encoded);
      if (locator.subjectSha256 !== subjectSha256
        || locator.kind !== kind
        || locator.scope !== scope) {
        throw new AccountResidencyError("corrupt_locator");
      }
      locators.push(locator);
    }
    return locators.sort((left, right) => (
      left.kind.localeCompare(right.kind) || left.scope.localeCompare(right.scope)
    ));
  }

  /**
   * Enumerate the complete signed locator namespace twice. This is used only
   * during startup reconciliation, before traffic is accepted; any concurrent
   * registration or unstable directory view fails closed instead of yielding
   * a partial managed-storage cutover set.
   */
  async allLocators(): Promise<readonly AccountResidencyLocator[]> {
    const first = await this.captureAllLocators();
    const second = await this.captureAllLocators();
    if (first.length !== second.length
      || first.some((locator, index) => (
        !encodeLocator(locator).equals(encodeLocator(second[index]!))
      ))) {
      throw new AccountResidencyError("corrupt_locator");
    }
    return first;
  }

  async hasPurgeBarrier(subjectSha256: string): Promise<boolean> {
    assertSubjectHash(subjectSha256);
    return await pathExistsNoFollow(this.root, this.fencePath(subjectSha256))
      || await pathExistsNoFollow(this.root, this.tombstonePath(subjectSha256));
  }

  async withSubjectLock<T>(subjectSha256: string, operation: () => Promise<T>): Promise<T> {
    return this.withLock(this.subjectLocks, subjectSha256, operation);
  }

  /**
   * Serialize complete purge attempts without excluding registrations while a
   * purge is quiescing account work. The shorter subject lock remains the
   * filesystem barrier: it installs the durable fence before quiescence and is
   * reacquired for the final inventory/delete/zero-rescan transaction.
   */
  async withSubjectPurgeLock<T>(subjectSha256: string, operation: () => Promise<T>): Promise<T> {
    return this.withLock(this.subjectPurgeLocks, subjectSha256, operation);
  }

  private async withLock<T>(
    locks: Map<string, Promise<void>>,
    subjectSha256: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    assertSubjectHash(subjectSha256);
    const predecessor = locks.get(subjectSha256) ?? Promise.resolve();
    let release = (): void => undefined;
    const current = new Promise<void>((resolve) => { release = resolve; });
    locks.set(subjectSha256, current);
    await predecessor;
    try {
      return await operation();
    } finally {
      release();
      if (locks.get(subjectSha256) === current) {
        locks.delete(subjectSha256);
      }
    }
  }

  fencePath(subjectSha256: string): string {
    assertSubjectHash(subjectSha256);
    return runnerPath(this.root, RESIDENCY_DIRECTORY, FENCES_DIRECTORY,
      `${subjectSha256}.json`);
  }

  journalPath(commandId: string): string {
    assertOpaqueIdentifier(commandId);
    const commandHash = createHash("sha256")
      .update("bluey-jobs-runner\0purge-journal-path-v1\0", "utf8")
      .update(commandId, "utf8")
      .digest("hex");
    return runnerPath(this.root, RESIDENCY_DIRECTORY, JOURNALS_DIRECTORY,
      `${commandHash}.json`);
  }

  tombstonePath(subjectSha256: string): string {
    assertSubjectHash(subjectSha256);
    return runnerPath(this.root, RESIDENCY_DIRECTORY, TOMBSTONES_DIRECTORY,
      `${subjectSha256}.json`);
  }

  async ensurePurgeDirectories(): Promise<void> {
    await ensurePrivateRunnerDirectory(this.root, RESIDENCY_DIRECTORY);
    await ensurePrivateRunnerDirectory(this.root, RESIDENCY_DIRECTORY, FENCES_DIRECTORY);
    await ensurePrivateRunnerDirectory(this.root, RESIDENCY_DIRECTORY, JOURNALS_DIRECTORY);
    await ensurePrivateRunnerDirectory(this.root, RESIDENCY_DIRECTORY, TOMBSTONES_DIRECTORY);
  }

  private async register(
    purgeSubject: string,
    kind: AccountLocatorKind,
    scope: string,
  ): Promise<AccountResidencyLocator> {
    const subjectSha256 = accountPurgeSubjectHash(purgeSubject);
    return this.withSubjectLock(subjectSha256, async () => {
      if (await this.hasPurgeBarrier(subjectSha256)) {
        throw new AccountResidencyError("purged_subject");
      }
      const locator = this.createLocator(subjectSha256, kind, scope);
      const encoded = encodeLocator(locator);
      await ensurePrivateRunnerDirectory(this.root, RESIDENCY_DIRECTORY);
      await ensurePrivateRunnerDirectory(this.root, RESIDENCY_DIRECTORY, LOCATORS_DIRECTORY);
      await ensurePrivateRunnerDirectory(this.root, RESIDENCY_DIRECTORY, LOCATORS_DIRECTORY,
        subjectSha256);
      const destination = runnerPath(this.root, RESIDENCY_DIRECTORY, LOCATORS_DIRECTORY,
        subjectSha256, locatorFileName(kind, scope));
      const created = await writeDurableFileExclusive(this.root, destination, encoded);
      if (!created) {
        const existing = await readBoundedRegularFile(
          this.root,
          destination,
          MAXIMUM_LOCATOR_BYTES,
        );
        if (!existing || !existing.equals(encoded)) {
          throw new AccountResidencyError("locator_conflict");
        }
        this.parseLocator(existing);
      }
      if (await this.hasPurgeBarrier(subjectSha256)) {
        throw new AccountResidencyError("purged_subject");
      }
      return locator;
    });
  }

  private locatorDirectory(subjectSha256: string): string {
    return runnerPath(this.root, RESIDENCY_DIRECTORY, LOCATORS_DIRECTORY, subjectSha256);
  }

  private async captureAllLocators(): Promise<readonly AccountResidencyLocator[]> {
    const subjects = await listSafeRunnerDirectory(
      this.root,
      runnerPath(this.root, RESIDENCY_DIRECTORY, LOCATORS_DIRECTORY),
    );
    if (subjects.length > MAXIMUM_LOCAL_SUBJECTS) {
      throw new AccountResidencyError("corrupt_locator");
    }
    const locators: AccountResidencyLocator[] = [];
    for (const subject of subjects) {
      if (!SUBJECT_HASH_PATTERN.test(subject)) {
        throw new AccountResidencyError("corrupt_locator");
      }
      locators.push(...await this.locatorsForSubjectHash(subject));
      if (locators.length > MAXIMUM_LOCAL_SUBJECTS) {
        throw new AccountResidencyError("corrupt_locator");
      }
    }
    return Object.freeze(locators);
  }

  private createLocator(
    subjectSha256: string,
    kind: AccountLocatorKind,
    scope: string,
  ): AccountResidencyLocator {
    const unsigned = {
      version: 1 as const,
      audience: LOCATOR_AUDIENCE as typeof LOCATOR_AUDIENCE,
      volumeId: this.identity.volumeId,
      volumeKeyFingerprint: this.identity.publicKeyFingerprint,
      subjectSha256,
      kind,
      scope,
      artifactFamilies: artifactFamilies(kind),
    };
    return {
      ...unsigned,
      signature: signEd25519(
        this.identity.privateKey,
        canonicalAccountResidencyLocatorBytes(unsigned),
      ),
    };
  }

  private parseLocator(encoded: Buffer): AccountResidencyLocator {
    try {
      const parsed: unknown = JSON.parse(encoded.toString("utf8"));
      if (!isRecord(parsed)
        || !hasExactKeys(parsed, [
          "artifactFamilies",
          "audience",
          "kind",
          "scope",
          "signature",
          "subjectSha256",
          "version",
          "volumeId",
          "volumeKeyFingerprint",
        ])
        || parsed.version !== 1
        || parsed.audience !== LOCATOR_AUDIENCE
        || parsed.volumeId !== this.identity.volumeId
        || parsed.volumeKeyFingerprint !== this.identity.publicKeyFingerprint
        || (parsed.kind !== "profile" && parsed.kind !== "result")
        || typeof parsed.scope !== "string"
        || typeof parsed.subjectSha256 !== "string"
        || typeof parsed.signature !== "string") {
        throw new AccountResidencyError("corrupt_locator");
      }
      assertSubjectHash(parsed.subjectSha256, "corrupt_locator");
      assertLocatorScope(parsed.kind, parsed.scope, "corrupt_locator");
      const expectedFamilies = artifactFamilies(parsed.kind);
      if (!Array.isArray(parsed.artifactFamilies)
        || parsed.artifactFamilies.length !== expectedFamilies.length
        || parsed.artifactFamilies.some((value, index) => value !== expectedFamilies[index])) {
        throw new AccountResidencyError("corrupt_locator");
      }
      const locator: AccountResidencyLocator = {
        version: 1,
        audience: LOCATOR_AUDIENCE,
        volumeId: parsed.volumeId,
        volumeKeyFingerprint: parsed.volumeKeyFingerprint,
        subjectSha256: parsed.subjectSha256,
        kind: parsed.kind,
        scope: parsed.scope,
        artifactFamilies: expectedFamilies,
        signature: parsed.signature,
      };
      const unsigned = {
        version: locator.version,
        audience: locator.audience,
        volumeId: locator.volumeId,
        volumeKeyFingerprint: locator.volumeKeyFingerprint,
        subjectSha256: locator.subjectSha256,
        kind: locator.kind,
        scope: locator.scope,
        artifactFamilies: locator.artifactFamilies,
      };
      if (!verifyEd25519(this.identity.publicKeyRaw, canonicalAccountResidencyLocatorBytes(unsigned),
        locator.signature)
        || !encoded.equals(encodeLocator(locator))) {
        throw new AccountResidencyError("corrupt_locator");
      }
      return locator;
    } catch (error) {
      if (error instanceof AccountResidencyError) throw error;
      throw new AccountResidencyError("corrupt_locator");
    }
  }
}

export function accountPurgeSubjectHash(purgeSubject: string): string {
  try {
    const subject = decodeCanonicalBase64Url(purgeSubject, SUBJECT_BYTES);
    return createHash("sha256").update(subject).digest("hex");
  } catch {
    throw new AccountResidencyError("invalid_subject");
  }
}

export function assertSubjectHash(
  subjectSha256: string,
  code: AccountResidencyErrorCode = "invalid_subject",
): void {
  if (typeof subjectSha256 !== "string" || !SUBJECT_HASH_PATTERN.test(subjectSha256)) {
    throw new AccountResidencyError(code);
  }
}

function assertProfileScope(scope: string): void {
  if (typeof scope !== "string" || !PROFILE_SCOPE_PATTERN.test(scope)) {
    throw new AccountResidencyError("invalid_scope");
  }
}

function assertResultScope(scope: string): void {
  if (typeof scope !== "string" || !RESULT_SCOPE_PATTERN.test(scope)) {
    throw new AccountResidencyError("invalid_scope");
  }
}

function assertLocatorScope(
  kind: AccountLocatorKind,
  scope: string,
  code: AccountResidencyErrorCode,
): void {
  try {
    if (kind === "profile") assertProfileScope(scope);
    else assertResultScope(scope);
  } catch {
    throw new AccountResidencyError(code);
  }
}

function assertOpaqueIdentifier(value: string): void {
  if (typeof value !== "string"
    || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(value)) {
    throw new AccountResidencyError("invalid_subject");
  }
}

function artifactFamilies(kind: AccountLocatorKind): readonly string[] {
  return kind === "profile" ? PROFILE_ARTIFACT_FAMILIES : RESULT_ARTIFACT_FAMILIES;
}

function locatorFileName(kind: AccountLocatorKind, scope: string): string {
  return `${kind}-${scope}.json`;
}

export function canonicalAccountResidencyLocatorBytes(
  locator: Omit<AccountResidencyLocator, "signature">,
): Buffer {
  return Buffer.from([
    "bluey-jobs-runner-account-residency-v1",
    `version=${locator.version}`,
    `audience=${locator.audience}`,
    `volume_id=${locator.volumeId}`,
    `volume_key_fingerprint=${locator.volumeKeyFingerprint}`,
    `subject_sha256=${locator.subjectSha256}`,
    `kind=${locator.kind}`,
    `scope=${locator.scope}`,
    `artifact_families=${locator.artifactFamilies.join(",")}`,
    "",
  ].join("\n"), "utf8");
}

/**
 * Verify a locator supplied across the current-storage attestation boundary.
 * The durable index parser additionally proves its on-disk JSON encoding; this
 * helper proves the complete typed value, immutable artifact-family contract,
 * volume binding, and Ed25519 signature before it enters an aggregate digest.
 */
export function assertAccountResidencyLocator(
  locator: AccountResidencyLocator,
  identity: Pick<
    RunnerVolumeIdentity,
    "publicKeyFingerprint" | "publicKeyRaw" | "volumeId"
  >,
): void {
  try {
    if (
      !isRecord(locator) ||
      !hasExactKeys(locator, [
        "artifactFamilies",
        "audience",
        "kind",
        "scope",
        "signature",
        "subjectSha256",
        "version",
        "volumeId",
        "volumeKeyFingerprint",
      ]) ||
      locator.version !== 1 ||
      locator.audience !== LOCATOR_AUDIENCE ||
      locator.volumeId !== identity.volumeId ||
      locator.volumeKeyFingerprint !== identity.publicKeyFingerprint ||
      (locator.kind !== "profile" && locator.kind !== "result") ||
      typeof locator.scope !== "string" ||
      typeof locator.subjectSha256 !== "string" ||
      typeof locator.signature !== "string"
    ) {
      throw new Error("invalid locator");
    }
    assertSubjectHash(locator.subjectSha256, "corrupt_locator");
    assertLocatorScope(locator.kind, locator.scope, "corrupt_locator");
    const expectedFamilies = artifactFamilies(locator.kind);
    if (
      !Array.isArray(locator.artifactFamilies) ||
      locator.artifactFamilies.length !== expectedFamilies.length ||
      locator.artifactFamilies.some(
        (value, index) => value !== expectedFamilies[index],
      )
    ) {
      throw new Error("invalid locator families");
    }
    const { signature, ...unsigned } = locator;
    if (
      !verifyEd25519(
        identity.publicKeyRaw,
        canonicalAccountResidencyLocatorBytes(unsigned),
        signature,
      )
    ) {
      throw new Error("invalid locator signature");
    }
  } catch (error) {
    if (error instanceof AccountResidencyError) throw error;
    throw new AccountResidencyError("corrupt_locator");
  }
}

function encodeLocator(locator: AccountResidencyLocator): Buffer {
  return Buffer.from(`${JSON.stringify(locator)}\n`, "utf8");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value).sort((left, right) => left.localeCompare(right));
  const wanted = [...expected].sort((left, right) => left.localeCompare(right));
  return keys.length === wanted.length && keys.every((key, index) => key === wanted[index]);
}
