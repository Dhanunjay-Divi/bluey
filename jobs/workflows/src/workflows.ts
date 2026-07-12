import { condition, defineSignal, proxyActivities, setHandler } from "@temporalio/workflow";
import type {
  ApplicationWorkflowInput,
  ApplicationWorkflowResult,
  InterventionResolution,
  JobsActivities,
  RunnerExecutionResult,
} from "./contracts.js";

export const resolveInterventionSignal = defineSignal<[InterventionResolution]>("resolveIntervention");

const activities = proxyActivities<JobsActivities>({
  startToCloseTimeout: "10 minutes",
  retry: {
    initialInterval: "2 seconds",
    backoffCoefficient: 2,
    maximumInterval: "1 minute",
    maximumAttempts: 4,
  },
});

const irreversibleActivities = proxyActivities<Pick<JobsActivities, "runApplication" | "resumeApplication">>({
  startToCloseTimeout: "10 minutes",
  retry: { maximumAttempts: 1 },
});

export async function applicationWorkflow(
  input: ApplicationWorkflowInput,
): Promise<ApplicationWorkflowResult> {
  let resolution: InterventionResolution | undefined;
  setHandler(resolveInterventionSignal, (value) => {
    resolution = value;
  });

  await activities.assertEntitlement(input);
  await activities.loadPacket(input);
  await activities.recordState(input, "running");
  const { browserSessionId } = await activities.allocateBrowser(input);
  try {
    let execution = await irreversibleActivities.runApplication({ ...input, browserSessionId });
    for (let interventionCount = 0; interventionCount < 6; interventionCount += 1) {
      // Clear the previous answer before publishing the next intervention. A
      // fast user response that arrives after createIntervention must not be
      // erased before condition observes it.
      resolution = undefined;
      const outcome = await finishOrPause(input, browserSessionId, execution);
      if (outcome) return outcome;

      const resolved = await condition(() => resolution !== undefined, "24 hours");
      if (!resolved || !resolution) {
        await activities.recordState(input, "failed");
        await activities.releaseBrowser(browserSessionId);
        return {
          state: "failed",
          receipt: {
            status: "failed",
            issues: [{ field: "intervention", message: "The application timed out while waiting for input.", severity: "blocking" }],
          },
        };
      }
      await activities.recordState(input, "running");
      execution = await irreversibleActivities.resumeApplication({
        ...input,
        browserSessionId,
        requestId: `${input.idempotencyKey}:resume:${interventionCount + 1}`,
        resolution,
      });
    }
    await activities.recordState(input, "failed");
    await activities.releaseBrowser(browserSessionId);
    return {
      state: "failed",
      receipt: {
        status: "failed",
        issues: [{ field: "intervention", message: "This application required too many manual interventions.", severity: "blocking" }],
      },
    };
  } catch (error) {
    await activities.recordState(input, "side_effect_unknown");
    return {
      state: "side_effect_unknown",
      receipt: {
        status: "failed",
        issues: [{
          field: "submission",
          message: "Bluey lost confirmation during an employer-facing step. The run is held for reconciliation and will not submit again automatically.",
          severity: "blocking",
        }],
      },
    };
  }
}

async function finishOrPause(
  input: ApplicationWorkflowInput,
  browserSessionId: string,
  execution: RunnerExecutionResult,
): Promise<ApplicationWorkflowResult | undefined> {
  const receipt = execution.receipt;
  if (receipt.status === "needs_input") {
    const interventionId = await activities.createIntervention(input, receipt);
    await activities.recordState(input, "needs_input");
    return undefined;
  }
  if (receipt.status === "submitted") {
    if (!execution.receiptBundle) throw new Error("Submitted run is missing its receipt bundle");
    if (!execution.evidenceObjects?.length) throw new Error("Submitted run is missing uploaded evidence bytes");
    await activities.persistReceipt({
      ...input,
      receiptBundle: execution.receiptBundle,
      evidenceObjects: execution.evidenceObjects,
    });
    await activities.recordState(input, "submitted");
    await activities.releaseBrowser(browserSessionId);
    return { state: "submitted", receipt };
  }
  await activities.recordState(input, "failed");
  await activities.releaseBrowser(browserSessionId);
  return { state: "failed", receipt };
}
