import { createHash } from "node:crypto";
import {
  AccountResidencyIndex,
  accountPurgeSubjectHash,
  type AccountResidencyLocator,
} from "./account-residency.js";
import {
  listSafeRunnerDirectory,
  readBoundedRegularFile,
  replaceDurableFile,
  runnerPath,
  writeDurableFileExclusive,
  type RunnerDataRoot,
} from "./safe-runner-storage.js";
import {
  assertRunnerPurgeCurrentTargetStorageEmpty,
  completeRunnerPurgeStorageEvidence,
  createRunnerPurgeStorageBeforeEvidence,
  EMPTY_RUNNER_INVENTORY_SHA256,
  parseRunnerPurgeStorageBeforeEvidence,
  parseRunnerPurgeStorageEvidence,
  runnerPurgeLocatorEvidence,
  runnerPurgeStorageEvidenceSha256,
  runnerPurgeTargetInventoryState,
  RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
  type RunnerPurgeStorageBeforeEvidence,
  type RunnerPurgeStorageEvidence,
} from "./purge-storage-evidence.js";
import { SUBJECT_STORAGE_LAYOUT_VERSION } from "./subject-storage-layout.js";
import type {
  LockedSubjectStorage,
  SubjectStorageInventory,
  SubjectStorageManager,
} from "./subject-storage-manager.js";
import {
  decodeCanonicalBase64Url,
  signEd25519,
  verifyEd25519,
} from "./volume-identity.js";

export { EMPTY_RUNNER_INVENTORY_SHA256 };

export const RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE =
  "bluey-jobs-runner-volume-purge-v2";
export const RUNNER_VOLUME_PURGE_ACK_AUDIENCE =
  "bluey-jobs-runner-volume-purge-ack-v2";
export const ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256 = createHash(
  "sha256",
)
  .update("bluey-jobs-runner-legacy-inventory-authority-absent-v1\n", "utf8")
  .digest("hex");

const JOURNAL_AUDIENCE = "bluey-jobs-runner-volume-purge-journal-v2";
const FENCE_AUDIENCE = "bluey-jobs-runner-volume-purge-fence";
const TOMBSTONE_AUDIENCE = "bluey-jobs-runner-volume-purge-tombstone";
const TOMBSTONE_RETENTION_POLICY = "indefinite_managed_restore_safety";
const MAXIMUM_CONTROL_RECORD_BYTES = 128 * 1024;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const PROFILE_SCOPE_PATTERN = /^[0-9a-f]{40}$/;
const RESULT_SCOPE_PATTERN = /^[0-9a-f]{64}$/;
const SAFE_IDENTIFIER_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:+-]{0,127}$/;
const RUNNER_BUILD_PATTERN =
  /^runner-(0|[1-9][0-9]{0,8})(?:\.(0|[1-9][0-9]{0,8}))?$/;

export type RunnerVolumePurgeErrorCode =
  | "ack_conflict"
  | "build_incompatible"
  | "command_conflict"
  | "corrupt_state"
  | "incomplete_purge"
  | "invalid_command"
  | "irreversible_work"
  | "restored_data"
  | "stale_command"
  | "wrong_epoch"
  | "wrong_volume";

export class RunnerVolumePurgeError extends Error {
  constructor(readonly code: RunnerVolumePurgeErrorCode) {
    super(
      {
        ack_conflict:
          "A stored purge acknowledgement conflicts with the command.",
        build_incompatible:
          "The runner build does not satisfy the purge command.",
        command_conflict:
          "A purge command conflicts with durable runner state.",
        corrupt_state:
          "The runner purge journal, fence, or tombstone is corrupt.",
        incomplete_purge: "The runner purge rescan was not canonically empty.",
        invalid_command:
          "The runner purge command is invalid or unauthenticated.",
        irreversible_work:
          "Irreversible account work must be reconciled before purge.",
        restored_data:
          "Data appeared beneath a completed runner purge tombstone.",
        stale_command: "The runner purge generation is stale.",
        wrong_epoch:
          "The runner purge command targets a different enrollment epoch.",
        wrong_volume:
          "The runner purge command targets a different volume identity.",
      }[code],
    );
    this.name = "RunnerVolumePurgeError";
  }
}

export interface UnsignedRunnerVolumePurgeCommand {
  readonly version: 2;
  readonly requestId: string;
  readonly audience: typeof RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE;
  readonly commandId: string;
  readonly targetVolumeId: string;
  readonly targetKeyFingerprint: string;
  readonly enrollmentEpoch: number;
  readonly purgeSubject: string;
  readonly purgeGeneration: number;
  readonly storageEvidenceVersion: typeof RUNNER_PURGE_STORAGE_EVIDENCE_VERSION;
  readonly subjectStorageLayoutVersion: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
  readonly legacyInventoryAuthorityGeneration: number;
  readonly legacyInventoryAuthoritySha256: string;
  readonly issuedAtMs: number;
  readonly minimumRunnerBuildId: string;
  readonly serverKeyId: string;
}

export interface RunnerVolumePurgeCommand extends UnsignedRunnerVolumePurgeCommand {
  readonly signature: string;
}

export interface UnsignedRunnerVolumePurgeAck {
  readonly version: 2;
  readonly audience: typeof RUNNER_VOLUME_PURGE_ACK_AUDIENCE;
  readonly requestId: string;
  readonly commandId: string;
  readonly commandSha256: string;
  readonly targetVolumeId: string;
  readonly targetKeyFingerprint: string;
  readonly enrollmentEpoch: number;
  readonly processInstanceId: string;
  readonly purgeSubjectSha256: string;
  readonly purgeGeneration: number;
  readonly storageEvidence: RunnerPurgeStorageEvidence;
  readonly storageEvidenceSha256: string;
  readonly beforeInventoryCount: number;
  readonly beforeInventorySha256: string;
  readonly afterInventoryCount: 0;
  readonly afterInventorySha256: typeof EMPTY_RUNNER_INVENTORY_SHA256;
  readonly removedCount: number;
  readonly runnerBuildId: string;
  readonly completedAtMs: number;
}

export interface RunnerVolumePurgeAck extends UnsignedRunnerVolumePurgeAck {
  readonly signature: string;
}

export type RunnerVolumePurgeFaultPhase =
  | "after_command_journal"
  | "after_active_work_stopped"
  | "after_fence"
  | "after_before_inventory"
  | "after_delete_target"
  | "after_zero_rescan"
  | "after_ack_journal"
  | "after_tombstone"
  | "after_completed_journal";

export type StopAccountWorkResult =
  { readonly status: "stopped" } | { readonly status: "irreversible" };

export interface RunnerVolumePurgerOptions {
  readonly enrollmentEpoch: number;
  readonly processInstanceId: string;
  readonly runnerBuildId: string;
  readonly serverCommandKeys: ReadonlyMap<string, string>;
  readonly subjectStorage: Pick<
    SubjectStorageManager,
    "identity" | "root" | "withLockedSubject"
  >;
  readonly stopAccountWork: (
    purgeSubjectSha256: string,
  ) => Promise<StopAccountWorkResult>;
  readonly nowMs?: () => number;
  readonly faultInjector?: (
    phase: RunnerVolumePurgeFaultPhase,
  ) => Promise<void>;
}

type JournalPhase =
  | "commanded"
  | "active_work_stopped"
  | "fenced"
  | "inventory_recorded"
  | "deleting"
  | "zero_verified"
  | "ack_signed"
  | "tombstoned"
  | "completed";

interface UnsignedPurgeJournal {
  readonly version: 2;
  readonly audience: typeof JOURNAL_AUDIENCE;
  readonly requestId: string;
  readonly commandId: string;
  readonly commandSha256: string;
  readonly commandAudience: typeof RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE;
  readonly targetVolumeId: string;
  readonly targetKeyFingerprint: string;
  readonly enrollmentEpoch: number;
  readonly purgeSubjectSha256: string;
  readonly purgeGeneration: number;
  readonly storageEvidenceVersion: typeof RUNNER_PURGE_STORAGE_EVIDENCE_VERSION;
  readonly subjectStorageLayoutVersion: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
  readonly legacyInventoryAuthorityGeneration: number;
  readonly legacyInventoryAuthoritySha256: string;
  readonly issuedAtMs: number;
  readonly minimumRunnerBuildId: string;
  readonly serverKeyId: string;
  readonly serverSignature: string;
  readonly phase: JournalPhase;
  readonly storageEvidenceBefore: RunnerPurgeStorageBeforeEvidence | null;
  readonly storageEvidence: RunnerPurgeStorageEvidence | null;
  readonly storageEvidenceSha256: string | null;
  readonly ack: RunnerVolumePurgeAck | null;
}

interface PurgeJournal extends UnsignedPurgeJournal {
  readonly localSignature: string;
}

interface UnsignedPurgeFence {
  readonly version: 1;
  readonly audience: typeof FENCE_AUDIENCE;
  readonly volumeId: string;
  readonly volumeKeyFingerprint: string;
  readonly requestId: string;
  readonly commandId: string;
  readonly commandSha256: string;
  readonly purgeSubjectSha256: string;
  readonly purgeGeneration: number;
}

interface PurgeFence extends UnsignedPurgeFence {
  readonly localSignature: string;
}

interface UnsignedPurgeTombstone {
  readonly version: 1;
  readonly audience: typeof TOMBSTONE_AUDIENCE;
  readonly volumeId: string;
  readonly volumeKeyFingerprint: string;
  readonly requestId: string;
  readonly commandId: string;
  readonly commandSha256: string;
  readonly purgeSubjectSha256: string;
  readonly purgeGeneration: number;
  readonly retentionPolicy: typeof TOMBSTONE_RETENTION_POLICY;
  readonly ack: RunnerVolumePurgeAck;
}

