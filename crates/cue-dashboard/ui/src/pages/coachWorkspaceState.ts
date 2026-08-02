import { assistantProfilesEqual, normalizeAssistantProfile, type AssistantProfile } from "./assistantProfile";
import type {
  WorkspaceDeletedResponse,
  WorkspaceListResponse,
  WorkspaceRecord,
  WorkspaceResponse,
} from "../lib/workspaces";

export interface WorkspaceDraft {
  title: string;
  profile: AssistantProfile;
  instructions: string;
}

export interface StaleDraftMerge {
  draft: WorkspaceDraft;
  preserved_fields: Array<keyof WorkspaceDraft>;
  conflicted_fields: Array<keyof WorkspaceDraft>;
}

export interface WorkspaceDeleteResult {
  workspaces: WorkspaceRecord[];
  active_workspace_id: string | null;
  selected_workspace_id: string | null;
}

export interface WorkspaceRefreshResult {
  workspaces: WorkspaceRecord[];
  active_workspace_id: string | null;
  selected_workspace: WorkspaceRecord | null;
  draft: WorkspaceDraft | null;
  preserved_fields: Array<keyof WorkspaceDraft>;
  conflicted_fields: Array<keyof WorkspaceDraft>;
}

export interface WorkspaceActivationResult {
  workspaces: WorkspaceRecord[];
  active_workspace_id: string | null;
  selected_workspace: WorkspaceRecord;
  draft: WorkspaceDraft;
}

export interface WorkspaceReferenceSections {
  activity: NonNullable<WorkspaceRecord["activity"]>;
  context: NonNullable<WorkspaceRecord["context"]>;
  artifacts: NonNullable<WorkspaceRecord["artifacts"]>;
}

export function workspaceDraftFromRecord(workspace: WorkspaceRecord): WorkspaceDraft {
  return {
    title: workspace.title,
    profile: normalizeAssistantProfile(workspace.profile),
    instructions: workspace.instructions ?? "",
  };
}

export function workspaceDraftDirty(workspace: WorkspaceRecord, draft: WorkspaceDraft): boolean {
  const normalized = workspaceDraftFromRecord(workspace);
  return normalized.title.trim() !== draft.title.trim()
    || normalized.instructions.trim() !== draft.instructions.trim()
    || !assistantProfilesEqual(normalized.profile, draft.profile);
}

export function mergeStaleWorkspaceDraft(
  base: WorkspaceRecord,
  local: WorkspaceDraft,
  latest: WorkspaceRecord,
): StaleDraftMerge {
  const baseDraft = workspaceDraftFromRecord(base);
  const latestDraft = workspaceDraftFromRecord(latest);
  const result = { ...latestDraft };
  const preserved_fields: Array<keyof WorkspaceDraft> = [];
  const conflicted_fields: Array<keyof WorkspaceDraft> = [];

  for (const field of ["title", "instructions"] as const) {
    const localChanged = !draftFieldEqual(field, local[field], baseDraft[field]);
    if (!localChanged) continue;
    const remoteChanged = !draftFieldEqual(field, latestDraft[field], baseDraft[field]);
    if (!remoteChanged || draftFieldEqual(field, local[field], latestDraft[field])) {
      assignDraftField(result, field, local[field]);
      preserved_fields.push(field);
    } else {
      conflicted_fields.push(field);
    }
  }


  const mergedProfile = mergeProfileAfterReload(
    baseDraft.profile,
    local.profile,
    latestDraft.profile,
  );
  result.profile = mergedProfile.profile;
  if (mergedProfile.preserved) preserved_fields.push("profile");
  if (mergedProfile.conflicted) conflicted_fields.push("profile");

  return { draft: result, preserved_fields, conflicted_fields };
}

export function workspaceSwitchNeedsConfirmation(
  selected_workspace_id: string | null,
  requested_workspace_id: string,
  dirty: boolean,
): boolean {
  return dirty
    && selected_workspace_id !== null
    && selected_workspace_id !== requested_workspace_id;
}

export function applyWorkspaceActivation(
  workspaces: WorkspaceRecord[],
  requested_workspace_id: string,
  response: WorkspaceResponse,
): WorkspaceActivationResult {
  if (response.workspace.id !== requested_workspace_id) {
    throw new Error("Bluey activated a different workspace than requested.");
  }
  if (response.active_workspace_id !== requested_workspace_id) {
    throw new Error("Bluey did not confirm the requested workspace as active.");
  }
  const next = workspaces.some((workspace) => workspace.id === response.workspace.id)
    ? workspaces.map((workspace) => workspace.id === response.workspace.id ? response.workspace : workspace)
    : [...workspaces, response.workspace];
  return {
    workspaces: next,
    active_workspace_id: response.active_workspace_id,
    selected_workspace: response.workspace,
    draft: workspaceDraftFromRecord(response.workspace),
  };
}

