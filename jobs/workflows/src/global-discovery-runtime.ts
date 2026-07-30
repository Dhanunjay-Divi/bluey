import { createHash } from "node:crypto";
import { rm } from "node:fs/promises";
import {
  DEFAULT_JOBHIVE_MAX_ARTIFACT_BYTES,
  downloadJobhiveArtifact,
  fetchJobhiveManifest,
  JobhiveArtifactError,
  JobhiveManifestError,
  streamVerifiedJobhiveCsv,
  type JobhiveCandidateRow,
  type JobhiveManifest,
  type JobhiveSourceSnapshot,
} from "@bluey/jobs-automation/jobhive-runtime";
import {
  GlobalDiscoveryApiError,
  type GlobalDiscoveredJobInput,
  type GlobalDiscoverySourceLease,
  type GlobalDiscoveryWorkerApi,
} from "./global-discovery-api.js";

export const DEFAULT_GLOBAL_DISCOVERY_POLL_INTERVAL_MS = 5_000;
export const DEFAULT_GLOBAL_DISCOVERY_RUN_INTERVAL_MS = 6 * 60 * 60_000;
export const DEFAULT_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS = 15 * 60_000;

const MIN_POLL_INTERVAL_MS = 250;
const MAX_POLL_INTERVAL_MS = 60_000;
const MIN_RUN_INTERVAL_MS = 5 * 60_000;
const MAX_RUN_INTERVAL_MS = 24 * 60 * 60_000;
const MIN_MANIFEST_REFRESH_MS = 60_000;
const MAX_MANIFEST_REFRESH_MS = 24 * 60 * 60_000;
export const DEFAULT_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS = 30 * 60_000;
export const DEFAULT_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES = DEFAULT_JOBHIVE_MAX_ARTIFACT_BYTES;
const DEFAULT_UPLOAD_BATCH_ROWS = 500;
const MAX_UPLOAD_BATCH_ROWS = 1_000;
const DEFAULT_UPLOAD_BATCH_BYTES = 8 * 1024 * 1024;
const MAX_UPLOAD_BATCH_BYTES = 24 * 1024 * 1024;

export type GlobalDiscoveryPollOutcome = "completed" | "failed" | "idle";

export type GlobalDiscoveryWorkerLogEvent =
  | { event: "global_discovery_manifest_synced"; source_count: number; row_count: number }
  | { event: "global_discovery_source_completed"; source_fingerprint: string; rows: number; batches: number }
  | { event: "global_discovery_source_failed"; source_fingerprint: string; error_code: string }
  | { event: "global_discovery_poll_failed"; error_code: string };

export interface GlobalDiscoveryWorkerLogger {
  log(event: GlobalDiscoveryWorkerLogEvent): void;
}

export type GlobalDiscoveryPollSleep = (milliseconds: number, signal: AbortSignal) => Promise<void>;

export interface GlobalDiscoveryWorkerRuntimeOptions {
  api: GlobalDiscoveryWorkerApi;
  stagingDirectory: string;
  sourceFamilies?: readonly string[];
  pollIntervalMs?: number;
  runIntervalMs?: number;
  manifestRefreshMs?: number;
  artifactTimeoutMs?: number;
  maxArtifactBytes?: number;
  uploadBatchRows?: number;
  uploadBatchBytes?: number;
  manifestFetch?: typeof fetch;
  artifactFetch?: typeof fetch;
  now?: () => number;
  sleep?: GlobalDiscoveryPollSleep;
  logger?: GlobalDiscoveryWorkerLogger;
}

interface ManifestSnapshot {
  manifest: JobhiveManifest;
  syncedAtMs: number;
}

interface GlobalSourceConfig {
  sourceFamily: string;
  artifactUrl: string;
  artifactSha256: string;
  expectedRows: number;
  snapshotAtMs: number;
  requiresOriginalRevalidation: true;
}

const consoleLogger: GlobalDiscoveryWorkerLogger = {
  log: (event) => console.log(JSON.stringify(event)),
};

export class GlobalDiscoveryWorkerRuntime {
  readonly pollIntervalMs: number;

