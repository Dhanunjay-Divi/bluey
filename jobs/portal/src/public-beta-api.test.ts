import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  jobsApi,
  loadJobsPortal,
  parseJobsBetaAccess,
  signOutOfBluey,
} from "./api";

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
describe("Jobs public-beta access", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.stubGlobal("localStorage", new MemoryStorage());
    vi.stubGlobal("sessionStorage", new MemoryStorage());
    localStorage.setItem("bluey_access_token", "dummy-access-token");
  });

  it("loads beta access before requesting any workspace data", async () => {
    const requests: string[] = [];
    let releaseBeta: () => void = () => undefined;
    const betaBarrier = new Promise<void>((resolve) => {
      releaseBeta = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input);
        requests.push(path);
        if (path === "/api/jobs/beta-access") {
          await betaBarrier;
          return Response.json({
            schemaVersion: 1,
            access: "admitted",
            reason: "admitted",
          });
        }
        if (path === "/api/jobs/workspace")
          return Response.json({ profile: {} });
        if (path === "/account/me") {
          return Response.json({ email: "user@example.com", balance_cents: 0 });
        }
        return Response.json({ message: "not found" }, { status: 404 });
      }),
    );

    const loading = loadJobsPortal();
    expect(requests).toEqual(["/api/jobs/beta-access"]);
    releaseBeta();
    const loaded = await loading;

    expect(requests).toEqual([
      "/api/jobs/beta-access",
      "/api/jobs/workspace",
      "/account/me",
    ]);
    expect(loaded.betaAccess.access).toBe("admitted");
    expect(loaded.workspace).toEqual({ profile: {} });
  });

  it("does not request workspace or account data when access is not admitted", async () => {
    const requests: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        requests.push(String(input));
        return Response.json({
          schemaVersion: 1,
          access: "not_admitted",
          reason: "capacity_reached",
        });
      }),
    );

    const loaded = await loadJobsPortal();

    expect(requests).toEqual(["/api/jobs/beta-access"]);
    expect(loaded.workspace).toBeNull();
    expect(loaded.account).toBeNull();
  });

  it("turns the closed 503 unavailable response into a fail-closed gate state", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json(
          {
            schemaVersion: 1,
            access: "not_admitted",
            reason: "unavailable",
          },
          { status: 503 },
        ),
      ),
    );

    const loaded = await loadJobsPortal();

    expect(loaded.betaAccess).toEqual({
      schemaVersion: 1,
      access: "not_admitted",
      reason: "unavailable",
    });
    expect(loaded.workspace).toBeNull();
  });

  it("bypasses the browser cache for every beta access decision", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) =>
        Response.json({
          schemaVersion: 1,
          access: "not_admitted",
          reason: "not_open",
        }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await jobsApi.betaAccess();

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(fetchMock.mock.calls[0][1]?.cache).toBe("no-store");
  });

  it("rejects response expansion, unknown values, and inconsistent state pairs", () => {
    expect(() =>
      parseJobsBetaAccess({
        schemaVersion: 1,
        access: "not_admitted",
        reason: "capacity_reached",
        assignedCount: 25,
      }),
    ).toThrow("could not safely confirm");
    expect(() =>
      parseJobsBetaAccess({
        schemaVersion: 1,
        access: "not_admitted",
        reason: "full",
      }),
    ).toThrow("could not safely confirm");
    expect(() =>
      parseJobsBetaAccess({
        schemaVersion: 1,
        access: "suspended",
        reason: "admitted",
      }),
    ).toThrow("could not safely confirm");
  });

  it("preserves the shared Bluey sign-out behavior from a beta gate", () => {
    localStorage.setItem("bluey_refresh_token", "dummy-refresh-token");
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) =>
        new Response(null, { status: 204 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("window", { location: { href: "/jobs" } });

    signOutOfBluey();

    expect(localStorage.getItem("bluey_access_token")).toBeNull();
    expect(localStorage.getItem("bluey_refresh_token")).toBeNull();
    expect(window.location.href).toBe("/");
    expect(fetchMock).toHaveBeenCalledOnce();
    const [path, init] = fetchMock.mock.calls[0];
    expect(path).toBe("/auth/logout");
    expect(new Headers(init?.headers).get("Authorization")).toBe(
      "Bearer dummy-access-token",
    );
    expect(init?.body).toBe(
      JSON.stringify({ refresh_token: "dummy-refresh-token" }),
    );
  });
});
