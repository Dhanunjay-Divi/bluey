import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ExecutionLeaseClient,
  ExecutionLeaseError,
  runnerOwnerId,
} from "../src/execution-lease.js";

const CLAIM = {
  accountId: "account-123",
  applicationId: "application-123",
  runId: "run-123",
  browserProfileId: "profile-123",
};

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("execution lease client", () => {
  it("claims with the worker credential and keeps lease data in internal requests", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetch = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(input), init });
      if (String(input).endsWith("/claim")) return grantResponse();
      if (String(input).endsWith("/irreversible")) return recordResponse("click_started");
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const client = createClient(fetch);

    const lease = await client.claim(CLAIM);
    await lease.beforeFinalSubmit();
    await lease.afterFinalSubmit("activated");
    await lease.finish("submitted");

    expect(calls).toHaveLength(3);
    expect(calls[0]?.url).toBe("https://jobs-api.example/api/jobs/internal/execution-leases/claim");
    expect(calls[0]?.init).toMatchObject({ method: "POST", redirect: "error" });
    expect(new Headers(calls[0]?.init?.headers).get("authorization")).toBeNull();
    expect(new Headers(calls[0]?.init?.headers).get("x-bluey-jobs-worker-scope")).toBe("execution");
    expect(new Headers(calls[0]?.init?.headers).get("x-bluey-jobs-worker-audience")).toBe("bluey-jobs-api");
    expect(new Headers(calls[0]?.init?.headers).get("x-bluey-jobs-worker-content-sha256"))
      .toMatch(/^[a-f0-9]{64}$/);
    expect(new Headers(calls[0]?.init?.headers).get("x-bluey-jobs-worker-signature"))
      .toMatch(/^[a-f0-9]{64}$/);
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      account_id: "account-123",
      application_id: "application-123",
      run_id: "run-123",
      browser_profile_id: "profile-123",
      owner_id: "runner-test-1",
    });
    expect(JSON.parse(String(calls[1]?.init?.body))).toMatchObject({
      lease_token: "lease-secret-value",
      fence: 7,
      action: "submit",
    });
    expect(JSON.parse(String(calls[2]?.init?.body))).toMatchObject({
      lease_token: "lease-secret-value",
      fence: 7,
      outcome: "submitted",
    });
    expect(JSON.stringify(lease)).not.toContain("lease-secret-value");
  });

  it("makes the irreversible fence single-shot when its success response is lost", async () => {
    let irreversibleCalls = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/irreversible")) {
        irreversibleCalls += 1;
        throw new Error("network failure containing worker-secret-token and lease-secret-value");
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch).claim(CLAIM);

    const first = await lease.beforeFinalSubmit().catch((error: unknown) => error);
    const second = await lease.beforeFinalSubmit().catch((error: unknown) => error);

    expect(first).toBeInstanceOf(ExecutionLeaseError);
    expect(String(first)).not.toContain("worker-secret-token");
    expect(String(first)).not.toContain("lease-secret-value");
    expect(second).toMatchObject({ code: "invalid_state" });
    expect(irreversibleCalls).toBe(1);
    expect(lease.finalSubmitAttempted).toBe(true);
    expect(lease.finalSubmitAuthorized).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it("does not authorize a click from an incomplete successful fence response", async () => {
    const fetch = vi.fn(async (input: string | URL | Request) => {
      if (String(input).endsWith("/claim")) return grantResponse();
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch).claim(CLAIM);

    await expect(lease.beforeFinalSubmit()).rejects.toMatchObject({ code: "invalid_response" });
    expect(lease.finalSubmitAttempted).toBe(true);
    expect(lease.finalSubmitAuthorized).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it("times out stalled responses and rejects oversized or redirected responses", async () => {
    vi.useFakeTimers();
    const stalledFetch = vi.fn((_input: string | URL | Request, init?: RequestInit) => (
      new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(new Error("aborted")), { once: true });
      })
    )) as typeof globalThis.fetch;
    const stalledClaim = createClient(stalledFetch, { requestTimeoutMs: 100 }).claim(CLAIM);
    const stalledExpectation = expect(stalledClaim).rejects.toMatchObject({ code: "timed_out" });
    await vi.advanceTimersByTimeAsync(100);
    await stalledExpectation;
    vi.useRealTimers();

    const oversized = createClient(vi.fn(async () => new Response("x".repeat(300), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    })) as typeof globalThis.fetch, { maxResponseBytes: 256 });
    await expect(oversized.claim(CLAIM)).rejects.toMatchObject({ code: "response_too_large" });

    const redirected = createClient(vi.fn(async () => new Response("", {
      status: 302,
      headers: { Location: "https://elsewhere.example/lease" },
    })) as typeof globalThis.fetch);
    await expect(redirected.claim(CLAIM)).rejects.toMatchObject({ code: "redirect_blocked" });
  });

  it("reports duplicate claims without exposing the server response", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({
      error: "owner worker-secret-token lease-secret-value https://private.example",
    }), {
      status: 409,
      headers: { "Content-Type": "application/json" },
    })) as typeof globalThis.fetch;

    const error = await createClient(fetch).claim(CLAIM).catch((caught: unknown) => caught);

    expect(error).toMatchObject({ code: "lease_unavailable", status: 409 });
    expect(String(error)).not.toContain("worker-secret-token");
    expect(String(error)).not.toContain("lease-secret-value");
    expect(String(error)).not.toContain("private.example");
  });

  it("heartbeats while active, records failures, and cleans up on finish", async () => {
    vi.useFakeTimers();
    let heartbeatCalls = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/heartbeat")) {
        heartbeatCalls += 1;
        if (heartbeatCalls === 1) throw new Error("transient heartbeat failure");
        return recordResponse("prepared");
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch, { heartbeatIntervalMs: 100 }).claim(CLAIM);

    await vi.advanceTimersByTimeAsync(100);
    expect(heartbeatCalls).toBe(1);
    expect(lease.heartbeatFailureCode).toBe("request_failed");
    await vi.advanceTimersByTimeAsync(100);
    expect(heartbeatCalls).toBe(2);
    expect(lease.heartbeatFailureCode).toBeUndefined();

    await lease.finish("failed");
    expect(lease.heartbeatActive).toBe(false);
    await vi.advanceTimersByTimeAsync(500);
    expect(heartbeatCalls).toBe(2);
  });

  it("revalidates the live fence before submit after a heartbeat failure", async () => {
    vi.useFakeTimers();
    let irreversibleCalls = 0;
    const fetch = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/claim")) return grantResponse();
      if (url.endsWith("/heartbeat")) throw new Error("heartbeat unavailable");
      if (url.endsWith("/irreversible")) {
        irreversibleCalls += 1;
        return new Response(JSON.stringify({ error: "expired" }), {
          status: 409,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;
    const lease = await createClient(fetch, { heartbeatIntervalMs: 100 }).claim(CLAIM);
    await vi.advanceTimersByTimeAsync(100);
    expect(lease.heartbeatFailureCode).toBe("request_failed");

    await expect(lease.beforeFinalSubmit()).rejects.toMatchObject({ status: 409 });

    expect(irreversibleCalls).toBe(1);
    expect(lease.finalSubmitAttempted).toBe(true);
    expect(lease.finalSubmitAuthorized).toBe(false);
    await lease.finish("side_effect_unknown");
  });

  it("derives a stable bounded process owner without preserving unsafe input", () => {
    const fallback = runnerOwnerId();
    expect(runnerOwnerId()).toBe(fallback);
    expect(Buffer.byteLength(fallback)).toBeLessThanOrEqual(128);

    const unsafe = "runner with spaces/".repeat(20);
    const bounded = runnerOwnerId(unsafe);
    expect(runnerOwnerId(unsafe)).toBe(bounded);
    expect(bounded).toMatch(/^runner-[a-f0-9]{48}$/);
    expect(Buffer.byteLength(bounded)).toBeLessThanOrEqual(128);
  });

  it("allows plaintext worker credentials only for explicit loopback development origins", () => {
    const fetch = vi.fn() as typeof globalThis.fetch;
    expect(() => createClient(fetch, { origin: "http://jobs.internal:8080" }))
      .toThrow("configuration");
    expect(() => createClient(fetch, { origin: "http://example.com" }))
      .toThrow("configuration");

    for (const origin of ["http://localhost:8080", "http://127.0.0.1:8080", "http://[::1]:8080"]) {
      expect(() => createClient(fetch, { origin })).not.toThrow();
    }
    expect(() => createClient(fetch, { origin: "https://jobs.internal" })).not.toThrow();
  });
});

function createClient(
  fetch: typeof globalThis.fetch,
  overrides: Partial<ConstructorParameters<typeof ExecutionLeaseClient>[0]> = {},
): ExecutionLeaseClient {
  return new ExecutionLeaseClient({
    origin: "https://jobs-api.example",
    workerSigningKey: "0123456789abcdef0123456789abcdef",
    ownerId: "runner-test-1",
    heartbeatIntervalMs: 60_000,
    fetch,
    ...overrides,
  });
}

function grantResponse(): Response {
  return new Response(JSON.stringify({
    run_id: "run-123",
    lease_token: "lease-secret-value",
    fence: 7,
    phase: "prepared",
  }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function recordResponse(phase: "prepared" | "click_started"): Response {
  return new Response(JSON.stringify({ run_id: "run-123", fence: 7, phase }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}