  private readonly api: GlobalDiscoveryWorkerApi;
  private readonly stagingDirectory: string;
  private readonly sourceFamilies?: ReadonlySet<string>;
  private readonly runIntervalMs: number;
  private readonly manifestRefreshMs: number;
  private readonly artifactTimeoutMs: number;
  private readonly maxArtifactBytes: number;
  private readonly uploadBatchRows: number;
  private readonly uploadBatchBytes: number;
  private readonly manifestFetch?: typeof fetch;
  private readonly artifactFetch?: typeof fetch;
  private readonly now: () => number;
  private readonly sleep: GlobalDiscoveryPollSleep;
  private readonly logger: GlobalDiscoveryWorkerLogger;
  private readonly stopController = new AbortController();
  private manifestSnapshot?: ManifestSnapshot;
  private stopping = false;

  constructor(options: GlobalDiscoveryWorkerRuntimeOptions) {
    this.api = options.api;
    this.stagingDirectory = options.stagingDirectory;
    if (this.stagingDirectory.trim() === "") throw new Error("Global discovery staging directory is required");
    const sourceFamilies = normalizeSourceFamilies(options.sourceFamilies);
    this.sourceFamilies = sourceFamilies ? new Set(sourceFamilies) : undefined;
    this.pollIntervalMs = boundedInteger(
      options.pollIntervalMs ?? DEFAULT_GLOBAL_DISCOVERY_POLL_INTERVAL_MS,
      MIN_POLL_INTERVAL_MS,
      MAX_POLL_INTERVAL_MS,
      "poll interval",
    );
    this.runIntervalMs = boundedInteger(
      options.runIntervalMs ?? DEFAULT_GLOBAL_DISCOVERY_RUN_INTERVAL_MS,
      MIN_RUN_INTERVAL_MS,
      MAX_RUN_INTERVAL_MS,
      "run interval",
    );
    this.manifestRefreshMs = boundedInteger(
      options.manifestRefreshMs ?? DEFAULT_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS,
      MIN_MANIFEST_REFRESH_MS,
      MAX_MANIFEST_REFRESH_MS,
      "manifest refresh interval",
    );
    this.artifactTimeoutMs = boundedInteger(
      options.artifactTimeoutMs ?? DEFAULT_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS,
      1_000,
      2 * 60 * 60_000,
      "artifact timeout",
    );
    this.maxArtifactBytes = boundedInteger(
      options.maxArtifactBytes ?? DEFAULT_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES,
      1,
      4 * 1024 * 1024 * 1024,
      "artifact byte limit",
    );
    this.uploadBatchRows = boundedInteger(
      options.uploadBatchRows ?? DEFAULT_UPLOAD_BATCH_ROWS,
      1,
      MAX_UPLOAD_BATCH_ROWS,
      "upload batch row limit",
    );
    this.uploadBatchBytes = boundedInteger(
      options.uploadBatchBytes ?? DEFAULT_UPLOAD_BATCH_BYTES,
      64 * 1024,
      MAX_UPLOAD_BATCH_BYTES,
      "upload batch byte limit",
    );
    this.manifestFetch = options.manifestFetch;
    this.artifactFetch = options.artifactFetch;
    this.now = options.now ?? Date.now;
    this.sleep = options.sleep ?? interruptibleSleep;
    this.logger = options.logger ?? consoleLogger;
  }

  async run(signal?: AbortSignal): Promise<void> {
    const stop = (): void => this.stop();
    if (signal?.aborted) this.stop();
    else signal?.addEventListener("abort", stop, { once: true });
    try {
      while (!this.stopping) {
        try {
          await this.pollOnce();
        } catch (error) {
          this.logger.log({
            event: "global_discovery_poll_failed",
            error_code: safeRuntimeErrorCode(error),
          });
        }
        if (!this.stopping) await this.sleep(this.pollIntervalMs, this.stopController.signal);
      }
    } finally {
      signal?.removeEventListener("abort", stop);
    }
  }

  stop(): void {
    if (this.stopping) return;
    this.stopping = true;
    this.stopController.abort();
  }

