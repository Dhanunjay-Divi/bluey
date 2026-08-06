import { createHash } from "node:crypto";
import { adapterCanFinalize } from "./adapter-capabilities.js";
import type {
  ApplicationPacket,
  AtsCertificationAdmission,
  AtsFinalSubmitCertificationProof,
  CertifiedFinalSubmitAdapter,
  ExactSubmitFieldEvidence,
  ExactSubmitFileEvidence,
  ExactSubmitPartOrderEntry,
  FinalSubmitDocumentProof,
  FinalSubmitJobProof,
  FinalSubmitProof,
  ProviderFinalSubmitProof,
} from "./contracts.js";
import { FORM_FILE_READBACK_LIMITS } from "./form-readback.js";
import { EXACT_SUBMIT_FIELD_LIMITS } from "./trusted-submit.js";
import {
  certifiedProviderJobKey,
  CertifiedProviderJobKeyError,
} from "./provider-job-key.js";

const SHA256 = /^[a-f0-9]{64}$/;

const FINAL_CONTROLS = Object.freeze({
  greenhouse: "greenhouse_submit_application",
  lever: "lever_application_submit",
} as const);

export interface MaterializedFinalSubmitDocuments {
  resume: {
    versionId: string;
    sha256: string;
  };
  coverLetter?: {
    sha256: string;
  };
}

export class FinalSubmitProofError extends Error {
  constructor() {
    super("Invalid final submit proof");
    this.name = "FinalSubmitProofError";
  }
}

/**
 * Combines provider state-machine evidence with hashes of the exact PDFs that
 * are already present in the employer form. The returned wire value is a
 * strict allowlist: local paths and document bytes cannot flow into it.
 */
export function createFinalSubmitProof(
  providerProof: ProviderFinalSubmitProof,
  materialized: MaterializedFinalSubmitDocuments,
  job: FinalSubmitJobProof,
  admission?: ApplicationPacket["approvedExecutionAdmission"],
): FinalSubmitProof {
  assertProviderFinalSubmitProof(providerProof);
  assertFinalSubmitJobProof(providerProof.adapter, job);
  assertTargetMatchesJob(providerProof.adapter, providerProof.target, job);
  if (!materialized || typeof materialized !== "object") throw new FinalSubmitProofError();
  const resume = materialized.resume;
  if (!resume
    || typeof resume.versionId !== "string"
    || !validVersionId(resume.versionId)
    || !validSha256(resume.sha256)) {
    throw new FinalSubmitProofError();
  }

  const documents: FinalSubmitDocumentProof[] = [{
    kind: "resume",
    versionId: resume.versionId,
    sha256: resume.sha256,
  }];
  if (materialized.coverLetter !== undefined) {
    if (!materialized.coverLetter || !validSha256(materialized.coverLetter.sha256)) {
      throw new FinalSubmitProofError();
    }
    documents.push({
      kind: "cover_letter",
      sha256: materialized.coverLetter.sha256,
    });
  }
  documents.sort((left, right) => left.kind < right.kind ? -1 : left.kind > right.kind ? 1 : 0);
  assertFilesMatchDocuments(providerProof.files, documents);

  const reviewedProof = {
    schemaVersion: 3,
    adapter: providerProof.adapter,
    adapterVersion: providerProof.adapterVersion,
    control: providerProof.control,
    target: { ...providerProof.target },
    files: providerProof.files.map((file) => ({ ...file })),
    fields: providerProof.fields.map((field) => ({ ...field })),
    partOrder: providerProof.partOrder.map((entry) => ({ ...entry })),
    job: { ...job },
    documents,
  } as const;
  if (admission?.kind !== "track_auto_submit") return reviewedProof;
  const admissionCertification = admission.ats_certification;
  const certification = finalSubmitCertification(admissionCertification, providerProof);
  if (!admissionCertification) throw new FinalSubmitProofError();
  return {
    ...reviewedProof,
    schemaVersion: 4,
    certification,
    observedSurface: {
      schemaVersion: 1,
      variantKey: admissionCertification.variant_key,
      layoutContractVersion: admissionCertification.layout_contract_version,
      surfaceSha256: finalSubmitSurfaceSha256(providerProof),
    },
  };
}

