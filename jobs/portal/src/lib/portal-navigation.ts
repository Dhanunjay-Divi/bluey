const PREVIEW_SCENARIOS = new Set([
  "final-review",
  "many-matches",
  "onboarding",
  "runner-beta",
  "stale-discovery",
]);

export interface PortalPreviewState {
  enabled: boolean;
  scenario: string;
  search: string;
}

export function portalPreviewState(search: string): PortalPreviewState {
  const query = new URLSearchParams(search);
  if (query.get("preview") !== "1") {
    return { enabled: false, scenario: "", search: "" };
  }
  const requestedScenario = query.get("scenario") || "";
  const scenario = PREVIEW_SCENARIOS.has(requestedScenario) ? requestedScenario : "";
  const normalized = new URLSearchParams({
    preview: "1",
    ...(scenario ? { scenario } : {}),
  });
  return { enabled: true, scenario, search: `?${normalized}` };
}

export function automationRoute(previewSearch = ""): string {
  return `/automation${previewSearch}`;
}

export function canonicalLegacyAutomationUrl(
  pathname: string,
  search: string,
): string | undefined {
  if (pathname !== "/jobs/browser" && pathname !== "/jobs/browser/") return undefined;
  return `/jobs${automationRoute(portalPreviewState(search).search)}`;
}
