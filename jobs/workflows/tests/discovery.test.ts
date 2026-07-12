import { describe, expect, it, vi } from "vitest";
import {
  DiscoveryProviderError,
  ScheduledDiscoveryLoop,
  type DiscoveryClock,
  type DiscoveryJob,
  type DiscoveryProviderRequest,
  type ScheduledDiscoveryProvider,
} from "../src/discovery.js";

const SOURCE = {
  id: "greenhouse:acme",
  url: "https://boards-api.greenhouse.io/v1/boards/acme/jobs",
};
const SLOT_ONE = "2026-07-11T12:00:00.000Z";
const SLOT_TWO = "2026-07-11T12:15:00.000Z";
const SLOT_THREE = "2026-07-11T12:30:00.000Z";

class FakeClock implements DiscoveryClock {
  readonly sleeps: number[] = [];
  private milliseconds: number;

  constructor(now: string) {
    this.milliseconds = Date.parse(now);
  }

  now(): Date {
    return new Date(this.milliseconds);
  }

  async sleep(milliseconds: number): Promise<void> {
    this.sleeps.push(milliseconds);
    this.milliseconds += milliseconds;
  }

  set(now: string): void {
    this.milliseconds = Date.parse(now);
  }
}

function job(overrides: Partial<DiscoveryJob> = {}): DiscoveryJob {
  return {
    externalId: "job-1",
    canonicalUrl: "https://jobs.acme.test/roles/job-1",
    postedAt: "2026-07-10T12:00:00.000Z",
    availability: "open",
    payload: {
      company: "Acme",
      title: "Platform Engineer",
      location: "Remote",
    },
    ...overrides,
  };
}

function provider(
  discover: ScheduledDiscoveryProvider["discover"],
  minimumRequestIntervalMs = 0,
): ScheduledDiscoveryProvider {
  return {
    name: "public-ats",
    version: "1.0.0",
    minimumRequestIntervalMs,
    discover,
  };
}

