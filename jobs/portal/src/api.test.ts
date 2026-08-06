import { beforeEach, describe, expect, it, vi } from "vitest";
import { jobsApi } from "./api";
import { previewWorkspace } from "./data/preview";

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
