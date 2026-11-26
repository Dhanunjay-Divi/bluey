import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { previewWorkspace } from "../data/preview";
import type { CareerTrackPolicyAuthority, JobsWorkspace } from "../types";
import {
  createTrackDraft,
  SettingsView,
  saveSearchAndAutomationDefaults,
  trackAutoSubmitReadiness,
} from "./SettingsView";

const mailboxSyncState = {
  connection_id: "preview",
  provider: "gmail" as const,
  cursor: {},
  next_sync_at_ms: 0,
  last_error: "",
  created_at_ms: 0,
  updated_at_ms: 0,
};

describe("Settings Career Track drafts", () => {
  it("copies saved employment and engagement choices without inheriting authority", () => {
    const defaults = { employment_types: ["contract"], engagement_types: ["c2c"] };
    const draft = createTrackDraft(null, defaults, "verified-default");

    expect(draft.policy.employment_types).toEqual(["contract"]);
    expect(draft.policy.engagement_types).toEqual(["c2c"]);
    expect(draft.policy.employment_types).not.toBe(defaults.employment_types);
    expect(draft.policy.engagement_types).not.toBe(defaults.engagement_types);
    expect(draft.policy.authority).toBeUndefined();
    expect(draft.application_identity_id).toBe("verified-default");
  });

  it("explicitly selects full-time when no employment preference was saved", () => {
    const draft = createTrackDraft(null, { employment_types: [], engagement_types: [] });

    expect(draft.policy.employment_types).toEqual(["full_time"]);
    expect(draft.policy.engagement_types).toEqual([]);
  });

  it("displays supported saved aliases as canonical choices without duplicating them", () => {
    const draft = createTrackDraft(null, {
      employment_types: [" Contractor ", "contract", "Part-time", "FT"],
      engagement_types: ["Corporation-to-Corporation", "c2c", "W-2"],
    });

    expect(draft.policy.employment_types).toEqual(["contract", "part_time", "full_time"]);
    expect(draft.policy.engagement_types).toEqual(["c2c", "w2"]);
  });

  it("keeps existing Track policy, authority and identity instead of applying account defaults", () => {
    const base = previewWorkspace.tracks[0];
    const authority = approvedAuthority("existing-identity");
    const existing = {
      ...base,
      application_identity_id: "existing-identity",
      policy: { ...base.policy, authority },
    };
    const draft = createTrackDraft(existing, {
      employment_types: ["contract"],
      engagement_types: ["c2c"],
    }, "other-identity");

    expect(draft.policy).toEqual(existing.policy);
    expect(draft.policy.authority).toBe(authority);
    expect(draft.application_identity_id).toBe(existing.application_identity_id);
  });
});

function renderSettings(workspace: JobsWorkspace): string {
  return renderToStaticMarkup(createElement(SettingsView, {
    workspace,
    onSaveProfile: async () => undefined,
    onSavePreferences: async () => undefined,
    onSaveTrack: async () => undefined,
    onDeleteTrack: async () => undefined,
    onAuthorizeTrackAutoSubmit: async () => undefined,
    onRevokeTrackAutoSubmit: async () => undefined,
    onCreateIdentity: async (identity) => identity,
    onUpdateIdentity: async (identity) => identity,
    onVerifyIdentity: async (identity) => identity,
    onResendIdentity: async () => undefined,
    onDeleteIdentity: async () => undefined,
    onMailboxProviders: async () => [],
    onConnectMailbox: async () => undefined,
    onAuthorizeMailboxCommunication: async () => undefined,
    onMailboxSyncState: async () => mailboxSyncState,
    onMailboxMessages: async () => [],
    onSyncMailbox: async () => mailboxSyncState,
    onDeleteMailbox: async () => undefined,
    onSaveAnswerMemory: async (answer) => answer,
    onLookupCompanyAnswerScope: async () => { throw new Error("No verified employer in this fixture"); },
    onDeleteAnswerMemory: async () => undefined,
  }));
}

function approvedAuthority(applicationIdentityId: string): CareerTrackPolicyAuthority {
  return {
    taxonomy_version: "bluey-jobs-taxonomy-v1-2026-08-25",
    taxonomy_sha256: "1".repeat(64),
    taxonomy_activation_epoch: 1,
    canonicalizer_schema_version: 1,
    canonicalizer_sha256: "2".repeat(64),
    account_input_generation: 1,
    account_input_transition_sha256: "3".repeat(64),
    account_input_semantic_sha256: "4".repeat(64),
    track_input_generation: 1,
    track_input_transition_sha256: "5".repeat(64),
    track_semantic_sha256: "6".repeat(64),
    canonical_role_id: "software-engineer",
    canonical_role_family_id: "software-engineering",
    canonical_location_ids: ["country:US"],
    source_resume_asset_id: previewWorkspace.profile.source_resume_asset_id,
    source_resume_sha256: "7".repeat(64),
    applicationIdentityId,
    application_identity_sha256: "8".repeat(64),
    job_preferences_sha256: "9".repeat(64),
    policy_revision_id: "revision-one",
    policy_revision_no: 1,
    canonical_policy_sha256: "a".repeat(64),
    policy_head_generation: 1,
    policy_head_transition_sha256: "b".repeat(64),
    policy_review_receipt_id: "receipt-one",
    policy_review_receipt_sha256: "c".repeat(64),
    review_state: "approved",
    review_reason_codes: [],
  };
}