  async pollOnce(): Promise<GlobalDiscoveryPollOutcome> {
    const manifest = await this.refreshManifestIfNeeded();
    const lease = await this.api.lease();
    if (!lease) return "idle";

    const sourceFingerprint = fingerprint(lease.source.id);
    const sourceConfig = parseSourceConfig(lease.source.config);
    const source = manifest.sources.find((candidate) => candidate.sourceFamily === sourceConfig.sourceFamily);
    if (!source || !matchesLeaseSnapshot(source, sourceConfig, manifest)) {
      return this.reportFailure(lease, sourceConfig.artifactSha256, "source_snapshot_mismatch", sourceFingerprint);
    }

    let verifiedPath: string | undefined;
    try {
      const verified = await downloadJobhiveArtifact({
        artifact: source.csv,
        sourceFamily: source.sourceFamily,
        stagingDirectory: this.stagingDirectory,
        fetch: this.artifactFetch,
        timeoutMs: this.artifactTimeoutMs,
        maxArtifactBytes: this.maxArtifactBytes,
      });
      verifiedPath = verified.path;
      let batchIndex = 0;
      let uploadedRows = 0;
      await streamVerifiedJobhiveCsv({
        verified,
        batchRows: this.uploadBatchRows,
        onBatch: async (rows) => {
          for (const jobs of uploadBatches(
            rows.map((row) => globalJob(row, source.sourceFamily)),
            this.uploadBatchRows,
            this.uploadBatchBytes,
          )) {
            const result = await this.api.ingestBatch(lease.source.id, {
              lease_token: lease.lease_token,
              replay_key: lease.replay_key,
              scheduled_for_ms: lease.scheduled_for_ms,
              batch_index: batchIndex,
              artifact_sha256: verified.sha256,
              jobs,
            });
            if (result.batch_index !== batchIndex || result.row_count !== jobs.length) {
              throw new Error("global_batch_ack_mismatch");
            }
            batchIndex += 1;
            uploadedRows += jobs.length;
          }
        },
      });
      if (uploadedRows !== source.rows || batchIndex < 1) {
        throw new JobhiveArtifactError("row_count_mismatch", "Jobhive upload row count did not match its manifest");
      }
      const result = await this.api.complete(lease.source.id, {
        lease_token: lease.lease_token,
        replay_key: lease.replay_key,
        scheduled_for_ms: lease.scheduled_for_ms,
        artifact_sha256: verified.sha256,
        expected_rows: uploadedRows,
        expected_batches: batchIndex,
        complete_snapshot: true,
      });
      if (result.received_rows !== uploadedRows || result.received_batches !== batchIndex) {
        throw new Error("global_completion_ack_mismatch");
      }
      this.logger.log({
        event: "global_discovery_source_completed",
        source_fingerprint: sourceFingerprint,
        rows: uploadedRows,
        batches: batchIndex,
      });
      return "completed";
    } catch (error) {
      if (error instanceof GlobalDiscoveryApiError) throw error;
      const errorCode = safeRuntimeErrorCode(error);
      return this.reportFailure(lease, sourceConfig.artifactSha256, errorCode, sourceFingerprint);
    } finally {
      if (verifiedPath) await rm(verifiedPath, { force: true }).catch(() => undefined);
    }
  }

  private async refreshManifestIfNeeded(): Promise<JobhiveManifest> {
    const now = this.now();
    if (this.manifestSnapshot && now - this.manifestSnapshot.syncedAtMs < this.manifestRefreshMs) {
      return this.manifestSnapshot.manifest;
    }
    const { manifest } = await fetchJobhiveManifest({ fetch: this.manifestFetch });
    const snapshotAtMs = Date.parse(manifest.updatedAt);
    const selectedSnapshots = manifest.sources.filter((source) => (
      source.rows > 0 && (!this.sourceFamilies || this.sourceFamilies.has(source.sourceFamily))
    ));
    const sources = selectedSnapshots
      .map((source) => ({
        provider: "jobhive" as const,
        source_key: `jobhive:${source.sourceFamily}`,
        source_family: source.sourceFamily,
        artifact_url: source.csv.url,
        artifact_sha256: source.csv.sha256,
        expected_rows: source.rows,
        snapshot_at_ms: snapshotAtMs,
        run_interval_ms: this.runIntervalMs,
      }));
    if (sources.length === 0) {
      throw new JobhiveManifestError(
        "invalid_manifest",
        this.sourceFamilies
          ? "Jobhive manifest has no nonempty sources matching the configured source-family allowlist"
          : "Jobhive manifest has no nonempty sources",
      );
    }
    await this.api.syncSources(sources);
    const selectedManifest = this.sourceFamilies
      ? { ...manifest, sources: selectedSnapshots, sourceCount: selectedSnapshots.length }
      : manifest;
    this.manifestSnapshot = { manifest: selectedManifest, syncedAtMs: now };
    this.logger.log({
      event: "global_discovery_manifest_synced",
      source_count: sources.length,
      row_count: sources.reduce((sum, source) => sum + source.expected_rows, 0),
    });
    return selectedManifest;
  }

