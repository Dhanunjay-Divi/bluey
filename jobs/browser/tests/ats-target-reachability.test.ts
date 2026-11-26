import { describe, expect, it } from "vitest";
import {
  createDefaultAdapterRegistry,
  submissionPolicy,
} from "@bluey/jobs-automation";

describe("local Browser ATS target reachability", () => {
  it("routes an exact Lever EU application to the provider state machine", () => {
    const url = "https://jobs.eu.lever.co/atlas/posting-123/apply";
    expect(submissionPolicy(url)).toMatchObject({
      policy: "automate",
      capability: "beta_review",
    });
    expect(createDefaultAdapterRegistry().resolve(url).kind).toBe("lever");
  });

  it("keeps malformed Lever EU targets out of local automation", () => {
    for (const url of [
      "http://jobs.eu.lever.co/atlas/posting-123/apply",
      "https://jobs.eu.lever.co:8443/atlas/posting-123/apply",
      "https://jobs.eu.lever.co/atlas/posting-123/edit",
    ]) {
      expect(submissionPolicy(url).policy).toBe("handoff");
      expect(createDefaultAdapterRegistry().resolve(url).kind).toBe("semantic");
    }
  });
});
