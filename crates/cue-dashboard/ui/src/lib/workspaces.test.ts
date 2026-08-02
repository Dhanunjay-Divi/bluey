import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./tauri", () => ({ invoke: vi.fn() }));

import { invoke } from "./tauri";
import {
  workspaceDelete,
  workspaceInstructionsPatch,
  workspaceList,
  workspaceUpdate,
  type WorkspaceRecord,
  type WorkspaceUpdateRequest,
} from "./workspaces";

const invokeMock = vi.mocked(invoke);

const workspace: WorkspaceRecord = {
  schema_version: 1,
  id: "11111111-1111-4111-8111-111111111111",
  title: "Interview workspace",
  revision: 3,
  profile: {
    schema_version: 1,
    mode: "interview",
    target_role: "Platform Engineer",
    company: "Acme",
    custom_instructions: null,
    priority_questions: [],
    source: null,
  },
  deletion_state: { state: "active" },
  created_at: "100",
  updated_at: "200",
};

beforeEach(() => {
  invokeMock.mockReset();
});

describe("workspace IPC bridge", () => {
  it("accepts the exact tagged list response with omitted empty reference arrays", async () => {
    invokeMock.mockResolvedValue({
      type: "workspace_list",
      workspaces: [workspace],
      active_workspace_id: workspace.id,
    });

    await expect(workspaceList()).resolves.toMatchObject({
      type: "workspace_list",
      active_workspace_id: workspace.id,
    });
    expect(invokeMock).toHaveBeenCalledWith("workspace_list");
  });

  it("sends optimistic revision and explicit tagged instruction action unchanged", async () => {
    const request: WorkspaceUpdateRequest = {
      workspace_id: workspace.id,
      expected_revision: workspace.revision,
      title: "Principal interview workspace",
      instructions: { action: "set", text: "Keep answers concise." },
    };
    invokeMock.mockResolvedValue({
      type: "workspace",
      workspace: { ...workspace, revision: 4, title: request.title },
      active_workspace_id: workspace.id,
    });

    await workspaceUpdate(request);

    expect(invokeMock).toHaveBeenCalledWith("workspace_update", { request });
  });

  it("keeps soft-delete idempotency and active workspace status in the response", async () => {
    invokeMock.mockResolvedValue({
      type: "workspace_deleted",
      workspace_id: workspace.id,
      deleted: false,
      active_workspace_id: null,
    });

    await expect(workspaceDelete(workspace.id, workspace.revision)).resolves.toEqual({
      type: "workspace_deleted",
      workspace_id: workspace.id,
      deleted: false,
      active_workspace_id: null,
    });
    expect(invokeMock).toHaveBeenCalledWith("workspace_delete", {
      workspaceId: workspace.id,
      expectedRevision: workspace.revision,
    });
  });

  it("builds unchanged, clear, and set instruction patches without ambiguous nulls", () => {
    expect(workspaceInstructionsPatch(" concise ", "concise")).toEqual({ action: "unchanged" });
    expect(workspaceInstructionsPatch("concise", " \n ")).toEqual({ action: "clear" });
    expect(workspaceInstructionsPatch(null, "  Use examples.  ")).toEqual({
      action: "set",
      text: "Use examples.",
    });
  });

  it("rejects an untagged or partial daemon response", async () => {
    invokeMock.mockResolvedValue({ workspaces: [workspace], active_workspace_id: null });

    await expect(workspaceList()).rejects.toThrow("unreadable workspace_list response");
  });

  it("rejects malformed nested references before the Coach can render them", async () => {
    invokeMock.mockResolvedValue({
      type: "workspace_list",
      workspaces: [{
        ...workspace,
        activity: [{
          meeting_id: "../../not-a-meeting",
          title: "Bad reference",
          started_at: "100",
        }],
      }],
      active_workspace_id: workspace.id,
    });

    await expect(workspaceList()).rejects.toThrow("unreadable workspace_list response");
  });

  it("rejects unsafe revisions and linked-job provenance that disagrees with the profile", async () => {
    invokeMock.mockResolvedValueOnce({
      type: "workspace_list",
      workspaces: [{ ...workspace, revision: Number.MAX_SAFE_INTEGER + 1 }],
      active_workspace_id: workspace.id,
    });
    await expect(workspaceList()).rejects.toThrow("unreadable workspace_list response");

    invokeMock.mockResolvedValueOnce({
      type: "workspace_list",
      workspaces: [{
        ...workspace,
        profile: {
          ...workspace.profile,
          source: {
            application_id: "app-current",
            receipt_fingerprint: "a".repeat(64),
          },
        },
        linked_job: {
          import_id: "import-1",
          context_sha256: "b".repeat(64),
          source: {
            application_id: "app-stale",
            receipt_fingerprint: "a".repeat(64),
          },
          linked_at: "200",
        },
      }],
      active_workspace_id: workspace.id,
    });
    await expect(workspaceList()).rejects.toThrow("unreadable workspace_list response");
  });
});
