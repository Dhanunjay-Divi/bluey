import { createHash } from "node:crypto";
import { app, BrowserWindow } from "electron";
import { chromium, type BrowserContext, type Page } from "playwright";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  PlaywrightBrowserPage,
  assertPublicApplicationUrl,
  createDefaultAdapterRegistry,
  createApplicationReceipt,
  executeApplication,
  materializeApplicationDocuments,
  submissionPolicy,
  type ApplicationPacket,
  type EvidenceObjectUpload,
  type ExecutionResult,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import {
  durableFinalSubmitHooks,
  finalSubmitMarkerExists,
  recordReconciledSubmitConfirmation,
} from "./irreversible-submit.js";
import {
  installBrowserNetworkGuard,
  LOCAL_BROWSER_SERVICE_WORKERS,
} from "./browser-network.js";
import {
  classifyLocalFailure,
  LocalBrowserError,
  safeLocalFailure,
  type LocalFailureClassification,
} from "./local-failure.js";
import { localRunCapabilities, localRunRequestPayload } from "./local-capability.js";
import { identityContextKey, identityProfileDirectory } from "./profile.js";
import {
  isApprovedLocalResumeAction,
  localProviderFinalReview,
  pendingProviderReviewReceipt,
  providerOptionsForApprovedReview,
  reconcileLocalProviderConfirmation,
  type LocalProviderFinalReview,
} from "./provider-final-review.js";
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
  protocolTicket: string;
  resultCapability: string;
  resumeCapability: string;
}

