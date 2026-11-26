import type { CareerTrackPolicyAuthority } from "../types";

const SHA256 = /^[a-f0-9]{64}$/;
const nonBlank = (value: unknown): value is string => typeof value === "string" && value.trim().length > 0;
const sha256 = (value: unknown): value is string =>
  typeof value === "string" && value.length === 64 && SHA256.test(value);
const positiveInteger = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value > 0;

export interface SourceResumePolicyBinding {
  source_resume_asset_id: string;
  source_resume_sha256: string;
  applicationIdentityId: string | undefined;
}

/**
 * Interpret only the complete server projection as approved UI readiness.
 * The server remains the execution authority; this prevents a partial or
 * legacy projection from being presented as ready in the portal.
 */
export function isApprovedTrackPolicyAuthority(
  authority: CareerTrackPolicyAuthority | undefined,
  sourceResume: SourceResumePolicyBinding,
): authority is CareerTrackPolicyAuthority {
  if (!authority || typeof authority !== "object" || authority.review_state !== "approved") return false;
  if (!Array.isArray(authority.review_reason_codes) || authority.review_reason_codes.length !== 0) return false;
  if (
    !positiveInteger(authority.policy_revision_no)
    || authority.policy_head_generation !== authority.policy_revision_no
    || !positiveInteger(authority.taxonomy_activation_epoch)
    || !positiveInteger(authority.canonicalizer_schema_version)
    || !positiveInteger(authority.account_input_generation)
    || !positiveInteger(authority.track_input_generation)
    || !Array.isArray(authority.canonical_location_ids)
    || !authority.canonical_location_ids.every(nonBlank)
  ) return false;

  const requiredIds = [
    authority.taxonomy_version,
    authority.canonical_role_id,
    authority.canonical_role_family_id,
    authority.source_resume_asset_id,
    authority.applicationIdentityId,
    authority.policy_revision_id,
    authority.policy_review_receipt_id,
  ];
  if (!requiredIds.every(nonBlank)) return false;

  const requiredDigests = [
    authority.taxonomy_sha256,
    authority.canonicalizer_sha256,
    authority.account_input_transition_sha256,
    authority.account_input_semantic_sha256,
    authority.track_input_transition_sha256,
    authority.track_semantic_sha256,
    authority.source_resume_sha256,
    authority.application_identity_sha256,
    authority.job_preferences_sha256,
    authority.canonical_policy_sha256,
    authority.policy_head_transition_sha256,
    authority.policy_review_receipt_sha256,
  ];
  if (!requiredDigests.every(sha256)) return false;

  return nonBlank(sourceResume.source_resume_asset_id)
    && sha256(sourceResume.source_resume_sha256)
    && authority.source_resume_asset_id === sourceResume.source_resume_asset_id
    && authority.source_resume_sha256 === sourceResume.source_resume_sha256
    && nonBlank(sourceResume.applicationIdentityId)
    && authority.applicationIdentityId === sourceResume.applicationIdentityId;
}
