import { createHash, createHmac } from "node:crypto";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  GlobalDiscoveryApiClient,
  type GlobalDiscoveryApiFetch,
  type GlobalDiscoverySourceInput,
  type GlobalDiscoverySourceLease,
  type GlobalDiscoverySourceRecord,
  type GlobalDiscoveryWorkerApi,
  type GlobalIngestionBatchInput,
  type GlobalIngestionBatchResult,
  type GlobalIngestionCompleteInput,
  type GlobalIngestionFailureInput,
  type GlobalIngestionRunResult,
} from "../src/global-discovery-api.js";
import {
  DEFAULT_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS,
  DEFAULT_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES,
  GlobalDiscoveryWorkerRuntime,
  type GlobalDiscoveryWorkerLogEvent,
  type GlobalDiscoveryWorkerLogger,
} from "../src/global-discovery-runtime.js";

const WORKER_KEY = "global-discovery-signing-key-0123456789abcdef";
const SNAPSHOT_AT = "2026-07-20T14:30:05Z";
const SNAPSHOT_AT_MS = Date.parse(SNAPSHOT_AT);
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

class FakeApi implements GlobalDiscoveryWorkerApi {
  readonly synced: GlobalDiscoverySourceInput[][] = [];
  readonly batches: GlobalIngestionBatchInput[] = [];
  readonly completed: GlobalIngestionCompleteInput[] = [];
  readonly failed: GlobalIngestionFailureInput[] = [];

  constructor(private readonly pendingLease: GlobalDiscoverySourceLease | null) {}

  async syncSources(sources: GlobalDiscoverySourceInput[]): Promise<GlobalDiscoverySourceRecord[]> {
    this.synced.push(sources);
    return [];
  }

  async lease(): Promise<GlobalDiscoverySourceLease | null> {
    return this.pendingLease;
  }

  async ingestBatch(_sourceId: string, input: GlobalIngestionBatchInput): Promise<GlobalIngestionBatchResult> {
    this.batches.push(input);
    return {
      run_id: "run-global-test",
      batch_index: input.batch_index,
      row_count: input.jobs.length,
      received_rows: this.batches.reduce((sum, batch) => sum + batch.jobs.length, 0),
      received_batches: this.batches.length,
      replayed: false,
    };
  }

  async complete(_sourceId: string, input: GlobalIngestionCompleteInput): Promise<GlobalIngestionRunResult> {
    this.completed.push(input);
    return runResult(input.replay_key, input.expected_rows, input.expected_batches, "completed");
  }

  async fail(_sourceId: string, input: GlobalIngestionFailureInput): Promise<GlobalIngestionRunResult> {
    this.failed.push(input);
    return runResult(input.replay_key, 0, 0, "failed");
  }
}

class RecordingLogger implements GlobalDiscoveryWorkerLogger {
  readonly events: GlobalDiscoveryWorkerLogEvent[] = [];

  log(event: GlobalDiscoveryWorkerLogEvent): void {
    this.events.push(event);
  }
}

