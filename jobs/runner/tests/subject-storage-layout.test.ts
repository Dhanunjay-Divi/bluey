import { createHash, createPrivateKey, createPublicKey, type KeyObject } from "node:crypto";
import { describe, expect, it } from "vitest";
import {
  ACCOUNT_DATA_V2_DIRECTORY,
  auditSubjectStorageLayout,
  canonicalScopeOwnershipBytes,
  canonicalSubjectMetadataBytes,
  createSignedScopeOwnership,
  createSignedSubjectMetadata,
  encodeScopeOwnership,
  encodeSubjectMetadata,
  parseScopeOwnership,
  parseSubjectMetadata,
  scopeOwnershipPath,
  subjectStoragePaths,
  type NativeScannerEntryKind,
  type NativeScannerRecord,
  type NativeRunnerRootEntryEvidence,
  type SignedScopeOwnership,
  type SubjectStorageAuditInput,
} from "../src/subject-storage-layout.js";
import {
  ed25519PublicKeyFingerprint,
  type RunnerVolumeIdentity,
  verifyEd25519,
} from "../src/volume-identity.js";

const SUBJECT_A = "1".repeat(64);
const SUBJECT_B = "2".repeat(64);
const NEVER_RESIDENT_SUBJECT = "3".repeat(64);
const PROFILE_SCOPE = "a".repeat(40);
const RESULT_SCOPE = "b".repeat(64);
const DEVICE_ID = "device-602";
const identity = fixedIdentity();

