import type { Server } from "node:http";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createGatewayHttpServer,
  workflowCleanupEnabled,
  workflowCleanupNamespace,
  workflowCleanupStartupNamespace,
  workflowGatewayToken,
  type GatewayCleanupExecutor,
  type GatewayCommandExecutor,
} from "../src/gateway.js";
import { WORKFLOW_CLEANUP_MAX_RUN_IDS } from "../src/gateway-cleanup-service.js";

const TOKEN = "gateway-token-0123456789abcdefgh";
const execute = vi.fn();
const executeCleanup = vi.fn();
let server: Server;
let origin: string;

beforeEach(async () => {
  execute.mockReset();
  executeCleanup.mockReset();
  server = createGatewayHttpServer(TOKEN, { execute } as GatewayCommandExecutor);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Test gateway did not bind");
  origin = `http://127.0.0.1:${address.port}`;
});

afterEach(async () => {
  server.closeAllConnections();
  await new Promise<void>((resolve, reject) => {
    server.close((error) => error ? reject(error) : resolve());
  });
  vi.restoreAllMocks();
});

describe("workflow gateway HTTP boundary", () => {
  it("requires the same bounded workflow token as the dispatcher", () => {
    expect(() => workflowGatewayToken("x".repeat(31))).toThrow("32 to 8192 byte");
    expect(workflowGatewayToken(`  ${"x".repeat(32)}  `)).toBe("x".repeat(32));
    expect(workflowGatewayToken("x".repeat(8 * 1024))).toHaveLength(8 * 1024);
    expect(() => workflowGatewayToken("x".repeat(8 * 1024 + 1)))
      .toThrow("32 to 8192 byte");
    for (const invalid of [
      `${"x".repeat(32)} internal`,
      `${"x".repeat(32)}\nsecond`,
      `${"x".repeat(31)}é`,
      `${"x".repeat(16)}=${"x".repeat(16)}`,
    ]) {
      expect(() => workflowGatewayToken(invalid)).toThrow("RFC 6750 bearer token");
    }
    expect(workflowGatewayToken(`${"x".repeat(32)}==`)).toBe(`${"x".repeat(32)}==`);
  });

  it("keeps cleanup physically absent unless an executor is explicitly registered", async () => {
    vi.stubEnv("BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED", "true");
    const response = await gatewayRequest("/workflow-cleanup", "{}");

    expect(response.status).toBe(404);
    expect(await response.json()).toEqual({
      schemaVersion: 2,
      outcome: "rejected",
      reason: "invalid_request",
    });
    expect(execute).not.toHaveBeenCalled();
    expect(executeCleanup).not.toHaveBeenCalled();
  });

  it("enables cleanup only for the exact literal true and validates its namespace", () => {
    expect(workflowCleanupEnabled("true")).toBe(true);
    for (const value of [undefined, "", "1", "TRUE", " true", "true "]) {
      expect(workflowCleanupEnabled(value)).toBe(false);
    }
    expect(workflowCleanupNamespace("bluey-jobs.production_1")).toBe(
      "bluey-jobs.production_1",
    );
    for (const value of [undefined, "", " private", "private/other", "é"]) {
      expect(() => workflowCleanupNamespace(value)).toThrow("Temporal namespace");
    }
    expect(workflowCleanupStartupNamespace("0", undefined, "temporal-prod")).toBeUndefined();
    expect(workflowCleanupStartupNamespace("true", "temporal-prod", "temporal-prod"))
      .toBe("temporal-prod");
    expect(() => workflowCleanupStartupNamespace("true", undefined, "temporal-prod"))
      .toThrow("Temporal namespace");
    expect(() => workflowCleanupStartupNamespace("true", "other", "temporal-prod"))
      .toThrow("must equal");
  });

  it("keeps health isolated from command authentication", async () => {
    const response = await fetch(`${origin}/healthz`);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ ok: true });
    expect(execute).not.toHaveBeenCalled();
  });

  it("rejects missing authentication with a closed body", async () => {
    const response = await commandRequest("{}", { authorization: undefined });

    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({
      schemaVersion: 2,
      outcome: "rejected",
      reason: "invalid_request",
    });
    expect(execute).not.toHaveBeenCalled();
  });

  it("exposes no legacy route alias or resume fallback", async () => {
    for (const path of [
      "/workflows/applications",
      "/workflows/applications/account/run/resume",
      "/workflows/commands",
      "/workflow-cleanups",
    ]) {
      const response = await fetch(`${origin}${path}`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${TOKEN}`,
          "Content-Type": "application/json",
        },
        body: "{}",
      });
      expect(response.status).toBe(404);
    }
    expect(execute).not.toHaveBeenCalled();
  });

  it.each([
    ["wrong content type", "{}", "text/plain"],
    ["malformed JSON", "{", "application/json"],
    ["empty body", "", "application/json"],
    ["oversized body", JSON.stringify({ padding: "x".repeat(17 * 1024) }), "application/json"],
  ])("rejects %s before service execution", async (_label, body, contentType) => {
    const response = await fetch(`${origin}/workflow-commands`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${TOKEN}`,
        "Content-Type": contentType,
      },
      body,
    });

    expect(response.status).toBe(400);
    expect(await response.json()).toEqual({
      schemaVersion: 2,
      outcome: "rejected",
      reason: "invalid_request",
    });
    expect(execute).not.toHaveBeenCalled();
  });

  it("passes an exact JSON command to the injected service", async () => {
    const command = {
      schemaVersion: 2,
      operation: "start",
      requestId: `wfreq-v2-${"a".repeat(32)}`,
      workflowId: `bluey-jobs-v2-${"b".repeat(32)}`,
      payloadDigest: "c".repeat(64),
    };
    execute.mockResolvedValueOnce({
      status: 202,
      body: {
        ...command,
        outcome: "accepted",
        temporalRunId: `run-${"d".repeat(32)}`,
      },
    });

    const response = await commandRequest(JSON.stringify(command));

    expect(response.status).toBe(202);
    expect(execute).toHaveBeenCalledWith(command);
    expect(response.headers.get("content-type")).toBe("application/json");
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(response.headers.get("x-content-type-options")).toBe("nosniff");
    for (const name of ["content-type", "cache-control", "x-content-type-options"]) {
      expect(response.headers.get(name)?.includes(",")).toBe(false);
    }
  });

  it("does not expose unfinished cleanup to unauthenticated requests", async () => {
    const response = await fetch(`${origin}/workflow-cleanup`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}",
    });

    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({
      schemaVersion: 2,
      outcome: "rejected",
      reason: "invalid_request",
    });
    expect(execute).not.toHaveBeenCalled();
  });

  it("registers the exact authenticated cleanup route without changing commands", async () => {
    await replaceServer({ executeCleanup } as GatewayCleanupExecutor);
    const request = {
      schemaVersion: 3,
      operation: "legacy_inventory_page",
      cleanupRequestId: `wfclean-v3-${"a".repeat(32)}`,
    };
    executeCleanup.mockResolvedValueOnce({
      status: 202,
      body: {
        ...request,
        outcome: "page",
      },
    });

    const cleanupResponse = await gatewayRequest(
      "/workflow-cleanup",
      JSON.stringify(request),
    );

    expect(cleanupResponse.status).toBe(202);
    expect(executeCleanup).toHaveBeenCalledWith(request);
    expect(execute).not.toHaveBeenCalled();
    expect(cleanupResponse.headers.get("content-type")).toBe("application/json");
    expect(cleanupResponse.headers.get("cache-control")).toBe("no-store");
    expect(cleanupResponse.headers.get("x-content-type-options")).toBe("nosniff");

    execute.mockResolvedValueOnce({
      status: 202,
      body: {
        schemaVersion: 2,
        requestId: `wfreq-v2-${"b".repeat(32)}`,
        workflowId: `bluey-jobs-v2-${"c".repeat(32)}`,
        payloadDigest: "d".repeat(64),
        outcome: "accepted",
        temporalRunId: `temporal-run-${"e".repeat(32)}`,
      },
    });
    expect((await commandRequest("{}")).status).toBe(202);
    expect(execute).toHaveBeenCalledWith({});
  });

  it("keeps auth generic v2 before exposing an enabled cleanup protocol", async () => {
    await replaceServer({ executeCleanup } as GatewayCleanupExecutor);

    const response = await fetch(`${origin}/workflow-cleanup`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}",
    });

    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({
      schemaVersion: 2,
      outcome: "rejected",
      reason: "invalid_request",
    });
    expect(executeCleanup).not.toHaveBeenCalled();
  });

  it.each([
    ["wrong method", "GET", undefined, 404, "not_found"],
    ["wrong content type", "POST", "text/plain", 400, "invalid_request"],
    ["malformed JSON", "POST", "application/json", 400, "invalid_request"],
  ])("uses closed v3 errors for authorized enabled cleanup: %s", async (
    label,
    method,
    contentType,
    status,
    reason,
  ) => {
    await replaceServer({ executeCleanup } as GatewayCleanupExecutor);
    const response = await fetch(`${origin}/workflow-cleanup`, {
      method,
      headers: {
        Authorization: `Bearer ${TOKEN}`,
        ...(contentType ? { "Content-Type": contentType } : {}),
      },
      ...(method === "POST" ? { body: label === "malformed JSON" ? "{" : "{}" } : {}),
    });

    expect(response.status).toBe(status);
    expect(await response.json()).toEqual({
      schemaVersion: 3,
      outcome: "rejected",
      reason,
    });
    expect(executeCleanup).not.toHaveBeenCalled();
  });

  it("converts an unexpected cleanup failure to a private v3 503", async () => {
    await replaceServer({ executeCleanup } as GatewayCleanupExecutor);
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    executeCleanup.mockRejectedValueOnce(new Error(
      "private account answer https://private.example",
    ));

    const response = await gatewayRequest("/workflow-cleanup", "{}");

    expect(response.status).toBe(503);
    expect(await response.json()).toEqual({
      schemaVersion: 3,
      outcome: "rejected",
      reason: "temporal_unavailable",
    });
    expect(error).not.toHaveBeenCalled();
  });

  it("allows the exact 128 KiB cleanup boundary and rejects one byte more", async () => {
    await replaceServer({ executeCleanup } as GatewayCleanupExecutor);
    executeCleanup.mockResolvedValue({
      status: 202,
      body: { schemaVersion: 3, outcome: "rejected", reason: "invalid_request" },
    });
    const exact = `{"padding":"${"x".repeat(128 * 1024 - 14)}"}`;
    const oversized = `{"padding":"${"x".repeat(128 * 1024 - 13)}"}`;
    expect(Buffer.byteLength(exact)).toBe(128 * 1024);
    expect(Buffer.byteLength(oversized)).toBe(128 * 1024 + 1);

    const accepted = await gatewayRequest("/workflow-cleanup", exact);
    const rejected = await gatewayRequest("/workflow-cleanup", oversized);

    expect(accepted.status).toBe(202);
    expect(executeCleanup).toHaveBeenCalledTimes(1);
    expect(rejected.status).toBe(400);
    expect(await rejected.json()).toEqual({
      schemaVersion: 3,
      outcome: "rejected",
      reason: "invalid_request",
    });
  });

  it("carries a valid cleanup request with the maximum 32 bounded run IDs", async () => {
    await replaceServer({ executeCleanup } as GatewayCleanupExecutor);
    executeCleanup.mockResolvedValueOnce({
      status: 202,
      body: { schemaVersion: 3, outcome: "rejected", reason: "invalid_request" },
    });
    const knownRunIds = Array.from(
      { length: WORKFLOW_CLEANUP_MAX_RUN_IDS },
      (_, index) => `run-${String(index).padStart(4, "0")}-${"a".repeat(119)}`,
    );
    const request = {
      schemaVersion: 3,
      operation: "reconcile_v2_target",
      cleanupRequestId: `wfclean-v3-${"b".repeat(32)}`,
      cleanupGenerationId: `wfcleanupgen-v3-${"c".repeat(32)}`,
      targetSetDigest: "d".repeat(64),
      namespace: "bluey-jobs",
      workflowType: "applicationWorkflowV2",
      workflowId: `bluey-jobs-v2-${"e".repeat(32)}`,
      firstExecutionRunId: knownRunIds[0],
      startRequestId: `wfreq-v2-${"f".repeat(32)}`,
      startPayloadDigest: "1".repeat(64),
      knownRunIds,
      targetDigest: "2".repeat(64),
      cleanupFence: 1,
      observationPass: 1,
    };
    const serialized = JSON.stringify(request);
    expect(Buffer.byteLength(serialized)).toBeLessThanOrEqual(128 * 1024);

    const response = await gatewayRequest("/workflow-cleanup", serialized);

    expect(response.status).toBe(202);
    expect(executeCleanup).toHaveBeenCalledWith(request);
  });

  it("does not log or reflect a raw unexpected service error", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    execute.mockRejectedValueOnce(new Error(
      "account-private answer=private-answer https://private.example/takeover",
    ));

    const response = await commandRequest("{}");
    const serialized = JSON.stringify(await response.json());

    expect(response.status).toBe(503);
    expect(serialized).toBe(JSON.stringify({
      schemaVersion: 2,
      outcome: "delivery_unknown",
      reason: "temporal_unavailable",
    }));
    expect(error).not.toHaveBeenCalled();
  });
});

function commandRequest(
  body: string,
  override: { authorization?: string } = {},
): Promise<Response> {
  const authorization = Object.prototype.hasOwnProperty.call(override, "authorization")
    ? override.authorization
    : `Bearer ${TOKEN}`;
  return fetch(`${origin}/workflow-commands`, {
    method: "POST",
    headers: {
      ...(authorization ? { Authorization: authorization } : {}),
      "Content-Type": "application/json; charset=utf-8",
    },
    body,
  });
}

function gatewayRequest(path: string, body: string): Promise<Response> {
  return fetch(`${origin}${path}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      "Content-Type": "application/json",
    },
    body,
  });
}

async function replaceServer(cleanupService: GatewayCleanupExecutor): Promise<void> {
  server.closeAllConnections();
  await new Promise<void>((resolve, reject) => {
    server.close((error) => error ? reject(error) : resolve());
  });
  server = createGatewayHttpServer(
    TOKEN,
    { execute } as GatewayCommandExecutor,
    cleanupService,
  );
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Test gateway did not bind");
  origin = `http://127.0.0.1:${address.port}`;
}
