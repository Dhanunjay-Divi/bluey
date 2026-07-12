import { createHash } from "node:crypto";

export type JsonPrimitive = boolean | number | string | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type DiscoveryHealthState = "healthy" | "degraded" | "paused";
export type DiscoveryAvailability = "open" | "closed";
export type DiscoveryRemovalReason = "stale" | "closed";
export type DiscoveryControl = "run" | "pause" | "resume";

export type DiscoveryFailureCode =
  | "throttled"
  | "timeout"
  | "unavailable"
  | "invalid_response"
  | "unauthorized"
  | "provider_error";

export interface DiscoverySource {
  /** Stable server-owned identifier for exactly one public ATS source. */
  id: string;
  /** Public API or board URL used to produce source proof. */
  url: string;
}

export interface DiscoveryJob<TPayload extends JsonValue = JsonObject> {
  externalId: string;
  canonicalUrl: string;
  postedAt?: string;
  availability: DiscoveryAvailability;
  payload: TPayload;
}

export interface DiscoveryProviderSnapshot<TPayload extends JsonValue = JsonObject> {
  /** A complete source snapshot. Missing previously active jobs are closed. */
  jobs: readonly DiscoveryJob<TPayload>[];
  /** Optional provider instruction applied after this successful request. */
  nextRequestAfterMs?: number;
}

export interface DiscoveryProviderRequest {
  source: Readonly<DiscoverySource>;
  replayId: string;
  scheduledFor: string;
  requestedAt: string;
  attempt: number;
}

export interface DiscoveryProviderFailure {
  code: DiscoveryFailureCode;
  retryable: boolean;
  retryAfterMs?: number;
}

export interface ScheduledDiscoveryProvider<TPayload extends JsonValue = JsonObject> {
  readonly name: string;
  readonly version: string;
  /** Minimum spacing between request starts, including retries and later cycles. */
  readonly minimumRequestIntervalMs?: number;
  discover(request: DiscoveryProviderRequest): Promise<DiscoveryProviderSnapshot<TPayload>>;
  classifyError?(error: unknown): DiscoveryProviderFailure;
}

export interface DiscoveryClock {
  now(): Date;
  sleep(milliseconds: number): Promise<void>;
}

export interface DiscoveryPolicy {
  scheduleIntervalMs: number;
  maximumJobAgeMs: number;
  maxAttempts: number;
  initialRetryDelayMs: number;
  maximumRetryDelayMs: number;
  pauseAfterConsecutiveFailures: number;
  replayHistoryLimit: number;
  maximumJobsPerSnapshot: number;
}

export type DiscoveryHealthReason =
  | "not_checked"
  | "provider_failure"
  | "failure_threshold"
  | "operator_pause";

export interface DiscoveryHealth {
  state: DiscoveryHealthState;
  automationPaused: boolean;
  consecutiveFailures: number;
  reason?: DiscoveryHealthReason;
  lastSuccessAt?: string;
  lastFailureAt?: string;
}

export type DiscoveryVerification =
  | "listed_open"
  | "listed_closed"
  | "posted_at_expired"
  | "missing_from_snapshot";

export interface DiscoverySourceProof {
  authority: "public_ats_provider";
  sourceId: string;
  sourceUrl: string;
  provider: string;
  providerVersion: string;
  replayId: string;
  fetchedAt: string;
  externalId: string;
  canonicalUrl: string;
  contentHash: string;
  availability: DiscoveryAvailability;
  verification: DiscoveryVerification;
}

export interface ActiveDiscoveryJob {
  canonicalKey: string;
  externalId: string;
  canonicalUrl: string;
  postedAt?: string;
  contentHash: string;
  firstSeenAt: string;
  lastSeenAt: string;
  proof: DiscoverySourceProof;
}

export interface DiscoveryLoopState {
  sourceId: string;
  health: DiscoveryHealth;
  activeJobs: Readonly<Record<string, ActiveDiscoveryJob>>;
  processedReplayIds: readonly string[];
  nextProviderRequestAt?: string;
}

export interface DiscoveryUpsertEvent<TPayload extends JsonValue = JsonObject> {
  type: "job_upserted";
  eventId: string;
  replayId: string;
  canonicalKey: string;
  job: DiscoveryJob<TPayload>;
  proof: DiscoverySourceProof;
}

export interface DiscoveryRemovalEvent {
  type: "job_removed";
  eventId: string;
  replayId: string;
  canonicalKey: string;
  reason: DiscoveryRemovalReason;
  proof: DiscoverySourceProof;
}

