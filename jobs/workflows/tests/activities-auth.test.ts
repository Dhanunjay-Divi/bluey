import { createHash, createHmac } from "node:crypto";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const WORKER_SIGNING_KEY = "workflow-signing-key-0123456789abcdef";
const WORKER_ID = "workflow-worker-test";

interface FetchCall {
  url: string;
  init?: RequestInit;
}

beforeEach(() => {
  vi.resetModules();
  vi.stubEnv("BLUEY_JOBS_API_ORIGIN", "https://jobs-api.example");
  vi.stubEnv("BLUEY_JOBS_WORKER_SIGNING_KEY", WORKER_SIGNING_KEY);
  vi.stubEnv("BLUEY_JOBS_WORKFLOW_WORKER_ID", WORKER_ID);
  vi.stubEnv("BLUEY_JOBS_RUNNER_ORIGIN", "https://jobs-runner.example");
  vi.stubEnv("BLUEY_JOBS_RUNNER_TOKEN", "runner-service-token");
});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("workflow activity worker authentication", () => {
  it("signs state, intervention, event, and receipt requests with their exact scopes", async () => {
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(input), init });
      if (String(input).endsWith("/interventions")) {
        return new Response(JSON.stringify({ id: "intervention-1" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");
    const input = workflowInput();

    await activities.recordState(input, "running");
    await expect(activities.createIntervention(input, { status: "needs_input", issues: [] }))
      .resolves.toBe("intervention-1");
    await activities.assertEntitlement(input);
    await activities.persistReceipt({
      ...input,
      receiptBundle: { receiptId: "receipt-1" },
      evidenceObjects: [],
    } as unknown as Parameters<typeof activities.persistReceipt>[0]);

    expect(calls).toHaveLength(5);
    expectSignedActivityRequest(calls[0], "application-state");
    expectSignedActivityRequest(calls[1], "intervention");
    expectSignedActivityRequest(calls[2], "run-events");
    expectSignedActivityRequest(calls[3], "receipt");
    expectSignedActivityRequest(calls[4], "run-events");
    expect(new Set(calls.map((call) => new Headers(call.init?.headers)
      .get("x-bluey-jobs-worker-nonce"))).size).toBe(calls.length);
  });

  it("preserves runner bearer auth while signing the Jobs API event", async () => {
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(input), init });
      if (String(input) === "https://jobs-runner.example/runs") {
        return new Response(JSON.stringify({ receipt: { status: "failed", issues: [] } }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await activities.runApplication({ ...workflowInput(), browserSessionId: "browser-1" });

    expectSignedActivityRequest(calls[0], "run-events");
    expect(calls[1]?.url).toBe("https://jobs-runner.example/runs");
    expect(new Headers(calls[1]?.init?.headers).get("authorization"))
      .toBe("Bearer runner-service-token");
    expect(new Headers(calls[1]?.init?.headers).get("x-bluey-jobs-worker-signature")).toBeNull();
  });
});

function workflowInput(): Parameters<
  typeof import("../src/activities.js").recordState
>[0] {
  return {
    accountId: "account-123",
    applicationId: "application-123",
    idempotencyKey: "run-123",
    runner: "cloud",
  } as unknown as Parameters<typeof import("../src/activities.js").recordState>[0];
}

function expectSignedActivityRequest(
  call: FetchCall | undefined,
  scope: string,
): void {
  expect(call).toBeDefined();
  const url = new URL(call?.url ?? "https://invalid.example");
  const headers = new Headers(call?.init?.headers);
  const body = String(call?.init?.body ?? "");
  const timestamp = headers.get("x-bluey-jobs-worker-timestamp");
  const nonce = headers.get("x-bluey-jobs-worker-nonce");
  const contentSha256 = createHash("sha256").update(body).digest("hex");
  expect(headers.get("authorization")).toBeNull();
  expect(headers.get("x-bluey-jobs-worker-id")).toBe(WORKER_ID);
  expect(headers.get("x-bluey-jobs-worker-audience")).toBe("bluey-jobs-api");
  expect(headers.get("x-bluey-jobs-worker-scope")).toBe(scope);
  expect(headers.get("x-bluey-jobs-worker-content-sha256")).toBe(contentSha256);
  expect(timestamp).toMatch(/^\d+$/);
  expect(nonce).toMatch(/^[A-Za-z0-9._:-]{24,128}$/);
  const canonical = [
    "bluey-jobs-worker-v1",
    timestamp,
    nonce,
    WORKER_ID,
    "bluey-jobs-api",
    scope,
    "POST",
    url.pathname,
    contentSha256,
  ].join("\n");
  expect(headers.get("x-bluey-jobs-worker-signature")).toBe(
    createHmac("sha256", WORKER_SIGNING_KEY).update(canonical).digest("hex"),
  );
}
