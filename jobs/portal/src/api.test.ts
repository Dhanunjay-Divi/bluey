import { beforeEach, describe, expect, it, vi } from "vitest";
import { jobsApi } from "./api";

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
});
