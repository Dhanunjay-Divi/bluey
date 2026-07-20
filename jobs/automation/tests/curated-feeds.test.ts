import { describe, expect, it, vi } from "vitest";

import {
  CuratedFeedError,
  fetchCuratedFeed,
  inferCuratedCategories,
  parseCuratedFeed,
  publicAtsSourceFromUrl,
} from "../src/curated-feeds.js";

describe("curated job feeds", () => {
  it("reads Simplify HTML tables, keeps original employer links, and drops closed rows", () => {
    const result = parseCuratedFeed("simplify-new-grad", `
      <table>
        <thead><tr><th>Company</th><th>Role</th><th>Location</th><th>Application</th><th>Age</th></tr></thead>
        <tbody>
          <tr>
            <td><a href="https://simplify.jobs/c/acme">Acme</a></td>
            <td>Software Engineer</td>
            <td>Austin, TX</td>
            <td>
              <a href="https://jobs.lever.co/acme/job-1?utm_source=list&ref=feed">Apply</a>
              <a href="https://simplify.jobs/p/job-1">Simplify</a>
            </td>
            <td>2d</td>
          </tr>
          <tr>
            <td>↳</td><td>Platform Engineer</td><td>Remote</td>
            <td><a href="https://boards.greenhouse.io/acme/jobs/2?gh_src=list">Apply</a></td><td>3d</td>
          </tr>
          <tr>
            <td>Closed Co</td><td>Closed role 🔒</td><td>Boston, MA</td>
            <td><a href="https://jobs.ashbyhq.com/closed/1">Apply</a></td><td>1d</td>
          </tr>
        </tbody>
      </table>
    `);

    expect(result.leads).toHaveLength(2);
    expect(result.skippedClosed).toBe(1);
    expect(result.leads[0]).toMatchObject({
      company: "Acme",
      title: "Software Engineer",
      originalUrl: "https://jobs.lever.co/acme/job-1",
      ageDays: 2,
      employmentType: "full_time",
      engagementType: null,
      categoryEvidence: "feed_default",
      submissionCapability: "beta_review",
      atsSource: { kind: "lever", site: "acme", company: "Acme" },
      requiresOriginalRevalidation: true,
    });
    expect(result.leads[1]).toMatchObject({
      company: "Acme",
      atsSource: { kind: "greenhouse", boardToken: "acme" },
    });
  });

  it("reads PrepAI Markdown with embedded HTML links and marks internship leads", () => {
    const result = parseCuratedFeed("prepai-internships", `
| Company | Role | Location | Apply | Posted |
| --- | --- | --- | --- | --- |
| Example Labs | Software Engineer Intern | Seattle,<br>Washington | <a href="https://jobs.ashbyhq.com/example-labs/role-1"><img src="apply.png" alt="Apply"></a> | Jul 20 |
| Example Health | Data Intern | Remote | <a href="https://jobs.smartrecruiters.com/ExampleHealth/123-data-intern">Apply</a> | Jul 19 |
    `);

    expect(result.leads).toHaveLength(2);
    expect(result.leads.every((lead) => lead.employmentType === "internship")).toBe(true);
    expect(result.leads[0]).toMatchObject({
      company: "Example Labs",
      location: "Seattle, Washington",
      atsSource: { kind: "ashby", boardName: "example-labs" },
    });
    expect(result.leads[1]?.atsSource).toMatchObject({
      kind: "smartrecruiters",
      companyIdentifier: "ExampleHealth",
    });
  });

  it("reads image-style Markdown links and leaves unknown sites review-only", () => {
    const result = parseCuratedFeed("zapply-new-grad", `
| Company | Role | Location | Posted | Visa | Apply |
| --- | --- | --- | --- | --- | --- |
| **Acme** | Software Engineer | Chicago, IL | Recently |  | [<img src="images/apply.png" alt="Apply">](https://careers.acme.example/jobs/42?utm_campaign=list) |
    `);

    expect(result.leads).toHaveLength(1);
    expect(result.leads[0]).toMatchObject({
      company: "Acme",
      title: "Software Engineer",
      originalUrl: "https://careers.acme.example/jobs/42",
      submissionCapability: "unknown_review",
      atsSource: null,
    });
  });

  it("extracts bounded source identities for each scheduled ATS family", () => {
    expect(publicAtsSourceFromUrl("https://job-boards.greenhouse.io/acme/jobs/1")).toEqual({
      kind: "greenhouse",
      boardToken: "acme",
    });
    expect(publicAtsSourceFromUrl("https://jobs.lever.co/acme/1")).toEqual({ kind: "lever", site: "acme" });
    expect(publicAtsSourceFromUrl("https://jobs.ashbyhq.com/acme/1")).toEqual({
      kind: "ashby",
      boardName: "acme",
    });
    expect(publicAtsSourceFromUrl("https://jobs.smartrecruiters.com/Acme/1-role")).toEqual({
      kind: "smartrecruiters",
      companyIdentifier: "Acme",
    });
    expect(publicAtsSourceFromUrl("https://acme.wd5.myworkdayjobs.com/Careers/job/Austin/Role_R1")).toEqual({
      kind: "workday",
      tenant: "acme",
      instance: "wd5",
      site: "Careers",
    });
  });

  it("deduplicates canonical employer links", () => {
    const result = parseCuratedFeed("prepai-new-grad", `
| Company | Role | Location | Apply | Posted |
| --- | --- | --- | --- | --- |
| Acme | Engineer | Remote | [Apply](https://jobs.lever.co/acme/one) | Today |
| Acme | Engineer duplicate | Remote | [Apply](https://jobs.lever.co/acme/one) | Today |
    `);
    expect(result.leads).toHaveLength(1);
    expect(result.duplicatesCollapsed).toBe(1);
  });

  it("classifies explicit employment and engagement language without making it authoritative", () => {
    expect(inferCuratedCategories("Backend Engineer - W2 contract", "full_time")).toEqual({
      employmentType: "contract",
      engagementType: "w2",
      explicit: true,
    });
    expect(inferCuratedCategories("C2C Data Engineer", "full_time")).toEqual({
      employmentType: "contract",
      engagementType: "c2c",
      explicit: true,
    });
    expect(inferCuratedCategories("Summer software internship", "full_time")).toEqual({
      employmentType: "internship",
      engagementType: null,
      explicit: true,
    });
    expect(inferCuratedCategories("Software Engineer", "full_time")).toEqual({
      employmentType: "full_time",
      engagementType: null,
      explicit: false,
    });
  });

  it("enforces response limits and conditional requests", async () => {
    const fetcher = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      expect(new Headers(init?.headers).get("if-none-match")).toBe("feed-v1");
      return new Response(null, { status: 304 });
    });
    await expect(fetchCuratedFeed("simplify-new-grad", {
      fetch: fetcher as typeof fetch,
      ifNoneMatch: "feed-v1",
    })).rejects.toMatchObject({ code: "not_modified" });

    expect(() => parseCuratedFeed("simplify-new-grad", "x".repeat(2 * 1024 * 1024 + 1)))
      .toThrow(CuratedFeedError);
  });
});
