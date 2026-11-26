// Intercepted component regression: synthetic control/storage, not a server or native-custody proof.
import { chromium, type Browser, type BrowserContext } from "playwright";
import { executeBrowserRun } from "../src/browser-run-execution.js";
import { profilePaths } from "../src/profile-store.js";
import {
  CURRENT_CHECKPOINT_VERSION,
  scanManagedRunCheckpoints,
  writeManagedRunCheckpoint,
  type CloudRunCheckpoint,
} from "../src/run-checkpoint-store.js";
import type { CloudRunRequest, RunEvent } from "../src/server.js";
import { createBrowserRunLeaseFixture } from "./browser-run-lease-fixture.js";
import {
  createBrowserRunFixture,
  createBrowserRunStorageFixture,
  greenhouseApplicationHtml,
  greenhouseConfirmationHtml,
  GREENHOUSE_JOB_URL,
} from "./browser-run-fixture.js";

export type StopClickRaceScenario =
  | "stop-during-preclick-checkpoint"
  | "stop-after-employer-post";
const preclickStages = [
  "executor_started",
  "fixture_get",
  "fixture_get_fulfilled",
  "fixture_document_loaded",
  "adapter_selected",
  "greenhouse_prepare",
  "greenhouse_fill",
  "greenhouse_validate",
  "greenhouse_submit",
  "presubmit_guard",
  "presubmit_document_readback",
  "presubmit_document_verified",
  "checkpoint_write",
  "checkpoint_scan",
] as const;
const greenhousePreclickStages = new Map<unknown, (typeof preclickStages)[number]>([
  ["prepare", "greenhouse_prepare"],
  ["fill", "greenhouse_fill"],
  ["validate", "greenhouse_validate"],
  ["submit", "greenhouse_submit"],
]);
export type StopClickRaceResult = {
  preclickStage: (typeof preclickStages)[number];
  posts: number;
  irreversibleCalls: number;
  stopChecks: number;
  checkpointCount: number;
  checkpointFailures: number;
  checkpointPhase?: string;
  checkpointStatus?: string;
  checkpointMatchesScenario: boolean;
  checkpointPersistedBeforeStop: boolean;
  stopDeliveredAtPausedCheckpoint: boolean;
  finalSubmitAttempted: boolean;
  finalSubmitAuthorized: boolean;
  finalSubmitActivationOutcome?: string;
  receiptStatus?: string;
  interventionKind?: string;
  errorName?: string;
  errorMessage?: string;
};
function errorDetail(error: unknown): Pick<StopClickRaceResult, "errorName" | "errorMessage"> {
  if (error === undefined) return {};
  if (!(error instanceof Error)) return { errorName: typeof error };
  return {
    errorName: error.name.slice(0, 120),
    errorMessage: error.message
      .replace(/https?:\/\/\S+/giu, "[url]")
      .replace(/\s+/gu, " ")
      .slice(0, 240),
  };
}

