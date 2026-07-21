import { submissionPolicy, type SubmissionCapability } from "./policy.js";

export const JOBHIVE_MANIFEST_URL = "https://storage.stapply.ai/jobhive/v1/manifest.json";

const JOBHIVE_HOST = "storage.stapply.ai";
const JOBHIVE_BASE_PATH = "/jobhive/v1/";
const MAX_MANIFEST_BYTES = 64 * 1024;
const DEFAULT_TIMEOUT_MS = 10_000;
const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const SOURCE_FAMILY_PATTERN = /^[a-z0-9][a-z0-9_-]{0,63}$/;

export const JOBHIVE_REQUIRED_COLUMNS = [
  "url",
  "title",
  "company",
  "ats_type",
  "ats_id",
  "location",
  "is_remote",
  "salary_min",
  "salary_max",
  "salary_currency",
  "salary_period",
  "salary_summary",
  "employment_type",
  "department",
  "team",
  "description",
  "posted_at",
  "requisition_id",
  "apply_url",
  "commitment",
  "raw",
  "country_iso",
] as const;

export const JOBHIVE_TYPED_ATS_FAMILIES = [
  "greenhouse",
  "lever",
  "ashby",
  "smartrecruiters",
  "workday",
] as const;

export type JobhiveArtifactFormat = "csv" | "parquet";

export interface JobhiveArtifact {
  format: JobhiveArtifactFormat;
  url: string;
  sha256: string;
  sizeBytes: number;
  rows: number;
}

export interface JobhiveSourceSnapshot {
  sourceFamily: string;
  rows: number;
  csv: JobhiveArtifact;
  parquet: JobhiveArtifact;
  submissionCapability: SubmissionCapability;
  requiresOriginalRevalidation: true;
}

export interface JobhiveManifest {
  version: "2.0";
  schemaVersion: "2.0";
  generatedAt: string;
  updatedAt: string;
  generator: string;
  totalJobs: number;
  totalJobsRaw: number;
  totalCompanies: number;
  sourceCount: number;
  schemaColumns: string[];
  all: {
    csv: JobhiveArtifact;
    parquet: JobhiveArtifact;
  };
  sources: JobhiveSourceSnapshot[];
}

export interface JobhiveManifestFetchOptions {
  fetch?: typeof fetch;
  timeoutMs?: number;
  ifNoneMatch?: string;
}

export interface JobhiveManifestFetchResult {
  manifest: JobhiveManifest;
  etag?: string;
}

export interface JobhiveIngestionPlanOptions {
  sourceFamilies?: readonly string[];
  format?: JobhiveArtifactFormat;
  maxBatchBytes?: number;
  maxBatchRows?: number;
  maxArtifactBytes?: number;
}

export interface JobhiveIngestionBatch {
  batch: number;
  rows: number;
  sizeBytes: number;
  artifacts: Array<{
    sourceFamily: string;
    artifact: JobhiveArtifact;
    submissionCapability: SubmissionCapability;
    requiresOriginalRevalidation: true;
  }>;
}

export interface JobhiveDeferredSource {
  sourceFamily: string;
  rows: number;
  reason: "empty" | "artifact_over_limit";
}

export interface JobhiveIngestionPlan {
  format: JobhiveArtifactFormat;
  batches: JobhiveIngestionBatch[];
  deferred: JobhiveDeferredSource[];
  plannedRows: number;
  plannedBytes: number;
  requiresSharedGlobalIndex: true;
  requiresOriginalRevalidation: true;
}

export const DEFAULT_JOBHIVE_MAX_ARTIFACT_BYTES = 4 * 1024 * 1024 * 1024;
export const DEFAULT_JOBHIVE_MAX_BATCH_BYTES = 4 * 1024 * 1024 * 1024;
export const DEFAULT_JOBHIVE_MAX_BATCH_ROWS = 10_000_000;

export class JobhiveManifestError extends Error {
  readonly code: "invalid_manifest" | "not_modified" | "too_large" | "unavailable";

  constructor(code: JobhiveManifestError["code"], message: string) {
    super(message);
    this.name = "JobhiveManifestError";
    this.code = code;
  }
}

