import { describe, expect, it } from "vitest";
import {
  findJobSourceByName,
  findJobSourceByUrl,
  JOB_SOURCE_CATALOG,
  requiresCanonicalEmployerRevalidation,
} from "../src/source-catalog.js";

describe("job source catalog", () => {
  it("deduplicates the supplied staffing catalog by stable source ID", () => {
    expect(new Set(JOB_SOURCE_CATALOG.map((entry) => entry.id)).size).toBe(JOB_SOURCE_CATALOG.length);
    expect(JOB_SOURCE_CATALOG.filter((entry) => entry.name === "Business Plan Solutions")).toHaveLength(1);
  });

  it("normalizes staffing aliases and domains", () => {
    expect(findJobSourceByName("Apex System")?.name).toBe("Apex Systems");
    expect(findJobSourceByName("Tek Systems")?.name).toBe("TEKsystems");
    expect(findJobSourceByUrl("https://careers.randstadusa.com/job/1")?.id).toBe("staffing-randstad");
  });

  it("keeps portals and curated repositories as leads or handoffs", () => {
    expect(findJobSourceByUrl("https://www.linkedin.com/jobs/view/1")?.submissionCapability).toBe("handoff");
    const feed = findJobSourceByName("SimplifyJobs/New-Grad-Positions");
    expect(feed?.discoveryCapability).toBe("candidate_lead");
    expect(feed && requiresCanonicalEmployerRevalidation(feed)).toBe(true);
    expect(findJobSourceByUrl("https://github.com/PrepAIJobs/Summer2026-Internships/tree/main")?.id)
      .toBe("feed-prepai-internships");
    expect(findJobSourceByUrl("https://github.com/zapplyjobs/New-Grad-Jobs-2027")?.id)
      .toBe("feed-zapply-new-grad");
    expect(findJobSourceByUrl("https://github.com/unlisted/repository")).toBeUndefined();
  });

  it("allows scheduled ingestion only for typed public ATS families", () => {
    const scheduled = JOB_SOURCE_CATALOG.filter((entry) => entry.discoveryCapability === "scheduled_public_feed");
    expect(scheduled.map((entry) => entry.name)).toEqual([
      "Greenhouse",
      "Lever",
      "Ashby",
      "SmartRecruiters",
      "Workday",
    ]);
    expect(scheduled.every((entry) => !entry.requiresCanonicalRevalidation)).toBe(true);
  });
});
