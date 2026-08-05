import { createHash } from "node:crypto";
import {
  decodeCanonicalBase64Url,
  ed25519PublicKeyFingerprint,
  signEd25519,
  type RunnerVolumeIdentity,
  verifyEd25519,
} from "./volume-identity.js";

export const SUBJECT_STORAGE_LAYOUT_VERSION = 2 as const;
export const ACCOUNT_DATA_V2_DIRECTORY = "account-data-v2" as const;
export const SUBJECTS_DIRECTORY = "subjects" as const;
export const SCOPE_OWNERS_DIRECTORY = "scope-owners" as const;
export const SUBJECT_METADATA_FILE = "subject.json" as const;
export const SUBJECT_METADATA_AUDIENCE =
  "bluey-jobs-runner-subject-metadata" as const;
export const SCOPE_OWNERSHIP_AUDIENCE =
  "bluey-jobs-runner-scope-ownership" as const;
export const EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256 = createHash("sha256")
  .update("bluey-jobs-runner-subject-storage-inventory-v2\n", "utf8")
  .digest("hex");

export const LEGACY_ARTIFACT_ROOTS = [
  "active",
  "snapshots",
  "run-checkpoints",
  "receipts",
  "step-results",
] as const;
export const RUNNER_CONTROL_DIRECTORIES = [
  "volume-identity",
  "account-residency-v1",
  "runner-volume-control-v1",
] as const;
export const RUNNER_CONTROL_FILES = [".bluey-runner-storage.lock"] as const;
export const RUNNER_CONTROL_ROOTS = [
  ...RUNNER_CONTROL_DIRECTORIES,
  ...RUNNER_CONTROL_FILES,
] as const;

const SUBJECT_SHA256_PATTERN = /^[0-9a-f]{64}$/;
const PROFILE_SCOPE_PATTERN = /^[0-9a-f]{40}$/;
const RESULT_SCOPE_PATTERN = /^[0-9a-f]{64}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const DEVICE_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const DECIMAL_U64_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const MAXIMUM_U64 = 18_446_744_073_709_551_615n;
const MAXIMUM_CONTROL_RECORD_BYTES = 8 * 1024;
const MAXIMUM_AUDIT_ENTRIES = 100_000;
const MAXIMUM_AUDIT_DEPTH = 128;

declare const subjectSha256Brand: unique symbol;
declare const profileScopeBrand: unique symbol;
declare const resultScopeBrand: unique symbol;

export type SubjectSha256 = string & { readonly [subjectSha256Brand]: true };
export type ProfileScope = string & { readonly [profileScopeBrand]: true };
export type ResultScope = string & { readonly [resultScopeBrand]: true };
export type ScopeKind = "profile" | "result";
export type LegacyArtifactRoot = (typeof LEGACY_ARTIFACT_ROOTS)[number];
export type RunnerControlRoot = (typeof RUNNER_CONTROL_ROOTS)[number];

export type SubjectStorageLayoutErrorCode =
  | "corrupt_scope_ownership"
  | "corrupt_subject_metadata"
  | "invalid_identity"
  | "invalid_profile_scope"
  | "invalid_result_scope"
  | "invalid_subject_hash";

export class SubjectStorageLayoutError extends Error {
  constructor(readonly code: SubjectStorageLayoutErrorCode) {
    super({
      corrupt_scope_ownership: "The runner scope-ownership record is corrupt.",
      corrupt_subject_metadata: "The runner subject metadata is corrupt.",
      invalid_identity: "The runner volume identity is invalid.",
      invalid_profile_scope: "The runner profile scope is invalid.",
      invalid_result_scope: "The runner result scope is invalid.",
      invalid_subject_hash: "The runner subject hash is invalid.",
    }[code]);
    this.name = "SubjectStorageLayoutError";
  }
}

export type SubjectStoragePathKind =
  | "layout_root"
  | "subject_root"
  | "subject_metadata"
  | "profiles_root"
  | "profile_root"
  | "profile_active"
  | "profile_snapshots"
  | "profile_snapshot"
  | "profile_snapshot_generation"
  | "profile_checkpoints"
  | "profile_receipts"
  | "profile_temporary"
  | "results_root"
  | "result_root"
  | "result_envelope"
  | "result_temporary"
  | "scope_owners_root"
  | "scope_owner";

/**
 * A component-only path for the native retained-root API. Callers must pass
 * components to that API; `relativePath` is canonical evidence, not a path to
 * reopen with Node filesystem functions.
 */
export interface SubjectStoragePath<
  Kind extends SubjectStoragePathKind = SubjectStoragePathKind,
  Components extends readonly string[] = readonly string[],
> {
  readonly kind: Kind;
  readonly components: Components;
  readonly relativePath: string;
}

export interface ProfileStoragePaths {
  readonly root: SubjectStoragePath<"profile_root">;
  readonly active: SubjectStoragePath<"profile_active">;
  readonly snapshots: SubjectStoragePath<"profile_snapshots">;
  readonly encryptedSnapshot: SubjectStoragePath<"profile_snapshot">;
  readonly snapshotGeneration: SubjectStoragePath<"profile_snapshot_generation">;
  readonly checkpoints: SubjectStoragePath<"profile_checkpoints">;
  readonly receipts: SubjectStoragePath<"profile_receipts">;
  readonly temporary: SubjectStoragePath<"profile_temporary">;
  readonly ownership: SubjectStoragePath<"scope_owner">;
}

export interface ResultStoragePaths {
  readonly root: SubjectStoragePath<"result_root">;
  readonly encryptedResult: SubjectStoragePath<"result_envelope">;
  readonly temporary: SubjectStoragePath<"result_temporary">;
  readonly ownership: SubjectStoragePath<"scope_owner">;
}

export interface SubjectStoragePaths {
  readonly root: SubjectStoragePath<"subject_root">;
  readonly metadata: SubjectStoragePath<"subject_metadata">;
  readonly profiles: SubjectStoragePath<"profiles_root">;
  readonly results: SubjectStoragePath<"results_root">;
  profile(scope: string): ProfileStoragePaths;
  result(scope: string): ResultStoragePaths;
}

export function subjectSha256(value: string): SubjectSha256 {
  if (typeof value !== "string" || !SUBJECT_SHA256_PATTERN.test(value)) {
    throw new SubjectStorageLayoutError("invalid_subject_hash");
  }
  return value as SubjectSha256;
}

export function profileScope(value: string): ProfileScope {
  if (typeof value !== "string" || !PROFILE_SCOPE_PATTERN.test(value)) {
    throw new SubjectStorageLayoutError("invalid_profile_scope");
  }
  return value as ProfileScope;
}

export function resultScope(value: string): ResultScope {
  if (typeof value !== "string" || !RESULT_SCOPE_PATTERN.test(value)) {
    throw new SubjectStorageLayoutError("invalid_result_scope");
  }
  return value as ResultScope;
}

export function accountDataV2Path(): SubjectStoragePath<"layout_root"> {
  return storagePath("layout_root", [ACCOUNT_DATA_V2_DIRECTORY]);
}

