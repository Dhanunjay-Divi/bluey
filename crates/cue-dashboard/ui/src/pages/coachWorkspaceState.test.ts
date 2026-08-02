import { describe, expect, it } from "vitest";
import {
  applyWorkspaceDelete,
  applyWorkspaceActivation,
  mergeStaleWorkspaceDraft,
  reconcileWorkspaceRefresh,
  workspaceDraftFromRecord,
  workspaceReferenceSections,
  workspaceSwitchNeedsConfirmation,
} from "./coachWorkspaceState";
import type { WorkspaceListResponse, WorkspaceRecord } from "../lib/workspaces";

function fixture(id: string, title: string, revision = 1): WorkspaceRecord {
  return {
    schema_version: 1,
    id,
    title,
    revision,
    profile: {
      schema_version: 1,
      mode: "general",
      target_role: null,
      company: null,
      custom_instructions: null,
      priority_questions: [],
      source: null,
    },
    instructions: "",
    deletion_state: { state: "active" },
    created_at: "100",
    updated_at: String(100 + revision),
  };
}

describe("workspace-first Coach state", () => {
  it("preserves only safe local fields after a stale optimistic save", () => {
    const base = fixture("workspace-a", "Base", 3);
    base.profile.target_role = "Engineer";
    const local = workspaceDraftFromRecord(base);
    local.title = "My local title";
    local.profile = { ...local.profile, target_role: "Staff Engineer" };
    const latest = fixture("workspace-a", "Base", 4);
    latest.profile.target_role = "Principal Engineer";

    const merged = mergeStaleWorkspaceDraft(base, local, latest);

    expect(merged.draft.title).toBe("My local title");
    expect(merged.draft.profile.target_role).toBe("Principal Engineer");
    expect(merged.preserved_fields).toEqual(["title"]);
    expect(merged.conflicted_fields).toEqual(["profile"]);
  });

  it("requires confirmation for a dirty activate switch but not a clean or same-workspace switch", () => {
    expect(workspaceSwitchNeedsConfirmation("workspace-a", "workspace-b", true)).toBe(true);
    expect(workspaceSwitchNeedsConfirmation("workspace-a", "workspace-b", false)).toBe(false);
    expect(workspaceSwitchNeedsConfirmation("workspace-a", "workspace-a", true)).toBe(false);

    const first = fixture("workspace-a", "A");
    const second = fixture("workspace-b", "B", 2);
    const activated = applyWorkspaceActivation([first, second], second.id, {
      type: "workspace",
      workspace: { ...second, revision: 3 },
      active_workspace_id: second.id,
    });
    expect(activated.active_workspace_id).toBe(second.id);
    expect(activated.selected_workspace.id).toBe(second.id);
    expect(activated.draft.title).toBe("B");
  });

  it("uses the backend replacement after delete and remains idempotent on replay", () => {
    const first = fixture("workspace-a", "A");
    const replacement = fixture("workspace-b", "B");
    const receipt = {
      type: "workspace_deleted" as const,
      workspace_id: first.id,
      deleted: true,
      active_workspace_id: replacement.id,
    };

    const removed = applyWorkspaceDelete([first, replacement], first.id, receipt);
    const replayed = applyWorkspaceDelete(removed.workspaces, removed.selected_workspace_id, {
      ...receipt,
      deleted: false,
    });

    expect(removed.selected_workspace_id).toBe(replacement.id);
    expect(removed.active_workspace_id).toBe(replacement.id);
    expect(replayed).toEqual(removed);
  });

  it("refreshes imported Jobs data while preserving a non-conflicting unsaved instruction draft", () => {
    const current = fixture("workspace-a", "Interview", 5);
    const draft = workspaceDraftFromRecord(current);
    draft.instructions = "Keep my local answer format.";
    draft.profile.custom_instructions = "Preserve my local speaking style.";
    const imported = fixture("workspace-a", "Interview", 6);
    imported.profile.mode = "interview";
    imported.profile.target_role = "Data Engineer";
    imported.profile.company = "Bluey Jobs Company";
    imported.profile.source = { application_id: "application-1" };
    imported.linked_job = {
      import_id: "import-1",
      context_sha256: "a".repeat(64),
      source: { application_id: "application-1" },
      linked_at: "300",
    };
    const response: WorkspaceListResponse = {
      type: "workspace_list",
      workspaces: [imported],
      active_workspace_id: imported.id,
    };

    const refreshed = reconcileWorkspaceRefresh(current, draft, response);

    expect(refreshed.selected_workspace?.linked_job).toBeDefined();
    expect(refreshed.draft?.profile.target_role).toBe("Data Engineer");
    expect(refreshed.draft?.profile.source?.application_id).toBe("application-1");
    expect(refreshed.draft?.instructions).toBe("Keep my local answer format.");
    expect(refreshed.draft?.profile.custom_instructions).toBe("Preserve my local speaking style.");
    expect(refreshed.preserved_fields).toContain("instructions");
  });

  it("keeps the current dirty selection until the caller confirms switching", () => {
    const current = fixture("workspace-a", "A");
    const other = fixture("workspace-b", "B");
    const draft = workspaceDraftFromRecord(current);
    draft.title = "Unsaved A";

    expect(workspaceSwitchNeedsConfirmation(current.id, other.id, true)).toBe(true);
    expect(current.id).toBe("workspace-a");
    expect(draft.title).toBe("Unsaved A");
  });

  it("normalizes omitted reference arrays into five-section-safe empty lists", () => {
    const empty = fixture("workspace-empty", "Empty");

    expect(workspaceReferenceSections(empty)).toEqual({
      activity: [],
      context: [],
      artifacts: [],
    });
  });
});
