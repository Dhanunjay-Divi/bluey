import { describe, expect, it } from "vitest";
import { cloudSyncConsentCopy } from "./cloudSyncConsent";

describe("cloud sync consent copy", () => {
  it("never describes sign-in as permission to upload saved sessions", () => {
    expect(cloudSyncConsentCopy.signedOutAccount).toContain(
      "Signing in does not upload saved sessions",
    );
    expect(cloudSyncConsentCopy.dataControls).toContain(
      "does not turn uploads on",
    );
    expect(cloudSyncConsentCopy.signedOutBalance).toContain(
      "uploads remain off",
    );
  });

  it("names the checkbox as the explicit upload boundary", () => {
    expect(cloudSyncConsentCopy.toggle).toContain("When this checkbox is on");
    expect(cloudSyncConsentCopy.dataControls).toContain(
      "until you enable Cloud session sync",
    );
    expect(cloudSyncConsentCopy.signedOutBalance).toContain(
      "explicitly enable Cloud session sync",
    );
    expect(cloudSyncConsentCopy.dataControls).toContain("Each account stays off");
    expect(cloudSyncConsentCopy.toggle).toContain("only to this signed-in account");
    expect(cloudSyncConsentCopy.toggle).toContain("your questions");
    expect(cloudSyncConsentCopy.toggle).toContain("final answers");
    expect(cloudSyncConsentCopy.toggle).toContain("meeting transcripts");
    expect(cloudSyncConsentCopy.toggle).toContain("previously synced copies remain");
  });
});
