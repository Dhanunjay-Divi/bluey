import { describe, expect, it, vi } from "vitest";
import {
  isRecentJob,
  parsePostedAt,
  PublicAtsDiscoveryProvider,
  type FetchResponse,
  type JobsFetch,
  type PublicAtsSource,
} from "../src/index.js";

function response(payload: unknown, status = 200): FetchResponse {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(payload),
  };
}

describe("public ATS discovery", () => {
  it("normalizes and filters provider-specific Greenhouse and Lever postings", async () => {
    const fetcher: JobsFetch = vi.fn(async (url, init) => {
      expect(init.redirect).toBe("error");
      if (url.includes("greenhouse")) {
        return response({ jobs: [{
          id: 10,
          title: "Senior Software Engineer",
          absolute_url: "https://boards.greenhouse.io/acme/jobs/10?gh_src=test",
          location: { name: "New York, NY" },
          content: "<p>Build reliable systems &amp; tools.</p>",
          updated_at: new Date().toISOString(),
          departments: [{ name: "Engineering" }],
        }] });
      }
      return response([{
        id: "lever-1",
        text: "Senior Software Engineer",
        hostedUrl: "https://boards.greenhouse.io/acme/jobs/10",
        categories: { location: "New York, NY", department: "Engineering" },
        descriptionPlain: "Duplicate feed entry",
        lists: [{ text: "Experience", content: "<li>Build reliable systems</li>" }],
        additional: "<div>Visa Sponsorship</div><div>This position is eligible for visa sponsorship.</div>",
        createdAt: Date.now(),
        salaryRange: { min: 150000, max: 450000, currency: "USD", interval: "per-year-salary" },
      }]);
    });
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher, sleep: async () => undefined });
    const page = await provider.search({
      roles: ["software engineer"],
      locations: ["new york"],
      remotePreference: "hybrid",
      excludedCompanies: [],
      sources: [
        { kind: "greenhouse", boardToken: "acme", company: "Acme" },
        { kind: "lever", site: "acme", company: "Acme" },
      ],
    });

    expect(page.jobs).toHaveLength(2);
    expect(page.jobs[0]).toMatchObject({
      company: "Acme",
      title: "Senior Software Engineer",
      description: "Build reliable systems & tools.",
      department: "Engineering",
    });
    expect(page.jobs[0]?.canonicalUrl).not.toContain("gh_src");
    expect(page.jobs.find((job) => job.source === "lever")?.compensation)
      .toBe("USD 150000-450000 per-year-salary");
    expect(page.jobs.find((job) => job.source === "lever")?.description)
      .toContain("This position is eligible for visa sponsorship.");
  });

  it("uses bounded Workday pagination and a pinned POST target", async () => {
    const fetcher: JobsFetch = vi.fn(async (url, init) => {
      expect(url).toBe("https://acme.wd5.myworkdayjobs.com/wday/cxs/acme/careers/jobs");
      expect(init.method).toBe("POST");
      return response({
        total: 1,
        jobPostings: [{
          title: "Product Designer",
          externalPath: "/en-US/careers/job/Product-Designer_R123",
          locationsText: "Remote - US",
          bulletFields: ["R123"],
          postedOn: "Posted Today",
        }],
      });
    });
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher, sleep: async () => undefined });
    const page = await provider.search({
      roles: ["product designer"],
      locations: [],
      remotePreference: "remote_only",
      excludedCompanies: [],
      sources: [{ kind: "workday", tenant: "acme", instance: "wd5", site: "careers", company: "Acme" }],
    });

    expect(page.jobs).toHaveLength(1);
    expect(page.jobs[0]?.workplace).toBe("remote");
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("builds canonical public Workday URLs from native CXS job paths only", async () => {
    const fetcher: JobsFetch = vi.fn(async (url) => {
      expect(url).toBe("https://workday.wd5.myworkdayjobs.com/wday/cxs/workday/Workday/jobs");
      return response({
        total: 4,
        jobPostings: [
          {
            title: "Senior Software Engineer",
            externalPath: "/job/Ireland-Dublin/Senior-Software-Engineer_JR-0107796",
            locationsText: "Dublin, Ireland",
            bulletFields: ["JR-0107796"],
            postedOn: "Posted Today",
          },
          {
            title: "Qualified same-host path",
            externalPath: "/en-US/Workday/job/United-States/Qualified-Path_JR-2",
            locationsText: "Remote - US",
            bulletFields: ["JR-2"],
            postedOn: "Posted Today",
          },
          {
            title: "Qualified same-host absolute URL",
            externalPath: "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Canada/Qualified-Url_JR-3",
            locationsText: "Toronto, Canada",
            bulletFields: ["JR-3"],
            postedOn: "Posted Today",
          },
          {
            title: "Cross-host URL is not a public Workday job",
            externalPath: "https://evil.example/job/Elsewhere/Blocked_JR-4",
            locationsText: "Nowhere",
            bulletFields: ["JR-4"],
            postedOn: "Posted Today",
          },
        ],
      });
    });
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher, sleep: async () => undefined });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{
        kind: "workday",
        tenant: "workday",
        instance: "wd5",
        site: "Workday",
        locale: "en-US",
        company: "Workday",
      }],
    });

    expect(page.jobs.map((job) => job.canonicalUrl)).toEqual([
      "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Ireland-Dublin/Senior-Software-Engineer_JR-0107796",
      "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/United-States/Qualified-Path_JR-2",
      "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Canada/Qualified-Url_JR-3",
    ]);
  });

  it("fails a scheduled snapshot when any provider-listed row is invalid", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () => response({
        total: 1,
        jobPostings: [{
          title: "Still listed but malformed",
          externalPath: "https://evil.example/job/Elsewhere/Blocked_JR-BAD",
          locationsText: "Remote",
          bulletFields: ["JR-BAD"],
          postedOn: "Posted Today",
        }],
      }),
      sleep: async () => undefined,
    });

    await expect(provider.snapshot({
      kind: "workday",
      tenant: "workday",
      instance: "wd5",
      site: "Workday",
    })).rejects.toThrow("invalid listed row");
  });

  it("fails closed when a scheduled provider payload omits its job list", async () => {
    const sources: PublicAtsSource[] = [
      { kind: "greenhouse", boardToken: "acme" },
      { kind: "lever", site: "acme" },
      { kind: "ashby", boardName: "acme" },
      { kind: "smartrecruiters", companyIdentifier: "Acme" },
      { kind: "workday", tenant: "acme", instance: "wd5", site: "Careers" },
    ];

    for (const source of sources) {
      const provider = new PublicAtsDiscoveryProvider({
        fetch: async () => response({}),
        sleep: async () => undefined,
      });
      await expect(provider.snapshot(source)).rejects.toThrow("expected job list");
    }
  });

  it("derives SmartRecruiters public URLs from the configured company and posting ID", async () => {
    const fetcher: JobsFetch = vi.fn(async (url) => {
      expect(url).toBe("https://api.smartrecruiters.com/v1/companies/Experian/postings?limit=100&offset=0");
      return response({
        totalFound: 2,
        content: [
          {
            id: "744000138411689",
            name: "Senior Software Engineer",
            ref: "https://api.smartrecruiters.com/v1/companies/Experian/postings/744000138411689",
            postingUrl: "https://evil.example/jobs/744000138411689",
            location: { city: "Dublin", country: "Ireland" },
            company: { name: "Experian" },
            releasedDate: new Date().toISOString(),
          },
          {
            id: "unsafe/id",
            name: "Malformed provider ID",
            ref: "https://evil.example/jobs/unsafe-id",
            location: { city: "Nowhere" },
            releasedDate: new Date().toISOString(),
          },
        ],
      });
    });
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher, sleep: async () => undefined });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{ kind: "smartrecruiters", companyIdentifier: "Experian", company: "Experian" }],
    });

    expect(page.jobs).toHaveLength(1);
    expect(page.jobs[0]?.canonicalUrl)
      .toBe("https://jobs.smartrecruiters.com/Experian/744000138411689");
  });

  it("refuses a capped SmartRecruiters or Workday feed as an incomplete snapshot", async () => {
    const smartFetch: JobsFetch = vi.fn(async (url) => {
      const offset = Number(new URL(url).searchParams.get("offset"));
      return response({
        totalFound: 501,
        content: Array.from({ length: 100 }, (_, index) => ({
          id: `${offset + index}`,
          name: `Job ${offset + index}`,
          location: { city: "Dublin" },
          releasedDate: new Date().toISOString(),
        })),
      });
    });
    const smart = new PublicAtsDiscoveryProvider({
      fetch: smartFetch,
      maxPages: 5,
      sleep: async () => undefined,
    });
    await expect(smart.snapshot({ kind: "smartrecruiters", companyIdentifier: "Experian" }))
      .rejects.toThrow("pagination cap");
    expect(smartFetch).toHaveBeenCalledTimes(5);

    const workdayFetch: JobsFetch = vi.fn(async () => response({
      total: 101,
      jobPostings: Array.from({ length: 20 }, (_, index) => ({
        title: `Job ${index}`,
        externalPath: `/job/Dublin/Engineer_JR-${index}`,
        bulletFields: [`JR-${index}`],
        postedOn: "Posted Today",
      })),
    }));
    const workday = new PublicAtsDiscoveryProvider({
      fetch: workdayFetch,
      maxPages: 5,
      sleep: async () => undefined,
    });
    await expect(workday.snapshot({
      kind: "workday",
      tenant: "workday",
      instance: "wd5",
      site: "Workday",
    })).rejects.toThrow("pagination cap");
    expect(workdayFetch).toHaveBeenCalledTimes(5);
  });

  it("retries transient ATS errors without following redirects", async () => {
    let calls = 0;
    const fetcher: JobsFetch = vi.fn(async () => {
      calls += 1;
      return calls === 1 ? response({ message: "busy" }, 429) : response({ jobs: [] });
    });
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher, maxAttempts: 2, sleep: async () => undefined });
    await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    });
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("cancels a chunked ATS response as soon as its byte limit is exceeded", async () => {
    let pulls = 0;
    let cancelled = false;
    const body = new ReadableStream<Uint8Array>({
      pull(controller) {
        pulls += 1;
        controller.enqueue(new Uint8Array(1024 * 1024));
      },
      cancel() {
        cancelled = true;
      },
    });
    const provider = new PublicAtsDiscoveryProvider({
      maxAttempts: 1,
      fetch: async () => ({
        ok: true,
        status: 200,
        body,
        text: async () => {
          throw new Error("streaming response must not be buffered through text()");
        },
      }),
    });

    await expect(provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    })).rejects.toThrow("size limit");
    expect(pulls).toBeLessThanOrEqual(7);
    expect(cancelled).toBe(true);
  });

  it("rejects source identifiers that could escape the pinned endpoint", async () => {
    const provider = new PublicAtsDiscoveryProvider({ fetch: vi.fn() });
    await expect(provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{ kind: "greenhouse", boardToken: "../private" }],
    })).rejects.toThrow("unsupported characters");
  });

  it("ignores blank exclusions instead of excluding every job", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () => response({ jobs: [{
        id: "1",
        title: "Engineer",
        absolute_url: "https://boards.greenhouse.io/acme/jobs/1",
        location: { name: "Remote" },
        updated_at: new Date().toISOString(),
      }] }),
    });
    const page = await provider.search({
      roles: [""],
      locations: [""],
      remotePreference: "any",
      excludedCompanies: [""],
      excludedTitles: [""],
      sources: [{ kind: "greenhouse", boardToken: "acme", company: "Acme" }],
    });
    expect(page.jobs).toHaveLength(1);
  });

  it("keeps healthy sources when another configured source fails", async () => {
    const fetcher: JobsFetch = vi.fn(async (url) => url.includes("greenhouse")
      ? response({ error: "down" }, 503)
      : response([{
          id: "lever-1",
          text: "Engineer",
          hostedUrl: "https://jobs.lever.co/atlas/lever-1",
          categories: { location: "Remote" },
          createdAt: Date.now(),
        }]));
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher, maxAttempts: 1 });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        { kind: "greenhouse", boardToken: "acme" },
        { kind: "lever", site: "atlas" },
      ],
    });
    expect(page.jobs).toHaveLength(1);
    expect(page.warnings?.[0]).toContain("greenhouse source failed");
  });

  it("replaces an unsafe payload link with the configured provider URL", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () => response({ jobs: [{
        id: "1",
        title: "Engineer",
        absolute_url: "http://127.0.0.1/admin",
        updated_at: new Date().toISOString(),
      }] }),
    });
    const page = await provider.search({
      roles: [], locations: [], remotePreference: "any", excludedCompanies: [],
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    });
    expect(page.jobs[0]?.canonicalUrl).toBe("https://boards.greenhouse.io/acme/jobs/1");
  });

  it("does not treat a provider payload's custom careers URL as source authority", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async (url) => url.includes("greenhouse")
        ? response({ jobs: [{
            id: "gh-1",
            title: "Engineer",
            absolute_url: "https://careers.example.test/opening?gh_jid=gh-1",
            updated_at: new Date().toISOString(),
          }] })
        : response([{
            id: "lever-1",
            text: "Designer",
            hostedUrl: "https://careers.example.test/designer",
            createdAt: Date.now(),
          }]),
    });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        { kind: "greenhouse", boardToken: "acme", company: "Acme" },
        { kind: "lever", site: "atlas", company: "Atlas" },
      ],
    });

    expect(page.jobs.map((job) => job.canonicalUrl)).toEqual([
      "https://boards.greenhouse.io/acme/jobs/gh-1",
      "https://jobs.lever.co/atlas/lever-1",
    ]);
  });

  it("keeps recent postings and skips old or undated listings", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () => response({ jobs: [
        { id: "recent", title: "Engineer", absolute_url: "https://boards.greenhouse.io/acme/jobs/recent", updated_at: "3 days ago" },
        { id: "old", title: "Engineer", absolute_url: "https://boards.greenhouse.io/acme/jobs/old", updated_at: "45 days ago" },
        { id: "undated", title: "Engineer", absolute_url: "https://boards.greenhouse.io/acme/jobs/undated" },
      ] }),
    });
    const page = await provider.search({
      roles: [], locations: [], remotePreference: "any", excludedCompanies: [], maxPostingAgeDays: 14,
      sources: [{ kind: "greenhouse", boardToken: "acme", company: "Acme" }],
    });
    expect(page.jobs.map((job) => job.externalId)).toEqual(["recent"]);
    expect(page.warnings?.[0]).toContain("2 old or undated jobs were skipped");
  });

  it("parses ATS relative dates against a stable clock", () => {
    const now = new Date("2026-07-10T12:00:00.000Z");
    expect(parsePostedAt("Posted Today", now)?.toISOString()).toBe(now.toISOString());
    expect(parsePostedAt("2 weeks ago", now)?.toISOString()).toBe("2026-06-26T12:00:00.000Z");
    expect(isRecentJob({
      externalId: "job", canonicalUrl: "https://example.com/job", company: "Acme", title: "Engineer",
      location: "Remote", workplace: "remote", description: "", source: "greenhouse", postedAt: "13 days ago",
    }, 14, now)).toBe(true);
  });
});