describe("subject-contained runner storage layout v2", () => {
  it("derives exact component paths and canonical deterministic signed records", () => {
    const paths = subjectStoragePaths(SUBJECT_A);
    const profile = paths.profile(PROFILE_SCOPE);
    const result = paths.result(RESULT_SCOPE);

    expect(paths.root.components).toEqual([
      "account-data-v2",
      "subjects",
      SUBJECT_A,
    ]);
    expect(profile.active.relativePath).toBe(
      `account-data-v2/subjects/${SUBJECT_A}/profiles/${PROFILE_SCOPE}/active`,
    );
    expect(profile.encryptedSnapshot.relativePath).toBe(
      `account-data-v2/subjects/${SUBJECT_A}/profiles/${PROFILE_SCOPE}/snapshots/profile.tar.gz.enc`,
    );
    expect(profile.snapshotGeneration.relativePath).toBe(
      `account-data-v2/subjects/${SUBJECT_A}/profiles/${PROFILE_SCOPE}/snapshots/generation`,
    );
    expect(result.encryptedResult.relativePath).toBe(
      `account-data-v2/subjects/${SUBJECT_A}/results/${RESULT_SCOPE}/step-result.json.enc`,
    );
    expect(scopeOwnershipPath("profile", PROFILE_SCOPE).relativePath).toBe(
      `account-data-v2/scope-owners/profiles/${PROFILE_SCOPE}.json`,
    );

    const firstMetadata = createSignedSubjectMetadata(identity, SUBJECT_A);
    const secondMetadata = createSignedSubjectMetadata(identity, SUBJECT_A);
    const firstOwner = createSignedScopeOwnership(identity, SUBJECT_A, "profile", PROFILE_SCOPE);
    const secondOwner = createSignedScopeOwnership(identity, SUBJECT_A, "profile", PROFILE_SCOPE);

    expect(secondMetadata).toEqual(firstMetadata);
    expect(secondOwner).toEqual(firstOwner);
    const { signature: metadataSignature, ...unsignedMetadata } = firstMetadata;
    const { signature: ownerSignature, ...unsignedOwner } = firstOwner;
    expect(verifyEd25519(
      identity.publicKeyRaw,
      canonicalSubjectMetadataBytes(unsignedMetadata),
      metadataSignature,
    )).toBe(true);
    expect(verifyEd25519(
      identity.publicKeyRaw,
      canonicalScopeOwnershipBytes(unsignedOwner),
      ownerSignature,
    )).toBe(true);
    expect(metadataSignature).toBe(
      "ay33HjsKSKftYZu6dXRcslbs9upLYtdXb0q8UATQv_bNA5fK6HqhWEAInb8kNZO8Xs66My7ZcjWQ9I6oBJG8CA",
    );
    expect(ownerSignature).toBe(
      "PWb5DpvjflFvUfr_GeID_Wq__UEiFjzLdknIT8SD-zmjJuB5hDrfDf7cdEA52pnXiNWrfD-Knrw4ABUewegfBA",
    );
    expect(parseSubjectMetadata(encodeSubjectMetadata(firstMetadata), identity))
      .toEqual(firstMetadata);
    expect(parseScopeOwnership(encodeScopeOwnership(firstOwner), identity)).toEqual(firstOwner);

    const reordered = Buffer.from(
      `${JSON.stringify({ signature: firstMetadata.signature, ...unsignedMetadata })}\n`,
      "utf8",
    );
    expect(() => parseSubjectMetadata(reordered, identity))
      .toThrow(expect.objectContaining({ code: "corrupt_subject_metadata" }));
    const reorderedOwner = Buffer.from(
      `${JSON.stringify({ signature: firstOwner.signature, ...unsignedOwner })}\n`,
      "utf8",
    );
    expect(() => parseScopeOwnership(reorderedOwner, identity))
      .toThrow(expect.objectContaining({ code: "corrupt_scope_ownership" }));
  });

  it("treats complete absence as clean only for a never-resident subject", () => {
    const result = auditSubjectStorageLayout({
      targetSubjectSha256: NEVER_RESIDENT_SUBJECT,
      identity,
      runnerRootDeviceId: DEVICE_ID,
      runnerRootEntries: rootEntries(false),
      accountDataV2: { state: "absent" },
    });

    expect(result).toEqual({
      ok: true,
      version: 2,
      target: {
        subjectSha256: NEVER_RESIDENT_SUBJECT,
        residency: "never_resident",
      },
      subjects: [],
      inventory: {
        entryCount: 0,
        fileBytes: "0",
        sha256: createHash("sha256")
          .update("bluey-jobs-runner-subject-storage-inventory-v2\n")
          .digest("hex"),
      },
    });
  });

  it("accepts a closed signed subject layout and keeps an absent target never-resident", () => {
    const entries = completeSubjectEntries(SUBJECT_A, {
      profiles: [PROFILE_SCOPE],
      results: [RESULT_SCOPE],
    });
    entries.push(...ownerEntries([
      createSignedScopeOwnership(identity, SUBJECT_A, "profile", PROFILE_SCOPE),
      createSignedScopeOwnership(identity, SUBJECT_A, "result", RESULT_SCOPE),
    ]));
    entries.push(file(
      ["subjects", SUBJECT_A, "profiles", PROFILE_SCOPE, "active", "Default", "Cookies"],
      Buffer.from("encrypted-browser-db"),
    ));
    entries.push(directory(
      ["subjects", SUBJECT_A, "profiles", PROFILE_SCOPE, "active", "Default"],
    ));
    entries.push(file(
      ["subjects", SUBJECT_A, "results", RESULT_SCOPE, "step-result.json.enc"],
      Buffer.from("BLUEYENC-result"),
    ));

    const resident = audit(entries, SUBJECT_A);
    expect(resident.ok).toBe(true);
    if (resident.ok) {
      expect(resident.target.residency).toBe("resident");
      expect(resident.subjects).toMatchObject([{
        subjectSha256: SUBJECT_A,
        profileScopes: [PROFILE_SCOPE],
        resultScopes: [RESULT_SCOPE],
        scopeOwnershipPaths: [
          `account-data-v2/scope-owners/profiles/${PROFILE_SCOPE}.json`,
          `account-data-v2/scope-owners/results/${RESULT_SCOPE}.json`,
        ],
      }]);
    }

    const absent = audit(entries, NEVER_RESIDENT_SUBJECT);
    expect(absent.ok).toBe(true);
    if (absent.ok) expect(absent.target.residency).toBe("never_resident");
  });

  it("fails closed for missing and corrupt signed subject metadata", () => {
    const missingEntries = completeSubjectEntries(SUBJECT_A, { profiles: [], results: [] })
      .filter((entry) => entry.components.at(-1) !== "subject.json");
    expect(failureCodes(audit(missingEntries))).toContain("missing_subject_metadata");

    const corruptEntries = completeSubjectEntries(SUBJECT_A, { profiles: [], results: [] });
    const metadataIndex = corruptEntries.findIndex((entry) => entry.components.at(-1) === "subject.json");
    const metadata = createSignedSubjectMetadata(identity, SUBJECT_A);
    const tampered = Buffer.from(`${JSON.stringify({ ...metadata, subjectRootPath: "wrong" })}\n`);
    corruptEntries[metadataIndex] = file(["subjects", SUBJECT_A, "subject.json"], tampered, true);
    expect(failureCodes(audit(corruptEntries))).toContain("corrupt_subject_metadata");
  });

  it("rejects one globally indexed scope appearing beneath two subjects", () => {
    const entries = [
      ...layoutScaffold(),
      ...subjectEntries(SUBJECT_A, { profiles: [PROFILE_SCOPE], results: [] }),
      ...subjectEntries(SUBJECT_B, { profiles: [PROFILE_SCOPE], results: [] }),
      ...ownerEntries([
        createSignedScopeOwnership(identity, SUBJECT_A, "profile", PROFILE_SCOPE),
      ]),
    ];

    const result = audit(entries, SUBJECT_A);
    expect(failureCodes(result)).toContain("duplicate_scope_ownership");
    expect(failureCodes(result)).toContain("scope_ownership_conflict");
  });

  it("rejects artifacts whose scope has no immutable global owner", () => {
    const entries = completeSubjectEntries(SUBJECT_A, {
      profiles: [PROFILE_SCOPE],
      results: [],
    });
    entries.push(file(
      ["subjects", SUBJECT_A, "profiles", PROFILE_SCOPE, "active", "Cookies"],
      Buffer.from("account data"),
    ));

    expect(failureCodes(audit(entries))).toContain("unindexed_artifact");
  });

  it("rejects any legacy artifact root even when the v2 namespace is absent", () => {
    const runnerRootEntries = rootEntries(false);
    runnerRootEntries.push({
      name: "snapshots",
      kind: "directory",
      deviceId: DEVICE_ID,
      linkCount: 2,
    });
    const result = auditSubjectStorageLayout({
      targetSubjectSha256: SUBJECT_A,
      identity,
      runnerRootDeviceId: DEVICE_ID,
      runnerRootEntries,
      accountDataV2: { state: "absent" },
    });

    expect(failureCodes(result)).toContain("legacy_entry");
  });

  it("accepts the exact native and runner control namespace shapes", () => {
    const runnerRootEntries = rootEntries(false);
    runnerRootEntries.push(
      {
        name: "account-residency-v1",
        kind: "directory",
        deviceId: DEVICE_ID,
        linkCount: 2,
      },
      {
        name: "runner-volume-control-v1",
        kind: "directory",
        deviceId: DEVICE_ID,
        linkCount: 2,
      },
      {
        name: ".bluey-runner-storage.lock",
        kind: "file",
        deviceId: DEVICE_ID,
        linkCount: 1,
      },
    );
    const result = auditSubjectStorageLayout({
      targetSubjectSha256: NEVER_RESIDENT_SUBJECT,
      identity,
      runnerRootDeviceId: DEVICE_ID,
      runnerRootEntries,
      accountDataV2: { state: "absent" },
    });

    expect(result.ok).toBe(true);
    runnerRootEntries[runnerRootEntries.length - 1] = {
      name: ".bluey-runner-storage.lock",
      kind: "directory",
      deviceId: DEVICE_ID,
      linkCount: 2,
    };
    expect(failureCodes(auditSubjectStorageLayout({
      targetSubjectSha256: NEVER_RESIDENT_SUBJECT,
      identity,
      runnerRootDeviceId: DEVICE_ID,
      runnerRootEntries,
      accountDataV2: { state: "absent" },
    }))).toContain("unsafe_special_entry");
  });

  it("rejects unknown, symlink, special, hardlinked, and cross-device scanner evidence", () => {
    const base = validProfileLayout();
    const cases: Array<{
      name: string;
      entry: NativeScannerRecord;
      code: string;
    }> = [
      {
        name: "unknown",
        entry: file(["mystery"], Buffer.from("x")),
        code: "unknown_entry",
      },
      {
        name: "symlink",
        entry: unsafeEntry(
          ["subjects", SUBJECT_A, "profiles", PROFILE_SCOPE, "active", "link"],
          "symlink",
        ),
        code: "unsafe_symlink",
      },
      {
        name: "special",
        entry: unsafeEntry(
          ["subjects", SUBJECT_A, "profiles", PROFILE_SCOPE, "active", "socket"],
          "special",
        ),
        code: "unsafe_special_entry",
      },
      {
        name: "hardlink",
        entry: file(
          ["subjects", SUBJECT_A, "profiles", PROFILE_SCOPE, "active", "Cookies"],
          Buffer.from("x"),
          false,
          { linkCount: 2 },
        ),
        code: "hardlinked_entry",
      },
      {
        name: "cross-device",
        entry: file(
          ["subjects", SUBJECT_A, "profiles", PROFILE_SCOPE, "active", "Cookies"],
          Buffer.from("x"),
          false,
          { deviceId: "other-device" },
        ),
        code: "cross_device_entry",
      },
    ];

    for (const testCase of cases) {
      const result = audit([...base, testCase.entry]);
      expect(failureCodes(result), testCase.name).toContain(testCase.code);
    }
  });
});

