import { describe, expect, it } from "vitest";
import {
  createDefaultAdapterRegistry,
  submissionPolicy,
} from "@bluey/jobs-automation";

describe("cloud runner ATS target reachability", () => {
  it("routes an exact Lever EU application to the provider state machine", () => {
    const url = "https://jobs.eu.lever.co/atlas/posting-123/apply";
    expect(submissionPolicy(url)).toMatchObject({
      policy: "automate",
      capability: "beta_review",
    });
    expect(createDefaultAdapterRegistry().resolve(url).kind).toBe("lever");
  });

  it("keeps spoofed Lever EU targets out of cloud automation", () => {
    for (const url of [
      "https://jobs.eu.lever.co.attacker.example/atlas/posting-123/apply",
      "https://candidate:secret@jobs.eu.lever.co/atlas/posting-123/apply",
      "https://jobs.eu.lever.co/atlas//posting-123/apply",
    ]) {
      expect(submissionPolicy(url).policy).toBe("handoff");
      expect(createDefaultAdapterRegistry().resolve(url).kind).toBe("semantic");
    }
  });
});
