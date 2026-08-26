import type { CareerTrackPolicyAuthority } from "../types";

const SHA256 = /^[a-f0-9]{64}$/;

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
  if (!authority || authority.review_state !== "approved") return false;
  if (authority.review_reason_codes.length !== 0) return false;
  if (
    authority.policy_revision_no <= 0
    || authority.policy_head_generation !== authority.policy_revision_no
    || authority.taxonomy_activation_epoch <= 0
    || authority.canonicalizer_schema_version <= 0
    || authority.account_input_generation <= 0
    || authority.track_input_generation <= 0
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
  if (requiredIds.some((value) => !value.trim())) return false;

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
  if (requiredDigests.some((value) => !SHA256.test(value))) return false;

  return Boolean(sourceResume.source_resume_asset_id.trim())
    && SHA256.test(sourceResume.source_resume_sha256)
    && authority.source_resume_asset_id === sourceResume.source_resume_asset_id
    && authority.source_resume_sha256 === sourceResume.source_resume_sha256
    && Boolean(sourceResume.applicationIdentityId?.trim())
    && authority.applicationIdentityId === sourceResume.applicationIdentityId;
}
