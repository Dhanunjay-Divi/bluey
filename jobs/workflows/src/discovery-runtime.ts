import { createHash } from "node:crypto";
import type { JobsFetch } from "@bluey/jobs-automation";
import {
  type DiscoveredJobInput,
  type DiscoveryCompleteInput,
  DiscoveryApiError,
  type DiscoveryFailInput,
  type DiscoverySourceLease,
  type DiscoverySourceRecord,
  type DiscoveryWorkerApi,
} from "./discovery-api.js";
import {
  createInitialDiscoveryState,
  ScheduledDiscoveryLoop,
  type DiscoveryClock,
  type DiscoveryFailureCode,
  type DiscoveryLoopState,
  type DiscoveryTelemetryEvent,
  type ScheduledDiscoveryRunResult,
} from "./discovery.js";
import {
  DiscoveryConfigurationError,
  preparePublicAtsDiscovery,
  type DiscoveryConfigurationErrorCode,
  type PublicAtsDiscoveryPayload,
} from "./discovery-provider.js";

export const DEFAULT_DISCOVERY_POLL_INTERVAL_MS = 5_000;
export const MIN_DISCOVERY_POLL_INTERVAL_MS = 250;
export const MAX_DISCOVERY_POLL_INTERVAL_MS = 60_000;

const DEFAULT_SOURCE_CACHE_SIZE = 16;
const MAX_SOURCE_CACHE_SIZE = 256;

export type DiscoveryRuntimeFailureCode =
  | DiscoveryConfigurationErrorCode
  | DiscoveryFailureCode
  | "invalid_source_state"
  | "replay_mismatch"
  | "source_inactive"
  | "source_paused";

type DiscoveryReportFailureCode = DiscoveryFailureCode | "unsupported_provider";

export type DiscoveryWorkerLogEvent =
  | {
      event: "discovery_poll_failed";
      error_code: "api_rejected" | "api_timeout" | "api_unavailable" | "invalid_response" | "worker_error";
    }
  | {
      event: "discovery_reported";
      outcome: "complete" | "failed";
      source_fingerprint: string;
      replayed: boolean;
      job_count?: number;
      error_code?: DiscoveryRuntimeFailureCode;
    }
  | {
      event: "discovery_telemetry";
      telemetry: DiscoveryTelemetryEvent;
    };

type DiscoveryPollErrorCode = Extract<
  DiscoveryWorkerLogEvent,
  { event: "discovery_poll_failed" }
>["error_code"];

export interface DiscoveryWorkerLogger {
  log(event: DiscoveryWorkerLogEvent): void;
}

export type DiscoveryPollSleep = (milliseconds: number, signal: AbortSignal) => Promise<void>;

export interface DiscoveryWorkerRuntimeOptions {
  api: DiscoveryWorkerApi;
  pollIntervalMs?: number;
  logger?: DiscoveryWorkerLogger;
  atsFetch?: JobsFetch;
  atsTimeoutMs?: number;
  loopClock?: DiscoveryClock;
  sleep?: DiscoveryPollSleep;
  sourceCacheSize?: number;
}

export type DiscoveryPollOutcome = "completed" | "failed" | "idle" | "replayed";

interface CachedCompleteReport {
  kind: "complete";
  replayKey: string;
  scheduledForMs: number;
  jobs: DiscoveredJobInput[];
}

interface CachedFailureReport {
  kind: "fail";
  replayKey: string;
  scheduledForMs: number;
  errorCode: DiscoveryRuntimeFailureCode;
}

type CachedReport = CachedCompleteReport | CachedFailureReport;

interface SourceCacheEntry {
  state?: DiscoveryLoopState;
  report?: CachedReport;
}

const consoleLogger: DiscoveryWorkerLogger = {
  log: (event) => console.log(JSON.stringify(event)),
};

export class DiscoveryWorkerRuntime {
  readonly pollIntervalMs: number;