describe("Settings authority-sensitive saves", () => {
  it("commits preferences and their readback before saving profile drift", async () => {
    let finishPreferences: (() => void) | undefined;
    const preferencesSaved = new Promise<void>((resolve) => {
      finishPreferences = resolve;
    });
    const onSavePreferences = vi.fn(() => preferencesSaved);
    const onSaveProfile = vi.fn(async () => undefined);

    const save = saveSearchAndAutomationDefaults(
      previewWorkspace.preferences,
      previewWorkspace.profile,
      onSavePreferences,
      onSaveProfile,
    );

    expect(onSavePreferences).toHaveBeenCalledTimes(1);
    expect(onSaveProfile).not.toHaveBeenCalled();
    finishPreferences?.();
    await save;
    expect(onSaveProfile).toHaveBeenCalledTimes(1);
  });

  it("never presents a stale active authorization as enabled", () => {
    expect(trackAutoSubmitReadiness("active", true)).toBe("active");
    expect(trackAutoSubmitReadiness("active", false)).toBe("needs_review");
    expect(trackAutoSubmitReadiness("needs_review", true)).toBe("needs_review");
    expect(trackAutoSubmitReadiness(undefined, true)).toBe("review_first");
  });

  it("renders an active authorization with stale policy authority as review-required", () => {
    const markup = renderSettings({ ...previewWorkspace, tracks: [previewWorkspace.tracks[0]] });

    expect(markup).toContain("Auto-submit needs review");
    expect(markup).toContain("Canonical policy review required");
    expect(markup).not.toContain("Auto-submit enabled");
    expect(markup).toMatch(/<button[^>]*class="button secondary compact">Turn off<\/button>/);
    expect(markup).not.toMatch(/>Enable again<\/button>/);
  });

  it("keeps Turn off available even when the server marks a paused Track's grant as needing review", () => {
    const markup = renderSettings({
      ...previewWorkspace,
      tracks: [{ ...previewWorkspace.tracks[0], active: false }],
      auto_submit_authorizations: [{ ...previewWorkspace.auto_submit_authorizations[0], status: "needs_review" }],
    });

    expect(markup).toContain("Auto-submit needs review");
    expect(markup).toMatch(/<button[^>]*class="button secondary compact">Turn off<\/button>/);
    expect(markup).not.toMatch(/>Enable again<\/button>/);
  });

  it.each([
    ["active", "Auto-submit enabled", "", true],
    ["needs_review", "Auto-submit needs review", "Enable again", true],
    [undefined, "Review first", "Enable Auto-submit", false],
  ] as const)("keeps authorization and revocation actions separate for %s", (status, label, enable, revoke) => {
    const base = previewWorkspace.tracks[0];
    const authority = approvedAuthority(base.application_identity_id ?? "");
    const markup = renderSettings({
      ...previewWorkspace,
      profile: { ...previewWorkspace.profile, source_resume_sha256: authority.source_resume_sha256 },
      tracks: [{ ...base, policy: { ...base.policy, authority } }],
      auto_submit_authorizations: status ? [{ ...previewWorkspace.auto_submit_authorizations[0], status }] : [],
    });

    expect(markup).toContain(`>${label}</b>`);
    const enableAction = markup.match(/<button[^>]*class="button primary compact">(Enable[^<]*)<\/button>/);
    expect(enableAction?.[1] ?? "").toBe(enable);
    expect(/<button[^>]*class="button secondary compact">Turn off<\/button>/.test(markup)).toBe(revoke);
  });

  it.each([
    ["location_required", "Add a location. Choose a country if you have no city preference."],
    ["ambiguous_location", "A location is ambiguous. Add its region or country."],
    ["unknown_location", "A location could not be verified. Try a city and region, or a country."],
    ["policy_ledger_review_required", "Canonical policy review required"],
  ])("shows the next step for %s without claiming approval", (code, message) => {
    const track = previewWorkspace.tracks[0];
    const workspace: JobsWorkspace = {
      ...previewWorkspace,
      tracks: [{
        ...track,
        policy: {
          ...track.policy,
          authority: {
            ...approvedAuthority(track.application_identity_id ?? ""),
            review_state: "needs_review",
            review_reason_codes: [code],
          },
        },
      }],
    };

    const markup = renderSettings(workspace);

    expect(markup).toContain(message);
    expect(markup).not.toContain(code);
    expect(markup).not.toContain("Canonical policy approved");
    expect(markup).not.toContain("Auto-submit enabled");
  });

  it("requires the approved policy identity to equal the Track identity", () => {
    const sourceResumeSha256 = "7".repeat(64);
    const track = previewWorkspace.tracks[0];
    const workspace: JobsWorkspace = {
      ...previewWorkspace,
      profile: {
        ...previewWorkspace.profile,
        source_resume_sha256: sourceResumeSha256,
      },
      tracks: [{
        ...track,
        policy: {
          ...track.policy,
          authority: approvedAuthority("identity-career"),
        },
      }],
    };

    const markup = renderSettings(workspace);

    expect(markup).toContain("Auto-submit needs review");
    expect(markup).toContain("Canonical policy review required");
    expect(markup).not.toContain("Auto-submit enabled");
  });
});