describe("global discovery worker runtime", () => {
  it("keeps current multi-gigabyte ATS snapshots inside explicit bounded defaults", () => {
    expect(DEFAULT_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS).toBe(30 * 60_000);
    expect(DEFAULT_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES).toBe(4 * 1024 * 1024 * 1024);
  });

  it("verifies, streams, normalizes, and commits a complete shared-feed snapshot", async () => {
    const csv = candidateCsv();
    const sha256 = createHash("sha256").update(csv).digest("hex");
    const api = new FakeApi(lease(sha256));
    const stagingDirectory = await temporaryDirectory();
    const manifestFetch = vi.fn(async () => jsonResponse(manifestFixture(csv, sha256)));
    const artifactFetch = vi.fn(async () => new Response(csv, {
      status: 200,
      headers: { "content-length": String(Buffer.byteLength(csv)) },
    }));
    const logger = new RecordingLogger();
    const runtime = new GlobalDiscoveryWorkerRuntime({
      api,
      stagingDirectory,
      manifestFetch: manifestFetch as typeof fetch,
      artifactFetch: artifactFetch as typeof fetch,
      uploadBatchRows: 1,
      logger,
      now: () => SNAPSHOT_AT_MS,
    });

    await expect(runtime.pollOnce()).resolves.toBe("completed");

    expect(api.synced).toHaveLength(1);
    expect(api.synced[0]?.[0]).toMatchObject({
      provider: "jobhive",
      source_key: "jobhive:lever",
      source_family: "lever",
      expected_rows: 1,
      snapshot_at_ms: SNAPSHOT_AT_MS,
    });
    expect(api.batches).toHaveLength(1);
    expect(api.batches[0]?.jobs[0]).toEqual({
      external_id: "lever-123",
      canonical_url: "https://jobs.lever.co/acme/lever-123/apply",
      title: "Platform Engineer",
      company: "Acme",
      source_catalog_id: "jobhive:lever",
      requires_original_revalidation: true,
      location: "Austin, TX (Hybrid)",
      workplace: "hybrid",
      description: "Build reliable systems.",
      compensation: "USD 120000-160000 year",
      employment_type: "contract",
      engagement_type: "w2",
      posted_at_ms: SNAPSHOT_AT_MS,
    });
    expect(api.completed[0]).toMatchObject({
      expected_rows: 1,
      expected_batches: 1,
      complete_snapshot: true,
      artifact_sha256: sha256,
    });
    expect(api.failed).toEqual([]);
    expect(await readdir(stagingDirectory)).toEqual([]);
    expect(logger.events).toContainEqual(expect.objectContaining({
      event: "global_discovery_source_completed",
      rows: 1,
      batches: 1,
    }));
  });

  it("reports checksum drift without publishing a partial snapshot", async () => {
    const csv = candidateCsv();
    const expectedSha = "f".repeat(64);
    const api = new FakeApi(lease(expectedSha));
    const stagingDirectory = await temporaryDirectory();
    const runtime = new GlobalDiscoveryWorkerRuntime({
      api,
      stagingDirectory,
      manifestFetch: (async () => jsonResponse(manifestFixture(csv, expectedSha))) as typeof fetch,
      artifactFetch: (async () => new Response(csv, { status: 200 })) as typeof fetch,
      now: () => SNAPSHOT_AT_MS,
    });

    await expect(runtime.pollOnce()).resolves.toBe("failed");

    expect(api.batches).toEqual([]);
    expect(api.completed).toEqual([]);
    expect(api.failed).toHaveLength(1);
    expect(api.failed[0]?.error_code).toBe("artifact_checksum_mismatch");
    expect(await readdir(stagingDirectory)).toEqual([]);
  });

  it("rejects a stale lease snapshot before downloading its artifact", async () => {
    const csv = candidateCsv();
    const sha256 = createHash("sha256").update(csv).digest("hex");
    const stale = lease(sha256);
    stale.source.config = { ...stale.source.config as object, expectedRows: 2 };
    const api = new FakeApi(stale);
    const artifactFetch = vi.fn(async () => new Response(csv, { status: 200 }));
    const runtime = new GlobalDiscoveryWorkerRuntime({
      api,
      stagingDirectory: await temporaryDirectory(),
      manifestFetch: (async () => jsonResponse(manifestFixture(csv, sha256))) as typeof fetch,
      artifactFetch: artifactFetch as typeof fetch,
      now: () => SNAPSHOT_AT_MS,
    });

    await expect(runtime.pollOnce()).resolves.toBe("failed");

    expect(artifactFetch).not.toHaveBeenCalled();
    expect(api.failed[0]?.error_code).toBe("source_snapshot_mismatch");
  });

  it("signs and retries an idempotent batch with the exact same body", async () => {
    const requests: Array<{ input: string; init?: RequestInit }> = [];
    const fetcher: GlobalDiscoveryApiFetch = vi.fn(async (input, init) => {
      requests.push({ input: String(input), init });
      return new Response(JSON.stringify({
        run_id: "run-global-test",
        batch_index: 0,
        row_count: 1,
        received_rows: 1,
        received_batches: 1,
        replayed: requests.length > 1,
      }), { status: requests.length === 1 ? 503 : 200 });
    });
    const client = new GlobalDiscoveryApiClient({
      origin: "https://jobs.internal",
      signingKey: WORKER_KEY,
      workerId: "global-discovery-test",
      fetch: fetcher,
      reportRetryMs: 0,
      sleep: async () => undefined,
    });
    const input: GlobalIngestionBatchInput = {
      lease_token: "lease-token-global",
      replay_key: "replay-key-global",
      scheduled_for_ms: SNAPSHOT_AT_MS,
      batch_index: 0,
      artifact_sha256: "a".repeat(64),
      jobs: [{
        external_id: "job-1",
        canonical_url: "https://jobs.lever.co/acme/job-1",
        title: "Engineer",
        company: "Acme",
        source_catalog_id: "jobhive:lever",
        requires_original_revalidation: true,
        location: "Remote",
        workplace: "remote",
        description: "Build things",
        compensation: "",
        employment_type: "full_time",
        engagement_type: "direct_hire",
        posted_at_ms: SNAPSHOT_AT_MS,
      }],
    };

    await expect(client.ingestBatch("source-global-lever", input)).resolves.toMatchObject({ replayed: true });

    expect(requests).toHaveLength(2);
    expect(requests[0]?.init?.body).toBe(requests[1]?.init?.body);
    for (const request of requests) expectSignedRequest(request);
    expect(new Headers(requests[0]?.init?.headers).get("x-bluey-jobs-worker-nonce"))
      .not.toBe(new Headers(requests[1]?.init?.headers).get("x-bluey-jobs-worker-nonce"));
  });
});

