import { createHash, createHmac } from "node:crypto";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  BrowserProfileSnapshotClient,
  createBrowserProfileSnapshotClientFromEnv,
} from "../src/profile-snapshot-client.js";

const WORKER_SIGNING_KEY = "worker-signing-key-secret-0123456789abcdef";
const SNAPSHOT_BYTES = Buffer.concat([
  Buffer.from("BLUEYJP2", "ascii"),
  Buffer.from("encrypted-browser-profile", "utf8"),
]);
const CONTEXT = {
  accountId: "account-123",
  applicationId: "application-123",
  runId: "run-123",
  browserProfileId: "account-123:identity-123",
  leaseToken: "lease-secret-value",
  fence: 7,
};

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("browser profile snapshot client", () => {
  it("restores nothing from a signed 204 response", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetch = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(input), init });
      return new Response(null, { status: 204 });
    }) as typeof globalThis.fetch;

    await expect(createClient(fetch).restore(CONTEXT)).resolves.toBeUndefined();
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe(
      "https://jobs-api.example/api/jobs/internal/execution-leases/run-123/profile/restore",
    );
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({
      account_id: CONTEXT.accountId,
      application_id: CONTEXT.applicationId,
      browser_profile_id: CONTEXT.browserProfileId,
      lease_token: CONTEXT.leaseToken,
      fence: CONTEXT.fence,
    });
    expectSignedWorkerRequest(calls[0]!, "runner-test-1");
  });

  it("restores an authenticated encrypted snapshot for a replacement runner", async () => {
    const digest = sha256(SNAPSHOT_BYTES);
    const fetch = vi.fn(async () => jsonResponse({
      browser_profile_id: CONTEXT.browserProfileId,
      generation: 4,
      envelope_version: 2,
      sha256: digest,
      size_bytes: SNAPSHOT_BYTES.length,
      encrypted_snapshot_base64: SNAPSHOT_BYTES.toString("base64"),
    })) as typeof globalThis.fetch;

    await expect(createClient(fetch).restore(CONTEXT)).resolves.toEqual({
      bytes: SNAPSHOT_BYTES,
      generation: 4,
      envelopeVersion: 2,
    });
  });

  it("stores a snapshot with generation compare-and-swap metadata", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const digest = sha256(SNAPSHOT_BYTES);
    const fetch = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(input), init });
      return jsonResponse({
        browser_profile_id: CONTEXT.browserProfileId,
        generation: 5,
        envelope_version: 2,
        sha256: digest,
        size_bytes: SNAPSHOT_BYTES.length,
      });
    }) as typeof globalThis.fetch;

    await expect(createClient(fetch).store(CONTEXT, {
      bytes: SNAPSHOT_BYTES,
      generation: 4,
      envelopeVersion: 2,
    })).resolves.toEqual({
      generation: 5,
      envelopeVersion: 2,
      sha256: digest,
      sizeBytes: SNAPSHOT_BYTES.length,
    });

    const body = JSON.parse(String(calls[0]?.init?.body));
    expect(body).toEqual({
      account_id: CONTEXT.accountId,
      application_id: CONTEXT.applicationId,
      browser_profile_id: CONTEXT.browserProfileId,
      lease_token: CONTEXT.leaseToken,
      fence: CONTEXT.fence,
      expected_generation: 4,
      envelope_version: 2,
      sha256: digest,
      size_bytes: SNAPSHOT_BYTES.length,
      encrypted_snapshot_base64: SNAPSHOT_BYTES.toString("base64"),
    });
    expectSignedWorkerRequest(calls[0]!, "runner-test-1");
  });

  it("rejects corrupt, cross-profile, and stale-generation responses", async () => {
    const digest = sha256(SNAPSHOT_BYTES);
    const corrupt = createClient(vi.fn(async () => jsonResponse({
      browser_profile_id: CONTEXT.browserProfileId,
      generation: 4,
      envelope_version: 2,
      sha256: digest.replace(/^./, digest.startsWith("a") ? "b" : "a"),
      size_bytes: SNAPSHOT_BYTES.length,
      encrypted_snapshot_base64: SNAPSHOT_BYTES.toString("base64"),
    })) as typeof globalThis.fetch);
    await expect(corrupt.restore(CONTEXT)).rejects.toMatchObject({ code: "invalid_response" });

    const crossProfile = createClient(vi.fn(async () => jsonResponse({
      browser_profile_id: "account-else:identity-else",
      generation: 4,
      envelope_version: 2,
      sha256: digest,
      size_bytes: SNAPSHOT_BYTES.length,
      encrypted_snapshot_base64: SNAPSHOT_BYTES.toString("base64"),
    })) as typeof globalThis.fetch);
    await expect(crossProfile.restore(CONTEXT)).rejects.toMatchObject({
      code: "invalid_response",
    });

    const staleStore = createClient(vi.fn(async () => jsonResponse({
      browser_profile_id: CONTEXT.browserProfileId,
      generation: 4,
      envelope_version: 2,
      sha256: digest,
      size_bytes: SNAPSHOT_BYTES.length,
    })) as typeof globalThis.fetch);
    await expect(staleStore.store(CONTEXT, {
      bytes: SNAPSHOT_BYTES,
      generation: 4,
      envelopeVersion: 2,
    })).rejects.toMatchObject({ code: "invalid_response" });
  });

  it("blocks redirects, oversized responses, and stalled requests", async () => {
    const redirected = createClient(vi.fn(async () => new Response("", {
      status: 302,
      headers: { Location: "https://elsewhere.example/profile" },
    })) as typeof globalThis.fetch);
    await expect(redirected.restore(CONTEXT)).rejects.toMatchObject({
      code: "redirect_blocked",
    });

    const oversized = createClient(vi.fn(async () => new Response("{}", {
      status: 200,
      headers: { "Content-Length": String(36 * 1024 * 1024 + 1) },
    })) as typeof globalThis.fetch);
    await expect(oversized.restore(CONTEXT)).rejects.toMatchObject({
      code: "response_too_large",
    });

    vi.useFakeTimers();
    const stalledFetch = vi.fn((_input: string | URL | Request, init?: RequestInit) => (
      new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(new Error("aborted")), { once: true });
      })
    )) as typeof globalThis.fetch;
    const stalled = createClient(stalledFetch, { requestTimeoutMs: 100 }).restore(CONTEXT);
    const expectation = expect(stalled).rejects.toMatchObject({ code: "timed_out" });
    await vi.advanceTimersByTimeAsync(100);
    await expectation;
  });

  it("keeps lease and worker secrets out of failures", async () => {
    const fetch = vi.fn(async () => jsonResponse({
      error: `${WORKER_SIGNING_KEY} ${CONTEXT.leaseToken} https://private.example`,
    }, 409)) as typeof globalThis.fetch;

    const error = await createClient(fetch).restore(CONTEXT).catch((caught: unknown) => caught);

    expect(error).toMatchObject({ code: "request_failed", status: 409 });
    expect(String(error)).not.toContain(WORKER_SIGNING_KEY);
    expect(String(error)).not.toContain(CONTEXT.leaseToken);
    expect(String(error)).not.toContain("private.example");
  });

  it("requires signed worker auth and permits plaintext only on loopback", () => {
    expect(() => createBrowserProfileSnapshotClientFromEnv({
      BLUEY_JOBS_API_ORIGIN: "https://jobs.internal",
      BLUEY_JOBS_WORKER_TOKEN: WORKER_SIGNING_KEY,
      BLUEY_JOBS_RUNNER_ID: "runner-env-test",
    })).toThrow("configuration");
    expect(() => createBrowserProfileSnapshotClientFromEnv({
      BLUEY_JOBS_API_ORIGIN: "https://jobs.internal",
      BLUEY_JOBS_WORKER_SIGNING_KEY: WORKER_SIGNING_KEY,
      BLUEY_JOBS_RUNNER_ID: "runner-env-test",
    })).not.toThrow();
    expect(() => createClient(vi.fn() as typeof globalThis.fetch, {
      origin: "http://jobs.internal:8080",
    })).toThrow("configuration");
    expect(() => createClient(vi.fn() as typeof globalThis.fetch, {
      origin: "http://127.0.0.1:8080",
    })).not.toThrow();
  });
});

