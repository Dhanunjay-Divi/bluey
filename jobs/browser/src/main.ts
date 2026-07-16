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
  restartDisposition,
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
import {
  parseLocalRunClaim,
  scopedLocalRunAuthorization,
  type LocalRunCapabilities,
  type LocalRunCapabilityOperation,
} from "./local-capabilities.js";
import {
  LocalCheckpointStore,
  type LocalRunCheckpoint,
} from "./local-checkpoint-store.js";
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
  capabilities: LocalRunCapabilities;
}

interface ActiveLocalRun {
  request: StartRunRequest;
  delivery: LocalRunDelivery;
  page: Page;
  runDirectory: string;
  checkpointScope: string;
  checkpointCreatedAtMs: number;
  providerFinalReview?: LocalProviderFinalReview;
  approvedSubmitActionConsumed?: boolean;
  events: Array<{ event: string; details: Record<string, unknown>; at: string }>;
}

let window: BrowserWindow | null = null;
let checkpointStore: LocalCheckpointStore | undefined;
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
  checkpointStore = await LocalCheckpointStore.open(app.getPath("userData"));
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
  await reconcileLocalRunCheckpoints();
  for (const url of pendingProtocolUrls.splice(0)) void openProtocolUrl(url);
}).catch(() => {
  console.error("Bluey Browser startup failed", { code: "checkpoint_reconciliation_failed" });
  app.quit();
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
  const active = activeLocalRuns.get(request.runId);
  if (resume && !active) throw new LocalBrowserError("run_not_active");
  if (!active && [...activeLocalRuns.values()].some((run) => (
    run.request.applicationIdentityId === request.applicationIdentityId
  ))) {
    throw new LocalBrowserError("identity_busy");
  }
  const checkpointCreatedAtMs = active?.checkpointCreatedAtMs ?? Date.now();
  const checkpointScope = active?.checkpointScope ?? requiredCheckpointStore().scopeFor(request);
  if (!resume) {
    await writeLocalCheckpoint({
      request,
      delivery,
      checkpointScope,
      checkpointCreatedAtMs,
      phase: "prepared",
      status: "prepared",
      browserUrl: request.url,
    });
  }
  const documents = await materializeApplicationDocuments({
    ...request.packet,
    applicationIdentityId: request.applicationIdentityId,
    browserProfileId: request.browserProfileId
      || identityContextKey(request.accountId, request.applicationIdentityId),
  }, join(runDirectory, "documents"));
  request.packet = documents.packet;
  const context = await contextFor(request.accountId, request.applicationIdentityId);
  const page = active?.page ?? await context.newPage();
  const events = active?.events ?? [];
  if (!active) {
    activeLocalRuns.set(request.runId, {
      request,
      delivery,
      page,
      runDirectory,
      checkpointScope,
      checkpointCreatedAtMs,
      events,
    });
  }
  if (!resume) await page.goto(request.url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.bringToFront();
  await checkpointActiveLocalRun(request.runId, "prepared", "prepared");
  const browserPage = new PlaywrightBrowserPage(page);
  const durableHooks = durableFinalSubmitHooks(runDirectory);
  const finalSubmitHooks = {
    async beforeFinalSubmit() {
      // The exclusive marker is written first. A crash before the encrypted
      // checkpoint update still fails closed during startup reconciliation.
      await durableHooks.beforeFinalSubmit();
      await checkpointActiveLocalRun(request.runId, "final_submit_started", "side_effect_unknown");
    },
    async afterFinalSubmit(outcome: "activated" | "activation_uncertain") {
      await durableHooks.afterFinalSubmit(outcome);
      await checkpointActiveLocalRun(
        request.runId,
        outcome === "activated" ? "final_submit_activated" : "side_effect_unknown",
        "side_effect_unknown",
      );
    },
  };
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
    if (!execution && !active.approvedSubmitActionConsumed) {
      const approved = await consumeApprovedLocalSubmitAction(request.runId, delivery);
      if (approved) {
        active.approvedSubmitActionConsumed = true;
        await checkpointActiveLocalRun(request.runId, "provider_review", "provider_review");
      }
      if (!approved) {
        const pending = pendingProviderReviewReceipt();
        pending.intervention!.takeoverUrl = localResumeUrl(request.runId, delivery);
        await showControllerPage(interventionPage(
          pending.intervention!.title,
          pending.intervention!.detail,
          localResumeUrl(request.runId, delivery),
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
    await checkpointActiveLocalRun(
      request.runId,
      "provider_review",
      "provider_review",
      execution.adapter,
    );
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
    execution.receipt.intervention.takeoverUrl = localResumeUrl(request.runId, delivery);
  }
  if (execution.receipt.status === "needs_input") {
    await checkpointActiveLocalRun(
      request.runId,
      providerReview ? "provider_review" : "needs_input",
      providerReview ? "provider_review" : "needs_input",
      execution.adapter,
    );
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
      ...localRunAuthorization(delivery, "result"),
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
      localResumeUrl(request.runId, delivery),
    ));
  } else {
    activeLocalRuns.delete(request.runId);
    await page.close().catch(() => undefined);
    await requiredCheckpointStore().remove(checkpointScope);
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
    if (command.action === "open") return;
    if (command.action === "run") {
      const { runId, ticket } = command;
      await showControllerPage(loadingPage("Opening your application"));
      const apiOrigin = jobsApiOrigin();
      const response = await fetch(`${apiOrigin}/api/jobs/local-runs/${encodeURIComponent(runId)}/claim`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ticket }),
      });
      if (!response.ok) throw new LocalBrowserError("launch_expired");
      let claimed: ReturnType<typeof parseLocalRunClaim<StartRunRequest>>;
      try {
        claimed = parseLocalRunClaim<StartRunRequest>(await response.json(), runId);
      } catch {
        throw new LocalBrowserError("launch_expired");
      }
      const { request, capabilities } = claimed;
      const delivery = { apiOrigin, capabilities };
      try {
        await executeLocalRequest(request, delivery);
      } catch (error) {
        const failure = await reportLocalFailure(request, delivery, error);
        await showLocalFailurePage(request.runId, delivery, failure);
      }
      return;
    }
    if (command.action === "resume") {
      const { runId, capability } = command;
      const active = activeLocalRuns.get(runId);
      if (!active || active.delivery.capabilities.resume !== capability) {
        throw new LocalBrowserError("run_not_active");
      }
      await showControllerPage(loadingPage("Checking the application"));
      try {
        await executeLocalRequest(active.request, active.delivery, true);
      } catch (error) {
        const failure = await reportLocalFailure(active.request, active.delivery, error);
        await showLocalFailurePage(runId, active.delivery, failure);
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
  if (active && failure.status === "side_effect_unknown") {
    await checkpointActiveLocalRun(
      request.runId,
      "side_effect_unknown",
      "side_effect_unknown",
    ).catch(() => undefined);
  }
  if (!failure.preservePage) {
    activeLocalRuns.delete(request.runId);
    await active?.page.close().catch(() => undefined);
  }
  const intervention = failure.status === "side_effect_unknown" && active
    ? {
        kind: "browser_takeover",
        title: "Confirm the application result",
        detail: failure.message,
        takeoverUrl: localResumeUrl(request.runId, delivery),
        resolution: { kind: "browser_takeover", resumeAfter: true },
      }
    : undefined;
  try {
    const response = await fetch(`${delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(request.runId)}/result`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ...localRunAuthorization(delivery, "result"),
        receipt: {
          status: failure.status,
          errorCode: failure.code,
          issues: [{ field: "browser", message: failure.message, severity: "blocking" }],
          ...(intervention ? { intervention } : {}),
        },
      }),
    });
    if (response.ok && failure.status === "failed") {
      const checkpointScope = active?.checkpointScope
        ?? requiredCheckpointStore().scopeFor(request);
      await requiredCheckpointStore().remove(checkpointScope);
    }
  } catch {
    // Failure reporting must never fall back to the root claim ticket.
  }
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
        body: JSON.stringify(localRunAuthorization(delivery, "resume")),
      },
    );
    if (!response.ok) return false;
    return isApprovedLocalResumeAction(await response.json(), runId);
  } catch {
    return false;
  }
}

