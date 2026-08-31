import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { ApiError } from "./api";
import { AppShell } from "./components/AppShell";
import { previewWorkspace } from "./data/preview";
import type {
  CareerProfile,
  CareerTrackPolicyAuthority,
  JobsWorkspace,
  MailboxConnection,
  UploadResumeSourceResponse,
} from "./types";

type ResumeUploadRequestId = ReturnType<Crypto["randomUUID"]>;

let ResumeUploadAttemptLineage: typeof import("./App").ResumeUploadAttemptLineage;
let uploadResumeSourceWithLineage: typeof import("./App").uploadResumeSourceWithLineage;
let openMailboxCommunicationAuthorization:
  typeof import("./App").openMailboxCommunicationAuthorization;
let jobsPortalHomeDestination: typeof import("./App").jobsPortalHomeDestination;
let trackSaveToast: typeof import("./App").trackSaveToast;
let invalidateWorkspacePolicyAuthority:
  typeof import("./App").invalidateWorkspacePolicyAuthority;
let reconcileWorkspacePolicyAuthority:
  typeof import("./App").reconcileWorkspacePolicyAuthority;
let WorkspaceAuthorityEpoch: typeof import("./App").WorkspaceAuthorityEpoch;
let installWorkspaceAtAuthorityEpoch:
  typeof import("./App").installWorkspaceAtAuthorityEpoch;
let runWorkspacePolicyAuthorityMutation:
  typeof import("./App").runWorkspacePolicyAuthorityMutation;
let workspaceAfterAutoSubmitAuthorization:
  typeof import("./App").workspaceAfterAutoSubmitAuthorization;
let workspaceAfterAutoSubmitRevocation:
  typeof import("./App").workspaceAfterAutoSubmitRevocation;
let workspaceAfterTrackDeletion:
  typeof import("./App").workspaceAfterTrackDeletion;

