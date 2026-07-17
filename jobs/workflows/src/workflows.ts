import { condition, defineSignal, proxyActivities, setHandler } from "@temporalio/workflow";
import {
  assertApprovedExecutionChecksum,
  createApprovedExecutionSnapshot,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import type {
  ApplicationWorkflowInput,
  ApplicationWorkflowResult,
  InterventionResolution,
  JobsActivities,
  RunnerExecutionResult,
} from "./contracts.js";
import { decideInterventionResolution } from "./intervention-policy.js";

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

  const approved = createApprovedExecutionSnapshot(input.packet, input.job);
  const approvedInput: ApplicationWorkflowInput = Object.freeze({
    ...input,
    packet: approved.approvedPacket,
    job: approved.approvedJob,
  });

  await activities.assertEntitlement(approvedInput);
  await activities.loadPacket(approvedInput);
  await activities.recordState(approvedInput, "running");
  const { browserSessionId } = await activities.allocateBrowser(approvedInput);
  try {
    assertApprovedExecutionChecksum(approvedInput.packet, approvedInput.job);
    let execution = await irreversibleActivities.runApplication({ ...approvedInput, browserSessionId });
    for (let interventionCount = 0; interventionCount < 6; interventionCount += 1) {
      // Clear the previous answer before publishing the next intervention. A
      // fast user response that arrives after createIntervention must not be
      // erased before condition observes it.
      resolution = undefined;
      const step = await finishOrPause(approvedInput, browserSessionId, execution);
      if (step.result) return step.result;

      const resolved = await condition(() => resolution !== undefined, "24 hours");
      if (!resolved || !resolution) {
        await activities.recordState(approvedInput, "failed");
        await activities.releaseBrowser(browserSessionId);
        return {
          state: "failed",
          receipt: {
            status: "failed",
            issues: [{ field: "intervention", message: "The application timed out while waiting for input.", severity: "blocking" }],
          },
        };
      }
      const decision = decideInterventionResolution(resolution);
      if (decision.kind === "requires_reapproval") {
        await activities.recordState(approvedInput, "needs_confirmation");
        await activities.releaseBrowser(browserSessionId);
        return {
          state: "needs_confirmation",
          receipt: requiresReapprovalReceipt(execution.receipt, decision.reason),
          interventionId: step.interventionId,
          requiresReapproval: true,
        };
      }
      try {
        assertApprovedExecutionChecksum(approvedInput.packet, approvedInput.job);
      } catch {
        await activities.recordState(approvedInput, "needs_confirmation");
        await activities.releaseBrowser(browserSessionId);
        return {
          state: "needs_confirmation",
          receipt: requiresReapprovalReceipt(
            execution.receipt,
            "The approved application packet changed and must be reviewed again.",
          ),
          interventionId: step.interventionId,
          requiresReapproval: true,
        };
      }
      await activities.recordState(approvedInput, "running");
      execution = await irreversibleActivities.resumeApplication({
        ...approvedInput,
        browserSessionId,
        requestId: `${approvedInput.idempotencyKey}:resume:${interventionCount + 1}`,
        resolution: decision.resolution,
      });
    }
    await activities.recordState(approvedInput, "failed");
    await activities.releaseBrowser(browserSessionId);
    return {
      state: "failed",
      receipt: {
        status: "failed",
        issues: [{ field: "intervention", message: "This application required too many manual interventions.", severity: "blocking" }],
      },
    };
  } catch (error) {
    await activities.recordState(approvedInput, "side_effect_unknown");
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
): Promise<{ result?: ApplicationWorkflowResult; interventionId?: string }> {
  const receipt = execution.receipt;
  if (receipt.status === "needs_input") {
    const interventionId = await activities.createIntervention(input, receipt);
    await activities.recordState(input, "needs_input");
    return { interventionId };
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
    return { result: { state: "submitted", receipt } };
  }
  await activities.recordState(input, "failed");
  await activities.releaseBrowser(browserSessionId);
  return { result: { state: "failed", receipt } };
}

function requiresReapprovalReceipt(receipt: SubmissionReceipt, reason: string): SubmissionReceipt {
  return {
    ...receipt,
    issues: [
      ...receipt.issues,
      { field: "approved_packet", message: reason, severity: "blocking" },
    ],
  };
}
