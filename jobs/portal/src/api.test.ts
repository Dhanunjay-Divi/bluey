import { beforeEach, describe, expect, it, vi } from "vitest";
import { jobsApi } from "./api";
import { previewWorkspace } from "./data/preview";
import {
  CANONICAL_TAXONOMY,
  canonicalTaxonomyDescriptor,
} from "./lib/canonical-taxonomy";
import type { CareerTrackPolicyAuthority } from "./types";

class MemoryStorage implements Storage {
  private readonly values = new Map<string, string>();

  get length(): number {
    return this.values.size;
  }

  clear(): void {
    this.values.clear();
  }

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  key(index: number): string | null {
    return [...this.values.keys()][index] ?? null;
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

function communicationActionResponse(status = "awaiting_approval") {
  const executionAvailable = status === "awaiting_approval" || status === "approved";
  const createdAtMs = 1_786_000_000_000;
  const updatedAtMs = status === "awaiting_approval" ? createdAtMs : createdAtMs + 1_000;
  return {
    id: "action-one",
    application_id: "application/one",
    connection_id: "connection-one",
    source_message_id: "message-one",
    kind: "reply",
    provider: "gmail",
    payload_sha256: "a".repeat(64),
    action_revision: status === "awaiting_approval" ? 1 : 2,
    status,
    execution_available: executionAvailable,
    execution_unavailable_reason: executionAvailable
      ? ""
      : "This communication is not awaiting executable approval.",
    approved_at_ms: status === "approved" ? updatedAtMs : null,
    dispatched_at_ms: null,
    created_at_ms: createdAtMs,
    updated_at_ms: updatedAtMs,
    connection_account_label: "candidate@gmail.com",
    source_context: {
      sender: "recruiter@example.org",
      reply_target: "recruiter@example.org",
      subject: "Interview availability",
      received_at_ms: 1_785_999_000_000,
    },
    payload: {
      to: "recruiter@example.org",
      subject: "Interview availability",
      body_text: "Tuesday afternoon works for me.",
    },
  };
}

async function currentTaxonomyHttpResponse() {
  const descriptor = await canonicalTaxonomyDescriptor();
  if (!descriptor.digest_sha256) throw new Error("canonical taxonomy digest is unavailable");
  return {
    taxonomyVersion: descriptor.taxonomy_version,
    taxonomySha256: descriptor.digest_sha256,
    registry: CANONICAL_TAXONOMY,
  };
}

function policyAuthority(applicationIdentityId: string): CareerTrackPolicyAuthority {
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
    source_resume_asset_id: "resume-one",
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

describe("Jobs API authentication", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.stubGlobal("localStorage", new MemoryStorage());
    vi.stubGlobal("sessionStorage", new MemoryStorage());
  });

  it("deduplicates concurrent refreshes for the initial workspace load", async () => {
    localStorage.setItem("bluey_access_token", "dummy-expired-access-token");
    localStorage.setItem("bluey_refresh_token", "dummy-refresh-token-one");
    let refreshCalls = 0;
    const authorizationHeaders: string[] = [];

    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/auth/refresh") {
        refreshCalls += 1;
        await Promise.resolve();
        return Response.json({
          access_token: "dummy-fresh-access-token",
          refresh_token: "dummy-refresh-token-two",
        });
      }

      const authorization = new Headers(init?.headers).get("Authorization") ?? "";
      authorizationHeaders.push(`${path}:${authorization}`);
      if (authorization !== "Bearer dummy-fresh-access-token") {
        return Response.json({ message: "expired" }, { status: 401 });
      }
      if (path === "/api/jobs/workspace") return Response.json({ profile: {} });
      if (path === "/account/me") return Response.json({ email: "user@example.com", balance_cents: 0 });
      return Response.json({ message: "not found" }, { status: 404 });
    }));

    const [workspace, account] = await Promise.all([jobsApi.workspace(), jobsApi.account()]);

    expect(refreshCalls).toBe(1);
    expect(workspace).toEqual({ profile: {} });
    expect(account.email).toBe("user@example.com");
    expect(localStorage.getItem("bluey_access_token")).toBe("dummy-fresh-access-token");
    expect(localStorage.getItem("bluey_refresh_token")).toBe("dummy-refresh-token-two");
    expect(authorizationHeaders).toEqual(expect.arrayContaining([
      "/api/jobs/workspace:Bearer dummy-expired-access-token",
      "/account/me:Bearer dummy-expired-access-token",
      "/api/jobs/workspace:Bearer dummy-fresh-access-token",
      "/account/me:Bearer dummy-fresh-access-token",
    ]));
  });

  it("maps and defaults the server policy identity binding at the API boundary", async () => {
    const firstAuthority = policyAuthority("identity-primary");
    const secondAuthority = policyAuthority("");
    const { applicationIdentityId: firstIdentityId, ...firstWireAuthority } = firstAuthority;
    const { applicationIdentityId: _missingIdentityId, ...secondWireAuthority } = secondAuthority;
    const wireWorkspace = {
      ...previewWorkspace,
      tracks: previewWorkspace.tracks.map((track, index) => ({
        ...track,
        policy: {
          ...track.policy,
          authority: index === 0
            ? { ...firstWireAuthority, application_identity_id: firstIdentityId }
            : secondWireAuthority,
        },
      })),
    };
    vi.stubGlobal("fetch", vi.fn(async () => Response.json(wireWorkspace)));

    const workspace = await jobsApi.workspace();

    expect(workspace.tracks[0].policy.authority?.applicationIdentityId)
      .toBe("identity-primary");
    expect(workspace.tracks[0].policy.authority).not.toHaveProperty(
      "application_identity_id",
    );
    expect(workspace.tracks[1].policy.authority?.applicationIdentityId).toBe("");
  });

  it("validates the authenticated taxonomy before a Track write and sends its exact binding", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const taxonomy = await currentTaxonomyHttpResponse();
    const track = {
      ...previewWorkspace.tracks[0],
      id: "track-client-created",
      created_at_ms: 0,
      policy: {
        ...previewWorkspace.tracks[0].policy,
        authority: policyAuthority("identity-primary"),
      },
    };
    const savedTrack = { ...track };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => (
      String(input) === "/api/jobs/taxonomy"
        ? Response.json(taxonomy)
        : Response.json(savedTrack)
    ));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.saveTrack(track)).resolves.toEqual(savedTrack);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/taxonomy",
      "/api/jobs/tracks",
    ]);
    expect(calls[0][1].method).toBeUndefined();
    expect(calls[1][1].method).toBe("POST");
    expect(new Headers(calls[0][1].headers).get("Authorization")).toBe(
      "Bearer dummy-access-token",
    );
    const writeHeaders = new Headers(calls[1][1].headers);
    expect(writeHeaders.get("Authorization")).toBe("Bearer dummy-access-token");
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-Version")).toBe(
      taxonomy.taxonomyVersion,
    );
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-SHA256")).toBe(
      taxonomy.taxonomySha256,
    );
    const writtenTrack = JSON.parse(String(calls[1][1].body)) as {
      policy: { authority: Record<string, unknown> };
    };
    expect(writtenTrack.policy.authority.application_identity_id).toBe("identity-primary");
    expect(writtenTrack.policy.authority).not.toHaveProperty("applicationIdentityId");
  });

  it("validates the authenticated taxonomy before the onboarding write", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const taxonomy = await currentTaxonomyHttpResponse();
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => (
      String(input) === "/api/jobs/taxonomy"
        ? Response.json(taxonomy)
        : Response.json(previewWorkspace)
    ));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.completeOnboarding(
      previewWorkspace.profile,
      previewWorkspace.preferences,
      previewWorkspace.tracks[0],
    )).resolves.toEqual(previewWorkspace);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/taxonomy",
      "/api/jobs/onboarding/complete",
    ]);
    expect(calls[1][1].method).toBe("POST");
    const writeHeaders = new Headers(calls[1][1].headers);
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-Version")).toBe(
      taxonomy.taxonomyVersion,
    );
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-SHA256")).toBe(
      taxonomy.taxonomySha256,
    );
  });

  it.each([
    {
      label: "Track",
      write: () => jobsApi.saveTrack({
        ...previewWorkspace.tracks[0],
        id: "track-client-created",
        created_at_ms: 0,
      }),
    },
    {
      label: "onboarding",
      write: () => jobsApi.completeOnboarding(
        previewWorkspace.profile,
        previewWorkspace.preferences,
        previewWorkspace.tracks[0],
      ),
    },
  ])("refuses stale or malformed taxonomy before the $label write", async ({ write }) => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const taxonomy = await currentTaxonomyHttpResponse();
    for (const invalidTaxonomy of [
      { ...taxonomy, taxonomySha256: "0".repeat(64) },
      { ...taxonomy, registry: {} },
    ]) {
      const fetchMock = vi.fn(async (_input: RequestInfo | URL) => (
        Response.json(invalidTaxonomy)
      ));
      vi.stubGlobal("fetch", fetchMock);

      await expect(write()).rejects.toMatchObject({ status: 409 });
      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(String(fetchMock.mock.calls[0][0])).toBe("/api/jobs/taxonomy");
    }
  });

  it("refreshes authentication on the taxonomy read before issuing the bound Track write", async () => {
    localStorage.setItem("bluey_access_token", "dummy-expired-access-token");
    localStorage.setItem("bluey_refresh_token", "dummy-refresh-token-one");
    const taxonomy = await currentTaxonomyHttpResponse();
    const track = {
      ...previewWorkspace.tracks[0],
      id: "track-client-created",
      created_at_ms: 0,
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/auth/refresh") {
        return Response.json({
          access_token: "dummy-fresh-access-token",
          refresh_token: "dummy-refresh-token-two",
        });
      }
      const authorization = new Headers(init?.headers).get("Authorization");
      if (authorization !== "Bearer dummy-fresh-access-token") {
        return Response.json({ message: "expired" }, { status: 401 });
      }
      return path === "/api/jobs/taxonomy"
        ? Response.json(taxonomy)
        : Response.json(track);
    });
    vi.stubGlobal("fetch", fetchMock);

    await jobsApi.saveTrack(track);

    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/taxonomy",
      "/auth/refresh",
      "/api/jobs/taxonomy",
      "/api/jobs/tracks",
    ]);
    expect(new Headers(calls[3][1].headers).get("Authorization")).toBe(
      "Bearer dummy-fresh-access-token",
    );
    expect(new Headers(calls[3][1].headers).get("X-Bluey-Jobs-Taxonomy-SHA256")).toBe(
      taxonomy.taxonomySha256,
    );
  });

  it("sends the narrow owner confirmation for an uncertain submission", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => Response.json({
      id: "application-one",
      job_id: "job-one",
      state: "failed",
    }));
    vi.stubGlobal("fetch", fetchMock);

    await jobsApi.reconcileSubmissionNotSubmitted("application-one");

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/jobs/applications/application-one/reconcile-submission");
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toEqual({ outcome: "not_submitted", confirmed: true });
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
  });

  it("downloads account-scoped application evidence with owner authentication", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => new Response("immutable evidence", {
      headers: {
        "Content-Disposition": "attachment; filename=\"application-receipt.json\"",
        "Content-Type": "application/json",
      },
    }));
    vi.stubGlobal("fetch", fetchMock);

    const downloaded = await jobsApi.downloadApplicationEvidence(
      "application/one",
      "evidence two",
      "fallback.json",
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe(
      "/api/jobs/applications/application%2Fone/evidence/evidence%20two/download",
    );
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
    expect(downloaded.fileName).toBe("application-receipt.json");
    expect(await downloaded.blob.text()).toBe("immutable evidence");
  });

  it("binds a stable client request id to a resume source upload", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => Response.json({
      asset: { id: "00000000-0000-4000-8000-000000000001" },
      profile: { full_name: "Ada Lovelace" },
    }));
    vi.stubGlobal("fetch", fetchMock);
    const file = new File(["exact resume"], "resume.txt", { type: "text/plain" });

    await jobsApi.uploadResumeSource(
      file,
      previewWorkspace.profile,
      undefined,
      "00000000-0000-4000-8000-000000000001",
    );

    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/jobs/resume-source");
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toMatchObject({
      request_id: "00000000-0000-4000-8000-000000000001",
      file_name: "resume.txt",
      media_type: "text/plain",
    });
  });

  it("lists privacy-safe communication summaries for the exact application", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const {
      payload: _payload,
      connection_account_label: _connectionAccountLabel,
      source_context: _sourceContext,
      ...summary
    } = communicationActionResponse();
    expect(_payload).toBeDefined();
    expect(_connectionAccountLabel).toBeDefined();
    expect(_sourceContext).toBeDefined();
    const fetchMock = vi.fn(async () => Response.json([summary]));
    vi.stubGlobal("fetch", fetchMock);

    const actions = await jobsApi.communicationActions("application/one", 500);

    expect(actions).toEqual([summary]);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe(
      "/api/jobs/communication-actions?limit=100&application_id=application%2Fone",
    );
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
    expect(init.cache).toBe("no-store");
  });

  it("loads, approves, and cancels the exact encoded communication action", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input);
      if (path.endsWith("/approve")) return Response.json(communicationActionResponse("approved"));
      if (path.endsWith("/cancel")) return Response.json(communicationActionResponse("cancelled"));
      return Response.json(communicationActionResponse());
    });
    vi.stubGlobal("fetch", fetchMock);

    const action = { ...communicationActionResponse(), id: "action/one" };
    await jobsApi.communicationAction("action/one");
    await jobsApi.approveCommunicationAction(action);
    await jobsApi.cancelCommunicationAction(action);

    expect(fetchMock).toHaveBeenCalledTimes(3);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/communication-actions/action%2Fone",
      "/api/jobs/communication-actions/action%2Fone/approve",
      "/api/jobs/communication-actions/action%2Fone/cancel",
    ]);
    expect(calls[0][1].method).toBeUndefined();
    expect(calls[0][1].cache).toBe("no-store");
    expect(calls[1][1].method).toBe("POST");
    expect(calls[1][1].cache).toBe("no-store");
    expect(calls[2][1].method).toBe("POST");
    expect(calls[2][1].cache).toBe("no-store");
    expect(JSON.parse(String(calls[1][1].body))).toEqual({
      action_revision: action.action_revision,
      payload_sha256: action.payload_sha256,
    });
    expect(JSON.parse(String(calls[2][1].body))).toEqual({
      action_revision: action.action_revision,
      payload_sha256: action.payload_sha256,
    });
    expect(calls.every(([, init]) => (
      new Headers(init.headers).get("Authorization") === "Bearer dummy-access-token"
    ))).toBe(true);
  });

  it("starts explicit communication authorization for the exact connected account", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => Response.json({
      authorization_url: "https://accounts.example.test/communication-consent",
    }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await jobsApi.startMailboxCommunicationAuthorization("connection/one");

    expect(result.authorization_url).toContain("communication-consent");
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe(
      "/api/jobs/mailbox-connections/connection%2Fone/communication-authorization/start",
    );
    expect(init.method).toBe("POST");
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
  });

  it("runtime-decodes mailbox provider availability and OAuth start envelopes", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const providers = [
      {
        provider: "gmail",
        configured: true,
        capabilities: [
          "status_sync",
          "application_correlation",
          "review_interventions",
        ],
      },
      {
        provider: "outlook",
        configured: true,
        capabilities: [
          "status_sync",
          "application_correlation",
          "review_interventions",
          "recruiter_reply",
          "interview_calendar",
        ],
      },
    ];
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => (
      String(input).endsWith("/config")
        ? Response.json(providers)
        : Response.json({ authorization_url: "https://accounts.google.com/o/oauth2/v2/auth" })
    )));

    await expect(jobsApi.mailboxOAuthProviders()).resolves.toEqual(providers);
    await expect(jobsApi.startMailboxOAuth("gmail")).resolves.toEqual({
      authorization_url: "https://accounts.google.com/o/oauth2/v2/auth",
    });
  });

  it("fails closed on malformed mailbox OAuth response envelopes", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => (
      String(input).endsWith("/config")
        ? Response.json([{ provider: "gmail", configured: true, capabilities: [] }])
        : Response.json({
            authorization_url: "https://accounts.google.com/o/oauth2/v2/auth",
            access_token: "secret",
          })
    )));

    await expect(jobsApi.mailboxOAuthProviders()).rejects.toThrow("could not verify");
    await expect(jobsApi.startMailboxOAuth("gmail")).rejects.toThrow("could not verify");
  });

  it("fails closed when a communication response omits execution readiness", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const malformed = communicationActionResponse();
    delete (malformed as Partial<typeof malformed>).execution_available;
    vi.stubGlobal("fetch", vi.fn(async () => Response.json(malformed)));

    await expect(jobsApi.communicationAction("action-one")).rejects.toThrow(
      "could not verify this reviewed communication action",
    );
  });
});
