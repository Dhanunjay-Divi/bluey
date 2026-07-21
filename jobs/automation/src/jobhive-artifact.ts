import { createHash, randomUUID } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, open, rename, rm } from "node:fs/promises";
import path from "node:path";

import { parse } from "csv-parse";

import {
  JOBHIVE_REQUIRED_COLUMNS,
  type JobhiveArtifact,
  validateJobhiveArtifactUrl,
} from "./jobhive-manifest.js";

const DEFAULT_TIMEOUT_MS = 30 * 60_000;
const DEFAULT_MAX_ARTIFACT_BYTES = 4 * 1024 * 1024 * 1024;
const DEFAULT_BATCH_ROWS = 500;
const DEFAULT_MAX_RECORD_BYTES = 2 * 1024 * 1024;
const SOURCE_FAMILY_PATTERN = /^[a-z0-9][a-z0-9_-]{0,63}$/;

export interface DownloadJobhiveArtifactOptions {
  artifact: JobhiveArtifact;
  sourceFamily: string;
  stagingDirectory: string;
  fetch?: typeof fetch;
  timeoutMs?: number;
  maxArtifactBytes?: number;
}

export interface VerifiedJobhiveArtifact {
  artifact: JobhiveArtifact;
  sourceFamily: string;
  path: string;
  bytes: number;
  sha256: string;
}

export interface JobhiveCandidateRow {
  url: string;
  title: string;
  company: string;
  atsType: string;
  atsId: string;
  location: string;
  isRemote: boolean | null;
  salaryMin: number | null;
  salaryMax: number | null;
  salaryCurrency: string;
  salaryPeriod: string;
  salarySummary: string;
  employmentType: string;
  department: string;
  team: string;
  description: string;
  postedAt: string;
  requisitionId: string;
  applyUrl: string;
  commitment: string;
  raw: string;
  countryIso: string;
}

export interface StreamJobhiveCsvOptions {
  verified: VerifiedJobhiveArtifact;
  onBatch: (rows: JobhiveCandidateRow[]) => void | Promise<void>;
  batchRows?: number;
  maxRecordBytes?: number;
}

export interface StreamJobhiveCsvResult {
  rows: number;
  batches: number;
}

export class JobhiveArtifactError extends Error {
  readonly code:
    | "invalid_artifact"
    | "checksum_mismatch"
    | "row_count_mismatch"
    | "too_large"
    | "unavailable";

  constructor(code: JobhiveArtifactError["code"], message: string) {
    super(message);
    this.name = "JobhiveArtifactError";
    this.code = code;
  }
}

/**
 * Downloads an immutable artifact to private staging and publishes it only
 * after its byte count and SHA-256 match validated manifest metadata fetched
 * from the pinned TLS origin. The upstream manifest is not cryptographically
 * signed, so it is candidate-feed evidence rather than application truth.
 */
export async function downloadJobhiveArtifact(
  options: DownloadJobhiveArtifactOptions,
): Promise<VerifiedJobhiveArtifact> {
  const sourceFamily = normalizedSourceFamily(options.sourceFamily);
  const artifact = options.artifact;
  validateJobhiveArtifactUrl(artifact.url, sourceFamily, artifact.format);
  const timeoutMs = boundedInteger(options.timeoutMs ?? DEFAULT_TIMEOUT_MS, 100, 2 * 60 * 60_000, "timeout");
  const maxArtifactBytes = boundedInteger(
    options.maxArtifactBytes ?? DEFAULT_MAX_ARTIFACT_BYTES,
    1,
    4 * 1024 * 1024 * 1024,
    "artifact bytes",
  );
  if (artifact.sizeBytes > maxArtifactBytes) {
    throw new JobhiveArtifactError("too_large", "Jobhive artifact exceeds the configured byte limit");
  }

  const stagingDirectory = path.resolve(options.stagingDirectory);
  await mkdir(stagingDirectory, { recursive: true, mode: 0o700 });
  const suffix = `${artifact.sha256.slice(0, 16)}.${artifact.format}`;
  const finalPath = path.join(stagingDirectory, `${sourceFamily}-${suffix}`);
  const temporaryPath = path.join(stagingDirectory, `.${sourceFamily}-${randomUUID()}.partial`);
  const cached = await reuseVerifiedArtifact({ artifact, sourceFamily, path: finalPath });
  if (cached) return cached;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  let handle: Awaited<ReturnType<typeof open>> | null = null;

  try {
    const response = await (options.fetch ?? fetch)(artifact.url, {
      method: "GET",
      headers: {
        Accept: artifact.format === "csv" ? "text/csv" : "application/octet-stream",
        "Accept-Encoding": "identity",
      },
      redirect: "error",
      signal: controller.signal,
    });
    if (!response.ok || !response.body) {
      throw new JobhiveArtifactError("unavailable", `Jobhive artifact returned HTTP ${response.status}`);
    }
    const contentEncoding = response.headers.get("content-encoding");
    if (contentEncoding && contentEncoding.toLowerCase() !== "identity") {
      throw new JobhiveArtifactError("invalid_artifact", "Jobhive artifact used an unexpected content encoding");
    }
    const declaredLength = response.headers.get("content-length");
    if (declaredLength !== null && Number(declaredLength) !== artifact.sizeBytes) {
      throw new JobhiveArtifactError("invalid_artifact", "Jobhive artifact byte count does not match its manifest");
    }

    handle = await open(temporaryPath, "wx", 0o600);
    const hash = createHash("sha256");
    const reader = response.body.getReader();
    let bytes = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > artifact.sizeBytes || bytes > maxArtifactBytes) {
        await reader.cancel();
        throw new JobhiveArtifactError("too_large", "Jobhive artifact exceeded its declared byte count");
      }
      hash.update(value);
      await handle.write(value);
    }
    await handle.sync();
    await handle.close();
    handle = null;
    if (bytes !== artifact.sizeBytes) {
      throw new JobhiveArtifactError("invalid_artifact", "Jobhive artifact ended before its declared byte count");
    }
    const sha256 = hash.digest("hex");
    if (sha256 !== artifact.sha256) {
      throw new JobhiveArtifactError("checksum_mismatch", "Jobhive artifact checksum did not match its manifest");
    }
    await rename(temporaryPath, finalPath);
    return { artifact, sourceFamily, path: finalPath, bytes, sha256 };
  } catch (error) {
    if (handle) await handle.close().catch(() => undefined);
    await rm(temporaryPath, { force: true }).catch(() => undefined);
    if (error instanceof JobhiveArtifactError) throw error;
    throw new JobhiveArtifactError(
      "unavailable",
      controller.signal.aborted ? "Jobhive artifact download timed out" : "Jobhive artifact could not be downloaded",
    );
  } finally {
    clearTimeout(timer);
  }
}

