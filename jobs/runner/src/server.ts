import { createHash, timingSafeEqual } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { mkdir, readFile, writeFile } from "node:fs/promises";
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
  type EvidenceObjectUpload,
  type NormalizedJob,
  type ReceiptDocument,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import { installBrowserNetworkGuard } from "./browser-network-guard.js";
import {
  createExecutionLeaseClientFromEnv,
  ExecutionLeaseError,
  type ActiveExecutionLease,
} from "./execution-lease.js";
import {
  abortLeasedRun,
  beginLeasedRun,
  finalizeLeasedRun,
  LeasedRunError,
  terminalLeaseOutcome,
} from "./leased-run.js";
import { RunnerEncryptionError } from "./crypto-envelope.js";
import { parseProfileKey, profilePaths, restoreProfile, sealProfile } from "./profile-store.js";
import { providerRegistryForResumeAction } from "./resume-policy.js";
import { readResult, ResultStoreError, stageResult, writeResult } from "./result-store.js";

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
  profileScope: string;
  action?: string;
  field?: string;
  answer?: string;
}

type RunEvent = { id: string; occurredAt: string; type: string; detail?: Record<string, unknown> };
type ExecutedRunResult = Awaited<ReturnType<typeof executeRun>>;
type CloudRunResult = {
  receipt: SubmissionReceipt;
  receiptBundle?: ExecutedRunResult["receiptBundle"];
  evidenceObjects?: EvidenceObjectUpload[];
  receiptPath?: string;
};

interface BrowserRunExecution {
  result: CloudRunResult;
  events: RunEvent[];
  context?: BrowserContext;
  keepActive: boolean;
}

const port = Number(process.env.PORT || 8091);
const root = process.env.BLUEY_JOBS_RUNNER_DATA || "/tmp/bluey-jobs-runner";
const serviceToken = process.env.BLUEY_JOBS_RUNNER_TOKEN || "";
const profileKey = process.env.BLUEY_JOBS_PROFILE_ENCRYPTION_KEY
  ? parseProfileKey(process.env.BLUEY_JOBS_PROFILE_ENCRYPTION_KEY)
  : undefined;