  private readonly api: DiscoveryWorkerApi;
  private readonly logger: DiscoveryWorkerLogger;
  private readonly atsFetch?: JobsFetch;
  private readonly atsTimeoutMs?: number;
  private readonly loopClock?: DiscoveryClock;
  private readonly sleep: DiscoveryPollSleep;
  private readonly sourceCacheSize: number;
  private readonly sourceCache = new Map<string, SourceCacheEntry>();
  private readonly stopController = new AbortController();
  private stopping = false;

  constructor(options: DiscoveryWorkerRuntimeOptions) {
    this.api = options.api;
    this.pollIntervalMs = boundedPollInterval(
      options.pollIntervalMs ?? DEFAULT_DISCOVERY_POLL_INTERVAL_MS,
    );
    this.logger = options.logger ?? consoleLogger;
    this.atsFetch = options.atsFetch;
    this.atsTimeoutMs = options.atsTimeoutMs;
    this.loopClock = options.loopClock;
    this.sleep = options.sleep ?? interruptibleSleep;
    this.sourceCacheSize = boundedInteger(
      options.sourceCacheSize ?? DEFAULT_SOURCE_CACHE_SIZE,
      1,
      MAX_SOURCE_CACHE_SIZE,
      "source cache size",
    );
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
            event: "discovery_poll_failed",
            error_code: safePollErrorCode(error),
          });
        }
        if (!this.stopping) {
          await this.sleep(this.pollIntervalMs, this.stopController.signal);
        }
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

  async pollOnce(): Promise<DiscoveryPollOutcome> {
    const lease = await this.api.lease();
    if (!lease) return "idle";

    const cached = this.cachedSource(lease.source.id);
    if (cached?.report?.replayKey === lease.replay_key) {
      if (cached.report.scheduledForMs !== lease.scheduled_for_ms) {
        return this.reportDirectFailure(lease, "replay_mismatch", cached.state);
      }
      await this.replayCachedReport(lease, cached.report);
      this.rememberSource(lease.source.id, cached);
      return "replayed";
    }

    const sourceStateFailure = sourceStateFailureCode(lease.source);
    if (sourceStateFailure) {
      return this.reportDirectFailure(lease, sourceStateFailure, cached?.state);
    }

    let prepared: ReturnType<typeof preparePublicAtsDiscovery>;
    try {
      prepared = preparePublicAtsDiscovery(lease.source, {
        fetch: this.atsFetch,
        timeoutMs: this.atsTimeoutMs,
      });
    } catch (error) {
      const code = error instanceof DiscoveryConfigurationError ? error.code : "invalid_config";
      return this.reportDirectFailure(lease, code, cached?.state);
    }

    const previousState = stateForLease(lease.source, cached?.state);
    let result: ScheduledDiscoveryRunResult<PublicAtsDiscoveryPayload>;
    try {
      result = await new ScheduledDiscoveryLoop({
        source: prepared.source,
        provider: prepared.provider,
        clock: this.loopClock,
        policy: { scheduleIntervalMs: lease.source.run_interval_ms },
      }).run({
        scheduledFor: new Date(lease.scheduled_for_ms).toISOString(),
        state: previousState,
      });
    } catch {
      return this.reportDirectFailure(lease, "invalid_config", cached?.state);
    }

    for (const telemetry of result.telemetry) {
      this.logger.log({ event: "discovery_telemetry", telemetry });
    }
    if (result.replayed) {
      return this.reportDirectFailure(lease, "replay_mismatch", result.state);
    }
    if (result.state.health.state !== "healthy") {
      const errorCode = resultFailureCode(result);
      const input = failureInput(lease, errorCode);
      await this.api.fail(lease.source.id, input);
      this.rememberSource(lease.source.id, {
        state: result.state,
        report: {
          kind: "fail",
          replayKey: lease.replay_key,
          scheduledForMs: lease.scheduled_for_ms,
          errorCode,
        },
      });
      this.logReport(lease.source.id, "failed", false, undefined, errorCode);
      return "failed";
    }

    const jobs = completedJobs(result);
    const input = completionInput(lease, jobs);
    await this.api.complete(lease.source.id, input);
    this.rememberSource(lease.source.id, {
      state: result.state,
      report: {
        kind: "complete",
        replayKey: lease.replay_key,
        scheduledForMs: lease.scheduled_for_ms,
        jobs,
      },
    });
    this.logReport(lease.source.id, "complete", false, jobs.length);
    return "completed";
  }

  private async replayCachedReport(lease: DiscoverySourceLease, report: CachedReport): Promise<void> {
    if (report.kind === "complete") {
      await this.api.complete(lease.source.id, completionInput(lease, report.jobs));
      this.logReport(lease.source.id, "complete", true, report.jobs.length);
      return;
    }
    await this.api.fail(lease.source.id, failureInput(lease, report.errorCode));
    this.logReport(lease.source.id, "failed", true, undefined, report.errorCode);
  }

  private async reportDirectFailure(
    lease: DiscoverySourceLease,
    errorCode: DiscoveryRuntimeFailureCode,
    state?: DiscoveryLoopState,
  ): Promise<"failed"> {
    await this.api.fail(lease.source.id, failureInput(lease, errorCode));
    this.rememberSource(lease.source.id, {
      state,
      report: {
        kind: "fail",
        replayKey: lease.replay_key,
        scheduledForMs: lease.scheduled_for_ms,
        errorCode,
      },
    });
    this.logReport(lease.source.id, "failed", false, undefined, errorCode);
    return "failed";
  }

  private logReport(
    sourceId: string,
    outcome: "complete" | "failed",
    replayed: boolean,
    jobCount?: number,
    errorCode?: DiscoveryRuntimeFailureCode,
  ): void {
    this.logger.log({
      event: "discovery_reported",
      outcome,
      source_fingerprint: sourceFingerprint(sourceId),
      replayed,
      ...(jobCount === undefined ? {} : { job_count: jobCount }),
      ...(errorCode === undefined ? {} : { error_code: errorCode }),
    });
  }

  private cachedSource(sourceId: string): SourceCacheEntry | undefined {
    const entry = this.sourceCache.get(sourceId);
    if (!entry) return undefined;
    this.sourceCache.delete(sourceId);
    this.sourceCache.set(sourceId, entry);
    return entry;
  }

  private rememberSource(sourceId: string, entry: SourceCacheEntry): void {
    this.sourceCache.delete(sourceId);
    this.sourceCache.set(sourceId, entry);
    while (this.sourceCache.size > this.sourceCacheSize) {
      const oldest = this.sourceCache.keys().next().value as string | undefined;
      if (oldest === undefined) break;
      this.sourceCache.delete(oldest);
    }
  }
}