export type DiscoveryMutation<TPayload extends JsonValue = JsonObject> =
  | DiscoveryUpsertEvent<TPayload>
  | DiscoveryRemovalEvent;

export interface DiscoveryCounts {
  received: number;
  canonical: number;
  duplicatesCollapsed: number;
  upserted: number;
  removedStale: number;
  removedClosed: number;
  active: number;
}

/**
 * Telemetry deliberately has no open-ended attributes, source URL, external ID,
 * job text, error message, query, email, resume, or answer fields.
 */
export interface DiscoveryTelemetryEvent {
  type: "attempt_failed" | "cycle_completed" | "cycle_skipped";
  eventId: string;
  replayId: string;
  provider: string;
  providerVersion: string;
  sourceFingerprint: string;
  health: DiscoveryHealthState;
  automationPaused: boolean;
  attempt: number;
  attempts: number;
  elapsedMs: number;
  waitedMs: number;
  retryDelayMs?: number;
  failureCode?: DiscoveryFailureCode;
  counts?: DiscoveryCounts;
}

export interface ScheduledDiscoveryRunInput {
  scheduledFor: string;
  state?: DiscoveryLoopState;
  control?: DiscoveryControl;
}

export interface ScheduledDiscoveryRunResult<TPayload extends JsonValue = JsonObject> {
  replayId: string;
  replayed: boolean;
  attempts: number;
  nextRunAt: string;
  state: DiscoveryLoopState;
  mutations: readonly DiscoveryMutation<TPayload>[];
  telemetry: readonly DiscoveryTelemetryEvent[];
}

export interface ScheduledDiscoveryLoopOptions<TPayload extends JsonValue = JsonObject> {
  source: DiscoverySource;
  provider: ScheduledDiscoveryProvider<TPayload>;
  clock?: DiscoveryClock;
  policy?: Partial<DiscoveryPolicy>;
}

const DEFAULT_POLICY: DiscoveryPolicy = {
  scheduleIntervalMs: 15 * 60 * 1_000,
  maximumJobAgeMs: 14 * 24 * 60 * 60 * 1_000,
  maxAttempts: 3,
  initialRetryDelayMs: 1_000,
  maximumRetryDelayMs: 60_000,
  pauseAfterConsecutiveFailures: 3,
  replayHistoryLimit: 64,
  maximumJobsPerSnapshot: 10_000,
};

const MAX_TIMER_MS = 2_147_000_000;
const MAX_FUTURE_POSTING_SKEW_MS = 24 * 60 * 60 * 1_000;
const SAFE_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const SAFE_SOURCE_ID = /^[A-Za-z0-9][A-Za-z0-9:._-]{0,199}$/;

const systemClock: DiscoveryClock = {
  now: () => new Date(),
  sleep: (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)),
};

export class DiscoveryProviderError extends Error {
  readonly code: DiscoveryFailureCode;
  readonly retryable: boolean;
  readonly retryAfterMs?: number;

  constructor(
    code: DiscoveryFailureCode,
    message: string,
    options: { retryable?: boolean; retryAfterMs?: number } = {},
  ) {
    super(message);
    this.name = "DiscoveryProviderError";
    this.code = code;
    this.retryable = options.retryable ?? true;
    this.retryAfterMs = options.retryAfterMs;
  }
}

class InvalidDiscoverySnapshotError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "InvalidDiscoverySnapshotError";
  }
}

interface NormalizedDiscoveryJob<TPayload extends JsonValue> {
  canonicalKey: string;
  contentHash: string;
  postedAtMs?: number;
  job: DiscoveryJob<TPayload>;
}

interface SnapshotProjection<TPayload extends JsonValue> {
  jobs: readonly NormalizedDiscoveryJob<TPayload>[];
  received: number;
}

interface ProjectionResult<TPayload extends JsonValue> {
  activeJobs: Record<string, ActiveDiscoveryJob>;
  mutations: DiscoveryMutation<TPayload>[];
  counts: DiscoveryCounts;
}

export function createInitialDiscoveryState(sourceId: string): DiscoveryLoopState {
  assertSourceId(sourceId);
  return {
    sourceId,
    health: {
      state: "degraded",
      automationPaused: true,
      consecutiveFailures: 0,
      reason: "not_checked",
    },
    activeJobs: {},
    processedReplayIds: [],
  };
}

