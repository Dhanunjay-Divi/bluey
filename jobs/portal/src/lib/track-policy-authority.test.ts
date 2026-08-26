import { describe, expect, it } from "vitest";
import type { CareerTrackPolicyAuthority } from "../types";
import { isApprovedTrackPolicyAuthority } from "./track-policy-authority";

const sourceResume = {
  source_resume_asset_id: "resume-source-one",
  source_resume_sha256: "1".repeat(64),
  applicationIdentityId: "identity-one",
};

function approvedAuthority(): CareerTrackPolicyAuthority {
  return {
    taxonomy_version: "bluey-jobs-taxonomy-v1-2026-08-25",
    taxonomy_sha256: "facdb3593457b6585ea83c9f735c42616e7cf03caa6e0369be9154f542dd7254",
    taxonomy_activation_epoch: 1,
    canonicalizer_schema_version: 1,
    canonicalizer_sha256: "7".repeat(64),
    account_input_generation: 3,
    account_input_transition_sha256: "8".repeat(64),
    account_input_semantic_sha256: "b".repeat(64),
    track_input_generation: 2,
    track_input_transition_sha256: "9".repeat(64),
    track_semantic_sha256: "a".repeat(64),
    canonical_role_id: "software-engineer",
    canonical_role_family_id: "software-engineering",
    canonical_location_ids: [
      "country:US",
      "subdivision:US-NY",
      "metro:US-NY-new-york-metro",
      "city:US-NY-new-york",
    ],
    source_resume_asset_id: sourceResume.source_resume_asset_id,
    source_resume_sha256: sourceResume.source_resume_sha256,
    applicationIdentityId: sourceResume.applicationIdentityId,
    application_identity_sha256: "2".repeat(64),
    job_preferences_sha256: "3".repeat(64),
    policy_revision_id: "track-policy-one",
    policy_revision_no: 2,
    canonical_policy_sha256: "4".repeat(64),
    policy_head_generation: 2,
    policy_head_transition_sha256: "5".repeat(64),
    policy_review_receipt_id: "track-policy-review-one",
    policy_review_receipt_sha256: "6".repeat(64),
    review_state: "approved",
    review_reason_codes: [],
  };
}

describe("Career Track policy projection readiness", () => {
  it("accepts only a complete approved server projection bound to the current source resume", () => {
    expect(isApprovedTrackPolicyAuthority(approvedAuthority(), sourceResume)).toBe(true);
  });

  it("rejects missing receipt evidence, head drift, review reasons, and source drift", () => {
    const cases: CareerTrackPolicyAuthority[] = [
      { ...approvedAuthority(), policy_review_receipt_sha256: "" },
      { ...approvedAuthority(), policy_head_generation: 3 },
      { ...approvedAuthority(), taxonomy_activation_epoch: 0 },
      { ...approvedAuthority(), account_input_generation: 0 },
      { ...approvedAuthority(), account_input_semantic_sha256: "" },
      { ...approvedAuthority(), track_input_transition_sha256: "" },
      { ...approvedAuthority(), review_reason_codes: ["policy_ledger_review_required"] },
      { ...approvedAuthority(), source_resume_asset_id: "another-resume" },
      { ...approvedAuthority(), applicationIdentityId: "identity-two" },
      { ...approvedAuthority(), applicationIdentityId: "" },
    ];

    for (const authority of cases) {
      expect(isApprovedTrackPolicyAuthority(authority, sourceResume)).toBe(false);
    }
  });
});
