import { app, BrowserWindow } from "electron";
import { chromium, type BrowserContext, type Page } from "playwright";
import { mkdir, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  PlaywrightBrowserPage,
  assertPublicApplicationUrl,
  createApplicationReceipt,
  executeApplication,
  materializeApplicationDocuments,
  submissionPolicy,
  type ApplicationPacket,
  type ExecutionResult,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import { identityContextKey, identityProfileDirectory } from "./profile.js";
import { parseBlueyJobsProtocol } from "./protocol.js";

interface StartRunRequest {
  accountId: string;
  applicationIdentityId: string;
  runId: string;
  applicationId: string;
  browserProfileId?: string;
  url: string;
  packet: ApplicationPacket;
  job?: NormalizedJob;
}

interface LocalRunDelivery {
  apiOrigin: string;
  ticket: string;
}

interface ActiveLocalRun {
  request: StartRunRequest;
  delivery: LocalRunDelivery;
  page: Page;
  events: Array<{ event: string; details: Record<string, unknown>; at: string }>;
}

let window: BrowserWindow | null = null;
const contexts = new Map<string, BrowserContext>();
const activeLocalRuns = new Map<string, ActiveLocalRun>();
const pendingProtocolUrls: string[] = [];
const singleInstance = app.requestSingleInstanceLock();

app.setName("Bluey Browser");
app.setAsDefaultProtocolClient("bluey-jobs");

if (!singleInstance) {
  app.quit();
} else {
  app.on("second-instance", (_event, commandLine) => {
    const protocolUrl = commandLine.find((argument) => argument.startsWith("bluey-jobs://"));
    if (protocolUrl) openProtocolUrl(protocolUrl);
    showWindow();
  });
  app.on("open-url", (event, url) => {
    event.preventDefault();
    openProtocolUrl(url);
  });
}

app.whenReady().then(async () => {
  window = new BrowserWindow({
    width: 520,
    height: 420,
    minWidth: 440,
    minHeight: 340,
    show: false,
    backgroundColor: "#070a0c",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  window.webContents.on("will-navigate", (event, url) => {
    if (!url.startsWith("bluey-jobs://")) return;
    event.preventDefault();
    void openProtocolUrl(url);
  });
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("bluey-jobs://")) void openProtocolUrl(url);
    return { action: "deny" };
  });
  window.once("ready-to-show", () => window?.show());
  await window.loadURL(statusPage());
  app.on("activate", showWindow);
  for (const url of pendingProtocolUrls.splice(0)) void openProtocolUrl(url);
});