export function boundedPollInterval(value: number): number {
  if (!Number.isFinite(value)) throw new Error("Discovery poll interval is invalid");
  return Math.min(
    MAX_DISCOVERY_POLL_INTERVAL_MS,
    Math.max(MIN_DISCOVERY_POLL_INTERVAL_MS, Math.trunc(value)),
  );
}

function stateForLease(source: DiscoverySourceRecord, cached?: DiscoveryLoopState): DiscoveryLoopState {
  const base = cached ?? createInitialDiscoveryState(source.id);
  const health = source.health === "healthy" ? "healthy" : "degraded";
  return {
    ...base,
    sourceId: source.id,
    health: {
      state: health,
      automationPaused: health !== "healthy",
      consecutiveFailures: source.consecutive_failures ?? base.health.consecutiveFailures,
      ...(health === "degraded"
        ? { reason: source.health === "waiting" ? "not_checked" as const : "provider_failure" as const }
        : {}),
      ...(timestamp(source.last_success_at_ms) ? { lastSuccessAt: timestamp(source.last_success_at_ms) } : {}),
      ...(timestamp(source.last_failure_at_ms) ? { lastFailureAt: timestamp(source.last_failure_at_ms) } : {}),
    },
  };
}

function sourceStateFailureCode(source: DiscoverySourceRecord): DiscoveryRuntimeFailureCode | undefined {
  if (source.status !== "active") return "source_inactive";
  if (source.health === "paused") return "source_paused";
  if (source.health !== "healthy" && source.health !== "degraded" && source.health !== "waiting") {
    return "invalid_source_state";
  }
  return undefined;
}