export function assertFinalSubmitProof(value: unknown): asserts value is FinalSubmitProof {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new FinalSubmitProofError();
  }
  const proof = value as Record<string, unknown>;
  const certified = proof.schemaVersion === 4;
  if (Object.keys(proof).length !== (certified ? 12 : 10)
    || (proof.schemaVersion !== 3 && !certified)
    || !Array.isArray(proof.documents)) {
    throw new FinalSubmitProofError();
  }
  assertProviderFinalSubmitProof(proof);
  assertFinalSubmitJobProof(proof.adapter as CertifiedFinalSubmitAdapter, proof.job);
  assertTargetMatchesJob(
    proof.adapter as CertifiedFinalSubmitAdapter,
    proof.target,
    proof.job as FinalSubmitJobProof,
  );
  if (proof.documents.length < 1 || proof.documents.length > 2) {
    throw new FinalSubmitProofError();
  }
  let previousKind = "";
  let sawResume = false;
  for (const value of proof.documents) {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new FinalSubmitProofError();
    }
    const document = value as Record<string, unknown>;
    if (document.kind !== "resume" && document.kind !== "cover_letter") {
      throw new FinalSubmitProofError();
    }
    if (document.kind < previousKind || !validSha256(document.sha256)) {
      throw new FinalSubmitProofError();
    }
    previousKind = document.kind;
    if (document.kind === "resume") {
      if (Object.keys(document).length !== 3
        || !validVersionId(document.versionId)
        || sawResume) {
        throw new FinalSubmitProofError();
      }
      sawResume = true;
    } else if (Object.keys(document).length !== 2 || document.versionId !== undefined) {
      throw new FinalSubmitProofError();
    }
  }
  if (!sawResume) throw new FinalSubmitProofError();
  assertFilesMatchDocuments(
    proof.files as ExactSubmitFileEvidence[],
    proof.documents as FinalSubmitDocumentProof[],
  );
  if (certified) {
    const observedSurface = proof.observedSurface as Record<string, unknown> | undefined;
    if (!observedSurface
      || typeof observedSurface !== "object"
      || Array.isArray(observedSurface)
      || !sameKeys(observedSurface, [
        "layoutContractVersion",
        "schemaVersion",
        "surfaceSha256",
        "variantKey",
      ])
      || observedSurface.schemaVersion !== 1
      || !validVersionId(observedSurface.variantKey)
      || !Number.isSafeInteger(observedSurface.layoutContractVersion)
      || (observedSurface.layoutContractVersion as number) <= 0
      || !validSha256(observedSurface.surfaceSha256)
      || observedSurface.surfaceSha256
        !== finalSubmitSurfaceSha256(proof as unknown as ProviderFinalSubmitProof)) {
      throw new FinalSubmitProofError();
    }
    const certification = finalSubmitCertification(
      wireCertificationToAdmission(proof.certification, observedSurface),
      proof as unknown as ProviderFinalSubmitProof,
    );
    if (JSON.stringify(certification) !== JSON.stringify(proof.certification)) {
      throw new FinalSubmitProofError();
    }
  }
}

export function finalSubmitSurfaceSha256(
  proof: ProviderFinalSubmitProof,
): string {
  assertProviderFinalSubmitProof(proof);
  const canonical = {
    adapter: proof.adapter,
    adapterVersion: proof.adapterVersion,
    control: proof.control,
    fields: proof.fields.map((field) => ({ fieldName: field.fieldName })),
    files: proof.files.map((file) => ({ fieldName: file.fieldName })),
    form: {
      enctype: proof.target.enctype,
      formIdentitySha256: createHash("sha256").update(proof.target.formIdentity).digest("hex"),
      formTarget: proof.target.formTarget,
      method: proof.target.method,
    },
    partOrder: proof.partOrder.map((entry) => ({
      index: entry.index,
      kind: entry.kind,
    })),
    schemaVersion: 1,
  };
  return createHash("sha256").update(JSON.stringify(canonical)).digest("hex");
}

