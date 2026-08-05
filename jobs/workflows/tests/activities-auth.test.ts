import { createHash, createHmac } from "node:crypto";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const WORKER_SIGNING_KEY = "workflow-signing-key-0123456789abcdef";
const WORKER_ID = "workflow-worker-test";
const LEASE_TOKEN = "a".repeat(43);

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
  it("signs state, intervention, and event requests with their exact scopes", async () => {
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

    expect(calls).toHaveLength(3);
    expectSignedActivityRequest(calls[0], "application-state");
    expectSignedActivityRequest(calls[1], "intervention");
    expectSignedActivityRequest(calls[2], "run-events");
    expect(new Set(calls.map((call) => new Headers(call.init?.headers)
      .get("x-bluey-jobs-worker-nonce"))).size).toBe(calls.length);
  });

  it("uses runner bearer auth for recovery and execution while signing API events", async () => {
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(input), init });
      if (String(input) === "https://jobs-runner.example/runs") {
        return jsonResponse({ receipt: { status: "failed", issues: [] } });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await activities.runApplication({ ...workflowInput(), browserSessionId: "browser-1" });

    expect(calls.map((call) => call.url)).toEqual([
      "https://jobs-runner.example/results",
      "https://jobs-api.example/api/jobs/internal/runs/run-123/events",
      "https://jobs-runner.example/runs",
    ]);
    expectRunnerRequest(calls[0]);
    expect(JSON.parse(String(calls[0]?.init?.body))).toMatchObject({
      accountId: "account-123",
      applicationId: "application-123",
      applicationIdentityId: "identity-123",
      browserSessionId: "browser-1",
      runId: "run-123",
      requestId: "run-123:initial",
    });
    expectSignedActivityRequest(calls[1], "run-events");
    expectRunnerRequest(calls[2]);
  });

  it("accepts only exact-submit-success HTTP statuses at the workflow boundary", async () => {
    let submitHttpStatus = 200;
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/runs") {
        return jsonResponse(submittedRunnerResult({
          receipt: { status: "submitted", submitHttpStatus, issues: [] },
        }));
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");
    const acceptedRedirectStatuses = new Set([301, 302, 303, 307, 308]);

    for (submitHttpStatus = 100; submitHttpStatus <= 599; submitHttpStatus += 1) {
      const execution = activities.runApplication({
        ...workflowInput(),
        browserSessionId: "browser-1",
      });
      const accepted = (submitHttpStatus >= 200 && submitHttpStatus <= 299)
        || acceptedRedirectStatuses.has(submitHttpStatus);
      if (accepted) {
        await expect(execution).resolves.toEqual({
          receipt: { status: "submitted", submitHttpStatus, issues: [] },
        });
      } else {
        await expect(execution).rejects.toThrow("Bluey Jobs runner returned an invalid receipt");
      }
    }
  });

  it.each([
    "BLUEY_JOBS_API_ORIGIN",
    "BLUEY_JOBS_RUNNER_ORIGIN",
  ])("rejects cleartext non-loopback %s before sending credentials", async (name) => {
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    vi.stubEnv(name, "http://jobs-internal.example");

    await expect(import("../src/activities.js"))
      .rejects.toThrow("must be an HTTPS origin or a loopback HTTP origin");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("keeps submitted evidence behind the activity boundary and persists it separately", async () => {
    const calls: FetchCall[] = [];
    const rawResult = submittedRunnerResult({
      receipt: {
        status: "submitted",
        submitHttpStatus: 200,
        issues: [],
        screenshotPath: "/Users/bluey/private/final.png",
        debug: { token: "nested-receipt-secret" },
      },
    });
    let committed = false;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      calls.push({ url: String(input), init });
      if (String(input) === "https://jobs-runner.example/results") {
        return committed ? jsonResponse(rawResult) : new Response(null, { status: 204 });
      }
      if (String(input) === "https://jobs-runner.example/runs") {
        committed = true;
        return jsonResponse(rawResult);
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    const result = await activities.runApplication({
      ...workflowInput(),
      browserSessionId: "browser-1",
    });
    await activities.persistSubmissionReceipt({
      ...workflowInput(),
      browserSessionId: "browser-1",
      resultRequestId: "run-123:initial",
    });

    expect(result).toEqual({
      receipt: { status: "submitted", submitHttpStatus: 200, issues: [] },
    });
    const serializedResult = JSON.stringify(result);
    expect(serializedResult).not.toContain(LEASE_TOKEN);
    expect(serializedResult).not.toContain("bundle-secret");
    expect(serializedResult).not.toContain("evidence-secret");
    expect(serializedResult).not.toContain("/Users/");
    expect(serializedResult).not.toContain("nested-receipt-secret");
    expect(calls.filter((call) => call.url === "https://jobs-runner.example/runs"))
      .toHaveLength(1);
    expectSignedActivityRequest(calls.at(-1), "receipt");
    const receiptBody = JSON.parse(String(calls.at(-1)?.init?.body)) as Record<string, unknown>;
    expect(receiptBody).toEqual(expect.objectContaining({
      account_id: "account-123",
      lease_token: LEASE_TOKEN,
      fence: 9,
      receipt: expect.objectContaining({ receiptId: "receipt-run-123" }),
    }));
  });

  it.each([
    {
      label: "submitted",
      receipt: {
        status: "submitted",
        submitHttpStatus: 200,
        confirmationText: "Application received",
        confirmationUrl: "https://boards.example/confirmation/123",
        submittedAt: "2026-08-04T03:00:00.000Z",
        screenshotPath: "/Users/bluey/private/final.png",
        token: "receipt-token-secret",
        rawBytes: [115, 101, 99, 114, 101, 116],
        issues: [{
          field: "submission",
          message: "Confirmed",
          severity: "warning",
          path: "/tmp/nested-issue.json",
          token: "issue-token-secret",
        }],
      },
      expected: {
        status: "submitted",
        submitHttpStatus: 200,
        confirmationText: "Application received",
        confirmationUrl: "https://boards.example/confirmation/123",
        submittedAt: "2026-08-04T03:00:00.000Z",
        issues: [{ field: "submission", message: "Confirmed", severity: "warning" }],
      },
    },
    {
      label: "failed",
      receipt: {
        status: "failed",
        screenshotPath: "C:\\Users\\bluey\\receipt.png",
        token: "receipt-token-secret",
        issues: [{
          field: "submission",
          message: "Employer rejected the form",
          severity: "blocking",
          debug: { path: "/private/rejection.json", bytes_base64: "raw-byte-secret" },
        }],
      },
      expected: {
        status: "failed",
        issues: [{
          field: "submission",
          message: "Employer rejected the form",
          severity: "blocking",
        }],
      },
    },
    {
      label: "needs_input",
      receipt: {
        status: "needs_input",
        screenshotPath: "/tmp/final.png",
        issues: [],
        intervention: {
          kind: "unknown_question",
          title: "Answer required",
          detail: "The employer requires an answer.",
          field: "salary",
          choices: ["Yes", "No"],
          takeoverUrl: "https://takeover.example/sessions/browser-1",
          screenshotPath: "/private/nested.png",
          token: "intervention-token-secret",
          resolution: {
            kind: "answer",
            resumeAfter: true,
            path: "/private/resolution.json",
            bytes_base64: "resolution-byte-secret",
          },
        },
      },
      expected: {
        status: "needs_input",
        issues: [],
        intervention: {
          kind: "unknown_question",
          title: "Answer required",
          detail: "The employer requires an answer.",
          field: "salary",
          choices: ["Yes", "No"],
          takeoverUrl: "https://takeover.example/sessions/browser-1",
          resolution: { kind: "answer", resumeAfter: true },
        },
      },
    },
  ])("deep-allowlists the $label receipt before Temporal can record it", async ({
    receipt,
    expected,
  }) => {
    const rawResult = receipt.status === "submitted"
      ? submittedRunnerResult({ receipt })
      : { receipt };
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/runs") return jsonResponse(rawResult);
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    const result = await activities.runApplication({
      ...workflowInput(),
      browserSessionId: "browser-1",
    });

    expect(result).toEqual({ receipt: expected });
    const historyPayload = JSON.stringify(result);
    for (const secret of [
      "/Users/",
      "C:\\Users",
      "/tmp/",
      "/private/",
      "token-secret",
      "byte-secret",
    ]) {
      expect(historyPayload).not.toContain(secret);
    }
  });

  it.each([
    ["missing", undefined],
    ["short token", { leaseToken: "short", fence: 9 }],
    ["unsafe fence", { leaseToken: LEASE_TOKEN, fence: Number.MAX_SAFE_INTEGER + 1 }],
    ["unexpected field", { leaseToken: LEASE_TOKEN, fence: 9, ownerId: "runner-one" }],
  ])("rejects %s submitted receipt authority", async (_label, receiptAuthority) => {
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/runs") {
        return new Response(JSON.stringify({
          receipt: { status: "submitted", submitHttpStatus: 200, issues: [] },
          receiptBundle: { receiptId: "receipt-run-123" },
          evidenceObjects: [{ kind: "screenshot" }],
          ...(receiptAuthority === undefined ? {} : { receiptAuthority }),
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.runApplication({
      ...workflowInput(),
      browserSessionId: "browser-1",
    })).rejects.toThrow("missing fenced receipt authority");
  });

  it("rejects receipt authority on a non-submitted runner result", async () => {
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/runs") {
        return new Response(JSON.stringify({
          receipt: { status: "failed", issues: [] },
          receiptAuthority: { leaseToken: LEASE_TOKEN, fence: 9 },
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.runApplication({
      ...workflowInput(),
      browserSessionId: "browser-1",
    })).rejects.toThrow("Non-submitted runner result included receipt authority");
  });

  it("recovers and persists a resumed submission without exposing its authority", async () => {
    const calls: FetchCall[] = [];
    const rawResult = submittedRunnerResult({
      receiptAuthority: { leaseToken: LEASE_TOKEN, fence: 12 },
    });
    let committed = false;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      calls.push({ url: String(input), init });
      if (String(input) === "https://jobs-runner.example/results") {
        return committed ? jsonResponse(rawResult) : new Response(null, { status: 204 });
      }
      if (String(input).includes("/runs/browser-1/resume")) {
        committed = true;
        return jsonResponse(rawResult);
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    const result = await activities.resumeApplication({
      ...workflowInput(),
      browserSessionId: "browser-1",
      requestId: "run-123:resume:1",
      resolution: { action: "approve_submission" },
    });
    await activities.persistSubmissionReceipt({
      ...workflowInput(),
      browserSessionId: "browser-1",
      resultRequestId: "run-123:resume:1",
    });

    expect(result).toEqual({
      receipt: { status: "submitted", submitHttpStatus: 200, issues: [] },
    });
    expect(JSON.stringify(result)).not.toContain(LEASE_TOKEN);
    const resumeCall = calls.find((call) => call.url.includes("/runs/browser-1/resume"));
    expect(JSON.parse(String(resumeCall?.init?.body))).toMatchObject({
      accountId: "account-123",
      applicationId: "application-123",
      applicationIdentityId: "identity-123",
      runId: "run-123",
      requestId: "run-123:resume:1",
    });
    expectSignedActivityRequest(calls.at(-1), "receipt");
    expect(JSON.parse(String(calls.at(-1)?.init?.body))).toEqual(expect.objectContaining({
      lease_token: LEASE_TOKEN,
      fence: 12,
    }));
  });

  it("applies the same fenced-authority validation to resumed runs", async () => {
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input).includes("/runs/browser-1/resume")) {
        return new Response(JSON.stringify({
          receipt: { status: "submitted", submitHttpStatus: 200, issues: [] },
          receiptBundle: { receiptId: "receipt-run-123" },
          evidenceObjects: [{ kind: "screenshot" }],
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.resumeApplication({
      ...workflowInput(),
      browserSessionId: "browser-1",
      requestId: "run-123:resume:1",
      resolution: { action: "approve_submission" },
    })).rejects.toThrow("missing fenced receipt authority");
  });

  it("does not expose receipt authority when canonical persistence fails", async () => {
    const rawResult = submittedRunnerResult({
      receiptAuthority: { leaseToken: LEASE_TOKEN, fence: 12 },
    });
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/results") return jsonResponse(rawResult);
      if (String(input).endsWith("/receipt")) {
        return new Response(null, { status: 503 });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    let error: unknown;
    try {
      await activities.persistSubmissionReceipt({
        ...workflowInput(),
        browserSessionId: "browser-1",
        resultRequestId: "run-123:initial",
      });
    } catch (caught) {
      error = caught;
    }

    expect(error).toBeInstanceOf(Error);
    expect(String(error)).toContain("Jobs API returned 503");
    expect(String(error)).not.toContain(LEASE_TOKEN);
  });

  it("recovers a committed runner result after the execute response is lost", async () => {
    const rawResult = submittedRunnerResult();
    let committed = false;
    let executionCalls = 0;
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/results") {
        return committed ? jsonResponse(rawResult) : new Response(null, { status: 204 });
      }
      if (String(input) === "https://jobs-runner.example/runs") {
        executionCalls += 1;
        committed = true;
        throw new TypeError("connection closed after durable commit");
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");
    const input = { ...workflowInput(), browserSessionId: "browser-1" };

    await expect(activities.runApplication(input)).rejects.toThrow("durable commit");
    await expect(activities.runApplication(input)).resolves.toEqual({
      receipt: { status: "submitted", submitHttpStatus: 200, issues: [] },
    });

    expect(executionCalls).toBe(1);
  });

  it("exactly replays canonical persistence after a committed response is lost", async () => {
    const rawResult = submittedRunnerResult();
    const receiptBodies: string[] = [];
    let persistenceCalls = 0;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      if (url === "https://jobs-runner.example/results") return jsonResponse(rawResult);
      if (url.endsWith("/receipt")) {
        persistenceCalls += 1;
        receiptBodies.push(String(init?.body));
        if (persistenceCalls === 1) {
          throw new TypeError("response lost after canonical commit");
        }
        return new Response(null, { status: 204 });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");
    const input = {
      ...workflowInput(),
      browserSessionId: "browser-1",
      resultRequestId: "run-123:initial",
    };

    await expect(activities.persistSubmissionReceipt(input)).rejects.toThrow("canonical commit");
    await expect(activities.persistSubmissionReceipt(input)).resolves.toBeUndefined();

    expect(persistenceCalls).toBe(2);
    expect(receiptBodies[1]).toBe(receiptBodies[0]);
  });
});

function workflowInput(): Parameters<
  typeof import("../src/activities.js").recordState
>[0] {
  return {
    accountId: "account-123",
    applicationId: "application-123",
    applicationIdentityId: "identity-123",
    idempotencyKey: "run-123",
    runner: "cloud",
  } as unknown as Parameters<typeof import("../src/activities.js").recordState>[0];
}

function submittedRunnerResult(
  override: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    receipt: { status: "submitted", submitHttpStatus: 200, issues: [] },
    receiptBundle: { receiptId: "receipt-run-123", secretMarker: "bundle-secret" },
    evidenceObjects: [{
      original_key: "/private/final.png",
      kind: "screenshot",
      media_type: "image/png",
      sha256: "b".repeat(64),
      bytes_base64: "evidence-secret",
    }],
    receiptAuthority: { leaseToken: LEASE_TOKEN, fence: 9 },
    receiptPath: "/private/receipt.json",
    ...override,
  };
}

function jsonResponse(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function expectRunnerRequest(call: FetchCall | undefined): void {
  expect(call).toBeDefined();
  const headers = new Headers(call?.init?.headers);
  expect(headers.get("authorization")).toBe("Bearer runner-service-token");
  expect(headers.get("x-bluey-jobs-worker-signature")).toBeNull();
  expect(call?.init?.redirect).toBe("error");
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
  expect(call?.init?.redirect).toBe("error");
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