  private async reportFailure(
    lease: GlobalDiscoverySourceLease,
    artifactSha256: string,
    errorCode: string,
    sourceFingerprint: string,
  ): Promise<"failed"> {
    await this.api.fail(lease.source.id, {
      lease_token: lease.lease_token,
      replay_key: lease.replay_key,
      scheduled_for_ms: lease.scheduled_for_ms,
      artifact_sha256: artifactSha256,
      error_code: safeErrorCode(errorCode),
    });
    this.logger.log({
      event: "global_discovery_source_failed",
      source_fingerprint: sourceFingerprint,
      error_code: safeErrorCode(errorCode),
    });
    return "failed";
  }
}

export function parseGlobalDiscoverySourceFamilies(value: string | undefined): string[] | undefined {
  if (value === undefined || value.trim() === "") return undefined;
  return normalizeSourceFamilies(value.split(","));
}

function normalizeSourceFamilies(values: readonly string[] | undefined): string[] | undefined {
  if (values === undefined) return undefined;
  const normalized = [...new Set(values.map((value) => value.trim().toLowerCase()).filter(Boolean))];
  if (normalized.length === 0) throw new Error("Global discovery source-family allowlist cannot be empty");
  for (const sourceFamily of normalized) {
    if (!/^[a-z0-9][a-z0-9_-]{0,63}$/.test(sourceFamily)) {
      throw new Error("Global discovery source-family allowlist contains an invalid source family");
    }
  }
  return normalized;
}

function parseSourceConfig(value: unknown): GlobalSourceConfig {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid_source_config");
  const config = value as Record<string, unknown>;
  const sourceFamily = requiredString(config.sourceFamily, "source family", 120);
  const artifactUrl = requiredString(config.artifactUrl, "artifact URL", 4096);
  const artifactSha256 = requiredSha256(config.artifactSha256);
  const expectedRows = requiredInteger(config.expectedRows, "expected rows", 1, 5_000_000);
  const snapshotAtMs = requiredInteger(config.snapshotAtMs, "snapshot time", 1, Number.MAX_SAFE_INTEGER);
  if (config.requiresOriginalRevalidation !== true) throw new Error("invalid_source_config");
  return {
    sourceFamily,
    artifactUrl,
    artifactSha256,
    expectedRows,
    snapshotAtMs,
    requiresOriginalRevalidation: true,
  };
}

function matchesLeaseSnapshot(
  source: JobhiveSourceSnapshot,
  config: GlobalSourceConfig,
  manifest: JobhiveManifest,
): boolean {
  return source.csv.url === config.artifactUrl
    && source.csv.sha256 === config.artifactSha256
    && source.rows === config.expectedRows
    && Date.parse(manifest.updatedAt) === config.snapshotAtMs
    && config.requiresOriginalRevalidation;
}

function globalJob(row: JobhiveCandidateRow, sourceFamily: string): GlobalDiscoveredJobInput {
  const canonicalUrl = row.applyUrl || row.url;
  if (!canonicalUrl || !row.company || !row.title) throw new JobhiveArtifactError("invalid_artifact", "Jobhive row is missing job identity");
  const employmentEvidence = `${row.employmentType} ${row.commitment}`.trim();
  return {
    external_id: row.atsId || row.requisitionId || createHash("sha256").update(canonicalUrl).digest("hex"),
    canonical_url: canonicalUrl,
    title: row.title,
    company: row.company,
    source_catalog_id: `jobhive:${sourceFamily}`,
    requires_original_revalidation: true,
    location: row.location || row.countryIso,
    workplace: workplace(row),
    description: row.description,
    compensation: compensation(row),
    employment_type: employmentType(employmentEvidence),
    engagement_type: engagementType(employmentEvidence),
    posted_at_ms: postedAt(row.postedAt),
  };
}

function workplace(row: JobhiveCandidateRow): string {
  const evidence = `${row.location} ${row.commitment}`.toLowerCase();
  if (row.isRemote === true || /\bremote\b/.test(evidence)) return "remote";
  if (/\bhybrid\b/.test(evidence)) return "hybrid";
  if (/\bon[ -]?site\b|\bin[ -]?office\b/.test(evidence)) return "onsite";
  return "";
}

