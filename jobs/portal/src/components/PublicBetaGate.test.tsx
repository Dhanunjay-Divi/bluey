import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { JobsBetaAccess, JobsBetaAccessReason } from "../api";
import { PublicBetaGate } from "./PublicBetaGate";

type BlockedAccess = Exclude<JobsBetaAccess, { access: "admitted" }>;

function blocked(
  reason: Exclude<JobsBetaAccessReason, "admitted">,
): BlockedAccess {
  return {
    schemaVersion: 1,
    access: reason === "suspended" ? "suspended" : "not_admitted",
    reason,
  } as BlockedAccess;
}

function render(reason: Exclude<JobsBetaAccessReason, "admitted">): string {
  return renderToStaticMarkup(
    <PublicBetaGate
      betaAccess={blocked(reason)}
      onRetry={() => undefined}
      onSignOut={() => undefined}
    />,
  );
}

describe("PublicBetaGate", () => {
  it.each([
    ["verification_required", "Verify your Bluey account"],
    ["not_open", "Public beta enrollment is not open yet"],
    ["window_closed", "Public beta enrollment is closed"],
    ["capacity_reached", "The public beta is full"],
    ["denied", "Bluey Jobs access is unavailable"],
    ["suspended", "Bluey Jobs is temporarily paused"],
    ["unavailable", "Bluey Jobs is temporarily unavailable"],
  ] as const)(
    "renders the %s state with accessible recovery actions",
    (reason, title) => {
      const html = render(reason);

      expect(html).toContain(`id="public-beta-title">${title}`);
      expect(html).toContain("Try again");
      expect(html).toContain("Sign out");
      expect(html).toContain("Back to Bluey");
      expect(html).toContain('aria-labelledby="public-beta-title"');
      expect(html).toContain('href="/"');
    },
  );

  it("routes verification-required users to their core Bluey account", () => {
    const html = render("verification_required");

    expect(html).toContain("Open Bluey account");
    expect(html).toContain('href="/account"');
  });

  it("does not describe access as invitation-only or expose internal cohort details", () => {
    for (const reason of [
      "verification_required",
      "not_open",
      "window_closed",
      "capacity_reached",
      "denied",
      "suspended",
      "unavailable",
    ] as const) {
      const html = render(reason).toLowerCase();
      expect(html).not.toContain("invitation-only");
      expect(html).not.toContain("invite only");
      expect(html).not.toContain("assigned_count");
      expect(html).not.toContain("hard_cap");
      expect(html).not.toContain("cohort revision");
      expect(html).not.toContain("account id");
      expect(html).not.toContain("queue position");
    }
  });
});