function finalSubmitCertification(
  value: AtsCertificationAdmission | undefined,
  providerProof: ProviderFinalSubmitProof,
): AtsFinalSubmitCertificationProof {
  if (!value
    || value.schema_version !== 1
    || value.provider !== providerProof.adapter
    || value.adapter_version !== providerProof.adapterVersion
    || !validVersionId(value.variant_key)
    || !Number.isSafeInteger(value.layout_contract_version)
    || value.layout_contract_version <= 0
    || !validSha256(value.surface_sha256)
    || value.surface_sha256 !== finalSubmitSurfaceSha256(providerProof)
    || !Number.isSafeInteger(value.activation_generation)
    || value.activation_generation <= 0
    || !Number.isSafeInteger(value.expires_at_ms)
    || value.expires_at_ms <= 0
    || !Array.isArray(value.runner_target_sha256s)
    || value.runner_target_sha256s.length < 1
    || value.runner_target_sha256s.length > 2) {
    throw new FinalSubmitProofError();
  }
  const digests = [
    value.manifest_sha256,
    value.activation_sha256,
    value.target_key_sha256,
    value.layout_set_sha256,
    value.adapter_bundle_sha256,
    ...value.runner_target_sha256s,
  ];
  if (!digests.every(validSha256)
    || value.runner_target_sha256s.some((digest, index, values) => (
      index > 0 && values[index - 1] >= digest
    ))) {
    throw new FinalSubmitProofError();
  }
  return {
    schemaVersion: 1,
    provider: value.provider,
    adapterVersion: value.adapter_version,
    manifestSha256: value.manifest_sha256,
    activationSha256: value.activation_sha256,
    activationGeneration: value.activation_generation,
    targetKeySha256: value.target_key_sha256,
    layoutSetSha256: value.layout_set_sha256,
    adapterBundleSha256: value.adapter_bundle_sha256,
    runnerTargetSha256s: [...value.runner_target_sha256s],
    expiresAtMs: value.expires_at_ms,
  };
}

function wireCertificationToAdmission(
  value: unknown,
  observedSurface: Record<string, unknown>,
): AtsCertificationAdmission | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const certification = value as Record<string, unknown>;
  if (!sameKeys(certification, [
    "activationGeneration",
    "activationSha256",
    "adapterBundleSha256",
    "adapterVersion",
    "expiresAtMs",
    "layoutSetSha256",
    "manifestSha256",
    "provider",
    "runnerTargetSha256s",
    "schemaVersion",
    "targetKeySha256",
  ])) return undefined;
  return {
    schema_version: certification.schemaVersion as 1,
    provider: certification.provider as CertifiedFinalSubmitAdapter,
    adapter_version: certification.adapterVersion as string,
    variant_key: observedSurface.variantKey as string,
    layout_contract_version: observedSurface.layoutContractVersion as number,
    surface_sha256: observedSurface.surfaceSha256 as string,
    manifest_sha256: certification.manifestSha256 as string,
    activation_sha256: certification.activationSha256 as string,
    activation_generation: certification.activationGeneration as number,
    target_key_sha256: certification.targetKeySha256 as string,
    layout_set_sha256: certification.layoutSetSha256 as string,
    adapter_bundle_sha256: certification.adapterBundleSha256 as string,
    runner_target_sha256s: certification.runnerTargetSha256s as string[],
    expires_at_ms: certification.expiresAtMs as number,
  };
}

function assertFinalSubmitJobProof(
  adapter: CertifiedFinalSubmitAdapter,
  value: unknown,
): asserts value is FinalSubmitJobProof {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new FinalSubmitProofError();
  }
  const job = value as Record<string, unknown>;
  if (Object.keys(job).length !== 2
    || typeof job.approvedCanonicalUrl !== "string"
    || typeof job.pageUrl !== "string") {
    throw new FinalSubmitProofError();
  }
  try {
    if (certifiedProviderJobKey(adapter, job.approvedCanonicalUrl, "submit")
      !== certifiedProviderJobKey(adapter, job.pageUrl, "submit")) {
      throw new FinalSubmitProofError();
    }
  } catch (error) {
    if (error instanceof CertifiedProviderJobKeyError) {
      throw new FinalSubmitProofError();
    }
    throw new FinalSubmitProofError();
  }
}