export async function executeStopClickRace(
  scenario: StopClickRaceScenario,
): Promise<StopClickRaceResult> {
  const fixture = await createBrowserRunFixture();
  let storage: Awaited<ReturnType<typeof createBrowserRunStorageFixture>> | undefined;
  let lease: Awaited<ReturnType<typeof createBrowserRunLeaseFixture>> | undefined;
  let browser: Browser | undefined;
  let context: BrowserContext | undefined;
  let primaryFailed = false;
  try {
    const input: CloudRunRequest = {
      accountId: "account-browser-execution-fixture",
      applicationId: fixture.packet.applicationId,
      applicationIdentityId: fixture.packet.applicationIdentityId!,
      browserProfileId: fixture.packet.browserProfileId!,
      browserSessionId: "session-stop-click-race",
      runId: "run-stop-click-race",
      requestId: "run-stop-click-race:initial",
      url: GREENHOUSE_JOB_URL,
      packet: fixture.packet,
      job: fixture.job,
    };
    lease = await createBrowserRunLeaseFixture(input, "success");
    const scope = profilePaths("/synthetic", input.accountId, input.applicationIdentityId).scope;
    storage = await createBrowserRunStorageFixture(lease.lease.purgeSubject, scope);
    const managed = storage.storage;
    browser = await chromium.launch({ executablePath: chromium.executablePath(), headless: true });
    context = await browser.newContext({ offline: true, serviceWorkers: "block" });
    let posts = 0;
    let stopping = false;
    let stopChecks = 0;
    let checkpointPersistedBeforeStop = false;
    let stopDeliveredAtPausedCheckpoint = false;
    let boundaryProgress = 0;
    const advance = (stage: (typeof preclickStages)[number]) => {
      boundaryProgress = Math.max(boundaryProgress, preclickStages.indexOf(stage));
    };
    context.on("page", (page) => {
      page.on("domcontentloaded", () => {
        if (page.url() === GREENHOUSE_JOB_URL) advance("fixture_document_loaded");
      });
    });
    let releaseCheckpoint: (() => void) | undefined;
    const checkpointPaused = new Promise<void>((resolve) => { releaseCheckpoint = resolve; });
    let checkpointEntered: (() => void) | undefined;
    const checkpointReached = new Promise<void>((resolve) => { checkpointEntered = resolve; });
    await context.route("**/*", async (route) => {
      const request = route.request();
      if (request.url() !== GREENHOUSE_JOB_URL) return route.abort("blockedbyclient");
      if (request.method() === "GET") {
        advance("fixture_get");
        await route.fulfill({ status: 200, contentType: "text/html", body: greenhouseApplicationHtml() });
        advance("fixture_get_fulfilled");
        return;
      }
      if (request.method() !== "POST") return route.abort("blockedbyclient");
      posts += 1;
      if (scenario === "stop-after-employer-post") {
        stopping = true;
        return route.abort("failed");
      }
      return route.fulfill({ status: 200, contentType: "text/html", body: greenhouseConfirmationHtml });
    });
    const events: RunEvent[] = [];
    const lastPreclickStage = () => {
      // Use existing closed transitions. Do not emit stored event details,
      // introduce a product log hook, or influence the race itself.
      let latest = boundaryProgress;
      for (const event of events) {
        const stage = event.type === "adapter_selected" && event.detail?.kind === "greenhouse"
          ? "adapter_selected"
          : event.type === "greenhouse_state_transition"
            ? greenhousePreclickStages.get(event.detail?.state)
            : undefined;
        if (stage) latest = Math.max(latest, preclickStages.indexOf(stage));
      }
      return preclickStages[latest];
    };
    const key = Buffer.alloc(32, 42);
    const execute = executeBrowserRun(input, context, events, lease.lease, true, {
      withProfileResidency: async (operation) => {
        const presubmit = stopChecks === 1;
        if (presubmit) advance("presubmit_document_readback");
        const result = await operation(managed);
        if (presubmit) advance("presubmit_document_verified");
        return result;
      },
      assertWorkMayContinue: () => {
        stopChecks += 1;
        if (stopChecks === 1) advance("presubmit_guard");
        if (stopping) throw new Error("synthetic tracked work stopping");
      },
      writeCheckpoint: async (part) => {
        const awaitingStop = scenario === "stop-during-preclick-checkpoint"
          && part.phase === "final_submit_started";
        if (awaitingStop) advance("checkpoint_write");
        const now = Date.now();
        const checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent> = {
          version: CURRENT_CHECKPOINT_VERSION,
          phase: part.phase,
          createdAtMs: now,
          updatedAtMs: now,
          expiresAtMs: now + 86_400_000,
          profileScope: scope,
          browserSessionId: input.browserSessionId,
          request: input,
          browser: { url: part.browserUrl },
          workflow: { status: part.status, requestId: input.requestId },
          events: [...events],
          lease: { ...lease!.lease.checkpointMetadata(), purgeSubject: lease!.lease.purgeSubject },
        };
        await writeManagedRunCheckpoint(managed, checkpoint, key);
        if (awaitingStop) {
          advance("checkpoint_scan");
          const scan = await scanManagedRunCheckpoints<CloudRunRequest, RunEvent>(managed, key);
          checkpointPersistedBeforeStop = scan.failures.length === 0
            && scan.checkpoints.length === 1
            && scan.checkpoints[0]?.checkpoint.phase === "final_submit_started"
            && scan.checkpoints[0]?.checkpoint.workflow.status === "side_effect_unknown";
          checkpointEntered!();
          await checkpointPaused;
        }
      },
      writeReceiptArtifact: async (runId, name, contents) => {
        const directory = await managed.receipts.openChildDirectory(runId);
        await directory.replaceFile(name, contents);
        return `${directory.canonicalPath}/${name}`;
      },
      takeoverOrigin: () => undefined,
    });
    const settled = execute.then(
      (value) => ({ kind: "resolved" as const, value }),
      (error: unknown) => ({ kind: "rejected" as const, error }),
    );
    if (scenario === "stop-during-preclick-checkpoint") {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const boundary = await Promise.race([
        checkpointReached.then(() => "checkpoint" as const),
        settled.then(() => "settled" as const),
        new Promise<"timeout">((resolve) => {
          timer = setTimeout(() => resolve("timeout"), 10_000);
        }),
      ]);
      if (timer) clearTimeout(timer);
      if (boundary !== "checkpoint") {
        // Raise the test stop fence before releasing a late checkpoint so a
        // failed watchdog cannot let its synthetic executor continue to click.
        stopping = true;
        releaseCheckpoint?.();
        throw new Error(`executor ${boundary} before paused checkpoint boundary: ${lastPreclickStage()}`);
      }
      stopping = true;
      stopDeliveredAtPausedCheckpoint = checkpointPersistedBeforeStop;
      releaseCheckpoint!();
    }
    const outcome = await settled;
    const scan = await scanManagedRunCheckpoints<CloudRunRequest, RunEvent>(managed, key);
    if (scan.failures.length) throw new Error("race fixture checkpoint readback failed");
    const checkpoint = scan.checkpoints[0]?.checkpoint;
    const expectedPhase = scenario === "stop-during-preclick-checkpoint"
      ? "final_submit_started" : "side_effect_unknown";
    const leaseMetadata = lease.lease.checkpointMetadata();
    const checkpointMatchesScenario = scan.checkpoints.length === 1
      && checkpoint?.phase === expectedPhase
      && checkpoint.workflow.status === "side_effect_unknown"
      && checkpoint.profileScope === scope
      && checkpoint.browserSessionId === input.browserSessionId
      && checkpoint.request.accountId === input.accountId
      && checkpoint.request.applicationId === input.applicationId
      && checkpoint.request.applicationIdentityId === input.applicationIdentityId
      && checkpoint.request.browserProfileId === input.browserProfileId
      && checkpoint.request.browserSessionId === input.browserSessionId
      && checkpoint.request.runId === input.runId
      && checkpoint.request.requestId === input.requestId
      && checkpoint.lease.ownerId === leaseMetadata.ownerId
      && checkpoint.lease.fence === leaseMetadata.fence
      && checkpoint.lease.leaseToken === leaseMetadata.leaseToken
      && checkpoint.lease.purgeSubject === lease.lease.purgeSubject;
    return {
      preclickStage: lastPreclickStage(),
      posts,
      irreversibleCalls: lease.calls.filter((call) => call.operation === "irreversible").length,
      stopChecks,
      checkpointCount: scan.checkpoints.length,
      checkpointFailures: scan.failures.length,
      checkpointPhase: checkpoint?.phase,
      checkpointStatus: checkpoint?.workflow.status,
      checkpointMatchesScenario,
      checkpointPersistedBeforeStop,
      stopDeliveredAtPausedCheckpoint,
      finalSubmitAttempted: lease.lease.finalSubmitAttempted,
      finalSubmitAuthorized: lease.lease.finalSubmitAuthorized,
      finalSubmitActivationOutcome: lease.lease.finalSubmitActivationOutcome,
      receiptStatus: outcome.kind === "resolved" ? outcome.value.receipt.status : undefined,
      interventionKind: outcome.kind === "resolved" ? outcome.value.receipt.intervention?.kind : undefined,
      ...errorDetail(outcome.kind === "rejected" ? outcome.error : undefined),
    };
  } catch (error) {
    primaryFailed = true;
    throw error;
  } finally {
    const errors: unknown[] = [];
    for (const close of [
      () => lease?.close(),
      () => context?.close(),
      () => browser?.close(),
      () => storage?.dispose(),
      () => fixture.dispose(),
    ]) {
      try {
        await close();
      } catch (error) {
        errors.push(error);
      }
    }
    if (errors.length) {
      // Fixture cleanup is still mandatory. If a primary error already failed
      // this case, don't throw over the only breadcrumb it retained.
      if (primaryFailed) console.error("stop_click_race_secondary_cleanup_failure");
      else throw new AggregateError(errors, "Stop-click race fixture cleanup failed");
    }
  }
}