export class ScheduledDiscoveryLoop<TPayload extends JsonValue = JsonObject> {
  readonly source: Readonly<DiscoverySource>;
  readonly policy: Readonly<DiscoveryPolicy>;

  private readonly provider: ScheduledDiscoveryProvider<TPayload>;
  private readonly clock: DiscoveryClock;
  private readonly minimumRequestIntervalMs: number;
  private readonly sourceFingerprint: string;

  constructor(options: ScheduledDiscoveryLoopOptions<TPayload>) {
    assertSourceId(options.source.id);
    assertSafeName(options.provider.name, "provider name");
    assertSafeName(options.provider.version, "provider version");

    this.source = {
      id: options.source.id,
      url: canonicalizePublicUrl(options.source.url, false),
    };
    this.provider = options.provider;
    this.clock = options.clock ?? systemClock;
    this.policy = { ...DEFAULT_POLICY, ...options.policy };
    this.minimumRequestIntervalMs = options.provider.minimumRequestIntervalMs ?? 0;
    this.sourceFingerprint = stableId("source", this.source.id);

    validatePolicy(this.policy);
    assertDelay(this.minimumRequestIntervalMs, "provider minimum request interval");
  }

  async run(input: ScheduledDiscoveryRunInput): Promise<ScheduledDiscoveryRunResult<TPayload>> {
    const scheduledForMs = parseRequiredTimestamp(input.scheduledFor, "scheduledFor");
    const scheduledFor = new Date(scheduledForMs).toISOString();
    const replayId = stableId("discovery", this.source.id, scheduledFor);
    const previous = input.state ?? createInitialDiscoveryState(this.source.id);
    this.assertState(previous);

    let logicalNowMs = readClock(this.clock);
    const startedAtMs = logicalNowMs;
    let waitedMs = 0;
    const now = (): number => {
      logicalNowMs = Math.max(logicalNowMs, readClock(this.clock));
      return logicalNowMs;
    };
    const sleepUntil = async (targetMs: number): Promise<number> => {
      const delay = Math.max(0, Math.ceil(targetMs - now()));
      if (delay === 0) return 0;
      await this.clock.sleep(delay);
      logicalNowMs = Math.max(targetMs, readClock(this.clock));
      waitedMs += delay;
      return delay;
    };

    if (previous.processedReplayIds.includes(replayId)) {
      return {
        replayId,
        replayed: true,
        attempts: 0,
        nextRunAt: this.nextRunAt(scheduledForMs, previous.nextProviderRequestAt, now()),
        state: previous,
        mutations: [],
        telemetry: [],
      };
    }

    const control = input.control ?? "run";
    const remainsPaused = previous.health.state === "paused" && control !== "resume";
    if (control === "pause" || remainsPaused) {
      const observedAt = new Date(now()).toISOString();
      const health: DiscoveryHealth = {
        ...previous.health,
        state: "paused",
        automationPaused: true,
        reason: control === "pause" ? "operator_pause" : previous.health.reason ?? "operator_pause",
      };
      const state: DiscoveryLoopState = {
        ...previous,
        health,
        processedReplayIds: appendReplay(previous.processedReplayIds, replayId, this.policy.replayHistoryLimit),
      };
      const telemetry = this.telemetry({
        type: "cycle_skipped",
        replayId,
        health,
        attempt: 0,
        attempts: 0,
        elapsedMs: now() - startedAtMs,
        waitedMs,
      });
      return {
        replayId,
        replayed: false,
        attempts: 0,
        nextRunAt: this.nextRunAt(scheduledForMs, state.nextProviderRequestAt, Date.parse(observedAt)),
        state,
        mutations: [],
        telemetry: [telemetry],
      };
    }

    let nextProviderRequestAtMs = parseOptionalTimestamp(previous.nextProviderRequestAt);
    let projection: SnapshotProjection<TPayload> | undefined;
    let fetchedAt = "";
    let attempts = 0;
    let lastFailure: DiscoveryProviderFailure | undefined;
    const telemetry: DiscoveryTelemetryEvent[] = [];

    for (let attempt = 1; attempt <= this.policy.maxAttempts; attempt += 1) {
      if (nextProviderRequestAtMs !== undefined) await sleepUntil(nextProviderRequestAtMs);

      attempts = attempt;
      const requestedAtMs = now();
      try {
        const snapshot = await this.provider.discover({
          source: this.source,
          replayId,
          scheduledFor,
          requestedAt: new Date(requestedAtMs).toISOString(),
          attempt,
        });
        const fetchedAtMs = now();
        projection = normalizeSnapshot(snapshot, fetchedAtMs, this.policy.maximumJobsPerSnapshot);
        fetchedAt = new Date(fetchedAtMs).toISOString();
        const successThrottle = normalizedDelay(snapshot.nextRequestAfterMs, "snapshot next request delay");
        nextProviderRequestAtMs = Math.max(
          requestedAtMs + this.minimumRequestIntervalMs,
          fetchedAtMs + successThrottle,
        );
        break;
      } catch (error) {
        const failedAtMs = now();
        lastFailure = classifyFailure(error, this.provider);
        const retryAfterMs = lastFailure.retryAfterMs ?? 0;
        nextProviderRequestAtMs = Math.max(
          requestedAtMs + this.minimumRequestIntervalMs,
          failedAtMs + retryAfterMs,
        );

        const willRetry = lastFailure.retryable && attempt < this.policy.maxAttempts;
        const retryAtMs = willRetry
          ? Math.max(
            nextProviderRequestAtMs,
            failedAtMs + retryDelay(this.policy, attempt),
          )
          : undefined;
        const retryDelayMs = retryAtMs === undefined ? undefined : Math.max(0, retryAtMs - failedAtMs);
        telemetry.push(this.telemetry({
          type: "attempt_failed",
          replayId,
          health: {
            state: "degraded",
            automationPaused: true,
            consecutiveFailures: previous.health.consecutiveFailures + 1,
            reason: "provider_failure",
          },
          attempt,
          attempts: attempt,
          elapsedMs: failedAtMs - startedAtMs,
          waitedMs,
          retryDelayMs,
          failureCode: lastFailure.code,
        }));
        if (!willRetry || retryAtMs === undefined) break;
        await sleepUntil(retryAtMs);
      }
    }

    const completedAtMs = now();
    const nextProviderRequestAt = nextProviderRequestAtMs === undefined
      ? undefined
      : new Date(nextProviderRequestAtMs).toISOString();

    if (!projection) {
      const consecutiveFailures = previous.health.consecutiveFailures + 1;
      const reachedPauseThreshold = consecutiveFailures >= this.policy.pauseAfterConsecutiveFailures;
      const health: DiscoveryHealth = {
        ...previous.health,
        state: reachedPauseThreshold ? "paused" : "degraded",
        automationPaused: true,
        consecutiveFailures,
        reason: reachedPauseThreshold ? "failure_threshold" : "provider_failure",
        lastFailureAt: new Date(completedAtMs).toISOString(),
      };
      const state: DiscoveryLoopState = {
        ...previous,
        health,
        activeJobs: { ...previous.activeJobs },
        processedReplayIds: appendReplay(previous.processedReplayIds, replayId, this.policy.replayHistoryLimit),
        nextProviderRequestAt,
      };
      const counts = emptyCounts(Object.keys(state.activeJobs).length);
      telemetry.push(this.telemetry({
        type: "cycle_completed",
        replayId,
        health,
        attempt: attempts,
        attempts,
        elapsedMs: completedAtMs - startedAtMs,
        waitedMs,
        failureCode: lastFailure?.code ?? "provider_error",
        counts,
      }));
      return {
        replayId,
        replayed: false,
        attempts,
        nextRunAt: this.nextRunAt(scheduledForMs, nextProviderRequestAt, completedAtMs),
        state,
        mutations: [],
        telemetry,
      };
    }

    const projected = this.projectSnapshot(previous, projection, replayId, fetchedAt);
    const health: DiscoveryHealth = {
      state: "healthy",
      automationPaused: false,
      consecutiveFailures: 0,
      lastSuccessAt: fetchedAt,
      lastFailureAt: previous.health.lastFailureAt,
    };
    const state: DiscoveryLoopState = {
      sourceId: this.source.id,
      health,
      activeJobs: projected.activeJobs,
      processedReplayIds: appendReplay(previous.processedReplayIds, replayId, this.policy.replayHistoryLimit),
      nextProviderRequestAt,
    };
    telemetry.push(this.telemetry({
      type: "cycle_completed",
      replayId,
      health,
      attempt: attempts,
      attempts,
      elapsedMs: completedAtMs - startedAtMs,
      waitedMs,
      counts: projected.counts,
    }));
    return {
      replayId,
      replayed: false,
      attempts,
      nextRunAt: this.nextRunAt(scheduledForMs, nextProviderRequestAt, completedAtMs),
      state,
      mutations: projected.mutations,
      telemetry,
    };
  }