export function subjectStoragePaths(value: string): SubjectStoragePaths {
  const subject = subjectSha256(value);
  const root = [ACCOUNT_DATA_V2_DIRECTORY, SUBJECTS_DIRECTORY, subject] as const;
  return Object.freeze({
    root: storagePath("subject_root", root),
    metadata: storagePath("subject_metadata", [...root, SUBJECT_METADATA_FILE]),
    profiles: storagePath("profiles_root", [...root, "profiles"]),
    results: storagePath("results_root", [...root, "results"]),
    profile(scopeValue: string): ProfileStoragePaths {
      return profileStoragePaths(subject, profileScope(scopeValue));
    },
    result(scopeValue: string): ResultStoragePaths {
      return resultStoragePaths(subject, resultScope(scopeValue));
    },
  });
}

export function scopeOwnershipPath(
  kind: ScopeKind,
  scopeValue: string,
): SubjectStoragePath<"scope_owner"> {
  const scope = kind === "profile" ? profileScope(scopeValue) : resultScope(scopeValue);
  return storagePath("scope_owner", [
    ACCOUNT_DATA_V2_DIRECTORY,
    SCOPE_OWNERS_DIRECTORY,
    `${kind}s`,
    `${scope}.json`,
  ]);
}

export interface UnsignedSubjectMetadata {
  readonly version: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
  readonly audience: typeof SUBJECT_METADATA_AUDIENCE;
  readonly volumeId: string;
  readonly volumeKeyFingerprint: string;
  readonly subjectSha256: SubjectSha256;
  readonly subjectRootPath: string;
  readonly metadataPath: string;
}

export interface SignedSubjectMetadata extends UnsignedSubjectMetadata {
  readonly signature: string;
}

export interface UnsignedScopeOwnership {
  readonly version: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
  readonly audience: typeof SCOPE_OWNERSHIP_AUDIENCE;
  readonly volumeId: string;
  readonly volumeKeyFingerprint: string;
  readonly subjectSha256: SubjectSha256;
  readonly kind: ScopeKind;
  readonly scope: ProfileScope | ResultScope;
  readonly subjectRootPath: string;
  readonly scopeRootPath: string;
  readonly ownershipPath: string;
}

export interface SignedScopeOwnership extends UnsignedScopeOwnership {
  readonly signature: string;
}

export type RunnerVolumeIdentityVerifier = Pick<
  RunnerVolumeIdentity,
  "publicKeyFingerprint" | "publicKeyRaw" | "volumeId"
>;

export function createSignedSubjectMetadata(
  identity: RunnerVolumeIdentity,
  subjectValue: string,
): SignedSubjectMetadata {
  assertRunnerVolumeIdentity(identity);
  const subject = subjectSha256(subjectValue);
  const paths = subjectStoragePaths(subject);
  const unsigned: UnsignedSubjectMetadata = {
    version: SUBJECT_STORAGE_LAYOUT_VERSION,
    audience: SUBJECT_METADATA_AUDIENCE,
    volumeId: identity.volumeId,
    volumeKeyFingerprint: identity.publicKeyFingerprint,
    subjectSha256: subject,
    subjectRootPath: paths.root.relativePath,
    metadataPath: paths.metadata.relativePath,
  };
  return Object.freeze({
    ...unsigned,
    signature: signEd25519(identity.privateKey, canonicalSubjectMetadataBytes(unsigned)),
  });
}

export function createSignedScopeOwnership(
  identity: RunnerVolumeIdentity,
  subjectValue: string,
  kind: ScopeKind,
  scopeValue: string,
): SignedScopeOwnership {
  assertRunnerVolumeIdentity(identity);
  const subject = subjectSha256(subjectValue);
  const subjectPaths = subjectStoragePaths(subject);
  const scope = kind === "profile" ? profileScope(scopeValue) : resultScope(scopeValue);
  const scopeRootPath = kind === "profile"
    ? subjectPaths.profile(scope).root.relativePath
    : subjectPaths.result(scope).root.relativePath;
  const ownershipPath = scopeOwnershipPath(kind, scope).relativePath;
  const unsigned: UnsignedScopeOwnership = {
    version: SUBJECT_STORAGE_LAYOUT_VERSION,
    audience: SCOPE_OWNERSHIP_AUDIENCE,
    volumeId: identity.volumeId,
    volumeKeyFingerprint: identity.publicKeyFingerprint,
    subjectSha256: subject,
    kind,
    scope,
    subjectRootPath: subjectPaths.root.relativePath,
    scopeRootPath,
    ownershipPath,
  };
  return Object.freeze({
    ...unsigned,
    signature: signEd25519(identity.privateKey, canonicalScopeOwnershipBytes(unsigned)),
  });
}

export function canonicalSubjectMetadataBytes(metadata: UnsignedSubjectMetadata): Buffer {
  return Buffer.from([
    "bluey-jobs-runner-subject-metadata-v2",
    `version=${metadata.version}`,
    `audience=${metadata.audience}`,
    `volume_id=${metadata.volumeId}`,
    `volume_key_fingerprint=${metadata.volumeKeyFingerprint}`,
    `subject_sha256=${metadata.subjectSha256}`,
    `subject_root_path=${metadata.subjectRootPath}`,
    `metadata_path=${metadata.metadataPath}`,
    "",
  ].join("\n"), "utf8");
}

export function canonicalScopeOwnershipBytes(ownership: UnsignedScopeOwnership): Buffer {
  return Buffer.from([
    "bluey-jobs-runner-scope-ownership-v2",
    `version=${ownership.version}`,
    `audience=${ownership.audience}`,
    `volume_id=${ownership.volumeId}`,
    `volume_key_fingerprint=${ownership.volumeKeyFingerprint}`,
    `subject_sha256=${ownership.subjectSha256}`,
    `kind=${ownership.kind}`,
    `scope=${ownership.scope}`,
    `subject_root_path=${ownership.subjectRootPath}`,
    `scope_root_path=${ownership.scopeRootPath}`,
    `ownership_path=${ownership.ownershipPath}`,
    "",
  ].join("\n"), "utf8");
}

export function encodeSubjectMetadata(metadata: SignedSubjectMetadata): Buffer {
  return Buffer.from(`${JSON.stringify(metadata)}\n`, "utf8");
}

export function encodeScopeOwnership(ownership: SignedScopeOwnership): Buffer {
  return Buffer.from(`${JSON.stringify(ownership)}\n`, "utf8");
}

