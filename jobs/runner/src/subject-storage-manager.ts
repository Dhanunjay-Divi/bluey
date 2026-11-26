import { createHash } from "node:crypto";
import type {
  AccountResidencyIndex,
  AccountResidencyLocator,
} from "./account-residency.js";
import { assertAccountResidencyLocator } from "./account-residency.js";
import type {
  NativeRunnerInventory,
  NativeRunnerInventoryEntry,
  NativeRunnerStorageDirectory,
  NativeRunnerStorageRoot,
} from "./native-runner-storage.js";
import {
  classifyLegacyRunnerInventory,
  type LegacyRunnerStorageInventory,
} from "./legacy-runner-storage.js";
import { legacyTargetArtifacts } from "./purge-storage-evidence.js";
import {
  ACCOUNT_DATA_V2_DIRECTORY,
  EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
  LEGACY_ARTIFACT_ROOTS,
  SCOPE_OWNERS_DIRECTORY,
  SUBJECT_STORAGE_LAYOUT_VERSION,
  SUBJECT_METADATA_FILE,
  SUBJECTS_DIRECTORY,
  auditSubjectStorageLayout,
  createSignedScopeOwnership,
  createSignedSubjectMetadata,
  encodeScopeOwnership,
  encodeSubjectMetadata,
  parseScopeOwnership,
  parseSubjectMetadata,
  profileScope,
  resultScope,
  scopeOwnershipPath,
  subjectSha256,
  subjectStoragePaths,
  type AuditedSubjectStorage,
  type NativeAccountDataV2Scan,
  type NativeRunnerRootEntryEvidence,
  type NativeScannerRecord,
  type ProfileStoragePaths,
  type ResultStoragePaths,
  type ScopeKind,
  type SignedScopeOwnership,
  type SubjectStorageAuditInput,
  type SubjectStorageAuditRejected,
  type SubjectStorageAuditResult,
  type SubjectStorageAuditSuccess,
  type SubjectStorageInventoryEvidence,
} from "./subject-storage-layout.js";
import type { RunnerVolumeIdentity } from "./volume-identity.js";

const MAXIMUM_SIGNED_CONTROL_BYTES = 8 * 1024;
const ABSENT_AUDIT_TARGET = "0".repeat(64);
const LOCATOR_AUDIENCE = "bluey-jobs-runner-account-residency";

const PROFILE_ARTIFACT_FAMILIES = Object.freeze([
  "active",
  "snapshots",
  "run-checkpoints",
  "receipts",
  "temporary",
]);
const RESULT_ARTIFACT_FAMILIES = Object.freeze(["step-results", "temporary"]);

export type SubjectStorageManagerErrorCode =
  | "audit_changed"
  | "audit_rejected"
  | "corrupt_storage"
  | "invalid_locator"
  | "locator_set_incomplete"
  | "owner_conflict"
  | "purge_barrier_required"
  | "purged_subject"
  | "restored_subject";

export class SubjectStorageManagerError extends Error {
  constructor(
    readonly code: SubjectStorageManagerErrorCode,
    readonly audit?: SubjectStorageAuditRejected,
    options?: ErrorOptions,
  ) {
    super(
      {
        audit_changed:
          "The runner storage namespace changed while it was being audited.",
        audit_rejected:
          "The runner subject-storage audit rejected the retained namespace.",
        corrupt_storage: "The runner subject-storage namespace is corrupt.",
        invalid_locator:
          "The supplied runner account-residency locator is not verified.",
        locator_set_incomplete:
          "The supplied runner account-residency locator set is incomplete.",
        owner_conflict:
          "The immutable runner scope owner conflicts with the requested subject.",
        purge_barrier_required:
          "A durable purge barrier is required before subject removal.",
        purged_subject: "The runner subject is fenced or tombstoned.",
        restored_subject:
          "A fenced or tombstoned runner subject became resident again.",
      }[code],
      options,
    );
    this.name = "SubjectStorageManagerError";
  }
}

/**
 * The manager intentionally needs only the signed-index authority surface.
 * `AccountResidencyIndex` satisfies this interface; tests can inject a memory
 * authority without loading the native addon or touching a host pathname.
 */
export type SubjectStorageResidencyAuthority = Pick<
  AccountResidencyIndex,
  "hasPurgeBarrier" | "identity" | "locatorsForSubjectHash" | "withSubjectLock"
>;

export interface ManagedProfileStorage {
  readonly kind: "profile";
  readonly subjectSha256: string;
  readonly scope: string;
  readonly paths: ProfileStoragePaths;
  readonly root: NativeRunnerStorageDirectory;
  readonly active: NativeRunnerStorageDirectory;
  readonly snapshots: NativeRunnerStorageDirectory;
  readonly checkpoints: NativeRunnerStorageDirectory;
  readonly receipts: NativeRunnerStorageDirectory;
  readonly temporary: NativeRunnerStorageDirectory;
}

export interface ManagedResultStorage {
  readonly kind: "result";
  readonly subjectSha256: string;
  readonly scope: string;
  readonly paths: ResultStoragePaths;
  readonly root: NativeRunnerStorageDirectory;
  readonly temporary: NativeRunnerStorageDirectory;
}

export interface SubjectStorageInventory {
  readonly residency: "never_resident" | "resident";
  readonly subject: AuditedSubjectStorage | null;
  /** Exact target subtree plus its immutable global owner records. */
  readonly inventory: SubjectStorageInventoryEvidence;
  /** Exact complete account-data-v2 inventory from the same native snapshot. */
  readonly completeInventory: SubjectStorageInventoryEvidence;
  /** Exact complete retained runner-root inventory from the same stable snapshot. */
  readonly runnerRootInventory: RunnerRootInventoryEvidence;
  /** Legacy and unknown-root classification derived from that identical snapshot. */
  readonly legacyInventory: LegacyRunnerStorageInventory;
}

export interface RunnerRootInventoryEvidence {
  readonly deviceId: string;
  readonly linkCount: number;
  readonly entryCount: number;
  readonly fileBytes: string;
  readonly sha256: string;
}

export interface SubjectStorageReconciliation {
  readonly createdOwners: number;
  readonly reconciledLocators: number;
  readonly skippedPurgedSubjects: readonly string[];
}

export interface SubjectStorageRemoval {
  /** Null only when resuming a deletion whose clean before-evidence must come from its journal. */
  readonly before: SubjectStorageInventory | null;
  readonly after: SubjectStorageInventory;
}

export interface CurrentStorageSubjectEvidence {
  readonly subjectSha256: string;
  readonly profileScopeCount: number;
  readonly resultScopeCount: number;
  readonly subjectTree: SubjectStorageInventoryEvidence;
  readonly ownership: SubjectStorageInventoryEvidence;
}

/**
 * One closed-world snapshot used by the volume storage attestation. The root,
 * v2 subject set, complete v2 inventory, and legacy classification are all
 * derived from the same retained native-root inventory.
 */
export interface CurrentSubjectStorageInventory {
  readonly root: RunnerRootInventoryEvidence;
  readonly subjectStorage: {
    readonly layoutVersion: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
    readonly subjects: readonly CurrentStorageSubjectEvidence[];
    readonly subjectCount: number;
    readonly subjectSetSha256: string;
    readonly scopeCount: number;
    readonly completeRoot: SubjectStorageInventoryEvidence;
  };
  readonly legacy: LegacyRunnerStorageInventory;
}

/**
 * Ephemeral capability available only while the residency subject lock and
 * the manager's native-root operation lock are held. Purge code uses this to
 * combine legacy control records and v2 evidence without reacquiring the same
 * non-reentrant subject lock.
 */
export interface LockedSubjectStorage {
  readonly subjectSha256: string;
  audit(): Promise<SubjectStorageAuditResult>;
  inventory(): Promise<SubjectStorageInventory>;
  removeLegacyTargets(
    locators: readonly AccountResidencyLocator[],
    afterRemoval?: () => Promise<void>,
  ): Promise<SubjectStorageInventory>;
  remove(): Promise<SubjectStorageRemoval>;
}