interface PurgeTombstone extends UnsignedPurgeTombstone {
  readonly localSignature: string;
}

const JOURNAL_PHASE_ORDER: Readonly<Record<JournalPhase, number>> =
  Object.freeze({
    commanded: 0,
    fenced: 1,
    active_work_stopped: 2,
    inventory_recorded: 3,
    deleting: 4,
    zero_verified: 5,
    ack_signed: 6,
    tombstoned: 7,
    completed: 8,
  });

export class RunnerVolumePurger {
  constructor(
    readonly residency: AccountResidencyIndex,
    readonly options: RunnerVolumePurgerOptions,
  ) {
    assertPositiveInteger(options.enrollmentEpoch, "invalid_command");
    assertCanonicalBytes(options.processInstanceId, 32, "invalid_command");
    assertRunnerBuildId(options.runnerBuildId, "invalid_command");
    if (
      options.subjectStorage.identity.volumeId !==
        residency.identity.volumeId ||
      options.subjectStorage.identity.publicKeyFingerprint !==
        residency.identity.publicKeyFingerprint ||
      options.subjectStorage.root.configuredPath !== residency.root.path
    ) {
      throw new RunnerVolumePurgeError("invalid_command");
    }
  }

  /**
   * Durably fence and quiesce one signed subject, then remove its exact
   * classified legacy targets without signing an acknowledgement. The volume
   * client prepares every paginated command before completing any of them so
   * each final acknowledgement can prove the required global legacy zero.
   */
  async prepare(input: unknown): Promise<number> {
    const remainingLegacyArtifactCount = await this.process(input, false);
    if (typeof remainingLegacyArtifactCount !== "number") {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return remainingLegacyArtifactCount;
  }

  async execute(input: unknown): Promise<RunnerVolumePurgeAck> {
    const ack = await this.process(input, true);
    if (typeof ack === "number") {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    return ack;
  }

  private async process(
    input: unknown,
    finalize: boolean,
  ): Promise<RunnerVolumePurgeAck | number> {
    const command = parseRunnerVolumePurgeCommand(input);
    const canonicalCommand = canonicalRunnerVolumePurgeCommand(command);
    const commandSha256 = createHash("sha256")
      .update(canonicalCommand)
      .digest("hex");
    this.verifyCommandAuthority(command, canonicalCommand);
    const purgeSubjectSha256 = accountPurgeSubjectHash(command.purgeSubject);
    return this.residency.withSubjectPurgeLock(purgeSubjectSha256, async () => {
      const prepared = await this.options.subjectStorage.withLockedSubject(
        purgeSubjectSha256,
        (storage) =>
          this.prepareBarrierLocked(
            command,
            commandSha256,
            purgeSubjectSha256,
            storage,
            finalize,
          ),
      );
      if (prepared.kind === "replay") {
        return finalize ? prepared.ack : prepared.legacyArtifactCount;
      }

      let quiesced = phaseAtLeast(
        prepared.journal.phase,
        "active_work_stopped",
      );
      if (!quiesced) {
        // Do not hold the subject filesystem lock here. A registration that
        // queued before the fence was installed must be able to acquire that
        // lock, observe the durable barrier, abort, and release any pending-work
        // promise that account quiescence is waiting for.
        const stopped = await this.options.stopAccountWork(purgeSubjectSha256);
        if (stopped.status === "irreversible") {
          throw new RunnerVolumePurgeError("irreversible_work");
        }
        if (stopped.status !== "stopped") {
          throw new RunnerVolumePurgeError("corrupt_state");
        }
        quiesced = true;
      }

      return this.options.subjectStorage.withLockedSubject(
        purgeSubjectSha256,
        async (storage) => {
          const resumed = await this.prepareBarrierLocked(
            command,
            commandSha256,
            purgeSubjectSha256,
            storage,
            finalize,
          );
          if (resumed.kind === "replay") {
            return finalize ? resumed.ack : resumed.legacyArtifactCount;
          }
          let journal = resumed.journal;
          if (!phaseAtLeast(journal.phase, "active_work_stopped")) {
            if (!quiesced) throw new RunnerVolumePurgeError("corrupt_state");
            journal = await this.saveJournal(
              this.residency.journalPath(command.commandId),
              {
                ...journal,
                phase: "active_work_stopped",
              },
            );
            await this.inject("after_active_work_stopped");
          }
          if (!finalize) {
            return this.prepareLegacyTargetsLocked(
              command,
              purgeSubjectSha256,
              journal,
              storage,
            );
          }
          return this.completeLocked(
            command,
            commandSha256,
            purgeSubjectSha256,
            journal,
            resumed.tombstone,
            storage,
          );
        },
      );
    });
  }

  private async prepareLegacyTargetsLocked(
    command: RunnerVolumePurgeCommand,
    purgeSubjectSha256: string,
    initialJournal: PurgeJournal,
    storage: LockedSubjectStorage,
  ): Promise<number> {
    if (!phaseAtLeast(initialJournal.phase, "active_work_stopped")) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    if (initialJournal.storageEvidence) {
      const inventory = await storage.inventory();
      await this.assertPreparedSubjectInventoryEmpty(
        purgeSubjectSha256,
        initialJournal,
        inventory,
      );
      return inventory.legacyInventory.legacyArtifactCount;
    }

    const journalPath = this.residency.journalPath(command.commandId);
    const locators =
      await this.residency.locatorsForSubjectHash(purgeSubjectSha256);
    const locatorState = runnerPurgeLocatorEvidence(locators);
    let journal = initialJournal;
    if (journal.storageEvidenceBefore) {
      if (
        journal.storageEvidenceBefore.locators.count !== locatorState.count ||
        journal.storageEvidenceBefore.locators.sha256 !== locatorState.sha256
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
    } else {
      const before = createRunnerPurgeStorageBeforeEvidence(
        await storage.inventory(),
        locators,
      );
      journal = await this.saveJournal(journalPath, {
        ...journal,
        phase: "inventory_recorded",
        storageEvidenceBefore: before,
      });
      await this.inject("after_before_inventory");
    }
    if (!phaseAtLeast(journal.phase, "deleting")) {
      journal = await this.saveJournal(journalPath, {
        ...journal,
        phase: "deleting",
      });
    }
    const inventory = await storage.removeLegacyTargets(locators, () =>
      this.inject("after_delete_target"),
    );
    return inventory.legacyInventory.legacyArtifactCount;
  }

  /**
   * Re-sign a locally completed purge only after the server rejects the
   * journaled ACK because its former process lease was never received. The
   * caller must first submit the old ACK: an exact server replay succeeds and
   * never reaches this path, while a 401 proves the pending command still
   * requires the currently leased process instance.
   */
  async recoverAcknowledgementForCurrentProcess(
    input: unknown,
    rejectedAck: RunnerVolumePurgeAck,
  ): Promise<RunnerVolumePurgeAck> {
    const command = parseRunnerVolumePurgeCommand(input);
    const canonicalCommand = canonicalRunnerVolumePurgeCommand(command);
    const commandSha256 = createHash("sha256")
      .update(canonicalCommand)
      .digest("hex");
    this.verifyCommandAuthority(command, canonicalCommand);
    const purgeSubjectSha256 = accountPurgeSubjectHash(command.purgeSubject);
    return this.options.subjectStorage.withLockedSubject(
      purgeSubjectSha256,
      async (storage) => {
        await this.residency.ensurePurgeDirectories();
        const journalPath = this.residency.journalPath(command.commandId);
        const tombstonePath = this.residency.tombstonePath(purgeSubjectSha256);
        const journal = await this.readJournal(journalPath);
        const tombstone = await this.readTombstone(tombstonePath);
        if (
          !journal ||
          !journal.ack ||
          !tombstone ||
          !phaseAtLeast(journal.phase, "tombstoned") ||
          !journalMatchesCommand(
            journal,
            command,
            commandSha256,
            purgeSubjectSha256,
          ) ||
          !tombstoneMatchesCommand(tombstone, command, commandSha256) ||
          !encodeAck(journal.ack).equals(encodeAck(rejectedAck))
        ) {
          throw new RunnerVolumePurgeError("ack_conflict");
        }
        this.assertTombstoneIdentity(tombstone, purgeSubjectSha256);
        this.assertAckMatchesCommand(
          journal.ack,
          command,
          commandSha256,
          purgeSubjectSha256,
        );
        if (
          journal.ack.processInstanceId === this.options.processInstanceId ||
          !ackRecoveryEvidenceMatches(tombstone.ack, journal.ack)
        ) {
          throw new RunnerVolumePurgeError("ack_conflict");
        }
        await this.assertSubjectInventoryEmpty(
          purgeSubjectSha256,
          journal,
          storage,
        );
        const recovered = this.createAck(
          command,
          commandSha256,
          purgeSubjectSha256,
          journal.storageEvidence!,
          Math.max(tombstone.ack.completedAtMs, journal.ack.completedAtMs),
        );
        await this.saveJournal(journalPath, {
          ...journal,
          phase: "completed",
          ack: recovered,
        });
        const persisted = await this.readJournal(journalPath);
        if (
          !persisted?.ack ||
          !encodeAck(persisted.ack).equals(encodeAck(recovered))
        ) {
          throw new RunnerVolumePurgeError("corrupt_state");
        }
        return recovered;
      },
    );
  }

  private verifyCommandAuthority(
    command: RunnerVolumePurgeCommand,
    canonicalCommand: Buffer,
  ): void {
    if (
      command.targetVolumeId !== this.residency.identity.volumeId ||
      command.targetKeyFingerprint !==
        this.residency.identity.publicKeyFingerprint
    ) {
      throw new RunnerVolumePurgeError("wrong_volume");
    }
    if (command.enrollmentEpoch !== this.options.enrollmentEpoch) {
      throw new RunnerVolumePurgeError("wrong_epoch");
    }
    if (
      !runnerBuildSatisfies(
        this.options.runnerBuildId,
        command.minimumRunnerBuildId,
      )
    ) {
      throw new RunnerVolumePurgeError("build_incompatible");
    }
    const serverPublicKey = this.options.serverCommandKeys.get(
      command.serverKeyId,
    );
    if (!serverPublicKey) throw new RunnerVolumePurgeError("invalid_command");
    try {
      if (
        !verifyEd25519(serverPublicKey, canonicalCommand, command.signature)
      ) {
        throw new RunnerVolumePurgeError("invalid_command");
      }
    } catch (error) {
      if (error instanceof RunnerVolumePurgeError) throw error;
      throw new RunnerVolumePurgeError("invalid_command");
    }
  }

  private async prepareBarrierLocked(
    command: RunnerVolumePurgeCommand,
    commandSha256: string,
    purgeSubjectSha256: string,
    storage: LockedSubjectStorage,
    finalize: boolean,
  ): Promise<
    | {
        readonly kind: "replay";
        readonly ack: RunnerVolumePurgeAck;
        readonly legacyArtifactCount: number;
      }
    | {
        readonly kind: "pending";
        readonly journal: PurgeJournal;
        readonly tombstone: PurgeTombstone | undefined;
      }
  > {
    await this.residency.ensurePurgeDirectories();
    const journalPath = this.residency.journalPath(command.commandId);
    const tombstonePath = this.residency.tombstonePath(purgeSubjectSha256);
    const fencePath = this.residency.fencePath(purgeSubjectSha256);
    let journal = await this.readJournal(journalPath);
    if (
      journal &&
      !journalMatchesCommand(
        journal,
        command,
        commandSha256,
        purgeSubjectSha256,
      )
    ) {
      throw new RunnerVolumePurgeError("command_conflict");
    }
    if (journal?.ack) {
      this.assertAckMatchesCommand(
        journal.ack,
        command,
        commandSha256,
        purgeSubjectSha256,
      );
    }

    const tombstone = await this.readTombstone(tombstonePath);
    if (tombstone) {
      this.assertTombstoneIdentity(tombstone, purgeSubjectSha256);
      if (tombstone.purgeGeneration > command.purgeGeneration) {
        throw new RunnerVolumePurgeError("stale_command");
      }
      if (tombstone.purgeGeneration === command.purgeGeneration) {
        if (!tombstoneMatchesCommand(tombstone, command, commandSha256)) {
          throw new RunnerVolumePurgeError("command_conflict");
        }
        if (
          !journal ||
          !journal.ack ||
          !phaseAtLeast(journal.phase, "ack_signed") ||
          !ackRecoveryEvidenceMatches(tombstone.ack, journal.ack)
        ) {
          throw new RunnerVolumePurgeError("corrupt_state");
        }
        const replayAck = journal.ack;
        this.assertAckMatchesCommand(
          replayAck,
          command,
          commandSha256,
          purgeSubjectSha256,
        );
        let legacyArtifactCount = 0;
        if (finalize) {
          await this.assertSubjectInventoryEmpty(
            purgeSubjectSha256,
            journal,
            storage,
          );
        } else {
          const inventory = await storage.inventory();
          await this.assertPreparedSubjectInventoryEmpty(
            purgeSubjectSha256,
            journal,
            inventory,
          );
          legacyArtifactCount =
            inventory.legacyInventory.legacyArtifactCount;
        }
        if (!phaseAtLeast(journal.phase, "completed")) {
          journal = await this.saveJournal(journalPath, {
            ...journal,
            phase: "completed",
          });
        }
        return { kind: "replay", ack: replayAck, legacyArtifactCount };
      }
    }
    if (journal && phaseAtLeast(journal.phase, "tombstoned")) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }

    const existingFence = await this.readFence(fencePath);
    if (existingFence) {
      this.assertFenceIdentity(existingFence, purgeSubjectSha256);
      if (existingFence.purgeGeneration > command.purgeGeneration) {
        throw new RunnerVolumePurgeError("stale_command");
      }
      if (
        existingFence.purgeGeneration === command.purgeGeneration &&
        !fenceMatchesCommand(existingFence, command, commandSha256)
      ) {
        throw new RunnerVolumePurgeError("command_conflict");
      }
    }

    if (!journal) {
      const initial = createInitialJournal(
        command,
        commandSha256,
        purgeSubjectSha256,
      );
      const created = await writeDurableFileExclusive(
        this.residency.root,
        journalPath,
        encodeJournal(signJournal(this.residency, initial)),
      );
      const persisted = await this.readJournal(journalPath);
      if (
        !persisted ||
        !journalMatchesCommand(
          persisted,
          command,
          commandSha256,
          purgeSubjectSha256,
        )
      ) {
        throw new RunnerVolumePurgeError(
          created ? "corrupt_state" : "command_conflict",
        );
      }
      journal = persisted;
      await this.inject("after_command_journal");
    }

    await this.ensureFence(
      fencePath,
      existingFence,
      command,
      commandSha256,
      purgeSubjectSha256,
    );
    if (!phaseAtLeast(journal.phase, "fenced")) {
      journal = await this.saveJournal(journalPath, {
        ...journal,
        phase: "fenced",
      });
      await this.inject("after_fence");
    }

    return { kind: "pending", journal, tombstone };
  }

  private async completeLocked(
    command: RunnerVolumePurgeCommand,
    commandSha256: string,
    purgeSubjectSha256: string,
    initialJournal: PurgeJournal,
    tombstone: PurgeTombstone | undefined,
    storage: LockedSubjectStorage,
  ): Promise<RunnerVolumePurgeAck> {
    const journalPath = this.residency.journalPath(command.commandId);
    const tombstonePath = this.residency.tombstonePath(purgeSubjectSha256);
    let journal = initialJournal;
    if (!phaseAtLeast(journal.phase, "active_work_stopped")) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }

    const locators =
      await this.residency.locatorsForSubjectHash(purgeSubjectSha256);
    const locatorState = runnerPurgeLocatorEvidence(locators);
    if (journal.storageEvidenceBefore) {
      if (
        journal.storageEvidenceBefore.locators.count !== locatorState.count ||
        journal.storageEvidenceBefore.locators.sha256 !== locatorState.sha256
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
    }
    if (!journal.storageEvidenceBefore) {
      const before = createRunnerPurgeStorageBeforeEvidence(
        await storage.inventory(),
        locators,
      );
      journal = await this.saveJournal(journalPath, {
        ...journal,
        phase: "inventory_recorded",
        storageEvidenceBefore: before,
      });
      await this.inject("after_before_inventory");
    }

    const storageEvidenceBefore = journal.storageEvidenceBefore;
    if (!storageEvidenceBefore) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }

    let ack = journal.ack;
    if (!ack) {
      let evidence = journal.storageEvidence;
      if (!evidence) {
        if (!phaseAtLeast(journal.phase, "deleting")) {
          journal = await this.saveJournal(journalPath, {
            ...journal,
            phase: "deleting",
          });
        }
        await storage.removeLegacyTargets(locators, () =>
          this.inject("after_delete_target"),
        );
        const removal = await storage.remove();
        await this.inject("after_delete_target");
        evidence = completeRunnerPurgeStorageEvidence(
          storageEvidenceBefore,
          removal.after,
          locators,
        );
        const storageEvidenceSha256 =
          runnerPurgeStorageEvidenceSha256(evidence);
        journal = await this.saveJournal(journalPath, {
          ...journal,
          phase: "zero_verified",
          storageEvidence: evidence,
          storageEvidenceSha256,
        });
        await this.inject("after_zero_rescan");
      } else {
        await this.assertSubjectInventoryEmpty(
          purgeSubjectSha256,
          journal,
          storage,
        );
      }
      ack = this.createAck(
        command,
        commandSha256,
        purgeSubjectSha256,
        evidence,
      );
      journal = await this.saveJournal(journalPath, {
        ...journal,
        phase: "ack_signed",
        ack,
      });
      await this.inject("after_ack_journal");
    } else {
      this.assertAckMatchesCommand(
        ack,
        command,
        commandSha256,
        purgeSubjectSha256,
      );
      await this.assertSubjectInventoryEmpty(
        purgeSubjectSha256,
        journal,
        storage,
      );
    }

    await this.persistTombstone(
      tombstonePath,
      tombstone,
      command,
      commandSha256,
      purgeSubjectSha256,
      ack,
    );
    if (!phaseAtLeast(journal.phase, "tombstoned")) {
      journal = await this.saveJournal(journalPath, {
        ...journal,
        phase: "tombstoned",
      });
    }
    await this.inject("after_tombstone");
    if (!phaseAtLeast(journal.phase, "completed")) {
      await this.saveJournal(journalPath, { ...journal, phase: "completed" });
    }
    await this.inject("after_completed_journal");
    return ack;
  }

  private async assertSubjectInventoryEmpty(
    purgeSubjectSha256: string,
    journal: PurgeJournal,
    storage: LockedSubjectStorage,
  ): Promise<void> {
    const locators =
      await this.residency.locatorsForSubjectHash(purgeSubjectSha256);
    const locatorState = runnerPurgeLocatorEvidence(locators);
    if (
      !journal.storageEvidenceBefore ||
      !journal.storageEvidence ||
      !journal.storageEvidenceSha256 ||
      journal.storageEvidenceBefore.locators.count !== locatorState.count ||
      journal.storageEvidenceBefore.locators.sha256 !== locatorState.sha256 ||
      runnerPurgeStorageEvidenceSha256(journal.storageEvidence) !==
        journal.storageEvidenceSha256
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    try {
      completeRunnerPurgeStorageEvidence(
        journal.storageEvidenceBefore,
        await storage.inventory(),
        locators,
      );
    } catch {
      throw new RunnerVolumePurgeError("restored_data");
    }
  }

  private async assertPreparedSubjectInventoryEmpty(
    purgeSubjectSha256: string,
    journal: PurgeJournal,
    inventory: SubjectStorageInventory,
  ): Promise<void> {
    const locators = journal.storageEvidenceBefore?.locators;
    if (
      !journal.storageEvidenceBefore ||
      !journal.storageEvidence ||
      !journal.storageEvidenceSha256 ||
      runnerPurgeStorageEvidenceSha256(journal.storageEvidence) !==
        journal.storageEvidenceSha256 ||
      locators === undefined
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    const currentLocators =
      await this.residency.locatorsForSubjectHash(purgeSubjectSha256);
    const locatorState = runnerPurgeLocatorEvidence(currentLocators);
    if (
      locators.count !== locatorState.count ||
      locators.sha256 !== locatorState.sha256
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    try {
      assertRunnerPurgeCurrentTargetStorageEmpty(inventory, currentLocators);
    } catch {
      throw new RunnerVolumePurgeError("restored_data");
    }
  }

  private createAck(
    command: RunnerVolumePurgeCommand,
    commandSha256: string,
    purgeSubjectSha256: string,
    storageEvidence: RunnerPurgeStorageEvidence,
    minimumCompletedAtMs = command.issuedAtMs,
  ): RunnerVolumePurgeAck {
    const completedAtMs = Math.max(
      (this.options.nowMs ?? Date.now)(),
      command.issuedAtMs,
      minimumCompletedAtMs,
    );
    assertNonNegativeInteger(completedAtMs, "corrupt_state");
    const storageEvidenceSha256 =
      runnerPurgeStorageEvidenceSha256(storageEvidence);
    const beforeInventory = runnerPurgeTargetInventoryState(
      storageEvidence,
      "before",
    );
    const afterInventory = runnerPurgeTargetInventoryState(
      storageEvidence,
      "after",
    );
    if (
      afterInventory.count !== 0 ||
      afterInventory.sha256 !== EMPTY_RUNNER_INVENTORY_SHA256
    ) {
      throw new RunnerVolumePurgeError("incomplete_purge");
    }
    const unsigned: UnsignedRunnerVolumePurgeAck = {
      version: 2,
      audience: RUNNER_VOLUME_PURGE_ACK_AUDIENCE,
      requestId: command.requestId,
      commandId: command.commandId,
      commandSha256,
      targetVolumeId: command.targetVolumeId,
      targetKeyFingerprint: command.targetKeyFingerprint,
      enrollmentEpoch: command.enrollmentEpoch,
      processInstanceId: this.options.processInstanceId,
      purgeSubjectSha256,
      purgeGeneration: command.purgeGeneration,
      storageEvidence,
      storageEvidenceSha256,
      beforeInventoryCount: beforeInventory.count,
      beforeInventorySha256: beforeInventory.sha256,
      afterInventoryCount: 0,
      afterInventorySha256: EMPTY_RUNNER_INVENTORY_SHA256,
      removedCount: beforeInventory.count,
      runnerBuildId: this.options.runnerBuildId,
      completedAtMs,
    };
    return {
      ...unsigned,
      signature: signEd25519(
        this.residency.identity.privateKey,
        canonicalRunnerVolumePurgeAck(unsigned),
      ),
    };
  }

  private assertAckMatchesCommand(
    ack: RunnerVolumePurgeAck,
    command: RunnerVolumePurgeCommand,
    commandSha256: string,
    purgeSubjectSha256: string,
  ): void {
    if (
      ack.requestId !== command.requestId ||
      ack.commandId !== command.commandId ||
      ack.commandSha256 !== commandSha256 ||
      ack.targetVolumeId !== command.targetVolumeId ||
      ack.targetKeyFingerprint !== command.targetKeyFingerprint ||
      ack.enrollmentEpoch !== command.enrollmentEpoch ||
      ack.purgeSubjectSha256 !== purgeSubjectSha256 ||
      ack.purgeGeneration !== command.purgeGeneration
    ) {
      throw new RunnerVolumePurgeError("ack_conflict");
    }
  }

  private async ensureFence(
    path: string,
    existing: PurgeFence | undefined,
    command: RunnerVolumePurgeCommand,
    commandSha256: string,
    purgeSubjectSha256: string,
  ): Promise<void> {
    const unsigned: UnsignedPurgeFence = {
      version: 1,
      audience: FENCE_AUDIENCE,
      volumeId: this.residency.identity.volumeId,
      volumeKeyFingerprint: this.residency.identity.publicKeyFingerprint,
      requestId: command.requestId,
      commandId: command.commandId,
      commandSha256,
      purgeSubjectSha256,
      purgeGeneration: command.purgeGeneration,
    };
    const fence = signFence(this.residency, unsigned);
    if (!existing) {
      const created = await writeDurableFileExclusive(
        this.residency.root,
        path,
        encodeFence(fence),
      );
      if (!created) {
        const raced = await this.readFence(path);
        if (!raced) throw new RunnerVolumePurgeError("corrupt_state");
        this.assertFenceIdentity(raced, purgeSubjectSha256);
        if (!fenceMatchesCommand(raced, command, commandSha256)) {
          throw new RunnerVolumePurgeError("command_conflict");
        }
      }
      return;
    }
    if (existing.purgeGeneration === command.purgeGeneration) return;
    await replaceDurableFile(this.residency.root, path, encodeFence(fence));
  }

  private async persistTombstone(
    path: string,
    existing: PurgeTombstone | undefined,
    command: RunnerVolumePurgeCommand,
    commandSha256: string,
    purgeSubjectSha256: string,
    ack: RunnerVolumePurgeAck,
  ): Promise<void> {
    const unsigned: UnsignedPurgeTombstone = {
      version: 1,
      audience: TOMBSTONE_AUDIENCE,
      volumeId: this.residency.identity.volumeId,
      volumeKeyFingerprint: this.residency.identity.publicKeyFingerprint,
      requestId: command.requestId,
      commandId: command.commandId,
      commandSha256,
      purgeSubjectSha256,
      purgeGeneration: command.purgeGeneration,
      retentionPolicy: TOMBSTONE_RETENTION_POLICY,
      ack,
    };
    const tombstone = signTombstone(this.residency, unsigned);
    if (!existing) {
      const created = await writeDurableFileExclusive(
        this.residency.root,
        path,
        encodeTombstone(tombstone),
      );
      if (!created) {
        const raced = await this.readTombstone(path);
        if (!raced) throw new RunnerVolumePurgeError("corrupt_state");
        this.assertTombstoneIdentity(raced, purgeSubjectSha256);
        if (
          !tombstoneMatchesCommand(raced, command, commandSha256) ||
          !encodeAck(raced.ack).equals(encodeAck(ack))
        ) {
          throw new RunnerVolumePurgeError("command_conflict");
        }
      }
      return;
    }
    if (existing.purgeGeneration === command.purgeGeneration) {
      if (!encodeAck(existing.ack).equals(encodeAck(ack))) {
        throw new RunnerVolumePurgeError("ack_conflict");
      }
      return;
    }
    await replaceDurableFile(
      this.residency.root,
      path,
      encodeTombstone(tombstone),
    );
  }

  private async saveJournal(
    path: string,
    journal: Omit<PurgeJournal, "localSignature">,
  ): Promise<PurgeJournal> {
    const withPossibleOldSignature = journal as Omit<
      PurgeJournal,
      "localSignature"
    > & {
      readonly localSignature?: string;
    };
    const { localSignature: _discarded, ...unsigned } =
      withPossibleOldSignature;
    const signed = signJournal(this.residency, unsigned);
    await replaceDurableFile(this.residency.root, path, encodeJournal(signed));
    return signed;
  }

  private async readJournal(path: string): Promise<PurgeJournal | undefined> {
    const encoded = await readBoundedRegularFile(
      this.residency.root,
      path,
      MAXIMUM_CONTROL_RECORD_BYTES,
    );
    return encoded ? this.parseJournal(encoded) : undefined;
  }

  private async readFence(path: string): Promise<PurgeFence | undefined> {
    const encoded = await readBoundedRegularFile(
      this.residency.root,
      path,
      MAXIMUM_CONTROL_RECORD_BYTES,
    );
    return encoded ? this.parseFence(encoded) : undefined;
  }

  private async readTombstone(
    path: string,
  ): Promise<PurgeTombstone | undefined> {
    const encoded = await readBoundedRegularFile(
      this.residency.root,
      path,
      MAXIMUM_CONTROL_RECORD_BYTES,
    );
    return encoded ? this.parseTombstone(encoded) : undefined;
  }

  private parseJournal(encoded: Buffer): PurgeJournal {
    try {
      const parsed = parseJsonRecord(encoded);
      if (
        !hasExactKeys(parsed, [
          "ack",
          "audience",
          "commandAudience",
          "commandId",
          "commandSha256",
          "enrollmentEpoch",
          "issuedAtMs",
          "legacyInventoryAuthorityGeneration",
          "legacyInventoryAuthoritySha256",
          "localSignature",
          "minimumRunnerBuildId",
          "phase",
          "purgeGeneration",
          "purgeSubjectSha256",
          "requestId",
          "serverKeyId",
          "serverSignature",
          "storageEvidence",
          "storageEvidenceBefore",
          "storageEvidenceSha256",
          "storageEvidenceVersion",
          "subjectStorageLayoutVersion",
          "targetKeyFingerprint",
          "targetVolumeId",
          "version",
        ])
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const unsigned = parseUnsignedJournal(parsed, (value) =>
        this.parseAck(value),
      );
      if (typeof parsed.localSignature !== "string") {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const journal: PurgeJournal = {
        ...unsigned,
        localSignature: parsed.localSignature,
      };
      if (
        !verifyLocalRecord(
          this.residency,
          JOURNAL_AUDIENCE,
          unsigned,
          journal.localSignature,
        ) ||
        !encoded.equals(encodeJournal(journal))
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      assertJournalPhaseState(journal);
      return journal;
    } catch (error) {
      if (error instanceof RunnerVolumePurgeError) throw error;
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private parseFence(encoded: Buffer): PurgeFence {
    try {
      const parsed = parseJsonRecord(encoded);
      if (
        !hasExactKeys(parsed, [
          "audience",
          "commandId",
          "commandSha256",
          "localSignature",
          "purgeGeneration",
          "purgeSubjectSha256",
          "requestId",
          "version",
          "volumeId",
          "volumeKeyFingerprint",
        ])
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const unsigned = parseUnsignedFence(parsed);
      if (typeof parsed.localSignature !== "string") {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const fence: PurgeFence = {
        ...unsigned,
        localSignature: parsed.localSignature,
      };
      if (
        !verifyLocalRecord(
          this.residency,
          FENCE_AUDIENCE,
          unsigned,
          fence.localSignature,
        ) ||
        !encoded.equals(encodeFence(fence))
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      return fence;
    } catch (error) {
      if (error instanceof RunnerVolumePurgeError) throw error;
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private parseTombstone(encoded: Buffer): PurgeTombstone {
    try {
      const parsed = parseJsonRecord(encoded);
      if (
        !hasExactKeys(parsed, [
          "ack",
          "audience",
          "commandId",
          "commandSha256",
          "localSignature",
          "purgeGeneration",
          "purgeSubjectSha256",
          "requestId",
          "retentionPolicy",
          "version",
          "volumeId",
          "volumeKeyFingerprint",
        ])
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const unsigned = parseUnsignedTombstone(parsed, (value) =>
        this.parseAck(value),
      );
      if (typeof parsed.localSignature !== "string") {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      const tombstone: PurgeTombstone = {
        ...unsigned,
        localSignature: parsed.localSignature,
      };
      if (
        !verifyLocalRecord(
          this.residency,
          TOMBSTONE_AUDIENCE,
          unsigned,
          tombstone.localSignature,
        ) ||
        !encoded.equals(encodeTombstone(tombstone))
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      return tombstone;
    } catch (error) {
      if (error instanceof RunnerVolumePurgeError) throw error;
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private parseAck(value: unknown): RunnerVolumePurgeAck {
    try {
      const ack = parseRunnerVolumePurgeAck(value);
      const { signature: _signature, ...unsigned } = ack;
      if (
        !verifyEd25519(
          this.residency.identity.publicKeyRaw,
          canonicalRunnerVolumePurgeAck(unsigned),
          ack.signature,
        )
      ) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      return ack;
    } catch (error) {
      if (error instanceof RunnerVolumePurgeError) throw error;
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private assertFenceIdentity(
    fence: PurgeFence,
    purgeSubjectSha256: string,
  ): void {
    if (
      fence.volumeId !== this.residency.identity.volumeId ||
      fence.volumeKeyFingerprint !==
        this.residency.identity.publicKeyFingerprint ||
      fence.purgeSubjectSha256 !== purgeSubjectSha256
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private assertTombstoneIdentity(
    tombstone: PurgeTombstone,
    purgeSubjectSha256: string,
  ): void {
    if (
      tombstone.volumeId !== this.residency.identity.volumeId ||
      tombstone.volumeKeyFingerprint !==
        this.residency.identity.publicKeyFingerprint ||
      tombstone.purgeSubjectSha256 !== purgeSubjectSha256 ||
      tombstone.ack.requestId !== tombstone.requestId ||
      tombstone.ack.commandId !== tombstone.commandId ||
      tombstone.ack.commandSha256 !== tombstone.commandSha256 ||
      tombstone.ack.targetVolumeId !== tombstone.volumeId ||
      tombstone.ack.targetKeyFingerprint !== tombstone.volumeKeyFingerprint ||
      tombstone.ack.purgeSubjectSha256 !== tombstone.purgeSubjectSha256 ||
      tombstone.ack.purgeGeneration !== tombstone.purgeGeneration
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
  }

  private async inject(phase: RunnerVolumePurgeFaultPhase): Promise<void> {
    await this.options.faultInjector?.(phase);
  }
}

export function canonicalRunnerVolumePurgeCommand(
  command: UnsignedRunnerVolumePurgeCommand,
): Buffer {
  return Buffer.from(
    [
      "bluey-jobs-runner-volume-purge-command-v2",
      `version=${command.version}`,
      `request_id=${command.requestId}`,
      `audience=${command.audience}`,
      `command_id=${command.commandId}`,
      `target_volume_id=${command.targetVolumeId}`,
      `target_key_fingerprint=${command.targetKeyFingerprint}`,
      `enrollment_epoch=${command.enrollmentEpoch}`,
      `purge_subject=${command.purgeSubject}`,
      `purge_generation=${command.purgeGeneration}`,
      `storage_evidence_version=${command.storageEvidenceVersion}`,
      `subject_storage_layout_version=${command.subjectStorageLayoutVersion}`,
      `legacy_inventory_authority_generation=${command.legacyInventoryAuthorityGeneration}`,
      `legacy_inventory_authority_sha256=${command.legacyInventoryAuthoritySha256}`,
      `issued_at_ms=${command.issuedAtMs}`,
      `minimum_runner_build_id=${command.minimumRunnerBuildId}`,
      `server_key_id=${command.serverKeyId}`,
      "",
    ].join("\n"),
    "utf8",
  );
}

export function canonicalRunnerVolumePurgeAck(
  ack: UnsignedRunnerVolumePurgeAck,
): Buffer {
  return Buffer.from(
    [
      "bluey-jobs-runner-volume-purge-ack-v2",
      `version=${ack.version}`,
      `audience=${ack.audience}`,
      `request_id=${ack.requestId}`,
      `command_id=${ack.commandId}`,
      `command_sha256=${ack.commandSha256}`,
      `target_volume_id=${ack.targetVolumeId}`,
      `target_key_fingerprint=${ack.targetKeyFingerprint}`,
      `enrollment_epoch=${ack.enrollmentEpoch}`,
      `process_instance_id=${ack.processInstanceId}`,
      `purge_subject_sha256=${ack.purgeSubjectSha256}`,
      `purge_generation=${ack.purgeGeneration}`,
      `storage_evidence_sha256=${ack.storageEvidenceSha256}`,
      `before_inventory_count=${ack.beforeInventoryCount}`,
      `before_inventory_sha256=${ack.beforeInventorySha256}`,
      `after_inventory_count=${ack.afterInventoryCount}`,
      `after_inventory_sha256=${ack.afterInventorySha256}`,
      `removed_count=${ack.removedCount}`,
      `runner_build_id=${ack.runnerBuildId}`,
      `completed_at_ms=${ack.completedAtMs}`,
      "",
    ].join("\n"),
    "utf8",
  );
}

export async function runnerPurgeTargetsForLocators(
  root: RunnerDataRoot,
  locators: readonly AccountResidencyLocator[],
): Promise<readonly string[]> {
  const targets = new Set<string>();
  for (const locator of locators) {
    if (locator.kind === "profile") {
      if (!PROFILE_SCOPE_PATTERN.test(locator.scope)) {
        throw new RunnerVolumePurgeError("corrupt_state");
      }
      targets.add(runnerPath(root, "active", locator.scope));
      targets.add(
        runnerPath(root, "active", `${locator.scope}.restore.tar.gz`),
      );
      targets.add(runnerPath(root, "active", `${locator.scope}.seal.tar.gz`));
      await addMatchingRemnants(root, targets, "active", [
        `${locator.scope}.restore.tar.gz`,
        `${locator.scope}.seal.tar.gz`,
      ]);

      targets.add(runnerPath(root, "snapshots", `${locator.scope}.tar.gz.enc`));
      targets.add(runnerPath(root, "snapshots", `${locator.scope}.generation`));
      await addMatchingRemnants(root, targets, "snapshots", [
        `${locator.scope}.tar.gz.enc`,
        `${locator.scope}.generation`,
      ]);
      targets.add(runnerPath(root, "run-checkpoints", locator.scope));
      targets.add(runnerPath(root, "receipts", locator.scope));
      continue;
    }
    if (
      locator.kind !== "result" ||
      !RESULT_SCOPE_PATTERN.test(locator.scope)
    ) {
      throw new RunnerVolumePurgeError("corrupt_state");
    }
    const resultName = `${locator.scope}.json.enc`;
    targets.add(runnerPath(root, "step-results", resultName));
    await addMatchingRemnants(root, targets, "step-results", [resultName]);
  }
  return [...targets].sort((left, right) => left.localeCompare(right));
}

export function parseRunnerVolumePurgeCommand(
  input: unknown,
): RunnerVolumePurgeCommand {
  try {
    if (
      !isRecord(input) ||
      !hasExactKeys(input, [
        "audience",
        "commandId",
        "enrollmentEpoch",
        "issuedAtMs",
        "legacyInventoryAuthorityGeneration",
        "legacyInventoryAuthoritySha256",
        "minimumRunnerBuildId",
        "purgeGeneration",
        "purgeSubject",
        "requestId",
        "serverKeyId",
        "signature",
        "storageEvidenceVersion",
        "subjectStorageLayoutVersion",
        "targetKeyFingerprint",
        "targetVolumeId",
        "version",
      ]) ||
      input.version !== 2 ||
      input.audience !== RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE ||
      typeof input.requestId !== "string" ||
      typeof input.commandId !== "string" ||
      typeof input.targetVolumeId !== "string" ||
      typeof input.targetKeyFingerprint !== "string" ||
      typeof input.enrollmentEpoch !== "number" ||
      typeof input.purgeSubject !== "string" ||
      typeof input.purgeGeneration !== "number" ||
      input.storageEvidenceVersion !== RUNNER_PURGE_STORAGE_EVIDENCE_VERSION ||
      input.subjectStorageLayoutVersion !== SUBJECT_STORAGE_LAYOUT_VERSION ||
      typeof input.legacyInventoryAuthorityGeneration !== "number" ||
      typeof input.legacyInventoryAuthoritySha256 !== "string" ||
      typeof input.issuedAtMs !== "number" ||
      typeof input.minimumRunnerBuildId !== "string" ||
      typeof input.serverKeyId !== "string" ||
      typeof input.signature !== "string"
    ) {
      throw new RunnerVolumePurgeError("invalid_command");
    }
    assertSafeIdentifier(input.requestId, "invalid_command");
    assertSafeIdentifier(input.commandId, "invalid_command");
    assertCanonicalBytes(input.targetVolumeId, 32, "invalid_command");
    assertSha256(input.targetKeyFingerprint, "invalid_command");
    assertPositiveInteger(input.enrollmentEpoch, "invalid_command");
    assertCanonicalBytes(input.purgeSubject, 32, "invalid_command");
    assertPositiveInteger(input.purgeGeneration, "invalid_command");
    assertLegacyInventoryAuthorityBinding(
      input.legacyInventoryAuthorityGeneration,
      input.legacyInventoryAuthoritySha256,
      "invalid_command",
    );
    assertNonNegativeInteger(input.issuedAtMs, "invalid_command");
    assertRunnerBuildId(input.minimumRunnerBuildId, "invalid_command");
    assertSafeIdentifier(input.serverKeyId, "invalid_command");
    assertCanonicalBytes(input.signature, 64, "invalid_command");
    return {
      version: 2,
      requestId: input.requestId,
      audience: RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
      commandId: input.commandId,
      targetVolumeId: input.targetVolumeId,
      targetKeyFingerprint: input.targetKeyFingerprint,
      enrollmentEpoch: input.enrollmentEpoch,
      purgeSubject: input.purgeSubject,
      purgeGeneration: input.purgeGeneration,
      storageEvidenceVersion: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
      subjectStorageLayoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
      legacyInventoryAuthorityGeneration:
        input.legacyInventoryAuthorityGeneration,
      legacyInventoryAuthoritySha256: input.legacyInventoryAuthoritySha256,
      issuedAtMs: input.issuedAtMs,
      minimumRunnerBuildId: input.minimumRunnerBuildId,
      serverKeyId: input.serverKeyId,
      signature: input.signature,
    };
  } catch (error) {
    if (error instanceof RunnerVolumePurgeError) throw error;
    throw new RunnerVolumePurgeError("invalid_command");
  }
}

export function parseRunnerVolumePurgeAck(
  input: unknown,
): RunnerVolumePurgeAck {
  if (
    !isRecord(input) ||
    !hasExactKeys(input, [
      "afterInventoryCount",
      "afterInventorySha256",
      "audience",
      "beforeInventoryCount",
      "beforeInventorySha256",
      "commandId",
      "commandSha256",
      "completedAtMs",
      "enrollmentEpoch",
      "processInstanceId",
      "purgeGeneration",
      "purgeSubjectSha256",
      "removedCount",
      "requestId",
      "runnerBuildId",
      "signature",
      "storageEvidence",
      "storageEvidenceSha256",
      "targetKeyFingerprint",
      "targetVolumeId",
      "version",
    ]) ||
    input.version !== 2 ||
    input.audience !== RUNNER_VOLUME_PURGE_ACK_AUDIENCE ||
    typeof input.requestId !== "string" ||
    typeof input.commandId !== "string" ||
    typeof input.commandSha256 !== "string" ||
    typeof input.targetVolumeId !== "string" ||
    typeof input.targetKeyFingerprint !== "string" ||
    typeof input.enrollmentEpoch !== "number" ||
    typeof input.processInstanceId !== "string" ||
    typeof input.purgeSubjectSha256 !== "string" ||
    typeof input.purgeGeneration !== "number" ||
    typeof input.storageEvidenceSha256 !== "string" ||
    typeof input.beforeInventoryCount !== "number" ||
    typeof input.beforeInventorySha256 !== "string" ||
    input.afterInventoryCount !== 0 ||
    input.afterInventorySha256 !== EMPTY_RUNNER_INVENTORY_SHA256 ||
    typeof input.removedCount !== "number" ||
    typeof input.runnerBuildId !== "string" ||
    typeof input.completedAtMs !== "number" ||
    typeof input.signature !== "string"
  ) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
  assertSafeIdentifier(input.requestId, "corrupt_state");
  assertSafeIdentifier(input.commandId, "corrupt_state");
  assertSha256(input.commandSha256, "corrupt_state");
  assertCanonicalBytes(input.targetVolumeId, 32, "corrupt_state");
  assertSha256(input.targetKeyFingerprint, "corrupt_state");
  assertPositiveInteger(input.enrollmentEpoch, "corrupt_state");
  assertCanonicalBytes(input.processInstanceId, 32, "corrupt_state");
  assertSha256(input.purgeSubjectSha256, "corrupt_state");
  assertPositiveInteger(input.purgeGeneration, "corrupt_state");
  const storageEvidence = parseRunnerPurgeStorageEvidence(
    input.storageEvidence,
  );
  assertSha256(input.storageEvidenceSha256, "corrupt_state");
  if (
    runnerPurgeStorageEvidenceSha256(storageEvidence) !==
    input.storageEvidenceSha256
  ) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
  assertNonNegativeInteger(input.beforeInventoryCount, "corrupt_state");
  assertSha256(input.beforeInventorySha256, "corrupt_state");
  const beforeInventory = runnerPurgeTargetInventoryState(
    storageEvidence,
    "before",
  );
  const afterInventory = runnerPurgeTargetInventoryState(
    storageEvidence,
    "after",
  );
  assertNonNegativeInteger(input.removedCount, "corrupt_state");
  if (
    input.beforeInventoryCount !== beforeInventory.count ||
    input.beforeInventorySha256 !== beforeInventory.sha256 ||
    input.afterInventoryCount !== afterInventory.count ||
    input.afterInventorySha256 !== afterInventory.sha256 ||
    input.removedCount !==
      input.beforeInventoryCount - input.afterInventoryCount
  ) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
  assertRunnerBuildId(input.runnerBuildId, "corrupt_state");
  assertNonNegativeInteger(input.completedAtMs, "corrupt_state");
  assertCanonicalBytes(input.signature, 64, "corrupt_state");
  return {
    version: 2,
    audience: RUNNER_VOLUME_PURGE_ACK_AUDIENCE,
    requestId: input.requestId,
    commandId: input.commandId,
    commandSha256: input.commandSha256,
    targetVolumeId: input.targetVolumeId,
    targetKeyFingerprint: input.targetKeyFingerprint,
    enrollmentEpoch: input.enrollmentEpoch,
    processInstanceId: input.processInstanceId,
    purgeSubjectSha256: input.purgeSubjectSha256,
    purgeGeneration: input.purgeGeneration,
    storageEvidence,
    storageEvidenceSha256: input.storageEvidenceSha256,
    beforeInventoryCount: input.beforeInventoryCount,
    beforeInventorySha256: input.beforeInventorySha256,
    afterInventoryCount: 0,
    afterInventorySha256: EMPTY_RUNNER_INVENTORY_SHA256,
    removedCount: input.removedCount,
    runnerBuildId: input.runnerBuildId,
    completedAtMs: input.completedAtMs,
    signature: input.signature,
  };
}

async function addMatchingRemnants(
  root: RunnerDataRoot,
  targets: Set<string>,
  directoryName: string,
  exactPrefixes: readonly string[],
): Promise<void> {
  const directory = runnerPath(root, directoryName);
  const entries = await listSafeRunnerDirectory(root, directory);
  for (const entry of entries) {
    if (exactPrefixes.some((prefix) => entry.startsWith(`${prefix}.`))) {
      targets.add(runnerPath(root, directoryName, entry));
    }
  }
}

function createInitialJournal(
  command: RunnerVolumePurgeCommand,
  commandSha256: string,
  purgeSubjectSha256: string,
): UnsignedPurgeJournal {
  return {
    version: 2,
    audience: JOURNAL_AUDIENCE,
    requestId: command.requestId,
    commandId: command.commandId,
    commandSha256,
    commandAudience: command.audience,
    targetVolumeId: command.targetVolumeId,
    targetKeyFingerprint: command.targetKeyFingerprint,
    enrollmentEpoch: command.enrollmentEpoch,
    purgeSubjectSha256,
    purgeGeneration: command.purgeGeneration,
    storageEvidenceVersion: command.storageEvidenceVersion,
    subjectStorageLayoutVersion: command.subjectStorageLayoutVersion,
    legacyInventoryAuthorityGeneration:
      command.legacyInventoryAuthorityGeneration,
    legacyInventoryAuthoritySha256: command.legacyInventoryAuthoritySha256,
    issuedAtMs: command.issuedAtMs,
    minimumRunnerBuildId: command.minimumRunnerBuildId,
    serverKeyId: command.serverKeyId,
    serverSignature: command.signature,
    phase: "commanded",
    storageEvidenceBefore: null,
    storageEvidence: null,
    storageEvidenceSha256: null,
    ack: null,
  };
}

function signJournal(
  residency: AccountResidencyIndex,
  journal: Omit<PurgeJournal, "localSignature">,
): PurgeJournal {
  return {
    ...journal,
    localSignature: signLocalRecord(residency, JOURNAL_AUDIENCE, journal),
  };
}

function signFence(
  residency: AccountResidencyIndex,
  fence: UnsignedPurgeFence,
): PurgeFence {
  return {
    ...fence,
    localSignature: signLocalRecord(residency, FENCE_AUDIENCE, fence),
  };
}

function signTombstone(
  residency: AccountResidencyIndex,
  tombstone: UnsignedPurgeTombstone,
): PurgeTombstone {
  return {
    ...tombstone,
    localSignature: signLocalRecord(residency, TOMBSTONE_AUDIENCE, tombstone),
  };
}

function signLocalRecord(
  residency: AccountResidencyIndex,
  audience: string,
  record: object,
): string {
  return signEd25519(
    residency.identity.privateKey,
    canonicalLocalRecord(audience, record),
  );
}

function verifyLocalRecord(
  residency: AccountResidencyIndex,
  audience: string,
  record: object,
  signature: string,
): boolean {
  return verifyEd25519(
    residency.identity.publicKeyRaw,
    canonicalLocalRecord(audience, record),
    signature,
  );
}

function canonicalLocalRecord(audience: string, record: object): Buffer {
  return Buffer.from(`${audience}\n${JSON.stringify(record)}\n`, "utf8");
}

function encodeJournal(journal: PurgeJournal): Buffer {
  return Buffer.from(`${JSON.stringify(journal)}\n`, "utf8");
}

function encodeFence(fence: PurgeFence): Buffer {
  return Buffer.from(`${JSON.stringify(fence)}\n`, "utf8");
}

function encodeTombstone(tombstone: PurgeTombstone): Buffer {
  return Buffer.from(`${JSON.stringify(tombstone)}\n`, "utf8");
}

function encodeAck(ack: RunnerVolumePurgeAck): Buffer {
  return Buffer.from(`${JSON.stringify(ack)}\n`, "utf8");
}

function parseUnsignedJournal(
  input: Record<string, unknown>,
  parseAck: (value: unknown) => RunnerVolumePurgeAck,
): UnsignedPurgeJournal {
  if (
    input.version !== 2 ||
    input.audience !== JOURNAL_AUDIENCE ||
    typeof input.requestId !== "string" ||
    typeof input.commandId !== "string" ||
    typeof input.commandSha256 !== "string" ||
    input.commandAudience !== RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE ||
    typeof input.targetVolumeId !== "string" ||
    typeof input.targetKeyFingerprint !== "string" ||
    typeof input.enrollmentEpoch !== "number" ||
    typeof input.purgeSubjectSha256 !== "string" ||
    typeof input.purgeGeneration !== "number" ||
    input.storageEvidenceVersion !== RUNNER_PURGE_STORAGE_EVIDENCE_VERSION ||
    input.subjectStorageLayoutVersion !== SUBJECT_STORAGE_LAYOUT_VERSION ||
    typeof input.legacyInventoryAuthorityGeneration !== "number" ||
    typeof input.legacyInventoryAuthoritySha256 !== "string" ||
    typeof input.issuedAtMs !== "number" ||
    typeof input.minimumRunnerBuildId !== "string" ||
    typeof input.serverKeyId !== "string" ||
    typeof input.serverSignature !== "string" ||
    !isJournalPhase(input.phase) ||
    (input.storageEvidenceSha256 !== null &&
      typeof input.storageEvidenceSha256 !== "string")
  ) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
  assertSafeIdentifier(input.requestId, "corrupt_state");
  assertSafeIdentifier(input.commandId, "corrupt_state");
  assertSha256(input.commandSha256, "corrupt_state");
  assertCanonicalBytes(input.targetVolumeId, 32, "corrupt_state");
  assertSha256(input.targetKeyFingerprint, "corrupt_state");
  assertPositiveInteger(input.enrollmentEpoch, "corrupt_state");
  assertSha256(input.purgeSubjectSha256, "corrupt_state");
  assertPositiveInteger(input.purgeGeneration, "corrupt_state");
  assertLegacyInventoryAuthorityBinding(
    input.legacyInventoryAuthorityGeneration,
    input.legacyInventoryAuthoritySha256,
    "corrupt_state",
  );
  assertNonNegativeInteger(input.issuedAtMs, "corrupt_state");
  assertSafeIdentifier(input.minimumRunnerBuildId, "corrupt_state");
  assertSafeIdentifier(input.serverKeyId, "corrupt_state");
  assertCanonicalBytes(input.serverSignature, 64, "corrupt_state");
  const storageEvidenceBefore =
    input.storageEvidenceBefore === null
      ? null
      : parseRunnerPurgeStorageBeforeEvidence(input.storageEvidenceBefore);
  const storageEvidence =
    input.storageEvidence === null
      ? null
      : parseRunnerPurgeStorageEvidence(input.storageEvidence);
  if (input.storageEvidenceSha256 !== null) {
    assertSha256(input.storageEvidenceSha256, "corrupt_state");
  }
  return {
    version: 2,
    audience: JOURNAL_AUDIENCE,
    requestId: input.requestId,
    commandId: input.commandId,
    commandSha256: input.commandSha256,
    commandAudience: RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
    targetVolumeId: input.targetVolumeId,
    targetKeyFingerprint: input.targetKeyFingerprint,
    enrollmentEpoch: input.enrollmentEpoch,
    purgeSubjectSha256: input.purgeSubjectSha256,
    purgeGeneration: input.purgeGeneration,
    storageEvidenceVersion: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
    subjectStorageLayoutVersion: SUBJECT_STORAGE_LAYOUT_VERSION,
    legacyInventoryAuthorityGeneration:
      input.legacyInventoryAuthorityGeneration,
    legacyInventoryAuthoritySha256: input.legacyInventoryAuthoritySha256,
    issuedAtMs: input.issuedAtMs,
    minimumRunnerBuildId: input.minimumRunnerBuildId,
    serverKeyId: input.serverKeyId,
    serverSignature: input.serverSignature,
    phase: input.phase,
    storageEvidenceBefore,
    storageEvidence,
    storageEvidenceSha256: input.storageEvidenceSha256,
    ack: input.ack === null ? null : parseAck(input.ack),
  };
}

function parseUnsignedFence(
  input: Record<string, unknown>,
): UnsignedPurgeFence {
  if (
    input.version !== 1 ||
    input.audience !== FENCE_AUDIENCE ||
    typeof input.volumeId !== "string" ||
    typeof input.volumeKeyFingerprint !== "string" ||
    typeof input.requestId !== "string" ||
    typeof input.commandId !== "string" ||
    typeof input.commandSha256 !== "string" ||
    typeof input.purgeSubjectSha256 !== "string" ||
    typeof input.purgeGeneration !== "number"
  ) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
  assertCanonicalBytes(input.volumeId, 32, "corrupt_state");
  assertSha256(input.volumeKeyFingerprint, "corrupt_state");
  assertSafeIdentifier(input.requestId, "corrupt_state");
  assertSafeIdentifier(input.commandId, "corrupt_state");
  assertSha256(input.commandSha256, "corrupt_state");
  assertSha256(input.purgeSubjectSha256, "corrupt_state");
  assertPositiveInteger(input.purgeGeneration, "corrupt_state");
  return {
    version: 1,
    audience: FENCE_AUDIENCE,
    volumeId: input.volumeId,
    volumeKeyFingerprint: input.volumeKeyFingerprint,
    requestId: input.requestId,
    commandId: input.commandId,
    commandSha256: input.commandSha256,
    purgeSubjectSha256: input.purgeSubjectSha256,
    purgeGeneration: input.purgeGeneration,
  };
}

function parseUnsignedTombstone(
  input: Record<string, unknown>,
  parseAck: (value: unknown) => RunnerVolumePurgeAck,
): UnsignedPurgeTombstone {
  if (input.retentionPolicy !== TOMBSTONE_RETENTION_POLICY) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
  const fence = parseUnsignedFence({ ...input, audience: FENCE_AUDIENCE });
  return {
    ...fence,
    audience: TOMBSTONE_AUDIENCE,
    retentionPolicy: TOMBSTONE_RETENTION_POLICY,
    ack: parseAck(input.ack),
  };
}

function journalMatchesCommand(
  journal: PurgeJournal,
  command: RunnerVolumePurgeCommand,
  commandSha256: string,
  purgeSubjectSha256: string,
): boolean {
  return (
    journal.requestId === command.requestId &&
    journal.commandId === command.commandId &&
    journal.commandSha256 === commandSha256 &&
    journal.commandAudience === command.audience &&
    journal.targetVolumeId === command.targetVolumeId &&
    journal.targetKeyFingerprint === command.targetKeyFingerprint &&
    journal.enrollmentEpoch === command.enrollmentEpoch &&
    journal.purgeSubjectSha256 === purgeSubjectSha256 &&
    journal.purgeGeneration === command.purgeGeneration &&
    journal.storageEvidenceVersion === command.storageEvidenceVersion &&
    journal.subjectStorageLayoutVersion ===
      command.subjectStorageLayoutVersion &&
    journal.legacyInventoryAuthorityGeneration ===
      command.legacyInventoryAuthorityGeneration &&
    journal.legacyInventoryAuthoritySha256 ===
      command.legacyInventoryAuthoritySha256 &&
    journal.issuedAtMs === command.issuedAtMs &&
    journal.minimumRunnerBuildId === command.minimumRunnerBuildId &&
    journal.serverKeyId === command.serverKeyId &&
    journal.serverSignature === command.signature
  );
}

function fenceMatchesCommand(
  fence: PurgeFence,
  command: RunnerVolumePurgeCommand,
  commandSha256: string,
): boolean {
  return (
    fence.requestId === command.requestId &&
    fence.commandId === command.commandId &&
    fence.commandSha256 === commandSha256 &&
    fence.purgeGeneration === command.purgeGeneration
  );
}

function tombstoneMatchesCommand(
  tombstone: PurgeTombstone,
  command: RunnerVolumePurgeCommand,
  commandSha256: string,
): boolean {
  return (
    tombstone.requestId === command.requestId &&
    tombstone.commandId === command.commandId &&
    tombstone.commandSha256 === commandSha256 &&
    tombstone.purgeGeneration === command.purgeGeneration
  );
}

/**
 * A recovered ACK may change only process lease, completion time, and its
 * Ed25519 signature. The immutable deletion evidence remains identical to the
 * ACK embedded in the original local tombstone.
 */
function ackRecoveryEvidenceMatches(
  original: RunnerVolumePurgeAck,
  candidate: RunnerVolumePurgeAck,
): boolean {
  return (
    original.version === candidate.version &&
    original.audience === candidate.audience &&
    original.requestId === candidate.requestId &&
    original.commandId === candidate.commandId &&
    original.commandSha256 === candidate.commandSha256 &&
    original.targetVolumeId === candidate.targetVolumeId &&
    original.targetKeyFingerprint === candidate.targetKeyFingerprint &&
    original.enrollmentEpoch === candidate.enrollmentEpoch &&
    original.purgeSubjectSha256 === candidate.purgeSubjectSha256 &&
    original.purgeGeneration === candidate.purgeGeneration &&
    original.storageEvidenceSha256 === candidate.storageEvidenceSha256 &&
    JSON.stringify(original.storageEvidence) ===
      JSON.stringify(candidate.storageEvidence) &&
    original.beforeInventoryCount === candidate.beforeInventoryCount &&
    original.beforeInventorySha256 === candidate.beforeInventorySha256 &&
    original.afterInventoryCount === candidate.afterInventoryCount &&
    original.afterInventorySha256 === candidate.afterInventorySha256 &&
    original.removedCount === candidate.removedCount &&
    original.runnerBuildId === candidate.runnerBuildId &&
    candidate.completedAtMs >= original.completedAtMs
  );
}

function assertJournalPhaseState(journal: PurgeJournal): void {
  const hasBefore = journal.storageEvidenceBefore !== null;
  const hasEvidence = journal.storageEvidence !== null;
  const hasEvidenceSha256 = journal.storageEvidenceSha256 !== null;
  if (
    hasEvidence !== hasEvidenceSha256 ||
    phaseAtLeast(journal.phase, "inventory_recorded") !== hasBefore ||
    phaseAtLeast(journal.phase, "zero_verified") !== hasEvidence ||
    (phaseAtLeast(journal.phase, "ack_signed") && !journal.ack) ||
    (!phaseAtLeast(journal.phase, "ack_signed") && journal.ack) ||
    (journal.storageEvidence !== null &&
      journal.storageEvidenceBefore !== null &&
      !storageEvidenceBeforeMatches(
        journal.storageEvidenceBefore,
        journal.storageEvidence,
      )) ||
    (journal.storageEvidence !== null &&
      runnerPurgeStorageEvidenceSha256(journal.storageEvidence) !==
        journal.storageEvidenceSha256) ||
    (journal.ack !== null &&
      (journal.storageEvidence === null ||
        journal.ack.storageEvidenceSha256 !== journal.storageEvidenceSha256 ||
        JSON.stringify(journal.ack.storageEvidence) !==
          JSON.stringify(journal.storageEvidence)))
  ) {
    throw new RunnerVolumePurgeError("corrupt_state");
  }
}

function storageEvidenceBeforeMatches(
  before: RunnerPurgeStorageBeforeEvidence,
  evidence: RunnerPurgeStorageEvidence,
): boolean {
  return (
    JSON.stringify(before) ===
    JSON.stringify({
      version: evidence.version,
      root: { deviceId: evidence.root.deviceId, before: evidence.root.before },
      locators: evidence.locators,
      subjectStorage: {
        layoutVersion: evidence.subjectStorage.layoutVersion,
        before: evidence.subjectStorage.before,
      },
      legacy: {
        inventoryVersion: evidence.legacy.inventoryVersion,
        targetBefore: evidence.legacy.targetBefore,
        rootBefore: evidence.legacy.rootBefore,
      },
    })
  );
}

function phaseAtLeast(actual: JournalPhase, expected: JournalPhase): boolean {
  return JOURNAL_PHASE_ORDER[actual] >= JOURNAL_PHASE_ORDER[expected];
}

function isJournalPhase(value: unknown): value is JournalPhase {
  return typeof value === "string" && Object.hasOwn(JOURNAL_PHASE_ORDER, value);
}

function parseJsonRecord(encoded: Buffer): Record<string, unknown> {
  const parsed: unknown = JSON.parse(encoded.toString("utf8"));
  if (!isRecord(parsed)) throw new RunnerVolumePurgeError("corrupt_state");
  return parsed;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const keys = Object.keys(value).sort((left, right) =>
    left.localeCompare(right),
  );
  const wanted = [...expected].sort((left, right) => left.localeCompare(right));
  return (
    keys.length === wanted.length &&
    keys.every((key, index) => key === wanted[index])
  );
}

function assertSafeIdentifier(
  value: string,
  errorCode: RunnerVolumePurgeErrorCode,
): void {
  if (typeof value !== "string" || !SAFE_IDENTIFIER_PATTERN.test(value)) {
    throw new RunnerVolumePurgeError(errorCode);
  }
}

export function runnerBuildSatisfies(actual: string, minimum: string): boolean {
  const actualVersion = parseRunnerBuildId(actual);
  const minimumVersion = parseRunnerBuildId(minimum);
  if (!actualVersion || !minimumVersion) return false;
  return (
    actualVersion.generation > minimumVersion.generation ||
    (actualVersion.generation === minimumVersion.generation &&
      actualVersion.revision >= minimumVersion.revision)
  );
}

function assertRunnerBuildId(
  value: string,
  errorCode: RunnerVolumePurgeErrorCode,
): void {
  if (!parseRunnerBuildId(value)) throw new RunnerVolumePurgeError(errorCode);
}

function parseRunnerBuildId(value: string):
  | {
      readonly generation: number;
      readonly revision: number;
    }
  | undefined {
  if (typeof value !== "string") return undefined;
  const match = RUNNER_BUILD_PATTERN.exec(value);
  if (!match) return undefined;
  const generation = Number(match[1]);
  const revision = Number(match[2] ?? "0");
  if (!Number.isSafeInteger(generation) || !Number.isSafeInteger(revision))
    return undefined;
  return { generation, revision };
}

function assertCanonicalBytes(
  value: string,
  byteLength: number,
  errorCode: RunnerVolumePurgeErrorCode,
): void {
  try {
    decodeCanonicalBase64Url(value, byteLength);
  } catch {
    throw new RunnerVolumePurgeError(errorCode);
  }
}

function assertSha256(
  value: string,
  errorCode: RunnerVolumePurgeErrorCode,
): void {
  if (typeof value !== "string" || !SHA256_PATTERN.test(value)) {
    throw new RunnerVolumePurgeError(errorCode);
  }
}

function assertLegacyInventoryAuthorityBinding(
  generation: number,
  sha256: string,
  errorCode: RunnerVolumePurgeErrorCode,
): void {
  assertNonNegativeInteger(generation, errorCode);
  assertSha256(sha256, errorCode);
  if (
    (generation === 0) !==
    (sha256 === ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256)
  ) {
    throw new RunnerVolumePurgeError(errorCode);
  }
}

function assertPositiveInteger(
  value: number,
  errorCode: RunnerVolumePurgeErrorCode,
): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new RunnerVolumePurgeError(errorCode);
  }
}

function assertNonNegativeInteger(
  value: number,
  errorCode: RunnerVolumePurgeErrorCode,
): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RunnerVolumePurgeError(errorCode);
  }
}