export function parseSubjectMetadata(
  encodedValue: Uint8Array,
  identity: RunnerVolumeIdentityVerifier,
): SignedSubjectMetadata {
  try {
    assertRunnerVolumeIdentity(identity);
    const encoded = boundedControlBytes(encodedValue, "corrupt_subject_metadata");
    const value: unknown = JSON.parse(encoded.toString("utf8"));
    if (!isRecord(value)
      || !hasExactKeys(value, [
        "audience",
        "metadataPath",
        "signature",
        "subjectRootPath",
        "subjectSha256",
        "version",
        "volumeId",
        "volumeKeyFingerprint",
      ])
      || value.version !== SUBJECT_STORAGE_LAYOUT_VERSION
      || value.audience !== SUBJECT_METADATA_AUDIENCE
      || value.volumeId !== identity.volumeId
      || value.volumeKeyFingerprint !== identity.publicKeyFingerprint
      || typeof value.subjectSha256 !== "string"
      || typeof value.subjectRootPath !== "string"
      || typeof value.metadataPath !== "string"
      || typeof value.signature !== "string") {
      throw new SubjectStorageLayoutError("corrupt_subject_metadata");
    }
    const subject = subjectSha256(value.subjectSha256);
    const paths = subjectStoragePaths(subject);
    if (value.subjectRootPath !== paths.root.relativePath
      || value.metadataPath !== paths.metadata.relativePath) {
      throw new SubjectStorageLayoutError("corrupt_subject_metadata");
    }
    const metadata: SignedSubjectMetadata = {
      version: SUBJECT_STORAGE_LAYOUT_VERSION,
      audience: SUBJECT_METADATA_AUDIENCE,
      volumeId: identity.volumeId,
      volumeKeyFingerprint: identity.publicKeyFingerprint,
      subjectSha256: subject,
      subjectRootPath: paths.root.relativePath,
      metadataPath: paths.metadata.relativePath,
      signature: value.signature,
    };
    const { signature, ...unsigned } = metadata;
    if (!verifyEd25519(identity.publicKeyRaw, canonicalSubjectMetadataBytes(unsigned), signature)
      || !encoded.equals(encodeSubjectMetadata(metadata))) {
      throw new SubjectStorageLayoutError("corrupt_subject_metadata");
    }
    return Object.freeze(metadata);
  } catch (error) {
    if (error instanceof SubjectStorageLayoutError
      && error.code === "corrupt_subject_metadata") {
      throw error;
    }
    throw new SubjectStorageLayoutError("corrupt_subject_metadata");
  }
}

export function parseScopeOwnership(
  encodedValue: Uint8Array,
  identity: RunnerVolumeIdentityVerifier,
): SignedScopeOwnership {
  try {
    assertRunnerVolumeIdentity(identity);
    const encoded = boundedControlBytes(encodedValue, "corrupt_scope_ownership");
    const value: unknown = JSON.parse(encoded.toString("utf8"));
    if (!isRecord(value)
      || !hasExactKeys(value, [
        "audience",
        "kind",
        "ownershipPath",
        "scope",
        "scopeRootPath",
        "signature",
        "subjectRootPath",
        "subjectSha256",
        "version",
        "volumeId",
        "volumeKeyFingerprint",
      ])
      || value.version !== SUBJECT_STORAGE_LAYOUT_VERSION
      || value.audience !== SCOPE_OWNERSHIP_AUDIENCE
      || value.volumeId !== identity.volumeId
      || value.volumeKeyFingerprint !== identity.publicKeyFingerprint
      || (value.kind !== "profile" && value.kind !== "result")
      || typeof value.subjectSha256 !== "string"
      || typeof value.scope !== "string"
      || typeof value.subjectRootPath !== "string"
      || typeof value.scopeRootPath !== "string"
      || typeof value.ownershipPath !== "string"
      || typeof value.signature !== "string") {
      throw new SubjectStorageLayoutError("corrupt_scope_ownership");
    }
    const subject = subjectSha256(value.subjectSha256);
    const scope = value.kind === "profile" ? profileScope(value.scope) : resultScope(value.scope);
    const subjectPaths = subjectStoragePaths(subject);
    const scopeRootPath = value.kind === "profile"
      ? subjectPaths.profile(scope).root.relativePath
      : subjectPaths.result(scope).root.relativePath;
    const ownershipPath = scopeOwnershipPath(value.kind, scope).relativePath;
    if (value.subjectRootPath !== subjectPaths.root.relativePath
      || value.scopeRootPath !== scopeRootPath
      || value.ownershipPath !== ownershipPath) {
      throw new SubjectStorageLayoutError("corrupt_scope_ownership");
    }
    const ownership: SignedScopeOwnership = {
      version: SUBJECT_STORAGE_LAYOUT_VERSION,
      audience: SCOPE_OWNERSHIP_AUDIENCE,
      volumeId: identity.volumeId,
      volumeKeyFingerprint: identity.publicKeyFingerprint,
      subjectSha256: subject,
      kind: value.kind,
      scope,
      subjectRootPath: subjectPaths.root.relativePath,
      scopeRootPath,
      ownershipPath,
      signature: value.signature,
    };
    const { signature, ...unsigned } = ownership;
    if (!verifyEd25519(identity.publicKeyRaw, canonicalScopeOwnershipBytes(unsigned), signature)
      || !encoded.equals(encodeScopeOwnership(ownership))) {
      throw new SubjectStorageLayoutError("corrupt_scope_ownership");
    }
    return Object.freeze(ownership);
  } catch (error) {
    if (error instanceof SubjectStorageLayoutError
      && error.code === "corrupt_scope_ownership") {
      throw error;
    }
    throw new SubjectStorageLayoutError("corrupt_scope_ownership");
  }
}

export type NativeScannerEntryKind = "directory" | "file" | "special" | "symlink";

/** Evidence emitted while the native scanner retains the parent directory handle. */
export interface NativeScannerNodeEvidence {
  readonly kind: NativeScannerEntryKind;
  readonly deviceId: string;
  readonly linkCount: number;
}

/**
 * `components` are relative to the retained `account-data-v2` directory
 * handle. Control bytes are populated only for signed metadata/ownership
 * records; the scanner supplies hashes rather than contents for artifacts.
 */
export interface NativeScannerRecord extends NativeScannerNodeEvidence {
  readonly components: readonly string[];
  readonly sizeBytes: string;
  readonly sha256: string | null;
  readonly controlBytes: Uint8Array | null;
}

export type NativeAccountDataV2Scan =
  | { readonly state: "absent" }
  | ({
    readonly state: "present";
    readonly entries: readonly NativeScannerRecord[];
  } & NativeScannerNodeEvidence);

export interface NativeRunnerRootEntryEvidence extends NativeScannerNodeEvidence {
  readonly name: string;
}

export interface SubjectStorageAuditInput {
  readonly targetSubjectSha256: string;
  readonly identity: RunnerVolumeIdentityVerifier;
  readonly runnerRootDeviceId: string;
  /** Complete immediate-child scan of the retained runner root handle. */
  readonly runnerRootEntries: readonly NativeRunnerRootEntryEvidence[];
  readonly accountDataV2: NativeAccountDataV2Scan;
}

export type SubjectStorageAuditFailureCode =
  | "corrupt_scope_ownership"
  | "corrupt_subject_metadata"
  | "cross_device_entry"
  | "duplicate_path"
  | "duplicate_scope_ownership"
  | "hardlinked_entry"
  | "incomplete_artifact"
  | "invalid_scan"
  | "legacy_entry"
  | "missing_parent"
  | "missing_required_entry"
  | "missing_subject_metadata"
  | "orphan_scope_ownership"
  | "scope_ownership_conflict"
  | "unindexed_artifact"
  | "unknown_entry"
  | "unsafe_special_entry"
  | "unsafe_symlink";

