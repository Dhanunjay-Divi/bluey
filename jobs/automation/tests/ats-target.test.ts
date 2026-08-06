import { describe, expect, it } from "vitest";
import vectorsJson from "./fixtures/ats-target-vectors.json";
import {
  detectAts,
  findJobSourceByUrl,
  parseProviderApplicationTarget,
  submissionPolicy,
  type ProviderApplicationTargetPurpose,
} from "../src/index.js";

interface AtsTargetVector {
  name: string;
  url: string;
  purpose: ProviderApplicationTargetPurpose;
  expectedTarget: {
    provider: "greenhouse" | "lever";
    host: string;
    tenant: string;
    job: string;
    variant: string;
    providerJobKey: string;
  } | null;
  expectedDetection: string;
  expectedPolicy: string;
  expectedSourceId: string | null;
}

const VECTORS = vectorsJson as AtsTargetVector[];

describe("canonical ATS application target grammar", () => {
  it("refuses normalized URL objects at the raw authority boundary", () => {
    const normalized = new URL("https://jobs.lever.co:443/acme/posting-123");
    expect(parseProviderApplicationTarget(normalized as unknown as string)).toBeUndefined();
  });

  it("applies the shared URL limit to UTF-8 bytes rather than UTF-16 units", () => {
    const oversized = `https://jobs.lever.co/acme/posting-123#${"😀".repeat(600)}`;
    expect(oversized.length).toBeLessThan(2_048);
    expect(parseProviderApplicationTarget(oversized)).toBeUndefined();
    expect(parseProviderApplicationTarget(`https://jobs.lever.co/${"a".repeat(1_000_000)}`))
      .toBeUndefined();
  });

  for (const vector of VECTORS) {
    it(vector.name, () => {
      const target = parseProviderApplicationTarget(vector.url, vector.purpose);
      if (vector.expectedTarget) {
        expect(target).toMatchObject({
          ...vector.expectedTarget,
          purpose: vector.purpose,
        });
      } else {
        expect(target).toBeUndefined();
      }

      expect(detectAts(vector.url)).toBe(vector.expectedDetection);
      expect(submissionPolicy(vector.url).policy).toBe(vector.expectedPolicy);
      expect(findJobSourceByUrl(vector.url)?.id ?? null).toBe(
        vector.expectedSourceId,
      );
    });
  }
});