  private projectSnapshot(
    previous: DiscoveryLoopState,
    snapshot: SnapshotProjection<TPayload>,
    replayId: string,
    fetchedAt: string,
  ): ProjectionResult<TPayload> {
    const activeJobs: Record<string, ActiveDiscoveryJob> = {};
    const mutations: DiscoveryMutation<TPayload>[] = [];
    const seen = new Set<string>();
    let removedStale = 0;
    let removedClosed = 0;
    let upserted = 0;
    const fetchedAtMs = Date.parse(fetchedAt);

    for (const discovered of snapshot.jobs) {
      seen.add(discovered.canonicalKey);
      const stale = isStale(discovered.postedAtMs, fetchedAtMs, this.policy.maximumJobAgeMs);
      if (discovered.job.availability === "closed" || stale) {
        const reason: DiscoveryRemovalReason = discovered.job.availability === "closed" ? "closed" : "stale";
        const verification: DiscoveryVerification = reason === "closed" ? "listed_closed" : "posted_at_expired";
        const proof = this.proof(discovered, replayId, fetchedAt, "closed", verification);
        mutations.push({
          type: "job_removed",
          eventId: mutationId(replayId, "job_removed", discovered.canonicalKey, reason),
          replayId,
          canonicalKey: discovered.canonicalKey,
          reason,
          proof,
        });
        if (reason === "closed") removedClosed += 1;
        else removedStale += 1;
        continue;
      }

      const proof = this.proof(discovered, replayId, fetchedAt, "open", "listed_open");
      const existing = previous.activeJobs[discovered.canonicalKey];
      activeJobs[discovered.canonicalKey] = {
        canonicalKey: discovered.canonicalKey,
        externalId: discovered.job.externalId,
        canonicalUrl: discovered.job.canonicalUrl,
        postedAt: discovered.job.postedAt,
        contentHash: discovered.contentHash,
        firstSeenAt: existing?.firstSeenAt ?? fetchedAt,
        lastSeenAt: fetchedAt,
        proof,
      };
      mutations.push({
        type: "job_upserted",
        eventId: mutationId(replayId, "job_upserted", discovered.canonicalKey),
        replayId,
        canonicalKey: discovered.canonicalKey,
        job: discovered.job,
        proof,
      });
      upserted += 1;
    }

    for (const canonicalKey of Object.keys(previous.activeJobs).sort()) {
      if (seen.has(canonicalKey)) continue;
      const existing = previous.activeJobs[canonicalKey];
      if (!existing) continue;
      const proof: DiscoverySourceProof = {
        ...existing.proof,
        authority: "public_ats_provider",
        sourceId: this.source.id,
        sourceUrl: this.source.url,
        provider: this.provider.name,
        providerVersion: this.provider.version,
        replayId,
        fetchedAt,
        availability: "closed",
        verification: "missing_from_snapshot",
      };
      mutations.push({
        type: "job_removed",
        eventId: mutationId(replayId, "job_removed", canonicalKey, "closed"),
        replayId,
        canonicalKey,
        reason: "closed",
        proof,
      });
      removedClosed += 1;
    }

    mutations.sort((left, right) => left.canonicalKey.localeCompare(right.canonicalKey)
      || left.type.localeCompare(right.type));
    return {
      activeJobs,
      mutations,
      counts: {
        received: snapshot.received,
        canonical: snapshot.jobs.length,
        duplicatesCollapsed: snapshot.received - snapshot.jobs.length,
        upserted,
        removedStale,
        removedClosed,
        active: Object.keys(activeJobs).length,
      },
    };
  }

