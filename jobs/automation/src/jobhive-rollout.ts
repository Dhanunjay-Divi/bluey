import type {
  JobhiveArtifactFormat,
  JobhiveManifest,
  JobhiveSourceSnapshot,
} from "./jobhive-manifest.js";
import type { SubmissionCapability } from "./policy.js";

export type JobhiveSourceClass =
  | "typed_ats"
  | "ats_feed"
  | "direct_employer"
  | "public_service"
  | "job_board";

export interface JobhiveRolloutOptions {
  format?: JobhiveArtifactFormat;
  maxRowsPerWave?: number;
  maxBytesPerWave?: number;
}

export interface JobhiveRolloutSource {
  sourceFamily: string;
  sourceClass: JobhiveSourceClass;
  rows: number;
  sizeBytes: number;
  submissionCapability: SubmissionCapability;
  requiresOriginalRevalidation: true;
  requiresCapacityReview: boolean;
}

export interface JobhiveRolloutWave {
  wave: number;
  sourceFamilies: string[];
  plannedRows: number;
  plannedBytes: number;
  requiresCapacityReview: boolean;
}

export interface JobhiveRolloutPlan {
  format: JobhiveArtifactFormat;
  sources: JobhiveRolloutSource[];
  waves: JobhiveRolloutWave[];
  deferred: Array<{ sourceFamily: string; reason: "empty" }>;
  quarantined: Array<{ sourceFamily: string; reason: "unclassified_source_family" }>;
  plannedRows: number;
  plannedBytes: number;
  requiresOriginalRevalidation: true;
}

const DEFAULT_MAX_ROWS_PER_WAVE = 500_000;
const DEFAULT_MAX_BYTES_PER_WAVE = 2 * 1024 * 1024 * 1024;

const SOURCE_CLASSES: Readonly<Record<JobhiveSourceClass, readonly string[]>> = {
  typed_ats: ["greenhouse", "lever", "ashby", "smartrecruiters", "workday"],
  ats_feed: [
    "avature",
    "bamboohr",
    "beisen",
    "breezy",
    "cornerstone",
    "eightfold",
    "gem",
    "icims",
    "jazzhr",
    "join_com",
    "oracle",
    "personio",
    "phenom",
    "pinpoint",
    "recruitee",
    "recruiterbox",
    "rippling",
    "successfactors",
    "taleo",
    "teamtailor",
    "workable",
  ],
  direct_employer: ["amazon", "apple", "google", "meta", "tesla", "tiktok", "uber"],
  public_service: ["arbetsformedlingen", "bundesagentur", "eures"],
  job_board: [
    "builtin",
    "getonbrd",
    "jobsch",
    "manfred",
    "mercor",
    "programathor",
    "remoteok",
    "thehub",
    "wanted",
    "welcometothejungle",
    "wellfound",
    "weworkremotely",
    "ycombinator",
  ],
};

const SOURCE_CLASS_BY_FAMILY = new Map<string, JobhiveSourceClass>(
  Object.entries(SOURCE_CLASSES).flatMap(([sourceClass, sourceFamilies]) => (
    sourceFamilies.map((sourceFamily) => [sourceFamily, sourceClass as JobhiveSourceClass])
  )),
);

const SOURCE_CLASS_PRIORITY: Readonly<Record<JobhiveSourceClass, number>> = {
  typed_ats: 0,
  ats_feed: 1,
  direct_employer: 2,
  public_service: 3,
  job_board: 4,
};

export const KNOWN_JOBHIVE_SOURCE_FAMILIES = Object.freeze(
  [...SOURCE_CLASS_BY_FAMILY.keys()].sort(),
);

export function classifyJobhiveSourceFamily(sourceFamily: string): JobhiveSourceClass | undefined {
  return SOURCE_CLASS_BY_FAMILY.get(sourceFamily.trim().toLowerCase());
}

/**
 * Produces deterministic, bounded source allowlists for the shared ingestion
 * service. Classification controls rollout order only: it never promotes a
 * source's submission capability or removes original-source revalidation.
 */