export interface SubjectStorageAuditFailure {
  readonly code: SubjectStorageAuditFailureCode;
  readonly path: string;
  readonly detail: string;
}

export interface SubjectStorageInventoryEvidence {
  readonly entryCount: number;
  readonly fileBytes: string;
  readonly sha256: string;
}

export interface AuditedSubjectStorage {
  readonly subjectSha256: SubjectSha256;
  readonly profileScopes: readonly ProfileScope[];
  readonly resultScopes: readonly ResultScope[];
  readonly scopeOwnershipPaths: readonly string[];
  readonly subjectTreeInventory: SubjectStorageInventoryEvidence;
  readonly scopeOwnershipInventory: SubjectStorageInventoryEvidence;
  /** Subject subtree plus its global immutable scope-ownership records. */
  readonly inventory: SubjectStorageInventoryEvidence;
}

export interface SubjectStorageAuditSuccess {
  readonly ok: true;
  readonly version: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
  readonly target: {
    readonly subjectSha256: SubjectSha256;
    readonly residency: "never_resident" | "resident";
  };
  readonly subjects: readonly AuditedSubjectStorage[];
  readonly inventory: SubjectStorageInventoryEvidence;
}

export interface SubjectStorageAuditRejected {
  readonly ok: false;
  readonly version: typeof SUBJECT_STORAGE_LAYOUT_VERSION;
  readonly failures: readonly SubjectStorageAuditFailure[];
}

export type SubjectStorageAuditResult =
  | SubjectStorageAuditSuccess
  | SubjectStorageAuditRejected;

interface NormalizedScannerRecord extends NativeScannerRecord {
  readonly components: readonly string[];
  readonly relativePath: string;
  readonly fullPath: string;
  readonly fileBytes: bigint;
}

interface ScopeDirectory {
  readonly kind: ScopeKind;
  readonly scope: ProfileScope | ResultScope;
  readonly subjectSha256: SubjectSha256;
  readonly entry: NormalizedScannerRecord;
}

interface OwnerFile {
  readonly kind: ScopeKind;
  readonly scope: ProfileScope | ResultScope;
  readonly entry: NormalizedScannerRecord;
}

/**
 * Audit a complete native scan without reopening any pathname. Success proves
 * the v2 namespace is structurally closed and every scope has exactly one
 * signed, volume-bound owner. It does not weaken native no-follow protections.
 */
export function auditSubjectStorageLayout(input: SubjectStorageAuditInput): SubjectStorageAuditResult {
  const failures: SubjectStorageAuditFailure[] = [];
  let target: SubjectSha256 | undefined;
  try {
    target = subjectSha256(input.targetSubjectSha256);
  } catch {
    addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "invalid_target_subject");
  }
  try {
    assertRunnerVolumeIdentity(input.identity);
  } catch {
    addFailure(failures, "invalid_scan", "volume-identity", "invalid_volume_identity");
  }
  const runnerRootDeviceId = validDeviceId(input.runnerRootDeviceId)
    ? input.runnerRootDeviceId
    : undefined;
  if (!runnerRootDeviceId) {
    addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "invalid_root_device_id");
  }

  const rawAccountDataV2: unknown = input.accountDataV2;
  if (!isRecord(rawAccountDataV2)
    || (rawAccountDataV2.state !== "absent" && rawAccountDataV2.state !== "present")) {
    addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "invalid_v2_root_state");
    return rejectedAudit(failures);
  }
  const accountDataV2 = rawAccountDataV2 as NativeAccountDataV2Scan;
  auditRunnerRootNamespace(
    input.runnerRootEntries,
    accountDataV2,
    runnerRootDeviceId,
    failures,
  );
  if (accountDataV2.state === "absent") {
    if (!target || failures.length !== 0) return rejectedAudit(failures);
    return Object.freeze({
      ok: true,
      version: SUBJECT_STORAGE_LAYOUT_VERSION,
      target: Object.freeze({ subjectSha256: target, residency: "never_resident" }),
      subjects: Object.freeze([]),
      inventory: emptyInventoryEvidence(),
    });
  }

  auditNodeSafety(
    accountDataV2,
    ACCOUNT_DATA_V2_DIRECTORY,
    runnerRootDeviceId,
    failures,
    true,
  );
  if (!Array.isArray(accountDataV2.entries)) {
    addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "missing_entries");
    return rejectedAudit(failures);
  }
  if (accountDataV2.entries.length > MAXIMUM_AUDIT_ENTRIES) {
    addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "entry_limit_exceeded");
    return rejectedAudit(failures);
  }

  const entries = normalizeScannerRecords(
    accountDataV2.entries,
    runnerRootDeviceId,
    failures,
  );
  const entryMap = new Map<string, NormalizedScannerRecord>();
  for (const entry of entries) {
    if (entryMap.has(entry.relativePath)) {
      addFailure(failures, "duplicate_path", entry.fullPath, "duplicate_scanner_record");
    } else {
      entryMap.set(entry.relativePath, entry);
    }
  }
  for (const entry of entries) auditParent(entry, entryMap, failures);

  const subjectRoots = new Map<SubjectSha256, NormalizedScannerRecord>();
  const subjectMetadata = new Map<SubjectSha256, NormalizedScannerRecord>();
  const scopeDirectories: ScopeDirectory[] = [];
  const ownerFiles: OwnerFile[] = [];
  for (const entry of entries) {
    classifyEntry(entry, failures, subjectRoots, subjectMetadata, scopeDirectories, ownerFiles);
  }

  requireDirectory(entryMap, [SUBJECTS_DIRECTORY], failures);
  requireDirectory(entryMap, [SCOPE_OWNERS_DIRECTORY], failures);
  requireDirectory(entryMap, [SCOPE_OWNERS_DIRECTORY, "profiles"], failures);
  requireDirectory(entryMap, [SCOPE_OWNERS_DIRECTORY, "results"], failures);

  const validSubjects = new Set<SubjectSha256>();
  for (const subject of [...subjectRoots.keys()].sort()) {
    const root = subjectRoots.get(subject)!;
    requireDirectory(entryMap, [SUBJECTS_DIRECTORY, subject, "profiles"], failures);
    requireDirectory(entryMap, [SUBJECTS_DIRECTORY, subject, "results"], failures);
    const metadata = subjectMetadata.get(subject);
    if (!metadata) {
      addFailure(failures, "missing_subject_metadata", root.fullPath, SUBJECT_METADATA_FILE);
      continue;
    }
    const parsed = parseSubjectMetadataForAudit(metadata, input.identity, failures);
    if (parsed && parsed.subjectSha256 === subject) validSubjects.add(subject);
    else if (parsed) {
      addFailure(failures, "corrupt_subject_metadata", metadata.fullPath, "path_subject_mismatch");
    }
  }

  for (const scopeDirectory of scopeDirectories) {
    auditScopeScaffold(scopeDirectory, entryMap, failures);
  }

  const parsedOwners = new Map<string, SignedScopeOwnership>();
  for (const owner of ownerFiles) {
    const parsed = parseScopeOwnershipForAudit(owner.entry, input.identity, failures);
    if (!parsed) continue;
    if (parsed.kind !== owner.kind || parsed.scope !== owner.scope) {
      addFailure(
        failures,
        "corrupt_scope_ownership",
        owner.entry.fullPath,
        "ownership_path_mismatch",
      );
      continue;
    }
    parsedOwners.set(scopeKey(owner.kind, owner.scope), parsed);
  }

  const directoriesByScope = new Map<string, ScopeDirectory[]>();
  for (const directory of scopeDirectories) {
    const key = scopeKey(directory.kind, directory.scope);
    const existing = directoriesByScope.get(key) ?? [];
    existing.push(directory);
    directoriesByScope.set(key, existing);
  }
  for (const [key, directories] of directoriesByScope) {
    directories.sort((left, right) => compareCanonicalUtf8(
      left.entry.fullPath,
      right.entry.fullPath,
    ));
    if (directories.length > 1) {
      addFailure(
        failures,
        "duplicate_scope_ownership",
        directories[0]!.entry.fullPath,
        `scope=${key}`,
      );
    }
    const owner = parsedOwners.get(key);
    if (!owner) {
      for (const directory of directories) {
        addFailure(failures, "unindexed_artifact", directory.entry.fullPath, "missing_scope_owner");
      }
      continue;
    }
    for (const directory of directories) {
      if (owner.subjectSha256 !== directory.subjectSha256
        || owner.scopeRootPath !== directory.entry.fullPath) {
        addFailure(
          failures,
          "scope_ownership_conflict",
          directory.entry.fullPath,
          `owner_subject=${owner.subjectSha256}`,
        );
      }
    }
  }
  for (const [key, owner] of parsedOwners) {
    if (!directoriesByScope.has(key)) {
      addFailure(failures, "orphan_scope_ownership", owner.ownershipPath, "missing_scope_root");
    }
    if (!validSubjects.has(owner.subjectSha256)) {
      addFailure(failures, "orphan_scope_ownership", owner.ownershipPath, "missing_valid_subject");
    }
  }

  if (!target || failures.length !== 0) return rejectedAudit(failures);

  const subjects = [...validSubjects].sort().map((subject) => {
    const directories = scopeDirectories.filter((value) => value.subjectSha256 === subject);
    const profileScopes = directories
      .filter((value) => value.kind === "profile")
      .map((value) => value.scope as ProfileScope)
      .sort();
    const resultScopes = directories
      .filter((value) => value.kind === "result")
      .map((value) => value.scope as ResultScope)
      .sort();
    const subjectTreeEntries = entries.filter((entry) => (
      entry.components[0] === SUBJECTS_DIRECTORY && entry.components[1] === subject
    ));
    const subjectOwnerEntries = ownerFiles
      .filter((owner) => parsedOwners.get(scopeKey(owner.kind, owner.scope))?.subjectSha256 === subject)
      .map((owner) => owner.entry)
      .sort((left, right) => compareCanonicalUtf8(left.fullPath, right.fullPath));
    return Object.freeze({
      subjectSha256: subject,
      profileScopes: Object.freeze(profileScopes),
      resultScopes: Object.freeze(resultScopes),
      scopeOwnershipPaths: Object.freeze(subjectOwnerEntries.map((entry) => entry.fullPath)),
      subjectTreeInventory: inventoryEvidence(subjectTreeEntries),
      scopeOwnershipInventory: inventoryEvidence(subjectOwnerEntries),
      inventory: inventoryEvidence([...subjectTreeEntries, ...subjectOwnerEntries]),
    });
  });
  return Object.freeze({
    ok: true,
    version: SUBJECT_STORAGE_LAYOUT_VERSION,
    target: Object.freeze({
      subjectSha256: target,
      residency: validSubjects.has(target) ? "resident" : "never_resident",
    }),
    subjects: Object.freeze(subjects),
    inventory: inventoryEvidence(entries),
  });
}