function requiredCheckpointStore(): LocalCheckpointStore {
  if (!checkpointStore) throw new LocalBrowserError("configuration_invalid");
  return checkpointStore;
}

async function checkpointActiveLocalRun(
  runId: string,
  phase: LocalRunCheckpoint["phase"],
  status: LocalRunCheckpoint["workflow"]["status"],
  adapter?: string,
): Promise<void> {
  const active = activeLocalRuns.get(runId);
  if (!active) throw new LocalBrowserError("run_not_active");
  await writeLocalCheckpoint({
    request: active.request,
    delivery: active.delivery,
    checkpointScope: active.checkpointScope,
    checkpointCreatedAtMs: active.checkpointCreatedAtMs,
    phase,
    status,
    browserUrl: active.page.url() || active.request.url,
    adapter,
    providerFinalReview: active.providerFinalReview,
  });
}

async function writeLocalCheckpoint(input: {
  request: StartRunRequest;
  delivery: LocalRunDelivery;
  checkpointScope: string;
  checkpointCreatedAtMs: number;
  phase: LocalRunCheckpoint["phase"];
  status: LocalRunCheckpoint["workflow"]["status"];
  browserUrl: string;
  adapter?: string;
  providerFinalReview?: LocalProviderFinalReview;
}): Promise<void> {
  const now = Date.now();
  const checkpoint: LocalRunCheckpoint<StartRunRequest, LocalProviderFinalReview> = {
    version: 1,
    phase: input.phase,
    createdAtMs: input.checkpointCreatedAtMs,
    updatedAtMs: now,
    expiresAtMs: input.delivery.capabilities.expiresAtMs,
    request: input.request,
    delivery: input.delivery,
    browser: { url: input.browserUrl },
    workflow: {
      status: input.status,
      ...(input.adapter ? { adapter: input.adapter } : {}),
      ...(activeLocalRuns.get(input.request.runId)?.approvedSubmitActionConsumed
        ? { approvedSubmitActionConsumed: true }
        : {}),
    },
    ...(input.providerFinalReview ? { providerFinalReview: input.providerFinalReview } : {}),
    events: activeLocalRuns.get(input.request.runId)?.events ?? [],
  };
  if (requiredCheckpointStore().scopeFor(input.request) !== input.checkpointScope) {
    throw new LocalBrowserError("run_request_invalid");
  }
  await requiredCheckpointStore().write(checkpoint);
}