describe("scheduled discovery loop", () => {
  it("uses stable replay IDs and skips an already completed replay", async () => {
    const clock = new FakeClock(SLOT_ONE);
    const discover = vi.fn(async () => ({ jobs: [job()] }));
    const loop = new ScheduledDiscoveryLoop({ source: SOURCE, provider: provider(discover), clock });

    const first = await loop.run({ scheduledFor: SLOT_ONE });
    const replay = await loop.run({ scheduledFor: SLOT_ONE, state: first.state });

    expect(discover).toHaveBeenCalledTimes(1);
    expect(replay.replayId).toBe(first.replayId);
    expect(replay.replayed).toBe(true);
    expect(replay.mutations).toEqual([]);
    expect(replay.telemetry).toEqual([]);
    expect(first.mutations).toHaveLength(1);
    expect(first.mutations[0]?.eventId).toMatch(/^discovery-event_[a-f0-9]{32}$/);
    expect(first.mutations[0]?.proof).toMatchObject({
      authority: "public_ats_provider",
      sourceId: SOURCE.id,
      provider: "public-ats",
      providerVersion: "1.0.0",
      replayId: first.replayId,
      verification: "listed_open",
    });
    expect(first.mutations[0]?.proof.contentHash).toMatch(/^sha256:[a-f0-9]{64}$/);
  });

  it("collapses canonical duplicates independently of provider order", async () => {
    const tracked = job({
      externalId: "tracked-copy",
      canonicalUrl: "https://JOBS.acme.test/roles/job-1/?b=2&utm_source=feed&a=1#apply",
      payload: { title: "Platform Engineer", copy: "tracked" },
    });
    const direct = job({
      externalId: "direct-copy",
      canonicalUrl: "https://jobs.acme.test/roles/job-1?a=1&b=2",
      payload: { title: "Platform Engineer", copy: "direct" },
    });
    const forwardDiscover = vi.fn(async () => ({ jobs: [tracked, direct] }));
    const reverseDiscover = vi.fn(async () => ({ jobs: [direct, tracked] }));
    const forward = new ScheduledDiscoveryLoop({
      source: SOURCE,
      provider: provider(forwardDiscover),
      clock: new FakeClock(SLOT_ONE),
    });
    const reverse = new ScheduledDiscoveryLoop({
      source: SOURCE,
      provider: provider(reverseDiscover),
      clock: new FakeClock(SLOT_ONE),
    });

    const left = await forward.run({ scheduledFor: SLOT_ONE });
    const right = await reverse.run({ scheduledFor: SLOT_ONE });

    expect(left.mutations).toHaveLength(1);
    expect(right.mutations).toEqual(left.mutations);
    expect(left.mutations[0]).toMatchObject({
      type: "job_upserted",
      job: { canonicalUrl: "https://jobs.acme.test/roles/job-1?a=1&b=2" },
    });
    expect(left.telemetry.at(-1)?.counts).toMatchObject({
      received: 2,
      canonical: 1,
      duplicatesCollapsed: 1,
      upserted: 1,
    });
  });

  it("emits removals for stale, explicitly closed, and missing jobs", async () => {
    const clock = new FakeClock(SLOT_ONE);
    const initialJobs = [
      job({ externalId: "stale", canonicalUrl: "https://jobs.acme.test/roles/stale" }),
      job({ externalId: "closed", canonicalUrl: "https://jobs.acme.test/roles/closed" }),
      job({ externalId: "missing", canonicalUrl: "https://jobs.acme.test/roles/missing" }),
    ];
    const discover = vi.fn()
      .mockResolvedValueOnce({ jobs: initialJobs })
      .mockResolvedValueOnce({
        jobs: [
          job({
            externalId: "stale",
            canonicalUrl: "https://jobs.acme.test/roles/stale",
            postedAt: "2026-05-01T12:00:00.000Z",
          }),
          job({
            externalId: "closed",
            canonicalUrl: "https://jobs.acme.test/roles/closed",
            availability: "closed",
          }),
        ],
      });
    const loop = new ScheduledDiscoveryLoop({ source: SOURCE, provider: provider(discover), clock });
    const seeded = await loop.run({ scheduledFor: SLOT_ONE });
    clock.set(SLOT_TWO);

    const result = await loop.run({ scheduledFor: SLOT_TWO, state: seeded.state });
    const removals = result.mutations.map((mutation) => ({
      externalId: mutation.proof.externalId,
      type: mutation.type,
      reason: mutation.type === "job_removed" ? mutation.reason : undefined,
      verification: mutation.proof.verification,
    }));

    expect(removals).toEqual(expect.arrayContaining([
      { externalId: "stale", type: "job_removed", reason: "stale", verification: "posted_at_expired" },
      { externalId: "closed", type: "job_removed", reason: "closed", verification: "listed_closed" },
      { externalId: "missing", type: "job_removed", reason: "closed", verification: "missing_from_snapshot" },
    ]));
    expect(result.mutations).toHaveLength(3);
    expect(result.state.activeJobs).toEqual({});
    expect(result.telemetry.at(-1)?.counts).toMatchObject({
      upserted: 0,
      removedStale: 1,
      removedClosed: 2,
      active: 0,
    });
  });

  it("bounds retries, honors provider throttles, and excludes raw errors from telemetry", async () => {
    const clock = new FakeClock(SLOT_ONE);
    const requests: DiscoveryProviderRequest[] = [];
    const discover = vi.fn(async (request: DiscoveryProviderRequest) => {
      requests.push(request);
      throw new DiscoveryProviderError(
        "throttled",
        "candidate@example.test searched for secret platform role",
        { retryAfterMs: 2_500 },
      );
    });
    const loop = new ScheduledDiscoveryLoop({
      source: SOURCE,
      provider: provider(discover, 1_000),
      clock,
      policy: {
        maxAttempts: 3,
        initialRetryDelayMs: 100,
        maximumRetryDelayMs: 100,
        pauseAfterConsecutiveFailures: 5,
      },
    });

    const result = await loop.run({ scheduledFor: SLOT_ONE });

    expect(discover).toHaveBeenCalledTimes(3);
    expect(result.attempts).toBe(3);
    expect(clock.sleeps).toEqual([2_500, 2_500]);
    expect(requests.map((request) => request.requestedAt)).toEqual([
      "2026-07-11T12:00:00.000Z",
      "2026-07-11T12:00:02.500Z",
      "2026-07-11T12:00:05.000Z",
    ]);
    expect(result.state.health).toMatchObject({
      state: "degraded",
      automationPaused: true,
      consecutiveFailures: 1,
      reason: "provider_failure",
    });
    expect(result.telemetry.filter((event) => event.type === "attempt_failed")).toHaveLength(3);
    const serializedTelemetry = JSON.stringify(result.telemetry);
    expect(serializedTelemetry).not.toContain("candidate@example.test");
    expect(serializedTelemetry).not.toContain("secret platform role");
    expect(serializedTelemetry).not.toContain(SOURCE.url);
    expect(serializedTelemetry).not.toContain(SOURCE.id);
  });

  it("sanitizes a malformed provider throttle hint without escaping the retry fence", async () => {
    const discover = vi.fn(async () => {
      throw new Error("raw provider failure");
    });
    const malformedProvider: ScheduledDiscoveryProvider = {
      ...provider(discover),
      classifyError: () => ({
        code: "throttled",
        retryable: true,
        retryAfterMs: Number.POSITIVE_INFINITY,
      }),
    };
    const loop = new ScheduledDiscoveryLoop({
      source: SOURCE,
      provider: malformedProvider,
      clock: new FakeClock(SLOT_ONE),
      policy: { maxAttempts: 3, pauseAfterConsecutiveFailures: 5 },
    });

    const result = await loop.run({ scheduledFor: SLOT_ONE });

    expect(discover).toHaveBeenCalledTimes(1);
    expect(result.attempts).toBe(1);
    expect(result.state.health.state).toBe("degraded");
    expect(result.telemetry.at(-1)?.failureCode).toBe("invalid_response");
  });

  it("keeps a failed source degraded until a successful recovery probe", async () => {
    const clock = new FakeClock(SLOT_ONE);
    const discover = vi.fn()
      .mockRejectedValueOnce(new DiscoveryProviderError(
        "unavailable",
        "source unavailable",
        { retryable: false },
      ))
      .mockResolvedValueOnce({ jobs: [job()] });
    const loop = new ScheduledDiscoveryLoop({
      source: SOURCE,
      provider: provider(discover),
      clock,
      policy: { pauseAfterConsecutiveFailures: 3 },
    });

    const failed = await loop.run({ scheduledFor: SLOT_ONE });
    expect(failed.attempts).toBe(1);
    expect(failed.state.health).toMatchObject({ state: "degraded", automationPaused: true });
    expect(failed.mutations).toEqual([]);

    clock.set(SLOT_TWO);
    const recovered = await loop.run({ scheduledFor: SLOT_TWO, state: failed.state });
    expect(discover).toHaveBeenCalledTimes(2);
    expect(recovered.state.health).toMatchObject({
      state: "healthy",
      automationPaused: false,
      consecutiveFailures: 0,
    });
    expect(recovered.mutations).toHaveLength(1);
  });

  it("does not contact a paused source until it is explicitly resumed", async () => {
    const clock = new FakeClock(SLOT_ONE);
    const discover = vi.fn(async () => ({ jobs: [job()] }));
    const loop = new ScheduledDiscoveryLoop({ source: SOURCE, provider: provider(discover), clock });

    const paused = await loop.run({ scheduledFor: SLOT_ONE, control: "pause" });
    expect(paused.state.health).toMatchObject({
      state: "paused",
      automationPaused: true,
      reason: "operator_pause",
    });
    expect(discover).not.toHaveBeenCalled();

    clock.set(SLOT_TWO);
    const stillPaused = await loop.run({ scheduledFor: SLOT_TWO, state: paused.state });
    expect(stillPaused.state.health.state).toBe("paused");
    expect(discover).not.toHaveBeenCalled();

    clock.set(SLOT_THREE);
    const resumed = await loop.run({
      scheduledFor: SLOT_THREE,
      state: stillPaused.state,
      control: "resume",
    });
    expect(discover).toHaveBeenCalledTimes(1);
    expect(resumed.state.health).toMatchObject({ state: "healthy", automationPaused: false });
  });
});
