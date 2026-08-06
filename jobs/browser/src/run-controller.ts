import { createHash } from "node:crypto";
import { app } from "electron";
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { BrowserContext, Page } from "playwright";
import {
  PlaywrightBrowserPage,
  assertPublicApplicationUrl,
  createApprovedExecutionSnapshot,
  createDefaultAdapterRegistry,
  createApplicationReceipt,
  executeApplication,
  materializeApplicationDocuments,
  submissionPolicy,
  type EvidenceObjectUpload,
  type ExecutionResult,
  type NormalizedJob,
  type ProviderFinalSubmitProof,
} from "@bluey/jobs-automation";
import {
  finalSubmitMarkerExists,
} from "./irreversible-submit.js";
import {
  authorizedFinalSubmitHooks,
} from "./authorized-final-submit.js";
import {
  classifyLocalFailure,
  isLocalSideEffectReason,
  LocalBrowserError,
  safeLocalFailure,
  type LocalFailureClassification,
  type LocalSideEffectReason,
} from "./local-failure.js";
import { LocalCheckpointStore, type LocalRunCheckpoint } from "./local-checkpoint-store.js";
import { identityContextKey } from "./profile.js";
import {
  isApprovedLocalResumeAction,
  localProviderConfirmationDisposition,
  localProviderFinalReview,
  pendingProviderReviewReceipt,
  providerOptionsForApprovedReview,
  reconcileLocalProviderConfirmation,
  type LocalProviderFinalReview,
} from "./provider-final-review.js";
import type { BrowserShell } from "./browser-shell.js";
import { LocalRunAdmission } from "./run-admission.js";
import { BrowserContextRegistry } from "./browser-context-registry.js";
import {
  localResumeUrl,
  localRunAuthorization,
  localRunReconciliationAuthorization,
  localRunDirectory,
  localRunDirectoryIfValid,
  safeJobContext,
  validateStartRunRequest,
  type LocalRunDelivery,
  type StartRunRequest,
} from "./local-run-contracts.js";
import {
  evidenceObject,
  handoffExecution,
} from "./execution-result.js";
import {
  RunControllerView,
  type RunDisplay,
} from "./run-controller-view.js";
import {
  handleLocalProtocol,
  type ProtocolActiveRun,
} from "./local-protocol-handler.js";
import type { ActiveLocalRun } from "./local-run-state.js";
import { reconcileLocalRunCheckpoints as recoverCheckpoints } from "./checkpoint-recovery.js";
import { ExecutionSingleFlight } from "./execution-single-flight.js";
import type { BrowserBuildProof } from "./release-authority.js";

let checkpointStore: LocalCheckpointStore | undefined;
let browserContexts: BrowserContextRegistry | undefined;
const activeLocalRuns = new Map<string, ActiveLocalRun>();
let browserShell: BrowserShell | undefined;
let runView: RunControllerView | undefined;
let packagedBuildProof: BrowserBuildProof | undefined;
const admission = new LocalRunAdmission();
const executionFlights = new ExecutionSingleFlight();

export async function initializeLocalRunController(
  shell: BrowserShell,
  buildProof?: BrowserBuildProof,
): Promise<void> {
  browserShell = shell;
  packagedBuildProof = buildProof;
  runView = new RunControllerView(shell, () => [...activeLocalRuns.values()].map(displayForRun));
  browserContexts = new BrowserContextRegistry(
    app.getPath("userData"),
    app.isPackaged,
    process.resourcesPath,
  );
  checkpointStore = await LocalCheckpointStore.open(app.getPath("userData"));
  const recoveredUnknown = await reconcileLocalRunCheckpoints();
  if (activeLocalRuns.size === 0 && !recoveredUnknown) view().showReady(admission.isPaused);
}

function executeLocalRequest(
  request: StartRunRequest,
  delivery: LocalRunDelivery,
  resume = false,
): Promise<unknown> {
  validateStartRunRequest(request);
  return executionFlights.run(
    {
      runId: request.runId,
      applicationIdentityId: request.applicationIdentityId,
    },
    () => executeLocalRequestWithFailureBoundary(request, delivery, resume),
    () => new LocalBrowserError("identity_busy"),
  );
}

