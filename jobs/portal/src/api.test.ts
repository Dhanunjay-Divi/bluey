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
    localStorage.setItem("bluey_access_token", "expired-access");
    localStorage.setItem("bluey_refresh_token", "refresh-one");
    let refreshCalls = 0;
    const authorizationHeaders: string[] = [];

    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/auth/refresh") {
        refreshCalls += 1;
        await Promise.resolve();
        return Response.json({ access_token: "fresh-access", refresh_token: "refresh-two" });
      }

      const authorization = new Headers(init?.headers).get("Authorization") ?? "";
      authorizationHeaders.push(`${path}:${authorization}`);
      if (authorization !== "Bearer fresh-access") {
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
    expect(localStorage.getItem("bluey_access_token")).toBe("fresh-access");
    expect(localStorage.getItem("bluey_refresh_token")).toBe("refresh-two");
    expect(authorizationHeaders).toEqual(expect.arrayContaining([
      "/api/jobs/workspace:Bearer expired-access",
      "/account/me:Bearer expired-access",
      "/api/jobs/workspace:Bearer fresh-access",
      "/account/me:Bearer fresh-access",
    ]));
  });
});