interface LayoutSnapshot {
  readonly input: SubjectStorageAuditInput;
  readonly controlBytes: ReadonlyMap<string, Buffer>;
  readonly nativeInventory: NativeRunnerInventory;
  readonly rootDeviceId: string;
  readonly rootLinkCount: number;
}

interface EnsuredScope<Capability> {
  readonly capability: Capability;
  readonly createdOwner: boolean;
}

interface ScopeDefinition {
  readonly kind: ScopeKind;
  readonly subjectSha256: string;
  readonly scope: string;
  readonly scopeRootPath: string;
  readonly ownerPath: string;
  readonly requiredDirectories: readonly string[];
}

/**
 * Owns the v1-intent to v2-publication boundary for one exclusively locked
 * native runner root. Every v2 path is passed as validated components to a
 * retained native capability; canonical path strings are evidence only.
 */
export class SubjectStorageManager {
  private operationTail: Promise<void> = Promise.resolve();

  constructor(
    readonly root: NativeRunnerStorageRoot,
    readonly identity: RunnerVolumeIdentity,
    readonly residency: SubjectStorageResidencyAuthority,
  ) {
    if (
      identity.volumeId !== residency.identity.volumeId ||
      identity.publicKeyRaw !== residency.identity.publicKeyRaw ||
      identity.publicKeyFingerprint !== residency.identity.publicKeyFingerprint
    ) {
      throw new SubjectStorageManagerError("corrupt_storage");
    }
    // Creating and immediately verifying one deterministic record proves that
    // the supplied private key is the mate of the bound public identity.
    const identityProbe = createSignedSubjectMetadata(
      identity,
      ABSENT_AUDIT_TARGET,
    );
    parseSubjectMetadata(encodeSubjectMetadata(identityProbe), identity);
  }

  async ensureProfile(
    locator: AccountResidencyLocator,
  ): Promise<ManagedProfileStorage> {
    this.assertLocator(locator, "profile");
    return this.withSubjectOperation(
      locator.subjectSha256,
      async () => (await this.ensureProfileLocked(locator)).capability,
    );
  }

  async ensureResult(
    locator: AccountResidencyLocator,
  ): Promise<ManagedResultStorage> {
    this.assertLocator(locator, "result");
    return this.withSubjectOperation(
      locator.subjectSha256,
      async () => (await this.ensureResultLocked(locator)).capability,
    );
  }

  async resolveProfile(
    scopeValue: string,
  ): Promise<ManagedProfileStorage | null> {
    const scope = profileScope(scopeValue);
    return this.resolveScope("profile", scope, (subject) =>
      this.openProfile(subject, scope),
    );
  }

  async resolveResult(
    scopeValue: string,
  ): Promise<ManagedResultStorage | null> {
    const scope = resultScope(scopeValue);
    return this.resolveScope("result", scope, (subject) =>
      this.openResult(subject, scope),
    );
  }

  async auditSubject(subjectValue: string): Promise<SubjectStorageAuditResult> {
    const subject = subjectSha256(subjectValue);
    return this.withSubjectOperation(subject, () =>
      this.auditSubjectLocked(subject),
    );
  }

  async inventorySubject(
    subjectValue: string,
  ): Promise<SubjectStorageInventory> {
    const subject = subjectSha256(subjectValue);
    return this.withSubjectOperation(subject, () =>
      this.inventorySubjectLocked(subject),
    );
  }

  async inventoryCurrentStorage(
    locators: readonly AccountResidencyLocator[],
  ): Promise<CurrentSubjectStorageInventory> {
    if (!Array.isArray(locators)) {
      throw new SubjectStorageManagerError("invalid_locator");
    }
    return this.serialized(async () => {
      const grouped = new Map<string, AccountResidencyLocator[]>();
      const locatorKeys = new Set<string>();
      for (const locator of locators) {
        try {
          assertAccountResidencyLocator(locator, this.identity);
        } catch (error) {
          throw new SubjectStorageManagerError("invalid_locator", undefined, {
            cause: error,
          });
        }
        const key = `${locator.subjectSha256}\0${locator.kind}\0${locator.scope}`;
        if (locatorKeys.has(key)) {
          throw new SubjectStorageManagerError("locator_set_incomplete");
        }
        locatorKeys.add(key);
        const subjectLocators = grouped.get(locator.subjectSha256) ?? [];
        subjectLocators.push(locator);
        grouped.set(locator.subjectSha256, subjectLocators);
      }

      const barrierBefore = new Map<string, boolean>();
      for (const subject of [...grouped.keys()].sort(compareCanonicalUtf8)) {
        const subjectLocators = grouped.get(subject)!.sort(compareLocators);
        await this.assertCompletePersistedLocatorSet(subject, subjectLocators);
        barrierBefore.set(
          subject,
          await this.residency.hasPurgeBarrier(subject),
        );
      }

      const snapshot = await this.captureLayoutSnapshot(ABSENT_AUDIT_TARGET);
      const audit = this.requireSuccessfulAudit(
        this.auditPurgeSnapshot(snapshot),
      );

      for (const [subject, before] of barrierBefore) {
        if ((await this.residency.hasPurgeBarrier(subject)) !== before) {
          throw new SubjectStorageManagerError("audit_changed");
        }
      }

      const auditedSubjects = new Map(
        audit.subjects.map((subject) => [subject.subjectSha256, subject]),
      );
      for (const subject of audit.subjects) {
        if (!grouped.has(subject.subjectSha256)) {
          throw new SubjectStorageManagerError("locator_set_incomplete");
        }
      }

      const subjects: CurrentStorageSubjectEvidence[] = [];
      for (const subjectValue of [...grouped.keys()].sort(compareCanonicalUtf8)) {
        const subjectLocators = grouped.get(subjectValue)!;
        const audited = auditedSubjects.get(subjectSha256(subjectValue));
        if (barrierBefore.get(subjectValue)) {
          if (audited) {
            throw new SubjectStorageManagerError("restored_subject");
          }
          continue;
        }
        if (!audited) {
          throw new SubjectStorageManagerError("locator_set_incomplete");
        }
        const expectedProfiles = subjectLocators
          .filter((locator) => locator.kind === "profile")
          .map((locator) => locator.scope)
          .sort(compareCanonicalUtf8);
        const expectedResults = subjectLocators
          .filter((locator) => locator.kind === "result")
          .map((locator) => locator.scope)
          .sort(compareCanonicalUtf8);
        if (
          !stringArraysEqual(audited.profileScopes, expectedProfiles) ||
          !stringArraysEqual(audited.resultScopes, expectedResults)
        ) {
          throw new SubjectStorageManagerError("locator_set_incomplete");
        }
        subjects.push(
          Object.freeze({
            subjectSha256: subjectValue,
            profileScopeCount: audited.profileScopes.length,
            resultScopeCount: audited.resultScopes.length,
            subjectTree: audited.subjectTreeInventory,
            ownership: audited.scopeOwnershipInventory,
          }),
        );
      }

      const scopeCount = subjects.reduce(
        (count, subject) =>
          count + subject.profileScopeCount + subject.resultScopeCount,
        0,
      );
      const legacy = classifyLegacyRunnerInventory(
        snapshot.nativeInventory,
        snapshot.rootDeviceId,
        snapshot.rootLinkCount,
      );
      return Object.freeze({
        root: runnerRootInventoryFromSnapshot(snapshot),
        subjectStorage: Object.freeze({
          layoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
          subjects: Object.freeze(subjects),
          subjectCount: subjects.length,
          subjectSetSha256: currentStorageSubjectSetSha256(subjects),
          scopeCount,
          completeRoot: audit.inventory,
        }),
        legacy,
      });
    });
  }