function assertProviderFinalSubmitProof(
  value: unknown,
): asserts value is ProviderFinalSubmitProof {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new FinalSubmitProofError();
  }
  const proof = value as Record<string, unknown>;
  if (proof.adapter !== "greenhouse" && proof.adapter !== "lever") {
    throw new FinalSubmitProofError();
  }
  const adapter = proof.adapter as CertifiedFinalSubmitAdapter;
  if (typeof proof.adapterVersion !== "string"
    || !adapterCanFinalize(adapter, proof.adapterVersion)
    || proof.control !== FINAL_CONTROLS[adapter]
    || !validSubmitTarget(adapter, proof.target)
    || !validSubmitFiles(proof.files)
    || !validSubmitFields(proof.fields)
    || !validSubmitPartOrder(proof.partOrder, proof.fields, proof.files)
    || hasCrossTypeFieldOverlap(proof.files, proof.fields)) {
    throw new FinalSubmitProofError();
  }
}

function validSubmitFields(value: unknown): value is ExactSubmitFieldEvidence[] {
  if (!Array.isArray(value)
    || value.length < 1
    || value.length > EXACT_SUBMIT_FIELD_LIMITS.maxFieldCount) {
    return false;
  }
  let aggregateBytes = 0;
  for (const entry of value) {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) return false;
    const field = entry as Record<string, unknown>;
    if (!sameKeys(field, ["fieldName", "valueByteLength", "valueSha256"])
      || typeof field.fieldName !== "string"
      || !validFieldName(field.fieldName)
      || typeof field.valueByteLength !== "number"
      || !Number.isSafeInteger(field.valueByteLength)
      || field.valueByteLength < 0
      || field.valueByteLength > EXACT_SUBMIT_FIELD_LIMITS.maxValueBytes
      || typeof field.valueSha256 !== "string"
      || !SHA256.test(field.valueSha256)) {
      return false;
    }
    aggregateBytes += field.valueByteLength;
  }
  return Number.isSafeInteger(aggregateBytes)
    && aggregateBytes <= EXACT_SUBMIT_FIELD_LIMITS.maxAggregateValueBytes;
}

function assertTargetMatchesJob(
  adapter: CertifiedFinalSubmitAdapter,
  targetValue: unknown,
  job: FinalSubmitJobProof,
): void {
  if (!validSubmitTarget(adapter, targetValue)) throw new FinalSubmitProofError();
  const target = targetValue as Record<string, string>;
  try {
    const approvedKey = certifiedProviderJobKey(adapter, job.approvedCanonicalUrl, "submit");
    const pageUrl = new URL(job.pageUrl);
    const actionUrl = new URL(target.actionUrl!);
    if (target.providerJobKey !== approvedKey || pageUrl.origin !== actionUrl.origin) {
      throw new FinalSubmitProofError();
    }
  } catch {
    throw new FinalSubmitProofError();
  }
}

function validSubmitTarget(
  adapter: CertifiedFinalSubmitAdapter,
  value: unknown,
): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const target = value as Record<string, unknown>;
  if (!sameKeys(target, [
    "actionUrl",
    "enctype",
    "formIdentity",
    "formTarget",
    "method",
    "providerJobKey",
  ])
    || typeof target.actionUrl !== "string"
    || target.method !== "post"
    || target.enctype !== "multipart/form-data"
    || target.formTarget !== "_self"
    || typeof target.providerJobKey !== "string"
    || typeof target.formIdentity !== "string"
    || target.formIdentity.length < 1
    || target.formIdentity.length > 1_024
    || /[\u0000-\u001f\u007f]/u.test(target.formIdentity)) {
    return false;
  }
  try {
    return certifiedProviderJobKey(adapter, target.actionUrl, "submit")
      === target.providerJobKey;
  } catch {
    return false;
  }
}