const leaseClient = createExecutionLeaseClientFromEnv();
const locks = new Map<string, Promise<void>>();
const activeRuns = new Map<string, {
  context: BrowserContext;
  paths: ReturnType<typeof profilePaths>;
  input: CloudRunRequest;
  events: RunEvent[];
  lease: ActiveExecutionLease;
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
      const paths = profilePaths(root, input.accountId, input.applicationIdentityId);
      const resultContext = { requestId, profileScope: paths.scope };
      const completed = await readResult<CloudRunResult>(root, resultContext, profileKey);
      if (completed) return json(response, 200, completed);
      const result = await serialized(paths.scope, async () => {
        const existing = await readResult<CloudRunResult>(root, resultContext, profileKey);
        if (existing) return existing;
        const { lease, execution } = await beginLeasedRun(
          leaseClient,
          {
            accountId: input.accountId,
            applicationId: input.applicationId,
            runId: input.runId,
            browserProfileId: input.browserProfileId,
          },
          (activeLease) => run(input, paths, activeLease),
          (activeLease) => abortLeasedRun(activeLease, async () => {}),
        );
        if (execution.keepActive) {
          try {
            await writeResult(root, resultContext, execution.result, profileKey);
          } catch {
            return abortLeasedRun(lease, () => closeBrowserExecution(execution.context, paths));
          }
          if (!execution.context) return abortLeasedRun(lease, async () => {});
          activeScopes.add(paths.scope);
          activeRuns.set(input.browserSessionId, {
            context: execution.context,
            paths,
            input,
            events: execution.events,
            lease,
          });
          return execution.result;
        }

        const outcome = terminalLeaseOutcome(execution.result.receipt.status, lease);
        if (outcome === "side_effect_unknown") {
          return abortLeasedRun(lease, () => closeBrowserExecution(execution.context, paths));
        }
        return finalizeLeasedRun({
          lease,
          intendedOutcome: outcome,
          cleanup: () => closeBrowserExecution(execution.context, paths),
          async stage() {
            await stageResult(root, resultContext, execution.result, profileKey);
          },
          async commit() {
            await writeResult(root, resultContext, execution.result, profileKey);
            return execution.result;
          },
        });
      });
      return json(response, 200, result);
    }
    const resume = request.url?.match(/^\/runs\/([A-Za-z0-9_-]{3,160})\/resume$/);
    if (resume && request.method === "POST") {
      const resolution = await body<RunResolution>(request);
      if (!/^[A-Za-z0-9:_-]{3,240}$/.test(resolution.requestId || "")) {
        return json(response, 400, { error: "A valid request ID is required" });
      }
      if (!/^[a-f0-9]{40}$/.test(resolution.profileScope || "")) {
        return json(response, 400, { error: "A valid profile scope is required" });
      }
      const resultContext = {
        requestId: resolution.requestId,
        profileScope: resolution.profileScope,
      };
      const completed = await readResult<CloudRunResult>(root, resultContext, profileKey);
      if (completed) return json(response, 200, completed);
      const active = activeRuns.get(resume[1]);
      if (!active || active.paths.scope !== resolution.profileScope) {
        return json(response, 404, { error: "Browser run not found" });
      }
      const result = await serialized(active.paths.scope, async () => {
        const existing = await readResult<CloudRunResult>(root, resultContext, profileKey);
        if (existing) return existing;
        if (activeRuns.get(resume[1]) !== active) throw new Error("Browser run is no longer active");
        if (resolution.field && resolution.answer) {
          active.input.packet.answers[resolution.field] = resolution.answer;
        }
        try {
          const executed = await executeRun(
            active.input,
            active.paths,
            active.context,
            active.events,
            active.lease,
            false,
            resolution.action,
          );
          if (executed.receipt.status === "needs_input" && !active.lease.finalSubmitAttempted) {
            await writeResult(root, resultContext, executed, profileKey);
            return executed;
          }

          const outcome = terminalLeaseOutcome(executed.receipt.status, active.lease);
          if (outcome === "side_effect_unknown") {
            return abortLeasedRun(active.lease, () => closeActiveRun(resume[1], active));
          }
          return finalizeLeasedRun({
            lease: active.lease,
            intendedOutcome: outcome,
            cleanup: () => closeActiveRun(resume[1], active),
            async stage() {
              await stageResult(root, resultContext, executed, profileKey);
            },
            async commit() {
              await writeResult(root, resultContext, executed, profileKey);
              return executed;
            },
          });
        } catch (error) {
          if (error instanceof LeasedRunError) throw error;
          return abortLeasedRun(active.lease, () => closeActiveRun(resume[1], active));
        }
      });
      return json(response, 200, result);
    }
    const release = request.url?.match(/^\/runs\/([A-Za-z0-9_-]{3,160})$/);
    if (release && request.method === "DELETE") {
      const active = activeRuns.get(release[1]);
      if (!active) return json(response, 204, {});
      await serialized(active.paths.scope, async () => {
        if (activeRuns.get(release[1]) !== active) return;
        await finalizeLeasedRun({
          lease: active.lease,
          intendedOutcome: "released",
          cleanup: () => closeActiveRun(release[1], active),
          async stage() {},
          async commit() {},
        });
      });
      return json(response, 204, {});
    }
    return json(response, 404, { error: "Not found" });
  } catch (error) {
    const failure = publicRunnerFailure(error);
    console.error("Bluey Jobs runner request failed", { code: failure.code });
    return json(response, failure.status, { error: failure.message });
  }
}).listen(port, "0.0.0.0", () => {
  console.log(`Bluey Jobs runner listening on ${port}`);
});

async function run(
  input: CloudRunRequest,
  paths: ReturnType<typeof profilePaths>,
  lease: ActiveExecutionLease,
): Promise<BrowserRunExecution> {
  const events: RunEvent[] = [];
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
    return { result: { receipt }, events, keepActive: false };
  }

  if (activeScopes.has(paths.scope)) throw new Error("This application email already has an active browser run");
  if (activeRuns.has(input.browserSessionId)) throw new Error("This browser session is already active");
  let context: BrowserContext | undefined;
  let profileRestored = false;
  try {
    await restoreProfile(paths, profileKey!);
    profileRestored = true;
    context = await chromium.launchPersistentContext(paths.directory, {
      headless: true,
      acceptDownloads: true,
      serviceWorkers: "block",
      viewport: { width: 1440, height: 1000 },
    });
    await installBrowserNetworkGuard(context);
    const result = await executeRun(input, paths, context, events, lease, true);
    return {
      result,
      events,
      context,
      keepActive: result.receipt.status === "needs_input" && !lease.finalSubmitAttempted,
    };
  } catch {
    if (context) {
      await closeBrowserExecution(context, paths).catch(() => undefined);
    } else if (profileRestored) {
      await sealProfile(paths, profileKey!).catch(() => undefined);
    }
    throw new Error("Browser execution failed");
  }
}