export async function fetchJobhiveManifest(
  options: JobhiveManifestFetchOptions = {},
): Promise<JobhiveManifestFetchResult> {
  const fetcher = options.fetch ?? fetch;
  const timeoutMs = boundedInteger(options.timeoutMs ?? DEFAULT_TIMEOUT_MS, 100, 60_000, "timeout");
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const headers = new Headers({ Accept: "application/json" });
    if (options.ifNoneMatch) headers.set("If-None-Match", options.ifNoneMatch);
    const response = await fetcher(JOBHIVE_MANIFEST_URL, {
      method: "GET",
      headers,
      redirect: "error",
      signal: controller.signal,
    });
    if (response.status === 304) {
      throw new JobhiveManifestError("not_modified", "Jobhive manifest has not changed");
    }
    if (!response.ok) {
      throw new JobhiveManifestError("unavailable", `Jobhive manifest returned HTTP ${response.status}`);
    }
    const declaredLength = Number(response.headers.get("content-length") ?? "0");
    if (Number.isFinite(declaredLength) && declaredLength > MAX_MANIFEST_BYTES) {
      throw new JobhiveManifestError("too_large", "Jobhive manifest exceeds the response limit");
    }
    const content = await readBoundedBody(response, MAX_MANIFEST_BYTES);
    return {
      manifest: parseJobhiveManifest(content),
      etag: response.headers.get("etag") ?? undefined,
    };
  } catch (error) {
    if (error instanceof JobhiveManifestError) throw error;
    throw new JobhiveManifestError(
      "unavailable",
      controller.signal.aborted ? "Jobhive manifest timed out" : "Jobhive manifest could not be read",
    );
  } finally {
    clearTimeout(timer);
  }
}

export function parseJobhiveManifest(content: string): JobhiveManifest {
  if (Buffer.byteLength(content, "utf8") > MAX_MANIFEST_BYTES) {
    throw new JobhiveManifestError("too_large", "Jobhive manifest exceeds the response limit");
  }
  let raw: unknown;
  try {
    raw = JSON.parse(content);
  } catch {
    throw invalid("Jobhive manifest is not valid JSON");
  }
  const root = objectValue(raw, "manifest");
  if (stringValue(root.version, "version") !== "2.0") throw invalid("Unsupported Jobhive manifest version");
  const stats = objectValue(root.stats, "stats");
  if (stringValue(stats.schema_version, "schema version") !== "2.0") {
    throw invalid("Unsupported Jobhive schema version");
  }
  const schemaColumns = stringArray(stats.schema_columns, "schema columns");
  for (const column of JOBHIVE_REQUIRED_COLUMNS) {
    if (!schemaColumns.includes(column)) throw invalid(`Jobhive schema is missing ${column}`);
  }
  const sourceCount = integerValue(stats.ats_count, "source count", 1);
  const totalJobs = integerValue(stats.total_jobs, "total jobs", 0);
  const totalJobsRaw = integerValue(stats.total_jobs_raw, "raw total jobs", 0);
  const totalCompanies = integerValue(stats.total_companies, "total companies", 0);
  const byAts = objectValue(root.by_ats, "source snapshots");
  const sourceEntries = Object.entries(byAts);
  if (sourceEntries.length !== sourceCount) throw invalid("Jobhive source count does not match the manifest");

  const sources = sourceEntries.map(([sourceFamily, descriptor]) => {
    if (!SOURCE_FAMILY_PATTERN.test(sourceFamily)) throw invalid("Jobhive source family is invalid");
    const source = objectValue(descriptor, `source ${sourceFamily}`);
    const rows = integerValue(source.rows, `${sourceFamily} rows`, 0);
    return {
      sourceFamily,
      rows,
      csv: parseArtifact(source, sourceFamily, "csv", rows),
      parquet: parseArtifact(source, sourceFamily, "parquet", rows),
      submissionCapability: sourceSubmissionCapability(sourceFamily),
      requiresOriginalRevalidation: true as const,
    };
  }).sort((left, right) => left.sourceFamily.localeCompare(right.sourceFamily));

  const all = objectValue(root.all, "all-jobs artifact");
  const allArtifacts = {
    csv: parseArtifact(all, "all", "csv", totalJobs),
    parquet: parseArtifact(all, "all", "parquet", totalJobs),
  };
  if (allArtifacts.csv.rows !== totalJobs || allArtifacts.parquet.rows !== totalJobs) {
    throw invalid("Jobhive all-jobs row count does not match the manifest");
  }
  const rawRows = sources.reduce((sum, source) => sum + source.rows, 0);
  if (rawRows !== totalJobsRaw) throw invalid("Jobhive source rows do not match the raw total");

  return {
    version: "2.0",
    schemaVersion: "2.0",
    generatedAt: isoTimestamp(root.generated_at, "generated timestamp"),
    updatedAt: isoTimestamp(root.updated_at, "updated timestamp"),
    generator: stringValue(root.generator, "generator"),
    totalJobs,
    totalJobsRaw,
    totalCompanies,
    sourceCount,
    schemaColumns,
    all: allArtifacts,
    sources,
  };
}

