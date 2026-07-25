import { describe, expect, it } from "vitest";
import {
  bindCareerTrackForSave,
  careerTrackAuthorityState,
  createCareerTrackDraft,
} from "./career-track";
import type { CareerTrack } from "../types";

const authority = {
  currentSourceAssetId: "resume-v2",
  sourceName: "Taylor-Resume-v2.docx",
};

function existingTrack(overrides: Partial<CareerTrack> = {}): CareerTrack {
  return {
    id: "track-1",
    name: "Software engineering",
    role: "Software Engineer",
    locations: ["New York, NY"],
    remote_preference: "remote_or_hybrid",
    application_identity_id: "identity-primary",
    source_resume_asset_id: "resume-v1",
    policy: {
      role_family: "software_engineering",
      relevant_employment_ids: ["job-1"],
      employment_types: ["full_time"],
      engagement_types: [],
      work_authorizations: [],
    },
    active: true,
    match_count: 4,
    created_at_ms: 1,
    updated_at_ms: 2,
    ...overrides,
  };
}

describe("Career Track authority", () => {
  it("starts new tracks with the verified identity and current resume source", () => {
    const draft = createCareerTrackDraft(null, "identity-default", authority);

    expect(draft.application_identity_id).toBe("identity-default");
    expect(draft.source_resume_asset_id).toBe("resume-v2");
  });

  it("keeps a stale source visible until the track is explicitly saved", () => {
    const draft = createCareerTrackDraft(
      existingTrack(),
      "identity-default",
      authority,
    );

    expect(draft.source_resume_asset_id).toBe("resume-v1");
    expect(careerTrackAuthorityState(draft, authority)).toEqual({
      sourceLabel: "Taylor-Resume-v2.docx",
      sourceStale: true,
    });
  });

  it("binds the current resume revision when the user saves", () => {
    const bound = bindCareerTrackForSave(
      existingTrack(),
      "identity-default",
      authority,
    );

    expect(bound.application_identity_id).toBe("identity-primary");
    expect(bound.source_resume_asset_id).toBe("resume-v2");
  });

  it("uses Career Profile facts for profiles built without an upload", () => {
    const state = careerTrackAuthorityState(
      existingTrack({ source_resume_asset_id: "" }),
      { currentSourceAssetId: "", sourceName: "" },
    );

    expect(state).toEqual({
      sourceLabel: "Career Profile facts",
      sourceStale: false,
    });
  });
});
