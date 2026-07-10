import { describe, expect, it, vi } from "vitest";
import { isRecentJob, parsePostedAt, PublicAtsDiscoveryProvider, type FetchResponse, type JobsFetch } from "../src/index.js";

function response(payload: unknown, status = 200): FetchResponse {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(payload),
  };
}

describe("public ATS discovery", () => {
  it("normalizes, filters, and deduplicates Greenhouse and Lever postings", async () => {
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
        createdAt: Date.now(),
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

    expect(page.jobs).toHaveLength(1);
    expect(page.jobs[0]).toMatchObject({
      company: "Acme",
      title: "Senior Software Engineer",
      description: "Build reliable systems & tools.",
      department: "Engineering",
    });
    expect(page.jobs[0]?.canonicalUrl).not.toContain("gh_src");
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

  it("drops unsafe canonical links supplied inside a trusted feed", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () => response({ jobs: [{ id: "1", title: "Engineer", absolute_url: "http://127.0.0.1/admin" }] }),
    });
    const page = await provider.search({
      roles: [], locations: [], remotePreference: "any", excludedCompanies: [],
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    });
    expect(page.jobs).toEqual([]);
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
