import type { ApplicationPacket } from "./contracts.js";
import type {
  ApplicationReceiptBundle,
  AtsCertifiedReceiptAuthority,
  CertifiedAutoSubmitAdmission,
  ReceiptPacketSnapshot,
} from "./receipts.js";
import { isSuccessfulExactSubmitHttpStatus } from "./trusted-submit.js";

const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const CERTIFIED_RECEIPT_KEYS = [
  "accountId",
  "adapter",
  "adapterVersion",
  "applicationId",
  "applicationIdentityId",
  "atsCertifiedReceiptAuthority",
  "browserProfileId",
  "documents",
  "events",
  "evidenceObjects",
  "finalUrl",
  "generatedAt",
  "job",
  "packet",
  "receiptId",
  "receiptObject",
  "result",
  "runId",
  "runner",
  "schemaVersion",
  "screenshotKeys",
] as const;
const CERTIFIED_PACKET_KEYS = [
  "answers",
  "applicationEmail",
  "approvedExecutionAdmission",
  "approvedExecutionSchemaVersion",
  "approvedPacketChecksum",
  "jobId",
  "resumeVersionId",
  "verifiedClaimIds",
] as const;
const CERTIFIED_AUTHORITY_KEYS = [
  "accountId",
  "activationGeneration",
  "activationSha256",
  "adapter",
  "adapterBundleSha256",
  "adapterVersion",
  "applicationAttemptId",
  "applicationId",
  "bindingConsumedAtMs",
  "bindingFence",
  "bindingSha256",
  "canaryReservationSha256",
  "layoutObservationSha256",
  "layoutSetSha256",
  "manifestSha256",
  "meteringReservationSha256",
  "phaseBRequestId",
  "provider",
  "rolloutChannel",
  "runId",
  "runnerKind",
  "runnerTargetSha256",
  "schemaVersion",
  "observedSurfaceSha256",
  "targetKeySha256",
] as const;

export function assertRunnablePacket(packet: ApplicationPacket): void {
  const missing = [
    ["applicationId", packet.applicationId],
    ["jobId", packet.jobId],
    ["resumeVersionId", packet.resumeVersionId],
    ["approvedPacketChecksum", packet.approvedPacketChecksum],
    ["applicationIdentityId", packet.applicationIdentityId],
    ["applicationEmail", packet.applicationEmail],
    ["browserProfileId", packet.browserProfileId],
  ]
    .filter(([, value]) => !String(value ?? "").trim())
    .map(([field]) => field);

  if (missing.length) {
    throw new Error(`Application packet is missing ${missing.join(", ")}`);
  }
}

export function assertSubmissionReceiptComplete(receipt: ApplicationReceiptBundle): void {
  assertReceiptSchemaAuthority(receipt);
  const missing = [
    ["receiptId", receipt.receiptId],
    ["accountId", receipt.accountId],
    ["applicationId", receipt.applicationId],
    ["runId", receipt.runId],
    ["applicationIdentityId", receipt.applicationIdentityId],
    ["browserProfileId", receipt.browserProfileId],
    ["adapter", receipt.adapter],
    ["adapterVersion", receipt.adapterVersion],
    ["packet.applicationEmail", receipt.packet.applicationEmail],
    ["packet.resumeVersionId", receipt.packet.resumeVersionId],
    ["packet.approvedPacketChecksum", receipt.packet.approvedPacketChecksum],
  ]
    .filter(([, value]) => !String(value ?? "").trim())
    .map(([field]) => field);
  const resume = receipt.documents.find((document) => document.kind === "resume");
  if (!resume) missing.push("resume document");
  if (resume && !resume.storageKey.trim()) missing.push("resume storage key");
  if (resume && !/^[a-f0-9]{64}$/i.test(resume.sha256)) missing.push("resume checksum");
  for (const document of receipt.documents) {
    if (!document.storageKey.trim()) missing.push(`${document.kind} storage key`);
    if (!/^[a-f0-9]{64}$/i.test(document.sha256)) missing.push(`${document.kind} checksum`);
  }
  if (resume && resume.versionId && resume.versionId !== receipt.packet.resumeVersionId) {
    throw new Error("Submission receipt resume document does not match the packet resume version");
  }
  if (receipt.result.status === "submitted") {
    const hasConfirmation = Boolean(receipt.result.confirmationText?.trim() || receipt.result.confirmationUrl?.trim());
    if (!hasConfirmation) missing.push("submission confirmation");
    const exactSubmitAdapter = receipt.adapter === "greenhouse" || receipt.adapter === "lever";
    if (exactSubmitAdapter
      && !isSuccessfulExactSubmitHttpStatus(receipt.result.submitHttpStatus)) {
      missing.push("successful submit HTTP status");
    } else if (receipt.result.submitHttpStatus !== undefined
      && !isSuccessfulExactSubmitHttpStatus(receipt.result.submitHttpStatus)) {
      missing.push("valid submit HTTP status");
    }
    if (!receipt.screenshotKeys.length) missing.push("confirmation screenshot");
  }
  if (receipt.receiptObject
    && receipt.receiptObject.schemaVersion !== receipt.schemaVersion) {
    throw new Error("Submission receipt object schema does not match the receipt");
  }
  if (missing.length) {
    throw new Error(`Submission receipt is missing ${missing.join(", ")}`);
  }
}

