import { createHash, createHmac } from "node:crypto";
import type { FetchResponse, JobsFetch } from "@bluey/jobs-automation";
import { describe, expect, it, vi } from "vitest";
import {
  DiscoveryApiClient,
  type DiscoveryApiFetch,
  type DiscoveryCompleteInput,
  type DiscoveryFailInput,
  type DiscoverySourceLease,
  type DiscoverySourceRecord,
  type DiscoveryWorkerApi,
} from "../src/discovery-api.js";
import type { DiscoveryClock } from "../src/discovery.js";
import {
  DiscoveryConfigurationError,
  preparePublicAtsDiscovery,
} from "../src/discovery-provider.js";
import {
  DiscoveryWorkerRuntime,
  type DiscoveryPollSleep,
  type DiscoveryWorkerLogEvent,
  type DiscoveryWorkerLogger,
} from "../src/discovery-runtime.js";

const SCHEDULED_FOR_MS = Date.now();
const SCHEDULED_FOR = new Date(SCHEDULED_FOR_MS).toISOString();
const WORKER_SIGNING_KEY = "discovery-signing-key-0123456789abcdef";

class ImmediateClock implements DiscoveryClock {
  private milliseconds = SCHEDULED_FOR_MS;

  now(): Date {
    return new Date(this.milliseconds);
  }

  async sleep(milliseconds: number): Promise<void> {
    this.milliseconds += milliseconds;
  }
}

class FakeApi implements DiscoveryWorkerApi {
  readonly completed: Array<{ sourceId: string; input: DiscoveryCompleteInput }> = [];
  readonly failed: Array<{ sourceId: string; input: DiscoveryFailInput }> = [];
  leaseCalls = 0;

  constructor(private readonly leases: Array<DiscoverySourceLease | null>) {}

  async lease(): Promise<DiscoverySourceLease | null> {
    this.leaseCalls += 1;
    return this.leases.shift() ?? null;
  }

  async complete(sourceId: string, input: DiscoveryCompleteInput): Promise<void> {
    this.completed.push({ sourceId, input });
  }

  async fail(sourceId: string, input: DiscoveryFailInput): Promise<void> {
    this.failed.push({ sourceId, input });
  }
}

class RecordingLogger implements DiscoveryWorkerLogger {
  readonly events: DiscoveryWorkerLogEvent[] = [];

  log(event: DiscoveryWorkerLogEvent): void {
    this.events.push(event);
  }
}

function source(overrides: Partial<DiscoverySourceRecord> = {}): DiscoverySourceRecord {
  return {
    id: "source-greenhouse-acme",
    account_id: "account-test",
    track_id: "track-test",
    provider: "greenhouse",
    source_key: "acme",
    config: { board_token: "acme", company: "Acme" },
    status: "active",
    health: "healthy",
    run_interval_ms: 15 * 60 * 1_000,
    consecutive_failures: 0,
    ...overrides,
  };
}

function lease(
  sourceOverrides: Partial<DiscoverySourceRecord> = {},
  leaseOverrides: Partial<DiscoverySourceLease> = {},
): DiscoverySourceLease {
  return {
    source: source(sourceOverrides),
    lease_token: "lease-token-test",
    replay_key: "replay-key-test",
    scheduled_for_ms: SCHEDULED_FOR_MS,
    ...leaseOverrides,
  };
}

function response(payload: unknown, status = 200): FetchResponse {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(payload),
  };
}

function greenhousePayload(): unknown {
  return {
    jobs: [{
      id: "job-1",
      title: "Platform Engineer",
      absolute_url: "https://boards.greenhouse.io/acme/jobs/job-1?gh_src=private",
      location: { name: "Remote" },
      content: "<p>Build reliable systems.</p>",
      updated_at: SCHEDULED_FOR,
    }],
  };
}