function validProfileLayout(): NativeScannerRecord[] {
  return [
    ...completeSubjectEntries(SUBJECT_A, { profiles: [PROFILE_SCOPE], results: [] }),
    ...ownerEntries([
      createSignedScopeOwnership(identity, SUBJECT_A, "profile", PROFILE_SCOPE),
    ]),
  ];
}

function completeSubjectEntries(
  subject: string,
  scopes: { profiles: readonly string[]; results: readonly string[] },
): NativeScannerRecord[] {
  return [...layoutScaffold(), ...subjectEntries(subject, scopes)];
}

function layoutScaffold(): NativeScannerRecord[] {
  return [
    directory(["subjects"]),
    directory(["scope-owners"]),
    directory(["scope-owners", "profiles"]),
    directory(["scope-owners", "results"]),
  ];
}

function subjectEntries(
  subject: string,
  scopes: { profiles: readonly string[]; results: readonly string[] },
): NativeScannerRecord[] {
  const metadata = createSignedSubjectMetadata(identity, subject);
  const entries: NativeScannerRecord[] = [
    directory(["subjects", subject]),
    file(["subjects", subject, "subject.json"], encodeSubjectMetadata(metadata), true),
    directory(["subjects", subject, "profiles"]),
    directory(["subjects", subject, "results"]),
  ];
  for (const scope of scopes.profiles) {
    const root = ["subjects", subject, "profiles", scope];
    entries.push(
      directory(root),
      directory([...root, "active"]),
      directory([...root, "snapshots"]),
      directory([...root, "run-checkpoints"]),
      directory([...root, "receipts"]),
      directory([...root, "temporary"]),
    );
  }
  for (const scope of scopes.results) {
    const root = ["subjects", subject, "results", scope];
    entries.push(directory(root), directory([...root, "temporary"]));
  }
  return entries;
}

