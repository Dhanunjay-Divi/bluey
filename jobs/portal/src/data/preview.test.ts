import { describe, expect, it } from "vitest";

import { previewWorkspace, previewWorkspaceForScenario } from "./preview";

describe("previewWorkspaceForScenario", () => {
  it("preserves application jobs while expanding many-matches to exactly 125 unique matches", () => {
    const workspace = previewWorkspaceForScenario(previewWorkspace, "many-matches");
    const matchIds = workspace.matches.map((match) => match.id);
    const matchIdSet = new Set(matchIds);

    expect(workspace.matches).toHaveLength(125);
    expect(matchIdSet.size).toBe(125);
    expect(matchIds.slice(0, previewWorkspace.matches.length)).toEqual(
      previewWorkspace.matches.map((match) => match.id),
    );
    expect(matchIds.at(-1)).toBe("volume-job-125");
    expect(
      workspace.applications.every((application) => matchIdSet.has(application.job_id)),
    ).toBe(true);
    expect(previewWorkspace.matches).toHaveLength(4);
  });

  it("keeps every application-related record attached to an existing consistent parent", () => {
    const workspace = previewWorkspaceForScenario(previewWorkspace, "many-matches");
    const matchIds = new Set(workspace.matches.map((match) => match.id));
    const applications = new Map(workspace.applications.map((application) => [application.id, application]));

    expect(
      workspace.application_evidence.every((evidence) => applications.has(evidence.application_id)),
    ).toBe(true);
    expect(
      workspace.browser_sessions.every(
        (session) => session.application_id === undefined || applications.has(session.application_id),
      ),
    ).toBe(true);
    expect(
      workspace.interventions.every(
        (intervention) =>
          intervention.application_id === undefined || applications.has(intervention.application_id),
      ),
    ).toBe(true);

    for (const event of workspace.candidate_events) {
      if (event.job_id !== undefined) expect(matchIds.has(event.job_id)).toBe(true);
      if (event.application_id === undefined) continue;

      const application = applications.get(event.application_id);
      expect(application).toBeDefined();
      if (event.job_id !== undefined) expect(application?.job_id).toBe(event.job_id);
    }
  });

  it("produces deterministic volume identities without mutating the source fixture", () => {
    const first = previewWorkspaceForScenario(previewWorkspace, "many-matches");
    const second = previewWorkspaceForScenario(previewWorkspace, "many-matches");
    const identity = (workspace: typeof first) =>
      workspace.matches.map(({ id, canonical_key, external_id, track_id }) => ({
        id,
        canonical_key,
        external_id,
        track_id,
      }));

    expect(identity(first)).toEqual(identity(second));
    expect(previewWorkspace.matches.map((match) => match.id)).toEqual([
      "job-1",
      "job-2",
      "job-3",
      "job-4",
    ]);
  });
});
