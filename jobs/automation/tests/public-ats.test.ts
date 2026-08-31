import { createHash } from "node:crypto";
import { describe, expect, it, vi } from "vitest";
import {
  InvalidPublicAtsCursorError,
  isRecentJob,
  parsePostedAt,
  PublicAtsContinuationHistoryLimitError,
  PublicAtsDiscoveryProvider,
  type DiscoveryQuery,
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

interface TestPublicAtsCursor {
  version: number;
  pageOffset: number;
  querySha256: string;
  windowSha256: string | null;
  sources: Array<{
    offset: number;
    advertisedTotal: number | null;
    prefixSha256: string | null;
    validationOffset: number;
    validationSha256: string | null;
    overlapCount: number;
    overlapSha256: string | null;
    done: boolean;
  }>;
  history: {
    seenJobSha256: string[];
    crossListingCandidates: string[];
    seenCrossListingSha256: string[];
  };
  checksumSha256: string;
}

function rewritePublicAtsCursor(
  cursor: string,
  mutate: (payload: TestPublicAtsCursor) => void,
): string {
  const payload = JSON.parse(
    Buffer.from(cursor, "base64url").toString("utf8"),
  ) as TestPublicAtsCursor;
  mutate(payload);
  payload.checksumSha256 = createHash("sha256")
    .update(
      JSON.stringify({
        version: payload.version,
        pageOffset: payload.pageOffset,
        querySha256: payload.querySha256,
        windowSha256: payload.windowSha256,
        sources: payload.sources,
        history: payload.history,
      }),
    )
    .digest("hex");
  return Buffer.from(JSON.stringify(payload), "utf8").toString("base64url");
}

function tamperPublicAtsCursor(cursor: string): string {
  const index = Math.floor(cursor.length / 2);
  const replacement = cursor[index] === "A" ? "B" : "A";
  return `${cursor.slice(0, index)}${replacement}${cursor.slice(index + 1)}`;
}

describe("public ATS discovery", () => {
  it("normalizes and filters provider-specific Greenhouse and Lever postings", async () => {
    const fetcher: JobsFetch = vi.fn(async (url, init) => {
      expect(init.redirect).toBe("error");
      if (url.includes("greenhouse")) {
        return response({
          jobs: [
            {
              id: 10,
              title: "Senior Software Engineer",
              absolute_url:
                "https://boards.greenhouse.io/acme/jobs/10?gh_src=test",
              location: { name: "New York, NY" },
              content: "<p>Build reliable systems &amp; tools.</p>",
              updated_at: new Date().toISOString(),
              departments: [{ name: "Engineering" }],
            },
          ],
        });
      }
      return response([
        {
          id: "lever-1",
          text: "Senior Software Engineer",
          hostedUrl: "https://boards.greenhouse.io/acme/jobs/10",
          categories: {
            location: "New York, NY",
            department: "Engineering",
            commitment: "Contract W2",
          },
          descriptionPlain: "Duplicate feed entry",
          lists: [
            { text: "Experience", content: "<li>Build reliable systems</li>" },
          ],
          additional:
            "<div>Visa Sponsorship</div><div>This position is eligible for visa sponsorship.</div>",
          createdAt: Date.now(),
          salaryRange: {
            min: 150000,
            max: 450000,
            currency: "USD",
            interval: "per-year-salary",
          },
        },
      ]);
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      sleep: async () => undefined,
    });
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
    expect(page.jobs.find((job) => job.source === "lever")?.compensation).toBe(
      "USD 150000-450000 per-year-salary",
    );
    expect(
      page.jobs.find((job) => job.source === "lever")?.description,
    ).toContain("This position is eligible for visa sponsorship.");
    expect(page.jobs.find((job) => job.source === "lever")).toMatchObject({
      employmentType: "contract",
      engagementType: "w2",
    });
  });

  it("normalizes explicit internship categories without inventing engagement", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response({
          totalFound: 1,
          content: [
            {
              id: "intern-1",
              name: "Software Engineering Intern",
              company: { name: "Acme" },
              location: { city: "Austin", region: "TX", country: "US" },
              typeOfEmployment: { label: "Internship" },
              releasedDate: new Date().toISOString(),
            },
          ],
        }),
      sleep: async () => undefined,
    });
    const jobs = await provider.snapshot({
      kind: "smartrecruiters",
      companyIdentifier: "Acme",
    });
    expect(jobs[0]).toMatchObject({ employmentType: "internship" });
    expect(jobs[0]?.engagementType).toBeUndefined();
  });

  it("does not infer Remote from a city when SmartRecruiters explicitly says remote false", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response({
          totalFound: 4,
          content: [
            {
              id: "onsite-remote-city",
              name: "Software Engineer",
              company: { name: "Acme" },
              location: {
                city: "Remote",
                region: "OR",
                country: "US",
                remote: false,
              },
              releasedDate: new Date().toISOString(),
            },
            {
              id: "contradictory-remote-true",
              name: "Software Engineer",
              company: { name: "Acme" },
              location: {
                city: "Portland",
                region: "OR",
                country: "US",
                remote: true,
              },
              workplaceType: "onsite",
              releasedDate: new Date().toISOString(),
            },
            {
              id: "contradictory-remote-false",
              name: "Software Engineer",
              company: { name: "Acme" },
              location: {
                city: "Portland",
                region: "OR",
                country: "US",
                remote: false,
              },
              workplaceType: "remote",
              releasedDate: new Date().toISOString(),
            },
            {
              id: "typed-onsite",
              name: "Software Engineer",
              company: { name: "Acme" },
              location: {
                city: "Portland",
                region: "OR",
                country: "US",
                remote: false,
              },
              workplaceType: "onsite",
              releasedDate: new Date().toISOString(),
            },
          ],
        }),
      sleep: async () => undefined,
    });
    const source = {
      kind: "smartrecruiters",
      companyIdentifier: "Acme",
    } as const;

    expect(
      (await provider.snapshot(source)).map((job) => job.workplace),
    ).toEqual(["unknown", "unknown", "unknown", "onsite"]);
    const remoteHint = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "remote_only",
      excludedCompanies: [],
      sources: [source],
    });
    expect(remoteHint.jobs).toHaveLength(4);
    expect(remoteHint.jobs.map((job) => job.workplace)).toEqual([
      "unknown",
      "unknown",
      "unknown",
      "onsite",
    ]);
  });

  it("uses bounded Workday pagination and a pinned POST target", async () => {
    const fetcher: JobsFetch = vi.fn(async (url, init) => {
      expect(url).toBe(
        "https://acme.wd5.myworkdayjobs.com/wday/cxs/acme/careers/jobs",
      );
      expect(init.method).toBe("POST");
      expect(JSON.parse(String(init.body))).toEqual({
        appliedFacets: {},
        limit: 20,
        offset: 0,
        searchText: "",
      });
      return response({
        total: 1,
        jobPostings: [
          {
            title: "SDE",
            externalPath: "/en-US/careers/job/SDE_R123",
            locationsText: "Remote - US",
            bulletFields: ["R123"],
            postedOn: "Posted Today",
            timeType: "Full time",
            employmentType: "Temporary",
          },
        ],
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      sleep: async () => undefined,
    });
    const page = await provider.search({
      roles: ["software engineer"],
      locations: [],
      remotePreference: "remote_only",
      excludedCompanies: [],
      sources: [
        {
          kind: "workday",
          tenant: "acme",
          instance: "wd5",
          site: "careers",
          company: "Acme",
        },
      ],
    });

    expect(page.jobs).toHaveLength(1);
    expect(page.jobs[0]?.workplace).toBe("unknown");
    expect(page.jobs[0]?.employmentType).toBeUndefined();
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("keeps negated or mixed workplace evidence unknown without prefilter loss", async () => {
    const payload = {
      jobs: [
        {
          id: "not-remote",
          title: "Software Engineer",
          workplace_type: "not remote",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "not-fully-remote",
          title: "Software Engineer",
          workplace_type: "not fully remote",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "not-completely-remote",
          title: "Software Engineer",
          workplace_type: "not completely remote",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "mixed-workplace",
          title: "Software Engineer",
          workplace_type: "remote or hybrid",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "remote",
          title: "Software Engineer",
          workplace_type: "remote",
          location: { name: "United States" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "onsite-unavailable",
          title: "Software Engineer",
          workplace_type: "onsite unavailable",
          location: { name: "United States" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "hybrid-prohibited",
          title: "Software Engineer",
          workplace_type: "hybrid roles prohibited",
          location: { name: "United States" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "cannot-remote",
          title: "Software Engineer",
          workplace_type: "cannot be remote",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "does-not-allow-remote",
          title: "Software Engineer",
          workplace_type: "does not allow remote",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "will-not-remote",
          title: "Software Engineer",
          workplace_type: "will not be remote",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "coordinated-workplace-rejection",
          title: "Software Engineer",
          workplace_type: "remote and hybrid roles not offered",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "qualified-suffix-rejection",
          title: "Software Engineer",
          workplace_type: "remote is not currently available",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "currently-unavailable",
          title: "Software Engineer",
          workplace_type: "remote currently unavailable",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "temporarily-unavailable",
          title: "Software Engineer",
          workplace_type: "remote temporarily unavailable",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
        {
          id: "no-longer-remote",
          title: "Software Engineer",
          workplace_type: "no longer remote",
          location: { name: "New York, NY" },
          updated_at: new Date().toISOString(),
        },
      ],
    };
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () => response(payload),
      sleep: async () => undefined,
    });
    const source = {
      kind: "greenhouse",
      boardToken: "acme",
      company: "Acme",
    } as const;

    const snapshot = await provider.snapshot(source);
    expect(snapshot.map((job) => job.workplace)).toEqual([
      "unknown",
      "unknown",
      "unknown",
      "unknown",
      "remote",
      "unknown",
      "unknown",
      "unknown",
      "unknown",
      "unknown",
      "unknown",
      "unknown",
      "unknown",
      "unknown",
      "unknown",
    ]);

    const remoteHint = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "remote_only",
      excludedCompanies: [],
      sources: [source],
    });
    expect(remoteHint.jobs.map((job) => job.externalId)).toEqual(
      payload.jobs.map((job) => job.id),
    );
  });

  it("does not turn negated or mixed engagement evidence into the first matching kind", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response([
          {
            id: "w2-only",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "No C2C; W2 only",
            },
            createdAt: Date.now(),
          },
          {
            id: "mixed-engagement",
            text: "Software Engineer",
            categories: { location: "New York, NY", engagement: "C2C or W2" },
            createdAt: Date.now(),
          },
          {
            id: "c2c-suffix-negation",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "C2C not accepted",
            },
            createdAt: Date.now(),
          },
          {
            id: "c2c-relational-negation",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "We do not accept C2C",
            },
            createdAt: Date.now(),
          },
          {
            id: "c2c-ineligible",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "Not eligible for C2C",
            },
            createdAt: Date.now(),
          },
          {
            id: "c2c-will-not-prefix",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "We will not accept C2C",
            },
            createdAt: Date.now(),
          },
          {
            id: "c2c-will-not-suffix",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "C2C will not be accepted",
            },
            createdAt: Date.now(),
          },
          {
            id: "no-longer-c2c",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "No longer C2C",
            },
            createdAt: Date.now(),
          },
          {
            id: "no-longer-accepting-c2c",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "No longer accepting C2C",
            },
            createdAt: Date.now(),
          },
          {
            id: "c2c-no-longer-accepted",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              engagement: "C2C no longer accepted",
            },
            createdAt: Date.now(),
          },
        ]),
      sleep: async () => undefined,
    });

    const snapshot = await provider.snapshot({
      kind: "lever",
      site: "acme",
      company: "Acme",
    });
    expect(snapshot[0]?.engagementType).toBe("w2");
    expect(snapshot[1]?.engagementType).toBeUndefined();
    expect(snapshot[2]?.engagementType).toBeUndefined();
    expect(snapshot[3]?.engagementType).toBeUndefined();
    expect(snapshot[4]?.engagementType).toBeUndefined();
    expect(snapshot[5]?.engagementType).toBeUndefined();
    expect(snapshot[6]?.engagementType).toBeUndefined();
    expect(snapshot[7]?.engagementType).toBeUndefined();
    expect(snapshot[8]?.engagementType).toBeUndefined();
    expect(snapshot[9]?.engagementType).toBeUndefined();
  });

  it("does not turn negated or mixed employment evidence into the first matching kind", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response([
          {
            id: "contract-only",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              commitment: "No full-time; contract only",
            },
            createdAt: Date.now(),
          },
          {
            id: "mixed-employment",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              commitment: "Full-time or contract",
            },
            createdAt: Date.now(),
          },
          {
            id: "full-time-suffix-negation",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              commitment: "Full-time roles unavailable",
            },
            createdAt: Date.now(),
          },
          {
            id: "contract-relational-negation",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              commitment: "Does not allow contract roles",
            },
            createdAt: Date.now(),
          },
          {
            id: "contract-no-longer-offered",
            text: "Software Engineer",
            categories: {
              location: "New York, NY",
              commitment: "Contract no longer offered",
            },
            createdAt: Date.now(),
          },
        ]),
      sleep: async () => undefined,
    });

    const snapshot = await provider.snapshot({
      kind: "lever",
      site: "acme",
      company: "Acme",
    });
    expect(snapshot[0]?.employmentType).toBe("contract");
    expect(snapshot[1]?.employmentType).toBeUndefined();
    expect(snapshot[2]?.employmentType).toBeUndefined();
    expect(snapshot[3]?.employmentType).toBeUndefined();
    expect(snapshot[4]?.employmentType).toBeUndefined();
  });

  it("does not manufacture typed job categories from titles or descriptions", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response([
          {
            id: "contract-title",
            text: "Contract Administrator",
            categories: { location: "New York, NY" },
            descriptionPlain:
              "Full-time benefits may be discussed during review.",
            createdAt: Date.now(),
          },
          {
            id: "engagement-title",
            text: "1099 Compliance Analyst",
            categories: { location: "New York, NY" },
            descriptionPlain: "Advise the W2 and C2C compliance teams.",
            createdAt: Date.now(),
          },
        ]),
      sleep: async () => undefined,
    });

    const snapshot = await provider.snapshot({
      kind: "lever",
      site: "acme",
      company: "Acme",
    });
    for (const job of snapshot) {
      expect(job.employmentType).toBeUndefined();
      expect(job.engagementType).toBeUndefined();
    }
  });

  it("builds canonical public Workday URLs from native CXS job paths only", async () => {
    const fetcher: JobsFetch = vi.fn(async (url) => {
      expect(url).toBe(
        "https://workday.wd5.myworkdayjobs.com/wday/cxs/workday/Workday/jobs",
      );
      return response({
        total: 4,
        jobPostings: [
          {
            title: "Senior Software Engineer",
            externalPath:
              "/job/Ireland-Dublin/Senior-Software-Engineer_JR-0107796",
            locationsText: "Dublin, Ireland",
            bulletFields: ["JR-0107796"],
            postedOn: "Posted Today",
          },
          {
            title: "Qualified same-host path",
            externalPath:
              "/en-US/Workday/job/United-States/Qualified-Path_JR-2",
            locationsText: "Remote - US",
            bulletFields: ["JR-2"],
            postedOn: "Posted Today",
          },
          {
            title: "Qualified same-host absolute URL",
            externalPath:
              "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Canada/Qualified-Url_JR-3",
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
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      sleep: async () => undefined,
    });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        {
          kind: "workday",
          tenant: "workday",
          instance: "wd5",
          site: "Workday",
          locale: "en-US",
          company: "Workday",
        },
      ],
    });

    expect(page.jobs.map((job) => job.canonicalUrl)).toEqual([
      "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Ireland-Dublin/Senior-Software-Engineer_JR-0107796",
      "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/United-States/Qualified-Path_JR-2",
      "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Canada/Qualified-Url_JR-3",
    ]);
  });

  it("fails a scheduled snapshot when any provider-listed row is invalid", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response({
          total: 1,
          jobPostings: [
            {
              title: "Still listed but malformed",
              externalPath: "https://evil.example/job/Elsewhere/Blocked_JR-BAD",
              locationsText: "Remote",
              bulletFields: ["JR-BAD"],
              postedOn: "Posted Today",
            },
          ],
        }),
      sleep: async () => undefined,
    });

    await expect(
      provider.snapshot({
        kind: "workday",
        tenant: "workday",
        instance: "wd5",
        site: "Workday",
      }),
    ).rejects.toThrow("invalid listed row");
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
      await expect(provider.snapshot(source)).rejects.toThrow(
        "expected job list",
      );
    }
  });

  it("rejects Ashby rows whose public URL is outside the configured board", async () => {
    for (const jobUrl of [
      "https://evil.example/acme/a-hostile",
      "https://jobs.ashbyhq.com/other-board/a-hostile",
      "https://jobs.ashbyhq.com/acme/different-job",
      "https://jobs.ashbyhq.com:444/acme/a-hostile",
    ]) {
      const provider = new PublicAtsDiscoveryProvider({
        fetch: async () =>
          response({
            jobs: [
              {
                id: "a-hostile",
                title: "Platform Engineer",
                jobUrl,
                publishedAt: new Date().toISOString(),
              },
            ],
          }),
        sleep: async () => undefined,
      });

      await expect(
        provider.snapshot({ kind: "ashby", boardName: "acme" }),
      ).rejects.toThrow("invalid listed row");
    }

    const valid = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response({
          jobs: [
            {
              id: "a-valid",
              title: "Platform Engineer",
              jobUrl:
                "https://jobs.ashbyhq.com/acme/a-valid?utm_source=feed#details",
              publishedAt: new Date().toISOString(),
            },
          ],
        }),
      sleep: async () => undefined,
    });
    await expect(
      valid.snapshot({ kind: "ashby", boardName: "acme" }),
    ).resolves.toMatchObject([
      {
        externalId: "a-valid",
        canonicalUrl: "https://jobs.ashbyhq.com/acme/a-valid",
      },
    ]);
  });

  it("derives SmartRecruiters public URLs from the configured company and posting ID", async () => {
    const fetcher: JobsFetch = vi.fn(async (url) => {
      expect(url).toBe(
        "https://api.smartrecruiters.com/v1/companies/Experian/postings?limit=100&offset=0",
      );
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
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      sleep: async () => undefined,
    });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        {
          kind: "smartrecruiters",
          companyIdentifier: "Experian",
          company: "Experian",
        },
      ],
    });

    expect(page.jobs).toHaveLength(1);
    expect(page.jobs[0]?.canonicalUrl).toBe(
      "https://jobs.smartrecruiters.com/Experian/744000138411689",
    );
  });

  it("traverses all 501 SmartRecruiters rows while snapshots remain fail-closed", async () => {
    const releasedDate = new Date().toISOString();
    const rows = Array.from({ length: 501 }, (_, index) => ({
      id: `${index}`,
      name: `Job ${index}`,
      location: { city: "Dublin" },
      releasedDate,
    }));
    const smartFetch: JobsFetch = vi.fn(async (url) => {
      const parsed = new URL(url);
      const offset = Number(parsed.searchParams.get("offset"));
      const limit = Number(parsed.searchParams.get("limit"));
      return response({
        totalFound: rows.length,
        content: rows.slice(offset, offset + limit),
      });
    });
    const smart = new PublicAtsDiscoveryProvider({
      fetch: smartFetch,
      maxPages: 5,
      sleep: async () => undefined,
    });
    await expect(
      smart.snapshot({
        kind: "smartrecruiters",
        companyIdentifier: "Experian",
      }),
    ).rejects.toThrow("pagination cap");
    expect(smartFetch).toHaveBeenCalledTimes(5);
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{ kind: "smartrecruiters", companyIdentifier: "Experian" }],
    };
    const traversed: string[] = [];
    let cursor: string | undefined;
    let pageCount = 0;
    do {
      const page = await smart.search({ ...query, cursor });
      traversed.push(...page.jobs.map((job) => job.externalId));
      cursor = page.nextCursor;
      pageCount += 1;
      if (cursor) {
        expect(page.warnings).toEqual(
          expect.arrayContaining([
            expect.stringContaining("Public ATS results are partial"),
          ]),
        );
      } else {
        expect(page.warnings).toBeUndefined();
      }
    } while (cursor);

    expect(pageCount).toBe(7);
    expect(traversed).toEqual(rows.map((row) => row.id));
    expect(new Set(traversed).size).toBe(501);
    expect(smartFetch).toHaveBeenCalledTimes(36);
  });

  it("preserves exact dedupe and cross-listing state across acquisition windows", async () => {
    const releasedDate = new Date().toISOString();
    const sharedDescription = Array.from(
      { length: 40 },
      (_, index) => `shared platform reliability responsibility ${index}`,
    ).join(" ");
    const rows = Array.from({ length: 202 }, (_, index) => ({
      id: `${index}`,
      name: `Job ${index}`,
      company: { name: "Acme" },
      location: { city: "Dublin" },
      releasedDate,
    }));
    const firstListing = {
      ...rows[0]!,
      id: "original-listing",
      name: "Platform Engineer",
      company: { name: "Alpha" },
      jobAd: { sections: { description: { text: sharedDescription } } },
    };
    const crossListing = {
      ...rows[101]!,
      id: "cross-listing",
      name: "Platform Engineer",
      company: { name: "Beta" },
      jobAd: { sections: { description: { text: sharedDescription } } },
    };
    rows[0] = firstListing;
    rows[100] = { ...firstListing };
    rows[101] = crossListing;
    rows[150] = { ...crossListing };

    const fetcher: JobsFetch = vi.fn(async (url) => {
      const parsed = new URL(url);
      const offset = Number(parsed.searchParams.get("offset"));
      const limit = Number(parsed.searchParams.get("limit"));
      return response({
        totalFound: rows.length,
        content: rows.slice(offset, offset + limit),
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 250,
      sources: [
        { kind: "smartrecruiters", companyIdentifier: "Example" },
      ],
    };
    const traversed: string[] = [];
    const pageWarnings: Array<string[] | undefined> = [];
    let cursor: string | undefined;
    do {
      const page = await provider.search({ ...query, cursor });
      traversed.push(...page.jobs.map((job) => job.externalId));
      pageWarnings.push(page.warnings);
      cursor = page.nextCursor;
    } while (cursor);

    expect(traversed).toHaveLength(200);
    expect(traversed.filter((id) => id === "original-listing")).toHaveLength(
      1,
    );
    expect(traversed.filter((id) => id === "cross-listing")).toHaveLength(1);
    expect(
      pageWarnings
        .flatMap((warnings) => warnings ?? [])
        .filter((warning) => warning.includes("possible cross-listed")),
    ).toEqual([
      "1 possible cross-listed posting pair kept separate for original-source comparison.",
    ]);
  });

  it("filters an earlier undated duplicate before same-window dedupe", async () => {
    const recent = new Date().toISOString();
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response({
          jobs: [
            {
              id: "shared",
              title: "Platform Engineer",
              location: { name: "Dublin" },
            },
            {
              id: "shared",
              title: "Platform Engineer",
              location: { name: "Dublin" },
              updated_at: recent,
            },
          ],
        }),
    });

    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      maxPostingAgeDays: 14,
      sources: [
        { kind: "greenhouse", boardToken: "acme", company: "Acme" },
      ],
    });

    expect(page.jobs).toMatchObject([
      { externalId: "shared", postedAt: recent },
    ]);
    expect(page.warnings).toEqual([
      "1 old or undated job was skipped.",
    ]);
  });

  it("does not advance history for a stale duplicate in an earlier window", async () => {
    const recent = new Date().toISOString();
    const rows = Array.from({ length: 101 }, (_, index) => ({
      id: `${index}`,
      name: `Job ${index}`,
      location: { city: "Dublin" },
      releasedDate: recent,
    }));
    rows[0] = {
      id: "shared",
      name: "Platform Engineer",
      location: { city: "Dublin" },
      releasedDate: "45 days ago",
    };
    rows[100] = {
      id: "shared",
      name: "Platform Engineer",
      location: { city: "Dublin" },
      releasedDate: recent,
    };
    const fetcher: JobsFetch = vi.fn(async (url) => {
      const parsed = new URL(url);
      const offset = Number(parsed.searchParams.get("offset"));
      const limit = Number(parsed.searchParams.get("limit"));
      return response({
        totalFound: rows.length,
        content: rows.slice(offset, offset + limit),
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 250,
      sources: [
        {
          kind: "smartrecruiters",
          companyIdentifier: "Example",
          company: "Acme",
        },
      ],
    };
    const traversed: string[] = [];
    let cursor: string | undefined;
    do {
      const page = await provider.search({ ...query, cursor });
      traversed.push(...page.jobs.map((job) => job.externalId));
      cursor = page.nextCursor;
    } while (cursor);

    expect(traversed).toHaveLength(100);
    expect(traversed.filter((id) => id === "shared")).toHaveLength(1);
  });

  it("fails closed instead of weakening exact history after its bounded cap", async () => {
    const releasedDate = new Date().toISOString();
    const rows = Array.from({ length: 700 }, (_, index) => ({
      id: `${index}`,
      name: `Job ${index}`,
      location: { city: "Dublin" },
      releasedDate,
    }));
    const fetcher: JobsFetch = vi.fn(async (url) => {
      const parsed = new URL(url);
      const offset = Number(parsed.searchParams.get("offset"));
      const limit = Number(parsed.searchParams.get("limit"));
      return response({
        totalFound: rows.length,
        content: rows.slice(offset, offset + limit),
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 250,
      sources: [
        { kind: "smartrecruiters", companyIdentifier: "Example" },
      ],
    };
    let cursor: string | undefined;
    let rejection: unknown;
    for (let invocation = 0; invocation < 100; invocation += 1) {
      const callsBefore = vi.mocked(fetcher).mock.calls.length;
      try {
        const page = await provider.search({ ...query, cursor });
        cursor = page.nextCursor;
        expect(cursor).toBeTypeOf("string");
      } catch (error) {
        rejection = error;
      }
      expect(
        vi.mocked(fetcher).mock.calls.length - callsBefore,
      ).toBeLessThanOrEqual(1);
      if (rejection) break;
    }

    expect(rejection).toBeInstanceOf(PublicAtsContinuationHistoryLimitError);
    expect(fetcher).toHaveBeenCalledTimes(39);
  });

  it("traverses all 101 Workday rows while snapshots remain fail-closed", async () => {
    const rows = Array.from({ length: 101 }, (_, index) => ({
      title: `Job ${index}`,
      externalPath: `/job/Dublin/Engineer_JR-${index}`,
      bulletFields: [`JR-${index}`],
      postedOn: "Posted Today",
    }));
    const workdayFetch: JobsFetch = vi.fn(async (_url, init) => {
      const body = JSON.parse(String(init.body)) as {
        offset: number;
        limit: number;
      };
      return response({
        total: rows.length,
        jobPostings: rows.slice(body.offset, body.offset + body.limit),
      });
    });
    const workday = new PublicAtsDiscoveryProvider({
      fetch: workdayFetch,
      maxPages: 5,
      sleep: async () => undefined,
    });
    await expect(
      workday.snapshot({
        kind: "workday",
        tenant: "workday",
        instance: "wd5",
        site: "Workday",
      }),
    ).rejects.toThrow("pagination cap");
    expect(workdayFetch).toHaveBeenCalledTimes(5);
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        {
          kind: "workday",
          tenant: "workday",
          instance: "wd5",
          site: "Workday",
        },
      ],
    };
    const traversed: string[] = [];
    let cursor: string | undefined;
    let pageCount = 0;
    do {
      const page = await workday.search({ ...query, cursor });
      traversed.push(...page.jobs.map((job) => job.externalId));
      cursor = page.nextCursor;
      pageCount += 1;
      if (cursor) {
        expect(page.warnings).toEqual([
          expect.stringContaining("source incomplete"),
          expect.stringContaining("remaining provider rows"),
        ]);
      } else {
        expect(page.warnings).toBeUndefined();
      }
    } while (cursor);

    expect(pageCount).toBe(3);
    expect(traversed).toEqual(rows.map((row) => row.bulletFields[0]!));
    expect(new Set(traversed).size).toBe(101);
    expect(workdayFetch).toHaveBeenCalledTimes(16);
  });

  it("rejects a changed provider boundary before starting the next window", async () => {
    const rows = Array.from({ length: 21 }, (_, index) => ({
      title: `Job ${index}`,
      externalPath: `/job/Dublin/Engineer_JR-${index}`,
      bulletFields: [`JR-${index}`],
      postedOn: "Posted Today",
    }));
    const fetcher: JobsFetch = vi.fn(async (_url, init) => {
      const body = JSON.parse(String(init.body)) as {
        offset: number;
        limit: number;
      };
      return response({
        total: rows.length,
        jobPostings: rows.slice(body.offset, body.offset + body.limit),
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        {
          kind: "workday",
          tenant: "workday",
          instance: "wd5",
          site: "Workday",
        },
      ],
    };

    const first = await provider.search(query);
    expect(first.jobs).toHaveLength(20);
    expect(first.nextCursor).toBeTypeOf("string");
    rows[19] = { ...rows[19]!, title: "Changed boundary row" };
    await expect(
      provider.search({ ...query, cursor: first.nextCursor }),
    ).rejects.toThrow("invalid or stale");
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("rejects a cross-cut swap even when the old one-row sentinel and total stay stable", async () => {
    const rows = Array.from({ length: 21 }, (_, index) => ({
      title: `Job ${index}`,
      externalPath: `/job/Dublin/Engineer_JR-${index}`,
      bulletFields: [`JR-${index}`],
      postedOn: "Posted Today",
    }));
    const fetcher: JobsFetch = vi.fn(async (_url, init) => {
      const body = JSON.parse(String(init.body)) as {
        offset: number;
        limit: number;
      };
      return response({
        total: rows.length,
        jobPostings: rows.slice(body.offset, body.offset + body.limit),
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        {
          kind: "workday",
          tenant: "workday",
          instance: "wd5",
          site: "Workday",
        },
      ],
    };

    const first = await provider.search(query);
    expect(first.nextCursor).toBeTypeOf("string");
    const oldSentinel = rows[19];
    [rows[18], rows[20]] = [rows[20]!, rows[18]!];
    expect(rows[19]).toBe(oldSentinel);
    await expect(
      provider.search({ ...query, cursor: first.nextCursor }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("rejects a swap outside the retained overlap by validating the full prefix", async () => {
    const rows = Array.from({ length: 21 }, (_, index) => ({
      title: `Job ${index}`,
      externalPath: `/job/Dublin/Engineer_JR-${index}`,
      bulletFields: [`JR-${index}`],
      postedOn: "Posted Today",
    }));
    const fetcher: JobsFetch = vi.fn(async (_url, init) => {
      const body = JSON.parse(String(init.body)) as {
        offset: number;
        limit: number;
      };
      return response({
        total: rows.length,
        jobPostings: rows.slice(body.offset, body.offset + body.limit),
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        {
          kind: "workday",
          tenant: "workday",
          instance: "wd5",
          site: "Workday",
        },
      ],
    };

    const first = await provider.search(query);
    expect(first.nextCursor).toBeTypeOf("string");
    const retainedOverlap = rows.slice(10, 20);
    [rows[9], rows[20]] = [rows[20]!, rows[9]!];
    expect(rows.slice(10, 20)).toEqual(retainedOverlap);
    await expect(
      provider.search({ ...query, cursor: first.nextCursor }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("fails closed when an overlap-only continuation makes no forward progress", async () => {
    const rows = Array.from({ length: 40 }, (_, index) => ({
      title: `Job ${index}`,
      externalPath: `/job/Dublin/Engineer_JR-${index}`,
      bulletFields: [`JR-${index}`],
      postedOn: "Posted Today",
    }));
    const fetcher: JobsFetch = vi.fn(async (_url, init) => {
      const body = JSON.parse(String(init.body)) as {
        offset: number;
        limit: number;
      };
      const length = body.offset === 0 ? body.limit : 10;
      return response({
        total: rows.length,
        jobPostings: rows.slice(body.offset, body.offset + length),
      });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [
        {
          kind: "workday",
          tenant: "workday",
          instance: "wd5",
          site: "Workday",
        },
      ],
    };

    const first = await provider.search(query);
    expect(first.jobs).toHaveLength(20);
    expect(first.nextCursor).toBeTypeOf("string");
    const validated = await provider.search({
      ...query,
      cursor: first.nextCursor,
    });
    expect(validated.jobs).toEqual([]);
    expect(validated.nextCursor).toBeTypeOf("string");
    await expect(
      provider.search({ ...query, cursor: validated.nextCursor }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).toHaveBeenCalledTimes(3);
  });

  it("rejects more than 24 configured sources before any fetch fan-out", async () => {
    const fetcher: JobsFetch = vi.fn(async () => response({ jobs: [] }));
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher });

    await expect(
      provider.search({
        roles: [],
        locations: [],
        remotePreference: "any",
        excludedCompanies: [],
        sources: Array.from({ length: 25 }, (_, index) => ({
          kind: "greenhouse" as const,
          boardToken: `company-${index}`,
        })),
      }),
    ).rejects.toThrow("at most 24 sources");
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("rejects a checksummed cursor carrying more than 24 source states", async () => {
    const updatedAt = new Date().toISOString();
    const fetcher: JobsFetch = vi.fn(async () =>
      response({
        jobs: [
          { id: "first", title: "Engineer I", updated_at: updatedAt },
          { id: "second", title: "Engineer II", updated_at: updatedAt },
        ],
      }),
    );
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 1,
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    };
    const first = await provider.search(query);
    const oversized = rewritePublicAtsCursor(first.nextCursor!, (payload) => {
      payload.sources = Array.from(
        { length: 25 },
        () => ({ ...payload.sources[0]! }),
      );
    });

    await expect(
      provider.search({ ...query, cursor: oversized }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("retries transient ATS errors without following redirects", async () => {
    let calls = 0;
    const fetcher: JobsFetch = vi.fn(async () => {
      calls += 1;
      return calls === 1
        ? response({ message: "busy" }, 429)
        : response({ jobs: [] });
    });
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxAttempts: 2,
      sleep: async () => undefined,
    });
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
          throw new Error(
            "streaming response must not be buffered through text()",
          );
        },
      }),
    });

    await expect(
      provider.search({
        roles: [],
        locations: [],
        remotePreference: "any",
        excludedCompanies: [],
        sources: [{ kind: "greenhouse", boardToken: "acme" }],
      }),
    ).rejects.toThrow("size limit");
    expect(pulls).toBeLessThanOrEqual(7);
    expect(cancelled).toBe(true);
  });

  it("rejects source identifiers that could escape the pinned endpoint", async () => {
    const provider = new PublicAtsDiscoveryProvider({ fetch: vi.fn() });
    await expect(
      provider.search({
        roles: [],
        locations: [],
        remotePreference: "any",
        excludedCompanies: [],
        sources: [{ kind: "greenhouse", boardToken: "../private" }],
      }),
    ).rejects.toThrow("unsupported characters");
  });

  it("ignores blank exclusions instead of excluding every job", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response({
          jobs: [
            {
              id: "1",
              title: "Engineer",
              absolute_url: "https://boards.greenhouse.io/acme/jobs/1",
              location: { name: "Remote" },
              updated_at: new Date().toISOString(),
            },
          ],
        }),
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

  it("does not discard candidates with raw worker-side role or location substrings", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async () =>
        response({
          jobs: [
            {
              id: "1",
              title: "SDE II",
              absolute_url: "https://boards.greenhouse.io/acme/jobs/1",
              location: { name: "Austin, TX" },
              updated_at: new Date().toISOString(),
            },
          ],
        }),
    });
    const page = await provider.search({
      roles: ["software engineer"],
      locations: ["New York, NY"],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{ kind: "greenhouse", boardToken: "acme", company: "Acme" }],
    });
    expect(page.jobs).toHaveLength(1);
    expect(page.jobs[0]).toMatchObject({
      title: "SDE II",
      location: "Austin, TX",
    });
  });

  it("continues past page boundaries without losing a desired role alias", async () => {
    const updatedAt = new Date().toISOString();
    const fetcher: JobsFetch = vi.fn(async () =>
      response({
        jobs: [
          {
            id: "first",
            title: "Accountant",
            location: { name: "New York, NY" },
            updated_at: updatedAt,
          },
          {
            id: "desired-alias",
            title: "SDE II",
            location: { name: "Austin, TX" },
            updated_at: updatedAt,
          },
        ],
      }),
    );
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher });
    const query: DiscoveryQuery = {
      roles: ["software engineer"],
      locations: ["New York, NY"],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 1,
      sources: [{ kind: "greenhouse", boardToken: "acme", company: "Acme" }],
    };

    const first = await provider.search(query);
    expect(first.jobs.map((job) => job.externalId)).toEqual(["first"]);
    expect(first.nextCursor).toBeTypeOf("string");
    expect(first.warnings).toEqual([
      "Public ATS results are partial: showing bounded candidates 1-1 of 2; continue with nextCursor.",
    ]);

    const second = await provider.search({
      ...query,
      cursor: first.nextCursor,
    });
    expect(second.jobs).toMatchObject([
      { externalId: "desired-alias", title: "SDE II" },
    ]);
    expect(second.nextCursor).toBeUndefined();
    expect(second.warnings).toBeUndefined();
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("fails closed for malformed, tampered, query-changed, and stale cursors", async () => {
    const updatedAt = new Date().toISOString();
    let mode: "normal" | "changed" | "reordered" = "normal";
    const fetcher: JobsFetch = vi.fn(async () => {
      const jobs = [
        {
          id: "first",
          title: "Engineer I",
          updated_at: updatedAt,
        },
        {
          id: "second",
          title: mode === "changed" ? "Engineer III" : "Engineer II",
          updated_at: updatedAt,
        },
      ];
      return response({ jobs: mode === "reordered" ? jobs.reverse() : jobs });
    });
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 1,
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    };

    await expect(
      provider.search({ ...query, cursor: "not-a-canonical-cursor!" }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).not.toHaveBeenCalled();

    const first = await provider.search(query);
    expect(first.nextCursor).toBeTypeOf("string");
    await expect(
      provider.search({
        ...query,
        roles: ["changed query"],
        cursor: first.nextCursor,
      }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    await expect(
      provider.search({
        ...query,
        cursor: tamperPublicAtsCursor(first.nextCursor!),
      }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).toHaveBeenCalledTimes(1);

    mode = "changed";
    await expect(
      provider.search({ ...query, cursor: first.nextCursor }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    mode = "normal";
    const stable = await provider.search(query);
    mode = "reordered";
    await expect(
      provider.search({ ...query, cursor: stable.nextCursor }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).toHaveBeenCalledTimes(4);
  });

  it("rejects checksummed misaligned and exhausted window offsets", async () => {
    const updatedAt = new Date().toISOString();
    const fetcher: JobsFetch = vi.fn(async () =>
      response({
        jobs: Array.from({ length: 4 }, (_, index) => ({
          id: `${index}`,
          title: `Engineer ${index}`,
          updated_at: updatedAt,
        })),
      }),
    );
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 2,
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    };
    const first = await provider.search(query);
    expect(first.nextCursor).toBeTypeOf("string");

    const misaligned = rewritePublicAtsCursor(first.nextCursor!, (payload) => {
      payload.pageOffset = 3;
    });
    await expect(
      provider.search({ ...query, cursor: misaligned }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);

    const exhausted = rewritePublicAtsCursor(first.nextCursor!, (payload) => {
      payload.pageOffset = 4;
    });
    await expect(
      provider.search({ ...query, cursor: exhausted }),
    ).rejects.toBeInstanceOf(InvalidPublicAtsCursorError);
    expect(fetcher).toHaveBeenCalledTimes(3);
  });

  it("continues in source order when requests settle out of order", async () => {
    const updatedAt = new Date().toISOString();
    const fetcher: JobsFetch = vi.fn(async (url) => {
      if (url.includes("greenhouse")) {
        await new Promise<void>((resolve) => queueMicrotask(resolve));
        return response({
          jobs: [
            {
              id: "greenhouse-first",
              title: "Greenhouse Engineer",
              updated_at: updatedAt,
            },
          ],
        });
      }
      return response([
        {
          id: "lever-second",
          text: "Lever Engineer",
          createdAt: updatedAt,
        },
      ]);
    });
    const provider = new PublicAtsDiscoveryProvider({ fetch: fetcher });
    const query: DiscoveryQuery = {
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      pageSize: 1,
      sources: [
        { kind: "greenhouse", boardToken: "acme" },
        { kind: "lever", site: "atlas" },
      ],
    };

    const first = await provider.search(query);
    expect(first.jobs.map((job) => job.externalId)).toEqual([
      "greenhouse-first",
    ]);
    expect(first.nextCursor).toBeTypeOf("string");
    const second = await provider.search({
      ...query,
      cursor: first.nextCursor,
    });
    expect(second.jobs.map((job) => job.externalId)).toEqual([
      "lever-second",
    ]);
    expect(second.nextCursor).toBeUndefined();
    expect(fetcher).toHaveBeenCalledTimes(4);
  });

  it("keeps healthy sources when another configured source fails", async () => {
    const fetcher: JobsFetch = vi.fn(async (url) =>
      url.includes("greenhouse")
        ? response({ error: "down" }, 503)
        : response([
            {
              id: "lever-1",
              text: "Engineer",
              hostedUrl: "https://jobs.lever.co/atlas/lever-1",
              categories: { location: "Remote" },
              createdAt: Date.now(),
            },
          ]),
    );
    const provider = new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxAttempts: 1,
    });
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
      fetch: async () =>
        response({
          jobs: [
            {
              id: "1",
              title: "Engineer",
              absolute_url: "http://127.0.0.1/admin",
              updated_at: new Date().toISOString(),
            },
          ],
        }),
    });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      sources: [{ kind: "greenhouse", boardToken: "acme" }],
    });
    expect(page.jobs[0]?.canonicalUrl).toBe(
      "https://boards.greenhouse.io/acme/jobs/1",
    );
  });

  it("does not treat a provider payload's custom careers URL as source authority", async () => {
    const provider = new PublicAtsDiscoveryProvider({
      fetch: async (url) =>
        url.includes("greenhouse")
          ? response({
              jobs: [
                {
                  id: "gh-1",
                  title: "Engineer",
                  absolute_url:
                    "https://careers.example.test/opening?gh_jid=gh-1",
                  updated_at: new Date().toISOString(),
                },
              ],
            })
          : response([
              {
                id: "lever-1",
                text: "Designer",
                hostedUrl: "https://careers.example.test/designer",
                createdAt: Date.now(),
              },
            ]),
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
      fetch: async () =>
        response({
          jobs: [
            {
              id: "recent",
              title: "Engineer",
              absolute_url: "https://boards.greenhouse.io/acme/jobs/recent",
              updated_at: "3 days ago",
            },
            {
              id: "old",
              title: "Engineer",
              absolute_url: "https://boards.greenhouse.io/acme/jobs/old",
              updated_at: "45 days ago",
            },
            {
              id: "undated",
              title: "Engineer",
              absolute_url: "https://boards.greenhouse.io/acme/jobs/undated",
            },
          ],
        }),
    });
    const page = await provider.search({
      roles: [],
      locations: [],
      remotePreference: "any",
      excludedCompanies: [],
      maxPostingAgeDays: 14,
      sources: [{ kind: "greenhouse", boardToken: "acme", company: "Acme" }],
    });
    expect(page.jobs.map((job) => job.externalId)).toEqual(["recent"]);
    expect(page.warnings?.[0]).toContain("2 old or undated jobs were skipped");
  });

  it("parses ATS relative dates against a stable clock", () => {
    const now = new Date("2026-07-10T12:00:00.000Z");
    expect(parsePostedAt("Posted Today", now)?.toISOString()).toBe(
      now.toISOString(),
    );
    expect(parsePostedAt("2 weeks ago", now)?.toISOString()).toBe(
      "2026-06-26T12:00:00.000Z",
    );
    expect(
      isRecentJob(
        {
          externalId: "job",
          canonicalUrl: "https://example.com/job",
          company: "Acme",
          title: "Engineer",
          location: "Remote",
          workplace: "remote",
          description: "",
          source: "greenhouse",
          postedAt: "13 days ago",
        },
        14,
        now,
      ),
    ).toBe(true);
  });
});