async function executeLocalRequestWithFailureBoundary(
  request: StartRunRequest,
  delivery: LocalRunDelivery,
  resume: boolean,
): Promise<unknown> {
  try {
    return await executeLocalRequestSingleFlight(request, delivery, resume);
  } catch (error) {
    try {
      const failure = await reportLocalFailure(request, delivery, error);
      await showLocalFailurePage(request.runId, delivery, failure);
      return { status: failure.status, errorCode: failure.code };
    } catch {
      const failure = safeLocalFailure(error);
      view().showFailure(
        view().current(),
        isLocalSideEffectReason(failure.code),
        activeLocalRuns.size > 0,
      );
      return { status: "failed", errorCode: failure.code };
    }
  }
}

async function executeLocalRequestSingleFlight(
  request: StartRunRequest,
  delivery: LocalRunDelivery,
  resume: boolean,
) {
  if (!request.job) throw new LocalBrowserError("run_request_invalid");
  const approved = createApprovedExecutionSnapshot(request.packet, request.job);
  const approvedJob = approved.approvedJob;
  request = Object.freeze({
    ...request,
    packet: approved.approvedPacket,
    job: approved.approvedJob,
  });
  const decision = submissionPolicy(request.url);
  if (decision.policy === "blocked") return { status: decision.policy, reason: decision.reason };
  const active = activeLocalRuns.get(request.runId);
  if (resume && !active) throw new LocalBrowserError("run_not_active");
  if (active?.uiInterventionKind === "side_effect_unknown") {
    await active.page.bringToFront().catch(() => undefined);
    view().showFailure(displayForRun(active), true, true);
    return { status: "side_effect_unknown" };
  }
  // Consume and durably checkpoint the one-shot approval before document,
  // page, or provider continuation work begins.
  if (resume && active?.providerFinalReview && !active.approvedSubmitActionConsumed) {
    const approved = await consumeApprovedLocalSubmitAction(request.runId, delivery);
    if (!approved) {
      const pending = pendingProviderReviewReceipt();
      pending.intervention!.takeoverUrl = localResumeUrl(request.runId, delivery);
      active.uiInterventionKind = pending.intervention?.kind;
      view().showNeedsYou(
        displayForRequest(request, pending.intervention?.kind),
        activeLocalRuns.size || 1,
      );
      return {
        status: pending.status,
        runId: request.runId,
        applicationId: request.applicationId,
        applicationIdentityId: request.applicationIdentityId,
        receipt: pending,
      };
    }
    active.approvedSubmitActionConsumed = true;
    await checkpointActiveLocalRun(request.runId, "provider_review", "provider_review");
  }
  if (!active && [...activeLocalRuns.values()].some((run) => (
    run.request.applicationIdentityId === request.applicationIdentityId
  ))) {
    throw new LocalBrowserError("identity_busy");
  }
  await assertPublicApplicationUrl(request.url);
  const runDirectory = localRunDirectory(app.getPath("userData"), request);
  await mkdir(runDirectory, { recursive: true });
  if (!resume && await finalSubmitMarkerExists(runDirectory)) {
    throw new LocalBrowserError("submit_outcome_unknown");
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
  const context = await requiredBrowserContexts().contextFor(
    request.accountId,
    request.applicationIdentityId,
  );
  const preparedPage = active
    ? await wrapActiveLocalPage(active.page, approvedJob)
    : await prepareFreshLocalPage(context, approvedJob);
  const { page, browserPage } = preparedPage;
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
  view().showRunning(
    displayForRequest(request),
    activeLocalRuns.size || 1,
    0,
    admission.isPaused,
  );
  if (!resume) await page.goto(request.url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.bringToFront();
  view().showRunning(
    displayForRequest(request),
    activeLocalRuns.size || 1,
    1,
    admission.isPaused,
  );
  await checkpointActiveLocalRun(request.runId, "prepared", "prepared");
  const durableHooks = authorizedFinalSubmitHooks(
    runDirectory,
    request,
    delivery,
    documents,
    () => browserPage.url(),
  );
  const finalSubmitHooks = {
    async beforeFinalSubmit(proof: ProviderFinalSubmitProof) {
      // The exclusive marker is durable before server click authority can be
      // granted. A crash before the encrypted checkpoint update still fails
      // closed during startup reconciliation.
      await durableHooks.beforeFinalSubmit(proof);
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
    const markerExistsBeforeContinuation = await finalSubmitMarkerExists(runDirectory);
    const reconciliation = reconcileLocalProviderConfirmation(
      active.providerFinalReview,
      approvedJob.canonicalUrl,
      await browserPage.bodyText(),
      browserPage.url(),
    );
    const disposition = localProviderConfirmationDisposition(
      reconciliation,
      markerExistsBeforeContinuation,
    );
    if (disposition !== "continue") throw new LocalBrowserError(disposition);
    approvedProviderReview = active.providerFinalReview;
  }
  if (!execution && decision.policy === "handoff") {
    execution = await handoffExecution(browserPage, decision.reason, resume);
  } else if (!execution) {
    const adapterContext = {
      runner: "local" as const,
      runId: request.runId,
      accountId: request.accountId,
      approvedCanonicalUrl: approvedJob.canonicalUrl,
      page: browserPage,
      // Runtime-only document paths must never mutate the checksum-bound
      // packet later embedded in the immutable receipt.
      packet: documents.packet,
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
    throw new LocalBrowserError(
      resume && active?.providerFinalReview
        ? "manual_submission_observed"
        : "submit_outcome_unknown",
    );
  } else if (execution.receipt.status !== "submitted" && markerExists) {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
  if (execution.receipt.status === "needs_input" && execution.receipt.intervention) {
    execution.receipt.intervention.takeoverUrl = localResumeUrl(request.runId, delivery);
  }
  if (execution.receipt.status === "needs_input") {
    const current = activeLocalRuns.get(request.runId);
    if (current) current.uiInterventionKind = execution.receipt.intervention?.kind;
    await checkpointActiveLocalRun(
      request.runId,
      providerReview ? "provider_review" : "needs_input",
      providerReview ? "provider_review" : "needs_input",
      execution.adapter,
    );
  }
  const job = approvedJob;
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
    await evidenceObject(documents.resume, "resume", "application/pdf"),
    ...(documents.coverLetter ? [await evidenceObject(
      documents.coverLetter,
      "cover_letter",
      "application/pdf",
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
      ...(markerExists
        ? localRunReconciliationAuthorization(delivery)
        : localRunAuthorization(delivery, "result")),
      receipt: execution.receipt,
      receiptBundle: bundle,
      evidenceObjects,
    }),
  });
  if (!response.ok) throw new LocalBrowserError("result_delivery_failed");
  if (execution.receipt.status === "needs_input") {
    view().showNeedsYou(
      displayForRequest(request, execution.receipt.intervention?.kind),
      activeLocalRuns.size || 1,
    );
  } else {
    activeLocalRuns.delete(request.runId);
    await page.close().catch(() => undefined);
    await requiredCheckpointStore().remove(checkpointScope);
    view().showCompleted(
      displayForRequest(request),
      execution.receipt.status === "submitted",
      admission.isPaused,
    );
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

async function wrapActiveLocalPage(
  page: Page,
  job: NormalizedJob,
): Promise<{ page: Page; browserPage: PlaywrightBrowserPage }> {
  const browserPage = new PlaywrightBrowserPage(page);
  await installCertifiedLocalSubmitGuard(browserPage, job);
  return { page, browserPage };
}

async function prepareFreshLocalPage(
  context: BrowserContext,
  job: NormalizedJob,
): Promise<{ page: Page; browserPage: PlaywrightBrowserPage }> {
  await context.setOffline(true);
  const existingPages = context.pages();
  const page = existingPages[0] ?? await context.newPage();
  await page.goto("about:blank", { waitUntil: "domcontentloaded", timeout: 10_000 });
  for (const candidate of existingPages) {
    if (candidate !== page) await candidate.close();
  }
  if (context.serviceWorkers().length > 0
    || context.pages().length !== 1
    || context.pages()[0] !== page) {
    throw new LocalBrowserError("configuration_invalid");
  }
  const browserPage = new PlaywrightBrowserPage(page);
  await installCertifiedLocalSubmitGuard(browserPage, job);
  await context.setOffline(false);
  return { page, browserPage };
}

async function installCertifiedLocalSubmitGuard(
  browserPage: PlaywrightBrowserPage,
  job: NormalizedJob,
): Promise<void> {
  if (job.source === "greenhouse" || job.source === "lever") {
    await browserPage.installExactSubmitGuard(job.source, job.canonicalUrl);
  }
}

export async function handleLocalProtocolUrl(rawUrl: string): Promise<void> {
  await handleLocalProtocol(rawUrl, {
    buildProof: packagedBuildProof,
    showController: () => shell().show(),
    isOnline: () => shell().online,
    admissionDecision: () => admission.decision(shell().online),
    showPaused: (reason, run) => {
      view().showPaused(reason, run ? displayForRequest(run.request) : undefined);
    },
    showPreparing: () => {
      view().showPreparing("Bluey is checking this reviewed application before opening a browser profile.");
    },
    showResuming: (run) => {
      view().showRunning(
        displayForRequest(run.request),
        activeLocalRuns.size,
        2,
        admission.isPaused,
        "Bluey is checking the application after your intervention.",
      );
    },
    activeRun: (runId) => protocolRun(activeLocalRuns.get(runId)),
    execute: async (request, delivery, resume) => {
      await executeLocalRequest(request, delivery, resume);
    },
    handleExecutionFailure: async (request, delivery, error) => {
      const failure = await reportLocalFailure(request, delivery, error);
      await showLocalFailurePage(request.runId, delivery, failure);
    },
    handleUnexpected: (error) => {
      const failure = safeLocalFailure(error);
      view().showFailure(
        view().current(),
        isLocalSideEffectReason(failure.code),
        activeLocalRuns.size > 0,
      );
    },
  });
}

async function reportLocalFailure(
  request: StartRunRequest,
  delivery: LocalRunDelivery,
  error: unknown,
): Promise<LocalFailureClassification> {
  const active = activeLocalRuns.get(request.runId);
  const failure = await classifyLocalFailure(
    active?.runDirectory ?? localRunDirectoryIfValid(app.getPath("userData"), request),
    error,
  );
  if (active && failure.status === "side_effect_unknown") {
    active.uiInterventionKind = "side_effect_unknown";
    await checkpointActiveLocalRun(
      request.runId,
      "side_effect_unknown",
      "side_effect_unknown",
      undefined,
      isLocalSideEffectReason(failure.code) ? failure.code : "submit_outcome_unknown",
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
        resolution: { kind: "browser_takeover", resumeAfter: false },
      }
    : undefined;
  try {
    const response = await fetch(`${delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(request.runId)}/result`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ...(failure.status === "side_effect_unknown"
          ? localRunReconciliationAuthorization(delivery)
          : localRunAuthorization(delivery, "result")),
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
  sideEffectReason?: LocalSideEffectReason,
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
    sideEffectReason,
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
  sideEffectReason?: LocalSideEffectReason;
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
      ...(input.status === "side_effect_unknown" ? {
        sideEffectReason: input.sideEffectReason ?? "submit_outcome_unknown",
      } : {}),
    },
    ...(input.providerFinalReview ? { providerFinalReview: input.providerFinalReview } : {}),
    events: activeLocalRuns.get(input.request.runId)?.events ?? [],
  };
  if (requiredCheckpointStore().scopeFor(input.request) !== input.checkpointScope) {
    throw new LocalBrowserError("run_request_invalid");
  }
  await requiredCheckpointStore().write(checkpoint);
}

async function reconcileLocalRunCheckpoints(): Promise<boolean> {
  let recoveredUnknown = false;
  await recoverCheckpoints({
    store: requiredCheckpointStore(),
    userDataDirectory: app.getPath("userData"),
    activeRuns: activeLocalRuns,
    contextFor: (accountId, identityId) => requiredBrowserContexts().contextFor(accountId, identityId),
    onRecovered: (run, sideEffectUnknown) => {
      view().showNeedsYou(displayForRun(run), activeLocalRuns.size, !sideEffectUnknown);
    },
    onUnknown: (checkpoint) => {
      recoveredUnknown = true;
      view().showFailure(displayForRequest(checkpoint.request, "side_effect_unknown"), true, false);
    },
  });
  return recoveredUnknown;
}

async function showLocalFailurePage(
  runId: string,
  _delivery: LocalRunDelivery,
  failure: LocalFailureClassification,
): Promise<void> {
  const active = activeLocalRuns.get(runId);
  view().showFailure(
    active ? displayForRun(active) : view().current(),
    failure.status === "side_effect_unknown",
    Boolean(active),
  );
}

export async function continueCurrentLocalRun(): Promise<void> {
  const active = activeForController();
  if (!active) {
    view().showReady(admission.isPaused);
    return;
  }
  if (active.uiInterventionKind === "side_effect_unknown") {
    await active.page.bringToFront().catch(() => undefined);
    view().showFailure(displayForRun(active), true, true);
    return;
  }
  if (!shell().online) {
    view().showPaused("offline", displayForRun(active));
    return;
  }
  const decision = admission.decision(shell().online);
  if (!decision.allowed) {
    view().showPaused(
      decision.reason === "paused" ? "manual" : decision.reason,
      displayForRun(active),
    );
    return;
  }
  view().showRunning(
    displayForRun(active),
    activeLocalRuns.size,
    2,
    admission.isPaused,
    "Bluey is checking the application after your intervention.",
  );
  try {
    await executeLocalRequest(active.request, active.delivery, true);
  } catch (error) {
    const failure = await reportLocalFailure(active.request, active.delivery, error);
    await showLocalFailurePage(active.request.runId, active.delivery, failure);
  }
}

export async function openCurrentApplicationBrowser(): Promise<void> {
  const active = activeForController();
  if (!active) return;
  await active.page.bringToFront().catch(() => undefined);
}

export function setLocalApplicationsPaused(paused: boolean): void {
  admission.setPaused(paused);
  const active = activeForController();
  if (paused) {
    view().showPaused("manual", active ? displayForRun(active) : undefined);
    return;
  }
  const decision = admission.decision(shell().online);
  if (!decision.allowed) {
    view().showPaused(
      decision.reason === "paused" ? "manual" : decision.reason,
      active ? displayForRun(active) : undefined,
    );
  } else if (active?.uiInterventionKind) {
    view().showNeedsYou(displayForRun(active), activeLocalRuns.size);
  } else if (active) {
    view().showRunning(displayForRun(active), activeLocalRuns.size, 2, admission.isPaused);
  } else {
    view().showReady(admission.isPaused);
  }
}

export async function requestSafeLocalStop(): Promise<void> {
  admission.requestStop();
  const active = activeForController();
  view().showPaused("stop_requested", active ? displayForRun(active) : undefined);
  if (active) return;
  await closeAllContexts();
}

export function refreshLocalControllerForConnectivity(online: boolean): void {
  const active = activeForController();
  if (!online) {
    if (!active) view().showPaused("offline");
    return;
  }
  const decision = admission.decision(online);
  if (!decision.allowed) {
    view().showPaused(
      decision.reason === "paused" ? "manual" : decision.reason,
      active ? displayForRun(active) : undefined,
    );
  } else if (!active) {
    view().showReady(false);
  }
}

export function setLocalPowerAvailable(available: boolean): void {
  admission.setDeviceAvailable(available);
  const active = activeForController();
  if (active) return;
  const decision = admission.decision(shell().online);
  if (decision.allowed) {
    view().showReady(false);
  } else {
    view().showPaused(decision.reason === "paused" ? "manual" : decision.reason);
  }
}

export async function shutdownLocalRunController(): Promise<void> {
  await closeAllContexts();
  activeLocalRuns.clear();
  runView = undefined;
  browserShell = undefined;
  packagedBuildProof = undefined;
}

export function localApplicationCount(): number {
  return activeLocalRuns.size;
}

function activeForController(): ActiveLocalRun | undefined {
  const focused = view().current();
  return (focused ? activeLocalRuns.get(focused.runId) : undefined)
    ?? activeLocalRuns.values().next().value;
}

function displayForRequest(request: StartRunRequest, interventionKind?: string): RunDisplay {
  return {
    runId: request.runId,
    context: safeJobContext(request),
    ...(interventionKind ? { interventionKind } : {}),
  };
}

function displayForRun(run: ActiveLocalRun): RunDisplay {
  return displayForRequest(run.request, run.uiInterventionKind);
}

function protocolRun(run: ActiveLocalRun | undefined): ProtocolActiveRun | undefined {
  return run ? { request: run.request, delivery: run.delivery } : undefined;
}

function shell(): BrowserShell {
  if (!browserShell) throw new LocalBrowserError("configuration_invalid");
  return browserShell;
}

function view(): RunControllerView {
  if (!runView) throw new LocalBrowserError("configuration_invalid");
  return runView;
}

async function closeAllContexts(): Promise<void> {
  await browserContexts?.closeAll();
}

function requiredBrowserContexts(): BrowserContextRegistry {
  if (!browserContexts) throw new LocalBrowserError("configuration_invalid");
  return browserContexts;
}