  private proof(
    discovered: NormalizedDiscoveryJob<TPayload>,
    replayId: string,
    fetchedAt: string,
    availability: DiscoveryAvailability,
    verification: DiscoveryVerification,
  ): DiscoverySourceProof {
    return {
      authority: "public_ats_provider",
      sourceId: this.source.id,
      sourceUrl: this.source.url,
      provider: this.provider.name,
      providerVersion: this.provider.version,
      replayId,
      fetchedAt,
      externalId: discovered.job.externalId,
      canonicalUrl: discovered.job.canonicalUrl,
      contentHash: discovered.contentHash,
      availability,
      verification,
    };
  }

  private telemetry(input: {
    type: DiscoveryTelemetryEvent["type"];
    replayId: string;
    health: DiscoveryHealth;
    attempt: number;
    attempts: number;
    elapsedMs: number;
    waitedMs: number;
    retryDelayMs?: number;
    failureCode?: DiscoveryFailureCode;
    counts?: DiscoveryCounts;
  }): DiscoveryTelemetryEvent {
    return {
      type: input.type,
      eventId: stableId("discovery-telemetry", input.replayId, input.type, String(input.attempt)),
      replayId: input.replayId,
      provider: this.provider.name,
      providerVersion: this.provider.version,
      sourceFingerprint: this.sourceFingerprint,
      health: input.health.state,
      automationPaused: input.health.automationPaused,
      attempt: input.attempt,
      attempts: input.attempts,
      elapsedMs: input.elapsedMs,
      waitedMs: input.waitedMs,
      retryDelayMs: input.retryDelayMs,
      failureCode: input.failureCode,
      counts: input.counts,
    };
  }