  async withLockedSubject<T>(
    subjectValue: string,
    operation: (storage: LockedSubjectStorage) => Promise<T>,
  ): Promise<T> {
    const subject = subjectSha256(subjectValue);
    if (typeof operation !== "function") {
      throw new SubjectStorageManagerError("corrupt_storage");
    }
    return this.withSubjectOperation(subject, async () => {
      let active = true;
      const calls: Promise<unknown>[] = [];
      const requireActive = (): void => {
        if (!active) throw new SubjectStorageManagerError("corrupt_storage");
      };
      const invoke = <Value>(call: () => Promise<Value>): Promise<Value> => {
        requireActive();
        const pending = Promise.resolve().then(call);
        calls.push(pending);
        void pending.catch(() => undefined);
        return pending;
      };
      const storage: LockedSubjectStorage = Object.freeze({
        subjectSha256: subject,
        audit: async () => invoke(() => this.auditSubjectLocked(subject, true)),
        inventory: async () =>
          invoke(() => this.inventorySubjectLocked(subject, true)),
        removeLegacyTargets: async (
          locators: readonly AccountResidencyLocator[],
          afterRemoval?: () => Promise<void>,
        ) =>
          invoke(() =>
            this.removeLegacyTargetsLocked(subject, locators, afterRemoval),
          ),
        remove: async () =>
          invoke(() => this.removeSubjectLocked(subject, true)),
      });
      let succeeded = false;
      let result: T | undefined;
      let callbackError: unknown;
      try {
        result = await operation(storage);
        succeeded = true;
      } catch (error) {
        callbackError = error;
      } finally {
        active = false;
      }
      const settled = await Promise.allSettled(calls);
      if (!succeeded) throw callbackError;
      const failed = settled.find((outcome) => outcome.status === "rejected");
      if (failed?.status === "rejected") throw failed.reason;
      return result as T;
    });
  }

  async reconcileLocators(
    locators: readonly AccountResidencyLocator[],
  ): Promise<SubjectStorageReconciliation> {
    if (!Array.isArray(locators)) {
      throw new SubjectStorageManagerError("invalid_locator");
    }
    const groups = new Map<string, AccountResidencyLocator[]>();
    const seen = new Set<string>();
    for (const locator of locators) {
      this.assertLocator(locator);
      const key = `${locator.subjectSha256}\0${locator.kind}\0${locator.scope}`;
      if (seen.has(key))
        throw new SubjectStorageManagerError("locator_set_incomplete");
      seen.add(key);
      const group = groups.get(locator.subjectSha256) ?? [];
      group.push(locator);
      groups.set(locator.subjectSha256, group);
    }

    let createdOwners = 0;
    const skippedPurgedSubjects: string[] = [];
    for (const subject of [...groups.keys()].sort()) {
      const group = groups.get(subject)!.sort(compareLocators);
      await this.withSubjectOperation(subject, async () => {
        await this.assertCompletePersistedLocatorSet(subject, group);
        if (await this.residency.hasPurgeBarrier(subject)) {
          const audit = this.requireSuccessfulAudit(
            this.auditPurgeSnapshot(await this.captureLayoutSnapshot(subject)),
          );
          if (audit.target.residency !== "never_resident") {
            throw new SubjectStorageManagerError("restored_subject");
          }
          skippedPurgedSubjects.push(subject);
          return;
        }
        for (const locator of group) {
          const ensured =
            locator.kind === "profile"
              ? await this.ensureProfileLocked(locator, true)
              : await this.ensureResultLocked(locator, true);
          if (ensured.createdOwner) createdOwners += 1;
        }
      });
    }
    await this.serialized(async () => {
      this.requireSuccessfulAudit(
        this.auditPurgeSnapshot(
          await this.captureLayoutSnapshot(ABSENT_AUDIT_TARGET),
        ),
      );
    });
    return Object.freeze({
      createdOwners,
      reconciledLocators: locators.length,
      skippedPurgedSubjects: Object.freeze(skippedPurgedSubjects),
    });
  }

  async removeSubject(subjectValue: string): Promise<SubjectStorageRemoval> {
    const subject = subjectSha256(subjectValue);
    return this.withSubjectOperation(subject, () =>
      this.removeSubjectLocked(subject),
    );
  }

  private async auditSubjectLocked(
    subject: string,
    allowLegacyRoots = false,
  ): Promise<SubjectStorageAuditResult> {
    const snapshot = await this.captureLayoutSnapshot(subject);
    return allowLegacyRoots
      ? this.auditPurgeSnapshot(snapshot)
      : this.auditSnapshot(snapshot);
  }

  private async inventorySubjectLocked(
    subject: string,
    allowLegacyRoots = false,
  ): Promise<SubjectStorageInventory> {
    const snapshot = await this.captureLayoutSnapshot(subject);
    const audit = this.requireSuccessfulAudit(
      allowLegacyRoots
        ? this.auditPurgeSnapshot(snapshot)
        : this.auditSnapshot(snapshot),
    );
    return inventoryFromAudit(audit, snapshot);
  }

  private async removeSubjectLocked(
    subject: string,
    allowLegacyRoots = false,
  ): Promise<SubjectStorageRemoval> {
    if (!(await this.residency.hasPurgeBarrier(subject))) {
      throw new SubjectStorageManagerError("purge_barrier_required");
    }
    const locators = await this.residency.locatorsForSubjectHash(subject);
    for (const locator of locators) this.assertLocator(locator);
    await this.assertCompletePersistedLocatorSet(subject, locators);

    // A crash may occur after v2 creation but before its global empty
    // scaffold is complete. Repairing only this non-account scaffold does
    // not recreate a fenced subject and makes the subsequent proof closed.
    await this.ensureGlobalLayoutScaffold();
    const beforeSnapshot = await this.captureLayoutSnapshot(subject);
    const beforeAudit = allowLegacyRoots
      ? this.auditPurgeSnapshot(beforeSnapshot)
      : this.auditSnapshot(beforeSnapshot);
    const expectedOwners = new Map<string, Buffer>();
    for (const locator of locators) {
      const owner = createSignedScopeOwnership(
        this.identity,
        subject,
        locator.kind,
        locator.scope,
      );
      expectedOwners.set(owner.ownershipPath, encodeScopeOwnership(owner));
    }
    this.assertTargetOwnerSet(beforeSnapshot, subject, expectedOwners);
    this.assertRemovalState(
      beforeSnapshot,
      beforeAudit,
      subject,
      expectedOwners,
      allowLegacyRoots,
    );
    const before = beforeAudit.ok
      ? inventoryFromAudit(beforeAudit, beforeSnapshot)
      : null;

    const subjectEntry = scannerEntry(
      beforeSnapshot.input,
      `${SUBJECTS_DIRECTORY}/${subject}`,
    );
    if (subjectEntry) {
      const subjects = await this.root.openDirectory([
        ACCOUNT_DATA_V2_DIRECTORY,
        SUBJECTS_DIRECTORY,
      ]);
      await subjects.removeEntry(subject);
    }
    for (const ownerPath of [...expectedOwners.keys()].sort()) {
      if (!scannerEntry(beforeSnapshot.input, stripLayoutRoot(ownerPath)))
        continue;
      const components = ownerPath.split("/");
      const ownerDirectory = await this.root.openDirectory(
        components.slice(0, -1),
      );
      await ownerDirectory.removeEntry(components.at(-1)!);
    }

    if (!(await this.residency.hasPurgeBarrier(subject))) {
      throw new SubjectStorageManagerError("purge_barrier_required");
    }
    const afterSnapshot = await this.captureLayoutSnapshot(subject);
    const afterAudit = this.requireSuccessfulAudit(
      allowLegacyRoots
        ? this.auditPurgeSnapshot(afterSnapshot)
        : this.auditSnapshot(afterSnapshot),
    );
    if (afterAudit.target.residency !== "never_resident") {
      throw new SubjectStorageManagerError("restored_subject");
    }
    return Object.freeze({
      before,
      after: inventoryFromAudit(afterAudit, afterSnapshot),
    });
  }

