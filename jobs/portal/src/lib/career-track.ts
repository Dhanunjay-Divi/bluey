import type { CareerTrack } from "../types";

export interface CareerTrackAuthority {
  currentSourceAssetId: string;
  sourceName: string;
}

export interface CareerTrackAuthorityState {
  sourceLabel: string;
  sourceStale: boolean;
}

export function createCareerTrackDraft(
  track: CareerTrack | null,
  applicationIdentityId: string | undefined,
  authority: CareerTrackAuthority,
): CareerTrack {
  if (!track) {
    return {
      id: "",
      name: "",
      role: "",
      locations: [],
      remote_preference: "remote_or_hybrid",
      application_identity_id: applicationIdentityId,
      source_resume_asset_id: authority.currentSourceAssetId,
      policy: {
        role_family: "",
        relevant_employment_ids: [],
        employment_types: [],
        engagement_types: [],
        work_authorizations: [],
      },
      active: true,
      match_count: 0,
      created_at_ms: 0,
      updated_at_ms: 0,
    };
  }

  return {
    ...track,
    application_identity_id: track.application_identity_id || applicationIdentityId,
    source_resume_asset_id:
      track.source_resume_asset_id || authority.currentSourceAssetId,
    policy: {
      role_family: track.policy?.role_family || "",
      relevant_employment_ids: track.policy?.relevant_employment_ids || [],
      employment_types: track.policy?.employment_types || [],
      engagement_types: track.policy?.engagement_types || [],
      work_authorizations: track.policy?.work_authorizations || [],
    },
  };
}

export function bindCareerTrackForSave(
  track: CareerTrack,
  applicationIdentityId: string | undefined,
  authority: CareerTrackAuthority,
): CareerTrack {
  return {
    ...track,
    application_identity_id: track.application_identity_id || applicationIdentityId,
    source_resume_asset_id: authority.currentSourceAssetId,
  };
}

export function careerTrackAuthorityState(
  track: CareerTrack,
  authority: CareerTrackAuthority,
): CareerTrackAuthorityState {
  return {
    sourceLabel: authority.sourceName || "Career Profile facts",
    sourceStale:
      track.source_resume_asset_id !== authority.currentSourceAssetId,
  };
}
