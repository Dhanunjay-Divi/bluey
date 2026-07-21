import { describe, expect, it, vi } from "vitest";

import {
  fetchJobhiveManifest,
  JobhiveManifestError,
  parseJobhiveManifest,
  planJobhiveIngestion,
} from "../src/jobhive-manifest.js";

const REQUIRED_COLUMNS = [
  "url", "title", "company", "ats_type", "ats_id", "location", "is_remote",
  "salary_min", "salary_max", "salary_currency", "salary_period", "salary_summary",
  "employment_type", "department", "team", "description", "posted_at",
  "requisition_id", "apply_url", "commitment", "raw", "country_iso",
];

function artifact(source: string, rows: number, csvBytes: number, parquetBytes: number) {
  return {
    csv: `https://storage.stapply.ai/jobhive/v1/${source}/jobs.csv`,
    parquet: `https://storage.stapply.ai/jobhive/v1/${source}/jobs.parquet`,
    sha256: "a".repeat(64),
    size_bytes: csvBytes,
    parquet_sha256: "b".repeat(64),
    parquet_size_bytes: parquetBytes,
    rows,
  };
}

function manifestFixture() {
  return {
    version: "2.0",
    generated_at: "2026-07-20T14:30:05.036044+00:00",
    updated_at: "2026-07-20T14:30:05Z",
    generator: "jobhive/0.1.0",
    stats: {
      ats_count: 4,
      schema_columns: REQUIRED_COLUMNS,
      schema_version: "2.0",
      total_companies: 12,
      total_jobs: 14,
      total_jobs_raw: 15,
    },
    all: {
      csv: "https://storage.stapply.ai/jobhive/v1/all.csv",
      parquet: "https://storage.stapply.ai/jobhive/v1/all.parquet",
      sha256: "c".repeat(64),
      size_bytes: 2_000,
      parquet_sha256: "d".repeat(64),
      parquet_size_bytes: 900,
      rows: 14,
    },
    by_ats: {
      greenhouse: artifact("greenhouse", 5, 500, 100),
      lever: artifact("lever", 4, 400, 90),
      remoteok: artifact("remoteok", 6, 600, 120),
      wellfound: artifact("wellfound", 0, 226, 226),
    },
  };
}

describe("Jobhive manifest", () => {
  it("validates pinned artifacts, schema totals, and conservative submission capabilities", () => {
    const parsed = parseJobhiveManifest(JSON.stringify(manifestFixture()));

    expect(parsed).toMatchObject({
      version: "2.0",
      schemaVersion: "2.0",
      totalJobs: 14,
      totalJobsRaw: 15,
      sourceCount: 4,
    });
    expect(parsed.sources.find((source) => source.sourceFamily === "greenhouse")).toMatchObject({
      rows: 5,
      submissionCapability: "beta_review",
      requiresOriginalRevalidation: true,
    });
    expect(parsed.sources.find((source) => source.sourceFamily === "remoteok")).toMatchObject({
      submissionCapability: "unknown_review",
      requiresOriginalRevalidation: true,
    });
  });

  it("creates global bounded batches instead of a per-account all-jobs download", () => {
    const parsed = parseJobhiveManifest(JSON.stringify(manifestFixture()));
    const plan = planJobhiveIngestion(parsed, {
      format: "parquet",
      maxBatchBytes: 200,
      maxBatchRows: 10,
      maxArtifactBytes: 150,
    });

    expect(plan.requiresSharedGlobalIndex).toBe(true);
    expect(plan.requiresOriginalRevalidation).toBe(true);
    expect(plan.plannedRows).toBe(15);
    expect(plan.batches).toHaveLength(2);
    expect(plan.batches.flatMap((batch) => batch.artifacts.map((item) => item.sourceFamily)))
      .toEqual(["lever", "greenhouse", "remoteok"]);
    expect(plan.deferred).toEqual([{ sourceFamily: "wellfound", rows: 0, reason: "empty" }]);
  });

  it("supports explicit source batches and reports oversized shards rather than silently truncating", () => {
    const fixture = manifestFixture();
    fixture.by_ats.remoteok.parquet_size_bytes = 800;
    const parsed = parseJobhiveManifest(JSON.stringify(fixture));
    const plan = planJobhiveIngestion(parsed, {
      sourceFamilies: ["remoteok"],
      maxBatchBytes: 500,
      maxBatchRows: 100,
      maxArtifactBytes: 500,
    });

    expect(plan.plannedRows).toBe(0);
    expect(plan.deferred).toEqual([
      { sourceFamily: "remoteok", rows: 6, reason: "artifact_over_limit" },
    ]);
    expect(() => planJobhiveIngestion(parsed, { sourceFamilies: ["not-present"] }))
      .toThrow(JobhiveManifestError);
  });

  it("plans current multi-gigabyte source families with bounded production defaults", () => {
    const fixture = manifestFixture();
    fixture.by_ats.remoteok.size_bytes = 3_600_000_000;
    const parsed = parseJobhiveManifest(JSON.stringify(fixture));
    const plan = planJobhiveIngestion(parsed);

    expect(plan.batches.flatMap((batch) => batch.artifacts.map((item) => item.sourceFamily)))
      .toContain("remoteok");
    expect(plan.deferred).not.toContainEqual(expect.objectContaining({
      sourceFamily: "remoteok",
      reason: "artifact_over_limit",
    }));
  });

  it("rejects redirected or attacker-controlled artifact URLs and inconsistent row totals", () => {
    const attacker = manifestFixture();
    attacker.by_ats.lever.csv = "https://attacker.example/jobhive/v1/lever/jobs.csv";
    expect(() => parseJobhiveManifest(JSON.stringify(attacker))).toThrow(/not pinned/);

    const inconsistent = manifestFixture();
    inconsistent.stats.total_jobs_raw = 999;
    expect(() => parseJobhiveManifest(JSON.stringify(inconsistent))).toThrow(/raw total/);
  });

  it("uses conditional requests and enforces the manifest response cap", async () => {
    const notModified = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      expect(new Headers(init?.headers).get("if-none-match")).toBe("manifest-v2");
      return new Response(null, { status: 304 });
    });
    await expect(fetchJobhiveManifest({
      fetch: notModified as typeof fetch,
      ifNoneMatch: "manifest-v2",
    })).rejects.toMatchObject({ code: "not_modified" });

    const tooLarge = vi.fn(async () => new Response("{}", {
      status: 200,
      headers: { "content-length": String(64 * 1024 + 1) },
    }));
    await expect(fetchJobhiveManifest({ fetch: tooLarge as typeof fetch }))
      .rejects.toMatchObject({ code: "too_large" });
  });
});
