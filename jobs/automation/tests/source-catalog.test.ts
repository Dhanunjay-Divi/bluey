import { describe, expect, it } from "vitest";
import {
  findJobSourceByName,
  findJobSourceByUrl,
  JOB_SOURCE_CATALOG,
  requiresCanonicalEmployerRevalidation,
} from "../src/source-catalog.js";

describe("job source catalog", () => {
  it("deduplicates the supplied staffing catalog by stable source ID", () => {
    expect(new Set(JOB_SOURCE_CATALOG.map((entry) => entry.id)).size).toBe(
      JOB_SOURCE_CATALOG.length,
    );
    expect(
      JOB_SOURCE_CATALOG.filter(
        (entry) => entry.name === "Business Plan Solutions",
      ),
    ).toHaveLength(1);
  });

  it("normalizes staffing aliases and domains", () => {
    expect(findJobSourceByName("Apex System")?.name).toBe("Apex Systems");
    expect(findJobSourceByName("Tek Systems")?.name).toBe("TEKsystems");
    expect(
      findJobSourceByUrl("https://careers.randstadusa.com/job/1")?.id,
    ).toBe("staffing-randstad");
  });

  it("covers every owner-supplied staffing and recruiting source name", () => {
    const suppliedNames = [
      "Apex System",
      "Armada Group",
      "Arthuer Alwrence",
      "Athenahealth",
      "Axelon Services",
      "Beacon Hill Technologies",
      "Brooksource",
      "Business Plan solutions",
      "Capgemini Americas",
      "CareerBuilder",
      "ClearBridge Technology Group",
      "Computer Futures",
      "Consol Partners",
      "consultsbc",
      "Corporate Biz Solutions Inc.",
      "Cross Creek Systems",
      "css-tech",
      "DeWinter Technology",
      "Eclaro International, Inc.",
      "Ekodus Inc",
      "Empiric Solutions",
      "Expedite Technology",
      "Experies",
      "FLEXCARE MEDICAL STAFFING",
      "Genesis10",
      "Global Force-US",
      "Hireforce",
      "Horizontal Integration",
      "Indotronix International Corporation",
      "Insight Global",
      "JSG",
      "KDS Strategic",
      "Kelly IT Resources",
      "Kforce, Inc",
      "Lawrenceharvey",
      "leadstackinc",
      "MatchPoint Solutions",
      "Maxonic",
      "Modis",
      "Nessiumconsulting",
      "Nexient",
      "Optizm Global",
      "Prairie Technology Recruiting",
      "Quantum Leap",
      "Randstad Technologies",
      "Robert Half Technology",
      "S&D Engineering Solutions, LLC",
      "Sabre Corporation",
      "SIGNATURE CONSULTANTS",
      "Softcom Systems, Inc.",
      "Sogeti USA",
      "Splunk",
      "Sprucetech",
      "Staff Perm",
      "starpoint",
      "Starpoint Solutions",
      "Systems Pros Inc",
      "Talentric",
      "Tech Providers, Inc",
      "Tekni Force",
      "Tek Systems",
      "The Judge Group",
      "The Midtown Group",
      "Three Bridge",
      "TwentyPine",
      "Ventas Consulting",
      "weinberg & associates, inc.",
      "Xchange Software Inc",
    ];

    const missing = suppliedNames.filter((name) => !findJobSourceByName(name));
    expect(missing).toEqual([]);
  });

  it("covers every owner-supplied recruiting domain", () => {
    const suppliedDomains = [
      "randstadusa.com",
      "roberthalf.com",
      "teksystems.com",
      "apexsystems.com",
      "hostventures.com",
      "synechron.com",
      "iconma.com",
      "modis.com",
    ];

    const missing = suppliedDomains.filter(
      (domain) => !findJobSourceByUrl(`https://careers.${domain}/jobs/1`),
    );
    expect(missing).toEqual([]);
  });

  it("keeps portals and curated repositories as leads or handoffs", () => {
    expect(
      findJobSourceByUrl("https://www.linkedin.com/jobs/view/1")
        ?.submissionCapability,
    ).toBe("handoff");
    const feed = findJobSourceByName("SimplifyJobs/New-Grad-Positions");
    expect(feed?.discoveryCapability).toBe("candidate_lead");
    expect(feed && requiresCanonicalEmployerRevalidation(feed)).toBe(true);
    expect(
      findJobSourceByUrl(
        "https://github.com/PrepAIJobs/Summer2026-Internships/tree/main",
      )?.id,
    ).toBe("feed-prepai-internships");
    expect(
      findJobSourceByUrl("https://github.com/zapplyjobs/New-Grad-Jobs-2027")
        ?.id,
    ).toBe("feed-zapply-new-grad");
    expect(
      findJobSourceByUrl("https://github.com/unlisted/repository"),
    ).toBeUndefined();
    expect(
      findJobSourceByUrl("https://storage.stapply.ai/jobhive/v1/manifest.json"),
    ).toMatchObject({
      id: "feed-jobhive-index",
      discoveryCapability: "shared_ingestion",
      submissionCapability: "unknown_review",
      requiresCanonicalRevalidation: true,
    });
    expect(
      findJobSourceByUrl("https://remoteok.com/remote-jobs/1"),
    ).toMatchObject({
      id: "feed-remoteok",
      discoveryCapability: "planned_shared_ingestion",
    });
  });

  it("classifies only exact provider job targets and includes Lever EU", () => {
    expect(
      findJobSourceByUrl("https://jobs.eu.lever.co/acme/posting-123/apply")?.id,
    ).toBe("ats-lever");
    expect(
      findJobSourceByUrl("https://boards.greenhouse.io/acme/jobs/job-123")?.id,
    ).toBe("ats-greenhouse");
    for (const url of [
      "http://jobs.eu.lever.co/acme/posting-123/apply",
      "https://evil.jobs.lever.co/acme/posting-123/apply",
      "https://boards.greenhouse.io/acme/departments/engineering",
    ]) {
      expect(findJobSourceByUrl(url)).toBeUndefined();
    }
  });

  it("does not label connector metadata as a live shared reader", () => {
    const liveShared = JOB_SOURCE_CATALOG.filter(
      (entry) => entry.discoveryCapability === "shared_ingestion",
    ).map((entry) => entry.id);
    expect(liveShared).toEqual(["feed-jobhive-index"]);
    expect(findJobSourceByName("Remote OK")?.discoveryCapability).toBe(
      "planned_shared_ingestion",
    );
    expect(findJobSourceByName("We Work Remotely")?.discoveryCapability).toBe(
      "planned_shared_ingestion",
    );
    expect(findJobSourceByName("Y Combinator jobs")?.discoveryCapability).toBe(
      "planned_shared_ingestion",
    );
    expect(findJobSourceByName("Built In")?.discoveryCapability).toBe(
      "planned_shared_ingestion",
    );
  });

  it("allows scheduled ingestion only for typed public ATS families", () => {
    const scheduled = JOB_SOURCE_CATALOG.filter(
      (entry) => entry.discoveryCapability === "scheduled_public_feed",
    );
    expect(scheduled.map((entry) => entry.name)).toEqual([
      "Greenhouse",
      "Lever",
      "Ashby",
      "SmartRecruiters",
      "Workday",
    ]);
    expect(
      scheduled.every((entry) => !entry.requiresCanonicalRevalidation),
    ).toBe(true);
    expect(
      scheduled
        .filter((entry) => entry.submissionCapability === "beta_review")
        .map((entry) => entry.name),
    ).toEqual(["Greenhouse", "Lever"]);
    expect(
      scheduled
        .filter((entry) => entry.submissionCapability === "unknown_review")
        .map((entry) => entry.name),
    ).toEqual(["Ashby", "SmartRecruiters", "Workday"]);
  });
});
