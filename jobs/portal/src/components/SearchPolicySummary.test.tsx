import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { previewWorkspace } from "../data/preview";
import { SearchPolicySummary } from "./SearchPolicySummary";

describe("SearchPolicySummary", () => {
  it("labels custom-role experience as review-required instead of authoritative", () => {
    const markup = renderToStaticMarkup(
      <SearchPolicySummary
        profile={previewWorkspace.profile}
        role="Clinical AI Workflow Specialist"
      />,
    );

    expect(markup).toContain(
      "Review required before Bluey sets an experience range for Clinical AI Workflow Specialist",
    );
    expect(markup).not.toMatch(/\d+-\d+ years for Clinical AI Workflow Specialist/);
  });
});