function employmentType(value: string): string {
  const normalized = value.toLowerCase();
  if (/\bintern(ship)?\b/.test(normalized)) return "internship";
  if (/\bapprentice(ship)?\b/.test(normalized)) return "apprenticeship";
  if (/\bpart[ -]?time\b/.test(normalized)) return "part_time";
  if (/\bfull[ -]?time\b|\bpermanent\b|\bdirect[ -]?hire\b/.test(normalized)) return "full_time";
  if (/\btemporary\b|\btemp\b/.test(normalized)) return "temporary";
  if (/\bseasonal\b/.test(normalized)) return "seasonal";
  if (/\bper[ -]?diem\b/.test(normalized)) return "per_diem";
  if (/\bcontract(or)?\b|\bc2c\b|\bw-?2\b|\b1099\b/.test(normalized)) return "contract";
  return "";
}

function engagementType(value: string): string {
  const normalized = value.toLowerCase();
  if (/\bc2c\b|\bcorp(?:oration)?[ -]?to[ -]?corp(?:oration)?\b/.test(normalized)) return "c2c";
  if (/\b1099\b|\bindependent contractor\b/.test(normalized)) return "1099";
  if (/\bw-?2\b/.test(normalized)) return "w2";
  if (/\bdirect[ -]?hire\b/.test(normalized)) return "direct_hire";
  return "";
}

function compensation(row: JobhiveCandidateRow): string {
  if (row.salarySummary) return row.salarySummary;
  if (row.salaryMin === null && row.salaryMax === null) return "";
  const currency = row.salaryCurrency || "USD";
  const range = row.salaryMin !== null && row.salaryMax !== null
    ? `${row.salaryMin}-${row.salaryMax}`
    : String(row.salaryMin ?? row.salaryMax);
  return `${currency} ${range}${row.salaryPeriod ? ` ${row.salaryPeriod}` : ""}`;
}

function postedAt(value: string): number | null {
  if (!value) return null;
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) && timestamp > 0 ? timestamp : null;
}

function uploadBatches(
  jobs: GlobalDiscoveredJobInput[],
  maximumRows: number,
  maximumBytes: number,
): GlobalDiscoveredJobInput[][] {
  const result: GlobalDiscoveredJobInput[][] = [];
  let batch: GlobalDiscoveredJobInput[] = [];
  let bytes = 2;
  for (const job of jobs) {
    const jobBytes = Buffer.byteLength(JSON.stringify(job), "utf8") + (batch.length > 0 ? 1 : 0);
    if (jobBytes > maximumBytes) throw new JobhiveArtifactError("too_large", "Jobhive row exceeds the upload limit");
    if (batch.length >= maximumRows || bytes + jobBytes > maximumBytes) {
      result.push(batch);
      batch = [];
      bytes = 2;
    }
    batch.push(job);
    bytes += jobBytes;
  }
  if (batch.length > 0) result.push(batch);
  return result;
}

function safeRuntimeErrorCode(error: unknown): string {
  if (error instanceof JobhiveArtifactError) {
    return error.code === "checksum_mismatch" ? "artifact_checksum_mismatch"
      : error.code === "too_large" ? "artifact_too_large"
        : error.code === "unavailable" ? "artifact_unavailable"
          : "artifact_invalid";
  }
  if (error instanceof JobhiveManifestError) return `manifest_${error.code}`;
  if (error instanceof GlobalDiscoveryApiError) return error.code;
  if (error instanceof Error && /^[a-z0-9_]{3,128}$/.test(error.message)) return error.message;
  return "worker_error";
}

function safeErrorCode(value: string): string {
  const normalized = value.trim().toLowerCase().replace(/[^a-z0-9_]+/g, "_").slice(0, 128);
  return normalized.length >= 3 ? normalized : "worker_error";
}

function fingerprint(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 16);
}

function requiredString(value: unknown, label: string, maximum: number): string {
  if (typeof value !== "string" || value.trim() === "" || value.length > maximum) {
    throw new Error(`invalid_${label.replaceAll(" ", "_")}`);
  }
  return value;
}

function requiredSha256(value: unknown): string {
  const sha256 = requiredString(value, "artifact hash", 64).toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(sha256)) throw new Error("invalid_artifact_hash");
  return sha256;
}

function requiredInteger(value: unknown, label: string, minimum: number, maximum: number): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`invalid_${label.replaceAll(" ", "_")}`);
  }
  return value;
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Global discovery ${label} is outside the allowed range`);
  }
  return value;
}

async function interruptibleSleep(milliseconds: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return;
  await new Promise<void>((resolve) => {
    const timer = setTimeout(resolve, milliseconds);
    signal.addEventListener("abort", () => {
      clearTimeout(timer);
      resolve();
    }, { once: true });
  });
}