  private nextRunAt(scheduledForMs: number, nextProviderRequestAt: string | undefined, nowMs: number): string {
    const providerReadyAt = parseOptionalTimestamp(nextProviderRequestAt) ?? 0;
    return new Date(Math.max(scheduledForMs + this.policy.scheduleIntervalMs, providerReadyAt, nowMs)).toISOString();
  }

  private assertState(state: DiscoveryLoopState): void {
    if (state.sourceId !== this.source.id) {
      throw new Error("Discovery state belongs to a different source");
    }
    if (state.nextProviderRequestAt !== undefined) {
      parseRequiredTimestamp(state.nextProviderRequestAt, "nextProviderRequestAt");
    }
  }
}

export async function runScheduledDiscoveryCycle<TPayload extends JsonValue = JsonObject>(
  options: ScheduledDiscoveryLoopOptions<TPayload>,
  input: ScheduledDiscoveryRunInput,
): Promise<ScheduledDiscoveryRunResult<TPayload>> {
  return new ScheduledDiscoveryLoop(options).run(input);
}

export function canonicalizeDiscoveryUrl(value: string): string {
  return canonicalizePublicUrl(value, true);
}

function normalizeSnapshot<TPayload extends JsonValue>(
  snapshot: DiscoveryProviderSnapshot<TPayload>,
  fetchedAtMs: number,
  maximumJobs: number,
): SnapshotProjection<TPayload> {
  if (!snapshot || !Array.isArray(snapshot.jobs)) {
    throw new InvalidDiscoverySnapshotError("Discovery provider did not return a job snapshot");
  }
  if (snapshot.jobs.length > maximumJobs) {
    throw new InvalidDiscoverySnapshotError("Discovery snapshot exceeded the configured job limit");
  }
  try {
    normalizedDelay(snapshot.nextRequestAfterMs, "snapshot next request delay");
  } catch {
    throw new InvalidDiscoverySnapshotError("Discovery snapshot contained an invalid throttle hint");
  }

  const grouped = new Map<string, NormalizedDiscoveryJob<TPayload>[]>();
  for (const raw of snapshot.jobs) {
    if (!raw || typeof raw !== "object") {
      throw new InvalidDiscoverySnapshotError("Discovery snapshot contained an invalid job");
    }
    if (!raw.externalId?.trim()) {
      throw new InvalidDiscoverySnapshotError("Discovery job external ID is required");
    }
    if (raw.availability !== "open" && raw.availability !== "closed") {
      throw new InvalidDiscoverySnapshotError("Discovery job availability is invalid");
    }
    const canonicalUrl = canonicalizeDiscoveryUrl(raw.canonicalUrl);
    const postedAtMs = parseOptionalTimestamp(raw.postedAt);
    const job: DiscoveryJob<TPayload> = {
      externalId: raw.externalId.trim(),
      canonicalUrl,
      postedAt: postedAtMs === undefined ? undefined : new Date(postedAtMs).toISOString(),
      availability: raw.availability,
      payload: raw.payload,
    };
    const contentHash = contentDigest(job);
    const canonicalKey = stableId("job", canonicalUrl);
    const normalized: NormalizedDiscoveryJob<TPayload> = {
      canonicalKey,
      contentHash,
      postedAtMs,
      job,
    };
    const candidates = grouped.get(canonicalKey) ?? [];
    candidates.push(normalized);
    grouped.set(canonicalKey, candidates);
  }

  const jobs = [...grouped.values()].map((candidates) => candidates.sort((left, right) => {
    const availability = availabilityRank(left.job.availability) - availabilityRank(right.job.availability);
    if (availability !== 0) return availability;
    const leftPosted = validPostedAtRank(left.postedAtMs, fetchedAtMs);
    const rightPosted = validPostedAtRank(right.postedAtMs, fetchedAtMs);
    if (leftPosted !== rightPosted) return rightPosted - leftPosted;
    return left.contentHash.localeCompare(right.contentHash)
      || left.job.externalId.localeCompare(right.job.externalId);
  })[0]).filter((job): job is NormalizedDiscoveryJob<TPayload> => job !== undefined);
  jobs.sort((left, right) => left.canonicalKey.localeCompare(right.canonicalKey));
  return { jobs, received: snapshot.jobs.length };
}