function validSubmitFiles(value: unknown): value is ExactSubmitFileEvidence[] {
  if (!Array.isArray(value)
    || value.length < 1
    || value.length > FORM_FILE_READBACK_LIMITS.maxFileCount) {
    return false;
  }
  let aggregateBytes = 0;
  const kinds = new Set<string>();
  for (const entry of value) {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) return false;
    const file = entry as Record<string, unknown>;
    if (!sameKeys(file, ["byteLength", "fieldName", "name", "sha256"])
      || typeof file.fieldName !== "string"
      || !validFieldName(file.fieldName)
      || typeof file.name !== "string"
      || typeof file.sha256 !== "string"
      || typeof file.byteLength !== "number"
      || !Number.isSafeInteger(file.byteLength)
      || file.byteLength < 1
      || file.byteLength > FORM_FILE_READBACK_LIMITS.maxFileBytes) {
      return false;
    }
    const match = /^(resume|cover-letter)-([a-f0-9]{64})\.pdf$/u.exec(file.name);
    if (!match || match[2] !== file.sha256 || kinds.has(match[1]!)) return false;
    kinds.add(match[1]!);
    aggregateBytes += file.byteLength;
  }
  return Number.isSafeInteger(aggregateBytes)
    && aggregateBytes <= FORM_FILE_READBACK_LIMITS.maxAggregateBytes;
}

function validSubmitPartOrder(
  value: unknown,
  fieldValue: unknown,
  fileValue: unknown,
): value is ExactSubmitPartOrderEntry[] {
  if (!Array.isArray(value) || !Array.isArray(fieldValue) || !Array.isArray(fileValue)
    || value.length !== fieldValue.length + fileValue.length) {
    return false;
  }
  const fieldIndexes = new Set<number>();
  const fileIndexes = new Set<number>();
  for (const entryValue of value) {
    if (!entryValue || typeof entryValue !== "object" || Array.isArray(entryValue)) return false;
    const entry = entryValue as Record<string, unknown>;
    if (!sameKeys(entry, ["index", "kind"])
      || (entry.kind !== "field" && entry.kind !== "file")
      || typeof entry.index !== "number"
      || !Number.isSafeInteger(entry.index)
      || entry.index < 0) {
      return false;
    }
    const indexes = entry.kind === "field" ? fieldIndexes : fileIndexes;
    const limit = entry.kind === "field" ? fieldValue.length : fileValue.length;
    if (entry.index >= limit || indexes.has(entry.index)) return false;
    indexes.add(entry.index);
  }
  return fieldIndexes.size === fieldValue.length && fileIndexes.size === fileValue.length;
}

function hasCrossTypeFieldOverlap(fileValue: unknown, fieldValue: unknown): boolean {
  if (!Array.isArray(fileValue) || !Array.isArray(fieldValue)) return true;
  const fileNames = new Set(fileValue.map((entry) => (
    (entry as Record<string, unknown>).fieldName
  )));
  return fieldValue.some((entry) => fileNames.has((entry as Record<string, unknown>).fieldName));
}

function validFieldName(value: unknown): value is string {
  return typeof value === "string"
    && value.length <= EXACT_SUBMIT_FIELD_LIMITS.maxFieldNameChars
    && /^[A-Za-z0-9_.:[\]-]{1,240}$/u.test(value);
}

function assertFilesMatchDocuments(
  fileValue: unknown,
  documents: readonly FinalSubmitDocumentProof[],
): void {
  if (!validSubmitFiles(fileValue) || fileValue.length !== documents.length) {
    throw new FinalSubmitProofError();
  }
  const documentHashes = new Map(documents.map((document) => [document.kind, document.sha256]));
  for (const file of fileValue) {
    const kind = file.name.startsWith("resume-") ? "resume" : "cover_letter";
    if (documentHashes.get(kind) !== file.sha256) throw new FinalSubmitProofError();
    documentHashes.delete(kind);
  }
  if (documentHashes.size !== 0) throw new FinalSubmitProofError();
}

function sameKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length
    && actual.every((key, index) => key === expected[index]);
}

function validSha256(value: unknown): value is string {
  return typeof value === "string" && SHA256.test(value);
}

function validVersionId(value: unknown): value is string {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 240
    && value.trim() === value
    && !/[\u0000-\u001f\u007f]/u.test(value);
}