/**
 * Builds bounded batches for one shared ingestion service. This intentionally
 * cannot be used as a per-account reader: every planned row still needs
 * canonical deduplication and original-employer revalidation before ranking.
 */
export function planJobhiveIngestion(
  manifest: JobhiveManifest,
  options: JobhiveIngestionPlanOptions = {},
): JobhiveIngestionPlan {
  const format = options.format ?? "parquet";
  const maxBatchBytes = boundedInteger(options.maxBatchBytes ?? DEFAULT_JOBHIVE_MAX_BATCH_BYTES, 1, 4 * 1024 * 1024 * 1024, "batch bytes");
  const maxBatchRows = boundedInteger(options.maxBatchRows ?? DEFAULT_JOBHIVE_MAX_BATCH_ROWS, 1, 10_000_000, "batch rows");
  const maxArtifactBytes = boundedInteger(options.maxArtifactBytes ?? DEFAULT_JOBHIVE_MAX_ARTIFACT_BYTES, 1, 4 * 1024 * 1024 * 1024, "artifact bytes");
  const requested = normalizeRequestedSources(options.sourceFamilies);
  const candidates = manifest.sources
    .filter((source) => requested === null || requested.has(source.sourceFamily))
    .sort(compareSources);
  if (requested) {
    for (const sourceFamily of requested) {
      if (!manifest.sources.some((source) => source.sourceFamily === sourceFamily)) {
        throw invalid(`Jobhive source ${sourceFamily} is not in the manifest`);
      }
    }
  }

  const batches: JobhiveIngestionBatch[] = [];
  const deferred: JobhiveDeferredSource[] = [];
  let current: JobhiveIngestionBatch | null = null;
  for (const source of candidates) {
    const artifact = source[format];
    if (source.rows === 0) {
      deferred.push({ sourceFamily: source.sourceFamily, rows: 0, reason: "empty" });
      continue;
    }
    if (artifact.sizeBytes > maxArtifactBytes || artifact.sizeBytes > maxBatchBytes || artifact.rows > maxBatchRows) {
      deferred.push({ sourceFamily: source.sourceFamily, rows: source.rows, reason: "artifact_over_limit" });
      continue;
    }
    if (!current
      || current.sizeBytes + artifact.sizeBytes > maxBatchBytes
      || current.rows + artifact.rows > maxBatchRows) {
      current = { batch: batches.length + 1, rows: 0, sizeBytes: 0, artifacts: [] };
      batches.push(current);
    }
    current.artifacts.push({
      sourceFamily: source.sourceFamily,
      artifact,
      submissionCapability: source.submissionCapability,
      requiresOriginalRevalidation: true,
    });
    current.rows += artifact.rows;
    current.sizeBytes += artifact.sizeBytes;
  }

  return {
    format,
    batches,
    deferred,
    plannedRows: batches.reduce((sum, batch) => sum + batch.rows, 0),
    plannedBytes: batches.reduce((sum, batch) => sum + batch.sizeBytes, 0),
    requiresSharedGlobalIndex: true,
    requiresOriginalRevalidation: true,
  };
}

function parseArtifact(
  descriptor: Record<string, unknown>,
  sourceFamily: string,
  format: JobhiveArtifactFormat,
  rows: number,
): JobhiveArtifact {
  const urlKey = format;
  const shaKey = format === "csv" ? "sha256" : "parquet_sha256";
  const sizeKey = format === "csv" ? "size_bytes" : "parquet_size_bytes";
  const url = stringValue(descriptor[urlKey], `${sourceFamily} ${format} URL`);
  const parsed = validateJobhiveArtifactUrl(url, sourceFamily, format);
  const sha256 = stringValue(descriptor[shaKey], `${sourceFamily} ${format} checksum`).toLowerCase();
  if (!SHA256_PATTERN.test(sha256)) throw invalid(`Jobhive ${sourceFamily} ${format} checksum is invalid`);
  return {
    format,
    url: parsed.toString(),
    sha256,
    sizeBytes: integerValue(descriptor[sizeKey], `${sourceFamily} ${format} bytes`, 1),
    rows,
  };
}