function classifyFailure<TPayload extends JsonValue>(
  error: unknown,
  provider: ScheduledDiscoveryProvider<TPayload>,
): DiscoveryProviderFailure {
  if (error instanceof InvalidDiscoverySnapshotError) {
    return { code: "invalid_response", retryable: false };
  }
  if (error instanceof DiscoveryProviderError) {
    return sanitizeFailure({
      code: error.code,
      retryable: error.retryable,
      retryAfterMs: error.retryAfterMs,
    });
  }
  if (provider.classifyError) {
    try {
      return sanitizeFailure(provider.classifyError(error));
    } catch {
      // A provider classifier cannot add arbitrary details or break the retry fence.
    }
  }
  return { code: "provider_error", retryable: true };
}

function sanitizeFailure(value: unknown): DiscoveryProviderFailure {
  if (!value || typeof value !== "object") {
    return { code: "provider_error", retryable: true };
  }
  const candidate = value as Partial<DiscoveryProviderFailure>;
  if (!isFailureCode(candidate.code) || typeof candidate.retryable !== "boolean") {
    return { code: "provider_error", retryable: true };
  }
  if (candidate.retryAfterMs !== undefined
    && (!Number.isFinite(candidate.retryAfterMs)
      || candidate.retryAfterMs < 0
      || candidate.retryAfterMs > MAX_TIMER_MS)) {
    return { code: "invalid_response", retryable: false };
  }
  return {
    code: candidate.code,
    retryable: candidate.retryable,
    retryAfterMs: candidate.retryAfterMs,
  };
}

function isFailureCode(value: unknown): value is DiscoveryFailureCode {
  return value === "throttled"
    || value === "timeout"
    || value === "unavailable"
    || value === "invalid_response"
    || value === "unauthorized"
    || value === "provider_error";
}

function retryDelay(policy: DiscoveryPolicy, failedAttempt: number): number {
  return Math.min(
    policy.maximumRetryDelayMs,
    policy.initialRetryDelayMs * 2 ** Math.max(0, failedAttempt - 1),
  );
}

function isStale(postedAtMs: number | undefined, fetchedAtMs: number, maximumAgeMs: number): boolean {
  if (postedAtMs === undefined) return true;
  if (postedAtMs > fetchedAtMs + MAX_FUTURE_POSTING_SKEW_MS) return true;
  return fetchedAtMs - postedAtMs > maximumAgeMs;
}

function validPostedAtRank(postedAtMs: number | undefined, fetchedAtMs: number): number {
  if (postedAtMs === undefined || postedAtMs > fetchedAtMs + MAX_FUTURE_POSTING_SKEW_MS) return -1;
  return postedAtMs;
}

function availabilityRank(availability: DiscoveryAvailability): number {
  return availability === "closed" ? 0 : 1;
}

function emptyCounts(active: number): DiscoveryCounts {
  return {
    received: 0,
    canonical: 0,
    duplicatesCollapsed: 0,
    upserted: 0,
    removedStale: 0,
    removedClosed: 0,
    active,
  };
}

function appendReplay(history: readonly string[], replayId: string, limit: number): string[] {
  return [...history.filter((value) => value !== replayId), replayId].slice(-limit);
}

function mutationId(
  replayId: string,
  type: DiscoveryMutation["type"],
  canonicalKey: string,
  reason = "current",
): string {
  return stableId("discovery-event", replayId, type, canonicalKey, reason);
}

function contentDigest(value: JsonValue | DiscoveryJob<JsonValue>): string {
  return `sha256:${digest(stableSerialize(value))}`;
}

function stableId(prefix: string, ...parts: string[]): string {
  return `${prefix}_${digest(parts.join("\u0000")).slice(0, 32)}`;
}

