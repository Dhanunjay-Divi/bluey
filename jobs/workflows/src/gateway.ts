import { timingSafeEqual } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { Client, Connection, WorkflowExecutionAlreadyStartedError } from "@temporalio/client";
import type { ApplicationWorkflowInput } from "./contracts.js";
import { applicationWorkflow, resolveInterventionSignal } from "./workflows.js";
import type { InterventionResolution } from "./contracts.js";

const token = process.env.BLUEY_JOBS_WORKFLOW_TOKEN || "";
const address = process.env.TEMPORAL_ADDRESS || "";
const namespace = process.env.TEMPORAL_NAMESPACE || "";
const port = Number(process.env.PORT || 8090);
if (!token) throw new Error("BLUEY_JOBS_WORKFLOW_TOKEN is required");
if (!address || !namespace) throw new Error("TEMPORAL_ADDRESS and TEMPORAL_NAMESPACE are required");

const connection = await Connection.connect({
  address,
  tls: process.env.TEMPORAL_TLS === "false" ? false : true,
  apiKey: process.env.TEMPORAL_API_KEY,
});
const client = new Client({ connection, namespace });

createServer(async (request, response) => {
  try {
    if (request.method === "GET" && request.url === "/healthz") return json(response, 200, { ok: true });
    if (!authorized(request)) return json(response, 401, { error: "Unauthorized" });
    if (request.method === "POST" && request.url === "/workflows/applications") {
      const input = await body<ApplicationWorkflowInput>(request);
      validate(input);
      const workflowId = `bluey-jobs:${input.accountId}:${input.idempotencyKey}`;
      await client.workflow.start(applicationWorkflow, {
        taskQueue: process.env.BLUEY_JOBS_TASK_QUEUE || "bluey-jobs-applications",
        workflowId,
        args: [input],
        workflowIdConflictPolicy: "FAIL",
      });
      return json(response, 202, { workflowId, runId: input.idempotencyKey });
    }
    const resume = request.url?.match(/^\/workflows\/applications\/([^/]+)\/([^/]+)\/resume$/);
    if (request.method === "POST" && resume) {
      const accountId = decodeURIComponent(resume[1]);
      const runId = decodeURIComponent(resume[2]);
      const resolution = await body<InterventionResolution>(request);
      const workflowId = `bluey-jobs:${accountId}:${runId}`;
      await client.workflow.getHandle(workflowId).signal(resolveInterventionSignal, resolution);
      return json(response, 202, { workflowId, resumed: true });
    }
    return json(response, 404, { error: "Not found" });
  } catch (error) {
    if (error instanceof WorkflowExecutionAlreadyStartedError) {
      return json(response, 409, { error: "This application is already queued." });
    }
    console.error("Bluey Jobs workflow gateway request failed", error);
    return json(response, 503, { error: "Bluey could not queue this application." });
  }
}).listen(port, "0.0.0.0", () => console.log(`Bluey Jobs workflow gateway listening on ${port}`));

function validate(input: ApplicationWorkflowInput): void {
  for (const [name, value] of Object.entries({
    accountId: input.accountId,
    applicationId: input.applicationId,
    applicationIdentityId: input.applicationIdentityId,
    browserProfileId: input.browserProfileId,
    idempotencyKey: input.idempotencyKey,
  })) {
    if (!/^[A-Za-z0-9:_-]{3,200}$/.test(value)) throw new Error(`Invalid ${name}`);
  }
  if (input.packet.applicationId !== input.applicationId) throw new Error("Application bundle mismatch");
  if (new URL(input.url).protocol !== "https:") throw new Error("Application link must use HTTPS");
}

function authorized(request: IncomingMessage): boolean {
  const supplied = Buffer.from((request.headers.authorization || "").replace(/^Bearer\s+/i, ""));
  const expected = Buffer.from(token);
  return supplied.length === expected.length && supplied.length > 0 && timingSafeEqual(supplied, expected);
}

async function body<T>(request: IncomingMessage): Promise<T> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    const value = Buffer.from(chunk);
    size += value.length;
    if (size > 8 * 1024 * 1024) throw new Error("Request is too large");
    chunks.push(value);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8")) as T;
}

function json(response: ServerResponse, status: number, value: unknown): void {
  response.writeHead(status, { "Content-Type": "application/json", "Cache-Control": "no-store" });
  response.end(JSON.stringify(value));
}
