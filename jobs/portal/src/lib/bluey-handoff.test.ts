import { describe, expect, it } from "vitest";
import { validatedBlueyHandoffUrl } from "./bluey-handoff";
import type { JobsBlueyHandoffIssueResponse } from "../types";

const nonce = "A".repeat(43);

function response(deepLink = `bluey://jobs/interview-prep?nonce=${nonce}`): JobsBlueyHandoffIssueResponse {
  return {
    schema_version: 1,
    audience: "bluey-desktop-interview-prep-v1",
    nonce,
    deep_link_url: deepLink,
    expires_at_ms: Date.now() + 90_000,
    expires_in_seconds: 90,
  };
}

describe("Bluey desktop handoff URL", () => {
  it("accepts the nonce-only first-party custom URL", () => {
    expect(validatedBlueyHandoffUrl(response())).toBe(
      `bluey://jobs/interview-prep?nonce=${nonce}`,
    );
  });

  it.each([
    `bluey://jobs/interview-prep?nonce=${nonce}&application_id=application-1`,
    `bluey://jobs/interview-prep?nonce=${nonce}&token=secret`,
    `bluey://jobs/interview-prep?nonce=${nonce}#receipt-1`,
    `bluey://link?nonce=${nonce}`,
    `https://bluey.sh/interview-prep?nonce=${nonce}`,
  ])("rejects URLs containing anything beyond the scoped nonce", (deepLink) => {
    expect(() => validatedBlueyHandoffUrl(response(deepLink))).toThrow("invalid");
  });

  it("rejects a URL nonce that differs from the issued nonce", () => {
    const other = "B".repeat(43);
    expect(() => validatedBlueyHandoffUrl(response(
      `bluey://jobs/interview-prep?nonce=${other}`,
    ))).toThrow("invalid");
  });

  it("rejects a handoff lifetime outside the server bound", () => {
    expect(() => validatedBlueyHandoffUrl({
      ...response(),
      expires_in_seconds: 91,
    })).toThrow("invalid");
  });
});