function ownerEntries(owners: readonly SignedScopeOwnership[]): NativeScannerRecord[] {
  return owners.map((owner) => file(
    owner.ownershipPath
      .slice(`${ACCOUNT_DATA_V2_DIRECTORY}/`.length)
      .split("/"),
    encodeScopeOwnership(owner),
    true,
  ));
}

function directory(components: readonly string[]): NativeScannerRecord {
  return {
    components,
    kind: "directory",
    deviceId: DEVICE_ID,
    linkCount: 2,
    sizeBytes: "0",
    sha256: null,
    controlBytes: null,
  };
}

function file(
  components: readonly string[],
  contents: Uint8Array,
  controlBytes = false,
  overrides: Partial<Pick<NativeScannerRecord, "deviceId" | "linkCount">> = {},
): NativeScannerRecord {
  const bytes = Buffer.from(contents);
  return {
    components,
    kind: "file",
    deviceId: overrides.deviceId ?? DEVICE_ID,
    linkCount: overrides.linkCount ?? 1,
    sizeBytes: String(bytes.length),
    sha256: createHash("sha256").update(bytes).digest("hex"),
    controlBytes: controlBytes ? bytes : null,
  };
}

function unsafeEntry(
  components: readonly string[],
  kind: Exclude<NativeScannerEntryKind, "directory" | "file">,
): NativeScannerRecord {
  return {
    components,
    kind,
    deviceId: DEVICE_ID,
    linkCount: 1,
    sizeBytes: "0",
    sha256: null,
    controlBytes: null,
  };
}

function rootEntries(v2Present: boolean): NativeRunnerRootEntryEvidence[] {
  return [
    {
      name: "volume-identity",
      kind: "directory",
      deviceId: DEVICE_ID,
      linkCount: 2,
    },
    ...(v2Present ? [{
      name: ACCOUNT_DATA_V2_DIRECTORY,
      kind: "directory" as const,
      deviceId: DEVICE_ID,
      linkCount: 2,
    }] : []),
  ];
}

function audit(
  entries: readonly NativeScannerRecord[],
  targetSubjectSha256 = SUBJECT_A,
) {
  const input: SubjectStorageAuditInput = {
    targetSubjectSha256,
    identity,
    runnerRootDeviceId: DEVICE_ID,
    runnerRootEntries: rootEntries(true),
    accountDataV2: {
      state: "present",
      kind: "directory",
      deviceId: DEVICE_ID,
      linkCount: 2,
      entries,
    },
  };
  return auditSubjectStorageLayout(input);
}

function failureCodes(result: ReturnType<typeof auditSubjectStorageLayout>): string[] {
  return result.ok ? [] : result.failures.map((failure) => failure.code);
}

function fixedIdentity(): RunnerVolumeIdentity {
  const seed = Buffer.from(
    "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    "hex",
  );
  const privateKey: KeyObject = createPrivateKey({
    key: Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), seed]),
    format: "der",
    type: "pkcs8",
  });
  const publicKey = createPublicKey(privateKey).export({ format: "jwk" });
  if (!publicKey.x) throw new Error("missing fixed public key");
  const publicKeyRaw = publicKey.x;
  const publicKeyBytes = Buffer.from(publicKeyRaw, "base64url");
  return {
    volumeId: createHash("sha256")
      .update("bluey-jobs-runner\0volume-id-v1\0", "utf8")
      .update(publicKeyBytes)
      .digest("base64url"),
    publicKeyRaw,
    publicKeyFingerprint: ed25519PublicKeyFingerprint(publicKeyRaw),
    privateKey,
  };
}
