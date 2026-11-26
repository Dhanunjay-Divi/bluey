import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { previewWorkspace } from "../data/preview";
import { Onboarding } from "./Onboarding";

const renderDefaults = (reviewNewClaims: boolean) => renderToStaticMarkup(
  <Onboarding
    workspace={{
      ...previewWorkspace,
      profile: { ...previewWorkspace.profile, onboarding_step: 4, review_new_claims: reviewNewClaims },
    }}
    error=""
    onImportResume={async () => previewWorkspace.profile}
    onProgress={async () => {}}
    onComplete={async () => {}}
  />,
);

const switchTag = (markup: string) => {
  const match = markup.match(/<button\b[^>]*role="switch"[^>]*>/);
  expect(match).not.toBeNull();
  return match![0];
};

describe("Onboarding defaults switch", () => {
  it("names the review-new-claims switch and retains its false/true state", () => {
    const falseState = switchTag(renderDefaults(false));
    expect(falseState).toContain('aria-label="Review new claims"');
    expect(falseState).toContain('aria-checked="false"');

    const trueState = switchTag(renderDefaults(true));
    expect(trueState).toContain('aria-label="Review new claims"');
    expect(trueState).toContain('aria-checked="true"');
  });
});