export function applyWorkspaceDelete(
  workspaces: WorkspaceRecord[],
  selected_workspace_id: string | null,
  receipt: WorkspaceDeletedResponse,
): WorkspaceDeleteResult {
  const remaining = workspaces.filter((workspace) => workspace.id !== receipt.workspace_id);
  const active = receipt.active_workspace_id
    && remaining.some((workspace) => workspace.id === receipt.active_workspace_id)
    ? receipt.active_workspace_id
    : null;
  const selectedStillExists = selected_workspace_id
    && remaining.some((workspace) => workspace.id === selected_workspace_id);
  return {
    workspaces: remaining,
    active_workspace_id: active,
    selected_workspace_id: selectedStillExists
      ? selected_workspace_id
      : active ?? remaining[0]?.id ?? null,
  };
}

export function reconcileWorkspaceRefresh(
  current_workspace: WorkspaceRecord | null,
  current_draft: WorkspaceDraft | null,
  response: WorkspaceListResponse,
): WorkspaceRefreshResult {
  const currentIsDirty = Boolean(
    current_workspace
    && current_draft
    && workspaceDraftDirty(current_workspace, current_draft),
  );
  const preferredId = (currentIsDirty ? current_workspace?.id : null)
    ?? response.active_workspace_id
    ?? current_workspace?.id
    ?? response.workspaces[0]?.id
    ?? null;
  const selected = response.workspaces.find((workspace) => workspace.id === preferredId)
    ?? response.workspaces[0]
    ?? null;

  if (!selected) {
    return {
      workspaces: response.workspaces,
      active_workspace_id: response.active_workspace_id,
      selected_workspace: null,
      draft: null,
      preserved_fields: [],
      conflicted_fields: [],
    };
  }

  if (current_workspace && current_draft && current_workspace.id === selected.id) {
    const merged = mergeStaleWorkspaceDraft(current_workspace, current_draft, selected);
    return {
      workspaces: response.workspaces,
      active_workspace_id: response.active_workspace_id,
      selected_workspace: selected,
      draft: merged.draft,
      preserved_fields: merged.preserved_fields,
      conflicted_fields: merged.conflicted_fields,
    };
  }

  return {
    workspaces: response.workspaces,
    active_workspace_id: response.active_workspace_id,
    selected_workspace: selected,
    draft: workspaceDraftFromRecord(selected),
    preserved_fields: [],
    conflicted_fields: [],
  };
}

export function workspaceReferenceSections(workspace: WorkspaceRecord): WorkspaceReferenceSections {
  return {
    activity: workspace.activity ?? [],
    context: workspace.context ?? [],
    artifacts: workspace.artifacts ?? [],
  };
}

function draftFieldEqual<K extends keyof WorkspaceDraft>(
  field: K,
  left: WorkspaceDraft[K],
  right: WorkspaceDraft[K],
): boolean {
  if (field === "profile") {
    return assistantProfilesEqual(left as AssistantProfile, right as AssistantProfile);
  }
  return String(left).trim() === String(right).trim();
}

function assignDraftField<K extends keyof WorkspaceDraft>(
  draft: WorkspaceDraft,
  field: K,
  value: WorkspaceDraft[K],
) {
  if (field === "profile") {
    draft.profile = value as AssistantProfile;
  } else if (field === "title") {
    draft.title = value as string;
  } else {
    draft.instructions = value as string;
  }
}

function mergeProfileAfterReload(
  base: AssistantProfile,
  local: AssistantProfile,
  latest: AssistantProfile,
): { profile: AssistantProfile; preserved: boolean; conflicted: boolean } {
  const profile: AssistantProfile = {
    ...latest,
    // Provenance is daemon-owned and always comes from the newest revision.
    source: latest.source ?? null,
  };
  let preserved = false;
  let conflicted = false;
  for (const field of [
    "mode",
    "target_role",
    "company",
    "custom_instructions",
    "priority_questions",
  ] as const) {
    const localChanged = !profileValueEqual(local[field], base[field]);
    if (!localChanged) continue;
    const remoteChanged = !profileValueEqual(latest[field], base[field]);
    if (!remoteChanged || profileValueEqual(local[field], latest[field])) {
      if (field === "priority_questions") {
        profile.priority_questions = [...local.priority_questions];
      } else if (field === "mode") {
        profile.mode = local.mode;
      } else {
        profile[field] = local[field];
      }
      preserved = true;
    } else {
      conflicted = true;
    }
  }
  return { profile, preserved, conflicted };
}

function profileValueEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(left ?? null) === JSON.stringify(right ?? null);
}