  private async removeLegacyTargetsLocked(
    subject: string,
    locatorsInput: readonly AccountResidencyLocator[],
    afterRemoval?: () => Promise<void>,
  ): Promise<SubjectStorageInventory> {
    if (!(await this.residency.hasPurgeBarrier(subject))) {
      throw new SubjectStorageManagerError("purge_barrier_required");
    }
    if (!Array.isArray(locatorsInput)) {
      throw new SubjectStorageManagerError("invalid_locator");
    }
    if (afterRemoval !== undefined && typeof afterRemoval !== "function") {
      throw new SubjectStorageManagerError("corrupt_storage");
    }
    const locators = [...locatorsInput].sort(compareLocators);
    const locatorKeys = new Set<string>();
    for (const locator of locators) {
      this.assertLocator(locator);
      if (locator.subjectSha256 !== subject) {
        throw new SubjectStorageManagerError("invalid_locator");
      }
      const key = `${locator.kind}\0${locator.scope}`;
      if (locatorKeys.has(key)) {
        throw new SubjectStorageManagerError("locator_set_incomplete");
      }
      locatorKeys.add(key);
    }
    await this.assertCompletePersistedLocatorSet(subject, locators);

    const beforeSnapshot = await this.captureLayoutSnapshot(subject);
    this.requireSuccessfulAudit(this.auditPurgeSnapshot(beforeSnapshot));
    const beforeLegacy = classifyLegacyRunnerInventory(
      beforeSnapshot.nativeInventory,
      beforeSnapshot.rootDeviceId,
      beforeSnapshot.rootLinkCount,
    );
    if (beforeLegacy.unclassifiedRootPaths.length !== 0) {
      throw new SubjectStorageManagerError("corrupt_storage");
    }
    const legacyRoots = new Set<string>(LEGACY_ARTIFACT_ROOTS);
    const targets = legacyTargetArtifacts(beforeLegacy, locators);
    const targetPaths = new Set(targets.map((target) => target.relativePath));
    const selectedChildren = new Map<string, Set<string>>();
    for (const target of targets) {
      const components = target.relativePath.split("/");
      if (
        components.length < 2 ||
        !legacyRoots.has(components[0]!)
      ) {
        throw new SubjectStorageManagerError("corrupt_storage");
      }
      const children = selectedChildren.get(components[0]!) ?? new Set<string>();
      children.add(components[1]!);
      selectedChildren.set(components[0]!, children);
    }
    for (const artifact of beforeLegacy.artifacts) {
      for (const [rootName, children] of selectedChildren) {
        for (const childName of children) {
          const selectedPath = `${rootName}/${childName}`;
          if (
            (artifact.relativePath === selectedPath ||
              artifact.relativePath.startsWith(`${selectedPath}/`)) &&
            !targetPaths.has(artifact.relativePath)
          ) {
            throw new SubjectStorageManagerError("corrupt_storage");
          }
        }
      }
    }

    for (const rootName of [...selectedChildren.keys()].sort(compareCanonicalUtf8)) {
      const directory = await this.root.openDirectory([rootName]);
      for (const childName of [...selectedChildren.get(rootName)!].sort(
        compareCanonicalUtf8,
      )) {
        await directory.removeEntry(childName);
        await afterRemoval?.();
      }
    }

    const afterTargetSnapshot = await this.captureLayoutSnapshot(subject);
    this.requireSuccessfulAudit(this.auditPurgeSnapshot(afterTargetSnapshot));
    const afterTargetLegacy = classifyLegacyRunnerInventory(
      afterTargetSnapshot.nativeInventory,
      afterTargetSnapshot.rootDeviceId,
      afterTargetSnapshot.rootLinkCount,
    );
    if (
      afterTargetLegacy.unclassifiedRootPaths.length !== 0 ||
      legacyTargetArtifacts(afterTargetLegacy, locators).length !== 0
    ) {
      throw new SubjectStorageManagerError("corrupt_storage");
    }

    const rootEntries = new Map(
      afterTargetSnapshot.nativeInventory.entries
        .filter((entry) => !entry.relativePath.includes("/"))
        .map((entry) => [entry.relativePath, entry]),
    );
    const rootDirectory = await this.root.openDirectory([]);
    for (const rootName of [...LEGACY_ARTIFACT_ROOTS].sort(compareCanonicalUtf8)) {
      const entry = rootEntries.get(rootName);
      if (!entry) continue;
      if (
        entry.kind !== "directory" ||
        entry.deviceId !== afterTargetSnapshot.rootDeviceId ||
        afterTargetSnapshot.nativeInventory.entries.some((candidate) =>
          candidate.relativePath.startsWith(`${rootName}/`),
        )
      ) {
        continue;
      }
      await rootDirectory.removeEntry(rootName);
    }

    if (!(await this.residency.hasPurgeBarrier(subject))) {
      throw new SubjectStorageManagerError("purge_barrier_required");
    }
    const finalSnapshot = await this.captureLayoutSnapshot(subject);
    const finalAudit = this.requireSuccessfulAudit(
      this.auditPurgeSnapshot(finalSnapshot),
    );
    const finalLegacy = classifyLegacyRunnerInventory(
      finalSnapshot.nativeInventory,
      finalSnapshot.rootDeviceId,
      finalSnapshot.rootLinkCount,
    );
    if (
      finalLegacy.unclassifiedRootPaths.length !== 0 ||
      legacyTargetArtifacts(finalLegacy, locators).length !== 0
    ) {
      throw new SubjectStorageManagerError("corrupt_storage");
    }
    return inventoryFromAudit(finalAudit, finalSnapshot);
  }

  private async ensureProfileLocked(
    locator: AccountResidencyLocator,
    allowLegacyRoots = false,
  ): Promise<EnsuredScope<ManagedProfileStorage>> {
    return this.ensureScopeLocked(
      locator,
      async () => this.openProfile(locator.subjectSha256, locator.scope),
      allowLegacyRoots,
    );
  }

  private async ensureResultLocked(
    locator: AccountResidencyLocator,
    allowLegacyRoots = false,
  ): Promise<EnsuredScope<ManagedResultStorage>> {
    return this.ensureScopeLocked(
      locator,
      async () => this.openResult(locator.subjectSha256, locator.scope),
      allowLegacyRoots,
    );
  }

