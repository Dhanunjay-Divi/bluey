import type { BrowserContext, Page } from "playwright";
import {
  assertCertifiedProviderNavigationJob,
  assertPublicApplicationUrl,
  PlaywrightBrowserPage,
  restartDisposition,
} from "@bluey/jobs-automation";
import { finalSubmitMarkerExists } from "./irreversible-submit.js";
import {
  LocalCheckpointStore,
  type LocalRunCheckpoint,
} from "./local-checkpoint-store.js";
import { LocalBrowserError } from "./local-failure.js";
import {
  localRunReconciliationAuthorization,
  localRunDirectoryIfValid,
  validateStartRunRequest,
  type StartRunRequest,
} from "./local-run-contracts.js";
import type { ActiveLocalRun } from "./local-run-state.js";
import type { LocalProviderFinalReview } from "./provider-final-review.js";

export interface CheckpointRecoveryDependencies {
  store: LocalCheckpointStore;
  userDataDirectory: string;
  activeRuns: Map<string, ActiveLocalRun>;
  contextFor(accountId: string, identityId: string): Promise<BrowserContext>;
  onRecovered(run: ActiveLocalRun, sideEffectUnknown: boolean): void;
  onUnknown(checkpoint: LocalRunCheckpoint<StartRunRequest, LocalProviderFinalReview>): void;
}

export async function reconcileLocalRunCheckpoints(
  dependencies: CheckpointRecoveryDependencies,
): Promise<void> {
  const checkpoints = await dependencies.store.list<StartRunRequest, LocalProviderFinalReview>();
  for (const { scope, checkpoint } of checkpoints) {
    const runDirectory = localRunDirectoryIfValid(dependencies.userDataDirectory, checkpoint.request);
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
      await dependencies.store.remove(scope);
      continue;
    }
    if (disposition === "side_effect_unknown") {
      await dependencies.store.write({
        ...checkpoint,
        phase: "side_effect_unknown",
        updatedAtMs: Date.now(),
        workflow: {
          ...checkpoint.workflow,
          status: "side_effect_unknown",
          sideEffectReason: checkpoint.workflow.sideEffectReason ?? "submit_outcome_unknown",
        },
      });
      const reportedState = await reportRecoveredUnknown(checkpoint).catch(() => "unknown" as const);
      if (reportedState === "submitted") {
        await dependencies.store.remove(scope);
        continue;
      }
      dependencies.onUnknown(checkpoint);
      continue;
    }

    let recoveryContext: BrowserContext | undefined;
    try {
      validateRestartedLocalRequest(checkpoint.request);
      if ([...dependencies.activeRuns.values()].some((run) => (
        run.request.applicationIdentityId === checkpoint.request.applicationIdentityId
      ))) continue;
      const requestedUrl = /^https?:\/\//i.test(checkpoint.browser.url)
        ? checkpoint.browser.url
        : checkpoint.request.url;
      assertCertifiedProviderNavigationJob(requestedUrl, checkpoint.request.job!);
      await assertPublicApplicationUrl(requestedUrl);
      const context = await dependencies.contextFor(
        checkpoint.request.accountId,
        checkpoint.request.applicationIdentityId,
      );
      recoveryContext = context;
      const page = await prepareRecoveredLocalPage(
        context,
        checkpoint.request,
        requestedUrl,
      );
      const active: ActiveLocalRun = {
        request: checkpoint.request,
        delivery: checkpoint.delivery,
        page,
        runDirectory: runDirectory!,
        checkpointScope: scope,
        checkpointCreatedAtMs: checkpoint.createdAtMs,
        providerFinalReview: checkpoint.providerFinalReview,
        approvedSubmitActionConsumed: checkpoint.workflow.approvedSubmitActionConsumed,
        uiInterventionKind: checkpoint.workflow.status === "side_effect_unknown"
          ? "side_effect_unknown"
          : "browser_takeover",
        events: checkpoint.events,
      };
      dependencies.activeRuns.set(checkpoint.request.runId, active);
      await page.bringToFront();
      dependencies.onRecovered(active, checkpoint.workflow.status === "side_effect_unknown");
    } catch {
      if (recoveryContext && !dependencies.activeRuns.has(checkpoint.request.runId)) {
        await recoveryContext.close().catch(() => undefined);
      }
      // Keep the encrypted checkpoint for a later safe recovery. Never fall
      // back to the root claim ticket or execute the application implicitly.
    }
  }
}

