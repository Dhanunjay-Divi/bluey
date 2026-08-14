import type { Server } from "node:http";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createGatewayHttpServer,
  workflowGatewayToken,
  type GatewayCommandExecutor,
} from "../src/gateway.js";

const TOKEN = "gateway-token-0123456789abcdefgh";
const execute = vi.fn();
let server: Server;
let origin: string;

beforeEach(async () => {
  execute.mockReset();
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

  it("keeps the unfinished workflow cleanup route absent in every environment", async () => {
    vi.stubEnv("BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED", "true");
    const response = await gatewayRequest("/workflow-cleanup", "{}");

    expect(response.status).toBe(404);
    expect(execute).not.toHaveBeenCalled();
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
    expect(execute).not.toHaveBeenCalled();
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