interface ActiveLocalRun {
  request: StartRunRequest;
  delivery: LocalRunDelivery;
  page: Page;
  runDirectory: string;
  providerFinalReview?: LocalProviderFinalReview;
  approvedSubmitActionConsumed?: boolean;
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
    throw new LocalBrowserError("run_request_invalid");
  }
  if (request.packet.applicationIdentityId
    && request.packet.applicationIdentityId !== request.applicationIdentityId) {
    throw new LocalBrowserError("identity_mismatch");
  }
  const decision = submissionPolicy(request.url);
  if (decision.policy === "blocked") return { status: decision.policy, reason: decision.reason };
  await assertPublicApplicationUrl(request.url);
  const runDirectory = localRunDirectory(request);
  await mkdir(runDirectory, { recursive: true });
  if (!resume && await finalSubmitMarkerExists(runDirectory)) {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
  const documents = await materializeApplicationDocuments({
    ...request.packet,
    applicationIdentityId: request.applicationIdentityId,
    browserProfileId: request.browserProfileId
      || identityContextKey(request.accountId, request.applicationIdentityId),
  }, join(runDirectory, "documents"));
  request.packet = documents.packet;
  const active = activeLocalRuns.get(request.runId);
  if (resume && !active) throw new LocalBrowserError("run_not_active");
  if (!active && [...activeLocalRuns.values()].some((run) => (
    run.request.applicationIdentityId === request.applicationIdentityId
  ))) {
    throw new LocalBrowserError("identity_busy");
  }
  const context = await contextFor(request.accountId, request.applicationIdentityId);
  const page = active?.page ?? await context.newPage();
  const events = active?.events ?? [];
  if (!active) {
    activeLocalRuns.set(request.runId, { request, delivery, page, runDirectory, events });
  }
  if (!resume) await page.goto(request.url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.bringToFront();
  const browserPage = new PlaywrightBrowserPage(page);
  const finalSubmitHooks = durableFinalSubmitHooks(runDirectory);
  let execution: ExecutionResult | undefined;
  let approvedProviderReview: LocalProviderFinalReview | undefined;
  if (resume && active?.providerFinalReview) {
    // The resume capability proves run access, not server-side submit approval.
    execution = reconcileLocalProviderConfirmation(
      active.providerFinalReview,
      await browserPage.bodyText(),
      browserPage.url(),
    );
    if (execution && !await finalSubmitMarkerExists(runDirectory)) {
      try {
        await recordReconciledSubmitConfirmation(runDirectory);
      } catch {
        throw new LocalBrowserError("submit_outcome_unknown");
      }
    }
    if (!execution && await finalSubmitMarkerExists(runDirectory)) {
      throw new LocalBrowserError("submit_outcome_unknown");
    }
    if (!active.approvedSubmitActionConsumed) {
      const approved = await consumeApprovedLocalSubmitAction(request.runId, delivery);
      if (approved) active.approvedSubmitActionConsumed = true;
      if (!approved) {
        const pending = pendingProviderReviewReceipt();
        pending.intervention!.takeoverUrl = localResumeUrl(request.runId, delivery.protocolTicket);
        await showControllerPage(interventionPage(
          pending.intervention!.title,
          pending.intervention!.detail,
          localResumeUrl(request.runId, delivery.protocolTicket),
        ));
        return {
          status: pending.status,
          runId: request.runId,
          applicationId: request.applicationId,
          applicationIdentityId: request.applicationIdentityId,
          receipt: pending,
        };
      }
    }
    if (!execution) {
      approvedProviderReview = active.providerFinalReview;
    }
  }
  if (!execution && decision.policy === "handoff") {
    execution = await handoffExecution(browserPage, decision.reason, resume);
  } else if (!execution) {
    const adapterContext = {
      runner: "local" as const,
      runId: request.runId,
      accountId: request.accountId,
      page: browserPage,
      packet: {
        ...request.packet,
        applicationIdentityId: request.applicationIdentityId,
        browserProfileId: request.browserProfileId
          || identityContextKey(request.accountId, request.applicationIdentityId),
      },
      async log(event: string, details: Record<string, unknown> = {}) {
        events.push({ event, details, at: new Date().toISOString() });
      },
      beforeFinalSubmit: finalSubmitHooks.beforeFinalSubmit,
      afterFinalSubmit: finalSubmitHooks.afterFinalSubmit,
    };
    execution = approvedProviderReview
      ? await executeApplication(
          adapterContext,
          createDefaultAdapterRegistry(undefined, providerOptionsForApprovedReview(approvedProviderReview)),
        )
      : await executeApplication(adapterContext);
  }
  const providerReview = localProviderFinalReview(execution);
  if (providerReview) {
    const current = activeLocalRuns.get(request.runId);
    if (current) current.providerFinalReview = providerReview;
  }
  const markerExists = await finalSubmitMarkerExists(runDirectory);
  if (execution.receipt.status === "submitted" && !markerExists) {
    try {
      await recordReconciledSubmitConfirmation(runDirectory);
    } catch {
      throw new LocalBrowserError("submit_outcome_unknown");
    }
  } else if (execution.receipt.status !== "submitted" && markerExists) {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
  if (execution.receipt.status === "needs_input" && execution.receipt.intervention) {
    execution.receipt.intervention.takeoverUrl = localResumeUrl(request.runId, delivery.protocolTicket);
  }
  const job = request.job ?? await resolveJob(execution.adapter, browserPage);
  const screenshotPath = join(runDirectory, "final.png");
  const screenshotBytes = Buffer.from(await browserPage.screenshot({ fullPage: true }));
  await writeFile(screenshotPath, screenshotBytes, { mode: 0o600 });
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
  const finalTitle = await page.title();
  const finalUrl = page.url();
  const response = await fetch(`${delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(request.runId)}/result`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      capability: delivery.resultCapability,
      receipt: execution.receipt,
      receiptBundle: bundle,
      evidenceObjects,
    }),
  });
  if (!response.ok) throw new LocalBrowserError("result_delivery_failed");
  if (execution.receipt.status === "needs_input") {
    await showControllerPage(interventionPage(
      execution.receipt.intervention?.title || "Application needs your input",
      execution.receipt.intervention?.detail || "Complete this step in the application browser.",
      localResumeUrl(request.runId, delivery.protocolTicket),
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
    serviceWorkers: LOCAL_BROWSER_SERVICE_WORKERS,
  });
  try {
    await installBrowserNetworkGuard(context);
  } catch {
    await context.close().catch(() => undefined);
    throw new LocalBrowserError("configuration_invalid");
  }
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
  if (!found) throw new LocalBrowserError("configuration_invalid");
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


function localRunDirectory(request: StartRunRequest): string {
  return join(
    identityProfileDirectory(app.getPath("userData"), request.accountId, request.applicationIdentityId),
    "runs",
    request.runId,
  );
}

function localRunDirectoryIfValid(request: StartRunRequest): string | undefined {
  return identifiersAreValid(request.accountId, request.applicationIdentityId, request.runId)
    ? localRunDirectory(request)
    : undefined;
}

function identifiersAreValid(...values: unknown[]): boolean {
  return values.every((value) => typeof value === "string" && /^[A-Za-z0-9_-]{3,160}$/.test(value));
}

function assertIdentifier(value: string, _label: string): void {
  if (!identifiersAreValid(value)) throw new LocalBrowserError("run_request_invalid");
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
      if (!response.ok) throw new LocalBrowserError("launch_expired");
      const claim = await response.json() as unknown;
      const capabilities = localRunCapabilities(claim, runId);
      const request = localRunRequestPayload(claim) as unknown as StartRunRequest;
      if (request.runId !== runId) throw new LocalBrowserError("launch_mismatch");
      const delivery: LocalRunDelivery = {
        apiOrigin,
        protocolTicket: ticket,
        resultCapability: capabilities.result,
        resumeCapability: capabilities.resume,
      };
      try {
        await executeLocalRequest(request, delivery);
      } catch (error) {
        const failure = await reportLocalFailure(request, delivery, error);
        await showLocalFailurePage(request.runId, delivery.protocolTicket, failure);
      }
      return;
    }
    if (command.action === "resume") {
      const active = activeLocalRuns.get(runId);
      if (!active || active.delivery.protocolTicket !== ticket) {
        throw new LocalBrowserError("run_not_active");
      }
      await showControllerPage(loadingPage("Checking the application"));
      try {
        await executeLocalRequest(active.request, active.delivery, true);
      } catch (error) {
        const failure = await reportLocalFailure(active.request, active.delivery, error);
        await showLocalFailurePage(runId, ticket, failure);
      }
    }
  } catch (error) {
    await showControllerPage(errorPage(safeLocalFailure(error).message));
  }
}