function createClient(
  fetch: typeof globalThis.fetch,
  overrides: Partial<ConstructorParameters<typeof BrowserProfileSnapshotClient>[0]> = {},
): BrowserProfileSnapshotClient {
  return new BrowserProfileSnapshotClient({
    origin: "https://jobs-api.example",
    workerSigningKey: WORKER_SIGNING_KEY,
    ownerId: "runner-test-1",
    fetch,
    ...overrides,
  });
}

function expectSignedWorkerRequest(
  call: { url: string; init?: RequestInit },
  workerId: string,
): void {
  const url = new URL(call.url);
  const headers = new Headers(call.init?.headers);
  const body = String(call.init?.body ?? "");
  const timestamp = headers.get("x-bluey-jobs-worker-timestamp");
  const nonce = headers.get("x-bluey-jobs-worker-nonce");
  const contentSha256 = sha256(body);
  expect(headers.get("x-bluey-jobs-worker-id")).toBe(workerId);
  expect(headers.get("x-bluey-jobs-worker-audience")).toBe("bluey-jobs-api");
  expect(headers.get("x-bluey-jobs-worker-scope")).toBe("execution");
  expect(headers.get("x-bluey-jobs-worker-content-sha256")).toBe(contentSha256);
  expect(timestamp).toMatch(/^\d+$/);
  expect(nonce).toMatch(/^[A-Za-z0-9._:-]{24,128}$/);
  const canonical = [
    "bluey-jobs-worker-v1",
    timestamp,
    nonce,
    workerId,
    "bluey-jobs-api",
    "execution",
    "POST",
    url.pathname,
    contentSha256,
  ].join("\n");
  expect(headers.get("x-bluey-jobs-worker-signature")).toBe(
    createHmac("sha256", WORKER_SIGNING_KEY).update(canonical).digest("hex"),
  );
}

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function sha256(value: string | Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}