  private async ensureScopeLocked<Capability>(
    locator: AccountResidencyLocator,
    openCapability: () => Promise<Capability>,
    allowLegacyRoots = false,
  ): Promise<EnsuredScope<Capability>> {
    if (await this.residency.hasPurgeBarrier(locator.subjectSha256)) {
      throw new SubjectStorageManagerError("purged_subject");
    }
    await this.assertPersistedLocator(locator);
    const definition = scopeDefinition(locator);
    const expectedOwner = createSignedScopeOwnership(
      this.identity,
      locator.subjectSha256,
      locator.kind,
      locator.scope,
    );
    const expectedOwnerBytes = encodeScopeOwnership(expectedOwner);
    const before = await this.captureLayoutSnapshot(locator.subjectSha256);
    const existingOwner = before.controlBytes.get(definition.ownerPath);
    if (existingOwner) {
      this.assertExactOwner(existingOwner, expectedOwner, expectedOwnerBytes);
      this.assertAuditContainsScope(
        this.requireSuccessfulAudit(
          allowLegacyRoots
            ? this.auditPurgeSnapshot(before)
            : this.auditSnapshot(before),
        ),
        definition,
      );
      if (await this.residency.hasPurgeBarrier(locator.subjectSha256)) {
        throw new SubjectStorageManagerError("purged_subject");
      }
      const capability = await openCapability();
      if (await this.residency.hasPurgeBarrier(locator.subjectSha256)) {
        throw new SubjectStorageManagerError("purged_subject");
      }
      return Object.freeze({ capability, createdOwner: false });
    }

    this.assertRepairableRegistration(before, definition, allowLegacyRoots);
    await this.ensureGlobalLayoutScaffold();
    const subjectPaths = subjectStoragePaths(locator.subjectSha256);
    const subjectDirectory = await this.root.ensureDirectory(
      subjectPaths.root.components,
    );
    await subjectDirectory.ensureChildDirectory("profiles");
    await subjectDirectory.ensureChildDirectory("results");
    const metadata = createSignedSubjectMetadata(
      this.identity,
      locator.subjectSha256,
    );
    await this.publishExact(
      subjectDirectory,
      SUBJECT_METADATA_FILE,
      encodeSubjectMetadata(metadata),
      "metadata",
    );
    for (const relativePath of definition.requiredDirectories) {
      await this.root.ensureDirectory([
        ACCOUNT_DATA_V2_DIRECTORY,
        ...relativePath.split("/"),
      ]);
    }

    if (await this.residency.hasPurgeBarrier(locator.subjectSha256)) {
      throw new SubjectStorageManagerError("purged_subject");
    }
    const ownerComponents = definition.ownerPath.split("/");
    const ownerDirectory = await this.root.openDirectory(
      ownerComponents.slice(0, -1),
    );
    const createdOwner = await this.publishExact(
      ownerDirectory,
      ownerComponents.at(-1)!,
      expectedOwnerBytes,
      "owner",
    );
    if (await this.residency.hasPurgeBarrier(locator.subjectSha256)) {
      throw new SubjectStorageManagerError("purged_subject");
    }
    const afterRegistration = await this.captureLayoutSnapshot(
      locator.subjectSha256,
    );
    this.assertAuditContainsScope(
      this.requireSuccessfulAudit(
        allowLegacyRoots
          ? this.auditPurgeSnapshot(afterRegistration)
          : this.auditSnapshot(afterRegistration),
      ),
      definition,
    );
    if (await this.residency.hasPurgeBarrier(locator.subjectSha256)) {
      throw new SubjectStorageManagerError("purged_subject");
    }
    const capability = await openCapability();
    if (await this.residency.hasPurgeBarrier(locator.subjectSha256)) {
      throw new SubjectStorageManagerError("purged_subject");
    }
    return Object.freeze({ capability, createdOwner });
  }

  private async resolveScope<Capability>(
    kind: ScopeKind,
    scope: string,
    openCapability: (subjectSha256: string) => Promise<Capability>,
  ): Promise<Capability | null> {
    const first = await this.serialized(async () => {
      const snapshot = await this.captureLayoutSnapshot(ABSENT_AUDIT_TARGET);
      const ownerPath = scopeOwnershipPath(kind, scope).relativePath;
      const encoded = snapshot.controlBytes.get(ownerPath);
      if (!encoded) {
        this.requireSuccessfulAudit(this.auditSnapshot(snapshot));
        return null;
      }
      return parseScopeOwnership(encoded, this.identity);
    });
    if (!first) return null;

    return this.withSubjectOperation(first.subjectSha256, async () => {
      if (await this.residency.hasPurgeBarrier(first.subjectSha256)) {
        throw new SubjectStorageManagerError("purged_subject");
      }
      const snapshot = await this.captureLayoutSnapshot(first.subjectSha256);
      const encoded = snapshot.controlBytes.get(first.ownershipPath);
      if (!encoded || !encoded.equals(encodeScopeOwnership(first))) {
        throw new SubjectStorageManagerError("corrupt_storage");
      }
      const parsed = parseScopeOwnership(encoded, this.identity);
      if (
        parsed.kind !== kind ||
        parsed.scope !== scope ||
        parsed.subjectSha256 !== first.subjectSha256
      ) {
        throw new SubjectStorageManagerError("owner_conflict");
      }
      this.assertAuditContainsScope(
        this.requireSuccessfulAudit(this.auditSnapshot(snapshot)),
        scopeDefinitionFromOwner(parsed),
      );
      if (await this.residency.hasPurgeBarrier(first.subjectSha256)) {
        throw new SubjectStorageManagerError("purged_subject");
      }
      const capability = await openCapability(first.subjectSha256);
      if (await this.residency.hasPurgeBarrier(first.subjectSha256)) {
        throw new SubjectStorageManagerError("purged_subject");
      }
      return capability;
    });
  }

  private async openProfile(
    subject: string,
    scope: string,
  ): Promise<ManagedProfileStorage> {
    const paths = subjectStoragePaths(subject).profile(scope);
    const [root, active, snapshots, checkpoints, receipts, temporary] =
      await Promise.all([
        this.root.openDirectory(paths.root.components),
        this.root.openDirectory(paths.active.components),
        this.root.openDirectory(paths.snapshots.components),
        this.root.openDirectory(paths.checkpoints.components),
        this.root.openDirectory(paths.receipts.components),
        this.root.openDirectory(paths.temporary.components),
      ]);
    return Object.freeze({
      kind: "profile",
      subjectSha256: subject,
      scope,
      paths,
      root,
      active,
      snapshots,
      checkpoints,
      receipts,
      temporary,
    });
  }

  private async openResult(
    subject: string,
    scope: string,
  ): Promise<ManagedResultStorage> {
    const paths = subjectStoragePaths(subject).result(scope);
    const [root, temporary] = await Promise.all([
      this.root.openDirectory(paths.root.components),
      this.root.openDirectory(paths.temporary.components),
    ]);
    return Object.freeze({
      kind: "result",
      subjectSha256: subject,
      scope,
      paths,
      root,
      temporary,
    });
  }

  private async ensureGlobalLayoutScaffold(): Promise<void> {
    const layout = await this.root.ensureDirectory([ACCOUNT_DATA_V2_DIRECTORY]);
    await layout.ensureChildDirectory(SUBJECTS_DIRECTORY);
    const owners = await layout.ensureChildDirectory(SCOPE_OWNERS_DIRECTORY);
    await owners.ensureChildDirectory("profiles");
    await owners.ensureChildDirectory("results");
  }

  private async publishExact(
    directory: NativeRunnerStorageDirectory,
    name: string,
    contents: Buffer,
    record: "metadata" | "owner",
  ): Promise<boolean> {
    const created = await directory.writeFileExclusive(name, contents);
    const persisted = await directory.readFileBounded(
      name,
      MAXIMUM_SIGNED_CONTROL_BYTES,
    );
    if (!persisted.equals(contents)) {
      throw new SubjectStorageManagerError(
        record === "owner" ? "owner_conflict" : "corrupt_storage",
      );
    }
    if (record === "owner") parseScopeOwnership(persisted, this.identity);
    else parseSubjectMetadata(persisted, this.identity);
    return created;
  }

  private async captureLayoutSnapshot(
    targetSubjectSha256: string,
  ): Promise<LayoutSnapshot> {
    this.root.assertUnchanged();
    const initialDeviceId = this.root.deviceId;
    const initialLinkCount = this.root.linkCount;
    const rootDirectory = await this.root.openDirectory([]);
    const first = await rootDirectory.inventory();
    const controlBytes = new Map<string, Buffer>();
    for (const entry of first.entries) {
      if (!isSignedControlEntry(entry)) continue;
      if (entry.sizeBytes < 1 || entry.sizeBytes > MAXIMUM_SIGNED_CONTROL_BYTES)
        continue;
      const components = entry.relativePath.split("/");
      const parent = await this.root.openDirectory(components.slice(0, -1));
      const encoded = await parent.readFileBounded(
        components.at(-1)!,
        MAXIMUM_SIGNED_CONTROL_BYTES,
      );
      if (
        encoded.length !== entry.sizeBytes ||
        createHash("sha256").update(encoded).digest("hex") !== entry.sha256
      ) {
        throw new SubjectStorageManagerError("audit_changed");
      }
      controlBytes.set(entry.relativePath, encoded);
    }
    const second = await rootDirectory.inventory();
    if (!inventoriesEqual(first, second)) {
      throw new SubjectStorageManagerError("audit_changed");
    }
    this.root.assertUnchanged();
    if (
      this.root.deviceId !== initialDeviceId ||
      this.root.linkCount !== initialLinkCount
    ) {
      throw new SubjectStorageManagerError("audit_changed");
    }
    return Object.freeze({
      input: inventoryToAuditInput(
        first,
        controlBytes,
        initialDeviceId,
        targetSubjectSha256,
        this.identity,
      ),
      controlBytes,
      nativeInventory: first,
      rootDeviceId: initialDeviceId,
      rootLinkCount: initialLinkCount,
    });
  }

