import {
  createHash,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  randomBytes,
  sign,
  type KeyObject,
} from "node:crypto";
import {
  closeSync,
  lstatSync,
  openSync,
  realpathSync,
  type Stats,
} from "node:fs";
import {
  chmod,
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  symlink,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  AccountResidencyIndex,
  accountPurgeSubjectHash,
} from "../src/account-residency.js";
import {
  openRunnerDataRoot,
  type RunnerDataRoot,
} from "../src/safe-runner-storage.js";
import { createInjectedNativeRunnerStorageForTest } from "../src/native-runner-storage.js";
import { SubjectStorageManager } from "../src/subject-storage-manager.js";
import { EMPTY_LEGACY_ARTIFACT_SET_SHA256 } from "../src/legacy-runner-storage.js";
import { EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256 } from "../src/subject-storage-layout.js";
import {
  createRunnerPurgeStorageBeforeEvidence,
  runnerPurgeStorageBeforeEvidence,
  runnerPurgeStorageEvidenceSha256,
  runnerPurgeTargetInventoryState,
  type RunnerPurgeStorageEvidence,
} from "../src/purge-storage-evidence.js";
import {
  canonicalRunnerVolumePurgeAck,
  canonicalRunnerVolumePurgeCommand,
  ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256,
  EMPTY_RUNNER_INVENTORY_SHA256,
  runnerBuildSatisfies,
  RUNNER_VOLUME_PURGE_ACK_AUDIENCE,
  RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
  RunnerVolumePurger,
  type RunnerVolumePurgeCommand,
  type RunnerVolumePurgeFaultPhase,
  type UnsignedRunnerVolumePurgeAck,
  type UnsignedRunnerVolumePurgeCommand,
} from "../src/volume-purge.js";
import {
  createRunnerProcessInstanceId,
  loadOrCreateRunnerVolumeIdentity,
  verifyEd25519,
} from "../src/volume-identity.js";

const PROFILE_A = "a".repeat(40);
const PROFILE_B = "b".repeat(40);
const RESULT_A = "c".repeat(64);
const RESULT_B = "d".repeat(64);
const RUNNER_BUILD_ID = "runner-602.1";
const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((path) =>
      rm(path, {
        recursive: true,
        force: true,
      }),
    ),
  );
});

