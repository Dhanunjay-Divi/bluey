import { describe, expect, it } from "vitest";

import type {
  JobhiveArtifact,
  JobhiveManifest,
  JobhiveSourceSnapshot,
} from "../src/jobhive-manifest.js";
import {
  classifyJobhiveSourceFamily,
  jobhiveRolloutAllowlist,
  KNOWN_JOBHIVE_SOURCE_FAMILIES,
  planJobhiveSourceRollout,
} from "../src/jobhive-rollout.js";

describe("Jobhive rollout planning", () => {
  it("classifies every known source family and plans every non-empty source", () => {
    const empty = new Set(["meta", "wellfound"]);
    const manifest = fixture([
      ...KNOWN_JOBHIVE_SOURCE_FAMILIES.map((sourceFamily) => source(
        sourceFamily,
        empty.has(sourceFamily) ? 0 : 100,
      )),
    ]);

    const plan = planJobhiveSourceRollout(manifest, {
      maxRowsPerWave: 1_000,
      maxBytesPerWave: 100_000,
    });

    expect(KNOWN_JOBHIVE_SOURCE_FAMILIES).toHaveLength(49);
    expect(plan.sources).toHaveLength(47);
    expect(plan.deferred.map((item) => item.sourceFamily)).toEqual(["meta", "wellfound"]);
    expect(plan.quarantined).toEqual([]);
    expect(plan.plannedRows).toBe(4_700);
    expect(plan.requiresOriginalRevalidation).toBe(true);
    expect(plan.sources.every((item) => item.requiresOriginalRevalidation)).toBe(true);
    expect(plan.waves.flatMap((wave) => wave.sourceFamilies)).toHaveLength(47);
  });

  it("keeps source class and submission authority separate", () => {
    expect(classifyJobhiveSourceFamily("greenhouse")).toBe("typed_ats");
    expect(classifyJobhiveSourceFamily("oracle")).toBe("ats_feed");
    expect(classifyJobhiveSourceFamily("amazon")).toBe("direct_employer");
    expect(classifyJobhiveSourceFamily("eures")).toBe("public_service");
    expect(classifyJobhiveSourceFamily("remoteok")).toBe("job_board");

    const plan = planJobhiveSourceRollout(fixture([
      source("greenhouse", 20, "blocked"),
      source("remoteok", 10, "beta_review"),
    ]));

    expect(plan.sources.find((item) => item.sourceFamily === "greenhouse")?.submissionCapability)
      .toBe("blocked");
    expect(plan.sources.find((item) => item.sourceFamily === "remoteok")?.submissionCapability)
      .toBe("beta_review");
  });

  it("quarantines an unknown family instead of silently ingesting it", () => {
    const plan = planJobhiveSourceRollout(fixture([
      source("greenhouse", 20),
      source("future_portal", 20),
    ]));

    expect(plan.sources.map((item) => item.sourceFamily)).toEqual(["greenhouse"]);
    expect(plan.quarantined).toEqual([{
      sourceFamily: "future_portal",
      reason: "unclassified_source_family",
    }]);
  });

  it("isolates oversized sources and emits an operator-ready cumulative allowlist", () => {
    const plan = planJobhiveSourceRollout(fixture([
      source("lever", 100),
      source("greenhouse", 100),
      source("workday", 900),
    ]), {
      maxRowsPerWave: 500,
      maxBytesPerWave: 500_000,
    });

    expect(plan.waves).toHaveLength(2);
    expect(plan.waves[0]).toMatchObject({
      sourceFamilies: ["greenhouse", "lever"],
      plannedRows: 200,
      requiresCapacityReview: false,
    });
    expect(plan.waves[1]).toMatchObject({
      sourceFamilies: ["workday"],
      plannedRows: 900,
      requiresCapacityReview: true,
    });
    expect(jobhiveRolloutAllowlist(plan, 1)).toBe("greenhouse,lever");
    expect(jobhiveRolloutAllowlist(plan, 2)).toBe("greenhouse,lever,workday");
  });
});

function fixture(sources: JobhiveSourceSnapshot[]): JobhiveManifest {
  const totalJobs = sources.reduce((total, item) => total + item.rows, 0);
  return {
    version: "2.0",
    schemaVersion: "2.0",
    generatedAt: "2026-07-21T12:00:00Z",
    updatedAt: "2026-07-21T12:00:00Z",
    generator: "test",
    totalJobs,
    totalJobsRaw: totalJobs,
    totalCompanies: sources.length,
    sourceCount: sources.length,
    schemaColumns: [],
    all: {
      csv: artifact("all", "csv", totalJobs),
      parquet: artifact("all", "parquet", totalJobs),
    },
    sources,
  };
}

function source(
  sourceFamily: string,
  rows: number,
  submissionCapability: JobhiveSourceSnapshot["submissionCapability"] = "unknown_review",
): JobhiveSourceSnapshot {
  return {
    sourceFamily,
    rows,
    csv: artifact(sourceFamily, "csv", rows),
    parquet: artifact(sourceFamily, "parquet", rows),
    submissionCapability,
    requiresOriginalRevalidation: true,
  };
}

function artifact(sourceFamily: string, format: "csv" | "parquet", rows: number): JobhiveArtifact {
  return {
    format,
    url: `https://storage.stapply.ai/jobhive/v1/${sourceFamily}.${format}`,
    sha256: "a".repeat(64),
    sizeBytes: rows * 100,
    rows,
  };
}