async function executeLocalRequest(
  request: StartRunRequest,
  delivery: LocalRunDelivery,
  resume = false,
) {
  assertIdentifier(request.accountId, "accountId");
  assertIdentifier(request.applicationIdentityId, "applicationIdentityId");
  assertIdentifier(request.runId, "runId");
  assertIdentifier(request.applicationId, "applicationId");
  if (request.packet.applicationId !== request.applicationId) {
    throw new Error("Application bundle does not match this run");
  }
  if (request.packet.applicationIdentityId
    && request.packet.applicationIdentityId !== request.applicationIdentityId) {
    throw new Error("Application email does not match this browser profile");
  }
  const decision = submissionPolicy(request.url);
  if (decision.policy === "blocked") return { status: decision.policy, reason: decision.reason };
  await assertPublicApplicationUrl(request.url);
  const runDirectory = join(
    identityProfileDirectory(app.getPath("userData"), request.accountId, request.applicationIdentityId),
    "runs",
    request.runId,
  );
  await mkdir(runDirectory, { recursive: true });
  const documents = await materializeApplicationDocuments({
    ...request.packet,
    applicationIdentityId: request.applicationIdentityId,
    browserProfileId: request.browserProfileId
      || identityContextKey(request.accountId, request.applicationIdentityId),
  }, join(runDirectory, "documents"));
  request.packet = documents.packet;
  const active = activeLocalRuns.get(request.runId);
  if (resume && !active) throw new Error("This local application is no longer active");
  if (!active && [...activeLocalRuns.values()].some((run) => (
    run.request.applicationIdentityId === request.applicationIdentityId
  ))) {
    throw new Error("Finish the other application using this application email first");
  }
  const context = await contextFor(request.accountId, request.applicationIdentityId);
  const page = active?.page ?? await context.newPage();
  if (!resume) await page.goto(request.url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.bringToFront();
  const browserPage = new PlaywrightBrowserPage(page);
  const events = active?.events ?? [];
  const execution = decision.policy === "handoff"
    ? await handoffExecution(browserPage, decision.reason, resume)
    : await executeApplication({
        runner: "local",
        runId: request.runId,
        accountId: request.accountId,
        page: browserPage,
        packet: {
          ...request.packet,
          applicationIdentityId: request.applicationIdentityId,
          browserProfileId: request.browserProfileId
            || identityContextKey(request.accountId, request.applicationIdentityId),
        },
        async log(event, details = {}) {
          events.push({ event, details, at: new Date().toISOString() });
        },
      });
  if (execution.receipt.status === "needs_input" && execution.receipt.intervention) {
    execution.receipt.intervention.takeoverUrl = localResumeUrl(request.runId, delivery.ticket);
  }
  const job = request.job ?? await resolveJob(execution.adapter, browserPage);
  const screenshotPath = join(runDirectory, "final.png");
  await writeFile(screenshotPath, await browserPage.screenshot({ fullPage: true }), { mode: 0o600 });
  execution.receipt.screenshotPath = screenshotPath;
  const bundle = createApplicationReceipt({
    receiptId: `receipt-${request.runId}`,
    runId: request.runId,
    accountId: request.accountId,
    runner: "local",
    adapter: execution.adapter,
    adapterVersion: execution.adapterVersion,
    applicationIdentityId: request.applicationIdentityId,
    browserProfileId: request.browserProfileId
      || identityContextKey(request.accountId, request.applicationIdentityId),
    job,
    packet: request.packet,
    documents: [
      {
        kind: "resume",
        versionId: request.packet.resumeVersionId,
        storageKey: documents.resume.path,
        sha256: documents.resume.sha256,
      },
      ...(documents.coverLetter ? [{
        kind: "cover_letter" as const,
        storageKey: documents.coverLetter.path,
        sha256: documents.coverLetter.sha256,
      }] : []),
    ],
    result: execution.receipt,
    events: events.map((entry, index) => ({
      id: `${request.runId}:${index + 1}`,
      occurredAt: entry.at,
      type: entry.event,
      detail: entry.details,
    })),
    finalUrl: page.url(),
    screenshotKeys: [screenshotPath],
  });
  const receiptPath = join(runDirectory, "receipt.json");
  await writeFile(receiptPath, `${JSON.stringify(bundle, null, 2)}\n`, { mode: 0o600 });
  const finalTitle = await page.title();
  const finalUrl = page.url();
  const response = await fetch(`${delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(request.runId)}/result`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ticket: delivery.ticket, receipt: execution.receipt, receiptBundle: bundle }),
  });
  if (!response.ok) throw new Error(`Bluey could not save the local run (${response.status})`);
  if (execution.receipt.status === "needs_input") {
    activeLocalRuns.set(request.runId, { request, delivery, page, events });
    await showControllerPage(interventionPage(
      execution.receipt.intervention?.title || "Application needs your input",
      execution.receipt.intervention?.detail || "Complete this step in the application browser.",
      localResumeUrl(request.runId, delivery.ticket),
    ));
  } else {
    activeLocalRuns.delete(request.runId);
    await page.close().catch(() => undefined);
    await showControllerPage(completedPage(execution.receipt.status));
  }
  return {
    status: execution.receipt.status,
    runId: request.runId,
    applicationId: request.applicationId,
    applicationIdentityId: request.applicationIdentityId,
    receiptPath,
    receipt: execution.receipt,
    title: finalTitle,
    url: finalUrl,
  };
}

