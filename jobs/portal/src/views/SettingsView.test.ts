import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { previewWorkspace } from "../data/preview";
import type { CareerTrackPolicyAuthority, JobsWorkspace } from "../types";
import {
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
    const markup = renderSettings(previewWorkspace);

    expect(markup).toContain("Auto-submit needs review");
    expect(markup).toContain("Canonical policy review required");
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
