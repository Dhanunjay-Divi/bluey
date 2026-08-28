import { Buffer } from "node:buffer";

import { describe, expect, it, vi } from "vitest";

import {
  OriginalSourceVerifier,
  originalSourceSha256,
  parseOriginalSourceVerificationSubject,
  type OriginalSourceProviderFamily,
  type OriginalSourceVerificationSubject,
} from "../src/original-source-verification.js";
import {
  PublicAtsDiscoveryProvider,
  type FetchResponse,
  type JobsFetch,
} from "../src/public-ats.js";

function subject(
  family: OriginalSourceProviderFamily,
  overrides: Partial<OriginalSourceVerificationSubject> = {},
): OriginalSourceVerificationSubject {
  const variants: Record<
    OriginalSourceProviderFamily,
    OriginalSourceVerificationSubject
  > = {
    greenhouse: baseSubject({
      provider_family: "greenhouse",
      provider_record_id: "greenhouse:acme:job-1",
      provider_target: {
        host: "boards.greenhouse.io",
        tenant: "acme",
        job: "job-1",
        variant: "greenhouse_public",
      },
      original_url: "https://boards.greenhouse.io/acme/jobs/job-1",
    }),
    lever: baseSubject({
      provider_family: "lever",
      provider_record_id: "lever:jobs.lever.co:acme:lever-1",
      provider_target: {
        host: "jobs.lever.co",
        tenant: "acme",
        job: "lever-1",
        variant: "lever_posting",
      },
      original_url: "https://jobs.lever.co/acme/lever-1",
    }),
    ashby: baseSubject({
      provider_family: "ashby",
      provider_record_id: "ashby:acme:ashby-1",
      provider_target: {
        host: "jobs.ashbyhq.com",
        tenant: "acme",
        job: "ashby-1",
        variant: "ashby_posting",
      },
      original_url: "https://jobs.ashbyhq.com/acme/ashby-1",
    }),
    smartrecruiters: baseSubject({
      provider_family: "smartrecruiters",
      provider_record_id: "smartrecruiters:acme:smart-1",
      provider_target: {
        host: "jobs.smartrecruiters.com",
        tenant: "acme",
        job: "smart-1",
        variant: "smartrecruiters_posting",
      },
      original_url: "https://jobs.smartrecruiters.com/acme/smart-1",
    }),
    workday: baseSubject({
      provider_family: "workday",
      provider_record_id: "workday:acme.wd5.myworkdayjobs.com:acme:JR-101",
      provider_target: {
        host: "acme.wd5.myworkdayjobs.com",
        tenant: "acme",
        job: "JR-101",
        variant: "workday_posting",
      },
      original_url:
        "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/JR-101",
    }),
  };
  return { ...variants[family], ...overrides };
}

function baseSubject(
  identity: Pick<
    OriginalSourceVerificationSubject,
    | "original_url"
    | "provider_family"
    | "provider_record_id"
    | "provider_target"
  >,
): OriginalSourceVerificationSubject {
  return {
    schema_version: 1,
    canonical_job_id: "canonical-job-acme-platform",
    employer_id: "employer-acme",
    ...identity,
    expected: {
      company: "Acme",
      title: "Platform Engineer",
      location: "Austin, TX",
      workplace: "hybrid",
      description: "Build reliable systems.",
      compensation: "USD 120000-160000 year",
      employment_type: "full_time",
      posted_at_ms: Date.parse("2026-08-25T12:00:00Z"),
      availability_status: "active",
    },
  };
}

function response(
  payload: unknown,
  status = 200,
  headers: Record<string, string> = {},
): FetchResponse {
  return rawResponse(JSON.stringify(payload), status, headers);
}

function rawResponse(
  payload: string | Uint8Array,
  status = 200,
  headers: Record<string, string> = {},
): FetchResponse {
  const bytes =
    typeof payload === "string"
      ? Buffer.from(payload, "utf8")
      : Buffer.from(payload);
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers({ "content-type": "application/json", ...headers }),
    body: new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(Uint8Array.from(bytes));
        controller.close();
      },
    }),
    text: async () => new TextDecoder("utf-8", { fatal: true }).decode(bytes),
  };
}

const PROVIDER_FAMILIES: OriginalSourceProviderFamily[] = [
  "ashby",
  "greenhouse",
  "lever",
  "smartrecruiters",
  "workday",
];

function positiveProviderFixture(family: OriginalSourceProviderFamily): {
  payload: unknown;
  subject: OriginalSourceVerificationSubject;
} {
  switch (family) {
    case "greenhouse":
      return {
        subject: subject("greenhouse", {
          expected: { ...subject("greenhouse").expected, compensation: "" },
        }),
        payload: {
          id: "job-1",
          absolute_url: "https://boards.greenhouse.io/acme/jobs/job-1",
          title: "Platform Engineer",
          location: { name: "Austin, TX" },
          workplace_type: "hybrid",
          content: "<p>Build reliable systems.</p>",
          employment_type: "Full-time",
          updated_at: "2026-08-25T12:00:00Z",
        },
      };
    case "lever":
      return {
        subject: subject("lever"),
        payload: {
          id: "lever-1",
          hostedUrl: "https://jobs.lever.co/acme/lever-1",
          applyUrl: "https://jobs.lever.co/acme/lever-1/apply",
          text: "Platform Engineer",
          categories: { location: "Austin, TX", commitment: "Full-time" },
          workplaceType: "hybrid",
          descriptionPlain: "Build reliable systems.",
          salaryRange: {
            currency: "USD",
            min: 120000,
            max: 160000,
            interval: "year",
          },
          createdAt: Date.parse("2026-08-25T12:00:00Z"),
        },
      };
    case "ashby":
      return {
        subject: subject("ashby"),
        payload: {
          jobs: [
            {
              id: "ashby-1",
              isListed: true,
              jobUrl: "https://jobs.ashbyhq.com/acme/ashby-1",
              applyUrl: "https://jobs.ashbyhq.com/acme/ashby-1/application",
              title: "Platform Engineer",
              location: "Austin, TX",
              workplaceType: "hybrid",
              descriptionPlain: "Build reliable systems.",
              compensation: "USD 120000-160000 year",
              employmentType: "Full Time",
              publishedAt: "2026-08-25T12:00:00Z",
            },
          ],
        },
      };
    case "smartrecruiters":
      return {
        subject: subject("smartrecruiters"),
        payload: {
          id: "smart-1",
          name: "Platform Engineer",
          company: { identifier: "acme", name: "Acme" },
          applyUrl:
            "https://www.smartrecruiters.com/acme/smart-1-platform-engineer",
          ref: "https://api.smartrecruiters.com/v1/companies/acme/postings/smart-1",
          location: { city: "Austin", region: "TX" },
          workplaceType: "hybrid",
          jobAd: {
            sections: { description: { text: "Build reliable systems." } },
          },
          compensation: "USD 120000-160000 year",
          typeOfEmployment: { label: "Full-time" },
          releasedDate: "2026-08-25T12:00:00Z",
        },
      };
    case "workday":
      return {
        subject: subject("workday", {
          expected: { ...subject("workday").expected, compensation: "" },
        }),
        payload: {
          jobPostingInfo: {
            jobReqId: "JR-101",
            title: "Platform Engineer",
            location: "Austin, TX",
            workplaceType: "hybrid",
            jobDescription: "Build reliable systems.",
            timeType: "Full time",
            startDate: "2026-08-25T12:00:00Z",
            externalUrl:
              "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/JR-101",
          },
        },
      };
  }
}