describe("discovery worker runtime", () => {
  it("handles a no-lease poll without starting provider work", async () => {
    const requests: Array<{ input: string; init?: RequestInit }> = [];
    const fetcher: DiscoveryApiFetch = vi.fn(async (input, init) => {
      requests.push({ input: String(input), init });
      return new Response(null, { status: 204 });
    });
    const client = new DiscoveryApiClient({
      origin: "https://jobs.internal",
      signingKey: WORKER_SIGNING_KEY,
      workerId: "discovery-worker-test",
      fetch: fetcher,
    });
    const worker = new DiscoveryWorkerRuntime({ api: client, logger: new RecordingLogger() });

    await expect(worker.pollOnce()).resolves.toBe("idle");

    expect(requests).toHaveLength(1);
    expect(requests[0]?.input).toBe("https://jobs.internal/api/jobs/internal/discovery/lease");
    expectSignedDiscoveryRequest(requests[0], "discovery-worker-test");
  });

  it("uses one bounded process-generated worker ID when no override is supplied", async () => {
    const workerIds: Array<string | null> = [];
    const client = new DiscoveryApiClient({
      origin: "https://jobs.internal",
      signingKey: WORKER_SIGNING_KEY,
      fetch: async (_input, init) => {
        workerIds.push(new Headers(init?.headers).get("x-bluey-jobs-worker-id"));
        return new Response(null, { status: 204 });
      },
    });

    await client.lease();
    await client.lease();

    expect(workerIds[0]).toBe(workerIds[1]);
    expect(workerIds[0]).toMatch(/^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/);
  });

  it("allows plaintext API traffic only for loopback development", () => {
    expect(() => new DiscoveryApiClient({
      origin: "http://jobs.internal:8080",
      signingKey: WORKER_SIGNING_KEY,
    })).toThrow("invalid");
    expect(() => new DiscoveryApiClient({
      origin: "http://127.0.0.1:8080",
      signingKey: WORKER_SIGNING_KEY,
    })).not.toThrow();
  });

  it("reports a successful Greenhouse result as one complete snapshot", async () => {
    const api = new FakeApi([lease()]);
    const fetched: string[] = [];
    const atsFetch: JobsFetch = vi.fn(async (url, init) => {
      fetched.push(url);
      expect(init.redirect).toBe("error");
      return response(greenhousePayload());
    });
    const logger = new RecordingLogger();
    const worker = new DiscoveryWorkerRuntime({
      api,
      atsFetch,
      loopClock: new ImmediateClock(),
      logger,
    });

    await expect(worker.pollOnce()).resolves.toBe("completed");

    expect(fetched).toEqual([
      "https://boards-api.greenhouse.io/v1/boards/acme/jobs?content=true",
    ]);
    expect(api.failed).toEqual([]);
    expect(api.completed).toHaveLength(1);
    expect(api.completed[0]).toMatchObject({
      sourceId: "source-greenhouse-acme",
      input: {
        lease_token: "lease-token-test",
        replay_key: "replay-key-test",
        scheduled_for_ms: SCHEDULED_FOR_MS,
        complete_snapshot: true,
        jobs: [{
          external_id: "job-1",
          canonical_url: "https://boards.greenhouse.io/acme/jobs/job-1",
          title: "Platform Engineer",
          location: "Remote",
          workplace: "remote",
          description: "Build reliable systems.",
          compensation: "",
          posted_at_ms: SCHEDULED_FOR_MS,
        }],
      },
    });
    expect(logger.events.some((event) => event.event === "discovery_telemetry")).toBe(true);
  });

  it("pins Lever discovery to the official API host", async () => {
    const fetched: string[] = [];
    const prepared = preparePublicAtsDiscovery(source({
      id: "source-lever-atlas",
      provider: "lever",
      source_key: "atlas",
      config: { kind: "lever", site: "atlas", company: "Atlas" },
    }), {
      fetch: async (url) => {
        fetched.push(url);
        return response([{
          id: "lever-1",
          text: "Product Engineer",
          hostedUrl: "https://jobs.lever.co/atlas/lever-1",
          categories: { location: "New York, NY" },
          createdAt: SCHEDULED_FOR_MS,
        }]);
      },
    });

    const snapshot = await prepared.provider.discover({
      source: prepared.source,
      replayId: "replay-test",
      scheduledFor: SCHEDULED_FOR,
      requestedAt: SCHEDULED_FOR,
      attempt: 1,
    });

    expect(fetched).toEqual(["https://api.lever.co/v0/postings/atlas?mode=json"]);
    expect(snapshot.jobs[0]).toMatchObject({
      externalId: "lever-1",
      postedAt: SCHEDULED_FOR,
      payload: { company: "Atlas", source: "lever" },
    });
  });

  it("maps every scheduled ATS source to its official provider endpoint", async () => {
    const cases: Array<{
      source: DiscoverySourceRecord;
      proofUrl: string;
      requestUrl: string;
      method?: string;
      payload: unknown;
    }> = [
      {
        source: source({
          id: "source-ashby-acme",
          provider: "ashby",
          source_key: "acme",
          config: { kind: "ashby", boardName: "acme", company: "Acme" },
        }),
        proofUrl: "https://api.ashbyhq.com/posting-api/job-board/acme",
        requestUrl: "https://api.ashbyhq.com/posting-api/job-board/acme",
        payload: { jobs: [] },
      },
      {
        source: source({
          id: "source-smartrecruiters-acme",
          provider: "smartrecruiters",
          source_key: "acme",
          config: { kind: "smartrecruiters", companyIdentifier: "acme", company: "Acme" },
        }),
        proofUrl: "https://api.smartrecruiters.com/v1/companies/acme/postings",
        requestUrl: "https://api.smartrecruiters.com/v1/companies/acme/postings?limit=100&offset=0",
        payload: { content: [], totalFound: 0 },
      },
      {
        source: source({
          id: "source-workday-acme",
          provider: "workday",
          source_key: "acme~wd5~careers",
          config: {
            kind: "workday",
            tenant: "acme",
            instance: "wd5",
            site: "careers",
            locale: "en-US",
            company: "Acme",
          },
        }),
        proofUrl: "https://acme.wd5.myworkdayjobs.com/wday/cxs/acme/careers/jobs",
        requestUrl: "https://acme.wd5.myworkdayjobs.com/wday/cxs/acme/careers/jobs",
        method: "POST",
        payload: { jobPostings: [], total: 0 },
      },
    ];

    for (const testCase of cases) {
      const requests: Array<{ url: string; method?: string }> = [];
      const prepared = preparePublicAtsDiscovery(testCase.source, {
        fetch: async (url, init) => {
          requests.push({ url, method: init.method });
          expect(init.redirect).toBe("error");
          return response(testCase.payload);
        },
      });

      expect(prepared.source.url).toBe(testCase.proofUrl);
      await prepared.provider.discover({
        source: prepared.source,
        replayId: `replay-${testCase.source.provider}`,
        scheduledFor: SCHEDULED_FOR,
        requestedAt: SCHEDULED_FOR,
        attempt: 1,
      });
      expect(requests).toEqual([{ url: testCase.requestUrl, method: testCase.method }]);
    }
  });

  it("runs a newly created source whose server health is waiting", async () => {
    const api = new FakeApi([lease({ health: "waiting" })]);
    const worker = new DiscoveryWorkerRuntime({
      api,
      atsFetch: async () => response(greenhousePayload()),
      loopClock: new ImmediateClock(),
      logger: new RecordingLogger(),
    });

    await expect(worker.pollOnce()).resolves.toBe("completed");
    expect(api.completed).toHaveLength(1);
    expect(api.failed).toEqual([]);
  });

  it("reports a provider failure instead of completing an empty snapshot", async () => {
    const api = new FakeApi([lease()]);
    const atsFetch: JobsFetch = vi.fn(async () => {
      throw new TypeError("network unavailable");
    });
    const worker = new DiscoveryWorkerRuntime({
      api,
      atsFetch,
      loopClock: new ImmediateClock(),
      logger: new RecordingLogger(),
    });

    await expect(worker.pollOnce()).resolves.toBe("failed");

    expect(atsFetch).toHaveBeenCalledTimes(3);
    expect(api.completed).toEqual([]);
    expect(api.failed).toEqual([{
      sourceId: "source-greenhouse-acme",
      input: {
        lease_token: "lease-token-test",
        replay_key: "replay-key-test",
        scheduled_for_ms: SCHEDULED_FOR_MS,
        error_code: "unavailable",
      },
    }]);
  });

  it("replays a duplicate lease without polling the provider again", async () => {
    const duplicate = lease();
    const api = new FakeApi([duplicate, duplicate]);
    const atsFetch: JobsFetch = vi.fn(async () => response(greenhousePayload()));
    const worker = new DiscoveryWorkerRuntime({
      api,
      atsFetch,
      loopClock: new ImmediateClock(),
      logger: new RecordingLogger(),
    });

    await expect(worker.pollOnce()).resolves.toBe("completed");
    await expect(worker.pollOnce()).resolves.toBe("replayed");

    expect(atsFetch).toHaveBeenCalledTimes(1);
    expect(api.completed).toHaveLength(2);
    expect(api.completed[1]?.input).toEqual(api.completed[0]?.input);
  });

  it("stops gracefully while waiting for the next bounded poll", async () => {
    const api = new FakeApi([null]);
    let enteredSleep: (() => void) | undefined;
    const sleeping = new Promise<void>((resolve) => {
      enteredSleep = resolve;
    });
    const sleep: DiscoveryPollSleep = vi.fn(async (_milliseconds, signal) => {
      enteredSleep?.();
      await new Promise<void>((resolve) => {
        if (signal.aborted) resolve();
        else signal.addEventListener("abort", () => resolve(), { once: true });
      });
    });
    const worker = new DiscoveryWorkerRuntime({
      api,
      pollIntervalMs: 1,
      sleep,
      logger: new RecordingLogger(),
    });

    const running = worker.run();
    await sleeping;
    worker.stop();
    await running;

    expect(worker.pollIntervalMs).toBe(250);
    expect(api.leaseCalls).toBe(1);
    expect(sleep).toHaveBeenCalledTimes(1);
  });

  it("fails invalid or unsupported server config without accepting a host", async () => {
    const api = new FakeApi([lease({
      config: {
        board_token: "acme",
        host: "https://candidate@example.test/private",
      },
    })]);
    const atsFetch: JobsFetch = vi.fn(async () => response(greenhousePayload()));
    const worker = new DiscoveryWorkerRuntime({
      api,
      atsFetch,
      logger: new RecordingLogger(),
    });

    await expect(worker.pollOnce()).resolves.toBe("failed");
    expect(atsFetch).not.toHaveBeenCalled();
    expect(api.failed[0]?.input.error_code).toBe("invalid_response");

    try {
      preparePublicAtsDiscovery(source({ provider: "unknown" }));
      throw new Error("Expected unsupported provider config to fail");
    } catch (error) {
      expect(error).toBeInstanceOf(DiscoveryConfigurationError);
      expect((error as DiscoveryConfigurationError).code).toBe("unsupported_provider");
    }

    expect(() => preparePublicAtsDiscovery(source({
      provider: "workday",
      source_key: "acme~wd5~careers",
      config: {
        kind: "workday",
        tenant: "other",
        instance: "wd5",
        site: "careers",
      },
    }))).toThrow("do not match");
  });

  it("never includes raw errors or candidate PII in logs", async () => {
    const api = new FakeApi([lease({
      account_id: "candidate@example.test",
      config: { board_token: "acme", company: "candidate@example.test" },
    })]);
    const logger = new RecordingLogger();
    const atsFetch: JobsFetch = vi.fn(async () => {
      throw new Error("candidate@example.test resume profile answers secret-value");
    });
    const worker = new DiscoveryWorkerRuntime({
      api,
      atsFetch,
      loopClock: new ImmediateClock(),
      logger,
    });

    await worker.pollOnce();

    const logs = JSON.stringify(logger.events);
    expect(logs).not.toContain("candidate@example.test");
    expect(logs).not.toContain("resume");
    expect(logs).not.toContain("profile");
    expect(logs).not.toContain("answers");
    expect(logs).not.toContain("secret-value");
    expect(logs).toContain("provider_error");
  });

  it("retries an idempotent completion with the exact same lease body", async () => {
    const requests: Array<{
      input: string;
      body: string;
      init?: RequestInit;
    }> = [];
    const fetcher: DiscoveryApiFetch = vi.fn(async (requestInput, init) => {
      requests.push({
        input: String(requestInput),
        body: String(init?.body),
        init,
      });
      return new Response(null, { status: requests.length === 1 ? 503 : 204 });
    });
    const client = new DiscoveryApiClient({
      origin: "https://jobs.internal",
      signingKey: WORKER_SIGNING_KEY,
      workerId: "discovery-worker-test",
      fetch: fetcher,
      reportRetryMs: 0,
      sleep: async () => undefined,
    });
    const input: DiscoveryCompleteInput = {
      lease_token: "lease-token-test",
      replay_key: "replay-key-test",
      scheduled_for_ms: SCHEDULED_FOR_MS,
      jobs: [],
      complete_snapshot: true,
    };

    await client.complete("source-greenhouse-acme", input);

    expect(requests).toHaveLength(2);
    expect(requests[1]?.body).toBe(requests[0]?.body);
    expect(JSON.parse(requests[0]?.body ?? "{}")).toMatchObject({
      lease_token: "lease-token-test",
      replay_key: "replay-key-test",
      complete_snapshot: true,
    });
    for (const request of requests) expectSignedDiscoveryRequest(request, "discovery-worker-test");
    expect(new Headers(requests[0]?.init?.headers).get("x-bluey-jobs-worker-nonce"))
      .not.toBe(new Headers(requests[1]?.init?.headers).get("x-bluey-jobs-worker-nonce"));
  });
});