function profileStoragePaths(subject: SubjectSha256, scope: ProfileScope): ProfileStoragePaths {
  const root = [
    ACCOUNT_DATA_V2_DIRECTORY,
    SUBJECTS_DIRECTORY,
    subject,
    "profiles",
    scope,
  ] as const;
  return Object.freeze({
    root: storagePath("profile_root", root),
    active: storagePath("profile_active", [...root, "active"]),
    snapshots: storagePath("profile_snapshots", [...root, "snapshots"]),
    encryptedSnapshot: storagePath("profile_snapshot", [
      ...root,
      "snapshots",
      "profile.tar.gz.enc",
    ]),
    snapshotGeneration: storagePath("profile_snapshot_generation", [
      ...root,
      "snapshots",
      "generation",
    ]),
    checkpoints: storagePath("profile_checkpoints", [...root, "run-checkpoints"]),
    receipts: storagePath("profile_receipts", [...root, "receipts"]),
    temporary: storagePath("profile_temporary", [...root, "temporary"]),
    ownership: scopeOwnershipPath("profile", scope),
  });
}

function resultStoragePaths(subject: SubjectSha256, scope: ResultScope): ResultStoragePaths {
  const root = [
    ACCOUNT_DATA_V2_DIRECTORY,
    SUBJECTS_DIRECTORY,
    subject,
    "results",
    scope,
  ] as const;
  return Object.freeze({
    root: storagePath("result_root", root),
    encryptedResult: storagePath("result_envelope", [...root, "step-result.json.enc"]),
    temporary: storagePath("result_temporary", [...root, "temporary"]),
    ownership: scopeOwnershipPath("result", scope),
  });
}

function storagePath<Kind extends SubjectStoragePathKind, Components extends readonly string[]>(
  kind: Kind,
  componentsValue: Components,
): SubjectStoragePath<Kind, Components> {
  const components = Object.freeze([...componentsValue]) as unknown as Components;
  return Object.freeze({
    kind,
    components,
    relativePath: components.join("/"),
  });
}

function assertRunnerVolumeIdentity(identity: RunnerVolumeIdentityVerifier): void {
  try {
    if (!identity || typeof identity !== "object"
      || typeof identity.volumeId !== "string"
      || typeof identity.publicKeyRaw !== "string"
      || typeof identity.publicKeyFingerprint !== "string") {
      throw new Error("invalid identity");
    }
    const publicKey = decodeCanonicalBase64Url(identity.publicKeyRaw, 32);
    decodeCanonicalBase64Url(identity.volumeId, 32);
    if (ed25519PublicKeyFingerprint(identity.publicKeyRaw) !== identity.publicKeyFingerprint
      || createHash("sha256")
        .update("bluey-jobs-runner\0volume-id-v1\0", "utf8")
        .update(publicKey)
        .digest("base64url") !== identity.volumeId) {
      throw new Error("identity binding mismatch");
    }
  } catch {
    throw new SubjectStorageLayoutError("invalid_identity");
  }
}

