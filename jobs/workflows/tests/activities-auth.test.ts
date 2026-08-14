import { createHash, createHmac } from "node:crypto";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  approvedExecutionChecksum,
  type ApplicationPacket,
  type NormalizedJob,
} from "@bluey/jobs-automation";

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
  it("materializes and consumes a v2 start without returning private command data", async () => {
    const calls: FetchCall[] = [];
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") return runnerResultNotFoundResponse();
      if (url === "https://jobs-runner.example/runs") {
        return jsonResponse(runnerResult(
          { status: "failed", issues: [] },
          "cloud-application-123",
          authority.requestId,
        ));
      }
      if (url.endsWith(`/${authority.requestId}/finalize`)) {
        return jsonResponse(finalizationEcho(authority, "failed", "runner_failed"));
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    const result = await activities.executeApplicationCommand(authority);

    expect(result).toEqual({ state: "failed" });
    expect(JSON.stringify(result)).not.toContain("account-123");
    expect(JSON.stringify(result)).not.toContain("private-answer");
    const materialize = calls[0];
    expect(materialize?.url).toBe(
      `https://jobs-api.example/api/jobs/internal/workflow-commands/${authority.requestId}/materialize`,
    );
    expectSignedActivityRequest(materialize, "workflow-command-materialize");
    expect(JSON.parse(String(materialize?.init?.body))).toEqual({
      schema_version: 2,
      workflow_id: authority.workflowId,
      payload_digest: authority.payloadDigest,
      operation: "start",
    });
    const runnerStart = calls.find((call) => call.url === "https://jobs-runner.example/runs");
    expect(Object.keys(JSON.parse(String(runnerStart?.init?.body)) as Record<string, unknown>).sort())
      .toEqual([
        "accountId",
        "applicationId",
        "applicationIdentityId",
        "browserProfileId",
        "browserSessionId",
        "job",
        "packet",
        "requestId",
        "runId",
        "url",
      ]);
    expect(JSON.parse(String(runnerStart?.init?.body))).toMatchObject({
      accountId: "account-123",
      applicationId: "application-123",
      applicationIdentityId: "identity-123",
      browserProfileId: "profile-123",
      browserSessionId: "cloud-application-123",
      runId: "run-123",
      requestId: authority.requestId,
    });
  });

  it("prepares then publishes an exact resume intervention with only opaque history values", async () => {
    const calls: FetchCall[] = [];
    const workflow = opaqueAuthority();
    const command = {
      schemaVersion: 2 as const,
      requestId: `wfreq-v2-${"d".repeat(32)}`,
      workflowId: workflow.workflowId,
      payloadDigest: "e".repeat(64),
      interventionId: `intervention-${"f".repeat(32)}`,
    };
    const nextIntervention = `intervention-${"9".repeat(32)}`;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${command.requestId}/materialize`)) {
        return jsonResponse({
          ...materializedStart(command),
          operation: "resume",
          intervention_id: command.interventionId,
          resolution: { action: "approve_submission" },
        });
      }
      if (url === "https://jobs-runner.example/results") return runnerResultNotFoundResponse();
      if (url.includes("/runs/cloud-application-123/resume")) {
        return jsonResponse(runnerResult(
          {
            status: "needs_input",
            issues: [],
            intervention: {
              kind: "browser_takeover",
              title: "Continue",
              detail: "Continue in the browser.",
            },
          },
          "cloud-application-123",
          command.requestId,
        ));
      }
      if (url.endsWith(`/${command.requestId}/intervention/prepare`)) {
        return jsonResponse(interventionEcho(command, nextIntervention));
      }
      if (url.endsWith(`/intervention/${nextIntervention}/publish`)) {
        return jsonResponse(interventionEcho(command, nextIntervention));
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    const result = await activities.resumeApplicationCommand({ workflow, command });

    expect(result).toEqual({ state: "intervention_prepared", interventionId: nextIntervention });
    expect(JSON.stringify(result)).not.toContain("approve_submission");
    const materialize = calls[0];
    expectSignedActivityRequest(materialize, "workflow-command-materialize");
    expect(JSON.parse(String(materialize?.init?.body))).toEqual({
      schema_version: 2,
      workflow_id: command.workflowId,
      payload_digest: command.payloadDigest,
      operation: "resume",
      intervention_id: command.interventionId,
    });
    const runnerResume = calls.find((call) => call.url.includes("/resume"));
    expect(Object.keys(JSON.parse(String(runnerResume?.init?.body)) as Record<string, unknown>).sort())
      .toEqual([
        "accountId",
        "action",
        "applicationId",
        "applicationIdentityId",
        "profileScope",
        "requestId",
        "runId",
      ]);
    expect(JSON.parse(String(runnerResume?.init?.body))).toMatchObject({
      action: "approve_submission",
      requestId: command.requestId,
    });
    const prepare = calls.find((call) => call.url.endsWith("/intervention/prepare"));
    expectSignedActivityRequest(prepare, "workflow-command-execution");
    expect(JSON.parse(String(prepare?.init?.body))).toMatchObject({
      schema_version: 2,
      workflow_id: command.workflowId,
      payload_digest: command.payloadDigest,
      operation: "resume",
      intervention_id: command.interventionId,
      receipt: { status: "needs_input" },
    });

    await expect(activities.publishApplicationIntervention({
      command,
      interventionId: nextIntervention,
    })).resolves.toEqual({ state: "needs_input", interventionId: nextIntervention });
    const publish = calls.find((call) => call.url.endsWith(`/${nextIntervention}/publish`));
    expectSignedActivityRequest(publish, "workflow-command-execution");
    expect(JSON.parse(String(publish?.init?.body))).toEqual({
      schema_version: 2,
      workflow_id: command.workflowId,
      payload_digest: command.payloadDigest,
      operation: "resume",
      intervention_id: command.interventionId,
    });
  });

  it("rejects a materialize echo mismatch before any runner request", async () => {
    const authority = opaqueAuthority();
    const fetch = vi.fn(async (input: string | URL | Request) => {
      if (String(input).endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse({
          ...materializedStart(authority),
          payload_digest: "0".repeat(64),
        });
      }
      return new Response(null, { status: 204 });
    });
    vi.stubGlobal("fetch", fetch);
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority)).rejects.toMatchObject({
      message: "opaque_failure",
      type: "identity_conflict",
      nonRetryable: true,
    });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("lets generic runner failures escape for exact Temporal activity retry", async () => {
    const authority = opaqueAuthority();
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") return runnerResultNotFoundResponse();
      if (url === "https://jobs-runner.example/runs") return new Response(null, { status: 503 });
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("runner transient failure");

    expect(calls.filter((call) => call.url.endsWith("/finalize"))).toHaveLength(0);
    const stateBodies = calls
      .filter((call) => call.url.endsWith("/state"))
      .map((call) => JSON.parse(String(call.init?.body)) as unknown);
    expect(stateBodies).toEqual([{ account_id: "account-123", state: "running" }]);
  });

  it("returns only a closed side-effect-unknown step for the exact runner contract", async () => {
    const authority = opaqueAuthority();
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        return jsonResponse({
          schemaVersion: 2,
          outcome: "side_effect_unknown",
          requestId: authority.requestId,
        }, 500);
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority)).resolves.toEqual({
      state: "side_effect_unknown",
    });
    expect(calls.filter((call) => call.url.endsWith("/finalize"))).toHaveLength(0);
    expect(JSON.stringify(calls.filter((call) => call.url.endsWith("/finalize"))))
      .not.toContain("account-123");
  });

  it("recovers an exact durable ambiguity after the runner response is lost", async () => {
    const authority = opaqueAuthority();
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return jsonResponse({
          schemaVersion: 2,
          outcome: "side_effect_unknown",
          requestId: authority.requestId,
        }, 500);
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority)).resolves.toEqual({
      state: "side_effect_unknown",
    });
    expect(calls.filter((call) => call.url === "https://jobs-runner.example/runs"))
      .toHaveLength(0);
    expect(calls.filter((call) => call.url.endsWith("/state"))).toHaveLength(0);
  });

  it("classifies the exact runner ambiguity contract after a resume", async () => {
    const workflow = opaqueAuthority();
    const command = {
      schemaVersion: 2 as const,
      requestId: `wfreq-v2-${"d".repeat(32)}`,
      workflowId: workflow.workflowId,
      payloadDigest: "e".repeat(64),
      interventionId: `intervention-${"f".repeat(32)}`,
    };
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith(`/${command.requestId}/materialize`)) {
        return jsonResponse({
          ...materializedStart(command),
          operation: "resume",
          intervention_id: command.interventionId,
          resolution: { action: "approve_submission" },
        });
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url.endsWith("/resume")) {
        return jsonResponse({
          schemaVersion: 2,
          outcome: "side_effect_unknown",
          requestId: command.requestId,
        }, 500);
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.resumeApplicationCommand({ workflow, command })).resolves.toEqual({
      state: "side_effect_unknown",
    });
  });

  it.each([
    ["wrong status", 503, "application/json", true, true, {
      schemaVersion: 2,
      outcome: "side_effect_unknown",
      requestId: opaqueAuthority().requestId,
    }],
    ["charset content type", 500, "application/json; charset=utf-8", true, true, {
      schemaVersion: 2,
      outcome: "side_effect_unknown",
      requestId: opaqueAuthority().requestId,
    }],
    ["missing no-store", 500, "application/json", false, true, {
      schemaVersion: 2,
      outcome: "side_effect_unknown",
      requestId: opaqueAuthority().requestId,
    }],
    ["missing nosniff", 500, "application/json", true, false, {
      schemaVersion: 2,
      outcome: "side_effect_unknown",
      requestId: opaqueAuthority().requestId,
    }],
    ["mismatched request", 500, "application/json", true, true, {
      schemaVersion: 2,
      outcome: "side_effect_unknown",
      requestId: `wfreq-v2-${"0".repeat(32)}`,
    }],
    ["extra field", 500, "application/json", true, true, {
      schemaVersion: 2,
      outcome: "side_effect_unknown",
      requestId: opaqueAuthority().requestId,
      error: "private detail",
    }],
    ["wrong outcome", 500, "application/json", true, true, {
      schemaVersion: 2,
      outcome: "runner_failed",
      requestId: opaqueAuthority().requestId,
    }],
    ["wrong schema", 500, "application/json", true, true, {
      schemaVersion: 1,
      outcome: "side_effect_unknown",
      requestId: opaqueAuthority().requestId,
    }],
    ["missing request", 500, "application/json", true, true, {
      schemaVersion: 2,
      outcome: "side_effect_unknown",
    }],
  ] as const)("keeps a %s ambiguity lookalike retryable", async (
    _label,
    status,
    contentType,
    noStore,
    nosniff,
    value,
  ) => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        return new Response(JSON.stringify(value), {
          status,
          headers: {
            "Content-Type": contentType,
            ...(noStore ? { "Cache-Control": "no-store" } : {}),
            ...(nosniff ? { "X-Content-Type-Options": "nosniff" } : {}),
          },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("runner transient failure");
  });

  it("keeps malformed JSON with ambiguity headers retryable", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        return new Response("{", {
          status: 500,
          headers: {
            "Cache-Control": "no-store",
            "Content-Type": "application/json",
            "X-Content-Type-Options": "nosniff",
          },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("runner transient failure");
  });

  it.each([400, 404, 409, 422])(
    "keeps status-only runner %s retryable without false terminal authority",
    async (status) => {
      const authority = opaqueAuthority();
      vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
        const url = String(input);
        if (url.endsWith(`/${authority.requestId}/materialize`)) {
          return jsonResponse(materializedStart(authority));
        }
        if (url === "https://jobs-runner.example/results") {
          return runnerResultNotFoundResponse();
        }
        if (url === "https://jobs-runner.example/runs") {
          return new Response("private upstream error", { status });
        }
        return new Response(null, { status: 204 });
      }));
      const activities = await import("../src/activities.js");

      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow("runner transient failure");
    },
  );

  it.each([408, 429, 500, 503])(
    "keeps runner %s retryable without exposing its response body",
    async (status) => {
      const authority = opaqueAuthority();
      vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
        const url = String(input);
        if (url.endsWith(`/${authority.requestId}/materialize`)) {
          return jsonResponse(materializedStart(authority));
        }
        if (url === "https://jobs-runner.example/results") {
          return runnerResultNotFoundResponse();
        }
        if (url === "https://jobs-runner.example/runs") {
          return new Response("private upstream error", { status });
        }
        return new Response(null, { status: 204 });
      }));
      const activities = await import("../src/activities.js");

      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow("runner transient failure");
    },
  );

  it.each([
    [204, undefined, "runner returned an invalid response"],
    [404, { "Content-Type": "text/plain" }, "runner transient failure"],
  ] as const)(
    "does not treat a status-only runner result %s as authoritative absence",
    async (status, headers, expectedError) => {
      const authority = opaqueAuthority();
      const calls: FetchCall[] = [];
      vi.stubGlobal("fetch", vi.fn(async (
        input: string | URL | Request,
        init?: RequestInit,
      ) => {
        const url = String(input);
        calls.push({ url, init });
        if (url.endsWith(`/${authority.requestId}/materialize`)) {
          return jsonResponse(materializedStart(authority));
        }
        if (url === "https://jobs-runner.example/results") {
          return new Response(status === 204 ? null : "not authoritative", { status, headers });
        }
        return new Response(null, { status: 204 });
      }));
      const activities = await import("../src/activities.js");

      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow(expectedError);
      expect(calls.some((call) => call.url === "https://jobs-runner.example/runs"))
        .toBe(false);
    },
  );

  it.each([400, 401, 409, 422])(
    "keeps status-only durable-result lookup %s retryable",
    async (status) => {
      const authority = opaqueAuthority();
      vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
        const url = String(input);
        if (url.endsWith(`/${authority.requestId}/materialize`)) {
          return jsonResponse(materializedStart(authority));
        }
        if (url === "https://jobs-runner.example/results") {
          return new Response("private lookup rejection", { status });
        }
        return new Response(null, { status: 204 });
      }));
      const activities = await import("../src/activities.js");

      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow("runner transient failure");
    },
  );

  it.each([408, 429, 500, 503])(
    "retries a transient durable-result lookup failure with runner %s",
    async (status) => {
      const authority = opaqueAuthority();
      vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
        const url = String(input);
        if (url.endsWith(`/${authority.requestId}/materialize`)) {
          return jsonResponse(materializedStart(authority));
        }
        if (url === "https://jobs-runner.example/results") {
          return new Response("private lookup failure", { status });
        }
        return new Response(null, { status: 204 });
      }));
      const activities = await import("../src/activities.js");

      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow("runner transient failure");
    },
  );

  it("retries a malformed successful runner response without exposing it", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        return new Response("private malformed response", {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("runner returned an invalid response");
  });

  it("does not accept a runner result without exact private headers", async () => {
    const authority = opaqueAuthority();
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        return new Response(JSON.stringify(runnerResult(
          { status: "failed", issues: [] },
          "cloud-application-123",
          authority.requestId,
        )), { status: 200, headers: { "Content-Type": "application/json" } });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("runner returned an invalid response");
    expect(calls.filter((call) => call.url.endsWith("/finalize"))).toHaveLength(0);
  });

  it("does not terminalize from a runner result bound to another application", async () => {
    const authority = opaqueAuthority();
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        return jsonResponse({
          ...runnerResult(
            { status: "failed", issues: [] },
            "cloud-application-123",
            authority.requestId,
          ),
          applicationId: "application-other",
        });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("runner returned an invalid response");
    expect(calls.filter((call) => call.url.endsWith("/finalize"))).toHaveLength(0);
  });

  it("does not terminalize from a same-run result bound to another request", async () => {
    const authority = opaqueAuthority();
    const calls: FetchCall[] = [];
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        return jsonResponse(runnerResult(
          { status: "failed", issues: [] },
          "cloud-application-123",
          `wfreq-v2-${"0".repeat(32)}`,
        ));
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("runner returned an invalid response");
    expect(calls.filter((call) => call.url.endsWith("/finalize"))).toHaveLength(0);
  });

  it.each([
    [400, "invalid_request"],
    [404, "not_found"],
    [409, "identity_conflict"],
  ] as const)("classifies Jobs API %s as permanent %s", async (status, type) => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async () =>
      workflowCommandErrorResponse(status)));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority)).rejects.toMatchObject({
      message: "opaque_failure",
      type,
      nonRetryable: true,
    });
  });

  it("keeps a Jobs API 503 retryable without exposing its response body", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async () =>
      jsonResponse({ outcome: "rejected" }, 503)));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("Jobs API transient failure");
  });

  it("keeps an intermediary 503 without private response headers retryable", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async () => new Response("upstream unavailable", {
      status: 503,
      headers: { "Content-Type": "text/plain" },
    })));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("Jobs API transient failure");
  });

  it("classifies an exact oversized workflow command as invalid", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async () => workflowCommandErrorResponse(413)));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority)).rejects.toMatchObject({
      message: "opaque_failure",
      type: "invalid_request",
      nonRetryable: true,
    });
  });

  it.each([400, 401, 403, 404, 409, 413, 422])(
    "retries a malformed or intermediary workflow-command rejection with %s",
    async (status) => {
      const authority = opaqueAuthority();
      vi.stubGlobal("fetch", vi.fn(async () =>
        jsonResponse({ outcome: "rejected" }, status)));
      const activities = await import("../src/activities.js");

      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow("Jobs API ambiguous response");
    },
  );

  it("retries a workflow-command error whose status and closed body disagree", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse({
      schema_version: 2,
      outcome: "rejected",
      reason: "invalid_request",
    }, 409)));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("Jobs API ambiguous response");
  });

  it("retries a workflow-command response without exact private headers", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async () => new Response(
      JSON.stringify(materializedStart(authority)),
      { status: 200, headers: { "Content-Type": "application/json" } },
    )));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("Jobs API ambiguous response");
  });

  it("retries malformed successful Jobs API JSON without exposing it", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async () => new Response("private malformed response", {
      status: 200,
      headers: {
        "Cache-Control": "no-store",
        "Content-Type": "application/json",
        "X-Content-Type-Options": "nosniff",
      },
    })));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("Jobs API ambiguous response");
  });

  it("replays a staged intervention after prepare response loss without creating another prompt", async () => {
    const authority = opaqueAuthority();
    const interventionId = `wfint-v2-${"7".repeat(32)}`;
    const receipt = {
      status: "needs_input",
      issues: [],
      intervention: {
        kind: "browser_takeover",
        title: "Continue",
        detail: "Continue in the browser.",
      },
    };
    const prepareBodies: string[] = [];
    let runnerCommitted = false;
    let executionCalls = 0;
    let prepareCalls = 0;
    const responseLosses = 5;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerCommitted
          ? jsonResponse(runnerResult(receipt, "cloud-application-123", authority.requestId))
          : runnerResultNotFoundResponse();
      }
      if (url === "https://jobs-runner.example/runs") {
        executionCalls += 1;
        runnerCommitted = true;
        return jsonResponse(runnerResult(receipt, "cloud-application-123", authority.requestId));
      }
      if (url.endsWith("/intervention/prepare")) {
        prepareCalls += 1;
        prepareBodies.push(String(init?.body));
        if (prepareCalls <= responseLosses) {
          throw new TypeError("response lost after hidden prepare");
        }
        return jsonResponse(interventionEcho(authority, interventionId, true));
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    for (let attempt = 0; attempt < responseLosses; attempt += 1) {
      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow("response lost after hidden prepare");
    }
    await expect(activities.executeApplicationCommand(authority)).resolves.toEqual({
      state: "intervention_prepared",
      interventionId,
    });

    expect(executionCalls).toBe(1);
    expect(prepareCalls).toBe(responseLosses + 1);
    expect(new Set(prepareBodies).size).toBe(1);
  });

  it("recovers a V2 submitted result without replaying running state or browser execution", async () => {
    const authority = opaqueAuthority();
    const rawResult = submittedRunnerResult(
      { requestId: authority.requestId },
      "cloud-application-123",
    );
    const stateBodies: string[] = [];
    const receiptBodies: string[] = [];
    let runnerCommitted = false;
    let executionCalls = 0;
    let receiptCalls = 0;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") {
        return runnerCommitted ? jsonResponse(rawResult) : runnerResultNotFoundResponse();
      }
      if (url.endsWith("/state")) {
        stateBodies.push(String(init?.body));
        return new Response(null, { status: 204 });
      }
      if (url === "https://jobs-runner.example/runs") {
        executionCalls += 1;
        runnerCommitted = true;
        return jsonResponse(rawResult);
      }
      if (url.endsWith("/receipt")) {
        receiptCalls += 1;
        receiptBodies.push(String(init?.body));
        if (receiptCalls === 1) throw new TypeError("response lost after receipt commit");
        return new Response(null, { status: 204 });
      }
      if (url.includes("https://jobs-runner.example/runs/")) {
        return new Response(null, { status: 204 });
      }
      return new Response(null, { status: 404 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .rejects.toThrow("response lost after receipt commit");
    await expect(activities.executeApplicationCommand(authority))
      .resolves.toEqual({ state: "submitted" });

    expect(executionCalls).toBe(1);
    expect(stateBodies).toHaveLength(1);
    expect(receiptBodies).toHaveLength(2);
    expect(receiptBodies[1]).toBe(receiptBodies[0]);
  });

  it("retries transient browser cleanup after a durable V2 submission", async () => {
    const authority = opaqueAuthority();
    const rawResult = submittedRunnerResult(
      { requestId: authority.requestId },
      "cloud-application-123",
    );
    const responseLosses = 5;
    let cleanupCalls = 0;
    let runnerExecutionCalls = 0;
    let receiptCalls = 0;
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") return jsonResponse(rawResult);
      if (url === "https://jobs-runner.example/runs") {
        runnerExecutionCalls += 1;
        return jsonResponse(rawResult);
      }
      if (url.endsWith("/receipt")) {
        receiptCalls += 1;
        return new Response(null, { status: 204 });
      }
      if (url.includes("https://jobs-runner.example/runs/")) {
        cleanupCalls += 1;
        if (cleanupCalls <= responseLosses) {
          throw new TypeError("response lost after browser cleanup");
        }
        return new Response(null, { status: 404 });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    for (let attempt = 0; attempt < responseLosses; attempt += 1) {
      await expect(activities.executeApplicationCommand(authority))
        .rejects.toThrow("response lost after browser cleanup");
    }
    await expect(activities.executeApplicationCommand(authority))
      .resolves.toEqual({ state: "submitted" });

    expect(runnerExecutionCalls).toBe(0);
    expect(receiptCalls).toBe(responseLosses + 1);
    expect(cleanupCalls).toBe(responseLosses + 1);
  });

  it("keeps a durable V2 submission when browser cleanup is permanently rejected", async () => {
    const authority = opaqueAuthority();
    const rawResult = submittedRunnerResult(
      { requestId: authority.requestId },
      "cloud-application-123",
    );
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url === "https://jobs-runner.example/results") return jsonResponse(rawResult);
      if (url.endsWith("/receipt")) return new Response(null, { status: 204 });
      if (url.includes("https://jobs-runner.example/runs/")) {
        return new Response("private cleanup rejection", { status: 422 });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.executeApplicationCommand(authority))
      .resolves.toEqual({ state: "submitted" });
  });

  it("replays exact publication after its response is lost", async () => {
    const authority = opaqueAuthority();
    const interventionId = `wfint-v2-${"7".repeat(32)}`;
    const bodies: string[] = [];
    let calls = 0;
    const responseLosses = 5;
    vi.stubGlobal("fetch", vi.fn(async (
      _input: string | URL | Request,
      init?: RequestInit,
    ) => {
      calls += 1;
      bodies.push(String(init?.body));
      if (calls <= responseLosses) throw new TypeError("response lost after publish");
      return jsonResponse(interventionEcho(authority, interventionId, true));
    }));
    const activities = await import("../src/activities.js");
    const input = { command: authority, interventionId };

    for (let attempt = 0; attempt < responseLosses; attempt += 1) {
      await expect(activities.publishApplicationIntervention(input))
        .rejects.toThrow("response lost after publish");
    }
    await expect(activities.publishApplicationIntervention(input)).resolves.toEqual({
      state: "needs_input",
      interventionId,
    });

    expect(bodies).toHaveLength(responseLosses + 1);
    expect(new Set(bodies).size).toBe(1);
  });

  it("replays exact terminal finalization before closing the runner", async () => {
    const authority = opaqueAuthority();
    const interventionId = `wfint-v2-${"7".repeat(32)}`;
    const finalizeBodies: string[] = [];
    let finalizeCalls = 0;
    const responseLosses = 5;
    let releaseCalls = 0;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url.endsWith(`/${authority.requestId}/finalize`)) {
        finalizeCalls += 1;
        finalizeBodies.push(String(init?.body));
        if (finalizeCalls <= responseLosses) {
          throw new TypeError("response lost after terminal commit");
        }
        return jsonResponse(finalizationEcho(
          authority,
          "failed",
          "intervention_timeout",
          interventionId,
          true,
        ));
      }
      if (url.includes("https://jobs-runner.example/runs/")) {
        releaseCalls += 1;
        return new Response(null, { status: 204 });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");
    const input = {
      command: authority,
      terminalState: "failed" as const,
      reasonCode: "intervention_timeout" as const,
      openInterventionId: interventionId,
    };

    for (let attempt = 0; attempt < responseLosses; attempt += 1) {
      await expect(activities.finalizeApplicationCommand(input))
        .rejects.toThrow("response lost after terminal commit");
    }
    expect(releaseCalls).toBe(0);
    await expect(activities.finalizeApplicationCommand(input)).resolves.toEqual({ state: "failed" });

    expect(finalizeBodies).toHaveLength(responseLosses + 1);
    expect(new Set(finalizeBodies).size).toBe(1);
    expect(JSON.parse(finalizeBodies[0]!)).toEqual({
      schema_version: 2,
      workflow_id: authority.workflowId,
      payload_digest: authority.payloadDigest,
      operation: "start",
      terminal_state: "failed",
      reason_code: "intervention_timeout",
      open_intervention_id: interventionId,
    });
    expect(releaseCalls).toBe(1);
  });

  it("replays finalization after repeated runner-cleanup response loss", async () => {
    const authority = opaqueAuthority();
    const finalizeBodies: string[] = [];
    let releaseCalls = 0;
    const responseLosses = 5;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url.endsWith(`/${authority.requestId}/finalize`)) {
        finalizeBodies.push(String(init?.body));
        return jsonResponse(finalizationEcho(
          authority,
          "failed",
          "runner_failed",
          undefined,
          finalizeBodies.length > 1,
        ));
      }
      if (url.includes("https://jobs-runner.example/runs/")) {
        releaseCalls += 1;
        if (releaseCalls <= responseLosses) {
          throw new TypeError("response lost after runner cleanup");
        }
        return new Response(null, { status: 404 });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");
    const input = {
      command: authority,
      terminalState: "failed" as const,
      reasonCode: "runner_failed" as const,
    };

    for (let attempt = 0; attempt < responseLosses; attempt += 1) {
      await expect(activities.finalizeApplicationCommand(input))
        .rejects.toThrow("response lost after runner cleanup");
    }
    await expect(activities.finalizeApplicationCommand(input)).resolves.toEqual({ state: "failed" });

    expect(releaseCalls).toBe(responseLosses + 1);
    expect(finalizeBodies).toHaveLength(responseLosses + 1);
    expect(new Set(finalizeBodies).size).toBe(1);
  });

  it("keeps durable terminal state when browser cleanup is permanently rejected", async () => {
    const authority = opaqueAuthority();
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith(`/${authority.requestId}/materialize`)) {
        return jsonResponse(materializedStart(authority));
      }
      if (url.endsWith(`/${authority.requestId}/finalize`)) {
        return jsonResponse(finalizationEcho(authority, "failed", "runner_failed"));
      }
      if (url.includes("https://jobs-runner.example/runs/")) {
        return new Response("private cleanup rejection", { status: 422 });
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await expect(activities.finalizeApplicationCommand({
      command: authority,
      terminalState: "failed",
      reasonCode: "runner_failed",
    })).resolves.toEqual({ state: "failed" });
  });

  it.each([400, 422])(
    "stops retrying runner cleanup rejected with %s",
    async (status) => {
      vi.stubGlobal("fetch", vi.fn(async () =>
        new Response("private cleanup rejection", { status })));
      const activities = await import("../src/activities.js");

      await expect(activities.releaseBrowser("browser-1")).rejects.toMatchObject({
        message: "opaque_failure",
        type: "runner_release_rejected",
        nonRetryable: true,
      });
    },
  );

  it.each([401, 403, 408, 429, 500, 503])(
    "retries runner cleanup after transient %s without exposing its body",
    async (status) => {
      vi.stubGlobal("fetch", vi.fn(async () =>
        new Response("private cleanup failure", { status })));
      const activities = await import("../src/activities.js");

      await expect(activities.releaseBrowser("browser-1"))
        .rejects.toThrow("runner transient failure");
    },
  );

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
      if (String(input) === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
      if (String(input) === "https://jobs-runner.example/runs") {
        return jsonResponse(runnerResult({ status: "failed", issues: [] }));
      }
      return new Response(null, { status: 204 });
    }));
    const activities = await import("../src/activities.js");

    await activities.runApplication({ ...privateWorkflowInput(), browserSessionId: "browser-1" });

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
    const runnerBody = JSON.parse(String(calls[2]?.init?.body)) as Record<string, unknown>;
    expect(Object.keys(runnerBody).sort()).toEqual([
      "accountId",
      "applicationId",
      "applicationIdentityId",
      "browserProfileId",
      "browserSessionId",
      "job",
      "packet",
      "requestId",
      "runId",
      "url",
    ]);
  });

  it("accepts only exact-submit-success HTTP statuses at the workflow boundary", async () => {
    let submitHttpStatus = 200;
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
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
        await expect(execution)
          .rejects.toThrow("runner returned an invalid response");
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
        return committed ? jsonResponse(rawResult) : runnerResultNotFoundResponse();
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
      : runnerResult(receipt);
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/results") {
        return runnerResultNotFoundResponse();
      }
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
    })).rejects.toThrow("runner returned an invalid response");
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
    })).rejects.toThrow("runner returned an invalid response");
  });

  it("recovers and persists a resumed submission without exposing its authority", async () => {
    const calls: FetchCall[] = [];
    const rawResult = submittedRunnerResult({
      requestId: "run-123:resume:1",
      receiptAuthority: { leaseToken: LEASE_TOKEN, fence: 12 },
    });
    let committed = false;
    vi.stubGlobal("fetch", vi.fn(async (
      input: string | URL | Request,
      init?: RequestInit,
    ) => {
      calls.push({ url: String(input), init });
      if (String(input) === "https://jobs-runner.example/results") {
        return committed ? jsonResponse(rawResult) : runnerResultNotFoundResponse();
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
    })).rejects.toThrow("runner returned an invalid response");
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
    expect(String(error)).toContain("Jobs API transient failure");
    expect(String(error)).not.toContain(LEASE_TOKEN);
  });

  it("recovers a committed runner result after the execute response is lost", async () => {
    const rawResult = submittedRunnerResult();
    let committed = false;
    let executionCalls = 0;
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
      if (String(input) === "https://jobs-runner.example/results") {
        return committed ? jsonResponse(rawResult) : runnerResultNotFoundResponse();
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

function opaqueAuthority() {
  return {
    schemaVersion: 2 as const,
    requestId: `wfreq-v2-${"a".repeat(32)}`,
    workflowId: `bluey-jobs-v2-${"b".repeat(32)}`,
    payloadDigest: "c".repeat(64),
  };
}

function privateWorkflowInput() {
  const job: NormalizedJob = {
    externalId: "job-123",
    canonicalUrl: "https://boards.example/jobs/123",
    source: "greenhouse",
    company: "Private employer",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Private job text",
  };
  const packet: ApplicationPacket = {
    applicationId: "application-123",
    jobId: "job-123",
    resumeVersionId: "resume-123",
    approvedPacketChecksum: "",
    applicationIdentityId: "identity-123",
    browserProfileId: "profile-123",
    answers: { private_question: "private-answer" },
    verifiedClaimIds: [],
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return {
    accountId: "account-123",
    applicationId: "application-123",
    jobId: "job-123",
    canonicalJobKey: "canonical-job-123",
    packetId: "resume-123",
    applicationIdentityId: "identity-123",
    browserProfileId: "profile-123",
    packet,
    job,
    runner: "cloud",
    url: "https://boards.example/jobs/123",
    idempotencyKey: "run-123",
  };
}

function materializedStart(authority: ReturnType<typeof opaqueAuthority>) {
  return {
    schema_version: 2,
    request_id: authority.requestId,
    workflow_id: authority.workflowId,
    payload_digest: authority.payloadDigest,
    operation: "start",
    workflow_input: privateWorkflowInput(),
    browser_session_id: "cloud-application-123",
    result_request_id: authority.requestId,
  };
}

function interventionEcho(
  command: ReturnType<typeof opaqueAuthority> & { interventionId?: string },
  interventionId: string,
  replayed = false,
) {
  const operation = command.interventionId ? "resume" : "start";
  return {
    schema_version: 2,
    request_id: command.requestId,
    workflow_id: command.workflowId,
    payload_digest: command.payloadDigest,
    operation,
    ...(command.interventionId
      ? { command_intervention_id: command.interventionId }
      : {}),
    intervention_id: interventionId,
    replayed,
  };
}

function finalizationEcho(
  command: ReturnType<typeof opaqueAuthority> & { interventionId?: string },
  terminalState: "failed" | "side_effect_unknown",
  reasonCode: "runner_failed" | "runner_ambiguous" | "intervention_timeout" | "intervention_limit",
  openInterventionId?: string,
  replayed = false,
) {
  const operation = command.interventionId ? "resume" : "start";
  return {
    schema_version: 2,
    request_id: command.requestId,
    workflow_id: command.workflowId,
    payload_digest: command.payloadDigest,
    operation,
    ...(command.interventionId
      ? { command_intervention_id: command.interventionId }
      : {}),
    terminal_state: terminalState,
    reason_code: reasonCode,
    ...(openInterventionId ? { open_intervention_id: openInterventionId } : {}),
    replayed,
  };
}

function submittedRunnerResult(
  override: Record<string, unknown> = {},
  browserSessionId = "browser-1",
): Record<string, unknown> {
  return {
    accountId: "account-123",
    applicationId: "application-123",
    applicationIdentityId: "identity-123",
    browserSessionId,
    runId: "run-123",
    requestId: "run-123:initial",
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

function runnerResult(
  receipt: Record<string, unknown>,
  browserSessionId = "browser-1",
  requestId = "run-123:initial",
): Record<string, unknown> {
  return {
    accountId: "account-123",
    applicationId: "application-123",
    applicationIdentityId: "identity-123",
    browserSessionId,
    requestId,
    receipt,
    runId: "run-123",
  };
}

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: {
      "Cache-Control": "no-store",
      "Content-Type": "application/json",
      "X-Content-Type-Options": "nosniff",
    },
  });
}

function workflowCommandErrorResponse(status: 400 | 404 | 409 | 413): Response {
  if (status === 409) {
    return jsonResponse({
      schema_version: 2,
      outcome: "identity_conflict",
      reason: "identity_conflict",
    }, status);
  }
  if (status === 404) {
    return jsonResponse({
      schema_version: 2,
      outcome: "rejected",
      reason: "not_found",
    }, status);
  }
  return jsonResponse({
    schema_version: 2,
    outcome: "rejected",
    reason: "invalid_request",
  }, status);
}

function runnerResultNotFoundResponse(): Response {
  return jsonResponse({ error: "Durable result not found" }, 404);
}

function expectRunnerRequest(call: FetchCall | undefined): void {
  expect(call).toBeDefined();
  const headers = new Headers(call?.init?.headers);
  expect(headers.get("authorization")).toBe("Bearer runner-service-token");
  expect(headers.get("x-bluey-jobs-worker-signature")).toBeNull();
  expect(call?.init?.redirect).toBe("error");
  expect(call?.init?.signal).toBeInstanceOf(AbortSignal);
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
  expect(call?.init?.signal).toBeInstanceOf(AbortSignal);
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