function digest(value: string): string {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function stableSerialize(value: unknown, seen = new Set<object>()): string {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new InvalidDiscoverySnapshotError("Discovery payload contains a non-finite number");
    return JSON.stringify(value);
  }
  if (!value || typeof value !== "object") {
    throw new InvalidDiscoverySnapshotError("Discovery payload is not JSON serializable");
  }
  if (seen.has(value)) throw new InvalidDiscoverySnapshotError("Discovery payload contains a cycle");
  seen.add(value);
  try {
    if (Array.isArray(value)) {
      return `[${value.map((item) => stableSerialize(item, seen)).join(",")}]`;
    }
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== Object.prototype && prototype !== null) {
      throw new InvalidDiscoverySnapshotError("Discovery payload contains a non-JSON object");
    }
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record).sort().map((key) => (
      `${JSON.stringify(key)}:${stableSerialize(record[key], seen)}`
    )).join(",")}}`;
  } finally {
    seen.delete(value);
  }
}

function canonicalizePublicUrl(value: string, removeTracking: boolean): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new InvalidDiscoverySnapshotError("Discovery URL is invalid");
  }
  if (url.protocol !== "https:" || url.username || url.password) {
    throw new InvalidDiscoverySnapshotError("Discovery URLs must be public HTTPS URLs without credentials");
  }
  url.hash = "";
  if (removeTracking) {
    for (const key of [...url.searchParams.keys()]) {
      if (/^(?:utm_.+|source|sourceid|gh_src|lever-source|referrer)$/i.test(key)) {
        url.searchParams.delete(key);
      }
    }
  }
  url.searchParams.sort();
  if (url.pathname.length > 1) url.pathname = url.pathname.replace(/\/+$/, "");
  return url.toString();
}

function validatePolicy(policy: DiscoveryPolicy): void {
  assertPositiveDelay(policy.scheduleIntervalMs, "schedule interval");
  assertPositiveDelay(policy.maximumJobAgeMs, "maximum job age");
  assertPositiveDelay(policy.initialRetryDelayMs, "initial retry delay");
  assertPositiveDelay(policy.maximumRetryDelayMs, "maximum retry delay");
  if (policy.maximumRetryDelayMs < policy.initialRetryDelayMs) {
    throw new Error("Maximum retry delay cannot be less than the initial retry delay");
  }
  assertIntegerInRange(policy.maxAttempts, 1, 10, "max attempts");
  assertIntegerInRange(policy.pauseAfterConsecutiveFailures, 1, 100, "failure pause threshold");
  assertIntegerInRange(policy.replayHistoryLimit, 1, 1_000, "replay history limit");
  assertIntegerInRange(policy.maximumJobsPerSnapshot, 1, 100_000, "snapshot job limit");
}

function assertSourceId(value: string): void {
  if (!SAFE_SOURCE_ID.test(value)) throw new Error("Discovery source ID is invalid");
}

function assertSafeName(value: string, label: string): void {
  if (!SAFE_NAME.test(value)) throw new Error(`Discovery ${label} is invalid`);
}

function assertIntegerInRange(value: number, minimum: number, maximum: number, label: string): void {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Discovery ${label} must be an integer from ${minimum} to ${maximum}`);
  }
}

function assertPositiveDelay(value: number, label: string): void {
  if (!Number.isFinite(value) || value <= 0 || value > MAX_TIMER_MS) {
    throw new Error(`Discovery ${label} is invalid`);
  }
}

function assertDelay(value: number, label: string): void {
  if (!Number.isFinite(value) || value < 0 || value > MAX_TIMER_MS) {
    throw new Error(`Discovery ${label} is invalid`);
  }
}

function normalizedDelay(value: number | undefined, label: string): number {
  if (value === undefined) return 0;
  assertDelay(value, label);
  return value;
}

function parseRequiredTimestamp(value: string, label: string): number {
  const result = Date.parse(value);
  if (!Number.isFinite(result)) throw new Error(`Discovery ${label} is invalid`);
  return result;
}

function parseOptionalTimestamp(value: string | undefined): number | undefined {
  if (value === undefined || value.trim() === "") return undefined;
  const result = Date.parse(value);
  return Number.isFinite(result) ? result : undefined;
}

function readClock(clock: DiscoveryClock): number {
  const result = clock.now().getTime();
  if (!Number.isFinite(result)) throw new Error("Discovery clock returned an invalid time");
  return result;
}