function duplicateIdentityJson(
  family: OriginalSourceProviderFamily,
  payload: unknown,
  escaped = false,
): string {
  const identity = {
    ashby: ["id", "ashby-1"],
    greenhouse: ["id", "job-1"],
    lever: ["id", "lever-1"],
    smartrecruiters: ["id", "smart-1"],
    workday: ["jobReqId", "JR-101"],
  }[family]!;
  const raw = JSON.stringify(payload);
  const key = escaped
    ? identity[0]!
        .split("")
        .map(
          (character) =>
            `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`,
        )
        .join("")
    : identity[0]!;
  const expected = `"${identity[0]}":"${identity[1]}"`;
  const replacement = `"${identity[0]}":"wrong-provider-record","${key}":"${identity[1]}"`;
  const duplicated = raw.replace(expected, replacement);
  if (duplicated === raw) throw new Error(`Missing ${family} identity fixture`);
  return duplicated;
}

function mutatedProviderFixture(
  family: OriginalSourceProviderFamily,
  mutate: (payload: Record<string, unknown>) => void,
): { payload: unknown; subject: OriginalSourceVerificationSubject } {
  const fixture = positiveProviderFixture(family);
  const payload = structuredClone(fixture.payload) as Record<string, unknown>;
  mutate(payload);
  return { payload, subject: fixture.subject };
}