function source(sha256: string): GlobalDiscoverySourceRecord {
  return {
    id: "source-global-lever",
    provider: "jobhive",
    source_key: "jobhive:lever",
    config: {
      sourceFamily: "lever",
      artifactUrl: "https://storage.stapply.ai/jobhive/v1/lever/jobs.csv",
      artifactSha256: sha256,
      expectedRows: 1,
      snapshotAtMs: SNAPSHOT_AT_MS,
      requiresOriginalRevalidation: true,
    },
    status: "active",
    health: "waiting",
    consecutive_failures: 0,
    run_interval_ms: 6 * 60 * 60_000,
    next_run_at_ms: SNAPSHOT_AT_MS,
    last_success_at_ms: null,
    last_failure_at_ms: null,
    last_error_code: null,
    lease_expires_at_ms: SNAPSHOT_AT_MS + 10 * 60_000,
    created_at_ms: SNAPSHOT_AT_MS,
    updated_at_ms: SNAPSHOT_AT_MS,
  };
}

function lease(sha256: string): GlobalDiscoverySourceLease {
  return {
    source: source(sha256),
    lease_token: "lease-token-global",
    replay_key: "replay-key-global",
    scheduled_for_ms: SNAPSHOT_AT_MS,
  };
}

function runResult(
  replayKey: string,
  receivedRows: number,
  receivedBatches: number,
  status: string,
): GlobalIngestionRunResult {
  return {
    run_id: "run-global-test",
    replay_key: replayKey,
    status,
    received_rows: receivedRows,
    received_batches: receivedBatches,
    expired_count: 0,
    replayed: false,
  };
}

async function temporaryDirectory(): Promise<string> {
  const directory = await mkdtemp(path.join(tmpdir(), "bluey-global-discovery-"));
  directories.push(directory);
  return directory;
}

function candidateCsv(): string {
  const row = [
    "https://jobs.lever.co/acme/lever-123",
    "Platform Engineer",
    "Acme",
    "lever",
    "lever-123",
    "Austin, TX (Hybrid)",
    "false",
    "120000",
    "160000",
    "USD",
    "year",
    "",
    "Contract",
    "Engineering",
    "Platform",
    "Build reliable systems.",
    SNAPSHOT_AT,
    "REQ-123",
    "https://jobs.lever.co/acme/lever-123/apply",
    "W2 contract",
    "{}",
    "US",
  ];
  return `${csvRow(headers)}\n${csvRow(row)}\n`;
}

function csvRow(values: string[]): string {
  return values.map((value) => `"${value.replaceAll('"', '""')}"`).join(",");
}

function manifestFixture(csv: string, csvSha256: string): unknown {
  return {
    version: "2.0",
    generated_at: SNAPSHOT_AT,
    updated_at: SNAPSHOT_AT,
    generator: "jobhive/test",
    stats: {
      ats_count: 1,
      schema_columns: headers,
      schema_version: "2.0",
      total_companies: 1,
      total_jobs: 1,
      total_jobs_raw: 1,
    },
    all: {
      csv: "https://storage.stapply.ai/jobhive/v1/all.csv",
      parquet: "https://storage.stapply.ai/jobhive/v1/all.parquet",
      sha256: "a".repeat(64),
      size_bytes: 1,
      parquet_sha256: "b".repeat(64),
      parquet_size_bytes: 1,
      rows: 1,
    },
    by_ats: {
      lever: {
        csv: "https://storage.stapply.ai/jobhive/v1/lever/jobs.csv",
        parquet: "https://storage.stapply.ai/jobhive/v1/lever/jobs.parquet",
        sha256: csvSha256,
        size_bytes: Buffer.byteLength(csv),
        parquet_sha256: "c".repeat(64),
        parquet_size_bytes: 1,
        rows: 1,
      },
    },
  };
}

function jsonResponse(payload: unknown): Response {
  const body = JSON.stringify(payload);
  return new Response(body, {
    status: 200,
    headers: { "content-length": String(Buffer.byteLength(body)) },
  });
}

function expectSignedRequest(request: { input: string; init?: RequestInit }): void {
  const url = new URL(request.input);
  const headers = new Headers(request.init?.headers);
  const body = String(request.init?.body ?? "");
  const timestamp = headers.get("x-bluey-jobs-worker-timestamp");
  const nonce = headers.get("x-bluey-jobs-worker-nonce");
  const contentSha256 = createHash("sha256").update(body).digest("hex");
  const canonical = [
    "bluey-jobs-worker-v1",
    timestamp,
    nonce,
    "global-discovery-test",
    "bluey-jobs-api",
    "discovery",
    "POST",
    url.pathname,
    contentSha256,
  ].join("\n");
  expect(headers.get("x-bluey-jobs-worker-scope")).toBe("discovery");
  expect(headers.get("x-bluey-jobs-worker-content-sha256")).toBe(contentSha256);
  expect(headers.get("x-bluey-jobs-worker-signature")).toBe(
    createHmac("sha256", WORKER_KEY).update(canonical).digest("hex"),
  );
}