async function handoffExecution(
  page: PlaywrightBrowserPage,
  reason: string,
  resume: boolean,
): Promise<ExecutionResult> {
  const body = await page.bodyText();
  const confirmed = /application (?:has been |was )?(?:submitted|received)|thank(?:s| you) for applying/i.test(body)
    || /(?:thank|confirmation|submitted|success|complete)/i.test(new URL(page.url()).pathname);
  if (resume && confirmed) {
    return {
      adapter: "semantic",
      adapterVersion: "2026.07.1-handoff",
      receipt: {
        status: "submitted" as const,
        confirmationText: body.replace(/\s+/g, " ").trim().slice(0, 500),
        confirmationUrl: page.url(),
        submittedAt: new Date().toISOString(),
        issues: [],
      },
    };
  }
  return {
    adapter: "semantic",
    adapterVersion: "2026.07.1-handoff",
    receipt: {
      status: "needs_input" as const,
      issues: [],
      intervention: {
        kind: "browser_takeover" as const,
        title: "Finish on this job site",
        detail: reason,
        resolution: { kind: "browser_takeover" as const, resumeAfter: true },
      },
    },
  };
}

async function contextFor(accountId: string, applicationIdentityId: string): Promise<BrowserContext> {
  const contextKey = identityContextKey(accountId, applicationIdentityId);
  const existing = contexts.get(contextKey);
  if (existing) return existing;
  const profile = join(
    identityProfileDirectory(app.getPath("userData"), accountId, applicationIdentityId),
    "chromium-profile",
  );
  await mkdir(profile, { recursive: true });
  const executablePath = app.isPackaged ? await packagedChromiumExecutable() : undefined;
  const context = await chromium.launchPersistentContext(profile, {
    headless: false,
    ...(executablePath ? { executablePath } : { channel: "chromium" }),
    viewport: null,
    acceptDownloads: true,
  });
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
  contexts.set(contextKey, context);
  context.on("close", () => contexts.delete(contextKey));
  return context;
}

async function packagedChromiumExecutable(): Promise<string> {
  const root = join(process.resourcesPath, "playwright");
  const candidates = process.platform === "darwin"
    ? ["Chromium", "Google Chrome for Testing"]
    : process.platform === "win32"
      ? ["chrome.exe"]
      : ["chrome", "headless_shell"];
  const found = await findExecutable(root, new Set(candidates), 0);
  if (!found) throw new Error("Bluey Browser's bundled Chromium is missing");
  return found;
}

async function findExecutable(directory: string, names: Set<string>, depth: number): Promise<string | undefined> {
  if (depth > 7) return undefined;
  const entries = await readdir(directory, { withFileTypes: true }).catch(() => []);
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isFile() && names.has(entry.name)) return path;
    if (entry.isDirectory()) {
      const nested = await findExecutable(path, names, depth + 1);
      if (nested) return nested;
    }
  }
  return undefined;
}

async function resolveJob(adapter: string, page: PlaywrightBrowserPage): Promise<NormalizedJob> {
  const title = await page.title();
  const parts = title.split(/[|\-–—]/).map((value) => value.trim()).filter(Boolean);
  return {
    externalId: new URL(page.url()).pathname.split("/").filter(Boolean).at(-1) || page.url(),
    canonicalUrl: page.url(),
    company: parts.at(-1) || "Employer",
    title: parts[0] || "Open role",
    location: "",
    workplace: "unknown",
    description: "",
    source: adapter as NormalizedJob["source"],
  };
}


function assertIdentifier(value: string, label: string): void {
  if (!/^[A-Za-z0-9_-]{3,160}$/.test(value)) throw new Error(`Invalid ${label}`);
}

async function openProtocolUrl(rawUrl: string): Promise<void> {
  if (!app.isReady()) {
    pendingProtocolUrls.push(rawUrl);
    return;
  }
  try {
    const command = parseBlueyJobsProtocol(rawUrl);
    showWindow();
    if (command.action === "open" || command.action === "takeover") return;
    const { runId, ticket } = command;
    if (command.action === "run") {
      await showControllerPage(loadingPage("Opening your application"));
      const apiOrigin = jobsApiOrigin();
      const response = await fetch(`${apiOrigin}/api/jobs/local-runs/${encodeURIComponent(runId)}/claim`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ticket }),
      });
      if (!response.ok) throw new Error("This application launch expired. Start it again from Bluey Jobs.");
      const request = await response.json() as StartRunRequest;
      if (request.runId !== runId) throw new Error("The application launch did not match this run");
      const delivery = { apiOrigin, ticket };
      try {
        await executeLocalRequest(request, delivery);
      } catch (error) {
        await reportLocalFailure(request, delivery, error);
        throw error;
      }
      return;
    }
    if (command.action === "resume") {
      const active = activeLocalRuns.get(runId);
      if (!active || active.delivery.ticket !== ticket) {
        throw new Error("This application is no longer active. Start it again from Bluey Jobs.");
      }
      await showControllerPage(loadingPage("Checking the application"));
      try {
        await executeLocalRequest(active.request, active.delivery, true);
      } catch (error) {
        await reportLocalFailure(active.request, active.delivery, error);
        throw error;
      }
    }
  } catch (error) {
    await showControllerPage(errorPage(error instanceof Error ? error.message : "Bluey Browser could not open this application."));
  }
}

