import {
  createHash,
  createPublicKey,
  sign,
  type KeyObject,
  verify,
} from "node:crypto";

export const BLUEY_BROWSER_BUILD_AUDIENCE =
  "bluey-jobs-browser-build-v1";
export const BLUEY_BROWSER_RELEASE_AUDIENCE =
  "bluey-jobs-browser-release-manifest-v1";
export const BLUEY_BROWSER_ACTIVATION_AUDIENCE =
  "bluey-jobs-browser-release-activation-v1";
export const BLUEY_BROWSER_ROLLBACK_AUDIENCE =
  "bluey-jobs-browser-release-rollback-v1";
export const BLUEY_BROWSER_REVOCATION_AUDIENCE =
  "bluey-jobs-browser-release-revocation-v1";
export const BLUEY_BROWSER_TRUST_POLICY_AUDIENCE =
  "bluey-jobs-browser-release-trust-policy-v1";
export const BLUEY_BROWSER_SIGNATURE_SET_AUDIENCE =
  "bluey-jobs-browser-release-signature-set-v1";
export const BLUEY_BROWSER_APP_ID = "sh.bluey.jobs.browser";

const SAFE_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const BUILD_ID = /^browser-(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$/;
const SEMVER = /^(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$/;
const HEX_64 = /^[0-9a-f]{64}$/;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/;
const DECIMAL_REVISION = /^(0|[1-9][0-9]{0,12})$/;
const BASE64URL = /^[A-Za-z0-9_-]+$/;
const MAX_SAFE_GENERATION = 9_007_199_254_740_991;
const RESERVED_RELEASE_IDS = new Set([
  "beta",
  "current",
  "download",
  "internal",
  "latest",
  "stable",
]);

export type BrowserReleaseChannel = "internal" | "beta" | "stable";
export type BrowserReleasePlatform = "darwin" | "windows";
export type BrowserReleaseArchitecture = "arm64" | "x64";
export type BrowserReleasePackageKind =
  | "darwin-dmg"
  | "darwin-zip"
  | "windows-nsis";
export type BrowserNativeSignatureKind =
  | "apple-developer-id"
  | "microsoft-authenticode";
export type BrowserRevocationSubjectKind =
  | "artifact"
  | "build-descriptor"
  | "manifest"
  | "release"
  | "signing-key";
export type BrowserReleaseAuthorityRole =
  | "incident"
  | "promotion"
  | "release"
  | "root";
export type BrowserReleaseKeyState = "active" | "retired" | "revoked";
export type BrowserReleaseSignedAudience =
  | typeof BLUEY_BROWSER_ACTIVATION_AUDIENCE
  | typeof BLUEY_BROWSER_RELEASE_AUDIENCE
  | typeof BLUEY_BROWSER_REVOCATION_AUDIENCE
  | typeof BLUEY_BROWSER_ROLLBACK_AUDIENCE
  | typeof BLUEY_BROWSER_TRUST_POLICY_AUDIENCE;

export interface UnsignedBrowserBuildDescriptor {
  readonly version: 1;
  readonly audience: typeof BLUEY_BROWSER_BUILD_AUDIENCE;
  readonly releaseId: string;
  readonly buildId: string;
  readonly appVersion: string;
  readonly appId: string;
  readonly protocolVersion: number;
  readonly sourceCommit: string;
  readonly platform: BrowserReleasePlatform;
  readonly architecture: BrowserReleaseArchitecture;
  readonly electronVersion: string;
  readonly playwrightVersion: string;
  readonly chromiumRevision: string;
  readonly issuedAtMs: number;
  readonly signingKeyId: string;
}

export interface BrowserBuildDescriptor extends UnsignedBrowserBuildDescriptor {
  readonly signature: string;
}

export interface BrowserBuildProof {
  readonly descriptor: string;
  readonly signature: string;
}

export interface VerifiedBrowserBuildProof {
  readonly descriptor: BrowserBuildDescriptor;
  readonly descriptorBytes: Buffer;
  readonly descriptorSha256: string;
  readonly proof: BrowserBuildProof;
}

export interface BrowserReleaseArtifact {
  readonly artifactId: string;
  readonly platform: BrowserReleasePlatform;
  readonly architecture: BrowserReleaseArchitecture;
  readonly packageKind: BrowserReleasePackageKind;
  readonly buildDescriptorSha256: string;
  readonly url: string;
  readonly sizeBytes: number;
  readonly sha256: string;
  readonly appContentSha256: string;
  readonly verificationEvidenceSha256: string;
  readonly nativeSignatureKind: BrowserNativeSignatureKind;
  readonly nativeSignerIdentity: string;
}

export interface UnsignedBrowserReleaseManifest {
  readonly version: 1;
  readonly audience: typeof BLUEY_BROWSER_RELEASE_AUDIENCE;
  readonly manifestId: string;
  readonly manifestGeneration: number;
  readonly releaseId: string;
  readonly releaseSequence: number;
  readonly buildId: string;
  readonly appVersion: string;
  readonly protocolVersion: number;
  readonly sourceCommit: string;
  readonly electronVersion: string;
  readonly playwrightVersion: string;
  readonly chromiumRevision: string;
  readonly publishedAtMs: number;
  readonly releaseNotesUrl: string;
  readonly artifacts: readonly BrowserReleaseArtifact[];
}

export type BrowserReleaseManifest = UnsignedBrowserReleaseManifest;

export interface UnsignedBrowserReleaseActivation {
  readonly version: 1;
  readonly audience: typeof BLUEY_BROWSER_ACTIVATION_AUDIENCE;
  readonly activationId: string;
  readonly activationGeneration: number;
  readonly trustGeneration: number;
  readonly channel: BrowserReleaseChannel;
  readonly channelSequence: number;
  readonly manifestSha256: string;
  readonly signatureSetSha256: string;
  readonly acceptedServerReleaseIds: readonly string[];
  readonly canaryEvidenceSha256: string;
  readonly issuedAtMs: number;
  readonly expiresAtMs: number;
}

export type BrowserReleaseActivation = UnsignedBrowserReleaseActivation;

export interface UnsignedBrowserReleaseRollback {
  readonly version: 1;
  readonly audience: typeof BLUEY_BROWSER_ROLLBACK_AUDIENCE;
  readonly rollbackId: string;
  readonly rollbackGeneration: number;
  readonly trustGeneration: number;
  readonly channel: BrowserReleaseChannel;
  readonly fromActivationSha256: string;
  readonly fromManifestSha256: string;
  readonly toManifestSha256: string;
  readonly toActivationSha256: string;
  readonly canaryEvidenceSha256: string;
  readonly reasonRef: string;
  readonly issuedAtMs: number;
}

export type BrowserReleaseRollback = UnsignedBrowserReleaseRollback;

export interface UnsignedBrowserReleaseRevocation {
  readonly version: 1;
  readonly audience: typeof BLUEY_BROWSER_REVOCATION_AUDIENCE;
  readonly revocationId: string;
  readonly revocationGeneration: number;
  readonly trustGeneration: number;
  readonly subjectKind: BrowserRevocationSubjectKind;
  readonly subjectId: string;
  readonly subjectSha256: string;
  readonly reasonRef: string;
  readonly issuedAtMs: number;
}

export type BrowserReleaseRevocation = UnsignedBrowserReleaseRevocation;

export interface BrowserReleaseTrustRole {
  readonly role: BrowserReleaseAuthorityRole;
  readonly threshold: number;
}

export interface BrowserReleaseTrustKey {
  readonly keyId: string;
  readonly role: BrowserReleaseAuthorityRole;
  readonly publicKey: string;
  readonly state: BrowserReleaseKeyState;
  readonly validFromMs: number;
  readonly validUntilMs: number;
  readonly minimumTrustGeneration: number;
  readonly maximumTrustGeneration: number;
}

export interface BrowserReleaseTrustPolicy {
  readonly version: 1;
  readonly audience: typeof BLUEY_BROWSER_TRUST_POLICY_AUDIENCE;
  readonly policyId: string;
  readonly trustGeneration: number;
  readonly predecessorPolicySha256: string | null;
  readonly artifactOrigin: string;
  readonly issuedAtMs: number;
  readonly validFromMs: number;
  readonly expiresAtMs: number;
  readonly roles: readonly BrowserReleaseTrustRole[];
  readonly keys: readonly BrowserReleaseTrustKey[];
}

export interface UnsignedBrowserReleaseSignatureSet {
  readonly version: 1;
  readonly audience: typeof BLUEY_BROWSER_SIGNATURE_SET_AUDIENCE;
  readonly signatureSetId: string;
  readonly trustGeneration: number;
  readonly role: BrowserReleaseAuthorityRole;
  readonly targetAudience: BrowserReleaseSignedAudience;
  readonly targetSha256: string;
  readonly signedAtMs: number;
}

export interface BrowserReleaseDetachedSignature {
  readonly keyId: string;
  readonly signature: string;
}

export interface BrowserReleaseSignatureSet
  extends UnsignedBrowserReleaseSignatureSet {
  readonly signatures: readonly BrowserReleaseDetachedSignature[];
}

export interface BrowserReleaseSignatureSetSigner {
  readonly keyId: string;
  readonly privateKey: KeyObject;
}

export interface BrowserRootTrustAnchor {
  readonly threshold: number;
  readonly keys: BrowserReleaseVerifyingKeys;
}

export type BrowserReleaseVerifyingKeys = Readonly<Record<string, string>>;

export type BrowserReleaseAuthorityErrorCode =
  | "invalid_descriptor"
  | "invalid_manifest"
  | "invalid_activation"
  | "invalid_rollback"
  | "invalid_revocation"
  | "invalid_trust_policy"
  | "invalid_signature_set"
  | "invalid_signature"
  | "unknown_signing_key"
  | "wrong_signing_role"
  | "signature_threshold_not_met"
  | "key_not_authorized"
  | "trust_rotation_invalid"
  | "authority_expired"
  | "binding_mismatch";

export class BrowserReleaseAuthorityError extends Error {
  constructor(readonly code: BrowserReleaseAuthorityErrorCode) {
    super({
      invalid_descriptor: "The Bluey Browser build descriptor is invalid.",
      invalid_manifest: "The Bluey Browser release manifest is invalid.",
      invalid_activation: "The Bluey Browser release activation is invalid.",
      invalid_rollback: "The Bluey Browser rollback authority is invalid.",
      invalid_revocation: "The Bluey Browser revocation authority is invalid.",
      invalid_trust_policy: "The Bluey Browser trust policy is invalid.",
      invalid_signature_set: "The Bluey Browser signature set is invalid.",
      invalid_signature: "The Bluey Browser release signature is invalid.",
      unknown_signing_key: "The Bluey Browser release signing key is unknown.",
      wrong_signing_role: "The Bluey Browser release signing role is invalid.",
      signature_threshold_not_met: "The Bluey Browser signature threshold was not met.",
      key_not_authorized: "The Bluey Browser signing key is not authorized.",
      trust_rotation_invalid: "The Bluey Browser trust-policy rotation is invalid.",
      authority_expired: "The Bluey Browser release authority is outside its validity window.",
      binding_mismatch: "The Bluey Browser release bindings do not match.",
    }[code]);
    this.name = "BrowserReleaseAuthorityError";
  }
}

export function createBrowserBuildDescriptor(
  input: UnsignedBrowserBuildDescriptor,
  privateKey: KeyObject,
): BrowserBuildDescriptor {
  const unsigned = parseUnsignedBuildDescriptor(input);
  assertEd25519PrivateKey(privateKey, "invalid_descriptor");
  return Object.freeze({
    ...unsigned,
    signature: sign(
      null,
      canonicalBrowserBuildDescriptorBytes(unsigned),
      privateKey,
    ).toString("base64url"),
  });
}

export function parseBrowserBuildDescriptor(
  input: unknown,
): BrowserBuildDescriptor {
  const record = requireRecord(input, "invalid_descriptor");
  assertExactKeys(
    record,
    [
      "appId",
      "appVersion",
      "architecture",
      "audience",
      "buildId",
      "chromiumRevision",
      "electronVersion",
      "issuedAtMs",
      "platform",
      "playwrightVersion",
      "protocolVersion",
      "releaseId",
      "signature",
      "signingKeyId",
      "sourceCommit",
      "version",
    ],
    "invalid_descriptor",
  );
  const unsigned = parseUnsignedBuildDescriptor(record);
  return Object.freeze({
    ...unsigned,
    signature: requireBase64Url(record.signature, 64, "invalid_descriptor"),
  });
}

export function verifyBrowserBuildDescriptor(
  input: unknown,
  keys: BrowserReleaseVerifyingKeys,
): BrowserBuildDescriptor {
  const descriptor = parseBrowserBuildDescriptor(input);
  verifyReleaseSignature(
    descriptor.signingKeyId,
    keys,
    canonicalBrowserBuildDescriptorBytes(descriptor),
    descriptor.signature,
  );
  return descriptor;
}

export function canonicalBrowserBuildDescriptorBytes(
  input: UnsignedBrowserBuildDescriptor,
): Buffer {
  const descriptor = parseUnsignedBuildDescriptor(input);
  return Buffer.from(
    [
      `version=${descriptor.version}`,
      `audience=${descriptor.audience}`,
      `release_id=${descriptor.releaseId}`,
      `build_id=${descriptor.buildId}`,
      `app_version=${descriptor.appVersion}`,
      `app_id=${descriptor.appId}`,
      `protocol_version=${descriptor.protocolVersion}`,
      `source_commit=${descriptor.sourceCommit}`,
      `platform=${descriptor.platform}`,
      `architecture=${descriptor.architecture}`,
      `electron_version=${descriptor.electronVersion}`,
      `playwright_version=${descriptor.playwrightVersion}`,
      `chromium_revision=${descriptor.chromiumRevision}`,
      `issued_at_ms=${descriptor.issuedAtMs}`,
      `signing_key_id=${descriptor.signingKeyId}`,
      "",
    ].join("\n"),
    "utf8",
  );
}

export function parseCanonicalBrowserBuildDescriptorBytes(
  input: Uint8Array,
): UnsignedBrowserBuildDescriptor {
  const bytes = Buffer.from(input);
  if (bytes.length < 1 || bytes.length > 4_096) {
    throw new BrowserReleaseAuthorityError("invalid_descriptor");
  }
  let text: string;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new BrowserReleaseAuthorityError("invalid_descriptor");
  }
  const lines = text.split("\n");
  const expectedPrefixes = [
    "version=",
    "audience=",
    "release_id=",
    "build_id=",
    "app_version=",
    "app_id=",
    "protocol_version=",
    "source_commit=",
    "platform=",
    "architecture=",
    "electron_version=",
    "playwright_version=",
    "chromium_revision=",
    "issued_at_ms=",
    "signing_key_id=",
  ] as const;
  if (
    lines.length !== expectedPrefixes.length + 1 ||
    lines[expectedPrefixes.length] !== ""
  ) {
    throw new BrowserReleaseAuthorityError("invalid_descriptor");
  }
  const values = expectedPrefixes.map((prefix, index) => {
    const line = lines[index]!;
    if (!line.startsWith(prefix)) {
      throw new BrowserReleaseAuthorityError("invalid_descriptor");
    }
    return line.slice(prefix.length);
  });
  const descriptor = parseUnsignedBuildDescriptor({
    version: Number(values[0]),
    audience: values[1],
    releaseId: values[2],
    buildId: values[3],
    appVersion: values[4],
    appId: values[5],
    protocolVersion: Number(values[6]),
    sourceCommit: values[7],
    platform: values[8],
    architecture: values[9],
    electronVersion: values[10],
    playwrightVersion: values[11],
    chromiumRevision: values[12],
    issuedAtMs: Number(values[13]),
    signingKeyId: values[14],
  });
  if (!canonicalBrowserBuildDescriptorBytes(descriptor).equals(bytes)) {
    throw new BrowserReleaseAuthorityError("invalid_descriptor");
  }
  return descriptor;
}

export function browserBuildProof(
  input: BrowserBuildDescriptor,
): BrowserBuildProof {
  const descriptor = parseBrowserBuildDescriptor(input);
  return Object.freeze({
    descriptor: canonicalBrowserBuildDescriptorBytes(descriptor).toString(
      "base64url",
    ),
    signature: descriptor.signature,
  });
}

export function verifyBrowserBuildProof(
  input: unknown,
  keys: BrowserReleaseVerifyingKeys,
): VerifiedBrowserBuildProof {
  const value = requireRecord(input, "invalid_descriptor");
  assertExactKeys(
    value,
    ["descriptor", "signature"],
    "invalid_descriptor",
  );
  const encodedDescriptor = requireString(
    value.descriptor,
    1,
    8_192,
    "invalid_descriptor",
  );
  const descriptorBytes = decodeBoundedBase64Url(
    encodedDescriptor,
    4_096,
    "invalid_descriptor",
  );
  const signature = requireBase64Url(
    value.signature,
    64,
    "invalid_descriptor",
  );
  const unsigned = parseCanonicalBrowserBuildDescriptorBytes(descriptorBytes);
  verifyReleaseSignature(
    unsigned.signingKeyId,
    keys,
    descriptorBytes,
    signature,
  );
  const descriptor = Object.freeze({ ...unsigned, signature });
  const proof = Object.freeze({ descriptor: encodedDescriptor, signature });
  return Object.freeze({
    descriptor,
    descriptorBytes,
    descriptorSha256: browserBuildDescriptorSha256(descriptor),
    proof,
  });
}

export function browserBuildDescriptorSha256(
  input: BrowserBuildDescriptor,
): string {
  const descriptor = parseBrowserBuildDescriptor(input);
  return createHash("sha256")
    .update(canonicalBrowserBuildDescriptorBytes(descriptor))
    .update(`signature=${descriptor.signature}\n`, "utf8")
    .digest("hex");
}

export function createBrowserReleaseManifest(
  input: UnsignedBrowserReleaseManifest,
): BrowserReleaseManifest {
  return parseBrowserReleaseManifest(input);
}

export function parseBrowserReleaseManifest(
  input: unknown,
): BrowserReleaseManifest {
  const record = requireRecord(input, "invalid_manifest");
  assertExactKeys(
    record,
    [
      "artifacts",
      "appVersion",
      "audience",
      "buildId",
      "chromiumRevision",
      "electronVersion",
      "manifestGeneration",
      "manifestId",
      "playwrightVersion",
      "protocolVersion",
      "publishedAtMs",
      "releaseId",
      "releaseNotesUrl",
      "releaseSequence",
      "sourceCommit",
      "version",
    ],
    "invalid_manifest",
  );
  return parseUnsignedReleaseManifest(record);
}

export function verifyBrowserReleaseManifest(
  input: Uint8Array,
  signatureSetInput: Uint8Array,
  policyInput: Uint8Array,
  verificationTimeMs: number,
): BrowserReleaseManifest {
  const manifest = parseCanonicalBrowserReleaseManifestBytes(input);
  verifyAuthoritySignatureSet(
    input,
    signatureSetInput,
    policyInput,
    "release",
    BLUEY_BROWSER_RELEASE_AUDIENCE,
    manifest.publishedAtMs,
    undefined,
    verificationTimeMs,
  );
  assertBrowserReleaseManifestArtifactOrigin(
    manifest,
    parseCanonicalBrowserReleaseTrustPolicyBytes(policyInput),
  );
  return manifest;
}

export function assertBrowserReleaseManifestArtifactOrigin(
  manifestInput: unknown,
  policyInput: unknown,
): void {
  const manifest = parseBrowserReleaseManifest(manifestInput);
  const policy = parseBrowserReleaseTrustPolicy(policyInput);
  for (const artifact of manifest.artifacts) {
    if (new URL(artifact.url).origin !== policy.artifactOrigin) {
      throw new BrowserReleaseAuthorityError("binding_mismatch");
    }
  }
}

export function canonicalBrowserReleaseManifestBytes(
  input: UnsignedBrowserReleaseManifest,
): Buffer {
  const manifest = parseBrowserReleaseManifest(input);
  return canonicalJsonBytes(manifest);
}

export function parseCanonicalBrowserReleaseManifestBytes(
  input: Uint8Array,
): BrowserReleaseManifest {
  return parseCanonicalJsonBytes(
    input,
    32 * 1024,
    "invalid_manifest",
    parseBrowserReleaseManifest,
    canonicalBrowserReleaseManifestBytes,
  );
}

export function browserReleaseManifestSha256(
  input: BrowserReleaseManifest,
): string {
  const manifest = parseBrowserReleaseManifest(input);
  return createHash("sha256")
    .update(canonicalBrowserReleaseManifestBytes(manifest))
    .digest("hex");
}

export function createBrowserReleaseActivation(
  input: UnsignedBrowserReleaseActivation,
): BrowserReleaseActivation {
  return parseBrowserReleaseActivation(input);
}

export function parseBrowserReleaseActivation(
  input: unknown,
): BrowserReleaseActivation {
  const record = requireRecord(input, "invalid_activation");
  assertExactKeys(
    record,
    [
      "acceptedServerReleaseIds",
      "activationGeneration",
      "activationId",
      "audience",
      "canaryEvidenceSha256",
      "channel",
      "channelSequence",
      "expiresAtMs",
      "issuedAtMs",
      "manifestSha256",
      "signatureSetSha256",
      "trustGeneration",
      "version",
    ],
    "invalid_activation",
  );
  return parseUnsignedActivation(record);
}

export function verifyBrowserReleaseActivation(
  input: Uint8Array,
  signatureSetInput: Uint8Array,
  policyInput: Uint8Array,
  verificationTimeMs: number,
): BrowserReleaseActivation {
  const activation = parseCanonicalBrowserReleaseActivationBytes(input);
  verifyAuthoritySignatureSet(
    input,
    signatureSetInput,
    policyInput,
    "promotion",
    BLUEY_BROWSER_ACTIVATION_AUDIENCE,
    activation.issuedAtMs,
    activation.trustGeneration,
    verificationTimeMs,
  );
  return activation;
}

export function canonicalBrowserReleaseActivationBytes(
  input: UnsignedBrowserReleaseActivation,
): Buffer {
  const activation = parseBrowserReleaseActivation(input);
  return canonicalJsonBytes(activation);
}

export function parseCanonicalBrowserReleaseActivationBytes(
  input: Uint8Array,
): BrowserReleaseActivation {
  return parseCanonicalJsonBytes(
    input,
    16 * 1024,
    "invalid_activation",
    parseBrowserReleaseActivation,
    canonicalBrowserReleaseActivationBytes,
  );
}

export function browserReleaseActivationSha256(
  input: BrowserReleaseActivation,
): string {
  const activation = parseBrowserReleaseActivation(input);
  return createHash("sha256")
    .update(canonicalBrowserReleaseActivationBytes(activation))
    .digest("hex");
}

export function createBrowserReleaseRollback(
  input: UnsignedBrowserReleaseRollback,
): BrowserReleaseRollback {
  return parseBrowserReleaseRollback(input);
}

export function parseBrowserReleaseRollback(
  input: unknown,
): BrowserReleaseRollback {
  const record = requireRecord(input, "invalid_rollback");
  assertExactKeys(
    record,
    [
      "audience",
      "canaryEvidenceSha256",
      "channel",
      "fromActivationSha256",
      "fromManifestSha256",
      "issuedAtMs",
      "reasonRef",
      "rollbackGeneration",
      "rollbackId",
      "toManifestSha256",
      "toActivationSha256",
      "trustGeneration",
      "version",
    ],
    "invalid_rollback",
  );
  return parseUnsignedRollback(record);
}

export function verifyBrowserReleaseRollback(
  input: Uint8Array,
  signatureSetInput: Uint8Array,
  policyInput: Uint8Array,
  verificationTimeMs: number,
): BrowserReleaseRollback {
  const rollback = parseCanonicalBrowserReleaseRollbackBytes(input);
  verifyAuthoritySignatureSet(
    input,
    signatureSetInput,
    policyInput,
    "promotion",
    BLUEY_BROWSER_ROLLBACK_AUDIENCE,
    rollback.issuedAtMs,
    rollback.trustGeneration,
    verificationTimeMs,
  );
  return rollback;
}

export function canonicalBrowserReleaseRollbackBytes(
  input: UnsignedBrowserReleaseRollback,
): Buffer {
  const rollback = parseBrowserReleaseRollback(input);
  return canonicalJsonBytes(rollback);
}

export function parseCanonicalBrowserReleaseRollbackBytes(
  input: Uint8Array,
): BrowserReleaseRollback {
  return parseCanonicalJsonBytes(
    input,
    8 * 1024,
    "invalid_rollback",
    parseBrowserReleaseRollback,
    canonicalBrowserReleaseRollbackBytes,
  );
}

export function browserReleaseRollbackSha256(
  input: BrowserReleaseRollback,
): string {
  const rollback = parseBrowserReleaseRollback(input);
  return createHash("sha256")
    .update(canonicalBrowserReleaseRollbackBytes(rollback))
    .digest("hex");
}

export function createBrowserReleaseRevocation(
  input: UnsignedBrowserReleaseRevocation,
): BrowserReleaseRevocation {
  return parseBrowserReleaseRevocation(input);
}

export function parseBrowserReleaseRevocation(
  input: unknown,
): BrowserReleaseRevocation {
  const record = requireRecord(input, "invalid_revocation");
  assertExactKeys(
    record,
    [
      "audience",
      "issuedAtMs",
      "reasonRef",
      "revocationGeneration",
      "revocationId",
      "subjectId",
      "subjectKind",
      "subjectSha256",
      "trustGeneration",
      "version",
    ],
    "invalid_revocation",
  );
  return parseUnsignedRevocation(record);
}

export function verifyBrowserReleaseRevocation(
  input: Uint8Array,
  signatureSetInput: Uint8Array,
  policyInput: Uint8Array,
  verificationTimeMs: number,
): BrowserReleaseRevocation {
  const revocation = parseCanonicalBrowserReleaseRevocationBytes(input);
  const policy = parseCanonicalBrowserReleaseTrustPolicyBytes(policyInput);
  verifyAuthoritySignatureSet(
    input,
    signatureSetInput,
    policyInput,
    "incident",
    BLUEY_BROWSER_REVOCATION_AUDIENCE,
    revocation.issuedAtMs,
    revocation.trustGeneration,
    verificationTimeMs,
  );
  assertRevocableSigningKey(revocation, policy);
  return revocation;
}

export function canonicalBrowserReleaseRevocationBytes(
  input: UnsignedBrowserReleaseRevocation,
): Buffer {
  const revocation = parseBrowserReleaseRevocation(input);
  return canonicalJsonBytes(revocation);
}

export function parseCanonicalBrowserReleaseRevocationBytes(
  input: Uint8Array,
): BrowserReleaseRevocation {
  return parseCanonicalJsonBytes(
    input,
    8 * 1024,
    "invalid_revocation",
    parseBrowserReleaseRevocation,
    canonicalBrowserReleaseRevocationBytes,
  );
}

export function browserReleaseRevocationSha256(
  input: BrowserReleaseRevocation,
): string {
  const revocation = parseBrowserReleaseRevocation(input);
  return createHash("sha256")
    .update(canonicalBrowserReleaseRevocationBytes(revocation))
    .digest("hex");
}

export function createBrowserReleaseTrustPolicy(
  input: BrowserReleaseTrustPolicy,
): BrowserReleaseTrustPolicy {
  return parseBrowserReleaseTrustPolicy(input);
}

export function parseBrowserReleaseTrustPolicy(
  input: unknown,
): BrowserReleaseTrustPolicy {
  const value = requireRecord(input, "invalid_trust_policy");
  assertExactKeys(
    value,
    [
      "audience",
      "artifactOrigin",
      "expiresAtMs",
      "issuedAtMs",
      "keys",
      "policyId",
      "predecessorPolicySha256",
      "roles",
      "trustGeneration",
      "validFromMs",
      "version",
    ],
    "invalid_trust_policy",
  );
  const trustGeneration = requirePositiveInteger(
    value.trustGeneration,
    "invalid_trust_policy",
  );
  const issuedAtMs = requireNonnegativeInteger(
    value.issuedAtMs,
    "invalid_trust_policy",
  );
  const validFromMs = requireNonnegativeInteger(
    value.validFromMs,
    "invalid_trust_policy",
  );
  const expiresAtMs = requireNonnegativeInteger(
    value.expiresAtMs,
    "invalid_trust_policy",
  );
  const roles = parseTrustRoles(value.roles);
  const keys = parseTrustKeys(value.keys);
  const policy = Object.freeze({
    version: requireLiteral(value.version, 1, "invalid_trust_policy"),
    audience: requireLiteral(
      value.audience,
      BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
      "invalid_trust_policy",
    ),
    policyId: requireSafeId(value.policyId, "invalid_trust_policy"),
    trustGeneration,
    predecessorPolicySha256:
      value.predecessorPolicySha256 === null
        ? null
        : requirePattern(
            value.predecessorPolicySha256,
            HEX_64,
            "invalid_trust_policy",
          ),
    artifactOrigin: requireArtifactOrigin(value.artifactOrigin),
    issuedAtMs,
    validFromMs,
    expiresAtMs,
    roles,
    keys,
  });
  if (
    validFromMs > issuedAtMs ||
    issuedAtMs >= expiresAtMs ||
    (trustGeneration === 1) !== (policy.predecessorPolicySha256 === null)
  ) {
    throw new BrowserReleaseAuthorityError("invalid_trust_policy");
  }
  assertCurrentRoleCoverage(policy);
  return policy;
}

export function canonicalBrowserReleaseTrustPolicyBytes(
  input: BrowserReleaseTrustPolicy,
): Buffer {
  return canonicalJsonBytes(parseBrowserReleaseTrustPolicy(input));
}

export function parseCanonicalBrowserReleaseTrustPolicyBytes(
  input: Uint8Array,
): BrowserReleaseTrustPolicy {
  return parseCanonicalJsonBytes(
    input,
    64 * 1024,
    "invalid_trust_policy",
    parseBrowserReleaseTrustPolicy,
    canonicalBrowserReleaseTrustPolicyBytes,
  );
}

export function browserReleaseTrustPolicySha256(
  input: BrowserReleaseTrustPolicy,
): string {
  return sha256(canonicalBrowserReleaseTrustPolicyBytes(input));
}

export function createBrowserReleaseSignatureSet(
  input: UnsignedBrowserReleaseSignatureSet,
  signers: readonly BrowserReleaseSignatureSetSigner[],
): BrowserReleaseSignatureSet {
  const unsigned = parseUnsignedSignatureSet(input);
  if (signers.length < 1 || signers.length > 32) {
    throw new BrowserReleaseAuthorityError("invalid_signature_set");
  }
  const signerIds = signers.map((signer) =>
    requireSafeId(signer.keyId, "invalid_signature_set"),
  );
  requireStrictlySortedUnique(signerIds, "invalid_signature_set");
  const message = canonicalBrowserReleaseSignaturePayloadBytes(unsigned);
  const signatures = signers.map((signer) => {
    assertEd25519PrivateKey(signer.privateKey, "invalid_signature_set");
    return Object.freeze({
      keyId: signer.keyId,
      signature: sign(null, message, signer.privateKey).toString("base64url"),
    });
  });
  return Object.freeze({ ...unsigned, signatures: Object.freeze(signatures) });
}

export function parseBrowserReleaseSignatureSet(
  input: unknown,
): BrowserReleaseSignatureSet {
  const value = requireRecord(input, "invalid_signature_set");
  assertExactKeys(
    value,
    [
      "audience",
      "role",
      "signatureSetId",
      "signatures",
      "signedAtMs",
      "targetAudience",
      "targetSha256",
      "trustGeneration",
      "version",
    ],
    "invalid_signature_set",
  );
  const unsigned = parseUnsignedSignatureSet({
    version: value.version,
    audience: value.audience,
    signatureSetId: value.signatureSetId,
    trustGeneration: value.trustGeneration,
    role: value.role,
    targetAudience: value.targetAudience,
    targetSha256: value.targetSha256,
    signedAtMs: value.signedAtMs,
  });
  if (
    !Array.isArray(value.signatures) ||
    value.signatures.length < 1 ||
    value.signatures.length > 32
  ) {
    throw new BrowserReleaseAuthorityError("invalid_signature_set");
  }
  const signatures = value.signatures.map((inputSignature) => {
    const signature = requireRecord(inputSignature, "invalid_signature_set");
    assertExactKeys(
      signature,
      ["keyId", "signature"],
      "invalid_signature_set",
    );
    return Object.freeze({
      keyId: requireSafeId(signature.keyId, "invalid_signature_set"),
      signature: requireBase64Url(
        signature.signature,
        64,
        "invalid_signature_set",
      ),
    });
  });
  requireStrictlySortedUnique(
    signatures.map((signature) => signature.keyId),
    "invalid_signature_set",
  );
  return Object.freeze({ ...unsigned, signatures: Object.freeze(signatures) });
}

export function canonicalBrowserReleaseSignaturePayloadBytes(
  input: UnsignedBrowserReleaseSignatureSet,
): Buffer {
  return canonicalJsonBytes(parseUnsignedSignatureSet(input));
}

export function canonicalBrowserReleaseSignatureSetBytes(
  input: BrowserReleaseSignatureSet,
): Buffer {
  return canonicalJsonBytes(parseBrowserReleaseSignatureSet(input));
}

export function parseCanonicalBrowserReleaseSignatureSetBytes(
  input: Uint8Array,
): BrowserReleaseSignatureSet {
  return parseCanonicalJsonBytes(
    input,
    32 * 1024,
    "invalid_signature_set",
    parseBrowserReleaseSignatureSet,
    canonicalBrowserReleaseSignatureSetBytes,
  );
}

export function browserReleaseSignatureSetSha256(
  input: BrowserReleaseSignatureSet,
): string {
  return sha256(canonicalBrowserReleaseSignatureSetBytes(input));
}

export function browserReleaseAuthorityTargetSha256(input: Uint8Array): string {
  return sha256(Buffer.from(input));
}

export function verifyBrowserReleaseTrustPolicy(
  input: Uint8Array,
  signatureSetInput: Uint8Array,
  predecessorInput: Uint8Array | null,
  bootstrap: BrowserRootTrustAnchor | null,
  verificationTimeMs: number,
): BrowserReleaseTrustPolicy {
  const policy = parseCanonicalBrowserReleaseTrustPolicyBytes(input);
  const signatureSet = parseCanonicalBrowserReleaseSignatureSetBytes(
    signatureSetInput,
  );
  requirePolicyCurrent(policy, verificationTimeMs);
  if (
    policy.issuedAtMs > verificationTimeMs ||
    signatureSet.signedAtMs > verificationTimeMs
  ) {
    throw new BrowserReleaseAuthorityError("authority_expired");
  }
  requireSignatureSetBinding(
    signatureSet,
    "root",
    BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
    browserReleaseAuthorityTargetSha256(input),
    policy.trustGeneration,
    policy.issuedAtMs,
  );

  const predecessor = predecessorInput
    ? parseCanonicalBrowserReleaseTrustPolicyBytes(predecessorInput)
    : null;
  if (policy.trustGeneration === 1) {
    if (predecessor || !bootstrap || policy.predecessorPolicySha256 !== null) {
      throw new BrowserReleaseAuthorityError("trust_rotation_invalid");
    }
  } else if (
    !predecessor ||
    policy.trustGeneration !== predecessor.trustGeneration + 1 ||
    policy.predecessorPolicySha256 !==
      browserReleaseTrustPolicySha256(predecessor) ||
    policy.issuedAtMs <= predecessor.issuedAtMs ||
    policy.issuedAtMs < predecessor.validFromMs ||
    policy.issuedAtMs >= predecessor.expiresAtMs
  ) {
    throw new BrowserReleaseAuthorityError("trust_rotation_invalid");
  }

  if (predecessor) assertTrustPolicyTransition(predecessor, policy);
  verifyRootRotationSignatures(signatureSet, policy, predecessor, bootstrap);
  return policy;
}

export function requireBrowserDescriptorInManifest(
  descriptorInput: unknown,
  manifestInput: unknown,
): BrowserReleaseArtifact {
  const descriptor = parseBrowserBuildDescriptor(descriptorInput);
  const manifest = parseBrowserReleaseManifest(manifestInput);
  const shared =
    descriptor.releaseId === manifest.releaseId &&
    descriptor.buildId === manifest.buildId &&
    descriptor.appVersion === manifest.appVersion &&
    descriptor.protocolVersion === manifest.protocolVersion &&
    descriptor.sourceCommit === manifest.sourceCommit &&
    descriptor.electronVersion === manifest.electronVersion &&
    descriptor.playwrightVersion === manifest.playwrightVersion &&
    descriptor.chromiumRevision === manifest.chromiumRevision;
  const descriptorSha256 = browserBuildDescriptorSha256(descriptor);
  const matches = manifest.artifacts.filter(
    (artifact) =>
      artifact.platform === descriptor.platform &&
      artifact.architecture === descriptor.architecture &&
      artifact.buildDescriptorSha256 === descriptorSha256,
  );
  if (!shared || matches.length === 0) {
    throw new BrowserReleaseAuthorityError("binding_mismatch");
  }
  return matches[0]!;
}

function parseUnsignedBuildDescriptor(
  input: unknown,
): UnsignedBrowserBuildDescriptor {
  const value = requireRecord(input, "invalid_descriptor");
  const descriptor: UnsignedBrowserBuildDescriptor = {
    version: requireLiteral(value.version, 1, "invalid_descriptor"),
    audience: requireLiteral(
      value.audience,
      BLUEY_BROWSER_BUILD_AUDIENCE,
      "invalid_descriptor",
    ),
    releaseId: requireReleaseId(value.releaseId, "invalid_descriptor"),
    buildId: requirePattern(value.buildId, BUILD_ID, "invalid_descriptor"),
    appVersion: requirePattern(value.appVersion, SEMVER, "invalid_descriptor"),
    appId: requireLiteral(
      value.appId,
      BLUEY_BROWSER_APP_ID,
      "invalid_descriptor",
    ),
    protocolVersion: requirePositiveInteger(
      value.protocolVersion,
      "invalid_descriptor",
    ),
    sourceCommit: requirePattern(
      value.sourceCommit,
      SOURCE_COMMIT,
      "invalid_descriptor",
    ),
    platform: requirePlatform(value.platform, "invalid_descriptor"),
    architecture: requireArchitecture(value.architecture, "invalid_descriptor"),
    electronVersion: requirePattern(
      value.electronVersion,
      SEMVER,
      "invalid_descriptor",
    ),
    playwrightVersion: requirePattern(
      value.playwrightVersion,
      SEMVER,
      "invalid_descriptor",
    ),
    chromiumRevision: requirePattern(
      value.chromiumRevision,
      DECIMAL_REVISION,
      "invalid_descriptor",
    ),
    issuedAtMs: requireNonnegativeInteger(value.issuedAtMs, "invalid_descriptor"),
    signingKeyId: requireSafeId(value.signingKeyId, "invalid_descriptor"),
  };
  assertPlatformArchitecture(
    descriptor.platform,
    descriptor.architecture,
    "invalid_descriptor",
  );
  return Object.freeze(descriptor);
}

function parseUnsignedReleaseManifest(
  input: unknown,
): UnsignedBrowserReleaseManifest {
  const value = requireRecord(input, "invalid_manifest");
  if (
    !Array.isArray(value.artifacts) ||
    value.artifacts.length !== 5
  ) {
    throw new BrowserReleaseAuthorityError("invalid_manifest");
  }
  const artifacts = value.artifacts.map(parseArtifact);
  const targetIdentities = artifacts.map(artifactTargetIdentity);
  const expectedTargets = [
    "darwin:arm64:darwin-dmg",
    "darwin:arm64:darwin-zip",
    "darwin:x64:darwin-dmg",
    "darwin:x64:darwin-zip",
    "windows:x64:windows-nsis",
  ];
  const descriptorByTarget = new Map<string, string>();
  const appContentByTarget = new Map<string, string>();
  for (const artifact of artifacts) {
    const target = `${artifact.platform}:${artifact.architecture}`;
    const existingDescriptor = descriptorByTarget.get(target);
    const existingAppContent = appContentByTarget.get(target);
    if (
      (existingDescriptor &&
        existingDescriptor !== artifact.buildDescriptorSha256) ||
      (existingAppContent &&
        existingAppContent !== artifact.appContentSha256)
    ) {
      throw new BrowserReleaseAuthorityError("invalid_manifest");
    }
    descriptorByTarget.set(target, artifact.buildDescriptorSha256);
    appContentByTarget.set(target, artifact.appContentSha256);
  }
  if (
    targetIdentities.some((identity, index) => identity !== expectedTargets[index]) ||
    new Set(artifacts.map((artifact) => artifact.artifactId)).size !== 5 ||
    new Set(artifacts.map((artifact) => artifact.url)).size !== 5 ||
    new Set(artifacts.map((artifact) => artifact.sha256)).size !== 5 ||
    descriptorByTarget.size !== 3 ||
    appContentByTarget.size !== 3 ||
    new Set(descriptorByTarget.values()).size !== 3
  ) {
    throw new BrowserReleaseAuthorityError("invalid_manifest");
  }
  const manifest: UnsignedBrowserReleaseManifest = {
    version: requireLiteral(value.version, 1, "invalid_manifest"),
    audience: requireLiteral(
      value.audience,
      BLUEY_BROWSER_RELEASE_AUDIENCE,
      "invalid_manifest",
    ),
    manifestId: requireSafeId(value.manifestId, "invalid_manifest"),
    manifestGeneration: requirePositiveInteger(
      value.manifestGeneration,
      "invalid_manifest",
    ),
    releaseId: requireReleaseId(value.releaseId, "invalid_manifest"),
    releaseSequence: requirePositiveInteger(
      value.releaseSequence,
      "invalid_manifest",
    ),
    buildId: requirePattern(value.buildId, BUILD_ID, "invalid_manifest"),
    appVersion: requirePattern(value.appVersion, SEMVER, "invalid_manifest"),
    protocolVersion: requirePositiveInteger(
      value.protocolVersion,
      "invalid_manifest",
    ),
    sourceCommit: requirePattern(
      value.sourceCommit,
      SOURCE_COMMIT,
      "invalid_manifest",
    ),
    electronVersion: requirePattern(
      value.electronVersion,
      SEMVER,
      "invalid_manifest",
    ),
    playwrightVersion: requirePattern(
      value.playwrightVersion,
      SEMVER,
      "invalid_manifest",
    ),
    chromiumRevision: requirePattern(
      value.chromiumRevision,
      DECIMAL_REVISION,
      "invalid_manifest",
    ),
    publishedAtMs: requireNonnegativeInteger(
      value.publishedAtMs,
      "invalid_manifest",
    ),
    releaseNotesUrl: requireImmutableReleaseUrl(
      value.releaseNotesUrl,
      String(value.releaseId ?? ""),
      "invalid_manifest",
    ),
    artifacts: Object.freeze(artifacts),
  };
  for (const artifact of artifacts) {
    requireImmutableArtifactUrl(
      artifact.url,
      manifest.releaseId,
      "invalid_manifest",
    );
  }
  return Object.freeze(manifest);
}

function parseArtifact(input: unknown): BrowserReleaseArtifact {
  const value = requireRecord(input, "invalid_manifest");
  assertExactKeys(
    value,
    [
      "architecture",
      "appContentSha256",
      "artifactId",
      "buildDescriptorSha256",
      "nativeSignatureKind",
      "nativeSignerIdentity",
      "packageKind",
      "platform",
      "sha256",
      "sizeBytes",
      "url",
      "verificationEvidenceSha256",
    ],
    "invalid_manifest",
  );
  const artifact: BrowserReleaseArtifact = {
    artifactId: requireSafeId(value.artifactId, "invalid_manifest"),
    platform: requirePlatform(value.platform, "invalid_manifest"),
    architecture: requireArchitecture(value.architecture, "invalid_manifest"),
    packageKind: requirePackageKind(value.packageKind, "invalid_manifest"),
    buildDescriptorSha256: requirePattern(
      value.buildDescriptorSha256,
      HEX_64,
      "invalid_manifest",
    ),
    url: requireString(value.url, 1, 2_048, "invalid_manifest"),
    sizeBytes: requirePositiveInteger(value.sizeBytes, "invalid_manifest"),
    sha256: requirePattern(value.sha256, HEX_64, "invalid_manifest"),
    appContentSha256: requirePattern(
      value.appContentSha256,
      HEX_64,
      "invalid_manifest",
    ),
    verificationEvidenceSha256: requirePattern(
      value.verificationEvidenceSha256,
      HEX_64,
      "invalid_manifest",
    ),
    nativeSignatureKind: requireNativeSignatureKind(
      value.nativeSignatureKind,
      "invalid_manifest",
    ),
    nativeSignerIdentity: requireString(
      value.nativeSignerIdentity,
      3,
      256,
      "invalid_manifest",
    ),
  };
  assertPlatformArchitecture(
    artifact.platform,
    artifact.architecture,
    "invalid_manifest",
  );
  assertPackagePlatform(artifact, "invalid_manifest");
  return Object.freeze(artifact);
}

function parseUnsignedActivation(
  input: unknown,
): UnsignedBrowserReleaseActivation {
  const value = requireRecord(input, "invalid_activation");
  const acceptedServerReleaseIds = requireSortedSafeIds(
    value.acceptedServerReleaseIds,
    1,
    32,
    "invalid_activation",
  );
  const activation: UnsignedBrowserReleaseActivation = {
    version: requireLiteral(value.version, 1, "invalid_activation"),
    audience: requireLiteral(
      value.audience,
      BLUEY_BROWSER_ACTIVATION_AUDIENCE,
      "invalid_activation",
    ),
    activationId: requireSafeId(value.activationId, "invalid_activation"),
    activationGeneration: requirePositiveInteger(
      value.activationGeneration,
      "invalid_activation",
    ),
    trustGeneration: requirePositiveInteger(
      value.trustGeneration,
      "invalid_activation",
    ),
    channel: requireChannel(value.channel, "invalid_activation"),
    channelSequence: requirePositiveInteger(
      value.channelSequence,
      "invalid_activation",
    ),
    manifestSha256: requirePattern(
      value.manifestSha256,
      HEX_64,
      "invalid_activation",
    ),
    signatureSetSha256: requirePattern(
      value.signatureSetSha256,
      HEX_64,
      "invalid_activation",
    ),
    acceptedServerReleaseIds,
    canaryEvidenceSha256: requirePattern(
      value.canaryEvidenceSha256,
      HEX_64,
      "invalid_activation",
    ),
    issuedAtMs: requireNonnegativeInteger(
      value.issuedAtMs,
      "invalid_activation",
    ),
    expiresAtMs: requireNonnegativeInteger(
      value.expiresAtMs,
      "invalid_activation",
    ),
  };
  if (activation.expiresAtMs <= activation.issuedAtMs) {
    throw new BrowserReleaseAuthorityError("invalid_activation");
  }
  return Object.freeze(activation);
}

function parseUnsignedRollback(input: unknown): UnsignedBrowserReleaseRollback {
  const value = requireRecord(input, "invalid_rollback");
  const rollback: UnsignedBrowserReleaseRollback = {
    version: requireLiteral(value.version, 1, "invalid_rollback"),
    audience: requireLiteral(
      value.audience,
      BLUEY_BROWSER_ROLLBACK_AUDIENCE,
      "invalid_rollback",
    ),
    rollbackId: requireSafeId(value.rollbackId, "invalid_rollback"),
    rollbackGeneration: requirePositiveInteger(
      value.rollbackGeneration,
      "invalid_rollback",
    ),
    trustGeneration: requirePositiveInteger(
      value.trustGeneration,
      "invalid_rollback",
    ),
    channel: requireChannel(value.channel, "invalid_rollback"),
    fromActivationSha256: requirePattern(
      value.fromActivationSha256,
      HEX_64,
      "invalid_rollback",
    ),
    fromManifestSha256: requirePattern(
      value.fromManifestSha256,
      HEX_64,
      "invalid_rollback",
    ),
    toManifestSha256: requirePattern(
      value.toManifestSha256,
      HEX_64,
      "invalid_rollback",
    ),
    toActivationSha256: requirePattern(
      value.toActivationSha256,
      HEX_64,
      "invalid_rollback",
    ),
    canaryEvidenceSha256: requirePattern(
      value.canaryEvidenceSha256,
      HEX_64,
      "invalid_rollback",
    ),
    reasonRef: requireSafeId(value.reasonRef, "invalid_rollback"),
    issuedAtMs: requireNonnegativeInteger(value.issuedAtMs, "invalid_rollback"),
  };
  if (
    rollback.fromManifestSha256 === rollback.toManifestSha256 ||
    rollback.fromActivationSha256 === rollback.toActivationSha256
  ) {
    throw new BrowserReleaseAuthorityError("invalid_rollback");
  }
  return Object.freeze(rollback);
}

function parseUnsignedRevocation(
  input: unknown,
): UnsignedBrowserReleaseRevocation {
  const value = requireRecord(input, "invalid_revocation");
  return Object.freeze({
    version: requireLiteral(value.version, 1, "invalid_revocation"),
    audience: requireLiteral(
      value.audience,
      BLUEY_BROWSER_REVOCATION_AUDIENCE,
      "invalid_revocation",
    ),
    revocationId: requireSafeId(value.revocationId, "invalid_revocation"),
    revocationGeneration: requirePositiveInteger(
      value.revocationGeneration,
      "invalid_revocation",
    ),
    trustGeneration: requirePositiveInteger(
      value.trustGeneration,
      "invalid_revocation",
    ),
    subjectKind: requireRevocationSubjectKind(
      value.subjectKind,
      "invalid_revocation",
    ),
    subjectId: requireSafeId(value.subjectId, "invalid_revocation"),
    subjectSha256: requirePattern(
      value.subjectSha256,
      HEX_64,
      "invalid_revocation",
    ),
    reasonRef: requireSafeId(value.reasonRef, "invalid_revocation"),
    issuedAtMs: requireNonnegativeInteger(
      value.issuedAtMs,
      "invalid_revocation",
    ),
  });
}

function parseTrustRoles(input: unknown): readonly BrowserReleaseTrustRole[] {
  if (!Array.isArray(input) || input.length !== 4) {
    throw new BrowserReleaseAuthorityError("invalid_trust_policy");
  }
  const roles = input.map((roleInput) => {
    const value = requireRecord(roleInput, "invalid_trust_policy");
    assertExactKeys(value, ["role", "threshold"], "invalid_trust_policy");
    return Object.freeze({
      role: requireAuthorityRole(value.role, "invalid_trust_policy"),
      threshold: requirePositiveInteger(
        value.threshold,
        "invalid_trust_policy",
      ),
    });
  });
  const roleNames = roles.map((role) => role.role);
  requireStrictlySortedUnique(roleNames, "invalid_trust_policy");
  if (roleNames.join(",") !== "incident,promotion,release,root") {
    throw new BrowserReleaseAuthorityError("invalid_trust_policy");
  }
  return Object.freeze(roles);
}

function parseTrustKeys(input: unknown): readonly BrowserReleaseTrustKey[] {
  if (!Array.isArray(input) || input.length < 4 || input.length > 64) {
    throw new BrowserReleaseAuthorityError("invalid_trust_policy");
  }
  const keys = input.map((keyInput) => {
    const value = requireRecord(keyInput, "invalid_trust_policy");
    assertExactKeys(
      value,
      [
        "keyId",
        "maximumTrustGeneration",
        "minimumTrustGeneration",
        "publicKey",
        "role",
        "state",
        "validFromMs",
        "validUntilMs",
      ],
      "invalid_trust_policy",
    );
    const validFromMs = requireNonnegativeInteger(
      value.validFromMs,
      "invalid_trust_policy",
    );
    const validUntilMs = requireNonnegativeInteger(
      value.validUntilMs,
      "invalid_trust_policy",
    );
    const minimumTrustGeneration = requirePositiveInteger(
      value.minimumTrustGeneration,
      "invalid_trust_policy",
    );
    const maximumTrustGeneration = requirePositiveInteger(
      value.maximumTrustGeneration,
      "invalid_trust_policy",
    );
    if (
      validUntilMs < validFromMs ||
      maximumTrustGeneration < minimumTrustGeneration
    ) {
      throw new BrowserReleaseAuthorityError("invalid_trust_policy");
    }
    return Object.freeze({
      keyId: requireSafeId(value.keyId, "invalid_trust_policy"),
      role: requireAuthorityRole(value.role, "invalid_trust_policy"),
      publicKey: requireBase64Url(
        value.publicKey,
        32,
        "invalid_trust_policy",
      ),
      state: requireKeyState(value.state, "invalid_trust_policy"),
      validFromMs,
      validUntilMs,
      minimumTrustGeneration,
      maximumTrustGeneration,
    });
  });
  requireStrictlySortedUnique(
    keys.map((key) => key.keyId),
    "invalid_trust_policy",
  );
  return Object.freeze(keys);
}

function assertCurrentRoleCoverage(policy: BrowserReleaseTrustPolicy): void {
  for (const role of policy.roles) {
    const active = policy.keys.filter(
      (key) =>
        key.role === role.role &&
        key.state === "active" &&
        keyAuthorizes(key, policy.trustGeneration, policy.issuedAtMs),
    ).length;
    if (active < role.threshold) {
      throw new BrowserReleaseAuthorityError("invalid_trust_policy");
    }
  }
}

function parseUnsignedSignatureSet(
  input: unknown,
): UnsignedBrowserReleaseSignatureSet {
  const value = requireRecord(input, "invalid_signature_set");
  assertExactKeys(
    value,
    [
      "audience",
      "role",
      "signatureSetId",
      "signedAtMs",
      "targetAudience",
      "targetSha256",
      "trustGeneration",
      "version",
    ],
    "invalid_signature_set",
  );
  return Object.freeze({
    version: requireLiteral(value.version, 1, "invalid_signature_set"),
    audience: requireLiteral(
      value.audience,
      BLUEY_BROWSER_SIGNATURE_SET_AUDIENCE,
      "invalid_signature_set",
    ),
    signatureSetId: requireSafeId(
      value.signatureSetId,
      "invalid_signature_set",
    ),
    trustGeneration: requirePositiveInteger(
      value.trustGeneration,
      "invalid_signature_set",
    ),
    role: requireAuthorityRole(value.role, "invalid_signature_set"),
    targetAudience: requireSignedAudience(
      value.targetAudience,
      "invalid_signature_set",
    ),
    targetSha256: requirePattern(
      value.targetSha256,
      HEX_64,
      "invalid_signature_set",
    ),
    signedAtMs: requireNonnegativeInteger(
      value.signedAtMs,
      "invalid_signature_set",
    ),
  });
}

function signatureSetPayload(
  input: BrowserReleaseSignatureSet,
): UnsignedBrowserReleaseSignatureSet {
  return {
    version: input.version,
    audience: input.audience,
    signatureSetId: input.signatureSetId,
    trustGeneration: input.trustGeneration,
    role: input.role,
    targetAudience: input.targetAudience,
    targetSha256: input.targetSha256,
    signedAtMs: input.signedAtMs,
  };
}

function verifyAuthoritySignatureSet(
  targetInput: Uint8Array,
  signatureSetInput: Uint8Array,
  policyInput: Uint8Array,
  role: BrowserReleaseAuthorityRole,
  targetAudience: BrowserReleaseSignedAudience,
  targetIssuedAtMs: number,
  targetTrustGeneration: number | undefined,
  verificationTimeMs: number,
): void {
  const policy = parseCanonicalBrowserReleaseTrustPolicyBytes(policyInput);
  const signatureSet = parseCanonicalBrowserReleaseSignatureSetBytes(
    signatureSetInput,
  );
  requirePolicyCurrent(policy, verificationTimeMs);
  if (
    targetIssuedAtMs > verificationTimeMs ||
    signatureSet.signedAtMs > verificationTimeMs
  ) {
    throw new BrowserReleaseAuthorityError("authority_expired");
  }
  if (
    targetTrustGeneration !== undefined &&
    (targetTrustGeneration !== policy.trustGeneration ||
      signatureSet.trustGeneration !== targetTrustGeneration)
  ) {
    throw new BrowserReleaseAuthorityError("key_not_authorized");
  }
  if (
    targetTrustGeneration === undefined &&
    signatureSet.trustGeneration > policy.trustGeneration
  ) {
    throw new BrowserReleaseAuthorityError("key_not_authorized");
  }
  requireSignatureSetBinding(
    signatureSet,
    role,
    targetAudience,
    browserReleaseAuthorityTargetSha256(targetInput),
    signatureSet.trustGeneration,
    targetIssuedAtMs,
  );

  const threshold = requireRolePolicy(policy, role).threshold;
  const keys = new Map(policy.keys.map((key) => [key.keyId, key]));
  const message = canonicalBrowserReleaseSignaturePayloadBytes(
    signatureSetPayload(signatureSet),
  );
  let authorized = 0;
  for (const detached of signatureSet.signatures) {
    const key = keys.get(detached.keyId);
    if (!key) throw new BrowserReleaseAuthorityError("unknown_signing_key");
    if (key.role !== role) {
      throw new BrowserReleaseAuthorityError("wrong_signing_role");
    }
    if (
      key.state !== "active" ||
      !keyAuthorizes(key, signatureSet.trustGeneration, signatureSet.signedAtMs)
    ) {
      throw new BrowserReleaseAuthorityError("key_not_authorized");
    }
    verifyEncodedEd25519Signature(key.publicKey, message, detached.signature);
    authorized += 1;
  }
  if (authorized < threshold) {
    throw new BrowserReleaseAuthorityError("signature_threshold_not_met");
  }
}

function requireSignatureSetBinding(
  signatureSet: BrowserReleaseSignatureSet,
  role: BrowserReleaseAuthorityRole,
  targetAudience: BrowserReleaseSignedAudience,
  targetSha256: string,
  trustGeneration: number,
  signedAtMs: number,
): void {
  if (
    signatureSet.role !== role ||
    signatureSet.targetAudience !== targetAudience ||
    signatureSet.targetSha256 !== targetSha256 ||
    signatureSet.trustGeneration !== trustGeneration ||
    signatureSet.signedAtMs !== signedAtMs
  ) {
    throw new BrowserReleaseAuthorityError("invalid_signature_set");
  }
}

function requirePolicyCurrent(
  policy: BrowserReleaseTrustPolicy,
  verificationTimeMs: number,
): void {
  const time = requireNonnegativeInteger(
    verificationTimeMs,
    "invalid_trust_policy",
  );
  if (time < policy.validFromMs || time >= policy.expiresAtMs) {
    throw new BrowserReleaseAuthorityError("authority_expired");
  }
}

function assertRevocableSigningKey(
  revocation: BrowserReleaseRevocation,
  policy: BrowserReleaseTrustPolicy,
): void {
  if (revocation.subjectKind !== "signing-key") return;
  const subject = policy.keys.find((key) => key.keyId === revocation.subjectId);
  const subjectDigest = subject
    ? browserReleaseAuthorityTargetSha256(
        Buffer.from(subject.publicKey, "base64url"),
      )
    : null;
  const targetsRootMaterial = policy.keys.some(
    (key) =>
      key.role === "root" &&
      (key.keyId === revocation.subjectId ||
        browserReleaseAuthorityTargetSha256(
          Buffer.from(key.publicKey, "base64url"),
        ) === revocation.subjectSha256),
  );
  if (
    !subject ||
    subjectDigest !== revocation.subjectSha256 ||
    targetsRootMaterial
  ) {
    throw new BrowserReleaseAuthorityError("invalid_revocation");
  }
}

function requireRolePolicy(
  policy: BrowserReleaseTrustPolicy,
  role: BrowserReleaseAuthorityRole,
): BrowserReleaseTrustRole {
  const found = policy.roles.find((candidate) => candidate.role === role);
  if (!found) throw new BrowserReleaseAuthorityError("wrong_signing_role");
  return found;
}

function keyAuthorizes(
  key: BrowserReleaseTrustKey,
  trustGeneration: number,
  signedAtMs: number,
): boolean {
  return trustGeneration >= key.minimumTrustGeneration &&
    trustGeneration <= key.maximumTrustGeneration &&
    signedAtMs >= key.validFromMs &&
    signedAtMs <= key.validUntilMs;
}

function assertTrustPolicyTransition(
  predecessor: BrowserReleaseTrustPolicy,
  successor: BrowserReleaseTrustPolicy,
): void {
  const successorKeys = new Map(successor.keys.map((key) => [key.keyId, key]));
  for (const previous of predecessor.keys) {
    const next = successorKeys.get(previous.keyId);
    if (
      !next ||
      next.publicKey !== previous.publicKey ||
      next.role !== previous.role ||
      next.validFromMs !== previous.validFromMs ||
      next.validUntilMs > previous.validUntilMs ||
      next.minimumTrustGeneration !== previous.minimumTrustGeneration ||
      next.maximumTrustGeneration > previous.maximumTrustGeneration ||
      !validKeyStateTransition(previous.state, next.state)
    ) {
      throw new BrowserReleaseAuthorityError("trust_rotation_invalid");
    }
  }
}

function validKeyStateTransition(
  previous: BrowserReleaseKeyState,
  next: BrowserReleaseKeyState,
): boolean {
  if (previous === "revoked") return next === "revoked";
  if (previous === "retired") return next !== "active";
  return true;
}

function verifyRootRotationSignatures(
  signatureSet: BrowserReleaseSignatureSet,
  successor: BrowserReleaseTrustPolicy,
  predecessor: BrowserReleaseTrustPolicy | null,
  bootstrap: BrowserRootTrustAnchor | null,
): void {
  const successorRoots = eligibleRootKeys(
    successor,
    signatureSet.trustGeneration,
    signatureSet.signedAtMs,
  );
  const predecessorRoots = predecessor
    ? eligibleRootKeys(
        predecessor,
        signatureSet.trustGeneration,
        signatureSet.signedAtMs,
      )
    : validateBootstrap(bootstrap);
  const predecessorThreshold = predecessor
    ? requireRolePolicy(predecessor, "root").threshold
    : bootstrap!.threshold;
  const successorThreshold = requireRolePolicy(successor, "root").threshold;
  for (const [keyId, predecessorKey] of predecessorRoots) {
    const successorKey = successorRoots.get(keyId);
    if (successorKey && successorKey !== predecessorKey) {
      throw new BrowserReleaseAuthorityError("trust_rotation_invalid");
    }
  }
  const allowed = new Map<string, string>([
    ...predecessorRoots,
    ...successorRoots,
  ]);
  const message = canonicalBrowserReleaseSignaturePayloadBytes(
    signatureSetPayload(signatureSet),
  );
  const signedKeyIds = new Set<string>();
  for (const detached of signatureSet.signatures) {
    const publicKey = allowed.get(detached.keyId);
    if (!publicKey) {
      throw new BrowserReleaseAuthorityError("key_not_authorized");
    }
    verifyEncodedEd25519Signature(publicKey, message, detached.signature);
    signedKeyIds.add(detached.keyId);
  }
  const predecessorCount = [...predecessorRoots.keys()].filter((keyId) =>
    signedKeyIds.has(keyId),
  ).length;
  const successorCount = [...successorRoots.keys()].filter((keyId) =>
    signedKeyIds.has(keyId),
  ).length;
  if (
    predecessorCount < predecessorThreshold ||
    successorCount < successorThreshold
  ) {
    throw new BrowserReleaseAuthorityError("signature_threshold_not_met");
  }
}

function eligibleRootKeys(
  policy: BrowserReleaseTrustPolicy,
  trustGeneration: number,
  signedAtMs: number,
): Map<string, string> {
  return new Map(
    policy.keys
      .filter(
        (key) =>
          key.role === "root" &&
          key.state === "active" &&
          keyAuthorizes(key, trustGeneration, signedAtMs),
      )
      .map((key) => [key.keyId, key.publicKey]),
  );
}

function validateBootstrap(
  bootstrap: BrowserRootTrustAnchor | null,
): Map<string, string> {
  if (!bootstrap) {
    throw new BrowserReleaseAuthorityError("trust_rotation_invalid");
  }
  const threshold = requirePositiveInteger(
    bootstrap.threshold,
    "invalid_trust_policy",
  );
  if (
    !bootstrap.keys ||
    typeof bootstrap.keys !== "object" ||
    Array.isArray(bootstrap.keys)
  ) {
    throw new BrowserReleaseAuthorityError("invalid_trust_policy");
  }
  const entries = Object.entries(bootstrap.keys);
  if (entries.length < threshold || entries.length > 32) {
    throw new BrowserReleaseAuthorityError("invalid_trust_policy");
  }
  return new Map(
    entries.map(([keyId, publicKey]) => [
      requireSafeId(keyId, "invalid_trust_policy"),
      requireBase64Url(publicKey, 32, "invalid_trust_policy"),
    ]),
  );
}

function verifyEncodedEd25519Signature(
  encodedKey: string,
  message: Uint8Array,
  signature: string,
): void {
  const publicKeyRaw = decodeBase64Url(encodedKey, 32, "invalid_signature");
  const signatureBytes = decodeBase64Url(signature, 64, "invalid_signature");
  try {
    const publicKey = createPublicKey({
      format: "jwk",
      key: {
        crv: "Ed25519",
        kty: "OKP",
        x: publicKeyRaw.toString("base64url"),
      },
    });
    if (!verify(null, message, publicKey, signatureBytes)) {
      throw new BrowserReleaseAuthorityError("invalid_signature");
    }
  } catch (error) {
    if (error instanceof BrowserReleaseAuthorityError) throw error;
    throw new BrowserReleaseAuthorityError("invalid_signature");
  }
}

function verifyReleaseSignature(
  signingKeyId: string,
  keys: BrowserReleaseVerifyingKeys,
  message: Uint8Array,
  signature: string,
): void {
  const encodedKey = Object.prototype.hasOwnProperty.call(keys, signingKeyId)
    ? keys[signingKeyId]
    : undefined;
  if (typeof encodedKey !== "string") {
    throw new BrowserReleaseAuthorityError("unknown_signing_key");
  }
  verifyEncodedEd25519Signature(encodedKey, message, signature);
}

function assertEd25519PrivateKey(
  privateKey: KeyObject,
  code: BrowserReleaseAuthorityErrorCode,
): void {
  if (privateKey.type !== "private" || privateKey.asymmetricKeyType !== "ed25519") {
    throw new BrowserReleaseAuthorityError(code);
  }
}

function requireRecord(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): Record<string, unknown> {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input as Record<string, unknown>;
}

function assertExactKeys(
  record: Record<string, unknown>,
  expected: readonly string[],
  code: BrowserReleaseAuthorityErrorCode,
): void {
  const actual = Object.keys(record).sort();
  const sortedExpected = [...expected].sort();
  if (
    actual.length !== sortedExpected.length ||
    actual.some((key, index) => key !== sortedExpected[index])
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
}

function requireLiteral<T extends string | number>(
  input: unknown,
  expected: T,
  code: BrowserReleaseAuthorityErrorCode,
): T {
  if (input !== expected) throw new BrowserReleaseAuthorityError(code);
  return expected;
}

function requireString(
  input: unknown,
  minimum: number,
  maximum: number,
  code: BrowserReleaseAuthorityErrorCode,
): string {
  if (typeof input !== "string") throw new BrowserReleaseAuthorityError(code);
  const bytes = Buffer.byteLength(input, "utf8");
  if (
    bytes < minimum ||
    bytes > maximum ||
    input.trim() !== input ||
    /[\u0000-\u001f\u007f]/.test(input)
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requireSafeId(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): string {
  const value = requireString(input, 3, 128, code);
  if (!SAFE_ID.test(value)) throw new BrowserReleaseAuthorityError(code);
  return value;
}

function requireReleaseId(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): string {
  const value = requireSafeId(input, code);
  if (RESERVED_RELEASE_IDS.has(value.toLowerCase())) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return value;
}

function requirePattern(
  input: unknown,
  pattern: RegExp,
  code: BrowserReleaseAuthorityErrorCode,
): string {
  const value = requireString(input, 1, 128, code);
  if (!pattern.test(value)) throw new BrowserReleaseAuthorityError(code);
  return value;
}

function requireSortedSafeIds(
  input: unknown,
  minimum: number,
  maximum: number,
  code: BrowserReleaseAuthorityErrorCode,
): readonly string[] {
  if (!Array.isArray(input) || input.length < minimum || input.length > maximum) {
    throw new BrowserReleaseAuthorityError(code);
  }
  const values = input.map((value) => requireSafeId(value, code));
  if (
    new Set(values).size !== values.length ||
    values.some((value, index) => index > 0 && values[index - 1]! >= value)
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return Object.freeze(values);
}

function requireNonnegativeInteger(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): number {
  if (
    typeof input !== "number" ||
    !Number.isSafeInteger(input) ||
    input < 0 ||
    input > MAX_SAFE_GENERATION
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requirePositiveInteger(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): number {
  const value = requireNonnegativeInteger(input, code);
  if (value < 1) throw new BrowserReleaseAuthorityError(code);
  return value;
}

function requireBase64Url(
  input: unknown,
  expectedBytes: number,
  code: BrowserReleaseAuthorityErrorCode,
): string {
  const value = requireString(input, 1, 256, code);
  decodeBase64Url(value, expectedBytes, code);
  return value;
}

function decodeBase64Url(
  value: string,
  expectedBytes: number,
  code: BrowserReleaseAuthorityErrorCode,
): Buffer {
  if (!BASE64URL.test(value)) throw new BrowserReleaseAuthorityError(code);
  const bytes = Buffer.from(value, "base64url");
  if (bytes.length !== expectedBytes || bytes.toString("base64url") !== value) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return bytes;
}

function decodeBoundedBase64Url(
  value: string,
  maximumBytes: number,
  code: BrowserReleaseAuthorityErrorCode,
): Buffer {
  if (!BASE64URL.test(value)) throw new BrowserReleaseAuthorityError(code);
  const bytes = Buffer.from(value, "base64url");
  if (
    bytes.length < 1 ||
    bytes.length > maximumBytes ||
    bytes.toString("base64url") !== value
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return bytes;
}

function requireChannel(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserReleaseChannel {
  if (input !== "internal" && input !== "beta" && input !== "stable") {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requireAuthorityRole(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserReleaseAuthorityRole {
  if (
    input !== "incident" &&
    input !== "promotion" &&
    input !== "release" &&
    input !== "root"
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requireKeyState(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserReleaseKeyState {
  if (input !== "active" && input !== "retired" && input !== "revoked") {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requireSignedAudience(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserReleaseSignedAudience {
  if (
    input !== BLUEY_BROWSER_ACTIVATION_AUDIENCE &&
    input !== BLUEY_BROWSER_RELEASE_AUDIENCE &&
    input !== BLUEY_BROWSER_REVOCATION_AUDIENCE &&
    input !== BLUEY_BROWSER_ROLLBACK_AUDIENCE &&
    input !== BLUEY_BROWSER_TRUST_POLICY_AUDIENCE
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requirePlatform(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserReleasePlatform {
  if (input !== "darwin" && input !== "windows") {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requireArchitecture(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserReleaseArchitecture {
  if (input !== "arm64" && input !== "x64") {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requireNativeSignatureKind(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserNativeSignatureKind {
  if (
    input !== "apple-developer-id" &&
    input !== "microsoft-authenticode"
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requireRevocationSubjectKind(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserRevocationSubjectKind {
  if (
    input !== "artifact" &&
    input !== "build-descriptor" &&
    input !== "manifest" &&
    input !== "release" &&
    input !== "signing-key"
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function requirePackageKind(
  input: unknown,
  code: BrowserReleaseAuthorityErrorCode,
): BrowserReleasePackageKind {
  if (
    input !== "darwin-dmg" &&
    input !== "darwin-zip" &&
    input !== "windows-nsis"
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return input;
}

function assertPlatformArchitecture(
  platform: BrowserReleasePlatform,
  architecture: BrowserReleaseArchitecture,
  code: BrowserReleaseAuthorityErrorCode,
): void {
  if (platform === "windows" && architecture !== "x64") {
    throw new BrowserReleaseAuthorityError(code);
  }
}

function assertPackagePlatform(
  artifact: BrowserReleaseArtifact,
  code: BrowserReleaseAuthorityErrorCode,
): void {
  const expectedExtension = artifact.packageKind === "darwin-dmg"
    ? ".dmg"
    : artifact.packageKind === "darwin-zip"
      ? ".zip"
      : ".exe";
  if (
    (artifact.packageKind.startsWith("darwin-") && artifact.platform !== "darwin") ||
    (artifact.packageKind === "windows-nsis" && artifact.platform !== "windows") ||
    !artifact.url.endsWith(expectedExtension) ||
    (artifact.platform === "darwin" &&
      artifact.nativeSignatureKind !== "apple-developer-id") ||
    (artifact.platform === "windows" &&
      artifact.nativeSignatureKind !== "microsoft-authenticode")
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
}

function requireImmutableReleaseUrl(
  input: unknown,
  releaseId: string,
  code: BrowserReleaseAuthorityErrorCode,
): string {
  const value = requireString(input, 1, 2_048, code);
  try {
    const url = new URL(value);
    const segments = url.pathname.split("/");
    if (
      url.protocol !== "https:" ||
      url.username ||
      url.password ||
      url.search ||
      url.hash ||
      url.toString() !== value ||
      segments.length < 6 ||
      segments[0] !== "" ||
      segments[1] !== "jobs" ||
      segments[2] !== "browser" ||
      segments[3] !== "releases" ||
      segments[4] !== releaseId ||
      segments.slice(5).some((segment) =>
        !segment || segment === "." || segment === ".." || segment.includes("%")
      )
    ) {
      throw new BrowserReleaseAuthorityError(code);
    }
  } catch (error) {
    if (error instanceof BrowserReleaseAuthorityError) throw error;
    throw new BrowserReleaseAuthorityError(code);
  }
  return value;
}

function requireImmutableArtifactUrl(
  input: unknown,
  releaseId: string,
  code: BrowserReleaseAuthorityErrorCode,
): string {
  const value = requireImmutableReleaseUrl(input, releaseId, code);
  if (new URL(value).pathname.split("/").length !== 6) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return value;
}

function requireArtifactOrigin(input: unknown): string {
  const value = requireString(input, 1, 2_048, "invalid_trust_policy");
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      !url.hostname ||
      url.username ||
      url.password ||
      url.port ||
      url.pathname !== "/" ||
      url.search ||
      url.hash ||
      url.origin !== value
    ) {
      throw new BrowserReleaseAuthorityError("invalid_trust_policy");
    }
  } catch (error) {
    if (error instanceof BrowserReleaseAuthorityError) throw error;
    throw new BrowserReleaseAuthorityError("invalid_trust_policy");
  }
  return value;
}

function requireStrictlySortedUnique(
  values: readonly string[],
  code: BrowserReleaseAuthorityErrorCode,
): void {
  if (
    new Set(values).size !== values.length ||
    values.some((value, index) => index > 0 && values[index - 1]! >= value)
  ) {
    throw new BrowserReleaseAuthorityError(code);
  }
}

function canonicalJsonBytes(input: unknown): Buffer {
  return Buffer.from(`${JSON.stringify(input)}\n`, "utf8");
}

function parseCanonicalJsonBytes<T>(
  input: Uint8Array,
  maximumBytes: number,
  code: BrowserReleaseAuthorityErrorCode,
  parser: (value: unknown) => T,
  canonicalizer: (value: T) => Buffer,
): T {
  const bytes = Buffer.from(input);
  if (bytes.length < 1 || bytes.length > maximumBytes || bytes.includes(0)) {
    throw new BrowserReleaseAuthorityError(code);
  }
  let decoded: unknown;
  try {
    decoded = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new BrowserReleaseAuthorityError(code);
  }
  const parsed = parser(decoded);
  if (!canonicalizer(parsed).equals(bytes)) {
    throw new BrowserReleaseAuthorityError(code);
  }
  return parsed;
}

function sha256(input: Uint8Array): string {
  return createHash("sha256").update(input).digest("hex");
}

function artifactTargetIdentity(artifact: BrowserReleaseArtifact): string {
  return `${artifact.platform}:${artifact.architecture}:${artifact.packageKind}`;
}
