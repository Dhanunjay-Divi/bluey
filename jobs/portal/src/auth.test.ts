import { afterEach, describe, expect, it, vi } from "vitest";
import { accessToken, initialTheme, loginUrl, shouldLoadWorkspace, startupMode } from "./auth";
import { canonicalLegacyAutomationUrl } from "./lib/portal-navigation";

class MemoryStorage implements Storage {
  private readonly values = new Map<string, string>();
  get length() { return this.values.size; }
  clear() { this.values.clear(); }
  getItem(key: string) { return this.values.get(key) ?? null; }
  key(index: number) { return [...this.values.keys()][index] ?? null; }
  removeItem(key: string) { this.values.delete(key); }
  setItem(key: string, value: string) { this.values.set(key, value); }
}

afterEach(() => vi.unstubAllGlobals());

describe("lightweight Jobs authentication entry", () => {
  it("uses the persisted token first unless session persistence is selected", () => {
    const local = new MemoryStorage();
    const session = new MemoryStorage();
    local.setItem("bluey_access_token", "local-token");
    session.setItem("bluey_access_token", "session-token");
    vi.stubGlobal("localStorage", local);
    vi.stubGlobal("sessionStorage", session);
    expect(accessToken()).toBe("local-token");
    local.setItem("bluey_auth_persistence", "session");
    expect(accessToken()).toBe("session-token");
    session.removeItem("bluey_access_token");
    expect(accessToken()).toBe("local-token");
  });

  it("fails closed to public entry when browser storage is unavailable", () => {
    vi.stubGlobal("localStorage", { getItem: () => { throw new Error("blocked"); } });
    vi.stubGlobal("sessionStorage", new MemoryStorage());
    expect(accessToken()).toBe("");
  });

  it("uses a deterministic theme when storage and media preference are denied", () => {
    vi.stubGlobal("localStorage", { getItem: () => { throw new Error("blocked"); } });
    vi.stubGlobal("sessionStorage", new MemoryStorage());
    vi.stubGlobal("window", { matchMedia: () => { throw new Error("blocked"); } });
    expect(initialTheme()).toBe("dark");
  });

  it("prefers a saved theme, then the system light preference", () => {
    const local = new MemoryStorage();
    vi.stubGlobal("localStorage", local);
    vi.stubGlobal("sessionStorage", new MemoryStorage());
    vi.stubGlobal("window", { matchMedia: () => ({ matches: true }) });
    expect(initialTheme()).toBe("light");
    local.setItem("bluey_jobs_theme", "dark");
    expect(initialTheme()).toBe("dark");
  });

  it("preserves the existing Jobs login destination", () => {
    expect(loginUrl()).toBe("/login?next=%2Fjobs");
  });

  it("loads the workspace only for a token or explicit preview", () => {
    vi.stubGlobal("localStorage", new MemoryStorage());
    vi.stubGlobal("sessionStorage", new MemoryStorage());
    expect(shouldLoadWorkspace("?scenario=final-review")).toBe(false);
    expect(shouldLoadWorkspace("?preview=1&scenario=final-review")).toBe(true);
    localStorage.setItem("bluey_access_token", "token");
    expect(shouldLoadWorkspace("")).toBe(true);
  });

  it("leaves only the marked anonymous public root untouched", () => {
    vi.stubGlobal("localStorage", new MemoryStorage());
    vi.stubGlobal("sessionStorage", new MemoryStorage());
    expect(startupMode("", true)).toBe("none");
    expect(startupMode("", false)).toBe("public");
    expect(startupMode("?preview=1&scenario=runner-beta", true)).toBe("app");
    localStorage.setItem("bluey_access_token", "token");
    expect(startupMode("", true)).toBe("app");
  });

  it("keeps the legacy Browser redirect and preview scenario query intact", () => {
    expect(canonicalLegacyAutomationUrl("/jobs/browser", "?preview=1&scenario=runner-beta"))
      .toBe("/jobs/automation?preview=1&scenario=runner-beta");
  });
});
