import { timingSafeEqual } from "node:crypto";
import {
  createServer,
  type IncomingMessage,
  type RequestListener,
  type Server,
  type ServerResponse,
} from "node:http";
import { pathToFileURL } from "node:url";
import { Client, Connection } from "@temporalio/client";
import {
  createGatewayService,
  WORKFLOW_COMMAND_PATH,
  type GatewayServiceResult,
} from "./gateway-service.js";
import {
  createTemporalCleanupClient,
  createWorkflowCleanupService,
  WORKFLOW_CLEANUP_PATH,
  type WorkflowCleanupServiceResult,
} from "./gateway-cleanup-service.js";
import type { WorkflowCleanupError, WorkflowGatewayError } from "./contracts.js";

const MAX_COMMAND_BYTES = 16 * 1024;
const MAX_CLEANUP_BYTES = 128 * 1024;

export async function runGateway(): Promise<void> {
  const token = workflowGatewayToken(process.env.BLUEY_JOBS_WORKFLOW_TOKEN);
  const address = process.env.TEMPORAL_ADDRESS || "";
  const namespace = process.env.TEMPORAL_NAMESPACE || "";
  const port = Number(process.env.PORT || 8090);
  if (!address || !namespace) throw new Error("TEMPORAL_ADDRESS and TEMPORAL_NAMESPACE are required");
  if (!Number.isSafeInteger(port) || port < 1 || port > 65_535) {
    throw new Error("PORT must be a valid TCP port");
  }
  const cleanupNamespace = workflowCleanupStartupNamespace(
    process.env.BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED,
    process.env.BLUEY_JOBS_WORKFLOW_NAMESPACE,
    namespace,
  );

  const connection = await Connection.connect({
    address,
    tls: process.env.TEMPORAL_TLS === "false" ? false : true,
    apiKey: process.env.TEMPORAL_API_KEY,
  });
  const client = new Client({
    connection,
    namespace,
    dataConverter: {
      failureConverterPath: new URL("./failure-converter.js", import.meta.url).pathname,
    },
  });
  const service = createGatewayService({
    client: client.workflow,
    taskQueue: process.env.BLUEY_JOBS_TASK_QUEUE || "bluey-jobs-applications",
  });
  const cleanupService = cleanupNamespace
    ? createWorkflowCleanupService({
      client: createTemporalCleanupClient(client.workflow),
      namespace: cleanupNamespace,
    })
    : undefined;
  createGatewayHttpServer(token, service, cleanupService)
    .listen(port, "0.0.0.0", () => console.log(`Bluey Jobs workflow gateway listening on ${port}`));
}

export interface GatewayCommandExecutor {
  execute(value: unknown): Promise<GatewayServiceResult>;
}

export interface GatewayCleanupExecutor {
  executeCleanup(value: unknown): Promise<WorkflowCleanupServiceResult>;
}

export function createGatewayRequestHandler(
  token: string,
  service: GatewayCommandExecutor,
  cleanupService?: GatewayCleanupExecutor,
): RequestListener {
  const expectedToken = workflowGatewayToken(token);
  return async (request, response) => {
    if (request.method === "GET" && request.url === "/healthz") {
      return json(response, 200, { ok: true });
    }
    if (!authorized(request, expectedToken)) {
      return json(response, 401, gatewayError("rejected", "invalid_request"));
    }
    const cleanupRoute = request.url === WORKFLOW_CLEANUP_PATH && cleanupService !== undefined;
    const commandRoute = request.url === WORKFLOW_COMMAND_PATH;
    if (!cleanupRoute && !commandRoute) {
      return json(response, 404, gatewayError("rejected", "invalid_request"));
    }
    if (request.method !== "POST") {
      return cleanupRoute
        ? json(response, 404, cleanupGatewayError("rejected", "not_found"))
        : json(response, 404, gatewayError("rejected", "invalid_request"));
    }
    if (!isJsonContentType(request.headers["content-type"])) {
      return cleanupRoute
        ? json(response, 400, cleanupGatewayError("rejected", "invalid_request"))
        : json(response, 400, gatewayError("rejected", "invalid_request"));
    }
    let parsed: unknown;
    try {
      parsed = await body(request, cleanupRoute ? MAX_CLEANUP_BYTES : MAX_COMMAND_BYTES);
    } catch {
      return cleanupRoute
        ? json(response, 400, cleanupGatewayError("rejected", "invalid_request"))
        : json(response, 400, gatewayError("rejected", "invalid_request"));
    }
    if (cleanupRoute) {
      let result: WorkflowCleanupServiceResult;
      try {
        result = await cleanupService.executeCleanup(parsed);
      } catch {
        result = {
          status: 503,
          body: cleanupGatewayError("rejected", "temporal_unavailable"),
        };
      }
      return json(response, result.status, result.body);
    }
    let result: GatewayServiceResult;
    try {
      result = await service.execute(parsed);
    } catch {
      result = {
        status: 503,
        body: gatewayError("delivery_unknown", "temporal_unavailable"),
      };
    }
    return json(response, result.status, result.body);
  };
}