async function reportLocalFailure(
  request: StartRunRequest,
  delivery: LocalRunDelivery,
  error: unknown,
): Promise<LocalFailureClassification> {
  const active = activeLocalRuns.get(request.runId);
  const failure = await classifyLocalFailure(
    active?.runDirectory ?? localRunDirectoryIfValid(request),
    error,
  );
  if (!failure.preservePage) {
    activeLocalRuns.delete(request.runId);
    await active?.page.close().catch(() => undefined);
  }
  const intervention = failure.status === "side_effect_unknown" && active
    ? {
        kind: "browser_takeover",
        title: "Confirm the application result",
        detail: failure.message,
        takeoverUrl: localResumeUrl(request.runId, delivery.protocolTicket),
        resolution: { kind: "browser_takeover", resumeAfter: true },
      }
    : undefined;
  await fetch(`${delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(request.runId)}/result`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      capability: delivery.resultCapability,
      receipt: {
        status: failure.status,
        errorCode: failure.code,
        issues: [{ field: "browser", message: failure.message, severity: "blocking" }],
        ...(intervention ? { intervention } : {}),
      },
    }),
  }).catch(() => undefined);
  return failure;
}

async function consumeApprovedLocalSubmitAction(
  runId: string,
  delivery: LocalRunDelivery,
): Promise<boolean> {
  try {
    const response = await fetch(
      `${delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(runId)}/resume`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ capability: delivery.resumeCapability }),
      },
    );
    if (!response.ok) return false;
    return isApprovedLocalResumeAction(await response.json(), runId);
  } catch {
    return false;
  }
}

async function showLocalFailurePage(
  runId: string,
  ticket: string,
  failure: LocalFailureClassification,
): Promise<void> {
  if (failure.status === "side_effect_unknown" && activeLocalRuns.has(runId)) {
    await showControllerPage(interventionPage(
      "Confirm the application result",
      failure.message,
      localResumeUrl(runId, ticket),
    ));
    return;
  }
  await showControllerPage(errorPage(failure.message));
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
    throw new LocalBrowserError("configuration_invalid");
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