async function reuseVerifiedArtifact(input: {
  artifact: JobhiveArtifact;
  sourceFamily: string;
  path: string;
}): Promise<VerifiedJobhiveArtifact | null> {
  let handle: Awaited<ReturnType<typeof open>> | null = null;
  let shouldDiscard = false;
  try {
    handle = await open(input.path, "r");
    const metadata = await handle.stat();
    if (metadata.size !== input.artifact.sizeBytes) {
      shouldDiscard = true;
    } else {
      const hash = createHash("sha256");
      let bytes = 0;
      for await (const chunk of handle.createReadStream({ autoClose: false })) {
        bytes += chunk.byteLength;
        hash.update(chunk);
      }
      const sha256 = hash.digest("hex");
      if (bytes === input.artifact.sizeBytes && sha256 === input.artifact.sha256) {
        return {
          artifact: input.artifact,
          sourceFamily: input.sourceFamily,
          path: input.path,
          bytes,
          sha256,
        };
      }
      shouldDiscard = true;
    }
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return null;
    throw new JobhiveArtifactError("unavailable", "Verified Jobhive artifact cache could not be read");
  } finally {
    if (handle) await handle.close().catch(() => undefined);
  }
  if (shouldDiscard) await rm(input.path, { force: true }).catch(() => undefined);
  return null;
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

/**
 * Streams a previously verified CSV in bounded batches. Consumers should
 * write these rows to the shared candidate index, never directly to one
 * account. Original-source revalidation remains mandatory before ranking.
 */
export async function streamVerifiedJobhiveCsv(
  options: StreamJobhiveCsvOptions,
): Promise<StreamJobhiveCsvResult> {
  if (options.verified.artifact.format !== "csv") {
    throw new JobhiveArtifactError("invalid_artifact", "Only verified CSV artifacts can be streamed");
  }
  const batchRows = boundedInteger(options.batchRows ?? DEFAULT_BATCH_ROWS, 1, 10_000, "batch rows");
  const maxRecordBytes = boundedInteger(
    options.maxRecordBytes ?? DEFAULT_MAX_RECORD_BYTES,
    1024,
    16 * 1024 * 1024,
    "record bytes",
  );
  let headersValidated = false;
  const parser = createReadStream(options.verified.path, { highWaterMark: 256 * 1024 }).pipe(parse({
    bom: true,
    columns: (headers: string[]) => {
      validateHeaders(headers);
      headersValidated = true;
      return headers;
    },
    max_record_size: maxRecordBytes,
    relax_column_count: false,
    skip_empty_lines: true,
  }));

  let batch: JobhiveCandidateRow[] = [];
  let rows = 0;
  let batches = 0;
  try {
    for await (const record of parser) {
      rows += 1;
      if (rows > options.verified.artifact.rows) {
        throw new JobhiveArtifactError("row_count_mismatch", "Jobhive artifact contains more rows than its manifest");
      }
      batch.push(normalizeCandidate(record as Record<string, string>));
      if (batch.length >= batchRows) {
        await options.onBatch(batch);
        batches += 1;
        batch = [];
      }
    }
  } catch (error) {
    if (error instanceof JobhiveArtifactError) throw error;
    throw new JobhiveArtifactError("invalid_artifact", "Jobhive CSV could not be parsed safely");
  }
  if (!headersValidated) throw new JobhiveArtifactError("invalid_artifact", "Jobhive CSV has no header row");
  if (rows !== options.verified.artifact.rows) {
    throw new JobhiveArtifactError("row_count_mismatch", "Jobhive artifact row count does not match its manifest");
  }
  if (batch.length > 0) {
    await options.onBatch(batch);
    batches += 1;
  }
  return { rows, batches };
}

function validateHeaders(headers: string[]): void {
  const normalized = headers.map((header) => header.trim());
  if (new Set(normalized).size !== normalized.length) {
    throw new JobhiveArtifactError("invalid_artifact", "Jobhive CSV contains duplicate columns");
  }
  for (const column of JOBHIVE_REQUIRED_COLUMNS) {
    if (!normalized.includes(column)) {
      throw new JobhiveArtifactError("invalid_artifact", `Jobhive CSV is missing ${column}`);
    }
  }
}

function normalizeCandidate(record: Record<string, string>): JobhiveCandidateRow {
  const url = boundedText(record.url, "url", 4096);
  const applyUrl = boundedText(record.apply_url, "apply URL", 4096);
  validateCandidateUrl(url, "job URL");
  validateCandidateUrl(applyUrl, "apply URL");
  return {
    url,
    title: boundedText(record.title, "title", 1024),
    company: boundedText(record.company, "company", 1024),
    atsType: boundedText(record.ats_type, "ATS type", 128),
    atsId: boundedText(record.ats_id, "ATS ID", 1024),
    location: boundedText(record.location, "location", 2048),
    isRemote: nullableBoolean(record.is_remote),
    salaryMin: nullableNumber(record.salary_min, "minimum salary"),
    salaryMax: nullableNumber(record.salary_max, "maximum salary"),
    salaryCurrency: boundedText(record.salary_currency, "salary currency", 32),
    salaryPeriod: boundedText(record.salary_period, "salary period", 64),
    salarySummary: boundedText(record.salary_summary, "salary summary", 4096),
    employmentType: boundedText(record.employment_type, "employment type", 256),
    department: boundedText(record.department, "department", 1024),
    team: boundedText(record.team, "team", 1024),
    description: boundedText(record.description, "description", 1024 * 1024),
    postedAt: boundedText(record.posted_at, "posted timestamp", 128),
    requisitionId: boundedText(record.requisition_id, "requisition ID", 1024),
    applyUrl,
    commitment: boundedText(record.commitment, "commitment", 256),
    raw: boundedText(record.raw, "raw source record", 1024 * 1024),
    countryIso: boundedText(record.country_iso, "country ISO", 32),
  };
}

function validateCandidateUrl(raw: string, label: string): void {
  if (raw === "") return;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new JobhiveArtifactError("invalid_artifact", `Jobhive ${label} is invalid`);
  }
  if ((url.protocol !== "https:" && url.protocol !== "http:") || url.username || url.password) {
    throw new JobhiveArtifactError("invalid_artifact", `Jobhive ${label} is unsafe`);
  }
}