  private auditSnapshot(snapshot: LayoutSnapshot): SubjectStorageAuditResult {
    return auditSubjectStorageLayout(snapshot.input);
  }

  private auditPurgeSnapshot(
    snapshot: LayoutSnapshot,
  ): SubjectStorageAuditResult {
    classifyLegacyRunnerInventory(
      snapshot.nativeInventory,
      snapshot.rootDeviceId,
      snapshot.rootLinkCount,
    );
    return this.auditPurgeInput(snapshot.input);
  }

  private auditPurgeInput(
    input: SubjectStorageAuditInput,
  ): SubjectStorageAuditResult {
    const legacyRoots = new Set<string>(LEGACY_ARTIFACT_ROOTS);
    return auditSubjectStorageLayout(
      Object.freeze({
        ...input,
        runnerRootEntries: Object.freeze(
          input.runnerRootEntries.filter(
            (entry) =>
              !legacyRoots.has(entry.name) ||
              entry.kind !== "directory" ||
              entry.deviceId !== input.runnerRootDeviceId,
          ),
        ),
      }),
    );
  }

  private requireSuccessfulAudit(
    audit: SubjectStorageAuditResult,
  ): SubjectStorageAuditSuccess {
    if (!audit.ok)
      throw new SubjectStorageManagerError("audit_rejected", audit);
    return audit;
  }

  private assertRepairableRegistration(
    snapshot: LayoutSnapshot,
    definition: ScopeDefinition,
    allowLegacyRoots = false,
  ): void {
    const audit = allowLegacyRoots
      ? this.auditPurgeSnapshot(snapshot)
      : this.auditSnapshot(snapshot);
    if (audit.ok) return;
    const expectedMissing = new Set([
      `${ACCOUNT_DATA_V2_DIRECTORY}/${SUBJECTS_DIRECTORY}`,
      `${ACCOUNT_DATA_V2_DIRECTORY}/${SCOPE_OWNERS_DIRECTORY}`,
      `${ACCOUNT_DATA_V2_DIRECTORY}/${SCOPE_OWNERS_DIRECTORY}/profiles`,
      `${ACCOUNT_DATA_V2_DIRECTORY}/${SCOPE_OWNERS_DIRECTORY}/results`,
      `${ACCOUNT_DATA_V2_DIRECTORY}/${SUBJECTS_DIRECTORY}/${definition.subjectSha256}/profiles`,
      `${ACCOUNT_DATA_V2_DIRECTORY}/${SUBJECTS_DIRECTORY}/${definition.subjectSha256}/results`,
      ...definition.requiredDirectories.map(
        (path) => `${ACCOUNT_DATA_V2_DIRECTORY}/${path}`,
      ),
    ]);
    const subjectRoot =
      `${ACCOUNT_DATA_V2_DIRECTORY}/${SUBJECTS_DIRECTORY}` +
      `/${definition.subjectSha256}`;
    for (const failure of audit.failures) {
      const repairable =
        (failure.code === "missing_required_entry" &&
          expectedMissing.has(failure.path)) ||
        (failure.code === "missing_subject_metadata" &&
          failure.path === subjectRoot) ||
        (failure.code === "unindexed_artifact" &&
          failure.path ===
            `${ACCOUNT_DATA_V2_DIRECTORY}/${definition.scopeRootPath}`);
      if (!repairable)
        throw new SubjectStorageManagerError("audit_rejected", audit);
    }

    const metadataPath =
      `${SUBJECTS_DIRECTORY}/${definition.subjectSha256}` +
      `/${SUBJECT_METADATA_FILE}`;
    const metadata = scannerEntry(snapshot.input, metadataPath);
    if (metadata) {
      const encoded = snapshot.controlBytes.get(
        `${ACCOUNT_DATA_V2_DIRECTORY}/${metadataPath}`,
      );
      const expected = encodeSubjectMetadata(
        createSignedSubjectMetadata(this.identity, definition.subjectSha256),
      );
      if (!encoded?.equals(expected)) {
        throw new SubjectStorageManagerError("corrupt_storage");
      }
    } else {
      this.assertEmptyIncompleteSubject(snapshot.input, definition);
    }
    this.assertEmptyIncompleteScope(snapshot.input, definition);
  }

  private assertEmptyIncompleteSubject(
    input: SubjectStorageAuditInput,
    definition: ScopeDefinition,
  ): void {
    if (input.accountDataV2.state === "absent") return;
    const prefix = `${SUBJECTS_DIRECTORY}/${definition.subjectSha256}`;
    const allowed = new Set([
      prefix,
      `${prefix}/profiles`,
      `${prefix}/results`,
      ...definition.requiredDirectories,
    ]);
    for (const entry of input.accountDataV2.entries) {
      const path = entry.components.join("/");
      if (
        (path === prefix || path.startsWith(`${prefix}/`)) &&
        (entry.kind !== "directory" || !allowed.has(path))
      ) {
        throw new SubjectStorageManagerError("corrupt_storage");
      }
    }
  }

  private assertEmptyIncompleteScope(
    input: SubjectStorageAuditInput,
    definition: ScopeDefinition,
  ): void {
    if (input.accountDataV2.state === "absent") return;
    const allowed = new Set(definition.requiredDirectories);
    for (const entry of input.accountDataV2.entries) {
      const path = entry.components.join("/");
      if (
        (path === definition.scopeRootPath ||
          path.startsWith(`${definition.scopeRootPath}/`)) &&
        (entry.kind !== "directory" || !allowed.has(path))
      ) {
        throw new SubjectStorageManagerError("corrupt_storage");
      }
    }
  }

  private assertRemovalState(
    snapshot: LayoutSnapshot,
    audit: SubjectStorageAuditResult,
    subject: string,
    expectedOwners: ReadonlyMap<string, Buffer>,
    allowLegacyRoots: boolean,
  ): void {
    const filtered = filterTargetFromAuditInput(
      snapshot.input,
      subject,
      expectedOwners.keys(),
    );
    const withoutTarget = allowLegacyRoots
      ? this.auditPurgeInput(filtered)
      : auditSubjectStorageLayout(filtered);
    if (
      !withoutTarget.ok ||
      withoutTarget.target.residency !== "never_resident"
    ) {
      throw new SubjectStorageManagerError(
        "audit_rejected",
        withoutTarget.ok ? undefined : withoutTarget,
      );
    }
    if (audit.ok) return;
    const subjectRoot = `${ACCOUNT_DATA_V2_DIRECTORY}/${SUBJECTS_DIRECTORY}/${subject}`;
    for (const failure of audit.failures) {
      const recoverable =
        (failure.code === "missing_subject_metadata" &&
          failure.path === subjectRoot) ||
        (failure.code === "missing_required_entry" &&
          failure.path.startsWith(`${subjectRoot}/`)) ||
        (failure.code === "unindexed_artifact" &&
          failure.path.startsWith(`${subjectRoot}/`)) ||
        (failure.code === "orphan_scope_ownership" &&
          expectedOwners.has(failure.path));
      if (!recoverable)
        throw new SubjectStorageManagerError("audit_rejected", audit);
    }
  }

  private assertTargetOwnerSet(
    snapshot: LayoutSnapshot,
    subject: string,
    expectedOwners: ReadonlyMap<string, Buffer>,
  ): void {
    for (const [path, encoded] of snapshot.controlBytes) {
      if (!isOwnerPath(path)) continue;
      const owner = parseScopeOwnership(encoded, this.identity);
      if (owner.subjectSha256 !== subject) continue;
      const expected = expectedOwners.get(path);
      if (!expected || !expected.equals(encoded)) {
        throw new SubjectStorageManagerError("owner_conflict");
      }
    }
    for (const [path, expected] of expectedOwners) {
      const actual = snapshot.controlBytes.get(path);
      if (
        scannerEntry(snapshot.input, stripLayoutRoot(path)) &&
        !actual?.equals(expected)
      ) {
        throw new SubjectStorageManagerError("owner_conflict");
      }
    }
  }