async function reportLocalFailure(
  request: StartRunRequest,
  delivery: LocalRunDelivery,
  error: unknown,
): Promise<void> {
  const active = activeLocalRuns.get(request.runId);
  activeLocalRuns.delete(request.runId);
  await active?.page.close().catch(() => undefined);
  const message = error instanceof Error ? error.message : "Bluey Browser could not finish this application.";
  await fetch(`${delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(request.runId)}/result`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      ticket: delivery.ticket,
      receipt: {
        status: "failed",
        issues: [{ field: "browser", message, severity: "blocking" }],
      },
    }),
  }).catch(() => undefined);
}

function showWindow(): void {
  if (!window) return;
  if (window.isMinimized()) window.restore();
  window.show();
  window.focus();
}

function statusPage(): string {
  return controllerPage(
    "Bluey Browser",
    "Your separate application profile is ready. Start a reviewed application from Bluey Jobs to open it here.",
    "Job-site sign-ins stay separate from your everyday browser.",
  );
}

function loadingPage(title: string): string {
  return controllerPage(title, "Bluey is preparing the exact resume and application for this job.", "Keep this window open.");
}

function interventionPage(title: string, detail: string, resumeUrl: string): string {
  return controllerPage(
    title,
    detail,
    "Complete the step in the application browser, then continue here.",
    `<a href="${escapeHtml(resumeUrl)}">Continue application</a>`,
  );
}

function completedPage(status: string): string {
  return status === "submitted"
    ? controllerPage("Application submitted", "Bluey saved the exact resume, answers, and confirmation in your application history.", "You can close this window.")
    : controllerPage("Application stopped", "Bluey could not complete this application. Open Jobs to review what needs attention.", "Your application history has the latest result.");
}

function errorPage(message: string): string {
  return controllerPage("Application did not open", message, "Return to Bluey Jobs and try again.");
}

function controllerPage(title: string, detail: string, footnote: string, action = ""): string {
  const html = `<!doctype html><html><meta charset="utf-8"><style>
  :root{color-scheme:dark;font-family:Inter,-apple-system,sans-serif;background:#070a0c;color:#f3f7f9}
  body{margin:0;padding:32px}main{border:1px solid #26323a;border-radius:8px;padding:24px;background:#0b0e10}
  b{color:#58d3ff}p{color:#9eabb3;line-height:1.5}small{color:#68747c;display:block;margin-top:16px}
  a{display:inline-block;margin-top:18px;padding:11px 16px;border-radius:6px;background:#72d8ff;color:#071017;text-decoration:none;font-weight:700}
  </style><body><main><b>bluey jobs</b><h1>${escapeHtml(title)}</h1><p>${escapeHtml(detail)}</p>${action}<small>${escapeHtml(footnote)}</small></main></body></html>`;
  return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`;
}

async function showControllerPage(page: string): Promise<void> {
  showWindow();
  await window?.loadURL(page);
}

function localResumeUrl(runId: string, ticket: string): string {
  return `bluey-jobs://resume/${encodeURIComponent(runId)}?ticket=${encodeURIComponent(ticket)}`;
}

function jobsApiOrigin(): string {
  const value = (process.env.BLUEY_JOBS_API_ORIGIN || "https://bluey.sh").replace(/\/$/, "");
  const url = new URL(value);
  if (url.protocol !== "https:" && !(url.protocol === "http:" && ["127.0.0.1", "localhost"].includes(url.hostname))) {
    throw new Error("Bluey Jobs API origin must use HTTPS");
  }
  return url.toString().replace(/\/$/, "");
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] || character);
}

app.on("before-quit", () => {
  for (const context of contexts.values()) void context.close();
});