function boundedControlBytes(
  value: Uint8Array,
  code: "corrupt_scope_ownership" | "corrupt_subject_metadata",
): Buffer {
  if (!(value instanceof Uint8Array)
    || value.byteLength === 0
    || value.byteLength > MAXIMUM_CONTROL_RECORD_BYTES) {
    throw new SubjectStorageLayoutError(code);
  }
  return Buffer.from(value);
}

function auditRunnerRootNamespace(
  rootEntries: readonly NativeRunnerRootEntryEvidence[],
  accountDataV2: NativeAccountDataV2Scan,
  rootDeviceId: string | undefined,
  failures: SubjectStorageAuditFailure[],
): void {
  if (!Array.isArray(rootEntries)) {
    addFailure(failures, "invalid_scan", "runner-root", "missing_root_namespace_scan");
    return;
  }
  if (rootEntries.length > 64) {
    addFailure(failures, "invalid_scan", "runner-root", "root_entry_limit_exceeded");
    return;
  }
  const seen = new Map<string, NativeRunnerRootEntryEvidence>();
  for (const evidence of rootEntries) {
    if (!isRecord(evidence)
      || typeof evidence.name !== "string"
      || !validPathComponent(evidence.name)) {
      addFailure(failures, "invalid_scan", "runner-root", "invalid_root_entry_evidence");
      continue;
    }
    if (seen.has(evidence.name)) {
      addFailure(failures, "duplicate_path", evidence.name, "duplicate_root_entry");
      continue;
    }
    const typed = evidence as unknown as NativeRunnerRootEntryEvidence;
    seen.set(evidence.name, typed);
    auditNodeSafety(typed, typed.name, rootDeviceId, failures, false);
    if (LEGACY_ARTIFACT_ROOTS.includes(typed.name as LegacyArtifactRoot)) {
      addFailure(failures, "legacy_entry", typed.name, "legacy_root_present");
    } else if (RUNNER_CONTROL_DIRECTORIES.includes(
      typed.name as (typeof RUNNER_CONTROL_DIRECTORIES)[number],
    )) {
      if (typed.kind !== "directory") {
        addFailure(failures, "unsafe_special_entry", typed.name, "control_root_not_directory");
      }
    } else if (RUNNER_CONTROL_FILES.includes(
      typed.name as (typeof RUNNER_CONTROL_FILES)[number],
    )) {
      if (typed.kind !== "file") {
        addFailure(failures, "unsafe_special_entry", typed.name, "control_entry_not_file");
      }
    } else if (typed.name !== ACCOUNT_DATA_V2_DIRECTORY) {
      addFailure(failures, "unknown_entry", typed.name, "unknown_runner_root_entry");
    }
  }
  const v2Root = seen.get(ACCOUNT_DATA_V2_DIRECTORY);
  if (accountDataV2.state === "absent") {
    if (v2Root) {
      addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "v2_root_state_mismatch");
    }
  } else if (!v2Root) {
    addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "v2_root_missing_from_scan");
  } else if (v2Root.kind !== accountDataV2.kind
    || v2Root.deviceId !== accountDataV2.deviceId
    || v2Root.linkCount !== accountDataV2.linkCount) {
    addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "v2_root_evidence_mismatch");
  }
}

function normalizeScannerRecords(
  records: readonly NativeScannerRecord[],
  rootDeviceId: string | undefined,
  failures: SubjectStorageAuditFailure[],
): NormalizedScannerRecord[] {
  const normalized: NormalizedScannerRecord[] = [];
  for (const candidate of records) {
    if (!isRecord(candidate) || !Array.isArray(candidate.components)) {
      addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "invalid_entry_record");
      continue;
    }
    const components = candidate.components;
    if (components.length === 0 || components.length > MAXIMUM_AUDIT_DEPTH
      || components.some((component) => !validPathComponent(component))) {
      addFailure(failures, "invalid_scan", ACCOUNT_DATA_V2_DIRECTORY, "invalid_components");
      continue;
    }
    const relativePath = components.join("/");
    const fullPath = `${ACCOUNT_DATA_V2_DIRECTORY}/${relativePath}`;
    if (!isNativeScannerEntryKind(candidate.kind)
      || !validDeviceId(candidate.deviceId)
      || !Number.isSafeInteger(candidate.linkCount)
      || candidate.linkCount < 1
      || typeof candidate.sizeBytes !== "string"
      || !DECIMAL_U64_PATTERN.test(candidate.sizeBytes)) {
      addFailure(failures, "invalid_scan", fullPath, "invalid_entry_evidence");
      continue;
    }
    let fileBytes: bigint;
    try {
      fileBytes = BigInt(candidate.sizeBytes);
    } catch {
      addFailure(failures, "invalid_scan", fullPath, "invalid_size");
      continue;
    }
    if (fileBytes > MAXIMUM_U64) {
      addFailure(failures, "invalid_scan", fullPath, "invalid_size");
      continue;
    }
    if (candidate.kind === "file") {
      if (typeof candidate.sha256 !== "string" || !SHA256_PATTERN.test(candidate.sha256)) {
        addFailure(failures, "invalid_scan", fullPath, "invalid_file_hash");
        continue;
      }
    } else if (candidate.sha256 !== null || candidate.sizeBytes !== "0") {
      addFailure(failures, "invalid_scan", fullPath, "invalid_nonfile_evidence");
      continue;
    }
    if (candidate.controlBytes !== null && !(candidate.controlBytes instanceof Uint8Array)) {
      addFailure(failures, "invalid_scan", fullPath, "invalid_control_bytes");
      continue;
    }
    if (candidate.controlBytes !== null) {
      const bytes = Buffer.from(candidate.controlBytes);
      if (candidate.kind !== "file"
        || BigInt(bytes.length) !== fileBytes
        || createHash("sha256").update(bytes).digest("hex") !== candidate.sha256) {
        addFailure(failures, "invalid_scan", fullPath, "control_evidence_mismatch");
        continue;
      }
    }
    const entry: NormalizedScannerRecord = {
      components: Object.freeze([...components]),
      kind: candidate.kind,
      deviceId: candidate.deviceId,
      linkCount: candidate.linkCount,
      sizeBytes: candidate.sizeBytes,
      sha256: candidate.sha256,
      controlBytes: candidate.controlBytes,
      relativePath,
      fullPath,
      fileBytes: candidate.kind === "file" ? fileBytes : 0n,
    };
    auditNodeSafety(entry, fullPath, rootDeviceId, failures, false);
    normalized.push(entry);
  }
  return normalized.sort((left, right) => compareCanonicalUtf8(
    left.relativePath,
    right.relativePath,
  ));
}

