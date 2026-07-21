import { createHash } from "node:crypto";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  downloadJobhiveArtifact,
  JobhiveArtifactError,
  streamVerifiedJobhiveCsv,
  type VerifiedJobhiveArtifact,
} from "../src/jobhive-artifact.js";
import type { JobhiveArtifact } from "../src/jobhive-manifest.js";

const directories: string[] = [];
const headers = [
  "url", "title", "company", "ats_type", "ats_id", "location", "is_remote",
  "salary_min", "salary_max", "salary_currency", "salary_period", "salary_summary",
  "employment_type", "department", "team", "description", "posted_at",
  "requisition_id", "apply_url", "commitment", "raw", "country_iso",
];

afterEach(async () => {
  await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

async function temporaryDirectory(): Promise<string> {
  const directory = await mkdtemp(path.join(tmpdir(), "bluey-jobhive-"));
  directories.push(directory);
  return directory;
}

function fixtureCsv(): string {
  const first = [
    "https://jobs.lever.co/acme/1", "Software Engineer", "Acme", "lever", "1", "Austin, TX", "true",
    "120000", "160000", "USD", "year", "$120k-$160k", "full_time", "Engineering", "Platform",
    "Build systems", "2026-07-20T00:00:00Z", "REQ-1", "https://jobs.lever.co/acme/1/apply", "Full-time",
    "{\"source\":\"lever\"}", "US",
  ];
  const second = [
    "https://boards.greenhouse.io/example/jobs/2", "Data Engineer", "Example", "greenhouse", "2", "Remote", "1",
    "", "", "", "", "", "contract", "Data", "", "Line one\nLine two", "", "REQ-2",
    "https://boards.greenhouse.io/example/jobs/2", "C2C", "{}", "US",
  ];
  return [headers, first, second].map(csvRow).join("\n") + "\n";
}

function csvRow(values: string[]): string {
  return values.map((value) => `"${value.replaceAll('"', '""')}"`).join(",");
}

function artifactFor(content: string, overrides: Partial<JobhiveArtifact> = {}): JobhiveArtifact {
  return {
    format: "csv",
    url: "https://storage.stapply.ai/jobhive/v1/lever/jobs.csv",
    sha256: createHash("sha256").update(content).digest("hex"),
    sizeBytes: Buffer.byteLength(content),
    rows: 2,
    ...overrides,
  };
}

describe("Jobhive artifact ingestion", () => {
  it("stages a pinned artifact only after exact checksum and size verification", async () => {
    const content = fixtureCsv();
    const stagingDirectory = await temporaryDirectory();
    const fetcher = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      expect(init?.redirect).toBe("error");
      expect(new Headers(init?.headers).get("accept-encoding")).toBe("identity");
      return new Response(content, {
        status: 200,
        headers: { "content-length": String(Buffer.byteLength(content)) },
      });
    });

    const verified = await downloadJobhiveArtifact({
      artifact: artifactFor(content),
      sourceFamily: "lever",
      stagingDirectory,
      fetch: fetcher as typeof fetch,
    });

    expect(await readFile(verified.path, "utf8")).toBe(content);
    expect(await readdir(stagingDirectory)).toEqual([path.basename(verified.path)]);
  });

  it("removes partial files after checksum failure and rejects oversized artifacts", async () => {
    const content = fixtureCsv();
    const stagingDirectory = await temporaryDirectory();
    const badChecksum = artifactFor(content, { sha256: "f".repeat(64) });
    const fetcher = vi.fn(async () => new Response(content, { status: 200 }));

    await expect(downloadJobhiveArtifact({
      artifact: badChecksum,
      sourceFamily: "lever",
      stagingDirectory,
      fetch: fetcher as typeof fetch,
    })).rejects.toMatchObject({ code: "checksum_mismatch" });
    expect(await readdir(stagingDirectory)).toEqual([]);

    await expect(downloadJobhiveArtifact({
      artifact: artifactFor(content),
      sourceFamily: "lever",
      stagingDirectory,
      maxArtifactBytes: 10,
    })).rejects.toMatchObject({ code: "too_large" });
  });

  it("parses quoted records and streams them with backpressure in bounded batches", async () => {
    const content = fixtureCsv();
    const stagingDirectory = await temporaryDirectory();
    const filePath = path.join(stagingDirectory, "lever.csv");
    await writeFile(filePath, content);
    const verified: VerifiedJobhiveArtifact = {
      artifact: artifactFor(content),
      sourceFamily: "lever",
      path: filePath,
      bytes: Buffer.byteLength(content),
      sha256: createHash("sha256").update(content).digest("hex"),
    };
    const batches: unknown[][] = [];

    const result = await streamVerifiedJobhiveCsv({
      verified,
      batchRows: 1,
      onBatch: async (rows) => batches.push(rows),
    });

    expect(result).toEqual({ rows: 2, batches: 2 });
    expect(batches[0]?.[0]).toMatchObject({ title: "Software Engineer", isRemote: true, salaryMin: 120000 });
    expect(batches[1]?.[0]).toMatchObject({ description: "Line one\nLine two", commitment: "C2C" });
  });

  it("rejects missing columns and row-count drift", async () => {
    const stagingDirectory = await temporaryDirectory();
    const missingHeader = headers.filter((header) => header !== "apply_url");
    const content = `${csvRow(missingHeader)}\n`;
    const filePath = path.join(stagingDirectory, "missing.csv");
    await writeFile(filePath, content);
    const verified: VerifiedJobhiveArtifact = {
      artifact: artifactFor(content, { rows: 0 }),
      sourceFamily: "lever",
      path: filePath,
      bytes: Buffer.byteLength(content),
      sha256: createHash("sha256").update(content).digest("hex"),
    };

    await expect(streamVerifiedJobhiveCsv({ verified, onBatch: () => undefined }))
      .rejects.toBeInstanceOf(JobhiveArtifactError);

    const valid = fixtureCsv();
    await writeFile(filePath, valid);
    verified.artifact = artifactFor(valid, { rows: 3 });
    verified.bytes = Buffer.byteLength(valid);
    verified.sha256 = createHash("sha256").update(valid).digest("hex");
    await expect(streamVerifiedJobhiveCsv({ verified, onBatch: () => undefined }))
      .rejects.toMatchObject({ code: "row_count_mismatch" });
  });
});