export function workflowCleanupEnabled(value: string | undefined): boolean {
  return value === "true";
}

export function workflowCleanupNamespace(value: string | undefined): string {
  if (typeof value !== "string"
    || value.length < 1
    || value.length > 255
    || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value)) {
    throw new Error("BLUEY_JOBS_WORKFLOW_NAMESPACE must be a valid Temporal namespace");
  }
  return value;
}

export function workflowCleanupStartupNamespace(
  enabledValue: string | undefined,
  cleanupNamespaceValue: string | undefined,
  temporalNamespace: string,
): string | undefined {
  if (!workflowCleanupEnabled(enabledValue)) return undefined;
  const cleanupNamespace = workflowCleanupNamespace(cleanupNamespaceValue);
  if (cleanupNamespace !== temporalNamespace) {
    throw new Error("BLUEY_JOBS_WORKFLOW_NAMESPACE must equal TEMPORAL_NAMESPACE");
  }
  return cleanupNamespace;
}

export function workflowGatewayToken(value: string | undefined): string {
  const token = value?.trim() ?? "";
  const length = Buffer.byteLength(token, "utf8");
  if (length < 32 || length > 8 * 1024 || !/^[A-Za-z0-9._~+/-]+=*$/.test(token)) {
    throw new Error(
      "BLUEY_JOBS_WORKFLOW_TOKEN must be a 32 to 8192 byte RFC 6750 bearer token",
    );
  }
  return token;
}

export function createGatewayHttpServer(
  token: string,
  service: GatewayCommandExecutor,
  cleanupService?: GatewayCleanupExecutor,
): Server {
  const server = createServer(createGatewayRequestHandler(token, service, cleanupService));
  server.headersTimeout = 10_000;
  server.requestTimeout = 20_000;
  server.keepAliveTimeout = 5_000;
  return server;
}

function authorized(request: IncomingMessage, token: string): boolean {
  const header = request.headers.authorization;
  if (typeof header !== "string" || !/^Bearer [^\s]+$/.test(header)) return false;
  const supplied = Buffer.from(header.slice("Bearer ".length));
  const expected = Buffer.from(token);
  return supplied.length === expected.length
    && supplied.length > 0
    && timingSafeEqual(supplied, expected);
}

async function body(request: IncomingMessage, maximumBytes: number): Promise<unknown> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    const value = Buffer.from(chunk);
    size += value.length;
    if (size > maximumBytes) throw new Error("Invalid workflow gateway request");
    chunks.push(value);
  }
  if (size === 0) throw new Error("Invalid workflow command");
  return JSON.parse(Buffer.concat(chunks).toString("utf8")) as unknown;
}

function isJsonContentType(value: string | undefined): boolean {
  return typeof value === "string" && /^application\/json(?:\s*;\s*charset=utf-8)?$/i.test(value);
}

function gatewayError(
  outcome: WorkflowGatewayError["outcome"],
  reason: WorkflowGatewayError["reason"],
): WorkflowGatewayError {
  return { schemaVersion: 2, outcome, reason };
}

function cleanupGatewayError(
  outcome: WorkflowCleanupError["outcome"],
  reason: WorkflowCleanupError["reason"],
): WorkflowCleanupError {
  return { schemaVersion: 3, outcome, reason };
}

function json(response: ServerResponse, status: number, value: unknown): void {
  response.writeHead(status, {
    "Content-Type": "application/json",
    "Cache-Control": "no-store",
    "X-Content-Type-Options": "nosniff",
  });
  response.end(JSON.stringify(value));
}

const entrypoint = process.argv[1];
if (entrypoint && import.meta.url === pathToFileURL(entrypoint).href) {
  void runGateway();
}
