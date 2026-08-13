import { describe, expect, it } from "vitest";
import {
  automationRoute,
  canonicalLegacyAutomationUrl,
  portalPreviewState,
} from "./portal-navigation";

describe("Jobs portal navigation", () => {
  it("normalizes preview state to the closed scenario set", () => {
    expect(portalPreviewState("?preview=1&scenario=final-review&private=value")).toEqual({
      enabled: true,
      scenario: "final-review",
      search: "?preview=1&scenario=final-review",
    });
    expect(portalPreviewState("?preview=1&scenario=unknown&private=value")).toEqual({
      enabled: true,
      scenario: "",
      search: "?preview=1",
    });
    expect(portalPreviewState("?scenario=final-review")).toEqual({
      enabled: false,
      scenario: "",
      search: "",
    });
  });

  it("canonicalizes only the legacy Browser route before authentication", () => {
    expect(canonicalLegacyAutomationUrl(
      "/jobs/browser",
      "?preview=1&scenario=runner-beta&token=private",
    )).toBe("/jobs/automation?preview=1&scenario=runner-beta");
    expect(canonicalLegacyAutomationUrl("/jobs/browser/", "?unexpected=1"))
      .toBe("/jobs/automation");
    expect(canonicalLegacyAutomationUrl("/jobs/applications", "?preview=1"))
      .toBeUndefined();
  });

  it("builds the basename-relative Automation route", () => {
    expect(automationRoute("?preview=1&scenario=runner-beta"))
      .toBe("/automation?preview=1&scenario=runner-beta");
  });
});
