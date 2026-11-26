const ACCESS_TOKEN_KEY = "bluey_access_token";
const AUTH_PERSISTENCE_KEY = "bluey_auth_persistence";

export function tokenStores(): Storage[] {
  return localStorage.getItem(AUTH_PERSISTENCE_KEY) === "session"
    ? [sessionStorage, localStorage]
    : [localStorage, sessionStorage];
}

export function accessToken(): string {
  try {
    for (const store of tokenStores()) {
      const token = store.getItem(ACCESS_TOKEN_KEY);
      if (token) return token;
    }
  } catch {
    // Storage denial must leave the public entry usable; it never grants access.
    return "";
  }
  return "";
}

export function loginUrl(): string {
  return "/login?next=%2Fjobs";
}

export function shouldLoadWorkspace(search: string): boolean {
  return new URLSearchParams(search).get("preview") === "1" || accessToken().length > 0;
}

export function startupMode(search: string, hasPrerenderedPublicRoot: boolean): "app" | "public" | "none" {
  if (shouldLoadWorkspace(search)) return "app";
  return hasPrerenderedPublicRoot ? "none" : "public";
}

export function initialTheme(): "light" | "dark" {
  try {
    const saved = localStorage.getItem("bluey_jobs_theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch { /* fall through to system preference */ }
  try {
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  } catch {
    return "dark";
  }
}
