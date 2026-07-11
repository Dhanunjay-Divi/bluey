import { timingSafeEqual } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { chromium, type BrowserContext } from "playwright";
import {
  PlaywrightBrowserPage,
  assertPublicApplicationUrl,
  createApplicationReceipt,
  executeApplication,
  materializeApplicationDocuments,
  submissionPolicy,
  type ApplicationPacket,
  type NormalizedJob,
  type ReceiptDocument,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import { parseProfileKey, profilePaths, restoreProfile, sealProfile } from "./profile-store.js";
import { readResult, writeResult } from "./result-store.js";

interface CloudRunRequest {
  accountId: string;
  applicationIdentityId: string;
  browserProfileId: string;
  browserSessionId: string;
  runId: string;
  applicationId: string;
  url: string;
  packet: ApplicationPacket;
  job: NormalizedJob;
}

interface RunResolution {
  requestId: string;
  action?: string;
  field?: string;
  answer?: string;
}

type RunEvent = { id: string; occurredAt: string; type: string; detail?: Record<string, unknown> };

const port = Number(process.env.PORT || 8091);
const root = process.env.BLUEY_JOBS_RUNNER_DATA || "/tmp/bluey-jobs-runner";
const serviceToken = process.env.BLUEY_JOBS_RUNNER_TOKEN || "";
const profileKey = process.env.BLUEY_JOBS_PROFILE_ENCRYPTION_KEY
  ? parseProfileKey(process.env.BLUEY_JOBS_PROFILE_ENCRYPTION_KEY)
  : undefined;
const locks = new Map<string, Promise<void>>();
const activeRuns = new Map<string, {
  context: BrowserContext;
  paths: ReturnType<typeof profilePaths>;
  input: CloudRunRequest;
  events: RunEvent[];
}>();
const activeScopes = new Set<string>();

if (!serviceToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
if (!profileKey) throw new Error("BLUEY_JOBS_PROFILE_ENCRYPTION_KEY is required");

createServer(async (request, response) => {
  try {
    if (request.url === "/healthz" && request.method === "GET") return json(response, 200, { ok: true });
    if (!authorized(request)) return json(response, 401, { error: "Unauthorized" });
    if (request.url === "/runs" && request.method === "POST") {
      const input = await body<CloudRunRequest>(request);
      await validate(input);
      const requestId = `${input.runId}:initial`;
      const completed = await readResult<Awaited<ReturnType<typeof executeRun>>>(root, requestId, profileKey);
      if (completed) return json(response, 200, completed);
      const paths = profilePaths(root, input.accountId, input.applicationIdentityId);
      const result = await serialized(paths.scope, async () => {
        const existing = await readResult<Awaited<ReturnType<typeof executeRun>>>(root, requestId, profileKey);
        if (existing) return existing;
        const executed = await run(input, paths);
        await writeResult(root, requestId, executed, profileKey);
        return executed;
      });
      return json(response, 200, result);
    }
    const resume = request.url?.match(/^\/runs\/([A-Za-z0-9_-]{3,160})\/resume$/);
    if (resume && request.method === "POST") {
      const resolution = await body<RunResolution>(request);
      if (!/^[A-Za-z0-9:_-]{3,240}$/.test(resolution.requestId || "")) {
        return json(response, 400, { error: "A valid request ID is required" });
      }
      const completed = await readResult<Awaited<ReturnType<typeof executeRun>>>(root, resolution.requestId, profileKey);
      if (completed) return json(response, 200, completed);
      const active = activeRuns.get(resume[1]);
      if (!active) return json(response, 404, { error: "Browser run not found" });
      if (resolution.field && resolution.answer) {
        active.input.packet.answers[resolution.field] = resolution.answer;
      }
      const result = await serialized(active.paths.scope, async () => {
        const existing = await readResult<Awaited<ReturnType<typeof executeRun>>>(root, resolution.requestId, profileKey);
        if (existing) return existing;
        const executed = await executeRun(
          active.input,
          active.paths,
          active.context,
          active.events,
          false,
        );
        await writeResult(root, resolution.requestId, executed, profileKey);
        return executed;
      });
      if (result.receipt.status !== "needs_input") {
        await active.context.close();
        await sealProfile(active.paths, profileKey!);
        activeRuns.delete(resume[1]);
        activeScopes.delete(active.paths.scope);
      }
      return json(response, 200, result);
    }
    const release = request.url?.match(/^\/runs\/([A-Za-z0-9_-]{3,160})$/);
    if (release && request.method === "DELETE") {
      const active = activeRuns.get(release[1]);
      if (!active) return json(response, 204, {});
      await serialized(active.paths.scope, async () => {
        await active.context.close();
        await sealProfile(active.paths, profileKey!);
        activeRuns.delete(release[1]);
        activeScopes.delete(active.paths.scope);
      });
      return json(response, 204, {});
    }
    return json(response, 404, { error: "Not found" });
  } catch (error) {
    console.error("Bluey Jobs runner request failed", error);
    return json(response, 500, { error: "The application runner could not finish this run." });
  }
}).listen(port, "0.0.0.0", () => {
  console.log(`Bluey Jobs runner listening on ${port}`);
});

async function run(input: CloudRunRequest, paths: ReturnType<typeof profilePaths>) {
  const decision = submissionPolicy(input.url);
  if (decision.policy !== "automate") {
    const receipt: SubmissionReceipt = {
      status: "needs_input",
      issues: [{ field: "submission", message: decision.reason, severity: "blocking" }],
      intervention: {
        kind: "browser_takeover",
        title: "Finish this application",
        detail: decision.reason,
        resolution: { kind: "browser_takeover", resumeAfter: false },
      },
    };
    return { receipt };
  }

  if (activeScopes.has(paths.scope)) throw new Error("This application email already has an active browser run");
  await restoreProfile(paths, profileKey!);
  const context = await chromium.launchPersistentContext(paths.directory, {
    headless: true,
    acceptDownloads: true,
    viewport: { width: 1440, height: 1000 },
  });
  await guardNavigations(context);
  const events: RunEvent[] = [];
  try {
    const result = await executeRun(input, paths, context, events, true);
    if (result.receipt.status === "needs_input") {
      activeScopes.add(paths.scope);
      activeRuns.set(input.browserSessionId, { context, paths, input, events });
    } else {
      await context.close();
      await sealProfile(paths, profileKey!);
    }
    return result;
  } catch (error) {
    await context.close();
    await sealProfile(paths, profileKey!);
    throw error;
  }
}

async function executeRun(
  input: CloudRunRequest,
  paths: ReturnType<typeof profilePaths>,
  context: BrowserContext,
  events: RunEvent[],
  navigate: boolean,
) {
  const runDirectory = join(root, "receipts", paths.scope, input.runId);
  await mkdir(runDirectory, { recursive: true });
  const documents = await materializeApplicationDocuments(input.packet, join(runDirectory, "documents"));
  input.packet = documents.packet;
  const page = context.pages()[0] || await context.newPage();
  if (navigate) await page.goto(input.url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  const browserPage = new PlaywrightBrowserPage(page);
  const execution = await executeApplication({
    runner: "cloud",
    runId: input.runId,
    accountId: input.accountId,
    page: browserPage,
    packet: {
      ...input.packet,
      applicationIdentityId: input.applicationIdentityId,
      browserProfileId: input.browserProfileId,
    },
    async log(type, detail = {}) {
      events.push({ id: `${input.runId}:${events.length + 1}`, occurredAt: new Date().toISOString(), type, detail });
    },
  });
  const screenshotPath = join(runDirectory, "final.png");
  await writeFile(screenshotPath, await browserPage.screenshot({ fullPage: true }), { mode: 0o600 });
  execution.receipt.screenshotPath = screenshotPath;
  if (execution.receipt.status === "needs_input") {
    const takeoverOrigin = process.env.BLUEY_JOBS_TAKEOVER_ORIGIN?.replace(/\/$/, "");
    if (execution.receipt.intervention && takeoverOrigin) {
      execution.receipt.intervention.takeoverUrl = `${takeoverOrigin}/sessions/${encodeURIComponent(input.browserSessionId)}`;
    }
  }
  const receiptDocuments: ReceiptDocument[] = [{
    kind: "resume",
    versionId: input.packet.resumeVersionId,
    storageKey: documents.resume.path,
    sha256: documents.resume.sha256,
  }];
  if (documents.coverLetter) receiptDocuments.push({
    kind: "cover_letter",
    storageKey: documents.coverLetter.path,
    sha256: documents.coverLetter.sha256,
  });
  const receipt = createApplicationReceipt({
    receiptId: `receipt-${input.runId}`,
    accountId: input.accountId,
    runId: input.runId,
    runner: "cloud",
    applicationIdentityId: input.applicationIdentityId,
    browserProfileId: input.browserProfileId,
    adapter: execution.adapter,
    adapterVersion: execution.adapterVersion,
    job: input.job,
    packet: input.packet,
    documents: receiptDocuments,
    events,
    result: execution.receipt,
    finalUrl: page.url(),
    screenshotKeys: [screenshotPath],
  });
  const receiptPath = join(runDirectory, "receipt.json");
  await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`, { mode: 0o600 });
  return { receipt: execution.receipt, receiptBundle: receipt, receiptPath };
}

async function validate(input: CloudRunRequest): Promise<void> {
  for (const [name, value] of Object.entries({
    accountId: input.accountId,
    applicationIdentityId: input.applicationIdentityId,
    browserSessionId: input.browserSessionId,
    runId: input.runId,
    applicationId: input.applicationId,
  })) {
    if (!/^[A-Za-z0-9_-]{3,160}$/.test(value)) throw new Error(`Invalid ${name}`);
  }
  if (input.packet.applicationId !== input.applicationId) throw new Error("Application bundle mismatch");
  if (input.packet.applicationIdentityId
    && input.packet.applicationIdentityId !== input.applicationIdentityId) throw new Error("Application email mismatch");
  await assertPublicApplicationUrl(input.url);
}

async function guardNavigations(context: BrowserContext): Promise<void> {
  await context.route("**/*", async (route) => {
    const request = route.request();
    if (!request.isNavigationRequest()) return route.continue();
    try {
      await assertPublicApplicationUrl(request.url());
      await route.continue();
    } catch {
      await route.abort("blockedbyclient");
    }
  });
}

function authorized(request: IncomingMessage): boolean {
  const supplied = Buffer.from((request.headers.authorization || "").replace(/^Bearer\s+/i, ""));
  const expected = Buffer.from(serviceToken);
  return supplied.length === expected.length && supplied.length > 0 && timingSafeEqual(supplied, expected);
}

async function body<T>(request: IncomingMessage): Promise<T> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    const value = Buffer.from(chunk);
    size += value.length;
    if (size > 5 * 1024 * 1024) throw new Error("Request is too large");
    chunks.push(value);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8")) as T;
}

function json(response: ServerResponse, status: number, value: unknown): void {
  response.writeHead(status, { "Content-Type": "application/json", "Cache-Control": "no-store" });
  response.end(JSON.stringify(value));
}

async function serialized<T>(scope: string, operation: () => Promise<T>): Promise<T> {
  const previous = locks.get(scope) || Promise.resolve();
  let release!: () => void;
  const current = new Promise<void>((resolve) => { release = resolve; });
  const queued = previous.then(() => current);
  locks.set(scope, queued);
  await previous;
  try {
    return await operation();
  } finally {
    release();
    if (locks.get(scope) === queued) locks.delete(scope);
  }
}
