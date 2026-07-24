import { describe, expect, it } from "vitest";

import {
  assessJobSourceTrust,
  findPotentialCrossListings,
  fingerprintDescription,
  fingerprintSimilarity,
  type NormalizedJob,
} from "../src/index.js";

const BASE_DESCRIPTION = `
  Build reliable data services for a healthcare platform. Design APIs, improve
  observability, operate distributed systems, review code, mentor engineers,
  and partner with product teams. The role requires TypeScript, PostgreSQL,
  event-driven architecture, testing, incident response, and clear written
  communication across engineering and clinical operations.
`;

describe("job source intelligence", () => {
  it("marks known ATS links as high trust without treating trust as verification", () => {
    expect(assessJobSourceTrust({
      applicationUrl: "https://jobs.lever.co/acme/role-1",
      company: "Acme Health",
    })).toEqual({
      score: 100,
      level: "high",
      flags: [],
      hostname: "jobs.lever.co",
      requiresOriginalRevalidation: true,
    });
  });

  it("flags redirectors, insecure URLs, and unexplained company-domain mismatches", () => {
    expect(assessJobSourceTrust({
      applicationUrl: "http://bit.ly/acme-role",
      company: "Acme Health",
    })).toMatchObject({
      level: "low",
      flags: [
        "insecure_application_url",
        "redirector_application_url",
        "company_domain_mismatch",
      ],
      requiresOriginalRevalidation: true,
    });
    expect(assessJobSourceTrust({
      applicationUrl: "https://careers.acmehealth.com/jobs/1",
      company: "Acme Health",
    })).toMatchObject({ level: "high", flags: [] });
  });

  it("creates deterministic fingerprints and ignores descriptions without enough evidence", () => {
    const fingerprint = fingerprintDescription(BASE_DESCRIPTION);
    expect(fingerprint).toMatch(/^[0-9a-f]{16}$/);
    expect(fingerprintDescription(BASE_DESCRIPTION)).toBe(fingerprint);
    expect(fingerprintDescription("Short job text")).toBe("");
    expect(fingerprintSimilarity(fingerprint, fingerprint)).toBe(1);
  });

  it("reports near-identical cross-listings but does not merge same-employer jobs", () => {
    const jobs: NormalizedJob[] = [
      job("direct", "Acme Health", "https://careers.acmehealth.com/jobs/1", BASE_DESCRIPTION),
      job(
        "agency",
        "Example Staffing",
        "https://jobs.example-staffing.com/roles/99",
        BASE_DESCRIPTION,
      ),
      job("same-company", "Acme Health", "https://careers.acmehealth.com/jobs/2", BASE_DESCRIPTION),
      job(
        "other",
        "Other Company",
        "https://other.example/jobs/3",
        "Design visual campaigns, write brand guidance, coordinate photography, and manage creative vendors.",
      ),
    ];

    const signals = findPotentialCrossListings(jobs);
    expect(signals).toHaveLength(1);
    expect(signals[0]).toMatchObject({
      firstExternalId: "direct",
      firstCompany: "Acme Health",
      secondExternalId: "agency",
      secondCompany: "Example Staffing",
    });
    expect(signals[0]!.similarity).toBeGreaterThanOrEqual(0.92);
  });
});

function job(
  externalId: string,
  company: string,
  canonicalUrl: string,
  description: string,
): NormalizedJob {
  return {
    externalId,
    canonicalUrl,
    company,
    title: "Senior Platform Engineer",
    location: "Remote - US",
    workplace: "remote",
    description,
    source: "lever",
  };
}