describe("signed runner volume purge", () => {
  it("matches the frozen Node/Rust Ed25519 command and acknowledgement vector", () => {
    const serverPrivateKey = createPrivateKey({
      key: Buffer.concat([
        Buffer.from("302e020100300506032b657004220420", "hex"),
        Buffer.alloc(32, 7),
      ]),
      format: "der",
      type: "pkcs8",
    });
    const volumePrivateKey = createPrivateKey({
      key: Buffer.concat([
        Buffer.from("302e020100300506032b657004220420", "hex"),
        Buffer.alloc(32, 9),
      ]),
      format: "der",
      type: "pkcs8",
    });
    const volumePublicJwk = createPublicKey(volumePrivateKey).export({
      format: "jwk",
    });
    if (!volumePublicJwk.x)
      throw new Error("missing vector Ed25519 public key");
    const volumePublicKeyBytes = Buffer.from(volumePublicJwk.x, "base64url");
    expect(
      createHash("sha256").update(volumePublicKeyBytes).digest("hex"),
    ).toBe("dbc298251c51321b7266e78d1c151c2b62aff8cb95b293096d3463018544face");
    expect(
      createHash("sha256")
        .update("bluey-jobs-runner\0volume-id-v1\0", "utf8")
        .update(volumePublicKeyBytes)
        .digest("base64url"),
    ).toBe("gpZyp68F-efxcxgBNDDQrX4XU_SauTNYymo-vF81hFM");
    const command: UnsignedRunnerVolumePurgeCommand = {
      version: 2,
      requestId: "request-vector",
      audience: RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
      commandId: "command-vector",
      targetVolumeId: "gpZyp68F-efxcxgBNDDQrX4XU_SauTNYymo-vF81hFM",
      targetKeyFingerprint:
        "dbc298251c51321b7266e78d1c151c2b62aff8cb95b293096d3463018544face",
      enrollmentEpoch: 1,
      purgeSubject: "CwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCws",
      purgeGeneration: 3,
      storageEvidenceVersion: 2,
      subjectStorageLayoutVersion: 2,
      legacyInventoryAuthorityGeneration: 0,
      legacyInventoryAuthoritySha256:
        ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256,
      issuedAtMs: 1_700_000_000_123,
      minimumRunnerBuildId: RUNNER_BUILD_ID,
      serverKeyId: "server-key-1",
    };
    const canonicalCommand = canonicalRunnerVolumePurgeCommand(command);
    const commandSha256 = createHash("sha256")
      .update(canonicalCommand)
      .digest("hex");
    expect(commandSha256).toBe(
      "5b48a3aa0e148f56d9743ae44c682ebba6555aa3071b5bdf74c9fdf7054a676a",
    );
    expect(
      sign(null, canonicalCommand, serverPrivateKey).toString("base64url"),
    ).toBe(
      "i_jumYN0vtasqIfC_W8M2UFktSCL2iUWjkMNy51uEEdH4VqZTBiRyECfxlcvMDGvUsZoRchikJP42sadpT38Dw",
    );

    const emptySubject = {
      entryCount: 0,
      fileBytes: "0",
      sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
    } as const;
    const storageEvidence: RunnerPurgeStorageEvidence = {
      version: 2,
      root: {
        deviceId: "unix:602:mount:17",
        before: {
          linkCount: 7,
          entryCount: 4,
          fileBytes: "14",
          sha256:
            "66da0d423ce7d5566199f946a5fde202479f483b9544632e06437a8202221831",
        },
        after: {
          linkCount: 7,
          entryCount: 2,
          fileBytes: "0",
          sha256:
            "4a29d991c29b74c4be392f3b33dc429ac13d3fdf97252550075638b380a1b29e",
        },
      },
      locators: {
        count: 0,
        sha256:
          "4d441f0625608ab97144be2d937707c8bdc3b524b32777171c1322dccc1fc3bf",
      },
      subjectStorage: {
        layoutVersion: 2,
        before: {
          residency: "never_resident",
          subjectTree: emptySubject,
          ownership: emptySubject,
          target: emptySubject,
          completeRoot: emptySubject,
        },
        after: {
          residency: "never_resident",
          subjectTree: emptySubject,
          ownership: emptySubject,
          target: emptySubject,
          completeRoot: emptySubject,
        },
      },
      legacy: {
        inventoryVersion: 1,
        targetBefore: {
          entryCount: 2,
          fileBytes: "14",
          sha256:
            "3bc3c50fcf66c103acf9c7203a1a03391b597814674172cd7fa314b1820c0c2f",
        },
        targetAfter: {
          entryCount: 0,
          fileBytes: "0",
          sha256: EMPTY_RUNNER_INVENTORY_SHA256,
        },
        rootBefore: {
          artifactCount: 2,
          artifactBytes: "14",
          artifactSetSha256:
            "bb1705b669172f45ee0f237247273d291dfb14826ffcc3c6c91203da3d111e94",
          unclassifiedRootCount: 0,
        },
        rootAfter: {
          artifactCount: 0,
          artifactBytes: "0",
          artifactSetSha256: EMPTY_LEGACY_ARTIFACT_SET_SHA256,
          unclassifiedRootCount: 0,
        },
      },
    };
    expect(runnerPurgeStorageEvidenceSha256(storageEvidence)).toBe(
      "7b8ee0312d1fc0431f9e754b66f547639530a4c4445457702b87c76928d07c28",
    );
    const beforeInventory = runnerPurgeTargetInventoryState(
      storageEvidence,
      "before",
    );

    const ack: UnsignedRunnerVolumePurgeAck = {
      version: 2,
      audience: RUNNER_VOLUME_PURGE_ACK_AUDIENCE,
      requestId: command.requestId,
      commandId: command.commandId,
      commandSha256,
      targetVolumeId: command.targetVolumeId,
      targetKeyFingerprint: command.targetKeyFingerprint,
      enrollmentEpoch: command.enrollmentEpoch,
      processInstanceId: "DAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAw",
      purgeSubjectSha256:
        "f0e38b830ebd8a506615ecd154330ec07ff6bf5030447b44e297db1d4b7514ac",
      purgeGeneration: command.purgeGeneration,
      storageEvidence,
      storageEvidenceSha256: runnerPurgeStorageEvidenceSha256(storageEvidence),
      beforeInventoryCount: beforeInventory.count,
      beforeInventorySha256: beforeInventory.sha256,
      afterInventoryCount: 0,
      afterInventorySha256: EMPTY_RUNNER_INVENTORY_SHA256,
      removedCount: 2,
      runnerBuildId: RUNNER_BUILD_ID,
      completedAtMs: 1_700_000_000_999,
    };
    expect(
      sign(null, canonicalRunnerVolumePurgeAck(ack), volumePrivateKey).toString(
        "base64url",
      ),
    ).toBe(
      "xufw1DQq7NgpaRQ_OZ_jq9VXN3ROvUybdHdzX7rVpaHs-cv42csqV8gBx0xMAWYGTd6w41H7b1VTU26mxmuqCA",
    );
  });

  it("purges the complete locator matrix, proves zero, and isolates another subject", async () => {
    const fixture = await freshFixture();
    const subjectA = randomBytes(32).toString("base64url");
    const subjectB = randomBytes(32).toString("base64url");
    await registerProfile(fixture, subjectA, PROFILE_A);
    await registerResult(fixture, subjectA, RESULT_A);
    const subjectBProfile = await registerProfile(fixture, subjectB, PROFILE_B);
    const subjectBResult = await registerResult(fixture, subjectB, RESULT_B);
    const subjectAPaths = await writeCompleteArtifactMatrix(
      fixture.root,
      PROFILE_A,
      RESULT_A,
      "a",
    );
    await subjectBProfile.active.writeFileExclusive(
      "Cookies",
      Buffer.from("managed-profile-b"),
    );
    await subjectBResult.root.writeFileExclusive(
      "step-result.json.enc",
      Buffer.from("managed-result-b"),
    );
    const subjectBHash = accountPurgeSubjectHash(subjectB);
    const subjectBBefore = await fixture.subjectStorage.withLockedSubject(
      subjectBHash,
      (storage) => storage.inventory(),
    );
    const command = signedCommand(fixture, subjectA, 1);

    expect("processInstanceId" in command).toBe(false);
    const ack = await fixture.purger.execute(command);

    for (const path of subjectAPaths)
      await expect(lstat(path)).rejects.toMatchObject({ code: "ENOENT" });
    const subjectBAfter = await fixture.subjectStorage.withLockedSubject(
      subjectBHash,
      (storage) => storage.inventory(),
    );
    expect(subjectBAfter.residency).toBe("resident");
    expect(subjectBAfter.inventory).toEqual(subjectBBefore.inventory);
    await expect(
      subjectBProfile.active.readFileBounded("Cookies", 64),
    ).resolves.toEqual(Buffer.from("managed-profile-b"));
    await expect(
      subjectBResult.root.readFileBounded("step-result.json.enc", 64),
    ).resolves.toEqual(Buffer.from("managed-result-b"));
    expect(ack).toMatchObject({
      version: 2,
      audience: RUNNER_VOLUME_PURGE_ACK_AUDIENCE,
      requestId: command.requestId,
      commandId: command.commandId,
      targetVolumeId: fixture.identity.volumeId,
      targetKeyFingerprint: fixture.identity.publicKeyFingerprint,
      enrollmentEpoch: 7,
      processInstanceId: fixture.processInstanceId,
      purgeSubjectSha256: accountPurgeSubjectHash(subjectA),
      purgeGeneration: 1,
      afterInventoryCount: 0,
      afterInventorySha256: EMPTY_RUNNER_INVENTORY_SHA256,
      removedCount: ack.beforeInventoryCount,
      runnerBuildId: RUNNER_BUILD_ID,
    });
    expect(ack.storageEvidence.subjectStorage.before).toMatchObject({
      residency: "resident",
    });
    expect(
      ack.storageEvidence.subjectStorage.before.target.entryCount,
    ).toBeGreaterThan(0);
    expect(ack.storageEvidence.legacy.targetBefore.entryCount).toBeGreaterThan(
      0,
    );
    expect(ack.storageEvidence.subjectStorage.after).toMatchObject({
      residency: "never_resident",
      target: {
        entryCount: 0,
        fileBytes: "0",
        sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
      },
    });
    expect(ack.storageEvidence.legacy.rootAfter).toEqual({
      artifactCount: 0,
      artifactBytes: "0",
      artifactSetSha256: EMPTY_LEGACY_ARTIFACT_SET_SHA256,
      unclassifiedRootCount: 0,
    });
    expect(ack.storageEvidenceSha256).toBe(
      runnerPurgeStorageEvidenceSha256(ack.storageEvidence),
    );
    expect(ack.beforeInventoryCount).toBeGreaterThan(0);
    const { signature, ...unsignedAck } = ack;
    expect(
      verifyEd25519(
        fixture.identity.publicKeyRaw,
        canonicalRunnerVolumePurgeAck(unsignedAck),
        signature,
      ),
    ).toBe(true);
    const diskBytes = await allRegularFileBytes(fixture.root.path);
    expect(diskBytes.includes(subjectA)).toBe(false);
    expect(diskBytes).toContain("indefinite_managed_restore_safety");
    expect(
      (await allRegularFilePaths(fixture.root.path)).join("\n"),
    ).not.toContain(subjectA);
  });

  it("prepares every authorized legacy target before globally empty ACKs", async () => {
    const fixture = await freshFixture();
    const subjectA = randomBytes(32).toString("base64url");
    const subjectB = randomBytes(32).toString("base64url");
    const subjectAHash = accountPurgeSubjectHash(subjectA);
    await registerProfile(fixture, subjectA, PROFILE_A);
    await registerProfile(fixture, subjectB, PROFILE_B);
    const legacyA = join(fixture.root.path, "active", PROFILE_A, "Cookies");
    const legacyB = join(fixture.root.path, "active", PROFILE_B, "Cookies");
    await writeArtifact(legacyA, "legacy-a");
    await writeArtifact(legacyB, "legacy-b");
    const commandA = signedCommand(fixture, subjectA, 1, {
      commandId: "command-mixed-a",
      requestId: "request-mixed-a",
    });
    const commandB = signedCommand(fixture, subjectB, 1, {
      commandId: "command-mixed-b",
      requestId: "request-mixed-b",
    });

    await expect(fixture.purger.prepare(commandA)).resolves.toBeGreaterThan(
      0,
    );
    await expect(lstat(legacyA)).rejects.toMatchObject({ code: "ENOENT" });
    await expect(readFile(legacyB, "utf8")).resolves.toBe("legacy-b");
    await expect(
      lstat(fixture.index.tombstonePath(subjectAHash)),
    ).rejects.toMatchObject({ code: "ENOENT" });

    await expect(fixture.purger.prepare(commandB)).resolves.toBe(0);
    const restarted = new RunnerVolumePurger(
      fixture.index,
      fixture.purger.options,
    );
    await expect(restarted.prepare(commandA)).resolves.toBe(0);
    await expect(restarted.execute(commandA)).resolves.toMatchObject({
      commandId: "command-mixed-a",
    });
    await expect(restarted.execute(commandB)).resolves.toMatchObject({
      commandId: "command-mixed-b",
    });
    await expect(lstat(legacyB)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("returns byte-identical exact replay and rejects command and generation conflicts", async () => {
    let stopCalls = 0;
    const fixture = await freshFixture({
      stopAccountWork: async () => {
        stopCalls += 1;
        return { status: "stopped" };
      },
    });
    const subject = randomBytes(32).toString("base64url");
    await registerProfile(fixture, subject, PROFILE_A);
    await writeArtifact(
      join(fixture.root.path, "active", PROFILE_A, "Cookies"),
      "a",
    );
    const generationTwo = signedCommand(fixture, subject, 2);
    const first = await fixture.purger.execute(generationTwo);
    const replay = await fixture.purger.execute(generationTwo);
    expect(replay).toEqual(first);
    expect(stopCalls).toBe(1);

    const sameGenerationConflict = signedCommand(fixture, subject, 2, {
      commandId: "command-generation-conflict",
      requestId: "request-generation-conflict",
    });
    await expect(
      fixture.purger.execute(sameGenerationConflict),
    ).rejects.toMatchObject({ code: "command_conflict" });
    await expect(
      fixture.purger.execute(signedCommand(fixture, subject, 1)),
    ).rejects.toMatchObject({ code: "stale_command" });

    const higher = await fixture.purger.execute(
      signedCommand(fixture, subject, 3),
    );
    expect(higher.beforeInventoryCount).toBe(0);
    expect(higher.beforeInventorySha256).toBe(EMPTY_RUNNER_INVENTORY_SHA256);
    expect(higher.removedCount).toBe(0);

    const changedSameCommandId = signedCommand(fixture, subject, 3, {
      commandId: higher.commandId,
      issuedAtMs: 1_799_999_999_999,
    });
    await expect(
      fixture.purger.execute(changedSameCommandId),
    ).rejects.toMatchObject({ code: "command_conflict" });
  });

  it("installs the purge fence before quiescence without deadlocking a queued registration", async () => {
    let releaseJournal = (): void => undefined;
    let signalJournal = (): void => undefined;
    const journalReached = new Promise<void>((resolve) => {
      signalJournal = resolve;
    });
    const journalRelease = new Promise<void>((resolve) => {
      releaseJournal = resolve;
    });
    let registrationSettled: Promise<void> | undefined;
    const fixture = await freshFixture({
      faultInjector: async (phase) => {
        if (phase !== "after_command_journal") return;
        signalJournal();
        await journalRelease;
      },
      stopAccountWork: async () => {
        if (!registrationSettled)
          throw new Error("registration was not queued");
        await registrationSettled;
        return { status: "stopped" };
      },
    });
    const subject = randomBytes(32).toString("base64url");
    const subjectSha256 = accountPurgeSubjectHash(subject);
    const command = signedCommand(fixture, subject, 1);

    const purge = fixture.purger.execute(command);
    await journalReached;
    const registration = fixture.index.registerProfile(subject, PROFILE_A);
    registrationSettled = registration.then(
      () => undefined,
      () => undefined,
    );
    releaseJournal();

    await expect(
      withTimeout(purge, 1_000, "purge deadlocked"),
    ).resolves.toMatchObject({ purgeSubjectSha256: subjectSha256 });
    await expect(registration).rejects.toMatchObject({
      code: "purged_subject",
    });
    await expect(fixture.index.hasPurgeBarrier(subjectSha256)).resolves.toBe(
      true,
    );
  });

  it("re-signs a never-received crash ACK only after exact old-process replay is rejected", async () => {
    const fixture = await freshFixture({ nowMs: () => 1_800_000_000_123 });
    const subject = randomBytes(32).toString("base64url");
    await registerProfile(fixture, subject, PROFILE_A);
    await writeArtifact(
      join(fixture.root.path, "active", PROFILE_A, "Cookies"),
      "crash",
    );
    const command = signedCommand(fixture, subject, 1);
    const original = await fixture.purger.execute(command);
    const replacementProcess = createRunnerProcessInstanceId();
    const restarted = new RunnerVolumePurger(fixture.index, {
      ...fixture.purger.options,
      processInstanceId: replacementProcess,
      nowMs: () => 1_800_000_000_456,
    });

    // The client must submit this exact durable ACK first so a response-loss
    // commit can replay at the server without manufacturing a conflict.
    await expect(restarted.execute(command)).resolves.toEqual(original);

    const recovered = await restarted.recoverAcknowledgementForCurrentProcess(
      command,
      original,
    );
    expect(recovered.processInstanceId).toBe(replacementProcess);
    expect(recovered.beforeInventoryCount).toBe(original.beforeInventoryCount);
    expect(recovered.beforeInventorySha256).toBe(
      original.beforeInventorySha256,
    );
    expect(recovered.removedCount).toBe(original.removedCount);
    expect(recovered.completedAtMs).toBeGreaterThanOrEqual(
      original.completedAtMs,
    );
    await expect(restarted.execute(command)).resolves.toEqual(recovered);

    // The immutable tombstone remains the original deletion evidence; only
    // the signed mutable journal tracks the latest attempted HTTP ACK.
    const tombstone = JSON.parse(
      await readFile(
        fixture.index.tombstonePath(accountPurgeSubjectHash(subject)),
        "utf8",
      ),
    ) as { ack: { processInstanceId: string } };
    expect(tombstone.ack.processInstanceId).toBe(original.processInstanceId);
    await expect(
      restarted.recoverAcknowledgementForCurrentProcess(command, original),
    ).rejects.toMatchObject({ code: "ack_conflict" });
  });

  it("rejects unauthenticated, mistargeted, stale-epoch, and incompatible commands", async () => {
    const fixture = await freshFixture();
    const subject = randomBytes(32).toString("base64url");

    const tampered = { ...signedCommand(fixture, subject, 1), issuedAtMs: 1 };
    await expect(fixture.purger.execute(tampered)).rejects.toMatchObject({
      code: "invalid_command",
    });
    await expect(
      fixture.purger.execute(
        signedCommand(fixture, subject, 1, {
          targetVolumeId: randomBytes(32).toString("base64url"),
        }),
      ),
    ).rejects.toMatchObject({ code: "wrong_volume" });
    await expect(
      fixture.purger.execute(
        signedCommand(fixture, subject, 1, {
          enrollmentEpoch: 8,
        }),
      ),
    ).rejects.toMatchObject({ code: "wrong_epoch" });
    await expect(
      fixture.purger.execute(
        signedCommand(fixture, subject, 1, {
          minimumRunnerBuildId: "runner-999.1",
        }),
      ),
    ).rejects.toMatchObject({ code: "build_incompatible" });
    await expect(
      fixture.purger.execute(
        signedCommand(fixture, subject, 1, {
          minimumRunnerBuildId: "runner-602.01",
        }),
      ),
    ).rejects.toMatchObject({ code: "invalid_command" });
    await expect(
      fixture.purger.execute({
        ...signedCommand(fixture, subject, 1),
        processInstanceId: fixture.processInstanceId,
      }),
    ).rejects.toMatchObject({ code: "invalid_command" });
    await expect(
      fixture.purger.execute({
        ...signedCommand(fixture, subject, 1),
        audience: "wrong-audience",
      }),
    ).rejects.toMatchObject({ code: "invalid_command" });
  });

  it("uses a canonical monotonic build floor across compatible runner upgrades", async () => {
    expect(runnerBuildSatisfies("runner-602.1", "runner-602")).toBe(true);
    expect(runnerBuildSatisfies("runner-602.1", "runner-602.0")).toBe(true);
    expect(runnerBuildSatisfies("runner-602.1", "runner-602.1")).toBe(true);
    expect(runnerBuildSatisfies("runner-603", "runner-602.99")).toBe(true);
    expect(runnerBuildSatisfies("runner-602", "runner-602.1")).toBe(false);
    expect(runnerBuildSatisfies("runner-601.99", "runner-602")).toBe(false);
    expect(runnerBuildSatisfies("runner-602.01", "runner-602")).toBe(false);
    expect(runnerBuildSatisfies("runner-build-test", "runner-602")).toBe(false);

    const fixture = await freshFixture();
    const acknowledgement = await fixture.purger.execute(
      signedCommand(fixture, randomBytes(32).toString("base64url"), 1, {
        minimumRunnerBuildId: "runner-602.0",
      }),
    );
    expect(acknowledgement.runnerBuildId).toBe(RUNNER_BUILD_ID);
  });

  it("resumes every durable crash boundary with the original before inventory", async () => {
    const phases: RunnerVolumePurgeFaultPhase[] = [
      "after_command_journal",
      "after_active_work_stopped",
      "after_fence",
      "after_before_inventory",
      "after_delete_target",
      "after_zero_rescan",
      "after_ack_journal",
      "after_tombstone",
      "after_completed_journal",
    ];
    for (const phase of phases) {
      let armed = true;
      const fixture = await freshFixture({
        nowMs: () => 1_800_000_000_123,
        faultInjector: async (current) => {
          if (armed && current === phase) {
            armed = false;
            throw new Error(`injected-${phase}`);
          }
        },
      });
      const subject = randomBytes(32).toString("base64url");
      await registerProfile(fixture, subject, PROFILE_A);
      const activePath = join(
        fixture.root.path,
        "active",
        PROFILE_A,
        "Cookies",
      );
      const snapshotPath = join(
        fixture.root.path,
        "snapshots",
        `${PROFILE_A}.tar.gz.enc`,
      );
      await writeArtifact(activePath, phase);
      await writeArtifact(snapshotPath, phase);
      const locators = await fixture.index.locatorsForSubjectHash(
        accountPurgeSubjectHash(subject),
      );
      const expectedBefore = createRunnerPurgeStorageBeforeEvidence(
        await fixture.subjectStorage.withLockedSubject(
          accountPurgeSubjectHash(subject),
          (storage) => storage.inventory(),
        ),
        locators,
      );
      const command = signedCommand(fixture, subject, 1);

      await expect(fixture.purger.execute(command)).rejects.toThrow(
        `injected-${phase}`,
      );
      const failedJournal = JSON.parse(
        await readFile(fixture.index.journalPath(command.commandId), "utf8"),
      ) as { readonly storageEvidenceBefore: unknown };
      if (phase === "after_delete_target") {
        await expect(lstat(activePath)).rejects.toMatchObject({
          code: "ENOENT",
        });
        await expect(lstat(snapshotPath)).resolves.toBeDefined();
      }
      const ack = await fixture.purger.execute(command);
      const persistedBefore = runnerPurgeStorageBeforeEvidence(
        ack.storageEvidence,
      );
      expect(persistedBefore.subjectStorage.before, phase).toEqual(
        expectedBefore.subjectStorage.before,
      );
      expect(persistedBefore.legacy, phase).toEqual(expectedBefore.legacy);
      expect(persistedBefore.locators, phase).toEqual(expectedBefore.locators);
      if (failedJournal.storageEvidenceBefore !== null) {
        expect(persistedBefore, phase).toEqual(
          failedJournal.storageEvidenceBefore,
        );
      }
      expect(ack.beforeInventoryCount, phase).toBeGreaterThan(0);
      expect(ack.removedCount, phase).toBe(ack.beforeInventoryCount);
      expect(ack.completedAtMs, phase).toBe(1_800_000_000_123);
    }
  }, 20_000);

  it("fences before quiescence but does not delete or acknowledge irreversible work", async () => {
    const fixture = await freshFixture({
      stopAccountWork: async () => ({ status: "irreversible" }),
    });
    const subject = randomBytes(32).toString("base64url");
    const subjectSha256 = accountPurgeSubjectHash(subject);
    await registerProfile(fixture, subject, PROFILE_A);
    const dataPath = join(fixture.root.path, "active", PROFILE_A, "Cookies");
    await writeArtifact(dataPath, "must-reconcile");

    await expect(
      fixture.purger.execute(signedCommand(fixture, subject, 1)),
    ).rejects.toMatchObject({ code: "irreversible_work" });
    await expect(readFile(dataPath, "utf8")).resolves.toBe("must-reconcile");
    await expect(fixture.index.hasPurgeBarrier(subjectSha256)).resolves.toBe(
      true,
    );
    await expect(
      lstat(fixture.index.tombstonePath(subjectSha256)),
    ).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("emits no acknowledgement for symlinks, hardlinks, or corrupt locators", async () => {
    const symlinkFixture = await freshFixture();
    const symlinkSubject = randomBytes(32).toString("base64url");
    const symlinkHash = accountPurgeSubjectHash(symlinkSubject);
    await registerProfile(symlinkFixture, symlinkSubject, PROFILE_A);
    const outsideDirectory = await temporaryDirectory();
    const outsideFile = join(outsideDirectory, "sentinel");
    await writeFile(outsideFile, "outside", { mode: 0o600 });
    await mkdir(join(symlinkFixture.root.path, "active", PROFILE_A), {
      recursive: true,
      mode: 0o700,
    });
    await symlink(
      outsideFile,
      join(symlinkFixture.root.path, "active", PROFILE_A, "unsafe"),
    );
    await expect(
      symlinkFixture.purger.execute(
        signedCommand(symlinkFixture, symlinkSubject, 1),
      ),
    ).rejects.toMatchObject({ code: "unsafe_entry" });
    await expect(readFile(outsideFile, "utf8")).resolves.toBe("outside");
    await expect(
      lstat(symlinkFixture.index.tombstonePath(symlinkHash)),
    ).rejects.toMatchObject({ code: "ENOENT" });

    if (process.platform !== "win32") {
      const hardlinkFixture = await freshFixture();
      const hardlinkSubject = randomBytes(32).toString("base64url");
      const hardlinkHash = accountPurgeSubjectHash(hardlinkSubject);
      await registerResult(hardlinkFixture, hardlinkSubject, RESULT_A);
      const outsideHardlink = join(await temporaryDirectory(), "sentinel");
      await writeFile(outsideHardlink, "hardlink", { mode: 0o600 });
      const insideHardlink = join(
        hardlinkFixture.root.path,
        "step-results",
        `${RESULT_A}.json.enc`,
      );
      await mkdir(dirname(insideHardlink), { recursive: true, mode: 0o700 });
      await link(outsideHardlink, insideHardlink);
      await expect(
        hardlinkFixture.purger.execute(
          signedCommand(hardlinkFixture, hardlinkSubject, 1),
        ),
      ).rejects.toMatchObject({ code: "unsafe_entry" });
      await expect(readFile(outsideHardlink, "utf8")).resolves.toBe("hardlink");
      await expect(
        lstat(hardlinkFixture.index.tombstonePath(hardlinkHash)),
      ).rejects.toMatchObject({ code: "ENOENT" });
    }

    const corruptFixture = await freshFixture();
    const corruptSubject = randomBytes(32).toString("base64url");
    const corruptHash = accountPurgeSubjectHash(corruptSubject);
    await registerProfile(corruptFixture, corruptSubject, PROFILE_A);
    const locator = join(
      corruptFixture.root.path,
      "account-residency-v1",
      "locators",
      corruptHash,
      `profile-${PROFILE_A}.json`,
    );
    await writeFile(locator, "{}\n", { mode: 0o600 });
    await expect(
      corruptFixture.purger.execute(
        signedCommand(corruptFixture, corruptSubject, 1),
      ),
    ).rejects.toMatchObject({ code: "corrupt_locator" });
    await expect(
      lstat(corruptFixture.index.tombstonePath(corruptHash)),
    ).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("requires a higher generation to repurge data restored beneath a tombstone", async () => {
    const fixture = await freshFixture();
    const subject = randomBytes(32).toString("base64url");
    await registerProfile(fixture, subject, PROFILE_A);
    const dataPath = join(fixture.root.path, "active", PROFILE_A, "Cookies");
    await writeArtifact(dataPath, "original");
    const first = signedCommand(fixture, subject, 1);
    await fixture.purger.execute(first);
    await writeArtifact(dataPath, "restored");

    await expect(fixture.purger.execute(first)).rejects.toMatchObject({
      code: "restored_data",
    });
    const enforcement = await fixture.purger.execute(
      signedCommand(fixture, subject, 2),
    );
    expect(enforcement.beforeInventoryCount).toBeGreaterThan(0);
    await expect(lstat(dataPath)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("fails closed when a frozen locator or signed control record is missing or corrupt", async () => {
    let locatorFaultArmed = true;
    const locatorFixture = await freshFixture({
      faultInjector: async (phase) => {
        if (locatorFaultArmed && phase === "after_before_inventory") {
          locatorFaultArmed = false;
          throw new Error("freeze-locators");
        }
      },
    });
    const locatorSubject = randomBytes(32).toString("base64url");
    const locatorHash = accountPurgeSubjectHash(locatorSubject);
    await registerProfile(locatorFixture, locatorSubject, PROFILE_A);
    await writeArtifact(
      join(locatorFixture.root.path, "active", PROFILE_A, "Cookies"),
      "data",
    );
    const locatorCommand = signedCommand(locatorFixture, locatorSubject, 1);
    await expect(locatorFixture.purger.execute(locatorCommand)).rejects.toThrow(
      "freeze-locators",
    );
    await unlink(
      join(
        locatorFixture.root.path,
        "account-residency-v1",
        "locators",
        locatorHash,
        `profile-${PROFILE_A}.json`,
      ),
    );
    await expect(
      locatorFixture.purger.execute(locatorCommand),
    ).rejects.toMatchObject({ code: "corrupt_state" });

    let journalFaultArmed = true;
    const journalFixture = await freshFixture({
      faultInjector: async (phase) => {
        if (journalFaultArmed && phase === "after_command_journal") {
          journalFaultArmed = false;
          throw new Error("freeze-journal");
        }
      },
    });
    const journalSubject = randomBytes(32).toString("base64url");
    const journalCommand = signedCommand(journalFixture, journalSubject, 1);
    await expect(journalFixture.purger.execute(journalCommand)).rejects.toThrow(
      "freeze-journal",
    );
    await writeFile(
      journalFixture.index.journalPath(journalCommand.commandId),
      "{}\n",
      {
        mode: 0o600,
      },
    );
    await expect(
      journalFixture.purger.execute(journalCommand),
    ).rejects.toMatchObject({ code: "corrupt_state" });

    let fenceFaultArmed = true;
    const fenceFixture = await freshFixture({
      faultInjector: async (phase) => {
        if (fenceFaultArmed && phase === "after_fence") {
          fenceFaultArmed = false;
          throw new Error("freeze-fence");
        }
      },
    });
    const fenceSubject = randomBytes(32).toString("base64url");
    const fenceHash = accountPurgeSubjectHash(fenceSubject);
    const fenceCommand = signedCommand(fenceFixture, fenceSubject, 1);
    await expect(fenceFixture.purger.execute(fenceCommand)).rejects.toThrow(
      "freeze-fence",
    );
    await writeFile(fenceFixture.index.fencePath(fenceHash), "{}\n", {
      mode: 0o600,
    });
    await expect(
      fenceFixture.purger.execute(fenceCommand),
    ).rejects.toMatchObject({ code: "corrupt_state" });

    const tombstoneFixture = await freshFixture();
    const tombstoneSubject = randomBytes(32).toString("base64url");
    const tombstoneHash = accountPurgeSubjectHash(tombstoneSubject);
    const tombstoneCommand = signedCommand(
      tombstoneFixture,
      tombstoneSubject,
      1,
    );
    await tombstoneFixture.purger.execute(tombstoneCommand);
    await writeFile(
      tombstoneFixture.index.tombstonePath(tombstoneHash),
      "{}\n",
      {
        mode: 0o600,
      },
    );
    await expect(
      tombstoneFixture.purger.execute(tombstoneCommand),
    ).rejects.toMatchObject({ code: "corrupt_state" });
  });
});

interface PurgerOverrides {
  readonly stopAccountWork?: () => Promise<
    { readonly status: "stopped" } | { readonly status: "irreversible" }
  >;
  readonly nowMs?: () => number;
  readonly faultInjector?: (
    phase: RunnerVolumePurgeFaultPhase,
  ) => Promise<void>;
}

async function freshFixture(overrides: PurgerOverrides = {}) {
  const parent = await temporaryDirectory();
  const root = await openRunnerDataRoot(join(parent, "runner"));
  const identity = await loadOrCreateRunnerVolumeIdentity(root);
  const index = new AccountResidencyIndex(root, identity);
  const nativeStorage = createInjectedNativeRunnerStorageForTest({
    RunnerStorageDirectory: FilesystemNativeDirectoryForTest,
    RunnerStorageRoot: FilesystemNativeRootForTest,
  }).openRoot(root.path);
  const subjectStorage = new SubjectStorageManager(
    nativeStorage,
    identity,
    index,
  );
  const server = generateKeyPairSync("ed25519");
  const publicJwk = server.publicKey.export({ format: "jwk" });
  if (!publicJwk.x) throw new Error("missing Ed25519 public key");
  const processInstanceId = createRunnerProcessInstanceId();
  const stopAccountWork = overrides.stopAccountWork;
  const purger = new RunnerVolumePurger(index, {
    enrollmentEpoch: 7,
    processInstanceId,
    runnerBuildId: RUNNER_BUILD_ID,
    serverCommandKeys: new Map([["server-key-602", publicJwk.x]]),
    subjectStorage,
    stopAccountWork: stopAccountWork
      ? async () => stopAccountWork()
      : async () => ({ status: "stopped" }),
    nowMs: overrides.nowMs,
    faultInjector: overrides.faultInjector,
  });
  return {
    root,
    identity,
    index,
    subjectStorage,
    purger,
    processInstanceId,
    serverPrivateKey: server.privateKey,
  };
}

async function registerProfile(
  fixture: Awaited<ReturnType<typeof freshFixture>>,
  purgeSubject: string,
  scope: string,
) {
  return fixture.subjectStorage.ensureProfile(
    await fixture.index.registerProfile(purgeSubject, scope),
  );
}

async function registerResult(
  fixture: Awaited<ReturnType<typeof freshFixture>>,
  purgeSubject: string,
  scope: string,
) {
  return fixture.subjectStorage.ensureResult(
    await fixture.index.registerResult(purgeSubject, scope),
  );
}

function signedCommand(
  fixture: {
    identity: { volumeId: string; publicKeyFingerprint: string };
    serverPrivateKey: KeyObject;
  },
  purgeSubject: string,
  purgeGeneration: number,
  overrides: Partial<UnsignedRunnerVolumePurgeCommand> = {},
): RunnerVolumePurgeCommand {
  const unsigned: UnsignedRunnerVolumePurgeCommand = {
    version: 2,
    requestId: `request-${purgeGeneration}`,
    audience: RUNNER_VOLUME_PURGE_COMMAND_AUDIENCE,
    commandId: `command-${purgeGeneration}`,
    targetVolumeId: fixture.identity.volumeId,
    targetKeyFingerprint: fixture.identity.publicKeyFingerprint,
    enrollmentEpoch: 7,
    purgeSubject,
    purgeGeneration,
    storageEvidenceVersion: 2,
    subjectStorageLayoutVersion: 2,
    legacyInventoryAuthorityGeneration: 0,
    legacyInventoryAuthoritySha256:
      ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256,
    issuedAtMs: 1_800_000_000_000,
    minimumRunnerBuildId: RUNNER_BUILD_ID,
    serverKeyId: "server-key-602",
    ...overrides,
  };
  return {
    ...unsigned,
    signature: sign(
      null,
      canonicalRunnerVolumePurgeCommand(unsigned),
      fixture.serverPrivateKey,
    ).toString("base64url"),
  };
}

interface FilesystemNativeInventoryEntryForTest {
  readonly relativePath: string;
  readonly kind: "directory" | "file";
  readonly deviceId: string;
  readonly linkCount: string;
  readonly sizeBytes: string;
  readonly sha256: string;
}

interface FilesystemNativeInventoryForTest {
  readonly entries: readonly FilesystemNativeInventoryEntryForTest[];
  readonly count: number;
  readonly bytes: string;
  readonly sha256: string;
}

/**
 * Filesystem-backed raw module injected through the production native facade.
 * It keeps this unit suite independent of a staged `.node` artifact while the
 * native crate's own tests cover retained-handle and process-lock semantics.
 */
class FilesystemNativeRootForTest {
  readonly #device: number;
  readonly #inode: number;
  readonly #deviceId: string;

  constructor(readonly configuredPath: string) {
    const root = testNativeDirectoryStatSync(configuredPath);
    this.#device = root.dev;
    this.#inode = root.ino;
    this.#deviceId = `unix:${root.dev.toString(16)}:mount:test`;
    const lockPath = join(configuredPath, ".bluey-runner-storage.lock");
    try {
      const descriptor = openSync(lockPath, "ax", 0o600);
      closeSync(descriptor);
    } catch (error) {
      if (!hasFsCode(error, "EEXIST")) throw testNativeFailure("io_failure");
    }
    const lock = lstatSync(lockPath);
    assertTestNativeFile(lock, this.#device);
  }

  get deviceId(): string {
    return this.#deviceId;
  }

  get linkCount(): string {
    return String(this.assertRootBinding().nlink);
  }

  assertUnchanged(): void {
    this.assertRootBinding();
  }

  async ensureDirectory(
    components: readonly string[],
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.openDirectoryInternal(components, true);
  }

  async openDirectory(
    components: readonly string[],
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.openDirectoryInternal(components, false);
  }

  async moveEntryNoReplace(
    sourceComponents: readonly string[],
    destinationComponents: readonly string[],
  ): Promise<"destination_exists" | "moved" | "source_missing"> {
    this.assertRootBinding();
    const source = join(this.configuredPath, ...sourceComponents);
    const destination = join(this.configuredPath, ...destinationComponents);
    const sourceStat = await testLstatOrUndefined(source);
    if (!sourceStat) return "source_missing";
    if (sourceStat.isDirectory()) {
      assertTestNativeDirectory(sourceStat, this.#device);
      await collectTestNativeInventory(source, this.#device, this.#deviceId);
    } else {
      assertTestNativeFile(sourceStat, this.#device);
    }
    const destinationStat = await testLstatOrUndefined(destination);
    if (destinationStat) {
      if (destinationStat.isDirectory()) {
        assertTestNativeDirectory(destinationStat, this.#device);
        await collectTestNativeInventory(
          destination,
          this.#device,
          this.#deviceId,
        );
      } else {
        assertTestNativeFile(destinationStat, this.#device);
      }
      return "destination_exists";
    }
    try {
      await rename(source, destination);
    } catch {
      throw testNativeFailure("io_failure");
    }
    this.assertRootBinding();
    return "moved";
  }

  async openDirectoryInternal(
    components: readonly string[],
    create: boolean,
  ): Promise<FilesystemNativeDirectoryForTest> {
    this.assertRootBinding();
    let current = this.configuredPath;
    try {
      for (const component of components) {
        current = join(current, component);
        let metadata = await testLstatOrUndefined(current);
        if (!metadata && create) {
          await mkdir(current, { mode: 0o700 });
          metadata = await lstat(current);
        }
        if (!metadata) throw testNativeFailure("io_failure");
        assertTestNativeDirectory(metadata, this.#device);
      }
    } catch (error) {
      throw translateTestNativeError(error);
    }
    const metadata = testNativeDirectoryStatSync(current, this.#device);
    this.assertRootBinding();
    return new FilesystemNativeDirectoryForTest(this, components, metadata.ino);
  }

  assertDirectoryBinding(components: readonly string[], inode: number): Stats {
    this.assertRootBinding();
    const metadata = testNativeDirectoryStatSync(
      join(this.configuredPath, ...components),
      this.#device,
    );
    if (metadata.ino !== inode) throw testNativeFailure("unsafe_entry");
    return metadata;
  }

  get rootDevice(): number {
    return this.#device;
  }

  private assertRootBinding(): Stats {
    let metadata: Stats;
    try {
      metadata = testNativeDirectoryStatSync(this.configuredPath);
      if (
        metadata.dev !== this.#device ||
        metadata.ino !== this.#inode ||
        realpathSync(this.configuredPath) !== this.configuredPath
      ) {
        throw testNativeFailure("root_changed");
      }
    } catch (error) {
      if (isTestNativeFailure(error)) throw error;
      throw testNativeFailure("root_changed");
    }
    return metadata;
  }
}

class FilesystemNativeDirectoryForTest {
  readonly relativePath: string;
  readonly canonicalPath: string;

  constructor(
    private readonly root: FilesystemNativeRootForTest,
    private readonly components: readonly string[],
    private readonly inode: number,
  ) {
    this.relativePath = components.join("/");
    this.canonicalPath = join(root.configuredPath, ...components);
  }

  get deviceId(): string {
    return this.root.deviceId;
  }

  get linkCount(): string {
    return String(this.assertBinding().nlink);
  }

  async ensureChildDirectory(
    name: string,
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.root.ensureDirectory([...this.components, name]);
  }

  async openChildDirectory(
    name: string,
  ): Promise<FilesystemNativeDirectoryForTest> {
    return this.root.openDirectory([...this.components, name]);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const existing = await testLstatOrUndefined(path);
    if (existing) {
      assertTestNativeFile(existing, this.root.rootDevice);
      return false;
    }
    try {
      await writeFile(path, contents, { flag: "wx", mode: 0o600 });
    } catch (error) {
      if (hasFsCode(error, "EEXIST")) {
        const raced = await lstat(path);
        assertTestNativeFile(raced, this.root.rootDevice);
        return false;
      }
      throw testNativeFailure("io_failure");
    }
    assertTestNativeFile(await lstat(path), this.root.rootDevice);
    this.assertBinding();
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const existing = await testLstatOrUndefined(path);
    if (existing) assertTestNativeFile(existing, this.root.rootDevice);
    try {
      await writeFile(path, contents, { mode: 0o600 });
    } catch {
      throw testNativeFailure("io_failure");
    }
    assertTestNativeFile(await lstat(path), this.root.rootDevice);
    this.assertBinding();
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const before = await testLstatOrUndefined(path);
    if (!before) throw testNativeFailure("io_failure");
    assertTestNativeFile(before, this.root.rootDevice);
    if (before.size > maximumBytes) throw testNativeFailure("inventory_limit");
    let contents: Buffer;
    try {
      contents = await readFile(path);
    } catch {
      throw testNativeFailure("io_failure");
    }
    const after = await lstat(path);
    assertTestNativeFile(after, this.root.rootDevice);
    if (
      before.ino !== after.ino ||
      before.size !== after.size ||
      contents.byteLength !== after.size
    ) {
      throw testNativeFailure("unsafe_entry");
    }
    this.assertBinding();
    return contents;
  }

  async inventory(): Promise<FilesystemNativeInventoryForTest> {
    this.assertBinding();
    const entries = await collectTestNativeInventory(
      this.canonicalPath,
      this.root.rootDevice,
      this.root.deviceId,
    );
    this.assertBinding();
    const bytes = entries.reduce(
      (total, entry) => total + Number(entry.sizeBytes),
      0,
    );
    const digest = createHash("sha256");
    digest.update("bluey-jobs-runner-native-inventory-v1\0", "utf8");
    for (const entry of entries) {
      digest.update(
        `${Buffer.byteLength(entry.relativePath, "utf8")}:`,
        "utf8",
      );
      digest.update(entry.relativePath, "utf8");
      digest.update("\n", "utf8");
      digest.update(`${entry.kind}\n`, "utf8");
      digest.update(`${entry.sizeBytes}\n`, "utf8");
      digest.update(`${entry.sha256}\n`, "utf8");
    }
    return {
      entries,
      count: entries.length,
      bytes: String(bytes),
      sha256: digest.digest("hex"),
    };
  }

  async removeEntry(name: string): Promise<void> {
    this.assertBinding();
    const path = join(this.canonicalPath, name);
    const metadata = await testLstatOrUndefined(path);
    if (!metadata) return;
    if (metadata.isDirectory()) {
      assertTestNativeDirectory(metadata, this.root.rootDevice);
      await collectTestNativeInventory(
        path,
        this.root.rootDevice,
        this.root.deviceId,
      );
    } else {
      assertTestNativeFile(metadata, this.root.rootDevice);
    }
    try {
      await rm(path, { recursive: true });
    } catch {
      throw testNativeFailure("io_failure");
    }
    this.assertBinding();
  }

  private assertBinding(): Stats {
    return this.root.assertDirectoryBinding(this.components, this.inode);
  }
}

async function collectTestNativeInventory(
  directory: string,
  rootDevice: number,
  deviceId: string,
): Promise<FilesystemNativeInventoryEntryForTest[]> {
  const entries: FilesystemNativeInventoryEntryForTest[] = [];
  const collect = async (path: string, prefix: string): Promise<void> => {
    let names: string[];
    try {
      names = await readdir(path);
    } catch {
      throw testNativeFailure("io_failure");
    }
    names.sort(compareTestNativeUtf8);
    for (const name of names) {
      assertTestNativeInventoryName(name);
      const relativePath = prefix ? `${prefix}/${name}` : name;
      const child = join(path, name);
      const before = await lstat(child);
      if (before.isDirectory()) {
        assertTestNativeDirectory(before, rootDevice);
        entries.push({
          relativePath,
          kind: "directory",
          deviceId,
          linkCount: String(before.nlink),
          sizeBytes: "0",
          sha256: createHash("sha256")
            .update("bluey-jobs-runner-native-inventory-directory-v1", "utf8")
            .digest("hex"),
        });
        await collect(child, relativePath);
      } else if (before.isFile()) {
        assertTestNativeFile(before, rootDevice);
        const contents = await readFile(child);
        const after = await lstat(child);
        assertTestNativeFile(after, rootDevice);
        if (
          before.ino !== after.ino ||
          before.size !== after.size ||
          contents.byteLength !== after.size
        ) {
          throw testNativeFailure("unsafe_entry");
        }
        entries.push({
          relativePath,
          kind: "file",
          deviceId,
          linkCount: String(before.nlink),
          sizeBytes: String(before.size),
          sha256: createHash("sha256").update(contents).digest("hex"),
        });
      } else {
        throw testNativeFailure("unsafe_entry");
      }
    }
  };
  await collect(directory, "");
  entries.sort((left, right) =>
    compareTestNativeUtf8(left.relativePath, right.relativePath),
  );
  return entries;
}

function assertTestNativeInventoryName(name: string): void {
  if (
    name.length === 0 ||
    name === "." ||
    name === ".." ||
    Buffer.byteLength(name, "utf8") > 255 ||
    /[\\\u0000-\u001f\u007f]/.test(name)
  ) {
    throw testNativeFailure("unsafe_entry");
  }
}

function assertTestNativeDirectory(metadata: Stats, rootDevice: number): void {
  if (!metadata.isDirectory() || metadata.dev !== rootDevice) {
    throw testNativeFailure("unsafe_entry");
  }
}

function assertTestNativeFile(metadata: Stats, rootDevice: number): void {
  if (
    !metadata.isFile() ||
    metadata.dev !== rootDevice ||
    metadata.nlink !== 1
  ) {
    throw testNativeFailure("unsafe_entry");
  }
}

function testNativeDirectoryStatSync(path: string, rootDevice?: number): Stats {
  const metadata = lstatSync(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw testNativeFailure("unsafe_entry");
  }
  if (rootDevice !== undefined && metadata.dev !== rootDevice) {
    throw testNativeFailure("unsafe_entry");
  }
  return metadata;
}

async function testLstatOrUndefined(path: string): Promise<Stats | undefined> {
  try {
    return await lstat(path);
  } catch (error) {
    if (hasFsCode(error, "ENOENT")) return undefined;
    throw testNativeFailure("io_failure");
  }
}

function compareTestNativeUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function hasFsCode(error: unknown, code: string): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    error.code === code
  );
}

function isTestNativeFailure(error: unknown): boolean {
  return (
    error instanceof Error &&
    /^bluey_runner_storage:[a-z_]+$/.test(error.message)
  );
}

function translateTestNativeError(error: unknown): Error {
  return isTestNativeFailure(error)
    ? (error as Error)
    : testNativeFailure("io_failure");
}

function testNativeFailure(code: string): Error {
  return new Error(`bluey_runner_storage:${code}`);
}

async function writeCompleteArtifactMatrix(
  root: RunnerDataRoot,
  profileScope: string,
  resultScope: string,
  contents: string,
): Promise<readonly string[]> {
  const paths = [
    join(root.path, "active", profileScope, "Default", "Cookies"),
    join(root.path, "active", `${profileScope}.restore.tar.gz`),
    join(
      root.path,
      "active",
      `${profileScope}.restore.tar.gz.42.0123456789abcdef.open`,
    ),
    join(root.path, "active", `${profileScope}.seal.tar.gz`),
    join(root.path, "snapshots", `${profileScope}.tar.gz.enc`),
    join(root.path, "snapshots", `${profileScope}.generation`),
    join(
      root.path,
      "snapshots",
      `${profileScope}.tar.gz.enc.42.0123456789abcdef.seal`,
    ),
    join(
      root.path,
      "snapshots",
      `${profileScope}.tar.gz.enc.42-0123456789abcdef.incoming`,
    ),
    join(
      root.path,
      "snapshots",
      `${profileScope}.generation.42-0123456789abcdef.incoming`,
    ),
    join(
      root.path,
      "run-checkpoints",
      profileScope,
      `${"e".repeat(64)}.json.enc`,
    ),
    join(
      root.path,
      "run-checkpoints",
      profileScope,
      `${"e".repeat(64)}.json.enc.42.tmp`,
    ),
    join(root.path, "receipts", profileScope, "run-1", "receipt.json"),
    join(root.path, "receipts", profileScope, "run-1", "final.png"),
    join(root.path, "step-results", `${resultScope}.json.enc`),
    join(root.path, "step-results", `${resultScope}.json.enc.42.write.json`),
    join(root.path, "step-results", `${resultScope}.json.enc.42.tmp`),
    join(root.path, "step-results", `${resultScope}.json.enc.42.tmp.42.seal`),
    join(root.path, "step-results", `${resultScope}.json.enc.42.read.json`),
    join(
      root.path,
      "step-results",
      `${resultScope}.json.enc.42.read.json.42.open`,
    ),
  ];
  for (const path of paths) await writeArtifact(path, contents);
  return paths;
}

async function writeArtifact(path: string, contents: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  if (process.platform !== "win32") await chmod(dirname(path), 0o700);
  await writeFile(path, contents, { mode: 0o600 });
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-volume-purge-"));
  temporaryDirectories.push(path);
  return path;
}

async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  message: string,
): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_resolve, reject) => {
        timeout = setTimeout(() => reject(new Error(message)), timeoutMs);
      }),
    ]);
  } finally {
    if (timeout) clearTimeout(timeout);
  }
}

async function allRegularFilePaths(root: string): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) found.push(...(await allRegularFilePaths(path)));
    else if (entry.isFile()) found.push(path);
  }
  return found.sort((left, right) => left.localeCompare(right));
}

async function allRegularFileBytes(root: string): Promise<string> {
  const buffers = await Promise.all(
    (await allRegularFilePaths(root)).map((path) => readFile(path)),
  );
  return Buffer.concat(buffers).toString("utf8");
}