function expectSignedDiscoveryRequest(
  request: { input: string; init?: RequestInit } | undefined,
  workerId: string,
): void {
  expect(request).toBeDefined();
  const url = new URL(request?.input ?? "https://invalid.example");
  const headers = new Headers(request?.init?.headers);
  const body = String(request?.init?.body ?? "");
  const timestamp = headers.get("x-bluey-jobs-worker-timestamp");
  const nonce = headers.get("x-bluey-jobs-worker-nonce");
  const contentSha256 = createHash("sha256").update(body).digest("hex");
  expect(headers.get("authorization")).toBeNull();
  expect(headers.get("x-bluey-jobs-worker-id")).toBe(workerId);
  expect(headers.get("x-bluey-jobs-worker-audience")).toBe("bluey-jobs-api");
  expect(headers.get("x-bluey-jobs-worker-scope")).toBe("discovery");
  expect(headers.get("x-bluey-jobs-worker-content-sha256")).toBe(contentSha256);
  expect(timestamp).toMatch(/^\d+$/);
  expect(nonce).toMatch(/^[A-Za-z0-9._:-]{24,128}$/);
  const canonical = [
    "bluey-jobs-worker-v1",
    timestamp,
    nonce,
    workerId,
    "bluey-jobs-api",
    "discovery",
    "POST",
    url.pathname,
    contentSha256,
  ].join("\n");
  expect(headers.get("x-bluey-jobs-worker-signature")).toBe(
    createHmac("sha256", WORKER_SIGNING_KEY).update(canonical).digest("hex"),
  );
}