async function reconcileLocalRunCheckpoints(): Promise<void> {
  const store = requiredCheckpointStore();
  const checkpoints = await store.list<StartRunRequest, LocalProviderFinalReview>();
  for (const { scope, checkpoint } of checkpoints) {
    const runDirectory = localRunDirectoryIfValid(checkpoint.request);
    let markerExists = true;
    if (runDirectory) {
      try {
        markerExists = await finalSubmitMarkerExists(runDirectory);
      } catch {
        // An unreadable marker is itself ambiguous and must fail closed.
      }
    }
    const disposition = restartDisposition(
      checkpoint.phase,
      checkpoint.expiresAtMs,
      Date.now(),
      markerExists,
    );
    if (disposition === "expired") {
      await store.remove(scope);
      continue;
    }
    if (disposition === "side_effect_unknown") {
      await store.write({
        ...checkpoint,
        phase: "side_effect_unknown",
        updatedAtMs: Date.now(),
        workflow: { ...checkpoint.workflow, status: "side_effect_unknown" },
      });
      await reportRecoveredUnknown(checkpoint).catch(() => undefined);
      continue;
    }

    let recoveryContext: BrowserContext | undefined;
    try {
      validateRestartedLocalRequest(checkpoint.request);
      if ([...activeLocalRuns.values()].some((run) => (
        run.request.applicationIdentityId === checkpoint.request.applicationIdentityId
      ))) continue;
      const context = await contextFor(
        checkpoint.request.accountId,
        checkpoint.request.applicationIdentityId,
      );
      recoveryContext = context;
      const requestedUrl = /^https?:\/\//i.test(checkpoint.browser.url)
        ? checkpoint.browser.url
        : checkpoint.request.url;
      await assertPublicApplicationUrl(requestedUrl);
      const existingPage = context.pages().find((candidate) => /^https?:\/\//i.test(candidate.url()));
      const page = existingPage ?? context.pages()[0] ?? await context.newPage();
      if (!/^https?:\/\//i.test(page.url())) {
        await page.goto(requestedUrl, { waitUntil: "domcontentloaded", timeout: 45_000 });
      }
      activeLocalRuns.set(checkpoint.request.runId, {
        request: checkpoint.request,
        delivery: checkpoint.delivery,
        page,
        runDirectory: runDirectory!,
        checkpointScope: scope,
        checkpointCreatedAtMs: checkpoint.createdAtMs,
        providerFinalReview: checkpoint.providerFinalReview,
        approvedSubmitActionConsumed: checkpoint.workflow.approvedSubmitActionConsumed,
        events: checkpoint.events,
      });
      await page.bringToFront();
      await showControllerPage(interventionPage(
        "Application recovered",
        "Bluey restored this safe pre-submit application after the browser restarted.",
        localResumeUrl(checkpoint.request.runId, checkpoint.delivery),
      ));
    } catch {
      if (recoveryContext && !activeLocalRuns.has(checkpoint.request.runId)) {
        await recoveryContext.close().catch(() => undefined);
      }
      // Keep the encrypted checkpoint for a later safe recovery. Never fall
      // back to the root claim ticket or execute the application implicitly.
    }
  }
}

function validateRestartedLocalRequest(request: StartRunRequest): void {
  assertIdentifier(request.accountId, "accountId");
  assertIdentifier(request.applicationIdentityId, "applicationIdentityId");
  assertIdentifier(request.runId, "runId");
  assertIdentifier(request.applicationId, "applicationId");
  if (request.packet.applicationId !== request.applicationId
    || (request.packet.applicationIdentityId
      && request.packet.applicationIdentityId !== request.applicationIdentityId)) {
    throw new LocalBrowserError("run_request_invalid");
  }
}

async function reportRecoveredUnknown(
  checkpoint: LocalRunCheckpoint<StartRunRequest, LocalProviderFinalReview>,
): Promise<void> {
  const response = await fetch(
    `${checkpoint.delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(checkpoint.request.runId)}/result`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ...localRunAuthorization(checkpoint.delivery, "result"),
        receipt: {
          status: "side_effect_unknown",
          errorCode: "submit_outcome_unknown",
          issues: [{
            field: "submission",
            message: "Bluey restarted across the final-submit boundary. This run is held for manual reconciliation and will not submit again automatically.",
            severity: "blocking",
          }],
        },
      }),
    },
  );
  if (!response.ok) throw new LocalBrowserError("result_delivery_failed");
}

async function showLocalFailurePage(
  runId: string,
  delivery: LocalRunDelivery,
  failure: LocalFailureClassification,
): Promise<void> {
  if (failure.status === "side_effect_unknown" && activeLocalRuns.has(runId)) {
    await showControllerPage(interventionPage(
      "Confirm the application result",
      failure.message,
      localResumeUrl(runId, delivery),
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

function localResumeUrl(runId: string, delivery: LocalRunDelivery): string {
  const { capability } = localRunAuthorization(delivery, "resume");
  return `bluey-jobs://resume/${encodeURIComponent(runId)}?capability=${encodeURIComponent(capability)}`;
}

function localRunAuthorization(
  delivery: LocalRunDelivery,
  operation: LocalRunCapabilityOperation,
): { capability: string } {
  try {
    return scopedLocalRunAuthorization(delivery.capabilities, operation);
  } catch {
    throw new LocalBrowserError("launch_expired");
  }
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