function boundedText(value: string | undefined, label: string, maximumLength: number): string {
  const text = typeof value === "string"
    ? value.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g, " ").trim()
    : "";
  if (text.length > maximumLength) {
    throw new JobhiveArtifactError("invalid_artifact", `Jobhive ${label} exceeds its field limit`);
  }
  return text;
}

function nullableBoolean(value: string | undefined): boolean | null {
  const normalized = (value ?? "").trim().toLowerCase();
  if (normalized === "") return null;
  if (["true", "1", "yes"].includes(normalized)) return true;
  if (["false", "0", "no"].includes(normalized)) return false;
  throw new JobhiveArtifactError("invalid_artifact", "Jobhive remote flag is invalid");
}

function nullableNumber(value: string | undefined, label: string): number | null {
  const normalized = (value ?? "").trim();
  if (normalized === "") return null;
  const parsed = Number(normalized);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new JobhiveArtifactError("invalid_artifact", `Jobhive ${label} is invalid`);
  }
  return parsed;
}

function normalizedSourceFamily(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (!SOURCE_FAMILY_PATTERN.test(normalized)) {
    throw new JobhiveArtifactError("invalid_artifact", "Jobhive source family is invalid");
  }
  return normalized;
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new JobhiveArtifactError("invalid_artifact", `Jobhive ${label} is outside the allowed range`);
  }
  return value;
}