describe("original-source provider verification", () => {
  it("has an exact positive observation fixture for every hosted ATS family", async () => {
    const fixtures: Array<{
      applicationUrl: string;
      family: OriginalSourceProviderFamily;
      payload: unknown;
      subject: OriginalSourceVerificationSubject;
    }> = [
      {
        applicationUrl: "https://boards.greenhouse.io/acme/jobs/job-1",
        family: "greenhouse",
        subject: subject("greenhouse", {
          expected: { ...subject("greenhouse").expected, compensation: "" },
        }),
        payload: {
          id: "job-1",
          absolute_url: "https://boards.greenhouse.io/acme/jobs/job-1",
          title: "Platform Engineer",
          location: { name: "Austin, TX" },
          workplace_type: "hybrid",
          content: "<p>Build reliable systems.</p>",
          employment_type: "Full-time",
          updated_at: "2026-08-25T12:00:00Z",
        },
      },
      {
        applicationUrl: "https://jobs.lever.co/acme/lever-1/apply",
        family: "lever",
        subject: subject("lever"),
        payload: {
          id: "lever-1",
          hostedUrl: "https://jobs.lever.co/acme/lever-1",
          applyUrl: "https://jobs.lever.co/acme/lever-1/apply",
          text: "Platform Engineer",
          categories: { location: "Austin, TX", commitment: "Full-time" },
          workplaceType: "hybrid",
          descriptionPlain: "Build reliable systems.",
          salaryRange: {
            currency: "USD",
            min: 120000,
            max: 160000,
            interval: "year",
          },
          createdAt: Date.parse("2026-08-25T12:00:00Z"),
        },
      },
      {
        applicationUrl: "https://jobs.ashbyhq.com/acme/ashby-1/application",
        family: "ashby",
        subject: subject("ashby"),
        payload: {
          jobs: [
            {
              id: "ashby-1",
              isListed: true,
              jobUrl: "https://jobs.ashbyhq.com/acme/ashby-1",
              applyUrl: "https://jobs.ashbyhq.com/acme/ashby-1/application",
              title: "Platform Engineer",
              location: "Austin, TX",
              workplaceType: "hybrid",
              descriptionPlain: "Build reliable systems.",
              compensation: "USD 120000-160000 year",
              employmentType: "Full Time",
              publishedAt: "2026-08-25T12:00:00Z",
            },
          ],
        },
      },
      {
        applicationUrl:
          "https://www.smartrecruiters.com/acme/smart-1-platform-engineer",
        family: "smartrecruiters",
        subject: subject("smartrecruiters"),
        payload: {
          id: "smart-1",
          name: "Platform Engineer",
          company: { identifier: "acme", name: "Acme" },
          applyUrl:
            "https://www.smartrecruiters.com/acme/smart-1-platform-engineer",
          ref: "https://api.smartrecruiters.com/v1/companies/acme/postings/smart-1",
          location: { city: "Austin", region: "TX" },
          workplaceType: "hybrid",
          jobAd: {
            sections: { description: { text: "Build reliable systems." } },
          },
          compensation: "USD 120000-160000 year",
          typeOfEmployment: { label: "Full-time" },
          releasedDate: "2026-08-25T12:00:00Z",
        },
      },
      {
        applicationUrl:
          "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/JR-101",
        family: "workday",
        subject: subject("workday", {
          expected: { ...subject("workday").expected, compensation: "" },
        }),
        payload: {
          jobPostingInfo: {
            jobReqId: "JR-101",
            title: "Platform Engineer",
            location: "Austin, TX",
            workplaceType: "hybrid",
            jobDescription: "Build reliable systems.",
            timeType: "Full time",
            startDate: "2026-08-25T12:00:00Z",
            externalUrl:
              "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/JR-101",
          },
        },
      },
    ];

    for (const fixture of fixtures) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response(fixture.payload),
      }).verify(fixture.subject);
      expect(result, fixture.family).toMatchObject({
        kind: "complete",
        observation: {
          result: "open",
          canonical_application_url: fixture.applicationUrl,
          application_domain: new URL(fixture.applicationUrl).hostname,
          mismatched_fields: [],
        },
      });
    }
  });

  it("rejects raw duplicate provider keys before last-wins JSON parsing", async () => {
    for (const family of PROVIDER_FAMILIES) {
      const fixture = positiveProviderFixture(family);
      const result = await new OriginalSourceVerifier({
        fetch: async () =>
          rawResponse(duplicateIdentityJson(family, fixture.payload)),
      }).verify(fixture.subject);
      expect(result, family).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
        observation: { result: "indeterminate" },
      });
    }

    for (const family of ["greenhouse", "ashby"] as const) {
      const fixture = positiveProviderFixture(family);
      const escaped = await new OriginalSourceVerifier({
        fetch: async () =>
          rawResponse(duplicateIdentityJson(family, fixture.payload, true)),
      }).verify(fixture.subject);
      expect(escaped, `${family}:escaped-key`).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
      });
    }
  });

  it("rejects contradictory provider aliases instead of choosing one", async () => {
    const conflicts = [
      mutatedProviderFixture("greenhouse", (payload) => {
        payload.name = "Security Engineer";
      }),
      mutatedProviderFixture("greenhouse", (payload) => {
        payload.employmentType = "Part-time";
      }),
      mutatedProviderFixture("lever", (payload) => {
        payload.location = "Dallas, TX";
      }),
      mutatedProviderFixture("lever", (payload) => {
        payload.description = "Operate a different system.";
      }),
      mutatedProviderFixture("ashby", (payload) => {
        const jobs = payload.jobs as Array<Record<string, unknown>>;
        jobs[0]!.jobId = "ashby-other";
      }),
      mutatedProviderFixture("ashby", (payload) => {
        const jobs = payload.jobs as Array<Record<string, unknown>>;
        jobs[0]!.descriptionHtml = "<p>Operate a different system.</p>";
      }),
      mutatedProviderFixture("ashby", (payload) => {
        const jobs = payload.jobs as Array<Record<string, unknown>>;
        jobs[0]!.employmentTypeLabel = "Part Time";
      }),
      mutatedProviderFixture("smartrecruiters", (payload) => {
        payload.employmentType = "Part-time";
      }),
      mutatedProviderFixture("smartrecruiters", (payload) => {
        (payload.location as Record<string, unknown>).remote = true;
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).id = "JR-999";
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).description =
          "Operate a different system.";
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).postedOn =
          "2026-08-26T12:00:00Z";
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).locationText =
          "Dallas, TX";
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).employmentType =
          "Part-time";
      }),
    ];

    for (const [index, fixture] of conflicts.entries()) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response(fixture.payload),
      }).verify(fixture.subject);
      expect(result, `alias-conflict:${index}`).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
      });
    }
  });

  it("rejects malformed present aliases and nested provider records", async () => {
    const malformed = [
      mutatedProviderFixture("greenhouse", (payload) => {
        payload.name = {};
      }),
      mutatedProviderFixture("greenhouse", (payload) => {
        payload.name = 42;
      }),
      mutatedProviderFixture("greenhouse", (payload) => {
        payload.location = [];
      }),
      mutatedProviderFixture("greenhouse", (payload) => {
        payload.workplace_type = {};
      }),
      mutatedProviderFixture("lever", (payload) => {
        payload.location = {};
      }),
      mutatedProviderFixture("lever", (payload) => {
        payload.categories = [];
      }),
      mutatedProviderFixture("lever", (payload) => {
        payload.lists = {};
      }),
      mutatedProviderFixture("lever", (payload) => {
        payload.salaryRange = [];
      }),
      mutatedProviderFixture("ashby", (payload) => {
        const jobs = payload.jobs as Array<Record<string, unknown>>;
        jobs[0]!.jobId = {};
      }),
      mutatedProviderFixture("ashby", (payload) => {
        const jobs = payload.jobs as Array<Record<string, unknown>>;
        jobs[0]!.isListed = {};
      }),
      mutatedProviderFixture("ashby", (payload) => {
        const jobs = payload.jobs as Array<Record<string, unknown>>;
        jobs[0]!.workplaceType = {};
      }),
      mutatedProviderFixture("smartrecruiters", (payload) => {
        (payload.location as Record<string, unknown>).remote = {};
      }),
      mutatedProviderFixture("smartrecruiters", (payload) => {
        payload.location = [];
      }),
      mutatedProviderFixture("smartrecruiters", (payload) => {
        payload.jobAd = [];
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).id = {};
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).hiringOrganization =
          [];
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).postedOn = {};
      }),
      mutatedProviderFixture("workday", (payload) => {
        (payload.jobPostingInfo as Record<string, unknown>).workplaceType = {};
      }),
    ];

    for (const [index, fixture] of malformed.entries()) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response(fixture.payload),
      }).verify(fixture.subject);
      expect(result, `malformed-present-field:${index}`).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
        observation: { result: "indeterminate" },
      });
    }
  });

  it("accepts equivalent provider aliases after their typed normalization", async () => {
    const fixtures = [
      mutatedProviderFixture("greenhouse", (payload) => {
        payload.name = "  platform   engineer ";
        payload.employmentType = "full time";
      }),
      mutatedProviderFixture("lever", (payload) => {
        payload.location = "austin,   tx";
        payload.description = "<p>Build reliable systems.</p>";
      }),
      mutatedProviderFixture("workday", (payload) => {
        const row = payload.jobPostingInfo as Record<string, unknown>;
        row.id = "JR-101";
        row.jobId = "JR-101";
        row.description = "<p>Build reliable systems.</p>";
        row.postedOn = Date.parse("2026-08-25T12:00:00Z");
        row.locationText = "austin,   tx";
        row.employmentType = "full time";
      }),
    ];
    for (const [index, fixture] of fixtures.entries()) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response(fixture.payload),
      }).verify(fixture.subject);
      expect(result, `alias-equivalent:${index}`).toMatchObject({
        kind: "complete",
        observation: { result: "open", mismatched_fields: [] },
      });
    }
  });

  it("fatally rejects malformed UTF-8 and hashes the exact bounded octets", async () => {
    const prefix = Buffer.from(
      "bluey-jobs-original-source-content-v1\0",
      "utf8",
    );
    const digests: string[] = [];
    for (const invalidByte of [0x80, 0x81]) {
      const bytes = Buffer.concat([
        Buffer.from('{"id":"', "utf8"),
        Buffer.from([invalidByte]),
        Buffer.from('"}', "utf8"),
      ]);
      const result = await new OriginalSourceVerifier({
        fetch: async () => rawResponse(bytes),
      }).verify(subject("greenhouse"));
      expect(result).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
        observation: { result: "indeterminate" },
      });
      if (result.kind !== "fail")
        throw new Error("Malformed UTF-8 was accepted");
      const expectedDigest = originalSourceSha256(
        Buffer.concat([prefix, bytes]),
      );
      expect(result.observation.content_digest).toBe(expectedDigest);
      digests.push(result.observation.content_digest);
    }
    expect(new Set(digests).size).toBe(2);
  });

  it("requires a raw byte stream before provider content can be trusted", async () => {
    const result = await new OriginalSourceVerifier({
      fetch: async () => ({
        ok: true,
        status: 200,
        headers: new Headers({ "content-type": "application/json" }),
        text: async () =>
          JSON.stringify(positiveProviderFixture("greenhouse").payload),
      }),
    }).verify(positiveProviderFixture("greenhouse").subject);
    expect(result).toMatchObject({
      kind: "fail",
      error_code: "parse_ambiguous",
    });
  });

  it("accepts only the closed JSON media-type and identity-encoding contract", async () => {
    const fixture = positiveProviderFixture("greenhouse");
    for (const headers of [
      { "content-type": "application/json; charset=utf-8" },
      {
        "content-encoding": "identity",
        "content-type": 'APPLICATION/JSON; CHARSET="UTF-8"',
      },
    ]) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response(fixture.payload, 200, headers),
      }).verify(fixture.subject);
      expect(result, JSON.stringify(headers)).toMatchObject({
        kind: "complete",
        observation: { result: "open" },
      });
    }

    for (const headers of [
      { "content-type": "text/application/json-evil" },
      { "content-type": "application/jsonp" },
      { "content-type": "application/json; charset=iso-8859-1" },
      { "content-type": "application/json; profile=provider" },
      { "content-type": `application/json;${"x".repeat(4_097)}` },
      { "content-encoding": "gzip", "content-type": "application/json" },
      { "content-encoding": "br", "content-type": "application/json" },
    ]) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response(fixture.payload, 200, headers),
      }).verify(fixture.subject);
      expect(result, JSON.stringify(headers)).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
        observation: { result: "indeterminate" },
      });
    }
  });

  it("digest-binds the exact bounded content encoding metadata", async () => {
    const fixture = positiveProviderFixture("greenhouse");
    const absent = await new OriginalSourceVerifier({
      fetch: async () => response(fixture.payload),
    }).verify(fixture.subject);
    const identity = await new OriginalSourceVerifier({
      fetch: async () =>
        response(fixture.payload, 200, { "content-encoding": "identity" }),
    }).verify(fixture.subject);
    expect(absent).toMatchObject({
      kind: "complete",
      observation: { result: "open" },
    });
    expect(identity).toMatchObject({
      kind: "complete",
      observation: { result: "open" },
    });
    if (absent.kind !== "complete" || identity.kind !== "complete") {
      throw new Error("Allowed JSON response metadata was rejected");
    }
    expect(identity.observation.headers_digest).not.toBe(
      absent.observation.headers_digest,
    );
  });

  it("fails closed for every provider across the frozen adversarial matrix", async () => {
    for (const family of PROVIDER_FAMILIES) {
      const fixture = positiveProviderFixture(family);

      for (const status of [404, 410]) {
        const closed = await new OriginalSourceVerifier({
          fetch: async () => response({ error: "missing" }, status),
        }).verify(fixture.subject);
        expect(closed, `${family}:closed:${status}`).toMatchObject({
          kind: "complete",
          observation: {
            result: "closed",
            retrieval_status: status === 404 ? "not_found" : "gone",
            http_status: status,
          },
        });
      }

      const mismatch = await new OriginalSourceVerifier({
        fetch: async () => response(fixture.payload),
      }).verify({
        ...fixture.subject,
        expected: {
          ...fixture.subject.expected,
          title: "Different exact title",
        },
      });
      expect(mismatch, `${family}:mismatch`).toMatchObject({
        kind: "complete",
        observation: {
          result: "mismatch",
          mismatched_fields: expect.arrayContaining(["title"]),
        },
      });

      const redirect = await new OriginalSourceVerifier({
        maxAttempts: 1,
        fetch: async () =>
          response({}, 302, { location: "https://invalid.example" }),
      }).verify(fixture.subject);
      expect(redirect, `${family}:redirect`).toMatchObject({
        kind: "fail",
        error_code: "source_untrusted",
      });

      const malformed = await new OriginalSourceVerifier({
        fetch: async () => response({}),
      }).verify(fixture.subject);
      expect(malformed, `${family}:malformed`).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
      });

      const auth = await new OriginalSourceVerifier({
        fetch: async () => response({}, 401),
      }).verify(fixture.subject);
      expect(auth, `${family}:auth`).toMatchObject({
        kind: "fail",
        error_code: "auth_required",
      });

      const captcha = await new OriginalSourceVerifier({
        fetch: async () =>
          response("<html>Verify you are human</html>", 200, {
            "content-type": "text/html",
          }),
      }).verify(fixture.subject);
      expect(captcha, `${family}:captcha`).toMatchObject({
        kind: "fail",
        error_code: "captcha_required",
      });

      const rateLimited = await new OriginalSourceVerifier({
        maxAttempts: 1,
        fetch: async () => response({}, 429),
      }).verify(fixture.subject);
      expect(rateLimited, `${family}:rate-limit`).toMatchObject({
        kind: "fail",
        error_code: "rate_limited",
      });

      const timedOut = await new OriginalSourceVerifier({
        maxAttempts: 1,
        timeoutMs: 100,
        fetch: async (_url, init) =>
          new Promise<FetchResponse>((_resolve, reject) => {
            init.signal?.addEventListener(
              "abort",
              () => reject(new Error("synthetic timeout")),
              { once: true },
            );
          }),
      }).verify(fixture.subject);
      expect(timedOut, `${family}:timeout`).toMatchObject({
        kind: "fail",
        error_code: "unreachable",
      });

      const hostileUrl = new URL(fixture.subject.original_url);
      hostileUrl.username = "user";
      hostileUrl.password = "secret";
      const hostile = await new OriginalSourceVerifier({
        fetch: vi.fn(async () => response({})),
      }).verify({ ...fixture.subject, original_url: hostileUrl.toString() });
      expect(hostile, `${family}:hostile-url`).toMatchObject({
        kind: "fail",
        error_code: "invalid_assignment",
      });

      const privateResolution = await new OriginalSourceVerifier({
        lookup: async () => [{ address: "127.0.0.1", family: 4 }],
      }).verify(fixture.subject);
      expect(privateResolution, `${family}:private-resolution`).toMatchObject({
        kind: "fail",
        error_code: "source_untrusted",
      });
    }
  });

  it("reads one exact Greenhouse job anonymously and without redirects", async () => {
    const requests: Array<{ url: string; init: RequestInit }> = [];
    const fetcher: JobsFetch = vi.fn(async (url, init) => {
      requests.push({ url, init });
      return response({
        id: "job-1",
        absolute_url: "https://boards.greenhouse.io/acme/jobs/job-1",
        title: "Platform Engineer",
        location: { name: "Austin, TX" },
        workplace_type: "hybrid",
        content: "<p>Build reliable systems.</p>",
        employment_type: "full_time",
        updated_at: "2026-08-25T12:00:00Z",
      });
    });

    const result = await new OriginalSourceVerifier({ fetch: fetcher }).verify(
      subject("greenhouse"),
    );

    expect(result).toMatchObject({
      kind: "complete",
      observation: {
        result: "mismatch",
        error_code: null,
        requested_url:
          "https://boards-api.greenhouse.io/v1/boards/acme/jobs/job-1?content=true",
        canonical_observed_url: "https://boards.greenhouse.io/acme/jobs/job-1",
        retrieval_status: "observed",
        http_status: 200,
        parser_version: "greenhouse.original_source.v1",
        provider_record_id: "greenhouse:acme:job-1",
        mismatched_fields: ["compensation"],
        evidence_sha256:
          "b17c32858763afbb9da6ebc48bf37841a34956bf9f9bf6c15ef146e7dabff69f",
      },
    });
    expect(requests[0]?.url).toBe(
      "https://boards-api.greenhouse.io/v1/boards/acme/jobs/job-1?content=true",
    );
    expect(requests[0]?.init.method).toBe("GET");
    expect(requests[0]?.init.redirect).toBe("error");
    const headers = new Headers(requests[0]?.init.headers);
    expect(headers.has("authorization")).toBe(false);
    expect(headers.has("cookie")).toBe(false);
  });

  it("keeps a material Lever title change separate from identity mismatch", async () => {
    const result = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          id: "lever-1",
          hostedUrl: "https://jobs.lever.co/acme/lever-1",
          applyUrl: "https://jobs.lever.co/acme/lever-1/apply",
          text: "Senior Platform Engineer",
          categories: { location: "Austin, TX", commitment: "full_time" },
          workplaceType: "hybrid",
          descriptionPlain: "Build reliable systems.",
          salaryRange: {
            currency: "USD",
            min: 120000,
            max: 160000,
            interval: "year",
          },
          createdAt: Date.parse("2026-08-25T12:00:01Z"),
        }),
    }).verify(subject("lever"));

    expect(result).toMatchObject({
      kind: "complete",
      observation: {
        result: "mismatch",
        mismatched_fields: ["posted_at_ms", "title"],
      },
    });
  });

  it("uses the same Lever list/detail description and employment normalization", async () => {
    const payload = [
      {
        id: "lever-1",
        hostedUrl: "https://jobs.lever.co/acme/lever-1",
        applyUrl: "https://jobs.lever.co/acme/lever-1/apply",
        text: "Platform Engineer",
        categories: { location: "Austin, TX", commitment: "Full-time" },
        workplaceType: "hybrid",
        descriptionPlain: "Build reliable systems.",
        lists: [
          { text: "Responsibilities", content: "<li>Own production</li>" },
        ],
        salaryRange: {
          currency: "USD",
          min: 120000,
          max: 160000,
          interval: "year",
        },
        createdAt: Date.parse("2026-08-25T12:00:00Z"),
      },
    ];
    const fetcher: JobsFetch = async (url) =>
      response(
        url.includes("?mode=json") &&
          url.endsWith("?mode=json") &&
          !url.includes("/lever-1?")
          ? payload
          : payload[0],
      );
    const [discovered] = await new PublicAtsDiscoveryProvider({
      fetch: fetcher,
    }).snapshot({ kind: "lever", site: "acme", company: "Acme" });
    expect(discovered).toBeDefined();
    const verificationSubject = subject("lever", {
      original_url: discovered!.canonicalUrl,
      expected: {
        company: discovered!.company,
        title: discovered!.title,
        location: discovered!.location,
        workplace: discovered!.workplace,
        description: discovered!.description,
        compensation: discovered!.compensation ?? "",
        employment_type: discovered!.employmentType ?? "",
        posted_at_ms: Number(discovered!.postedAt),
        availability_status: "active",
      },
    });

    const result = await new OriginalSourceVerifier({ fetch: fetcher }).verify(
      verificationSubject,
    );

    expect(result).toMatchObject({
      kind: "complete",
      observation: {
        result: "open",
        description: "Build reliable systems. Responsibilities Own production",
        employment_type: "full_time",
        mismatched_fields: [],
      },
    });
  });

  it("treats absence from a complete valid Ashby board as closed", async () => {
    const result = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          jobs: [
            {
              id: "other-job",
              title: "Other role",
              jobUrl: "https://jobs.ashbyhq.com/acme/other-job",
              applyUrl: "https://jobs.ashbyhq.com/acme/other-job/application",
            },
          ],
        }),
    }).verify(subject("ashby"));

    expect(result).toMatchObject({
      kind: "complete",
      observation: { result: "closed", canonical_observed_url: null },
    });
  });

  it("does not call a malformed or duplicate Ashby listing closed", async () => {
    for (const jobs of [
      [{ id: "", title: "Missing identity" }],
      [
        { id: "other", title: "A" },
        { id: "other", title: "B" },
      ],
    ]) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response({ jobs }),
      }).verify(subject("ashby"));
      expect(result).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
      });
    }
  });

  it("reports a SmartRecruiters provider-record mismatch without granting authority", async () => {
    const result = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          id: "smart-swapped",
          name: "Platform Engineer",
          company: { identifier: "acme", name: "Acme" },
          applyUrl:
            "https://www.smartrecruiters.com/acme/smart-swapped-platform-engineer",
          ref: "https://api.smartrecruiters.com/v1/companies/acme/postings/smart-swapped",
          location: { city: "Austin", region: "TX" },
          workplaceType: "hybrid",
          releasedDate: "2026-08-25T12:00:00Z",
        }),
    }).verify(
      subject("smartrecruiters", {
        expected: {
          ...subject("smartrecruiters").expected,
          location: "Austin, TX",
          description: "",
          compensation: "",
          employment_type: "",
        },
      }),
    );

    expect(result).toMatchObject({
      kind: "complete",
      observation: {
        result: "mismatch",
        mismatched_fields: expect.arrayContaining(["provider_record_id"]),
      },
    });
  });

  it("treats case-only URL and provider-record drift as identity mismatches", async () => {
    const expected = subject("workday", {
      expected: {
        ...subject("workday").expected,
        compensation: "",
      },
    });
    const result = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          jobPostingInfo: {
            jobReqId: "jr-101",
            title: "Platform Engineer",
            location: "Austin, TX",
            workplaceType: "hybrid",
            jobDescription: "Build reliable systems.",
            timeType: "full_time",
            startDate: "2026-08-25T12:00:00Z",
            externalUrl:
              "https://acme.wd5.myworkdayjobs.com/EN-us/careers/job/Austin-TX/JR-101",
          },
        }),
    }).verify(expected);

    expect(result).toMatchObject({
      kind: "complete",
      observation: {
        result: "mismatch",
        mismatched_fields: expect.arrayContaining([
          "original_url",
          "provider_record_id",
        ]),
      },
    });
  });

  it("derives the exact Workday anonymous detail endpoint from the pinned original URL", async () => {
    const fetched: string[] = [];
    const fetcher: JobsFetch = async (url) => {
      fetched.push(url);
      return response({
        jobPostingInfo: {
          jobReqId: "JR-101",
          title: "Platform Engineer",
          location: "Austin, TX",
          workplaceType: "hybrid",
          jobDescription: "Build reliable systems.",
          timeType: "full_time",
          startDate: "2026-08-25T12:00:00Z",
          externalUrl:
            "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/Platform-Engineer_JR-101",
        },
      });
    };
    const exact = subject("workday", {
      original_url:
        "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/Platform-Engineer_JR-101",
      expected: { ...subject("workday").expected, compensation: "" },
    });

    const result = await new OriginalSourceVerifier({ fetch: fetcher }).verify(
      exact,
    );

    expect(result).toMatchObject({
      kind: "complete",
      observation: { result: "open" },
    });
    expect(fetched).toEqual([
      "https://acme.wd5.myworkdayjobs.com/wday/cxs/acme/careers/job/Austin-TX/Platform-Engineer_JR-101",
    ]);
  });

  it("reconciles a normal Workday discovery preview with its exact detail record", async () => {
    const fetcher: JobsFetch = async (url) =>
      url.endsWith("/jobs")
        ? response({
            total: 1,
            jobPostings: [
              {
                title: "Platform Engineer",
                locationsText: "Austin, TX",
                externalPath: "/job/Austin-TX/Platform-Engineer_JR-101",
                bulletFields: ["JR-101"],
                workplaceType: "hybrid",
                descriptionPreview: "Build reliable systems.",
                postedOn: "2026-08-25T12:00:00Z",
                timeType: "full_time",
              },
            ],
          })
        : response({
            jobPostingInfo: {
              jobReqId: "JR-101",
              title: "Platform Engineer",
              location: "Austin, TX",
              workplaceType: "hybrid",
              jobDescription:
                "Build reliable systems. Own the production platform.",
              timeType: "full_time",
              startDate: "2026-08-25T12:00:00Z",
              externalUrl:
                "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/Platform-Engineer_JR-101",
            },
          });
    const [discovered] = await new PublicAtsDiscoveryProvider({
      fetch: fetcher,
      maxPages: 1,
    }).snapshot({
      kind: "workday",
      tenant: "acme",
      instance: "wd5",
      site: "careers",
      locale: "en-US",
      company: "Acme",
    });
    expect(discovered).toBeDefined();
    const verificationSubject = subject("workday", {
      original_url: discovered!.canonicalUrl,
      expected: {
        company: discovered!.company,
        title: discovered!.title,
        location: discovered!.location,
        workplace: discovered!.workplace,
        description: discovered!.description,
        compensation: discovered!.compensation ?? "",
        employment_type: discovered!.employmentType ?? "",
        posted_at_ms: Date.parse(discovered!.postedAt!),
        availability_status: "active",
      },
    });

    const result = await new OriginalSourceVerifier({ fetch: fetcher }).verify(
      verificationSubject,
    );

    expect(result).toMatchObject({
      kind: "complete",
      observation: {
        result: "open",
        description: "Build reliable systems. Own the production platform.",
        mismatched_fields: [],
      },
    });
  });

  it("accepts only exact 404/410 semantics as direct closed evidence", async () => {
    for (const status of [404, 410]) {
      const result = await new OriginalSourceVerifier({
        fetch: async () => response({ error: "missing" }, status),
      }).verify(subject("greenhouse"));
      expect(result).toMatchObject({
        kind: "complete",
        observation: {
          result: "closed",
          error_code: null,
          retrieval_status: status === 404 ? "not_found" : "gone",
          http_status: status,
        },
      });
    }
    const forbidden = await new OriginalSourceVerifier({
      fetch: async () => response({ error: "forbidden" }, 403),
    }).verify(subject("greenhouse"));
    expect(forbidden).toMatchObject({
      kind: "fail",
      error_code: "auth_required",
      observation: {
        result: "indeterminate",
        error_code: "auth_required",
        retrieval_status: "observed",
        http_status: 403,
      },
    });
  });

  it("requires provider-returned posting and application destinations", async () => {
    const greenhouse = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          id: "job-1",
          title: "Platform Engineer",
          location: { name: "Austin, TX" },
        }),
    }).verify(subject("greenhouse"));
    expect(greenhouse).toMatchObject({
      kind: "fail",
      error_code: "parse_ambiguous",
    });

    const lever = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          id: "lever-1",
          hostedUrl: "https://jobs.lever.co/acme/lever-1",
          applyUrl: "https://jobs.lever.co/another/lever-1/apply",
          text: "Platform Engineer",
        }),
    }).verify(subject("lever"));
    expect(lever).toMatchObject({
      kind: "fail",
      error_code: "parse_ambiguous",
    });

    const ashby = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          jobs: [
            {
              id: "ashby-1",
              title: "Platform Engineer",
              jobUrl: "https://jobs.ashbyhq.com/acme/ashby-1",
              applyUrl: "https://jobs.ashbyhq.com/acme/another/application",
            },
          ],
        }),
    }).verify(subject("ashby"));
    expect(ashby).toMatchObject({
      kind: "fail",
      error_code: "parse_ambiguous",
    });

    const smartrecruiters = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          id: "smart-1",
          name: "Platform Engineer",
          company: { identifier: "acme", name: "Acme" },
          applyUrl:
            "https://www.smartrecruiters.com/acme/smart-1-platform-engineer",
          ref: "https://api.smartrecruiters.com/v1/companies/other/postings/smart-1",
        }),
    }).verify(subject("smartrecruiters"));
    expect(smartrecruiters).toMatchObject({
      kind: "fail",
      error_code: "parse_ambiguous",
    });

    const workday = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          jobPostingInfo: {
            jobReqId: "JR-101",
            title: "Platform Engineer",
          },
        }),
    }).verify(subject("workday"));
    expect(workday).toMatchObject({
      kind: "fail",
      error_code: "parse_ambiguous",
    });
  });

  it("bounds safe GET retries and never converts rate limiting into closure", async () => {
    const fetcher = vi.fn(async () => response({ error: "limited" }, 429));
    const sleep = vi.fn(async () => undefined);
    const result = await new OriginalSourceVerifier({
      fetch: fetcher,
      maxAttempts: 3,
      sleep,
    }).verify(subject("lever"));

    expect(result).toMatchObject({ kind: "fail", error_code: "rate_limited" });
    expect(fetcher).toHaveBeenCalledTimes(3);
    expect(sleep).toHaveBeenCalledTimes(2);
  });

  it("quarantines a target/URL mismatch before making a provider request", async () => {
    const fetcher = vi.fn(async () => response({}));
    const result = await new OriginalSourceVerifier({ fetch: fetcher }).verify(
      subject("greenhouse", {
        original_url: "https://boards.greenhouse.io/other/jobs/job-1",
      }),
    );

    expect(result).toMatchObject({
      kind: "complete",
      observation: { result: "mismatch", mismatched_fields: ["original_url"] },
    });
    expect(fetcher).not.toHaveBeenCalled();

    const workdayFetcher = vi.fn(async () => response({}));
    const workday = await new OriginalSourceVerifier({
      fetch: workdayFetcher,
    }).verify(
      subject("workday", {
        original_url:
          "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/Austin-TX/JR-999",
      }),
    );
    expect(workday).toMatchObject({
      kind: "complete",
      observation: { result: "mismatch", mismatched_fields: ["original_url"] },
    });
    expect(workdayFetcher).not.toHaveBeenCalled();

    const smartFetcher = vi.fn(async () => response({}));
    const smart = await new OriginalSourceVerifier({
      fetch: smartFetcher,
    }).verify(
      subject("smartrecruiters", {
        original_url: "https://jobs.smartrecruiters.com/acme/smart-10-title",
      }),
    );
    expect(smart).toMatchObject({
      kind: "complete",
      observation: { result: "mismatch", mismatched_fields: ["original_url"] },
    });
    expect(smartFetcher).not.toHaveBeenCalled();
  });

  it("rejects oversized and non-JSON responses without authority", async () => {
    const oversizedDigests: string[] = [];
    for (const marker of ["x", "y"]) {
      const oversized = await new OriginalSourceVerifier({
        responseBytes: 1_024,
        maxAttempts: 1,
        fetch: async () => response({ value: marker.repeat(2_000) }),
      }).verify(subject("greenhouse"));
      expect(oversized).toMatchObject({
        kind: "fail",
        error_code: "parse_ambiguous",
      });
      if (oversized.kind !== "fail") {
        throw new Error("Oversized provider body was accepted");
      }
      oversizedDigests.push(oversized.observation.content_digest);
    }
    expect(new Set(oversizedDigests).size).toBe(2);

    const html = await new OriginalSourceVerifier({
      fetch: async () =>
        response("challenge", 200, { "content-type": "text/html" }),
    }).verify(subject("greenhouse"));
    expect(html).toMatchObject({ kind: "fail", error_code: "parse_ambiguous" });

    const captcha = await new OriginalSourceVerifier({
      fetch: async () =>
        response("<html>Verify you are human</html>", 200, {
          "content-type": "text/html",
        }),
    }).verify(subject("greenhouse"));
    expect(captcha).toMatchObject({
      kind: "fail",
      error_code: "captcha_required",
      observation: { result: "indeterminate", retrieval_status: "observed" },
    });
  });

  it("bounds provider parser work and terminal observation fields", async () => {
    let nested: unknown = "leaf";
    for (let depth = 0; depth < 40; depth += 1) nested = { nested };
    const deep = await new OriginalSourceVerifier({
      fetch: async () => response(nested),
    }).verify(subject("greenhouse"));
    expect(deep).toMatchObject({ kind: "fail", error_code: "parse_ambiguous" });

    const wide = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          jobs: Array.from({ length: 10_001 }, (_, index) => ({
            id: `job-${index}`,
          })),
        }),
    }).verify(subject("ashby"));
    expect(wide).toMatchObject({ kind: "fail", error_code: "parse_ambiguous" });

    const largeObservation = await new OriginalSourceVerifier({
      fetch: async () =>
        response({
          id: "job-1",
          absolute_url: "https://boards.greenhouse.io/acme/jobs/job-1",
          title: "Platform Engineer",
          location: { name: "Austin, TX" },
          workplace_type: "hybrid",
          content: "x".repeat(129 * 1024),
          employment_type: "full_time",
          updated_at: "2026-08-25T12:00:00Z",
        }),
    }).verify(subject("greenhouse"));
    expect(largeObservation).toMatchObject({
      kind: "fail",
      error_code: "parse_ambiguous",
      observation: { description: null },
    });
  });

  it("rejects redirects and unsafe URL authority before provider bytes can be trusted", async () => {
    const redirected = await new OriginalSourceVerifier({
      maxAttempts: 1,
      fetch: async () =>
        response({ redirected: true }, 302, { location: "https://evil.test" }),
    }).verify(subject("greenhouse"));
    expect(redirected).toMatchObject({
      kind: "fail",
      error_code: "source_untrusted",
    });

    for (const original_url of [
      "https://user:pass@boards.greenhouse.io/acme/jobs/job-1",
      "https://boards.greenhouse.io:8443/acme/jobs/job-1",
    ]) {
      const fetcher = vi.fn(async () => response({}));
      const result = await new OriginalSourceVerifier({
        fetch: fetcher,
      }).verify(subject("greenhouse", { original_url }));
      expect(result).toMatchObject({
        kind: "fail",
        error_code: "invalid_assignment",
      });
      expect(fetcher).not.toHaveBeenCalled();
    }

    const privateFetcher = vi.fn(async () => response({}));
    const privateHost = await new OriginalSourceVerifier({
      fetch: privateFetcher,
    }).verify(
      subject("greenhouse", {
        original_url: "https://127.0.0.1/acme/jobs/job-1",
        provider_target: {
          ...subject("greenhouse").provider_target,
          host: "127.0.0.1",
        },
      }),
    );
    expect(privateHost).toMatchObject({
      kind: "complete",
      observation: { result: "mismatch", mismatched_fields: ["original_url"] },
    });
    expect(privateFetcher).not.toHaveBeenCalled();

    for (const addresses of [
      [{ address: "127.0.0.1", family: 4 as const }],
      [
        { address: "8.8.8.8", family: 4 as const },
        { address: "169.254.169.254", family: 4 as const },
      ],
      [{ address: "::1", family: 6 as const }],
      [{ address: "2001:db8::1", family: 6 as const }],
      [{ address: "2002:7f00:1::", family: 6 as const }],
      [
        {
          address: "2001:0000:4136:e378:8000:63bf:3fff:fdd2",
          family: 6 as const,
        },
      ],
    ]) {
      const dnsRejected = await new OriginalSourceVerifier({
        lookup: async () => addresses,
      }).verify(subject("greenhouse"));
      expect(dnsRejected).toMatchObject({
        kind: "fail",
        error_code: "source_untrusted",
        observation: {
          result: "quarantined",
          retrieval_status: "preflight_rejected",
          http_status: null,
        },
      });
    }
  });

  it("requires the exact canonical subject shape and five-family target vocabulary", () => {
    expect(
      parseOriginalSourceVerificationSubject(subject("greenhouse")),
    ).toEqual(subject("greenhouse"));
    expect(() =>
      parseOriginalSourceVerificationSubject({
        ...subject("greenhouse"),
        caller_verified: true,
      }),
    ).toThrow(/shape/);
    expect(() =>
      parseOriginalSourceVerificationSubject({
        ...subject("greenhouse"),
        provider_family: "semantic",
      }),
    ).toThrow(/family/);
  });
});