function assertReceiptSchemaAuthority(receipt: ApplicationReceiptBundle): void {
  const admission = certifiedAdmission(receipt.packet);
  const hasAuthority = Object.hasOwn(receipt, "atsCertifiedReceiptAuthority");
  if (!admission) {
    if (receipt.schemaVersion !== 1) {
      throw new Error("Review submission receipt must use schema version 1");
    }
    if (hasAuthority) {
      throw new Error("Review submission receipt cannot carry certified authority");
    }
    return;
  }

  if (receipt.schemaVersion !== 2) {
    throw new Error("Certified submission receipt must use schema version 2");
  }
  if (!hasOnlyKeys(receipt as unknown as Record<string, unknown>, CERTIFIED_RECEIPT_KEYS)) {
    throw new Error("Certified submission receipt schema is invalid");
  }
  if (!hasExactKeys(
    receipt.packet as unknown as Record<string, unknown>,
    CERTIFIED_PACKET_KEYS,
  )) {
    throw new Error("Certified submission receipt packet schema is invalid");
  }
  const receiptDigests = [
    receipt.packet.approvedPacketChecksum,
    ...receipt.documents.map((document) => document.sha256),
    ...(receipt.evidenceObjects ?? []).map((evidence) => evidence.sha256),
    ...(receipt.receiptObject ? [receipt.receiptObject.sha256] : []),
  ];
  if (!receiptDigests.every(validSha256)) {
    throw new Error("Certified submission receipt digest is invalid");
  }
  const authority = receipt.atsCertifiedReceiptAuthority;
  if (!hasAuthority || !authority) {
    throw new Error("Certified submission receipt authority is missing");
  }
  assertCertifiedReceiptAuthority(receipt, admission, authority);
}

function certifiedAdmission(
  packet: ReceiptPacketSnapshot,
): CertifiedAutoSubmitAdmission | undefined {
  const schemaVersion = packet.approvedExecutionSchemaVersion;
  const admission = packet.approvedExecutionAdmission;
  if (schemaVersion === undefined && admission === undefined) return undefined;
  if (schemaVersion !== 3
    || !isRecord(admission)
    || !hasExactKeys(admission, [
      "ats_certification",
      "authority_fingerprint",
      "authorization_id",
      "career_track_id",
      "kind",
      "revision_no",
    ])
    || admission.kind !== "track_auto_submit"
    || !validId(admission.authorization_id)
    || !validId(admission.career_track_id)
    || !positiveSafeInteger(admission.revision_no)
    || !validSha256(admission.authority_fingerprint)
    || !validCertificationAdmission(admission.ats_certification)) {
    throw new Error("Certified submission receipt frozen admission is invalid");
  }
  return admission as unknown as CertifiedAutoSubmitAdmission;
}

