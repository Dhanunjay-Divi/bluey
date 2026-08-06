import { describe, expect, it } from "vitest";
import {
  adapterCanFinalize,
  detectAts,
  submissionPolicy,
} from "../src/index.js";

describe("Jobs submission policy", () => {
  it("hands LinkedIn and Indeed back to the user", () => {
    expect(
      submissionPolicy("https://www.linkedin.com/jobs/view/1").policy,
    ).toBe("handoff");
    expect(submissionPolicy("https://indeed.com/viewjob?jk=1").policy).toBe(
      "handoff",
    );
  });

  it("blocks private-network and non-http targets", () => {
    expect(submissionPolicy("http://127.0.0.1/internal").policy).toBe(
      "blocked",
    );
    expect(submissionPolicy("file:///etc/passwd").policy).toBe("blocked");
  });

  it("allows only provider-specific reviewed runners and keeps other forms review-only", () => {
    expect(
      submissionPolicy("https://boards.greenhouse.io/acme/jobs/1"),
    ).toMatchObject({
      policy: "automate",
      capability: "beta_review",
    });
    expect(submissionPolicy("https://jobs.lever.co/acme/1")).toMatchObject({
      policy: "automate",
      capability: "beta_review",
    });
    expect(
      submissionPolicy("https://jobs.eu.lever.co/acme/1/apply"),
    ).toMatchObject({
      policy: "automate",
      capability: "beta_review",
    });
    for (const url of [
      "https://acme.wd5.myworkdayjobs.com/en-US/jobs/job/1",
      "https://jobs.ashbyhq.com/acme/1",
      "https://jobs.smartrecruiters.com/Acme/1",
    ]) {
      expect(submissionPolicy(url)).toMatchObject({
        policy: "handoff",
        capability: "unknown_review",
      });
    }
    expect(submissionPolicy("https://jobs.acme.com/engineer")).toMatchObject({
      policy: "handoff",
      capability: "unknown_review",
    });
  });

  it("does not let hostname or query-string lookalikes inherit ATS authority", () => {
    for (const url of [
      "https://boards.greenhouse.io.attacker.example/jobs/1",
      "https://evil.example/jobs?next=https://jobs.lever.co/acme/1",
      "https://notindeed.com/viewjob/1",
    ]) {
      expect(submissionPolicy(url)).toMatchObject({
        policy: "handoff",
        capability: "unknown_review",
      });
    }
  });

  it("binds final-submit authority to the exact provider adapter version", () => {
    expect(adapterCanFinalize("greenhouse", "2026.07.1-beta.1")).toBe(true);
    expect(adapterCanFinalize("lever", "2026.07.0-beta.1")).toBe(true);
    expect(adapterCanFinalize("greenhouse", "2026.07.1")).toBe(false);
    expect(adapterCanFinalize("workday", "2026.07.1")).toBe(false);
    expect(adapterCanFinalize("ashby", "2026.07.1")).toBe(false);
    expect(adapterCanFinalize("smartrecruiters", "2026.07.1")).toBe(false);
    expect(adapterCanFinalize("semantic", "2026.07.1")).toBe(false);
  });
});

describe("ATS detection", () => {
  it("selects deterministic adapters before semantic fallback", () => {
    expect(
      detectAts("https://acme.wd5.myworkdayjobs.com/en-US/jobs/job/1"),
    ).toBe("workday");
    expect(detectAts("https://boards.greenhouse.io/acme/jobs/1")).toBe(
      "greenhouse",
    );
    expect(detectAts("https://jobs.lever.co/acme/abc")).toBe("lever");
    expect(detectAts("https://jobs.eu.lever.co/acme/abc/apply")).toBe(
      "lever",
    );
    expect(detectAts("https://jobs.ashbyhq.com/acme/abc")).toBe("ashby");
    expect(detectAts("https://jobs.smartrecruiters.com/Acme/1")).toBe(
      "smartrecruiters",
    );
    expect(detectAts("https://jobs.acme.com/roles/1")).toBe("semantic");
  });
});