function auditNodeSafety(
  evidence: NativeScannerNodeEvidence,
  path: string,
  rootDeviceId: string | undefined,
  failures: SubjectStorageAuditFailure[],
  requireDirectory: boolean,
): void {
  if (!isRecord(evidence)
    || !isNativeScannerEntryKind(evidence.kind)
    || !validDeviceId(evidence.deviceId)
    || !Number.isSafeInteger(evidence.linkCount)
    || evidence.linkCount < 1) {
    addFailure(failures, "invalid_scan", path, "invalid_node_evidence");
    return;
  }
  if (evidence.kind === "symlink") {
    addFailure(failures, "unsafe_symlink", path, "native_scanner_reported_symlink");
  } else if (evidence.kind === "special") {
    addFailure(failures, "unsafe_special_entry", path, "native_scanner_reported_special");
  } else if (requireDirectory && evidence.kind !== "directory") {
    addFailure(failures, "unsafe_special_entry", path, "root_is_not_directory");
  }
  if (rootDeviceId && evidence.deviceId !== rootDeviceId) {
    addFailure(failures, "cross_device_entry", path, `device=${evidence.deviceId}`);
  }
  if (evidence.kind === "file" && evidence.linkCount !== 1) {
    addFailure(failures, "hardlinked_entry", path, `link_count=${evidence.linkCount}`);
  }
}

function auditParent(
  entry: NormalizedScannerRecord,
  entries: ReadonlyMap<string, NormalizedScannerRecord>,
  failures: SubjectStorageAuditFailure[],
): void {
  if (entry.components.length === 1) return;
  const parentPath = entry.components.slice(0, -1).join("/");
  const parent = entries.get(parentPath);
  if (!parent || parent.kind !== "directory") {
    addFailure(failures, "missing_parent", entry.fullPath, `parent=${parentPath}`);
  }
}

function classifyEntry(
  entry: NormalizedScannerRecord,
  failures: SubjectStorageAuditFailure[],
  subjectRoots: Map<SubjectSha256, NormalizedScannerRecord>,
  subjectMetadata: Map<SubjectSha256, NormalizedScannerRecord>,
  scopeDirectories: ScopeDirectory[],
  ownerFiles: OwnerFile[],
): void {
  const components = entry.components;
  if (components[0] === SUBJECTS_DIRECTORY) {
    classifySubjectEntry(
      entry,
      failures,
      subjectRoots,
      subjectMetadata,
      scopeDirectories,
    );
    return;
  }
  if (components[0] === SCOPE_OWNERS_DIRECTORY) {
    classifyOwnerEntry(entry, failures, ownerFiles);
    return;
  }
  addFailure(failures, "unknown_entry", entry.fullPath, "unknown_layout_root_entry");
}

function classifySubjectEntry(
  entry: NormalizedScannerRecord,
  failures: SubjectStorageAuditFailure[],
  subjectRoots: Map<SubjectSha256, NormalizedScannerRecord>,
  subjectMetadata: Map<SubjectSha256, NormalizedScannerRecord>,
  scopeDirectories: ScopeDirectory[],
): void {
  const components = entry.components;
  if (components.length === 1) {
    requireEntryKind(entry, "directory", failures);
    return;
  }
  const subjectValue = components[1];
  if (!subjectValue || !SUBJECT_SHA256_PATTERN.test(subjectValue)) {
    addFailure(failures, "unknown_entry", entry.fullPath, "invalid_subject_directory");
    return;
  }
  const subject = subjectValue as SubjectSha256;
  if (components.length === 2) {
    requireEntryKind(entry, "directory", failures);
    subjectRoots.set(subject, entry);
    return;
  }
  const category = components[2];
  if (components.length === 3) {
    if (category === SUBJECT_METADATA_FILE) {
      requireEntryKind(entry, "file", failures);
      subjectMetadata.set(subject, entry);
    } else if (category === "profiles" || category === "results") {
      requireEntryKind(entry, "directory", failures);
    } else {
      addFailure(failures, "unknown_entry", entry.fullPath, "unknown_subject_entry");
    }
    return;
  }
  if (category !== "profiles" && category !== "results") {
    addFailure(failures, "unknown_entry", entry.fullPath, "unknown_subject_subtree");
    return;
  }
  const kind: ScopeKind = category === "profiles" ? "profile" : "result";
  const scopeValue = components[3];
  const scopeIsValid = typeof scopeValue === "string"
    && (kind === "profile"
      ? PROFILE_SCOPE_PATTERN.test(scopeValue)
      : RESULT_SCOPE_PATTERN.test(scopeValue));
  if (!scopeIsValid) {
    addFailure(failures, "unknown_entry", entry.fullPath, "invalid_scope_directory");
    return;
  }
  const scope = scopeValue as ProfileScope | ResultScope;
  if (components.length === 4) {
    requireEntryKind(entry, "directory", failures);
    scopeDirectories.push({ kind, scope, subjectSha256: subject, entry });
    return;
  }
  if (kind === "profile") classifyProfileArtifact(entry, failures);
  else classifyResultArtifact(entry, failures);
}

function classifyProfileArtifact(
  entry: NormalizedScannerRecord,
  failures: SubjectStorageAuditFailure[],
): void {
  const family = entry.components[4];
  if (family === "active" || family === "receipts" || family === "temporary") {
    if (entry.components.length === 5) requireEntryKind(entry, "directory", failures);
    return;
  }
  if (family === "snapshots") {
    if (entry.components.length === 5) {
      requireEntryKind(entry, "directory", failures);
      return;
    }
    if (entry.components.length === 6
      && (entry.components[5] === "profile.tar.gz.enc" || entry.components[5] === "generation")) {
      requireEntryKind(entry, "file", failures);
      return;
    }
  }
  if (family === "run-checkpoints") {
    if (entry.components.length === 5) {
      requireEntryKind(entry, "directory", failures);
      return;
    }
    if (entry.components.length === 6
      && /^[0-9a-f]{64}\.json\.enc$/.test(entry.components[5]!)) {
      requireEntryKind(entry, "file", failures);
      return;
    }
  }
  addFailure(failures, "unknown_entry", entry.fullPath, "unknown_profile_artifact");
}

function classifyResultArtifact(
  entry: NormalizedScannerRecord,
  failures: SubjectStorageAuditFailure[],
): void {
  const family = entry.components[4];
  if (family === "temporary") {
    if (entry.components.length === 5) requireEntryKind(entry, "directory", failures);
    return;
  }
  if (entry.components.length === 5 && family === "step-result.json.enc") {
    requireEntryKind(entry, "file", failures);
    return;
  }
  addFailure(failures, "unknown_entry", entry.fullPath, "unknown_result_artifact");
}