function validCertificationAdmission(value: unknown): boolean {
  if (!isRecord(value)
    || !hasExactKeys(value, [
      "activation_generation",
      "activation_sha256",
      "adapter_bundle_sha256",
      "adapter_version",
      "expires_at_ms",
      "layout_contract_version",
      "layout_set_sha256",
      "manifest_sha256",
      "provider",
      "runner_target_sha256s",
      "schema_version",
      "surface_sha256",
      "target_key_sha256",
      "variant_key",
    ])
    || value.schema_version !== 1
    || (value.provider !== "greenhouse" && value.provider !== "lever")
    || !validId(value.adapter_version)
    || !validId(value.variant_key)
    || !positiveSafeInteger(value.layout_contract_version)
    || !positiveSafeInteger(value.activation_generation)
    || !positiveSafeInteger(value.expires_at_ms)
    || !validSha256(value.manifest_sha256)
    || !validSha256(value.activation_sha256)
    || !validSha256(value.target_key_sha256)
    || !validSha256(value.layout_set_sha256)
    || !validSha256(value.surface_sha256)
    || !validSha256(value.adapter_bundle_sha256)
    || !Array.isArray(value.runner_target_sha256s)
    || value.runner_target_sha256s.length < 1
    || value.runner_target_sha256s.length > 2
    || !value.runner_target_sha256s.every(validSha256)) {
    return false;
  }
  return value.runner_target_sha256s.every((digest, index, digests) => (
    index === 0 || digests[index - 1] < digest
  ));
}

function assertCertifiedReceiptAuthority(
  receipt: ApplicationReceiptBundle,
  admission: CertifiedAutoSubmitAdmission,
  authority: AtsCertifiedReceiptAuthority,
): void {
  if (!isRecord(authority)
    || !hasExactKeys(authority, CERTIFIED_AUTHORITY_KEYS)
    || authority.schemaVersion !== 1
    || (authority.provider !== "greenhouse" && authority.provider !== "lever")
    || (authority.adapter !== "greenhouse" && authority.adapter !== "lever")
    || (authority.runnerKind !== "local" && authority.runnerKind !== "cloud")
    || (authority.rolloutChannel !== "canary" && authority.rolloutChannel !== "general")
    || !positiveSafeInteger(authority.activationGeneration)
    || !positiveSafeInteger(authority.bindingFence)
    || !positiveSafeInteger(authority.bindingConsumedAtMs)
    || ![
      authority.accountId,
      authority.applicationId,
      authority.runId,
      authority.adapterVersion,
      authority.applicationAttemptId,
      authority.phaseBRequestId,
    ].every(validId)
    || ![
      authority.manifestSha256,
      authority.activationSha256,
      authority.targetKeySha256,
      authority.layoutSetSha256,
      authority.layoutObservationSha256,
      authority.observedSurfaceSha256,
      authority.adapterBundleSha256,
      authority.runnerTargetSha256,
      authority.bindingSha256,
      authority.canaryReservationSha256,
      authority.meteringReservationSha256,
    ].every(validSha256)) {
    throw new Error("Certified submission receipt authority schema is invalid");
  }

  const certification = admission.ats_certification;
  const generatedAtMs = Date.parse(receipt.generatedAt);
  if (authority.accountId !== receipt.accountId
    || authority.applicationId !== receipt.applicationId
    || authority.runId !== receipt.runId
    || authority.provider !== receipt.job.source
    || authority.provider !== certification.provider
    || authority.adapter !== authority.provider
    || authority.adapter !== receipt.adapter
    || authority.adapterVersion !== receipt.adapterVersion
    || authority.adapterVersion !== certification.adapter_version
    || authority.manifestSha256 !== certification.manifest_sha256
    || authority.activationSha256 !== certification.activation_sha256
    || authority.activationGeneration !== certification.activation_generation
    || authority.targetKeySha256 !== certification.target_key_sha256
    || authority.layoutSetSha256 !== certification.layout_set_sha256
    || authority.observedSurfaceSha256 !== certification.surface_sha256
    || authority.adapterBundleSha256 !== certification.adapter_bundle_sha256
    || authority.runnerKind !== receipt.runner
    || !certification.runner_target_sha256s.includes(authority.runnerTargetSha256)
    || !Number.isSafeInteger(generatedAtMs)
    || generatedAtMs <= 0
    || authority.bindingConsumedAtMs > generatedAtMs
    || authority.bindingConsumedAtMs > certification.expires_at_ms) {
    throw new Error(
      "Certified submission receipt authority does not match the frozen execution",
    );
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return actual.length === sortedExpected.length
    && actual.every((key, index) => key === sortedExpected[index]);
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  allowed: readonly string[],
): boolean {
  const allowedKeys = new Set(allowed);
  return Object.keys(value).every((key) => allowedKeys.has(key));
}

function validId(value: unknown): value is string {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 240
    && value.trim() === value
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function validSha256(value: unknown): value is string {
  return typeof value === "string" && SHA256_PATTERN.test(value);
}

function positiveSafeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
}
