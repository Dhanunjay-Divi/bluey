import { describe, expect, it } from "vitest";
import { detectAts, submissionPolicy } from "../src/index.js";

describe("Jobs submission policy", () => {
  it("hands LinkedIn and Indeed back to the user", () => {
    expect(submissionPolicy("https://www.linkedin.com/jobs/view/1").policy).toBe("handoff");
    expect(submissionPolicy("https://indeed.com/viewjob?jk=1").policy).toBe("handoff");
  });

  it("blocks private-network and non-http targets", () => {
    expect(submissionPolicy("http://127.0.0.1/internal").policy).toBe("blocked");
    expect(submissionPolicy("file:///etc/passwd").policy).toBe("blocked");
  });

  it("allows direct employer forms", () => {
    expect(submissionPolicy("https://jobs.acme.com/engineer").policy).toBe("automate");
  });
});

describe("ATS detection", () => {
  it("selects deterministic adapters before semantic fallback", () => {
    expect(detectAts("https://acme.wd5.myworkdayjobs.com/en-US/jobs/job/1")).toBe("workday");
    expect(detectAts("https://boards.greenhouse.io/acme/jobs/1")).toBe("greenhouse");
    expect(detectAts("https://jobs.lever.co/acme/abc")).toBe("lever");
    expect(detectAts("https://jobs.ashbyhq.com/acme/abc")).toBe("ashby");
    expect(detectAts("https://jobs.smartrecruiters.com/Acme/1")).toBe("smartrecruiters");
    expect(detectAts("https://jobs.acme.com/roles/1")).toBe("semantic");
  });
});