function completionInput(
  lease: DiscoverySourceLease,
  jobs: DiscoveredJobInput[],
): DiscoveryCompleteInput {
  return {
    lease_token: lease.lease_token,
    replay_key: lease.replay_key,
    scheduled_for_ms: lease.scheduled_for_ms,
    jobs,
    complete_snapshot: true,
  };
}

function failureInput(
  lease: DiscoverySourceLease,
  errorCode: DiscoveryRuntimeFailureCode,
): DiscoveryFailInput {
  return {
    lease_token: lease.lease_token,
    replay_key: lease.replay_key,
    scheduled_for_ms: lease.scheduled_for_ms,
    error_code: reportFailureCode(errorCode),
  };
}

function completedJobs(
  result: ScheduledDiscoveryRunResult<PublicAtsDiscoveryPayload>,
): DiscoveredJobInput[] {
  return result.mutations.flatMap((mutation) => {
    if (mutation.type !== "job_upserted") return [];
    const postedAtMs = mutation.job.postedAt === undefined
      ? null
      : Date.parse(mutation.job.postedAt);
    return [{
      external_id: mutation.job.externalId,
      canonical_url: mutation.job.canonicalUrl,
      title: mutation.job.payload.title,
      location: mutation.job.payload.location,
      workplace: mutation.job.payload.workplace,
      description: mutation.job.payload.description,
      compensation: mutation.job.payload.compensation,
      posted_at_ms: postedAtMs === null || !Number.isFinite(postedAtMs) ? null : postedAtMs,
    }];
  });
}

function reportFailureCode(errorCode: DiscoveryRuntimeFailureCode): DiscoveryReportFailureCode {
  switch (errorCode) {
    case "invalid_config":
    case "invalid_source_state":
    case "replay_mismatch":
      return "invalid_response";
    case "source_inactive":
    case "source_paused":
      return "provider_error";
    default:
      return errorCode;
  }
}

function resultFailureCode(
  result: ScheduledDiscoveryRunResult<PublicAtsDiscoveryPayload>,
): DiscoveryFailureCode {
  for (let index = result.telemetry.length - 1; index >= 0; index -= 1) {
    const failureCode = result.telemetry[index]?.failureCode;
    if (failureCode) return failureCode;
  }
  return "provider_error";
}

function timestamp(value: number | null | undefined): string | undefined {
  if (value === null || value === undefined || !Number.isFinite(value)) return undefined;
  return new Date(value).toISOString();
}

function sourceFingerprint(sourceId: string): string {
  return `source_${createHash("sha256").update(sourceId, "utf8").digest("hex").slice(0, 32)}`;
}

function safePollErrorCode(error: unknown): DiscoveryPollErrorCode {
  return error instanceof DiscoveryApiError ? error.code : "worker_error";
}

function interruptibleSleep(milliseconds: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted || milliseconds === 0) return Promise.resolve();
  return new Promise((resolve) => {
    const finish = (): void => {
      clearTimeout(timer);
      signal.removeEventListener("abort", finish);
      resolve();
    };
    const timer = setTimeout(finish, milliseconds);
    signal.addEventListener("abort", finish, { once: true });
  });
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Discovery ${label} must be an integer from ${minimum} to ${maximum}`);
  }
  return value;
}