  private assertExactOwner(
    encoded: Buffer,
    expected: SignedScopeOwnership,
    expectedBytes: Buffer,
  ): void {
    const parsed = parseScopeOwnership(encoded, this.identity);
    if (
      !encoded.equals(expectedBytes) ||
      parsed.subjectSha256 !== expected.subjectSha256 ||
      parsed.kind !== expected.kind ||
      parsed.scope !== expected.scope
    ) {
      throw new SubjectStorageManagerError("owner_conflict");
    }
  }

  private assertAuditContainsScope(
    audit: SubjectStorageAuditSuccess,
    definition: ScopeDefinition,
  ): void {
    const subject = audit.subjects.find(
      (candidate) => candidate.subjectSha256 === definition.subjectSha256,
    );
    const ownsScope =
      definition.kind === "profile"
        ? subject?.profileScopes.some((scope) => scope === definition.scope)
        : subject?.resultScopes.some((scope) => scope === definition.scope);
    if (audit.target.residency !== "resident" || !ownsScope) {
      throw new SubjectStorageManagerError("corrupt_storage");
    }
  }

  private assertLocator(
    locator: AccountResidencyLocator,
    expectedKind?: ScopeKind,
  ): void {
    try {
      if (
        !locator ||
        typeof locator !== "object" ||
        locator.version !== 1 ||
        locator.audience !== LOCATOR_AUDIENCE ||
        locator.volumeId !== this.identity.volumeId ||
        locator.volumeKeyFingerprint !== this.identity.publicKeyFingerprint ||
        (locator.kind !== "profile" && locator.kind !== "result") ||
        (expectedKind && locator.kind !== expectedKind) ||
        typeof locator.scope !== "string" ||
        typeof locator.signature !== "string"
      ) {
        throw new Error("invalid locator");
      }
      subjectSha256(locator.subjectSha256);
      if (locator.kind === "profile") profileScope(locator.scope);
      else resultScope(locator.scope);
      const expectedFamilies =
        locator.kind === "profile"
          ? PROFILE_ARTIFACT_FAMILIES
          : RESULT_ARTIFACT_FAMILIES;
      if (
        !Array.isArray(locator.artifactFamilies) ||
        locator.artifactFamilies.length !== expectedFamilies.length ||
        locator.artifactFamilies.some(
          (value, index) => value !== expectedFamilies[index],
        )
      ) {
        throw new Error("invalid locator families");
      }
    } catch (error) {
      if (error instanceof SubjectStorageManagerError) throw error;
      throw new SubjectStorageManagerError("invalid_locator", undefined, {
        cause: error,
      });
    }
  }

  private async assertPersistedLocator(
    locator: AccountResidencyLocator,
  ): Promise<void> {
    const persisted = await this.residency.locatorsForSubjectHash(
      locator.subjectSha256,
    );
    if (!persisted.some((candidate) => locatorsEqual(candidate, locator))) {
      throw new SubjectStorageManagerError("invalid_locator");
    }
  }

  private async assertCompletePersistedLocatorSet(
    subject: string,
    supplied: readonly AccountResidencyLocator[],
  ): Promise<void> {
    const persisted = [
      ...(await this.residency.locatorsForSubjectHash(subject)),
    ].sort(compareLocators);
    const expected = [...supplied].sort(compareLocators);
    if (
      persisted.length !== expected.length ||
      persisted.some(
        (locator, index) => !locatorsEqual(locator, expected[index]!),
      )
    ) {
      throw new SubjectStorageManagerError("locator_set_incomplete");
    }
  }

  private withSubjectOperation<T>(
    subject: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    return this.residency.withSubjectLock(subject, () =>
      this.serialized(operation),
    );
  }

  private async serialized<T>(operation: () => Promise<T>): Promise<T> {
    const predecessor = this.operationTail;
    let release = (): void => undefined;
    const current = new Promise<void>((resolve) => {
      release = resolve;
    });
    this.operationTail = current;
    await predecessor;
    try {
      return await operation();
    } finally {
      release();
    }
  }
}

function scopeDefinition(locator: AccountResidencyLocator): ScopeDefinition {
  const paths = subjectStoragePaths(locator.subjectSha256);
  if (locator.kind === "profile") {
    const profile = paths.profile(locator.scope);
    return Object.freeze({
      kind: "profile",
      subjectSha256: locator.subjectSha256,
      scope: locator.scope,
      scopeRootPath: stripLayoutRoot(profile.root.relativePath),
      ownerPath: profile.ownership.relativePath,
      requiredDirectories: Object.freeze([
        stripLayoutRoot(profile.root.relativePath),
        stripLayoutRoot(profile.active.relativePath),
        stripLayoutRoot(profile.snapshots.relativePath),
        stripLayoutRoot(profile.checkpoints.relativePath),
        stripLayoutRoot(profile.receipts.relativePath),
        stripLayoutRoot(profile.temporary.relativePath),
      ]),
    });
  }
  const result = paths.result(locator.scope);
  return Object.freeze({
    kind: "result",
    subjectSha256: locator.subjectSha256,
    scope: locator.scope,
    scopeRootPath: stripLayoutRoot(result.root.relativePath),
    ownerPath: result.ownership.relativePath,
    requiredDirectories: Object.freeze([
      stripLayoutRoot(result.root.relativePath),
      stripLayoutRoot(result.temporary.relativePath),
    ]),
  });
}

function scopeDefinitionFromOwner(
  owner: SignedScopeOwnership,
): ScopeDefinition {
  const locator = {
    kind: owner.kind,
    scope: owner.scope,
    subjectSha256: owner.subjectSha256,
  } as Pick<AccountResidencyLocator, "kind" | "scope" | "subjectSha256">;
  const paths = subjectStoragePaths(locator.subjectSha256);
  if (locator.kind === "profile") {
    const profile = paths.profile(locator.scope);
    return {
      kind: locator.kind,
      subjectSha256: locator.subjectSha256,
      scope: locator.scope,
      scopeRootPath: stripLayoutRoot(profile.root.relativePath),
      ownerPath: profile.ownership.relativePath,
      requiredDirectories: [],
    };
  }
  const result = paths.result(locator.scope);
  return {
    kind: locator.kind,
    subjectSha256: locator.subjectSha256,
    scope: locator.scope,
    scopeRootPath: stripLayoutRoot(result.root.relativePath),
    ownerPath: result.ownership.relativePath,
    requiredDirectories: [],
  };
}

function inventoryToAuditInput(
  inventory: NativeRunnerInventory,
  controls: ReadonlyMap<string, Buffer>,
  rootDeviceId: string,
  targetSubjectSha256: string,
  identity: RunnerVolumeIdentity,
): SubjectStorageAuditInput {
  const rootEntries: NativeRunnerRootEntryEvidence[] = inventory.entries
    .filter((entry) => !entry.relativePath.includes("/"))
    .map((entry) => ({
      name: entry.relativePath,
      kind: entry.kind,
      deviceId: entry.deviceId,
      linkCount: entry.linkCount,
    }));
  const v2Root = inventory.entries.find(
    (entry) => entry.relativePath === ACCOUNT_DATA_V2_DIRECTORY,
  );
  let accountDataV2: NativeAccountDataV2Scan = { state: "absent" };
  if (v2Root) {
    const prefix = `${ACCOUNT_DATA_V2_DIRECTORY}/`;
    const entries: NativeScannerRecord[] = inventory.entries
      .filter((entry) => entry.relativePath.startsWith(prefix))
      .map((entry) => ({
        components: Object.freeze(
          entry.relativePath.slice(prefix.length).split("/"),
        ),
        kind: entry.kind,
        deviceId: entry.deviceId,
        linkCount: entry.linkCount,
        sizeBytes: String(entry.sizeBytes),
        sha256: entry.kind === "file" ? entry.sha256 : null,
        controlBytes: controls.get(entry.relativePath) ?? null,
      }));
    accountDataV2 = {
      state: "present",
      kind: v2Root.kind,
      deviceId: v2Root.deviceId,
      linkCount: v2Root.linkCount,
      entries: Object.freeze(entries),
    };
  }
  return Object.freeze({
    targetSubjectSha256,
    identity,
    runnerRootDeviceId: rootDeviceId,
    runnerRootEntries: Object.freeze(rootEntries),
    accountDataV2,
  });
}