type LocalPageGuardInstaller = (
  page: Page,
  request: StartRunRequest,
) => Promise<void>;

export async function prepareRecoveredLocalPage(
  context: BrowserContext,
  request: StartRunRequest,
  requestedUrl: string,
  installPageGuard: LocalPageGuardInstaller = installCertifiedRecoveredLocalPageGuard,
): Promise<Page> {
  await context.setOffline(true);
  const pages = context.pages();
  const existingHttpPages = pages.filter((candidate) => /^https?:\/\//i.test(candidate.url()));
  for (const candidate of existingHttpPages) {
    assertCertifiedProviderNavigationJob(candidate.url(), request.job!);
  }
  if (request.job!.source !== "greenhouse" && request.job!.source !== "lever"
    && existingHttpPages.some((candidate) => candidate.url() !== requestedUrl)) {
    throw new LocalBrowserError("run_request_invalid");
  }
  if (existingHttpPages.length > 1) {
    throw new LocalBrowserError("run_request_invalid");
  }
  const page = existingHttpPages[0]
    ?? pages.find((candidate) => candidate.url() === "about:blank")
    ?? await context.newPage();
  if (!/^https?:\/\//i.test(page.url()) && page.url() !== "about:blank") {
    await page.goto("about:blank", { waitUntil: "domcontentloaded", timeout: 10_000 });
  }
  await installPageGuard(page, request);
  for (const candidate of context.pages()) {
    if (candidate !== page) await candidate.close();
  }
  if (context.serviceWorkers().length > 0
    || context.pages().length !== 1
    || context.pages()[0] !== page) {
    throw new LocalBrowserError("configuration_invalid");
  }
  await context.setOffline(false);
  if (!/^https?:\/\//i.test(page.url())) {
    await page.goto(requestedUrl, { waitUntil: "domcontentloaded", timeout: 45_000 });
  }
  return page;
}

async function installCertifiedRecoveredLocalPageGuard(
  page: Page,
  request: StartRunRequest,
): Promise<void> {
  const guardedPage = new PlaywrightBrowserPage(page);
  if (request.job!.source === "greenhouse" || request.job!.source === "lever") {
    await guardedPage.installExactSubmitGuard(
      request.job!.source,
      request.job!.canonicalUrl,
    );
  }
}

function validateRestartedLocalRequest(request: StartRunRequest): void {
  try {
    validateStartRunRequest(request);
  } catch {
    throw new LocalBrowserError("run_request_invalid");
  }
}

async function reportRecoveredUnknown(
  checkpoint: LocalRunCheckpoint<StartRunRequest, LocalProviderFinalReview>,
): Promise<"submitted" | "unknown"> {
  const reason = checkpoint.workflow.sideEffectReason ?? "submit_outcome_unknown";
  const failure = new LocalBrowserError(reason);
  const response = await fetch(
    `${checkpoint.delivery.apiOrigin}/api/jobs/local-runs/${encodeURIComponent(checkpoint.request.runId)}/result`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ...localRunReconciliationAuthorization(checkpoint.delivery),
        receipt: {
          status: "side_effect_unknown",
          errorCode: reason,
          issues: [{
            field: "submission",
            message: failure.message,
            severity: "blocking",
          }],
        },
      }),
    },
  );
  if (!response.ok) throw new LocalBrowserError("result_delivery_failed");
  const contentLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(contentLength) && contentLength > 16_384) return "unknown";
  const responseBody = await response.text();
  if (Buffer.byteLength(responseBody, "utf8") > 16_384) return "unknown";
  let payload: unknown;
  try {
    payload = JSON.parse(responseBody);
  } catch {
    return "unknown";
  }
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return "unknown";
  const record = payload as Record<string, unknown>;
  return record.state === "submitted"
    && record.id === checkpoint.request.applicationId
    && record.run_id === checkpoint.request.runId
    && typeof record.submitted_at_ms === "number"
    && Number.isSafeInteger(record.submitted_at_ms)
    && record.submitted_at_ms > 0
    ? "submitted"
    : "unknown";
}