export function validateJobhiveArtifactUrl(
  rawUrl: string,
  sourceFamily: string,
  format: JobhiveArtifactFormat,
): URL {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw invalid(`Jobhive ${sourceFamily} ${format} URL is invalid`);
  }
  const expectedPath = sourceFamily === "all"
    ? `${JOBHIVE_BASE_PATH}all.${format}`
    : `${JOBHIVE_BASE_PATH}${sourceFamily}/jobs.${format}`;
  if (url.protocol !== "https:"
    || url.hostname !== JOBHIVE_HOST
    || url.port !== ""
    || url.username !== ""
    || url.password !== ""
    || url.pathname !== expectedPath
    || url.search !== ""
    || url.hash !== "") {
    throw invalid(`Jobhive ${sourceFamily} ${format} URL is not pinned`);
  }
  return url;
}

function sourceSubmissionCapability(sourceFamily: string): SubmissionCapability {
  if (JOBHIVE_TYPED_ATS_FAMILIES.includes(sourceFamily as typeof JOBHIVE_TYPED_ATS_FAMILIES[number])) {
    const exampleUrl = sourceFamily === "greenhouse" ? "https://boards.greenhouse.io/example/jobs/1"
      : sourceFamily === "lever" ? "https://jobs.lever.co/example/1"
        : sourceFamily === "ashby" ? "https://jobs.ashbyhq.com/example/1"
          : sourceFamily === "smartrecruiters" ? "https://jobs.smartrecruiters.com/example/1"
            : "https://example.wd5.myworkdayjobs.com/Careers/job/example_R1";
    return submissionPolicy(exampleUrl).capability;
  }
  return "unknown_review";
}

function compareSources(left: JobhiveSourceSnapshot, right: JobhiveSourceSnapshot): number {
  const leftTier = JOBHIVE_TYPED_ATS_FAMILIES.includes(left.sourceFamily as typeof JOBHIVE_TYPED_ATS_FAMILIES[number]) ? 0 : 1;
  const rightTier = JOBHIVE_TYPED_ATS_FAMILIES.includes(right.sourceFamily as typeof JOBHIVE_TYPED_ATS_FAMILIES[number]) ? 0 : 1;
  return leftTier - rightTier || left.parquet.sizeBytes - right.parquet.sizeBytes || left.sourceFamily.localeCompare(right.sourceFamily);
}

function normalizeRequestedSources(values: readonly string[] | undefined): Set<string> | null {
  if (values === undefined) return null;
  const result = new Set<string>();
  for (const raw of values) {
    const value = raw.trim().toLowerCase();
    if (!SOURCE_FAMILY_PATTERN.test(value)) throw invalid("Requested Jobhive source family is invalid");
    result.add(value);
  }
  if (result.size === 0) throw invalid("At least one Jobhive source family is required");
  return result;
}

function objectValue(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw invalid(`Jobhive ${label} is invalid`);
  return value as Record<string, unknown>;
}

function stringValue(value: unknown, label: string): string {
  if (typeof value !== "string" || value.trim() === "") throw invalid(`Jobhive ${label} is invalid`);
  return value.trim();
}

function stringArray(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string" || item.trim() === "")) {
    throw invalid(`Jobhive ${label} are invalid`);
  }
  return value.map((item) => (item as string).trim());
}

function integerValue(value: unknown, label: string, minimum: number): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < minimum) {
    throw invalid(`Jobhive ${label} is invalid`);
  }
  return value;
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw invalid(`Jobhive ${label} is outside the allowed range`);
  }
  return value;
}

function isoTimestamp(value: unknown, label: string): string {
  const timestamp = stringValue(value, label);
  if (!Number.isFinite(Date.parse(timestamp))) throw invalid(`Jobhive ${label} is invalid`);
  return timestamp;
}

function invalid(message: string): JobhiveManifestError {
  return new JobhiveManifestError("invalid_manifest", message);
}

async function readBoundedBody(response: Response, maximumBytes: number): Promise<string> {
  if (!response.body) return "";
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let size = 0;
  let output = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > maximumBytes) {
      await reader.cancel();
      throw new JobhiveManifestError("too_large", "Jobhive manifest exceeds the response limit");
    }
    output += decoder.decode(value, { stream: true });
  }
  return output + decoder.decode();
}