function classifyOwnerEntry(
  entry: NormalizedScannerRecord,
  failures: SubjectStorageAuditFailure[],
  ownerFiles: OwnerFile[],
): void {
  const components = entry.components;
  if (components.length === 1) {
    requireEntryKind(entry, "directory", failures);
    return;
  }
  const category = components[1];
  if (category !== "profiles" && category !== "results") {
    addFailure(failures, "unknown_entry", entry.fullPath, "unknown_owner_category");
    return;
  }
  if (components.length === 2) {
    requireEntryKind(entry, "directory", failures);
    return;
  }
  const kind: ScopeKind = category === "profiles" ? "profile" : "result";
  const name = components[2];
  const match = typeof name === "string" ? /^([0-9a-f]+)\.json$/.exec(name) : null;
  const scopeValue = match?.[1];
  const scopeIsValid = typeof scopeValue === "string"
    && (kind === "profile"
      ? PROFILE_SCOPE_PATTERN.test(scopeValue)
      : RESULT_SCOPE_PATTERN.test(scopeValue));
  if (components.length !== 3 || !scopeIsValid) {
    addFailure(failures, "unknown_entry", entry.fullPath, "invalid_owner_record_name");
    return;
  }
  requireEntryKind(entry, "file", failures);
  ownerFiles.push({
    kind,
    scope: scopeValue as ProfileScope | ResultScope,
    entry,
  });
}

function requireEntryKind(
  entry: NormalizedScannerRecord,
  kind: "directory" | "file",
  failures: SubjectStorageAuditFailure[],
): void {
  if (entry.kind !== kind) {
    addFailure(failures, "unsafe_special_entry", entry.fullPath, `expected_${kind}`);
  }
}

function requireDirectory(
  entries: ReadonlyMap<string, NormalizedScannerRecord>,
  components: readonly string[],
  failures: SubjectStorageAuditFailure[],
): void {
  const path = components.join("/");
  const entry = entries.get(path);
  if (!entry || entry.kind !== "directory") {
    addFailure(
      failures,
      "missing_required_entry",
      `${ACCOUNT_DATA_V2_DIRECTORY}/${path}`,
      "required_directory",
    );
  }
}

function auditScopeScaffold(
  directory: ScopeDirectory,
  entries: ReadonlyMap<string, NormalizedScannerRecord>,
  failures: SubjectStorageAuditFailure[],
): void {
  const prefix = directory.entry.components;
  if (directory.kind === "profile") {
    for (const family of ["active", "snapshots", "run-checkpoints", "receipts", "temporary"]) {
      requireDirectory(entries, [...prefix, family], failures);
    }
    const snapshotPath = [...prefix, "snapshots", "profile.tar.gz.enc"].join("/");
    const generationPath = [...prefix, "snapshots", "generation"].join("/");
    if (entries.has(snapshotPath) !== entries.has(generationPath)) {
      addFailure(
        failures,
        "incomplete_artifact",
        directory.entry.fullPath,
        "snapshot_generation_pair",
      );
    }
  } else {
    requireDirectory(entries, [...prefix, "temporary"], failures);
  }
}

function parseSubjectMetadataForAudit(
  entry: NormalizedScannerRecord,
  identity: RunnerVolumeIdentityVerifier,
  failures: SubjectStorageAuditFailure[],
): SignedSubjectMetadata | undefined {
  if (entry.kind !== "file" || !entry.controlBytes) {
    addFailure(failures, "corrupt_subject_metadata", entry.fullPath, "missing_control_bytes");
    return undefined;
  }
  try {
    return parseSubjectMetadata(entry.controlBytes, identity);
  } catch {
    addFailure(failures, "corrupt_subject_metadata", entry.fullPath, "invalid_signed_record");
    return undefined;
  }
}

function parseScopeOwnershipForAudit(
  entry: NormalizedScannerRecord,
  identity: RunnerVolumeIdentityVerifier,
  failures: SubjectStorageAuditFailure[],
): SignedScopeOwnership | undefined {
  if (entry.kind !== "file" || !entry.controlBytes) {
    addFailure(failures, "corrupt_scope_ownership", entry.fullPath, "missing_control_bytes");
    return undefined;
  }
  try {
    return parseScopeOwnership(entry.controlBytes, identity);
  } catch {
    addFailure(failures, "corrupt_scope_ownership", entry.fullPath, "invalid_signed_record");
    return undefined;
  }
}

function inventoryEvidence(entries: readonly NormalizedScannerRecord[]): SubjectStorageInventoryEvidence {
  const ordered = [...entries].sort((left, right) => compareCanonicalUtf8(
    left.relativePath,
    right.relativePath,
  ));
  let bytes = 0n;
  const hash = createHash("sha256");
  hash.update("bluey-jobs-runner-subject-storage-inventory-v2\n", "utf8");
  for (const entry of ordered) {
    bytes += entry.fileBytes;
    hash.update([
      `components=${entry.components.map(encodeInventoryComponent).join("/")}`,
      `kind=${entry.kind}`,
      `size_bytes=${entry.sizeBytes}`,
      `sha256=${entry.sha256 ?? "-"}`,
      "",
    ].join("\n"), "utf8");
  }
  return Object.freeze({
    entryCount: ordered.length,
    fileBytes: bytes.toString(),
    sha256: hash.digest("hex"),
  });
}

function emptyInventoryEvidence(): SubjectStorageInventoryEvidence {
  return Object.freeze({
    entryCount: 0,
    fileBytes: "0",
    sha256: EMPTY_SUBJECT_STORAGE_INVENTORY_SHA256,
  });
}

function rejectedAudit(failures: SubjectStorageAuditFailure[]): SubjectStorageAuditRejected {
  const unique = new Map<string, SubjectStorageAuditFailure>();
  for (const failure of failures) {
    unique.set(`${failure.code}\0${failure.path}\0${failure.detail}`, failure);
  }
  const ordered = [...unique.values()].sort((left, right) => (
    compareCanonicalUtf8(left.path, right.path)
      || compareCanonicalUtf8(left.code, right.code)
      || compareCanonicalUtf8(left.detail, right.detail)
  ));
  return Object.freeze({
    ok: false,
    version: SUBJECT_STORAGE_LAYOUT_VERSION,
    failures: Object.freeze(ordered),
  });
}

function addFailure(
  failures: SubjectStorageAuditFailure[],
  code: SubjectStorageAuditFailureCode,
  path: string,
  detail: string,
): void {
  failures.push(Object.freeze({ code, path, detail }));
}

function scopeKey(kind: ScopeKind, scope: string): string {
  return `${kind}:${scope}`;
}

function encodeInventoryComponent(component: string): string {
  return Buffer.from(component, "utf8").toString("base64url");
}

function validPathComponent(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value === "." || value === ".."
    || /[\\/\u0000-\u001f\u007f]/.test(value)) {
    return false;
  }
  const encoded = Buffer.from(value, "utf8");
  return encoded.length <= 255 && encoded.toString("utf8") === value;
}

function validDeviceId(value: unknown): value is string {
  return typeof value === "string" && DEVICE_ID_PATTERN.test(value);
}

function isNativeScannerEntryKind(value: unknown): value is NativeScannerEntryKind {
  return value === "directory" || value === "file" || value === "special" || value === "symlink";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value).sort(compareCanonicalUtf8);
  const wanted = [...expected].sort(compareCanonicalUtf8);
  return keys.length === wanted.length && keys.every((key, index) => key === wanted[index]);
}

function compareCanonicalUtf8(left: string, right: string): number {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}
