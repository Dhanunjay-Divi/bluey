import type { BrowserContext } from "playwright";
import {
  assertPublicApplicationUrl,
  restartDisposition,
} from "@bluey/jobs-automation";
import { finalSubmitMarkerExists } from "./irreversible-submit.js";
import {
  LocalCheckpointStore,
  type LocalRunCheckpoint,
} from "./local-checkpoint-store.js";
import { LocalBrowserError } from "./local-failure.js";
import {
  localRunAuthorization,
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
        workflow: { ...checkpoint.workflow, status: "side_effect_unknown" },
      });
      await reportRecoveredUnknown(checkpoint).catch(() => undefined);
      dependencies.onUnknown(checkpoint);
      continue;
    }

    let recoveryContext: BrowserContext | undefined;
    try {
      validateRestartedLocalRequest(checkpoint.request);
      if ([...dependencies.activeRuns.values()].some((run) => (
        run.request.applicationIdentityId === checkpoint.request.applicationIdentityId
      ))) continue;
      const context = await dependencies.contextFor(
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

function validateRestartedLocalRequest(request: StartRunRequest): void {
  try {
    validateStartRunRequest(request);
  } catch {
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