async function executeRun(
  input: CloudRunRequest,
  paths: ReturnType<typeof profilePaths>,
  context: BrowserContext,
  events: RunEvent[],
  lease: ActiveExecutionLease,
  navigate: boolean,
  resumeAction?: string,
) {
  const runDirectory = join(root, "receipts", paths.scope, input.runId);
  await mkdir(runDirectory, { recursive: true });
  const documents = await materializeApplicationDocuments(input.packet, join(runDirectory, "documents"));
  input.packet = documents.packet;
  const page = context.pages()[0] || await context.newPage();
  if (navigate) await page.goto(input.url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  const browserPage = new PlaywrightBrowserPage(page);
  const adapterContext = {
    runner: "cloud",
    runId: input.runId,
    accountId: input.accountId,
    page: browserPage,
    packet: {
      ...input.packet,
      applicationIdentityId: input.applicationIdentityId,
      browserProfileId: input.browserProfileId,
    },
    async log(type: string, detail: Record<string, unknown> = {}) {
      events.push({ id: `${input.runId}:${events.length + 1}`, occurredAt: new Date().toISOString(), type, detail });
    },
    beforeFinalSubmit: () => lease.beforeFinalSubmit(),
    afterFinalSubmit: (outcome: "activated" | "activation_uncertain") => lease.afterFinalSubmit(outcome),
  } as const;
  const providerRegistry = providerRegistryForResumeAction(resumeAction);
  const execution = providerRegistry
    ? await executeApplication(adapterContext, providerRegistry)
    : await executeApplication(adapterContext);
  const screenshotPath = join(runDirectory, "final.png");
  const screenshotBytes = Buffer.from(await browserPage.screenshot({ fullPage: true }));
  await writeFile(screenshotPath, screenshotBytes, { mode: 0o600 });
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
  const evidenceObjects: EvidenceObjectUpload[] = [
    await evidenceObject(documents.resume.path, "resume", "application/pdf", documents.resume.sha256),
    ...(documents.coverLetter ? [await evidenceObject(
      documents.coverLetter.path,
      "cover_letter",
      "application/pdf",
      documents.coverLetter.sha256,
    )] : []),
    {
      original_key: screenshotPath,
      kind: "screenshot",
      media_type: "image/png",
      sha256: createHash("sha256").update(screenshotBytes).digest("hex"),
      bytes_base64: screenshotBytes.toString("base64"),
    },
  ];
  return { receipt: execution.receipt, receiptBundle: receipt, evidenceObjects, receiptPath };
}

async function evidenceObject(
  path: string,
  kind: "resume" | "cover_letter" | "attachment",
  mediaType: string,
  sha256: string,
): Promise<EvidenceObjectUpload> {
  return {
    original_key: path,
    kind,
    media_type: mediaType,
    sha256,
    bytes_base64: (await readFile(path)).toString("base64"),
  };
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
  if (!/^[A-Za-z0-9:_-]{3,160}$/.test(input.browserProfileId)) throw new Error("Invalid browserProfileId");
  if (input.packet.applicationId !== input.applicationId) throw new Error("Application bundle mismatch");
  if (input.packet.applicationIdentityId
    && input.packet.applicationIdentityId !== input.applicationIdentityId) throw new Error("Application email mismatch");
  if (input.packet.browserProfileId
    && input.packet.browserProfileId !== input.browserProfileId) throw new Error("Browser profile mismatch");
  await assertPublicApplicationUrl(input.url);
}

async function closeBrowserExecution(
  context: BrowserContext | undefined,
  paths: ReturnType<typeof profilePaths>,
): Promise<void> {
  if (!context) return;
  let failed = false;
  try {
    await context.close();
  } catch {
    failed = true;
  }
  try {
    await sealProfile(paths, profileKey!);
  } catch {
    failed = true;
  }
  if (failed) throw new Error("Browser cleanup failed");
}

async function closeActiveRun(
  browserSessionId: string,
  active: NonNullable<ReturnType<typeof activeRuns.get>>,
): Promise<void> {
  try {
    await closeBrowserExecution(active.context, active.paths);
  } finally {
    if (activeRuns.get(browserSessionId) === active) activeRuns.delete(browserSessionId);
    activeScopes.delete(active.paths.scope);
  }
}

function publicRunnerFailure(error: unknown): { status: number; code: string; message: string } {
  if (error instanceof RunnerEncryptionError) {
    return {
      status: 500,
      code: `encryption_${error.code}`,
      message: "The application runner could not read encrypted state.",
    };
  }
  if (error instanceof ResultStoreError) {
    return {
      status: 500,
      code: `result_store_${error.code}`,
      message: "The application runner could not read durable result state.",
    };
  }
  if (error instanceof ExecutionLeaseError && error.code === "lease_unavailable") {
    return { status: 409, code: "lease_unavailable", message: "This run is already active." };
  }
  if (error instanceof LeasedRunError) {
    return {
      status: 500,
      code: error.outcome,
      message: "The application runner could not safely finish this run.",
    };
  }
  if (error instanceof ExecutionLeaseError) {
    return { status: 503, code: `lease_${error.code}`, message: "The application runner is temporarily unavailable." };
  }
  return { status: 500, code: "runner_failed", message: "The application runner could not finish this run." };
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