beforeAll(async () => {
  vi.stubGlobal("window", {
    location: { search: "", origin: "https://jobs.bluey.example" },
    matchMedia: () => ({ matches: false }),
  });
  vi.stubGlobal("localStorage", {
    getItem: () => null,
    setItem: () => undefined,
    removeItem: () => undefined,
  });
  const app = await import("./App");
  ResumeUploadAttemptLineage = app.ResumeUploadAttemptLineage;
  uploadResumeSourceWithLineage = app.uploadResumeSourceWithLineage;
  openMailboxCommunicationAuthorization = app.openMailboxCommunicationAuthorization;
  jobsPortalHomeDestination = app.jobsPortalHomeDestination;
  trackSaveToast = app.trackSaveToast;
  invalidateWorkspacePolicyAuthority = app.invalidateWorkspacePolicyAuthority;
  reconcileWorkspacePolicyAuthority = app.reconcileWorkspacePolicyAuthority;
  WorkspaceAuthorityEpoch = app.WorkspaceAuthorityEpoch;
  installWorkspaceAtAuthorityEpoch = app.installWorkspaceAtAuthorityEpoch;
  runWorkspacePolicyAuthorityMutation = app.runWorkspacePolicyAuthorityMutation;
  workspaceAfterAutoSubmitAuthorization = app.workspaceAfterAutoSubmitAuthorization;
  workspaceAfterAutoSubmitRevocation = app.workspaceAfterAutoSubmitRevocation;
  workspaceAfterTrackDeletion = app.workspaceAfterTrackDeletion;
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("Jobs portal home route", () => {
  it("uses Overview as the canonical basename-relative home", () => {
    expect(jobsPortalHomeDestination()).toBe("/overview");
    expect(jobsPortalHomeDestination("?preview=1&scenario=many-matches"))
      .toBe("/overview?preview=1&scenario=many-matches");
  });

  it("exposes Overview in both navigation layouts without growing the mobile tab bar", () => {
    const markup = renderToStaticMarkup(createElement(
      MemoryRouter,
      { initialEntries: ["/overview?preview=1"] },
      createElement(
        AppShell,
        {
          account: { email: "taylor@example.com", balance_cents: 2450 },
          workspace: previewWorkspace,
          onRefresh: () => undefined,
          preview: true,
          previewSearch: "?preview=1",
          children: createElement("p", null, "Command center"),
        },
      ),
    ));
    const desktopNavigation = markup.match(/<nav class="desktop-nav"[\s\S]*?<\/nav>/)?.[0] || "";
    const mobileNavigation = markup.match(/<nav class="mobile-nav"[\s\S]*?<\/nav>/)?.[0] || "";

    expect(desktopNavigation).toContain('href="/overview?preview=1"');
    expect(desktopNavigation).toContain('href="/settings?preview=1"');
    expect(mobileNavigation).toContain('href="/overview?preview=1"');
    expect(mobileNavigation).not.toContain('href="/settings?preview=1"');
    expect(mobileNavigation.match(/ href=/g)).toHaveLength(5);
  });
});

describe("Career Track save feedback", () => {
  it("uses workspace existence rather than a client-generated ID to distinguish create from update", () => {
    expect(trackSaveToast(false)).toBe("Career Track started.");
    expect(trackSaveToast(true)).toBe("Career Track updated.");
  });
});

describe("resume source upload request lineage", () => {
  it("reuses one request id after an ambiguous timeout and clears it after success", async () => {
    const requestIds: ResumeUploadRequestId[] = [
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
    ];
    const lineage = new ResumeUploadAttemptLineage(() => {
      const requestId = requestIds.shift();
      if (!requestId) throw new Error("test request ids exhausted");
      return requestId;
    });
    const file = new File(["exact resume"], "resume.txt", { type: "text/plain" });
    const profile = previewWorkspace.profile;
    const observed: string[] = [];
    let calls = 0;
    const upload = async (
      _file: File,
      savedProfile: CareerProfile,
      _pageCount: number | undefined,
      requestId: ResumeUploadRequestId,
    ): Promise<UploadResumeSourceResponse> => {
      observed.push(requestId);
      calls += 1;
      if (calls === 1) throw new ApiError(0, "Upload timed out after the server may have committed.");
      return responseFor(savedProfile, requestId);
    };

    await expect(
      uploadResumeSourceWithLineage(lineage, file, profile, undefined, upload),
    ).rejects.toMatchObject({ status: 0 });
    await expect(
      uploadResumeSourceWithLineage(lineage, file, profile, undefined, upload),
    ).resolves.toMatchObject({ asset: { id: observed[0] } });
    await uploadResumeSourceWithLineage(lineage, file, profile, undefined, upload);

    expect(observed).toEqual([
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
    ]);
  });

  it("uses a new request id for a reselected file or changed profile", async () => {
    let nextId = 0;
    const lineage = new ResumeUploadAttemptLineage(
      () =>
        `00000000-0000-4000-8000-${String(++nextId).padStart(12, "0")}` as ResumeUploadRequestId,
    );
    const firstSelection = new File(["same bytes"], "resume.txt", { type: "text/plain" });
    const reselectedFile = new File(["same bytes"], "resume.txt", { type: "text/plain" });
    const profile = previewWorkspace.profile;
    const changedProfile = { ...profile, headline: `${profile.headline} II` };
    const observed: string[] = [];
    const ambiguousUpload = async (
      _file: File,
      _profile: CareerProfile,
      _pageCount: number | undefined,
      requestId: ResumeUploadRequestId,
    ): Promise<UploadResumeSourceResponse> => {
      observed.push(requestId);
      throw new ApiError(503, "The response was unavailable.");
    };

    await expect(
      uploadResumeSourceWithLineage(lineage, firstSelection, profile, undefined, ambiguousUpload),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      uploadResumeSourceWithLineage(lineage, reselectedFile, profile, undefined, ambiguousUpload),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      uploadResumeSourceWithLineage(
        lineage,
        reselectedFile,
        changedProfile,
        undefined,
        ambiguousUpload,
      ),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      uploadResumeSourceWithLineage(
        lineage,
        firstSelection,
        profile,
        undefined,
        ambiguousUpload,
      ),
    ).rejects.toMatchObject({ status: 503 });

    expect(observed).toEqual([
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
      "00000000-0000-4000-8000-000000000003",
      "00000000-0000-4000-8000-000000000004",
    ]);
  });

  it("clears a request id after a definitive conflict", async () => {
    let nextId = 0;
    const lineage = new ResumeUploadAttemptLineage(
      () =>
        `00000000-0000-4000-8000-${String(++nextId).padStart(12, "0")}` as ResumeUploadRequestId,
    );
    const file = new File(["resume"], "resume.txt", { type: "text/plain" });
    const observed: string[] = [];
    const conflict = async (
      _file: File,
      _profile: CareerProfile,
      _pageCount: number | undefined,
      requestId: ResumeUploadRequestId,
    ): Promise<UploadResumeSourceResponse> => {
      observed.push(requestId);
      throw new ApiError(409, "Choose the file again.");
    };

    await expect(
      uploadResumeSourceWithLineage(lineage, file, previewWorkspace.profile, undefined, conflict),
    ).rejects.toMatchObject({ status: 409 });
    await expect(
      uploadResumeSourceWithLineage(lineage, file, previewWorkspace.profile, undefined, conflict),
    ).rejects.toMatchObject({ status: 409 });

    expect(observed).toEqual([
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
    ]);
  });
});

describe("workspace policy authority reconciliation", () => {
  async function expectStaleRefreshRejectedAfterOperation<T>(
    result: T,
    updateWorkspace: (workspace: JobsWorkspace, result: T) => JobsWorkspace,
    assertInstalled: (workspace: JobsWorkspace) => void,
  ): Promise<void> {
    const staleWorkspace = workspaceWithApprovedPolicyAuthority();
    let current: JobsWorkspace | null = staleWorkspace;
    let finishMutation: ((value: T) => void) | undefined;
    const mutationResult = new Promise<T>((resolve) => {
      finishMutation = resolve;
    });
    const authorityEpoch = new WorkspaceAuthorityEpoch();
    const staleRefreshToken = authorityEpoch.beginRefresh();
    const setWorkspace = (
      update: (value: JobsWorkspace | null) => JobsWorkspace | null,
    ) => {
      current = update(current);
    };

    const mutation = runWorkspacePolicyAuthorityMutation(
      setWorkspace,
      authorityEpoch,
      () => mutationResult,
      updateWorkspace,
      {
        preview: true,
        invalidateWorkspace: (workspace) => workspace,
      },
    );
    finishMutation?.(result);
    await mutation;

    expect(installWorkspaceAtAuthorityEpoch(
      setWorkspace,
      authorityEpoch,
      staleRefreshToken,
      staleWorkspace,
    )).toBe(false);
    expect(current).not.toBeNull();
    assertInstalled(current as JobsWorkspace);
  }

  it("invalidates every Track after a preference or source-resume change", () => {
    const workspace = workspaceWithApprovedPolicyAuthority();

    const invalidated = invalidateWorkspacePolicyAuthority(workspace);

    expect(invalidated.tracks.map((track) => track.policy.authority)).toEqual(
      workspace.tracks.map((track) => expect.objectContaining({
        review_state: "needs_review",
        review_reason_codes: ["portal_authority_refresh_required"],
      })),
    );
    expect(invalidated.auto_submit_authorizations.map((authorization) => authorization.status))
      .toEqual(["needs_review", "needs_review"]);
    expect(workspace.tracks.map((track) => track.policy.authority?.review_state))
      .toEqual(["approved", "approved"]);
    expect(workspace.auto_submit_authorizations.map((authorization) => authorization.status))
      .toEqual(["active", "active"]);
  });

  it("invalidates every Track after any identity mutation", async () => {
    const workspace = workspaceWithApprovedPolicyAuthority();
    let current: JobsWorkspace | null = workspace;
    const setWorkspace = (
      update: (value: JobsWorkspace | null) => JobsWorkspace | null,
    ) => {
      current = update(current);
    };

    await reconcileWorkspacePolicyAuthority(
      setWorkspace,
      (value) => ({
        ...value,
        application_identities: value.application_identities.filter(
          (identity) => identity.id !== "identity-primary",
        ),
      }),
      { preview: true },
    );

    expect(current?.application_identities.some((identity) => identity.id === "identity-primary"))
      .toBe(false);
    expect(current?.tracks[0].policy.authority).toMatchObject({
      review_state: "needs_review",
      review_reason_codes: ["portal_authority_refresh_required"],
    });
    expect(current?.tracks[1].policy.authority).toMatchObject({
      review_state: "needs_review",
      review_reason_codes: ["portal_authority_refresh_required"],
    });
    expect(current?.auto_submit_authorizations.map((authorization) => authorization.status))
      .toEqual(["needs_review", "needs_review"]);
  });

  it("rejects a refresh that began before an authority-sensitive mutation", async () => {
    const staleWorkspace = workspaceWithApprovedPolicyAuthority();
    const savedPreferences = { ...staleWorkspace.preferences, daily_limit: 3 };
    let current: JobsWorkspace | null = staleWorkspace;
    let finishMutation: ((value: typeof savedPreferences) => void) | undefined;
    const mutationResult = new Promise<typeof savedPreferences>((resolve) => {
      finishMutation = resolve;
    });
    const authorityEpoch = new WorkspaceAuthorityEpoch();
    const staleRefreshToken = authorityEpoch.beginRefresh();
    const setWorkspace = (
      update: (value: JobsWorkspace | null) => JobsWorkspace | null,
    ) => {
      current = update(current);
    };

    const mutation = runWorkspacePolicyAuthorityMutation(
      setWorkspace,
      authorityEpoch,
      () => mutationResult,
      (value, preferences) => ({ ...value, preferences }),
      { preview: true },
    );

    expect(current?.tracks.every(
      (track) => track.policy.authority?.review_state === "needs_review",
    )).toBe(true);
    finishMutation?.(savedPreferences);
    await mutation;

    expect(installWorkspaceAtAuthorityEpoch(
      setWorkspace,
      authorityEpoch,
      staleRefreshToken,
      staleWorkspace,
    )).toBe(false);
    expect(current?.preferences.daily_limit).toBe(3);
    expect(current?.tracks.every(
      (track) => track.policy.authority?.review_state === "needs_review",
    )).toBe(true);
    expect(current?.auto_submit_authorizations.every(
      (authorization) => authorization.status === "needs_review",
    )).toBe(true);
  });

  it("rejects a stale refresh after Auto-submit authorization", async () => {
    const saved = {
      ...workspaceWithApprovedPolicyAuthority().auto_submit_authorizations[0],
      id: "authorization-current",
      status: "active" as const,
    };
    await expectStaleRefreshRejectedAfterOperation(
      saved,
      workspaceAfterAutoSubmitAuthorization,
      (workspace) => {
        expect(workspace.auto_submit_authorizations.find(
          (authorization) => authorization.career_track_id === saved.career_track_id,
        )?.id).toBe(saved.id);
      },
    );
  });

  it("rejects a stale refresh after Auto-submit revocation", async () => {
    const trackId = workspaceWithApprovedPolicyAuthority().tracks[0].id;
    await expectStaleRefreshRejectedAfterOperation(
      trackId,
      workspaceAfterAutoSubmitRevocation,
      (workspace) => {
        expect(workspace.auto_submit_authorizations.some(
          (authorization) => authorization.career_track_id === trackId,
        )).toBe(false);
      },
    );
  });

  it("rejects a stale refresh after Track deletion", async () => {
    const trackId = workspaceWithApprovedPolicyAuthority().tracks[0].id;
    await expectStaleRefreshRejectedAfterOperation(
      trackId,
      workspaceAfterTrackDeletion,
      (workspace) => {
        expect(workspace.tracks.some((track) => track.id === trackId)).toBe(false);
        expect(workspace.auto_submit_authorizations.some(
          (authorization) => authorization.career_track_id === trackId,
        )).toBe(false);
        expect(workspace.matches.some((job) => job.track_id === trackId)).toBe(false);
      },
    );
  });

  it("waits for overlapping mutations before installing one current readback", async () => {
    const workspace = workspaceWithApprovedPolicyAuthority();
    const authoritative = workspaceWithApprovedPolicyAuthority();
    authoritative.profile = { ...authoritative.profile, headline: "Principal Engineer" };
    authoritative.preferences = { ...authoritative.preferences, daily_limit: 4 };
    let current: JobsWorkspace | null = workspace;
    let currentInstalls = 0;
    let finishProfile: ((value: CareerProfile) => void) | undefined;
    let finishPreferences: ((value: typeof workspace.preferences) => void) | undefined;
    let finishReadback: ((value: JobsWorkspace) => void) | undefined;
    const profileResult = new Promise<CareerProfile>((resolve) => {
      finishProfile = resolve;
    });
    const preferencesResult = new Promise<typeof workspace.preferences>((resolve) => {
      finishPreferences = resolve;
    });
    const readback = new Promise<JobsWorkspace>((resolve) => {
      finishReadback = resolve;
    });
    const loadWorkspace = vi.fn(() => readback);
    const authorityEpoch = new WorkspaceAuthorityEpoch();
    const setWorkspace = (
      update: (value: JobsWorkspace | null) => JobsWorkspace | null,
    ) => {
      const next = update(current);
      if (
        next?.profile.headline === "Principal Engineer"
        && next.preferences.daily_limit === 4
        && next.tracks.every((track) => track.policy.authority?.review_state === "approved")
      ) currentInstalls += 1;
      current = next;
    };

    const profileMutation = runWorkspacePolicyAuthorityMutation(
      setWorkspace,
      authorityEpoch,
      () => profileResult,
      (value, profile) => ({ ...value, profile }),
      { preview: false, loadWorkspace },
    );
    const preferencesMutation = runWorkspacePolicyAuthorityMutation(
      setWorkspace,
      authorityEpoch,
      () => preferencesResult,
      (value, preferences) => ({ ...value, preferences }),
      { preview: false, loadWorkspace },
    );
    let profileResolved = false;
    let preferencesResolved = false;
    void profileMutation.then(() => {
      profileResolved = true;
    });
    void preferencesMutation.then(() => {
      preferencesResolved = true;
    });

    finishPreferences?.({ ...workspace.preferences, daily_limit: 2 });
    await flushMicrotasks();
    expect(loadWorkspace).not.toHaveBeenCalled();
    expect(profileResolved).toBe(false);
    expect(preferencesResolved).toBe(false);

    finishProfile?.({ ...workspace.profile, headline: "Principal Engineer" });
    await flushMicrotasks();

    expect(loadWorkspace).toHaveBeenCalledTimes(1);
    expect(profileResolved).toBe(false);
    expect(preferencesResolved).toBe(false);
    expect(currentInstalls).toBe(0);

    finishReadback?.(authoritative);
    await Promise.all([profileMutation, preferencesMutation]);

    expect(profileResolved).toBe(true);
    expect(preferencesResolved).toBe(true);
    expect(currentInstalls).toBe(1);
    expect(current?.profile.headline).toBe("Principal Engineer");
    expect(current?.preferences.daily_limit).toBe(4);
  });

  it("restarts the shared readback when a mutation arrives during it", async () => {
    const workspace = workspaceWithApprovedPolicyAuthority();
    const staleReadback = workspaceWithApprovedPolicyAuthority();
    const authoritative = workspaceWithApprovedPolicyAuthority();
    authoritative.profile = { ...authoritative.profile, headline: "Principal Engineer" };
    authoritative.preferences = { ...authoritative.preferences, daily_limit: 4 };
    let current: JobsWorkspace | null = workspace;
    let currentInstalls = 0;
    let finishFirstReadback: ((value: JobsWorkspace) => void) | undefined;
    let finishSecondReadback: ((value: JobsWorkspace) => void) | undefined;
    const firstReadback = new Promise<JobsWorkspace>((resolve) => {
      finishFirstReadback = resolve;
    });
    const secondReadback = new Promise<JobsWorkspace>((resolve) => {
      finishSecondReadback = resolve;
    });
    let readbackCalls = 0;
    const loadWorkspace = vi.fn(() => ++readbackCalls === 1
      ? firstReadback
      : secondReadback);
    const authorityEpoch = new WorkspaceAuthorityEpoch();
    const setWorkspace = (
      update: (value: JobsWorkspace | null) => JobsWorkspace | null,
    ) => {
      const next = update(current);
      if (
        next?.profile.headline === "Principal Engineer"
        && next.preferences.daily_limit === 4
        && next.tracks.every((track) => track.policy.authority?.review_state === "approved")
      ) currentInstalls += 1;
      current = next;
    };

    const profileMutation = runWorkspacePolicyAuthorityMutation(
      setWorkspace,
      authorityEpoch,
      async () => ({ ...workspace.profile, headline: "Principal Engineer" }),
      (value, profile) => ({ ...value, profile }),
      { preview: false, loadWorkspace },
    );
    let profileResolved = false;
    void profileMutation.then(() => {
      profileResolved = true;
    });
    await flushMicrotasks();
    expect(loadWorkspace).toHaveBeenCalledTimes(1);

    const preferencesMutation = runWorkspacePolicyAuthorityMutation(
      setWorkspace,
      authorityEpoch,
      async () => ({ ...workspace.preferences, daily_limit: 2 }),
      (value, preferences) => ({ ...value, preferences }),
      { preview: false, loadWorkspace },
    );
    let preferencesResolved = false;
    void preferencesMutation.then(() => {
      preferencesResolved = true;
    });
    await flushMicrotasks();

    finishFirstReadback?.(staleReadback);
    await flushMicrotasks();

    expect(loadWorkspace).toHaveBeenCalledTimes(2);
    expect(profileResolved).toBe(false);
    expect(preferencesResolved).toBe(false);
    expect(currentInstalls).toBe(0);

    finishSecondReadback?.(authoritative);
    await Promise.all([profileMutation, preferencesMutation]);

    expect(profileResolved).toBe(true);
    expect(preferencesResolved).toBe(true);
    expect(currentInstalls).toBe(1);
    expect(current?.profile.headline).toBe("Principal Engineer");
    expect(current?.preferences.daily_limit).toBe(4);
  });

  it("keeps local authority fail-closed when the authoritative readback fails", async () => {
    const workspace = workspaceWithApprovedPolicyAuthority();
    const savedPreferences = { ...workspace.preferences, daily_limit: 3 };
    let current: JobsWorkspace | null = workspace;
    const setWorkspace = (
      update: (value: JobsWorkspace | null) => JobsWorkspace | null,
    ) => {
      current = update(current);
    };

    await expect(reconcileWorkspacePolicyAuthority(
      setWorkspace,
      (value) => ({ ...value, preferences: savedPreferences }),
      {
        preview: false,
        loadWorkspace: async () => {
          throw new Error("authoritative workspace unavailable");
        },
      },
    )).rejects.toThrow("authoritative workspace unavailable");

    expect(current?.preferences).toBe(savedPreferences);
    expect(current?.tracks.every(
      (track) => track.policy.authority?.review_state === "needs_review",
    )).toBe(true);
    expect(current?.auto_submit_authorizations.every(
      (authorization) => authorization.status === "needs_review",
    )).toBe(true);
  });

  it("fails closed immediately when Career Profile fields drift", async () => {
    const workspace = workspaceWithApprovedPolicyAuthority();
    const savedProfile = { ...workspace.profile, headline: "Principal Software Engineer" };
    let current: JobsWorkspace | null = workspace;
    let finishReadback: ((value: JobsWorkspace) => void) | undefined;
    const readback = new Promise<JobsWorkspace>((resolve) => {
      finishReadback = resolve;
    });

    const reconciliation = reconcileWorkspacePolicyAuthority(
      (update) => {
        current = update(current);
      },
      (value) => ({ ...value, profile: savedProfile }),
      { preview: false, loadWorkspace: () => readback },
    );

    expect(current?.profile).toBe(savedProfile);
    expect(current?.tracks.every(
      (track) => track.policy.authority?.review_state === "needs_review",
    )).toBe(true);
    expect(current?.auto_submit_authorizations.every(
      (authorization) => authorization.status === "needs_review",
    )).toBe(true);

    const authoritative = invalidateWorkspacePolicyAuthority({
      ...workspace,
      profile: savedProfile,
    });
    finishReadback?.(authoritative);
    await reconciliation;
    expect(current?.profile.headline).toBe("Principal Software Engineer");
  });

  it("awaits and installs the exact authoritative workspace before resolving", async () => {
    const workspace = workspaceWithApprovedPolicyAuthority();
    const authoritative = workspaceWithApprovedPolicyAuthority();
    authoritative.preferences = { ...authoritative.preferences, daily_limit: 4 };
    let current: JobsWorkspace | null = workspace;
    let resolveWorkspace: ((value: JobsWorkspace) => void) | undefined;
    const setWorkspace = (
      update: (value: JobsWorkspace | null) => JobsWorkspace | null,
    ) => {
      current = update(current);
    };
    const readback = new Promise<JobsWorkspace>((resolve) => {
      resolveWorkspace = resolve;
    });

    const reconciliation = reconcileWorkspacePolicyAuthority(
      setWorkspace,
      (value) => ({ ...value, preferences: { ...value.preferences, daily_limit: 2 } }),
      { preview: false, loadWorkspace: () => readback },
    );

    expect(current?.tracks.every(
      (track) => track.policy.authority?.review_state === "needs_review",
    )).toBe(true);
    let resolved = false;
    void reconciliation.then(() => {
      resolved = true;
    });
    await Promise.resolve();
    expect(resolved).toBe(false);

    resolveWorkspace?.(authoritative);
    await reconciliation;

    expect(resolved).toBe(true);
    expect(current?.preferences.daily_limit).toBe(4);
    expect(current?.tracks.map((track) => track.policy.authority?.review_state))
      .toEqual(["approved", "approved"]);
    expect(current?.auto_submit_authorizations.map((authorization) => authorization.status))
      .toEqual(["active", "active"]);
  });
});

describe("mailbox communication authorization", () => {
  const connection: MailboxConnection = {
    id: "connection-one",
    provider: "gmail",
    status: "connected",
    account_label: "candidate@gmail.com",
    aliases: [],
    capabilities: ["status_sync"],
    created_at_ms: 1,
    updated_at_ms: 1,
  };

  it("never starts or redirects from preview mode", async () => {
    const start = vi.fn(async () => ({ authorization_url: "https://accounts.example.test" }));
    const redirect = vi.fn();

    await openMailboxCommunicationAuthorization(
      true,
      connection,
      start,
      redirect,
      "https://jobs.bluey.example",
    );

    expect(start).not.toHaveBeenCalled();
    expect(redirect).not.toHaveBeenCalled();
  });

  it("starts the account-bound flow before redirecting outside preview", async () => {
    const authorizationUrl = googleCommunicationAuthorizationUrl();
    const start = vi.fn(async () => ({
      authorization_url: authorizationUrl,
    }));
    const redirect = vi.fn();

    await openMailboxCommunicationAuthorization(
      false,
      connection,
      start,
      redirect,
      "https://jobs.bluey.example",
    );

    expect(start).toHaveBeenCalledWith("connection-one");
    expect(redirect).toHaveBeenCalledWith(authorizationUrl);
  });

  it("does not redirect when the account-bound authorization URL fails verification", async () => {
    const start = vi.fn(async () => ({
      authorization_url: "https://attacker.example/communication-consent",
    }));
    const redirect = vi.fn();

    await expect(openMailboxCommunicationAuthorization(
      false,
      connection,
      start,
      redirect,
      "https://jobs.bluey.example",
    )).rejects.toThrow("could not verify");

    expect(start).toHaveBeenCalledWith("connection-one");
    expect(redirect).not.toHaveBeenCalled();
  });
});

function googleCommunicationAuthorizationUrl(): string {
  const token = "A".repeat(43);
  const url = new URL("https://accounts.google.com/o/oauth2/v2/auth");
  url.searchParams.set("client_id", "client.apps.googleusercontent.com");
  url.searchParams.set(
    "redirect_uri",
    "https://jobs.bluey.example/api/jobs/oauth/gmail/callback",
  );
  url.searchParams.set("response_type", "code");
  url.searchParams.set("scope", [
    "openid",
    "email",
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/calendar.events",
  ].join(" "));
  url.searchParams.set("state", token);
  url.searchParams.set("code_challenge", token);
  url.searchParams.set("code_challenge_method", "S256");
  url.searchParams.set("access_type", "offline");
  url.searchParams.set("include_granted_scopes", "true");
  url.searchParams.set("prompt", "consent");
  return url.href;
}

async function flushMicrotasks(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

function responseFor(profile: CareerProfile, requestId: string): UploadResumeSourceResponse {
  return {
    asset: {
      id: requestId,
      file_name: "resume.txt",
      media_type: "text/plain",
      file_type: "txt",
      sha256: "a".repeat(64),
      size_bytes: 12,
      template_status: "text_only",
      created_at_ms: 1,
      updated_at_ms: 1,
    },
    profile,
  };
}

function workspaceWithApprovedPolicyAuthority(): JobsWorkspace {
  const profile = {
    ...previewWorkspace.profile,
    source_resume_asset_id: "preview-source-resume",
    source_resume_sha256: "a".repeat(64),
  };
  const tracks = previewWorkspace.tracks.map((track, index) => ({
    ...track,
    policy: {
      ...track.policy,
      authority: approvedPolicyAuthority(
        track.id,
        track.application_identity_id || "",
        index + 1,
      ),
    },
  }));
  return {
    ...previewWorkspace,
    profile,
    tracks,
    auto_submit_authorizations: tracks.map((track, index) => ({
      id: `authorization-${track.id}`,
      career_track_id: track.id,
      application_identity_id: track.application_identity_id || "",
      source_resume_asset_id: profile.source_resume_asset_id,
      revision_no: index + 1,
      authorized_at_ms: index + 1,
      status: "active",
    })),
  };
}

function approvedPolicyAuthority(
  trackId: string,
  applicationIdentityId: string,
  revisionNo: number,
): CareerTrackPolicyAuthority {
  return {
    taxonomy_version: "bluey-jobs-taxonomy-v1-2026-08-25",
    taxonomy_sha256: "1".repeat(64),
    taxonomy_activation_epoch: 1,
    canonicalizer_schema_version: 1,
    canonicalizer_sha256: "2".repeat(64),
    account_input_generation: 1,
    account_input_transition_sha256: "3".repeat(64),
    account_input_semantic_sha256: "e".repeat(64),
    track_input_generation: revisionNo,
    track_input_transition_sha256: "4".repeat(64),
    track_semantic_sha256: "5".repeat(64),
    canonical_role_id: `${trackId}-role`,
    canonical_role_family_id: "software-engineering",
    canonical_location_ids: ["country:US"],
    source_resume_asset_id: "preview-source-resume",
    source_resume_sha256: "a".repeat(64),
    applicationIdentityId,
    application_identity_sha256: "b".repeat(64),
    job_preferences_sha256: "c".repeat(64),
    policy_revision_id: `revision-${trackId}`,
    policy_revision_no: revisionNo,
    canonical_policy_sha256: "d".repeat(64),
    policy_head_generation: revisionNo,
    policy_head_transition_sha256: "e".repeat(64),
    policy_review_receipt_id: `receipt-${trackId}`,
    policy_review_receipt_sha256: "f".repeat(64),
    review_state: "approved",
    review_reason_codes: [],
  };
}