export function planJobhiveSourceRollout(
  manifest: JobhiveManifest,
  options: JobhiveRolloutOptions = {},
): JobhiveRolloutPlan {
  const format = options.format ?? "csv";
  const maxRowsPerWave = positiveInteger(
    options.maxRowsPerWave ?? DEFAULT_MAX_ROWS_PER_WAVE,
    "max rows per wave",
  );
  const maxBytesPerWave = positiveInteger(
    options.maxBytesPerWave ?? DEFAULT_MAX_BYTES_PER_WAVE,
    "max bytes per wave",
  );
  const deferred: JobhiveRolloutPlan["deferred"] = [];
  const quarantined: JobhiveRolloutPlan["quarantined"] = [];
  const sources: JobhiveRolloutSource[] = [];

  for (const source of manifest.sources) {
    const sourceClass = classifyJobhiveSourceFamily(source.sourceFamily);
    if (!sourceClass) {
      quarantined.push({
        sourceFamily: source.sourceFamily,
        reason: "unclassified_source_family",
      });
      continue;
    }
    if (source.rows === 0) {
      deferred.push({ sourceFamily: source.sourceFamily, reason: "empty" });
      continue;
    }
    sources.push(rolloutSource(source, sourceClass, format, maxRowsPerWave, maxBytesPerWave));
  }

  sources.sort((left, right) => (
    SOURCE_CLASS_PRIORITY[left.sourceClass] - SOURCE_CLASS_PRIORITY[right.sourceClass]
    || left.rows - right.rows
    || left.sourceFamily.localeCompare(right.sourceFamily)
  ));
  deferred.sort((left, right) => left.sourceFamily.localeCompare(right.sourceFamily));
  quarantined.sort((left, right) => left.sourceFamily.localeCompare(right.sourceFamily));

  const waves: JobhiveRolloutWave[] = [];
  let pending: JobhiveRolloutSource[] = [];
  const flush = () => {
    if (pending.length === 0) return;
    waves.push({
      wave: waves.length + 1,
      sourceFamilies: pending.map((source) => source.sourceFamily),
      plannedRows: pending.reduce((total, source) => total + source.rows, 0),
      plannedBytes: pending.reduce((total, source) => total + source.sizeBytes, 0),
      requiresCapacityReview: pending.some((source) => source.requiresCapacityReview),
    });
    pending = [];
  };

  for (const source of sources) {
    const pendingRows = pending.reduce((total, item) => total + item.rows, 0);
    const pendingBytes = pending.reduce((total, item) => total + item.sizeBytes, 0);
    if (
      pending.length > 0
      && (pendingRows + source.rows > maxRowsPerWave
        || pendingBytes + source.sizeBytes > maxBytesPerWave)
    ) {
      flush();
    }
    pending.push(source);
    if (source.requiresCapacityReview) flush();
  }
  flush();

  return {
    format,
    sources,
    waves,
    deferred,
    quarantined,
    plannedRows: sources.reduce((total, source) => total + source.rows, 0),
    plannedBytes: sources.reduce((total, source) => total + source.sizeBytes, 0),
    requiresOriginalRevalidation: true,
  };
}

export function jobhiveRolloutAllowlist(
  plan: JobhiveRolloutPlan,
  throughWave: number,
): string {
  if (!Number.isInteger(throughWave) || throughWave < 1) {
    throw new Error("through wave must be a positive integer");
  }
  return plan.waves
    .filter((wave) => wave.wave <= throughWave)
    .flatMap((wave) => wave.sourceFamilies)
    .join(",");
}

function rolloutSource(
  source: JobhiveSourceSnapshot,
  sourceClass: JobhiveSourceClass,
  format: JobhiveArtifactFormat,
  maxRowsPerWave: number,
  maxBytesPerWave: number,
): JobhiveRolloutSource {
  const artifact = source[format];
  return {
    sourceFamily: source.sourceFamily,
    sourceClass,
    rows: source.rows,
    sizeBytes: artifact.sizeBytes,
    submissionCapability: source.submissionCapability,
    requiresOriginalRevalidation: true,
    requiresCapacityReview: source.rows > maxRowsPerWave || artifact.sizeBytes > maxBytesPerWave,
  };
}

function positiveInteger(value: number, label: string): number {
  if (!Number.isInteger(value) || value < 1) throw new Error(`${label} must be a positive integer`);
  return value;
}