function inventoriesEqual(
  left: NativeRunnerInventory,
  right: NativeRunnerInventory,
): boolean {
  return (
    left.count === right.count &&
    left.bytes === right.bytes &&
    left.sha256 === right.sha256 &&
    left.entries.length === right.entries.length &&
    left.entries.every((entry, index) =>
      inventoryEntriesEqual(entry, right.entries[index]!),
    )
  );
}

function inventoryEntriesEqual(
  left: NativeRunnerInventoryEntry,
  right: NativeRunnerInventoryEntry,
): boolean {
  return (
    left.relativePath === right.relativePath &&
    left.kind === right.kind &&
    left.deviceId === right.deviceId &&
    left.linkCount === right.linkCount &&
    left.sizeBytes === right.sizeBytes &&
    left.sha256 === right.sha256
  );
}

function isSignedControlEntry(entry: NativeRunnerInventoryEntry): boolean {
  if (entry.kind !== "file") return false;
  const components = entry.relativePath.split("/");
  if (
    components.length === 4 &&
    components[0] === ACCOUNT_DATA_V2_DIRECTORY &&
    components[1] === SUBJECTS_DIRECTORY &&
    /^[0-9a-f]{64}$/.test(components[2]!) &&
    components[3] === SUBJECT_METADATA_FILE
  ) {
    return true;
  }
  return (
    components.length === 4 &&
    components[0] === ACCOUNT_DATA_V2_DIRECTORY &&
    components[1] === SCOPE_OWNERS_DIRECTORY &&
    ((components[2] === "profiles" &&
      /^[0-9a-f]{40}\.json$/.test(components[3]!)) ||
      (components[2] === "results" &&
        /^[0-9a-f]{64}\.json$/.test(components[3]!)))
  );
}

function isOwnerPath(path: string): boolean {
  const components = path.split("/");
  return (
    components.length === 4 &&
    components[0] === ACCOUNT_DATA_V2_DIRECTORY &&
    components[1] === SCOPE_OWNERS_DIRECTORY &&
    (components[2] === "profiles" || components[2] === "results")
  );
}

function scannerEntry(
  input: SubjectStorageAuditInput,
  relativePath: string,
): NativeScannerRecord | undefined {
  if (input.accountDataV2.state === "absent") return undefined;
  return input.accountDataV2.entries.find(
    (entry) => entry.components.join("/") === relativePath,
  );
}

function filterTargetFromAuditInput(
  input: SubjectStorageAuditInput,
  subject: string,
  ownerPaths: Iterable<string>,
): SubjectStorageAuditInput {
  if (input.accountDataV2.state === "absent") return input;
  const ownerSet = new Set([...ownerPaths].map(stripLayoutRoot));
  const subjectPrefix = `${SUBJECTS_DIRECTORY}/${subject}`;
  const entries = input.accountDataV2.entries.filter((entry) => {
    const path = entry.components.join("/");
    return (
      path !== subjectPrefix &&
      !path.startsWith(`${subjectPrefix}/`) &&
      !ownerSet.has(path)
    );
  });
  return Object.freeze({
    ...input,
    accountDataV2: Object.freeze({
      ...input.accountDataV2,
      entries: Object.freeze(entries),
    }),
  });
}

function inventoryFromAudit(
  audit: SubjectStorageAuditSuccess,
  snapshot: LayoutSnapshot,
): SubjectStorageInventory {
  const subject =
    audit.subjects.find(
      (candidate) => candidate.subjectSha256 === audit.target.subjectSha256,
    ) ?? null;
  return Object.freeze({
    residency: audit.target.residency,
    subject,
    inventory: subject?.inventory ?? emptyInventory(),
    completeInventory: audit.inventory,
    runnerRootInventory: runnerRootInventoryFromSnapshot(snapshot),
    legacyInventory: classifyLegacyRunnerInventory(
      snapshot.nativeInventory,
      snapshot.rootDeviceId,
      snapshot.rootLinkCount,
    ),
  });
}

function runnerRootInventoryFromSnapshot(
  snapshot: LayoutSnapshot,
): RunnerRootInventoryEvidence {
  return Object.freeze({
    deviceId: snapshot.rootDeviceId,
    linkCount: snapshot.rootLinkCount,
    entryCount: snapshot.nativeInventory.count,
    fileBytes: String(snapshot.nativeInventory.bytes),
    sha256: snapshot.nativeInventory.sha256,
  });
}

export function currentStorageSubjectSetSha256(
  subjects: readonly CurrentStorageSubjectEvidence[],
): string {
  const digest = createHash("sha256");
  digest.update(
    "bluey-jobs-runner-subject-storage-subject-set-v1\n",
    "utf8",
  );
  for (const subject of [...subjects].sort((left, right) =>
    compareCanonicalUtf8(left.subjectSha256, right.subjectSha256),
  )) {
    digest.update(`subject_sha256=${subject.subjectSha256}\n`, "utf8");
    digest.update(
      `profile_scope_count=${subject.profileScopeCount}\n`,
      "utf8",
    );
    digest.update(
      `result_scope_count=${subject.resultScopeCount}\n`,
      "utf8",
    );
    digest.update(
      `subject_tree_entry_count=${subject.subjectTree.entryCount}\n`,
      "utf8",
    );
    digest.update(
      `subject_tree_file_bytes=${subject.subjectTree.fileBytes}\n`,
      "utf8",
    );
    digest.update(
      `subject_tree_sha256=${subject.subjectTree.sha256}\n`,
      "utf8",
    );
    digest.update(
      `ownership_entry_count=${subject.ownership.entryCount}\n`,
      "utf8",
    );
    digest.update(
      `ownership_file_bytes=${subject.ownership.fileBytes}\n`,
      "utf8",
    );
    digest.update(`ownership_sha256=${subject.ownership.sha256}\n`, "utf8");
  }
  return digest.digest("hex");
}

function emptyInventory(): SubjectStorageInventoryEvidence {
  return Object.freeze({
    entryCount: 0,
    fileBytes: "0",
    sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
  });
}

function stripLayoutRoot(path: string): string {
  const prefix = `${ACCOUNT_DATA_V2_DIRECTORY}/`;
  if (!path.startsWith(prefix))
    throw new SubjectStorageManagerError("corrupt_storage");
  return path.slice(prefix.length);
}

function compareLocators(
  left: AccountResidencyLocator,
  right: AccountResidencyLocator,
): number {
  return (
    left.kind.localeCompare(right.kind) || left.scope.localeCompare(right.scope)
  );
}

function locatorsEqual(
  left: AccountResidencyLocator,
  right: AccountResidencyLocator,
): boolean {
  return (
    left.version === right.version &&
    left.audience === right.audience &&
    left.volumeId === right.volumeId &&
    left.volumeKeyFingerprint === right.volumeKeyFingerprint &&
    left.subjectSha256 === right.subjectSha256 &&
    left.kind === right.kind &&
    left.scope === right.scope &&
    left.signature === right.signature &&
    left.artifactFamilies.length === right.artifactFamilies.length &&
    left.artifactFamilies.every(
      (value, index) => value === right.artifactFamilies[index],
    )
  );
}

function stringArraysEqual(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return (
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
}

function compareCanonicalUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}
